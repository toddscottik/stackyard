use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use stackyard_core::{append_log, Error, Result};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

pub struct GroupProcess {
    pub child: Child,
    pub pid: u32,
    pub group_id: u32,
    pub identity: String,
    logs: Vec<JoinHandle<()>>,
    #[cfg(windows)]
    job: std::sync::Arc<JobGuard>,
}

pub fn spawn_grouped(
    program: &Path,
    args: &[String],
    cwd: &Path,
    env: &[(String, String)],
    log_path: &Path,
) -> Result<GroupProcess> {
    spawn_grouped_scoped(program, args, cwd, env, &[], log_path)
}

pub fn spawn_grouped_scoped(
    program: &Path,
    args: &[String],
    cwd: &Path,
    env: &[(String, String)],
    env_remove: &[String],
    log_path: &Path,
) -> Result<GroupProcess> {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in env_remove {
        command.env_remove(key);
    }
    for (key, value) in env {
        command.env(key, value);
    }
    #[cfg(unix)]
    {
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP.0);
    }
    let mut child = command
        .spawn()
        .map_err(|source| Error::io("start project process", source))?;
    let pid = child.id();
    let mut logs = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        logs.push(pump(stdout, log_path.to_path_buf()));
    }
    if let Some(stderr) = child.stderr.take() {
        logs.push(pump(stderr, log_path.to_path_buf()));
    }
    let identity = process_identity(pid).unwrap_or_else(|| "unknown".to_string());
    #[cfg(windows)]
    let job = match JobGuard::attach(&child) {
        Ok(job) => std::sync::Arc::new(job),
        Err(err) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(err);
        }
    };
    Ok(GroupProcess {
        child,
        pid,
        group_id: pid,
        identity,
        logs,
        #[cfg(windows)]
        job,
    })
}

impl GroupProcess {
    pub fn finish_io(&mut self) {
        for handle in self.logs.drain(..) {
            let _ = handle.join();
        }
    }

    /// Wait for stdout/stderr pumps without allowing inherited pipe handles in
    /// descendants to hold the caller forever after the direct child exits.
    pub fn finish_io_timeout(&mut self, limit: Duration) {
        let started = Instant::now();
        while self.logs.iter().any(|handle| !handle.is_finished()) {
            if started.elapsed() >= limit {
                #[cfg(unix)]
                terminate_tree(self.group_id, true);
                #[cfg(windows)]
                self.job.terminate();
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        if self.logs.iter().all(JoinHandle::is_finished) {
            self.finish_io();
        } else {
            self.logs.clear();
        }
    }

    pub fn request_stop(&mut self) {
        if ours(self.pid, &self.identity) {
            terminate_tree(self.group_id, false);
            #[cfg(windows)]
            self.job.terminate();
        }
    }

    pub fn force_stop(&mut self) {
        if ours(self.pid, &self.identity) {
            terminate_tree(self.group_id, true);
            #[cfg(windows)]
            self.job.terminate();
        }
        let _ = self.child.kill();
    }

    pub fn wait_timeout(&mut self, limit: Duration) -> Result<ExitStatus> {
        let started = Instant::now();
        loop {
            if let Some(status) = self
                .child
                .try_wait()
                .map_err(|source| Error::io("wait for project process", source))?
            {
                self.finish_io();
                return Ok(status);
            }
            if started.elapsed() >= limit {
                self.force_stop();
                let status = self
                    .child
                    .wait()
                    .map_err(|source| Error::io("wait for project process", source))?;
                self.finish_io();
                return Ok(status);
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    #[cfg(windows)]
    pub fn job(&self) -> std::sync::Arc<JobGuard> {
        std::sync::Arc::clone(&self.job)
    }
}

pub fn same_process(recorded: &str, current: Option<&str>) -> bool {
    matches!(
        current,
        Some(value) if !recorded.is_empty() && recorded != "unknown" && value == recorded
    )
}

pub fn stop_recorded(pid: u32, group_id: u32, identity: &str) {
    if !ours(pid, identity) {
        return;
    }
    terminate_tree(group_id, false);
    terminate_pid(pid);
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(2) {
        if !ours(pid, identity) {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    if ours(pid, identity) {
        terminate_tree(group_id, true);
        terminate_pid(pid);
    }
}

fn ours(pid: u32, identity: &str) -> bool {
    same_process(identity, process_identity(pid).as_deref())
}

fn pump(mut reader: impl Read + Send + 'static, path: std::path::PathBuf) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut buf = [0_u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    let _ = append_log(&path, &buf[..read]);
                }
            }
        }
    })
}

#[cfg(target_os = "linux")]
pub fn process_identity(pid: u32) -> Option<String> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = text.rsplit_once(')')?.1;
    let start = rest.split_whitespace().nth(19)?;
    Some(format!("linux:{start}"))
}

#[cfg(target_os = "macos")]
pub fn process_identity(pid: u32) -> Option<String> {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "lstart="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(format!("macos:{text}"))
    }
}

#[cfg(windows)]
pub fn process_identity(pid: u32) -> Option<String> {
    use windows::Win32::Foundation::{CloseHandle, FILETIME};
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        let times = GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user);
        let _ = CloseHandle(handle);
        times.ok()?;
        let ticks = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
        Some(format!("windows:{ticks}"))
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub fn process_identity(pid: u32) -> Option<String> {
    let _ = pid;
    None
}

#[cfg(unix)]
fn terminate_tree(group_id: u32, force: bool) {
    if group_id <= 1 {
        return;
    }
    let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
    unsafe {
        if libc::kill(-(group_id as i32), 0) != 0
            && std::io::Error::last_os_error().raw_os_error() != Some(libc::EPERM)
        {
            return;
        }
        libc::kill(-(group_id as i32), signal);
    }
}

#[cfg(not(unix))]
fn terminate_tree(group_id: u32, force: bool) {
    let _ = (group_id, force);
}

#[cfg(windows)]
fn terminate_pid(pid: u32) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    unsafe {
        if let Ok(handle) = OpenProcess(PROCESS_TERMINATE, false, pid) {
            let _ = TerminateProcess(handle, 1);
            let _ = CloseHandle(handle);
        }
    }
}

#[cfg(not(windows))]
fn terminate_pid(pid: u32) {
    let _ = pid;
}

#[cfg(windows)]
pub struct JobGuard(windows::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl JobGuard {
    fn attach(child: &Child) -> Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        unsafe {
            let job = CreateJobObjectW(None, windows::core::PCWSTR::null()).map_err(|err| {
                Error::io("create process job", std::io::Error::other(err.to_string()))
            })?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of_val(&info) as u32,
            )
            .map_err(|err| {
                Error::io(
                    "configure process job",
                    std::io::Error::other(err.to_string()),
                )
            })?;
            AssignProcessToJobObject(job, HANDLE(child.as_raw_handle())).map_err(|err| {
                Error::io("assign process job", std::io::Error::other(err.to_string()))
            })?;
            Ok(Self(job))
        }
    }

    pub fn terminate(&self) {
        unsafe {
            let _ = windows::Win32::System::JobObjects::TerminateJobObject(self.0, 1);
        }
    }
}

#[cfg(windows)]
impl Drop for JobGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
unsafe impl Send for JobGuard {}

#[cfg(windows)]
unsafe impl Sync for JobGuard {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn child_exit_with_descendant_holding_logs_is_bounded() {
        let root = stackyard_core::disposable_dir("group-log-descendant");
        std::fs::create_dir_all(&root).unwrap();
        let log = root.join("child.log");
        let mut process = spawn_grouped(
            Path::new("sh"),
            &["-c".into(), "sleep 30 & exit 0".into()],
            &root,
            &[],
            &log,
        )
        .unwrap();
        let started = Instant::now();
        while process.child.try_wait().unwrap().is_none() {
            thread::sleep(Duration::from_millis(10));
        }
        process.finish_io_timeout(Duration::from_millis(100));
        assert!(started.elapsed() < Duration::from_secs(3));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    #[cfg(windows)]
    fn windows_command_bounds_a_stalled_pump_and_stops_job_children() {
        use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0, WAIT_TIMEOUT};
        use windows::Win32::System::Threading::{
            OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
        };

        let Some(node) = std::env::var_os("STACKYARD_TEST_NODE") else {
            eprintln!("Set STACKYARD_TEST_NODE to run the Windows inherited-pipe check.");
            return;
        };
        let root = stackyard_core::disposable_dir("group-win-descendant");
        std::fs::create_dir_all(&root).unwrap();
        let log = root.join("child.log");
        let mut process = spawn_grouped(
            Path::new(&node),
            &[
                "-e".into(),
                r#"
                    const { spawn } = require('node:child_process');
                    const fs = require('node:fs');
                    const waiting = setInterval(() => {
                        if (!fs.existsSync('job-ready')) return;
                        clearInterval(waiting);
                        const child = spawn(process.execPath, ['-e',
                            'process.stdout.write("ready"); setTimeout(() => {}, 60000)'],
                            { stdio: ['ignore', 'pipe', 'inherit'], windowsHide: true });
                        child.stdout.once('data', () => {
                            process.stdout.write(`${child.pid}\n`, () => process.exit(0));
                        });
                    }, 10);
                "#
                .into(),
            ],
            &root,
            &[],
            &log,
        )
        .unwrap();
        std::fs::write(root.join("job-ready"), []).unwrap();
        let started = Instant::now();
        let status = loop {
            if let Some(status) = process.child.try_wait().unwrap() {
                break status;
            }
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "Node fixture did not exit"
            );
            thread::sleep(Duration::from_millis(10));
        };
        assert!(status.success());
        let pid: u32 = loop {
            if let Ok(text) = std::fs::read_to_string(&log) {
                if let Ok(pid) = text.trim().parse() {
                    break pid;
                }
            }
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "No child PID was logged"
            );
            thread::sleep(Duration::from_millis(10));
        };
        let child = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }.unwrap();
        let before = unsafe { WaitForSingleObject(child, 0) };
        // Make the stalled reader deterministic regardless of Windows handle inheritance.
        let (release, blocked) = std::sync::mpsc::channel::<()>();
        process.logs.push(thread::spawn(move || {
            let _ = blocked.recv_timeout(Duration::from_secs(10));
        }));
        process.finish_io_timeout(Duration::from_millis(100));
        let _ = release.send(());
        assert!(started.elapsed() < Duration::from_secs(7));
        let after = unsafe { WaitForSingleObject(child, 2_000) };
        unsafe { CloseHandle(child) }.unwrap();
        assert_eq!(before, WAIT_TIMEOUT, "JobObject child was already stopped");
        assert_eq!(after, WAIT_OBJECT_0, "JobObject child was not stopped");
        drop(process);
        let _ = std::fs::remove_dir_all(root);
    }
}

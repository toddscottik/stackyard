use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use stackyard_core::{
    append_log, load_project_manifest, log_tail, project_view, require_startable, service_launch,
    service_log_path, start_order, Error, ProcessRecord, ProjectRecord, ProjectView, RestartPolicy,
    Result, ServiceSpec, Store,
};

use crate::group::{self, GroupProcess};

pub struct Supervisor {
    data_dir: PathBuf,
    shutdown: AtomicBool,
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
    stops: Mutex<HashMap<String, Arc<AtomicBool>>>,
    live: Mutex<HashMap<String, Live>>,
}

struct Live {
    pid: u32,
    group_id: u32,
    identity: String,
    #[cfg(windows)]
    job: Arc<group::JobGuard>,
}

pub fn new(data_dir: PathBuf) -> Arc<Supervisor> {
    Arc::new(Supervisor {
        data_dir,
        shutdown: AtomicBool::new(false),
        cancels: Mutex::new(HashMap::new()),
        stops: Mutex::new(HashMap::new()),
        live: Mutex::new(HashMap::new()),
    })
}

impl Supervisor {
    pub fn reconcile(&self) -> Result<()> {
        let store = Store::open(&self.data_dir)?;
        for process in store.active_processes()? {
            let current = process
                .pid
                .and_then(|pid| u32::try_from(pid).ok())
                .and_then(group::process_identity);
            let same = process
                .identity
                .as_deref()
                .is_some_and(|recorded| group::same_process(recorded, current.as_deref()));
            if same {
                self.write(process_row(&process, "orphaned", process.detail.clone()))?;
            } else {
                let mut stopped = process;
                stopped.state = "stopped".to_string();
                stopped.pid = None;
                stopped.detail = "process was not running after the engine restarted".to_string();
                self.write(stopped)?;
            }
        }
        Ok(())
    }

    pub fn start(self: &Arc<Self>, project_id: &str) -> Result<ProjectView> {
        if self.shutdown.load(Ordering::Relaxed) {
            return Err(Error::OperationFailed("the engine is stopping".to_string()));
        }
        let (project, manifest) = load_project_manifest(&self.data_dir, project_id)?;
        let view = project_view(&self.data_dir, project_id)?;
        require_startable(&view)?;
        let order = start_order(&manifest.services)?;
        for id in &order {
            let service = manifest.service(id).expect("service from order");
            let root = PathBuf::from(&project.root);
            let _ = stackyard_core::contained_dir(&root, &service.working_directory)?;
            let _ = stackyard_core::resolve_runtime(&self.data_dir, project_id, service.runtime)?;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancels
            .lock()
            .expect("cancels")
            .insert(project_id.to_string(), Arc::clone(&cancel));
        for id in &order {
            self.mark(
                &project.id,
                id,
                "starting",
                None,
                None,
                None,
                None,
                None,
                "",
            )?;
        }
        let supervisor = Arc::clone(self);
        let id = project_id.to_string();
        tokio::task::spawn_blocking(move || supervisor.run_start(id, cancel));
        project_view(&self.data_dir, project_id)
    }

    pub fn cancel(&self, project_id: &str) -> Result<()> {
        let flag = self
            .cancels
            .lock()
            .expect("cancels")
            .get(project_id)
            .cloned();
        let Some(flag) = flag else {
            return Err(Error::InvalidMessage(format!(
                "project {project_id} is not starting"
            )));
        };
        flag.store(true, Ordering::Relaxed);
        self.signal_project(project_id);
        Ok(())
    }

    pub fn stop(&self, project_id: &str) -> Result<ProjectView> {
        self.request_halt(project_id);
        self.signal_project(project_id);
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(3) {
            let busy = self
                .live
                .lock()
                .expect("live")
                .keys()
                .any(|key| key.starts_with(&format!("{project_id}\0")));
            if !busy {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        let store = Store::open(&self.data_dir)?;
        for service in store.list_processes(project_id)? {
            if matches!(service.state.as_str(), "starting" | "running" | "orphaned") {
                self.mark(
                    project_id,
                    &service.service_id,
                    "stopped",
                    None,
                    None,
                    None,
                    service.port,
                    service.exit_code,
                    "stopped",
                )?;
            }
        }
        project_view(&self.data_dir, project_id)
    }

    pub fn restart(self: &Arc<Self>, project_id: &str) -> Result<ProjectView> {
        self.stop(project_id)?;
        self.start(project_id)
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let project_ids: Vec<String> = self
            .cancels
            .lock()
            .expect("cancels")
            .keys()
            .cloned()
            .collect();
        for id in &project_ids {
            if let Some(flag) = self.cancels.lock().expect("cancels").get(id) {
                flag.store(true, Ordering::Relaxed);
            }
        }
        let store = match Store::open(&self.data_dir) {
            Ok(store) => store,
            Err(_) => return,
        };
        let Ok(projects) = store.list_projects() else {
            return;
        };
        for project in projects {
            let _ = self.stop(&project.id);
        }
    }

    fn run_start(self: &Arc<Self>, project_id: String, cancel: Arc<AtomicBool>) {
        if let Err(err) = self.start_ordered(&project_id, &cancel) {
            let detail = err.to_string();
            if let Ok((project, manifest)) = load_project_manifest(&self.data_dir, &project_id) {
                for service in &manifest.services {
                    if let Ok(store) = Store::open(&self.data_dir) {
                        if let Ok(rows) = store.list_processes(&project.id) {
                            if let Some(row) = rows.iter().find(|row| row.service_id == service.id)
                            {
                                if row.state == "starting" {
                                    let _ = self.mark(
                                        &project.id,
                                        &service.id,
                                        "failed",
                                        None,
                                        None,
                                        None,
                                        None,
                                        None,
                                        &detail,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        self.cancels.lock().expect("cancels").remove(&project_id);
    }

    fn start_ordered(self: &Arc<Self>, project_id: &str, cancel: &AtomicBool) -> Result<()> {
        let (project, manifest) = load_project_manifest(&self.data_dir, project_id)?;
        let order = start_order(&manifest.services)?;
        let mut started = Vec::new();
        for id in order {
            if self.halted(cancel) {
                self.rollback(&project, &started, "cancelled")?;
                return Ok(());
            }
            let service = manifest.service(&id).expect("service from order").clone();
            match self.bring_up(&project, &service, cancel) {
                Ok(true) => started.push(service.id),
                Ok(false) => {
                    self.rollback(&project, &started, "cancelled")?;
                    return Ok(());
                }
                Err(err) => {
                    self.rollback(&project, &started, "stopped because another service failed")?;
                    return Err(err);
                }
            }
        }
        Ok(())
    }

    fn bring_up(
        self: &Arc<Self>,
        project: &ProjectRecord,
        service: &ServiceSpec,
        cancel: &AtomicBool,
    ) -> Result<bool> {
        let port = reserve_port(service.port)?;
        let log_path = service_log_path(&self.data_dir, &project.id, &service.id);
        let stop = Arc::new(AtomicBool::new(false));
        let key = live_key(&project.id, &service.id);
        self.stops
            .lock()
            .expect("stops")
            .insert(key.clone(), Arc::clone(&stop));
        let mut restarts_left = service.restart_max;
        loop {
            if self.halted(cancel) || stop.load(Ordering::Relaxed) {
                self.mark_state(
                    project,
                    service,
                    "stopped",
                    None,
                    None,
                    None,
                    None,
                    None,
                    "cancelled",
                )?;
                return Ok(false);
            }
            let launch = service_launch(&self.data_dir, project, service, port, &service.command)?;
            let _ = append_log(
                &log_path,
                format!(
                    "stackyard: starting {} {}\n",
                    launch.program.display(),
                    launch.args.join(" ")
                )
                .as_bytes(),
            );
            let mut group = spawn_grouped(
                &launch.program,
                &launch.args,
                &launch.working_directory,
                &launch.env,
                &log_path,
            )?;
            self.remember(&key, &group);
            match wait_ready(
                &mut group,
                service,
                port,
                &log_path,
                cancel,
                &self.shutdown,
                &stop,
            ) {
                Wait::Ready => {
                    self.mark_state(
                        project,
                        service,
                        "running",
                        Some(i64::from(group.pid)),
                        Some(i64::from(group.group_id)),
                        Some(group.identity.clone()),
                        Some(i64::from(port)),
                        None,
                        "",
                    )?;
                    if self.halted(cancel) || stop.load(Ordering::Relaxed) {
                        group.request_stop();
                        let _ = group.wait_timeout(Duration::from_secs(2));
                        self.forget(&key);
                        self.mark_state(
                            project,
                            service,
                            "stopped",
                            None,
                            None,
                            None,
                            Some(i64::from(port)),
                            None,
                            "cancelled",
                        )?;
                        return Ok(false);
                    }
                    self.watch(
                        group,
                        project.clone(),
                        service.clone(),
                        port,
                        restarts_left,
                        stop,
                        log_path,
                    );
                    return Ok(true);
                }
                Wait::Cancelled => {
                    group.request_stop();
                    let _ = group.wait_timeout(Duration::from_secs(2));
                    self.forget(&key);
                    self.mark_state(
                        project,
                        service,
                        "stopped",
                        None,
                        None,
                        None,
                        Some(i64::from(port)),
                        None,
                        "cancelled",
                    )?;
                    return Ok(false);
                }
                Wait::Exited { detail, reaped } => {
                    if !reaped {
                        group.request_stop();
                        let _ = group.wait_timeout(Duration::from_secs(2));
                    }
                    self.forget(&key);
                    let can_restart = service.restart == RestartPolicy::OnFailure
                        && restarts_left > 0
                        && !self.halted(cancel)
                        && !stop.load(Ordering::Relaxed);
                    if can_restart {
                        restarts_left -= 1;
                        thread::sleep(Duration::from_millis(200));
                        continue;
                    }
                    self.mark_state(
                        project,
                        service,
                        "failed",
                        None,
                        None,
                        None,
                        Some(i64::from(port)),
                        None,
                        &detail,
                    )?;
                    return Err(Error::ServiceFailed {
                        project: project.id.clone(),
                        service: service.id.clone(),
                        detail,
                    });
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn watch(
        self: &Arc<Self>,
        group: GroupProcess,
        project: ProjectRecord,
        service: ServiceSpec,
        port: u16,
        mut restarts_left: u32,
        stop: Arc<AtomicBool>,
        log_path: PathBuf,
    ) {
        let supervisor = Arc::clone(self);
        let key = live_key(&project.id, &service.id);
        thread::spawn(move || {
            let mut group = group;
            loop {
                let status = group.child.wait();
                group.finish_io();
                supervisor.forget(&key);
                if stop.load(Ordering::Relaxed) || supervisor.shutdown.load(Ordering::Relaxed) {
                    let _ = supervisor.mark_state(
                        &project,
                        &service,
                        "stopped",
                        None,
                        None,
                        None,
                        Some(i64::from(port)),
                        None,
                        "stopped",
                    );
                    return;
                }
                let detail = match status {
                    Ok(status) => format!("exited {status}\n{}", log_tail(&log_path, 800)),
                    Err(err) => err.to_string(),
                };
                if service.restart == RestartPolicy::OnFailure
                    && restarts_left > 0
                    && !stop.load(Ordering::Relaxed)
                    && !supervisor.shutdown.load(Ordering::Relaxed)
                {
                    restarts_left -= 1;
                    thread::sleep(Duration::from_millis(200));
                    if stop.load(Ordering::Relaxed) || supervisor.shutdown.load(Ordering::Relaxed) {
                        let _ = supervisor.mark_state(
                            &project,
                            &service,
                            "stopped",
                            None,
                            None,
                            None,
                            Some(i64::from(port)),
                            None,
                            "stopped",
                        );
                        return;
                    }
                    let Ok(launch) = service_launch(
                        &supervisor.data_dir,
                        &project,
                        &service,
                        port,
                        &service.command,
                    ) else {
                        let _ = supervisor.mark_state(
                            &project,
                            &service,
                            "failed",
                            None,
                            None,
                            None,
                            Some(i64::from(port)),
                            None,
                            &detail,
                        );
                        return;
                    };
                    let Ok(next) = spawn_grouped(
                        &launch.program,
                        &launch.args,
                        &launch.working_directory,
                        &launch.env,
                        &log_path,
                    ) else {
                        let _ = supervisor.mark_state(
                            &project,
                            &service,
                            "failed",
                            None,
                            None,
                            None,
                            Some(i64::from(port)),
                            None,
                            &detail,
                        );
                        return;
                    };
                    supervisor.remember(&key, &next);
                    let _ = supervisor.mark_state(
                        &project,
                        &service,
                        "running",
                        Some(i64::from(next.pid)),
                        Some(i64::from(next.group_id)),
                        Some(next.identity.clone()),
                        Some(i64::from(port)),
                        None,
                        "",
                    );
                    group = next;
                    continue;
                }
                let _ = supervisor.mark_state(
                    &project,
                    &service,
                    "failed",
                    None,
                    None,
                    None,
                    Some(i64::from(port)),
                    None,
                    &detail,
                );
                return;
            }
        });
    }

    fn rollback(&self, project: &ProjectRecord, started: &[String], detail: &str) -> Result<()> {
        let store = Store::open(&self.data_dir)?;
        for id in started.iter().rev() {
            let key = live_key(&project.id, id);
            if let Some(flag) = self.stops.lock().expect("stops").get(&key) {
                flag.store(true, Ordering::Relaxed);
            }
            if let Some(row) = store
                .list_processes(&project.id)?
                .into_iter()
                .find(|row| row.service_id == *id)
            {
                self.stop_row(&row);
            }
            self.mark(
                &project.id,
                id,
                "stopped",
                None,
                None,
                None,
                None,
                None,
                detail,
            )?;
        }
        Ok(())
    }

    fn stop_row(&self, process: &ProcessRecord) {
        let Some(pid) = process.pid.and_then(|pid| u32::try_from(pid).ok()) else {
            return;
        };
        let group_id = process
            .process_group
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(pid);
        let identity = process.identity.clone().unwrap_or_default();
        let key = live_key(&process.project_id, &process.service_id);
        if let Some(flag) = self.stops.lock().expect("stops").get(&key) {
            flag.store(true, Ordering::Relaxed);
        }
        #[cfg(windows)]
        if let Some(live) = self.live.lock().expect("live").get(&key) {
            live.job.terminate();
        }
        group::stop_recorded(pid, group_id, &identity);
    }

    fn request_halt(&self, project_id: &str) {
        if let Some(flag) = self.cancels.lock().expect("cancels").get(project_id) {
            flag.store(true, Ordering::Relaxed);
        }
        let prefix = format!("{project_id}\0");
        for (key, flag) in self.stops.lock().expect("stops").iter() {
            if key.starts_with(&prefix) {
                flag.store(true, Ordering::Relaxed);
            }
        }
    }

    fn signal_project(&self, project_id: &str) {
        let prefix = format!("{project_id}\0");
        let lives: Vec<(u32, u32, String)> = self
            .live
            .lock()
            .expect("live")
            .iter()
            .filter(|(key, _)| key.starts_with(&prefix))
            .map(|(_, live)| (live.pid, live.group_id, live.identity.clone()))
            .collect();
        #[cfg(windows)]
        {
            for (key, live) in self.live.lock().expect("live").iter() {
                if key.starts_with(&prefix) {
                    live.job.terminate();
                }
            }
        }
        for (pid, group_id, identity) in lives {
            group::stop_recorded(pid, group_id, &identity);
        }
        let store = match Store::open(&self.data_dir) {
            Ok(store) => store,
            Err(_) => return,
        };
        let Ok(rows) = store.list_processes(project_id) else {
            return;
        };
        for row in rows {
            if matches!(row.state.as_str(), "starting" | "running" | "orphaned") {
                self.stop_row(&row);
            }
        }
    }

    fn remember(&self, key: &str, group: &GroupProcess) {
        self.live.lock().expect("live").insert(
            key.to_string(),
            Live {
                pid: group.pid,
                group_id: group.group_id,
                identity: group.identity.clone(),
                #[cfg(windows)]
                job: group.job(),
            },
        );
    }

    fn forget(&self, key: &str) {
        self.live.lock().expect("live").remove(key);
    }

    fn halted(&self, cancel: &AtomicBool) -> bool {
        cancel.load(Ordering::Relaxed) || self.shutdown.load(Ordering::Relaxed)
    }

    #[allow(clippy::too_many_arguments)]
    fn mark_state(
        &self,
        project: &ProjectRecord,
        service: &ServiceSpec,
        state: &str,
        pid: Option<i64>,
        group_id: Option<i64>,
        identity: Option<String>,
        port: Option<i64>,
        exit_code: Option<i64>,
        detail: &str,
    ) -> Result<()> {
        self.mark(
            &project.id,
            &service.id,
            state,
            pid,
            group_id,
            identity,
            port,
            exit_code,
            detail,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn mark(
        &self,
        project_id: &str,
        service_id: &str,
        state: &str,
        pid: Option<i64>,
        process_group: Option<i64>,
        identity: Option<String>,
        port: Option<i64>,
        exit_code: Option<i64>,
        detail: &str,
    ) -> Result<()> {
        self.write(ProcessRecord {
            project_id: project_id.to_string(),
            service_id: service_id.to_string(),
            state: state.to_string(),
            pid,
            process_group,
            started_at: Some(now_unix()),
            identity,
            port,
            exit_code,
            detail: clip(detail),
        })
    }

    fn write(&self, process: ProcessRecord) -> Result<()> {
        Store::open(&self.data_dir)?.upsert_process(&process)
    }
}

enum Wait {
    Ready,
    Cancelled,
    Exited { detail: String, reaped: bool },
}

fn wait_ready(
    group: &mut GroupProcess,
    service: &ServiceSpec,
    port: u16,
    log_path: &Path,
    cancel: &AtomicBool,
    shutdown: &AtomicBool,
    stop: &AtomicBool,
) -> Wait {
    let deadline = Instant::now() + Duration::from_secs(service.ready_timeout_secs);
    loop {
        if cancel.load(Ordering::Relaxed)
            || shutdown.load(Ordering::Relaxed)
            || stop.load(Ordering::Relaxed)
        {
            return Wait::Cancelled;
        }
        match group.child.try_wait() {
            Ok(Some(status)) => {
                group.finish_io();
                return Wait::Exited {
                    detail: format!(
                        "exited {status} before it was ready\n{}",
                        log_tail(log_path, 800)
                    ),
                    reaped: true,
                };
            }
            Ok(None) => {}
            Err(err) => {
                return Wait::Exited {
                    detail: err.to_string(),
                    reaped: false,
                };
            }
        }
        if http_ready(port, &service.ready_path) {
            return Wait::Ready;
        }
        if Instant::now() >= deadline {
            return Wait::Exited {
                detail: format!(
                    "not ready at http://127.0.0.1:{port}{} within {}s\n{}",
                    service.ready_path,
                    service.ready_timeout_secs,
                    log_tail(log_path, 800)
                ),
                reaped: false,
            };
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn reserve_port(requested: u16) -> Result<u16> {
    let bind_port = if requested == 0 { 0 } else { requested };
    let listener =
        TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], bind_port))).map_err(|source| {
            if requested != 0 {
                Error::PortInUse { port: requested }
            } else {
                Error::io("choose a loopback port", source)
            }
        })?;
    let port = listener
        .local_addr()
        .map_err(|source| Error::io("read loopback port", source))?
        .port();
    drop(listener);
    Ok(port)
}

fn http_ready(port: u16, path: &str) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(200)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
    let request = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut buf = [0_u8; 128];
    let Ok(read) = stream.read(&mut buf) else {
        return false;
    };
    let text = String::from_utf8_lossy(&buf[..read]);
    text.split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .is_some_and(|code| (200..300).contains(&code))
}

fn spawn_grouped(
    program: &Path,
    args: &[String],
    cwd: &Path,
    env: &[(String, String)],
    log_path: &Path,
) -> Result<GroupProcess> {
    group::spawn_grouped(program, args, cwd, env, log_path)
}

fn live_key(project: &str, service: &str) -> String {
    format!("{project}\0{service}")
}

fn process_row(process: &ProcessRecord, state: &str, detail: String) -> ProcessRecord {
    let mut next = process.clone();
    next.state = state.to_string();
    next.detail = clip(&detail);
    next
}

fn clip(text: &str) -> String {
    const MAX: usize = 1_800;
    if text.chars().count() <= MAX {
        text.to_string()
    } else {
        text.chars().take(MAX).collect()
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_secs()).unwrap_or(0))
        .unwrap_or(0)
}

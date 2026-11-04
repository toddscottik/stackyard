//! Exercise client autostart from a captured subprocess, not an embedded engine.
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use stackyard_core::{roundtrip, Command as EngineCommand, Outcome};

fn request(data_dir: &Path, command: EngineCommand) -> stackyard_core::Result<Outcome> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("client runtime")
        .block_on(roundtrip(data_dir, command))
}

struct EngineCleanup(PathBuf);

impl Drop for EngineCleanup {
    fn drop(&mut self) {
        let _ = request(&self.0, EngineCommand::Stop);
        let _ = fs::remove_dir_all(&self.0);
    }
}

// This helper runs only in the subprocess selected by the parent regression.
#[test]
fn captured_client_helper() {
    let Some(data_dir) = std::env::var_os("STACKYARD_CAPTURED_CLIENT_DATA") else {
        return;
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("helper runtime");
    let data_dir = PathBuf::from(data_dir);
    runtime.block_on(async {
        let data_dir = stackyard_core::ensure_engine(&data_dir)
            .await
            .expect("autostart engine");
        let status = roundtrip(&data_dir, EngineCommand::Status)
            .await
            .expect("autostarted status");
        let Outcome::Status { pid, .. } = status else {
            panic!("expected status");
        };
        println!("captured-client-engine-pid={pid}");
    });
}

#[test]
fn captured_client_output_closes_while_autostarted_engine_stays_alive() {
    let data_dir =
        stackyard_core::prepare_data_dir(&stackyard_core::disposable_dir("cli startup пример"))
            .expect("disposable data dir");
    let _cleanup = EngineCleanup(data_dir.clone());
    let engine = data_dir.join("engine binary пример");
    fs::create_dir(&engine).expect("engine directory with spaces and Unicode");
    let engine = engine.join(if cfg!(windows) {
        "stackyard-engine.exe"
    } else {
        "stackyard-engine"
    });
    fs::copy(env!("CARGO_BIN_EXE_stackyard-engine"), &engine).expect("copy disposable engine");
    #[cfg(windows)]
    let (extra_reader, extra_writer) = inheritable_pipe();
    let mut child = Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", "captured_client_helper", "--nocapture"])
        .env("STACKYARD_CAPTURED_CLIENT_DATA", &data_dir)
        .env("LOCALDEV_ENGINE", &engine)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("captured client subprocess");
    #[cfg(windows)]
    drop(extra_writer);
    let (send, receive) = mpsc::channel();
    let readers = vec![
        (
            "stdout",
            Box::new(child.stdout.take().expect("stdout")) as Box<dyn Read + Send>,
        ),
        (
            "stderr",
            Box::new(child.stderr.take().expect("stderr")) as Box<dyn Read + Send>,
        ),
    ];
    #[cfg(windows)]
    let readers = {
        let mut readers = readers;
        readers.push((
            "extra inheritable pipe",
            Box::new(extra_reader) as Box<dyn Read + Send>,
        ));
        readers
    };
    let reader_count = readers.len();
    for (label, reader) in readers {
        let send = send.clone();
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = reader.take(64 * 1024).read_to_end(&mut bytes);
            let _ = send.send((label, result, bytes));
        });
    }
    drop(send);
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll captured client") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("captured client did not exit within 15 seconds");
        }
        thread::sleep(Duration::from_millis(20));
    };
    assert!(status.success(), "captured client failed: {status}");
    let mut stdout = None;
    for _ in 0..reader_count {
        let (label, result, bytes) = receive
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("captured stdout/stderr must close while the engine remains alive");
        result.expect("read captured output");
        if label == "stdout" {
            stdout = Some(String::from_utf8_lossy(&bytes).into_owned());
        }
    }
    let Outcome::Status { pid, .. } = request(&data_dir, EngineCommand::Status)
        .expect("engine must remain alive after the captured client exits")
    else {
        panic!("expected engine status");
    };
    assert!(stdout
        .expect("captured stdout")
        .contains(&format!("captured-client-engine-pid={pid}")));
    request(&data_dir, EngineCommand::Stop).expect("stop only the disposable engine");
}

#[cfg(windows)]
fn inheritable_pipe() -> (fs::File, std::os::windows::io::OwnedHandle) {
    use std::ffi::c_void;
    use std::os::windows::io::{FromRawHandle, OwnedHandle};

    #[repr(C)]
    struct SecurityAttributes {
        size: u32,
        descriptor: *mut c_void,
        inherit: i32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreatePipe(
            read: *mut *mut c_void,
            write: *mut *mut c_void,
            security: *const SecurityAttributes,
            size: u32,
        ) -> i32;
    }
    let mut read = std::ptr::null_mut();
    let mut write = std::ptr::null_mut();
    let security = SecurityAttributes {
        size: std::mem::size_of::<SecurityAttributes>() as u32,
        descriptor: std::ptr::null_mut(),
        inherit: 1,
    };
    // SAFETY: the security struct and writable output pointers remain valid;
    // successful returned handles are each transferred to one owning wrapper.
    unsafe {
        assert_ne!(
            CreatePipe(&mut read, &mut write, &security, 0),
            0,
            "create inheritable fixture pipe"
        );
        (
            fs::File::from_raw_handle(read),
            OwnedHandle::from_raw_handle(write),
        )
    }
}

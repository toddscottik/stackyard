//! The native case needs an explicitly selected, previously verified artifact.
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, Socket, Type};
use stackyard_core::{CacheInstanceRecord, Outcome, PackageRecord, Store};
use stackyard_engine::Caches;

struct Fixture {
    dir: PathBuf,
    caches: Arc<Caches>,
}

impl Fixture {
    fn new(executable: &Path) -> Self {
        let dir = stackyard_core::prepare_data_dir(
            &stackyard_core::disposable_dir("cache-provider").join("данные with spaces"),
        )
        .unwrap();
        Store::open(&dir)
            .unwrap()
            .insert_package(&PackageRecord {
                id: "explicit-cache".into(),
                kind: "server".into(),
                name: "memcached".into(),
                version: "1.6.8".into(),
                target: stackyard_core::current_target().into(),
                source: "existing".into(),
                path: executable.parent().unwrap().display().to_string(),
                launch: executable.display().to_string(),
                digest: None,
                removable: false,
            })
            .unwrap();
        Self {
            caches: Caches::new(dir.clone()),
            dir,
        }
    }
    fn rows(&self) -> Vec<CacheInstanceRecord> {
        let Outcome::Caches { instances } = self.caches.list().unwrap() else {
            panic!("cache outcome");
        };
        instances
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.caches.shutdown();
        if !thread::panicking() {
            let _ = fs::remove_dir_all(&self.dir);
        } else {
            eprintln!("Retained cache fixture: {}", self.dir.display());
        }
    }
}

fn probe(port: u16) {
    let result = stackyard_core::memcached::probe_memcached(
        stackyard_core::memcached::MemcachedEndpoint {
            address: "127.0.0.1".parse().unwrap(),
            port,
        },
        Duration::from_secs(2),
    )
    .unwrap();
    assert_eq!(result.version, "1.6.8");
    assert!(result.roundtrip_verified && result.cleanup_verified);
}

fn exchange(port: u16, request: &[u8]) -> String {
    let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client.write_all(request).unwrap();
    let mut response = Vec::new();
    let mut byte = [0];
    while response.len() < 512 {
        client.read_exact(&mut byte).unwrap();
        response.push(byte[0]);
        if response.ends_with(b"END\r\n") || response.ends_with(b"STORED\r\n") {
            return String::from_utf8(response).unwrap();
        }
    }
    panic!("unbounded native fixture response");
}

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn raw_contender(executable: &Path, port: u16) -> OwnedChild {
    let mut command = Command::new(executable);
    command
        .args([
            "-l",
            "127.0.0.1",
            "-p",
            &port.to_string(),
            "-U",
            "0",
            "-m",
            "16",
            "-t",
            "1",
        ])
        .current_dir(executable.parent().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    OwnedChild(command.spawn().unwrap())
}

fn kill_owned_backend(pid: u32) {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
        let handle = OpenProcess(PROCESS_TERMINATE, false, pid).unwrap();
        TerminateProcess(handle, 37).unwrap();
        CloseHandle(handle).unwrap();
    }
    #[cfg(unix)]
    unsafe {
        assert_eq!(libc::kill(pid as i32, libc::SIGKILL), 0);
    }
}

#[test]
fn occupied_external_listener_is_preserved_before_any_spawn() {
    // current_exe is deliberately not a Memcached binary. Occupied-port
    // validation must fail before executing it or creating a private socket.
    let fixture = Fixture::new(&std::env::current_exe().unwrap());
    let external = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = external.local_addr().unwrap();
    assert!(fixture
        .caches
        .create("explicit-cache", address.port())
        .is_err());
    let rows = fixture.rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].state, "failed");
    assert!(rows[0].pid.is_none() && rows[0].socket_path.is_empty());
    let mut client = TcpStream::connect(address).unwrap();
    let (mut accepted, _) = external.accept().unwrap();
    accepted.write_all(b"external-owner").unwrap();
    let mut marker = [0; 14];
    client.read_exact(&mut marker).unwrap();
    assert_eq!(&marker, b"external-owner");
    fixture.caches.stop(&rows[0].id).unwrap();
    fixture.caches.forget(&rows[0].id).unwrap();
    assert!(fixture.rows().is_empty());
    assert!(Store::open(&fixture.dir)
        .unwrap()
        .get_package("explicit-cache")
        .unwrap()
        .is_some());
    assert!(fixture.caches.create("explicit-cache", 80).is_err());
}

#[test]
fn failed_native_launch_releases_reserved_port_and_retains_retry_record() {
    let fixture = Fixture::new(&std::env::current_exe().unwrap());
    // The Rust test executable rejects Memcached's -s option and exits. This
    // is failure-path evidence only; it is not a Memcached server fixture.
    assert!(fixture.caches.create("explicit-cache", 0).is_err());
    let failed = fixture.rows().remove(0);
    assert_eq!(failed.state, "failed");
    assert!(failed.pid.is_none() && failed.socket_path.is_empty());
    assert!(failed.port >= 1024);
    assert!(TcpListener::bind(("127.0.0.1", failed.port)).is_ok());
    assert!(fixture.caches.start(&failed.id).is_err());
    assert_eq!(fixture.rows().len(), 1);
    fixture.caches.stop(&failed.id).unwrap();
    fixture.caches.forget(&failed.id).unwrap();
    assert!(fixture.rows().is_empty());
}

#[test]
fn reconcile_and_forget_leave_foreign_paths_and_reused_pids_untouched() {
    let fixture = Fixture::new(&std::env::current_exe().unwrap());
    let foreign = fixture.dir.join("foreign-socket-placeholder");
    fs::write(&foreign, "keep external file").unwrap();
    Store::open(&fixture.dir)
        .unwrap()
        .upsert_cache_instance(&CacheInstanceRecord {
            id: "stale-cache".into(),
            package_id: "explicit-cache".into(),
            port: 11211,
            state: "running".into(),
            pid: Some(std::process::id()),
            identity: Some("a-different-creation-identity".into()),
            socket_path: foreign.display().to_string(),
            detail: String::new(),
        })
        .unwrap();
    fixture.caches.reconcile().unwrap();
    assert_eq!(fixture.rows()[0].state, "stopped");
    fixture.caches.stop("stale-cache").unwrap();
    fixture.caches.forget("stale-cache").unwrap();
    assert_eq!(fs::read_to_string(foreign).unwrap(), "keep external file");
    assert!(fixture.rows().is_empty());
}

#[test]
fn wrong_runtime_or_target_never_becomes_a_server_instance() {
    let fixture = Fixture::new(&std::env::current_exe().unwrap());
    let store = Store::open(&fixture.dir).unwrap();
    let mut package = store.get_package("explicit-cache").unwrap().unwrap();
    package.id = "wrong-runtime".into();
    package.name = "php".into();
    store.insert_package(&package).unwrap();
    assert!(fixture.caches.create(&package.id, 0).is_err());
    package.id = "wrong-target".into();
    package.name = "memcached".into();
    package.target = "foreign-target".into();
    store.insert_package(&package).unwrap();
    assert!(fixture.caches.create(&package.id, 0).is_err());
    assert!(fixture.caches.create("missing-package", 0).is_err());
    assert!(fixture.caches.start("../escape").is_err());
    assert!(fixture.rows().is_empty());
}

#[test]
fn native_private_server_exclusive_relay_lifecycle_and_failure_cleanup() {
    let Some(executable) = std::env::var_os("STACKYARD_TEST_MEMCACHED") else {
        eprintln!("SKIPPED native cache lifecycle: set STACKYARD_TEST_MEMCACHED to a previously verified binary; transport fixtures do not prove native Memcached support");
        return;
    };
    let executable = PathBuf::from(executable);
    let fixture = Fixture::new(&executable);
    fixture.caches.create("explicit-cache", 0).unwrap();
    let first = fixture.rows().remove(0);
    assert_eq!(first.state, "running");
    assert!(first.pid.is_some() && Path::new(&first.socket_path).exists());
    probe(first.port);
    fixture.caches.create("explicit-cache", 0).unwrap();
    let second = fixture
        .rows()
        .into_iter()
        .find(|row| row.id != first.id)
        .unwrap();
    probe(second.port);
    assert_ne!(first.port, second.port);
    assert!(fixture.caches.forget(&first.id).is_err());
    assert!(fixture.caches.start(&first.id).is_err());
    let contender = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)).unwrap();
    contender.set_reuse_address(true).unwrap();
    assert!(contender
        .bind(&SocketAddr::from(([127, 0, 0, 1], first.port)).into())
        .is_err());
    let mut raw = raw_contender(&executable, first.port);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = raw.0.try_wait().unwrap() {
            assert!(!status.success());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "raw server must not steal exclusive relay port"
        );
        thread::sleep(Duration::from_millis(25));
    }
    probe(first.port);
    assert_eq!(
        exchange(first.port, b"set lifecycle-key 0 60 5\r\nfirst\r\n"),
        "STORED\r\n"
    );
    assert_eq!(
        exchange(second.port, b"set lifecycle-key 0 60 6\r\nsecond\r\n"),
        "STORED\r\n"
    );

    // Another provider lacking this process's run handles must report an
    // orphan and refuse implicit adoption/stopping. The original stays alive.
    let observer = Caches::new(fixture.dir.clone());
    observer.reconcile().unwrap();
    assert!(observer.start(&first.id).is_err());
    assert!(observer.stop(&first.id).is_err());
    assert!(observer.forget(&first.id).is_err());
    drop(observer);
    probe(first.port);

    let mut inflight = TcpStream::connect(("127.0.0.1", first.port)).unwrap();
    inflight
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let started = Instant::now();
    fixture.caches.stop(&first.id).unwrap();
    assert!(started.elapsed() < Duration::from_secs(3));
    let mut byte = [0];
    assert!(
        !matches!(inflight.read(&mut byte), Err(err) if matches!(err.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock))
    );
    drop(inflight);
    assert!(!Path::new(&first.socket_path).exists());
    assert!(!Path::new(&first.socket_path).parent().unwrap().exists());
    fixture.caches.start(&first.id).unwrap();
    let restarted = fixture
        .rows()
        .into_iter()
        .find(|row| row.id == first.id)
        .unwrap();
    assert_eq!(restarted.port, first.port);
    assert_eq!(exchange(first.port, b"get lifecycle-key\r\n"), "END\r\n");
    assert!(exchange(second.port, b"get lifecycle-key\r\n").contains("second"));

    kill_owned_backend(restarted.pid.unwrap());
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let crashed = fixture
            .rows()
            .into_iter()
            .find(|row| row.id == first.id)
            .unwrap();
        if crashed.state == "failed" {
            assert!(crashed.pid.is_none() && crashed.socket_path.is_empty());
            assert!(crashed.detail.contains("exited"));
            break;
        }
        assert!(Instant::now() < deadline, "backend crash was not observed");
        thread::sleep(Duration::from_millis(30));
    }
    assert!(TcpStream::connect(("127.0.0.1", first.port)).is_err());
    assert!(!Path::new(&restarted.socket_path).exists());
    probe(second.port);
    fixture.caches.start(&first.id).unwrap();
    probe(first.port);
    fixture.caches.stop(&first.id).unwrap();
    fixture.caches.forget(&first.id).unwrap();
    assert_eq!(fixture.rows().len(), 1);
    fixture.caches.shutdown();
    let stopped = fixture.rows().remove(0);
    assert_eq!(stopped.state, "stopped");
    assert!(TcpStream::connect(("127.0.0.1", second.port)).is_err());
    assert!(!Path::new(&second.socket_path).exists());
    assert!(Store::open(&fixture.dir)
        .unwrap()
        .get_package("explicit-cache")
        .unwrap()
        .is_some());
}

#[test]
fn dropped_provider_stops_owned_process_without_arc_cycle() {
    let Some(executable) = std::env::var_os("STACKYARD_TEST_MEMCACHED") else {
        return;
    };
    let fixture = Fixture::new(&PathBuf::from(executable));
    let provider = Caches::new(fixture.dir.clone());
    provider.create("explicit-cache", 0).unwrap();
    let record = fixture.rows().remove(0);
    probe(record.port);
    drop(provider);
    assert!(TcpStream::connect(("127.0.0.1", record.port)).is_err());
    assert!(!Path::new(&record.socket_path).exists());
    assert_eq!(fixture.rows()[0].state, "stopped");
}

#[test]
fn shutdown_during_native_start_cleans_partial_or_ready_run() {
    let Some(executable) = std::env::var_os("STACKYARD_TEST_MEMCACHED") else {
        return;
    };
    let fixture = Fixture::new(&PathBuf::from(executable));
    let provider = Arc::clone(&fixture.caches);
    let startup = thread::spawn(move || provider.create("explicit-cache", 0));
    let deadline = Instant::now() + Duration::from_secs(3);
    let observed = loop {
        if let Some(record) = Store::open(&fixture.dir)
            .unwrap()
            .list_cache_instances()
            .unwrap()
            .into_iter()
            .find(|row| !row.socket_path.is_empty())
        {
            break record;
        }
        assert!(
            Instant::now() < deadline,
            "startup did not publish its owned socket path"
        );
        thread::sleep(Duration::from_millis(1));
    };
    let started = Instant::now();
    fixture.caches.shutdown();
    let _ = startup.join().unwrap(); // either ready then stopped, or cancelled
    assert!(started.elapsed() < Duration::from_secs(3));
    let stopped = fixture.rows().remove(0);
    assert!(matches!(stopped.state.as_str(), "failed" | "stopped"));
    assert!(stopped.pid.is_none() && stopped.socket_path.is_empty());
    assert!(TcpStream::connect(("127.0.0.1", stopped.port)).is_err());
    assert!(!Path::new(&observed.socket_path).exists());
    assert!(!Path::new(&observed.socket_path).parent().unwrap().exists());
}

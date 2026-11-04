//! Managed Memcached uses a private local socket, never the server's unsafe TCP
//! listener. The engine holds an exclusive loopback listener for the full run.
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use stackyard_core::{CacheInstanceRecord, Error, Outcome, Result, Store};

use crate::group::{self, GroupProcess};

const IO_WAIT: Duration = Duration::from_millis(200);
const MAX_CONNECTIONS: usize = 32;

fn error(detail: impl Into<String>) -> Error {
    Error::OperationFailed(format!("managed Memcached: {}", detail.into()))
}

struct Running {
    stop: Arc<AtomicBool>,
    monitor: JoinHandle<()>,
}

pub struct Caches {
    data_dir: PathBuf,
    shutting_down: AtomicBool,
    operation: Mutex<()>,
    running: Mutex<HashMap<String, Running>>,
}

impl Caches {
    pub fn new(data_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            data_dir,
            shutting_down: AtomicBool::new(false),
            operation: Mutex::new(()),
            running: Mutex::new(HashMap::new()),
        })
    }

    pub fn reconcile(&self) -> Result<()> {
        let _operation = self.operation.lock().expect("cache operation");
        let store = Store::open(&self.data_dir)?;
        for mut record in store.list_cache_instances()? {
            if self
                .running
                .lock()
                .expect("cache runs")
                .contains_key(&record.id)
            {
                continue;
            }
            if recorded_alive(&record) {
                record.state = "orphaned".into();
                record.detail = "Recorded process is still alive; no listener/process was adopted or stopped. Stop it explicitly outside Stackyard before retrying.".into();
            } else {
                record.state = "stopped".into();
                record.pid = None;
                record.identity = None;
                // A persisted path is not an in-memory ownership claim. Never
                // remove a crashed run's directory while reconciling records.
                record.detail = "Engine restart: cache is stopped; cached values are volatile. Old socket paths were left untouched.".into();
            }
            store.upsert_cache_instance(&record)?;
        }
        Ok(())
    }

    pub fn list(&self) -> Result<Outcome> {
        self.reap();
        self.view()
    }

    pub fn create(self: &Arc<Self>, package_id: &str, port: u16) -> Result<Outcome> {
        let _operation = self.operation.lock().expect("cache operation");
        self.package(package_id)?;
        validate_port(port)?;
        let id = format!("memcached-{}", &stackyard_core::random_secret()?[..12]);
        let mut record = CacheInstanceRecord {
            id,
            package_id: package_id.into(),
            port,
            state: "stopped".into(),
            pid: None,
            identity: None,
            socket_path: String::new(),
            detail: "Cache values are volatile; stop/restart discards them.".into(),
        };
        Store::open(&self.data_dir)?.upsert_cache_instance(&record)?;
        self.start_record(&mut record)?;
        self.view()
    }

    pub fn start(self: &Arc<Self>, id: &str) -> Result<Outcome> {
        let _operation = self.operation.lock().expect("cache operation");
        self.reap();
        let mut record = self.instance(id)?;
        if self.running.lock().expect("cache runs").contains_key(id) {
            return Err(error(format!("{id} already has an owned process")));
        }
        self.start_record(&mut record)?;
        self.view()
    }

    pub fn stop(&self, id: &str) -> Result<Outcome> {
        let _operation = self.operation.lock().expect("cache operation");
        let record = self.instance(id)?;
        let running = self.running.lock().expect("cache runs").remove(id);
        if let Some(running) = running {
            running.stop.store(true, Ordering::Release);
            running
                .monitor
                .join()
                .map_err(|_| error("cache monitor failed"))?;
        } else if recorded_alive(&record) {
            return Err(error(
                "recorded process has no owned run handle; stopping an orphan was refused",
            ));
        } else {
            let mut record = record;
            record.state = "stopped".into();
            record.pid = None;
            record.identity = None;
            record.detail =
                "Stopped; cached values are volatile. No external process was stopped.".into();
            Store::open(&self.data_dir)?.upsert_cache_instance(&record)?;
        }
        self.view()
    }

    pub fn forget(&self, id: &str) -> Result<Outcome> {
        let _operation = self.operation.lock().expect("cache operation");
        self.reap();
        let record = self.instance(id)?;
        if record.state != "stopped"
            || recorded_alive(&record)
            || self.running.lock().expect("cache runs").contains_key(id)
        {
            return Err(error("stop the owned cache before forgetting its record"));
        }
        // Forget removes metadata only. Package/logs/unknown socket paths stay.
        Store::open(&self.data_dir)?.delete_cache_instance(id)?;
        self.view()
    }

    pub fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::Release);
        let _operation = self.operation.lock().expect("cache operation");
        let runs: Vec<_> = self
            .running
            .lock()
            .expect("cache runs")
            .drain()
            .map(|(_, run)| run)
            .collect();
        for run in &runs {
            run.stop.store(true, Ordering::Release);
        }
        for run in runs {
            let _ = run.monitor.join();
        }
    }

    fn view(&self) -> Result<Outcome> {
        Ok(Outcome::Caches {
            instances: Store::open(&self.data_dir)?.list_cache_instances()?,
        })
    }

    fn instance(&self, id: &str) -> Result<CacheInstanceRecord> {
        if id.is_empty()
            || id.len() > 64
            || !id.starts_with(|c: char| c.is_ascii_lowercase())
            || !id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(error("cache instance id must be a lowercase identifier"));
        }
        Store::open(&self.data_dir)?
            .get_cache_instance(id)?
            .ok_or_else(|| error(format!("cache instance {id:?} was not found")))
    }

    fn package(&self, id: &str) -> Result<stackyard_core::PackageRecord> {
        let package = Store::open(&self.data_dir)?
            .get_package(id)?
            .ok_or_else(|| Error::PackageNotFound { id: id.into() })?;
        if package.name != "memcached"
            || !matches!(package.source.as_str(), "managed" | "existing")
            || package.target != stackyard_core::current_target()
            || !Path::new(&package.launch).is_file()
        {
            return Err(error("select an installed Memcached package for this native target; explicit existing references are allowed"));
        }
        Ok(package)
    }

    fn reap(&self) {
        let mut running = self.running.lock().expect("cache runs");
        let finished: Vec<_> = running
            .iter()
            .filter(|(_, run)| run.monitor.is_finished())
            .map(|(id, _)| id.clone())
            .collect();
        for id in finished {
            if let Some(run) = running.remove(&id) {
                let _ = run.monitor.join();
            }
        }
    }

    fn start_record(&self, record: &mut CacheInstanceRecord) -> Result<()> {
        if recorded_alive(record) {
            return Err(error(
                "recorded process still exists without an owned run; implicit adoption was refused",
            ));
        }
        let outcome = self.prepare_run(record);
        if let Err(err) = &outcome {
            record.state = "failed".into();
            record.pid = None;
            record.identity = None;
            record.socket_path.clear();
            record.detail = err.to_string();
            Store::open(&self.data_dir)?.upsert_cache_instance(record)?;
        }
        outcome
    }

    fn prepare_run(&self, record: &mut CacheInstanceRecord) -> Result<()> {
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(error("startup cancelled by engine shutdown"));
        }
        let package = self.package(&record.package_id)?;
        let listener = exclusive_listener(record.port)?; // held through spawn and the entire run
        record.port = listener
            .local_addr()
            .map_err(|e| Error::io("read cache listener port", e))?
            .port();
        let private = PrivateSocket::create()?;
        record.state = "starting".into();
        record.socket_path = private.path.display().to_string();
        record.pid = None;
        record.identity = None;
        Store::open(&self.data_dir)?.upsert_cache_instance(record)?;
        let executable = Path::new(&package.launch);
        let logs = self.data_dir.join("cache");
        fs::create_dir_all(&logs).map_err(|e| Error::io("create cache log directory", e))?;
        let process = group::spawn_grouped(
            executable,
            &[
                "-s".into(),
                private.path.display().to_string(),
                "-a".into(),
                "700".into(),
                "-U".into(),
                "0".into(),
                "-m".into(),
                "16".into(),
                "-t".into(),
                "1".into(),
            ],
            executable.parent().unwrap_or(executable),
            &[],
            &logs.join(format!("{}.log", record.id)),
        )?;
        let mut startup = Startup {
            process: Some(process),
            relay: None,
            private: Some(private),
        };
        let process = startup.process.as_mut().expect("cache process");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if self.shutting_down.load(Ordering::Acquire) {
                return Err(error("startup cancelled by engine shutdown"));
            }
            if let Some(status) = process
                .child
                .try_wait()
                .map_err(|e| Error::io("check cache startup", e))?
            {
                return Err(error(format!(
                    "native server exited {status} before its private socket was ready"
                )));
            }
            if connect_private(&startup.private.as_ref().expect("private socket").path).is_ok() {
                break;
            }
            if Instant::now() >= deadline {
                return Err(error("private socket readiness timed out"));
            }
            thread::sleep(Duration::from_millis(30));
        }
        startup.relay = Some(Relay::start(
            listener,
            startup
                .private
                .as_ref()
                .expect("private socket")
                .path
                .clone(),
        )?);
        stackyard_core::memcached::probe_memcached(
            stackyard_core::memcached::MemcachedEndpoint {
                address: "127.0.0.1".parse().expect("loopback"),
                port: record.port,
            },
            Duration::from_secs(2),
        )?;
        let process = startup.process.as_mut().expect("cache process");
        if process
            .child
            .try_wait()
            .map_err(|e| Error::io("check cache readiness", e))?
            .is_some()
        {
            return Err(error(
                "native server exited immediately after protocol verification",
            ));
        }
        record.pid = Some(process.pid);
        record.identity = Some(process.identity.clone());
        record.state = "running".into();
        record.detail =
            "Private native socket behind an exclusive engine loopback relay; values are volatile."
                .into();
        Store::open(&self.data_dir)?.upsert_cache_instance(record)?;
        let mut process = startup.process.take().expect("cache process");
        let relay = startup.relay.take().expect("cache relay");
        let private = startup.private.take().expect("cache socket");
        let stop = Arc::clone(&relay.stop);
        let monitor_stop = Arc::clone(&stop);
        let data_dir = self.data_dir.clone();
        let mut observed = record.clone();
        let monitor = thread::spawn(move || {
            let reason = loop {
                if monitor_stop.load(Ordering::Acquire) {
                    break None;
                }
                match process.child.try_wait() {
                    Ok(Some(status)) => {
                        break Some(format!(
                            "native server exited {status}; relay closed and volatile values lost"
                        ))
                    }
                    Err(err) => break Some(format!("native server monitoring failed: {err}")),
                    Ok(None) => thread::sleep(Duration::from_millis(30)),
                }
            };
            relay.shutdown();
            process.request_stop();
            let _ = process.wait_timeout(Duration::from_secs(2));
            drop(private);
            observed.state = if reason.is_some() {
                "failed"
            } else {
                "stopped"
            }
            .into();
            observed.detail = reason.unwrap_or_else(|| "Stopped; cached values were discarded. Other instances/endpoints were not stopped.".into());
            observed.pid = None;
            observed.identity = None;
            observed.socket_path.clear();
            if let Ok(store) = Store::open(&data_dir) {
                let _ = store.upsert_cache_instance(&observed);
            }
        });
        self.running
            .lock()
            .expect("cache runs")
            .insert(record.id.clone(), Running { stop, monitor });
        Ok(())
    }
}

impl Drop for Caches {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn validate_port(port: u16) -> Result<()> {
    if port != 0 && port < 1024 {
        return Err(error(
            "choose port 0 or an unprivileged port of at least 1024",
        ));
    }
    Ok(())
}

fn recorded_alive(record: &CacheInstanceRecord) -> bool {
    match (record.pid, record.identity.as_deref()) {
        (Some(pid), Some(identity)) => {
            group::same_process(identity, group::process_identity(pid).as_deref())
        }
        _ => false,
    }
}

/// Bind once and hold the listener. A preflight bind/drop cannot secure Windows
/// listeners against the backend's SO_REUSEADDR behavior.
pub fn exclusive_listener(port: u16) -> Result<TcpListener> {
    validate_port(port)?;
    let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))
        .map_err(|e| Error::io("create cache listener", e))?;
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawSocket;
        use windows::Win32::Networking::WinSock::{
            setsockopt, WSAGetLastError, SOCKET, SOL_SOCKET, SO_EXCLUSIVEADDRUSE,
        };
        let value = 1_i32.to_ne_bytes();
        // The socket is valid and the byte slice holds the required BOOL/i32.
        if unsafe {
            setsockopt(
                SOCKET(socket.as_raw_socket() as usize),
                SOL_SOCKET,
                SO_EXCLUSIVEADDRUSE,
                Some(&value),
            )
        } != 0
        {
            return Err(Error::io(
                "make cache listener exclusive",
                io::Error::from_raw_os_error(unsafe { WSAGetLastError().0 }),
            ));
        }
    }
    socket
        .bind(&SocketAddr::from(([127, 0, 0, 1], port)).into())
        .map_err(|e| {
            Error::io(
                "bind exclusive cache loopback listener (existing listener was left untouched)",
                e,
            )
        })?;
    socket
        .listen(32)
        .map_err(|e| Error::io("listen for cache clients", e))?;
    let listener: TcpListener = socket.into();
    listener
        .set_nonblocking(true)
        .map_err(|e| Error::io("configure cache listener", e))?;
    Ok(listener)
}

struct PrivateSocket {
    root: PathBuf,
    path: PathBuf,
    marker: String,
}

impl PrivateSocket {
    fn create() -> Result<Self> {
        let marker = stackyard_core::random_secret()?;
        let root = std::env::temp_dir().join(format!("syc-{}", &marker[..16]));
        fs::create_dir(&root)
            .map_err(|e| Error::io("exclusively create private cache socket directory", e))?;
        let path = root.join("m.sock");
        let owned = Self { root, path, marker };
        stackyard_core::prepare_data_dir(&owned.root)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(owned.root.join("owner"))
            .map_err(|e| Error::io("mark private cache socket ownership", e))?;
        file.write_all(owned.marker.as_bytes())
            .map_err(|e| Error::io("write cache socket ownership", e))?;
        SockAddr::unix(&owned.path)
            .map_err(|e| Error::io("private cache socket path is too long", e))?;
        Ok(owned)
    }
}

impl Drop for PrivateSocket {
    fn drop(&mut self) {
        // Creation can fail before its marker is written. This run created the
        // directory exclusively, so removing an empty directory is safe; never
        // remove any unexpected entries to make that cleanup succeed.
        if !self.root.join("owner").exists() && !self.path.exists() {
            let _ = fs::remove_dir(&self.root);
            return;
        }
        if matches!(fs::read_to_string(self.root.join("owner")), Ok(marker) if marker == self.marker)
        {
            let _ = fs::remove_file(&self.path);
            let _ = fs::remove_file(self.root.join("owner"));
            let _ = fs::remove_dir(&self.root); // deliberately never recursive
        }
    }
}

fn connect_private(path: &Path) -> Result<Socket> {
    let socket = Socket::new(Domain::UNIX, Type::STREAM, None)
        .map_err(|e| Error::io("create private cache client socket", e))?;
    let address =
        SockAddr::unix(path).map_err(|e| Error::io("encode private cache socket path", e))?;
    socket
        .connect_timeout(&address, IO_WAIT)
        .map_err(|e| Error::io("connect private native cache", e))?;
    socket
        .set_read_timeout(Some(IO_WAIT))
        .map_err(|e| Error::io("set cache backend read timeout", e))?;
    socket
        .set_write_timeout(Some(IO_WAIT))
        .map_err(|e| Error::io("set cache backend write timeout", e))?;
    Ok(socket)
}

struct Startup {
    process: Option<GroupProcess>,
    relay: Option<Relay>,
    private: Option<PrivateSocket>,
}

impl Drop for Startup {
    fn drop(&mut self) {
        if let Some(relay) = self.relay.take() {
            relay.shutdown();
        }
        if let Some(mut process) = self.process.take() {
            process.request_stop();
            let _ = process.wait_timeout(Duration::from_secs(2));
        }
        // PrivateSocket drops after its process, even during startup failure.
    }
}

/// Byte-transparent transport. No protocol response is synthesized here.
pub struct Relay {
    stop: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
}

impl Relay {
    pub fn start(listener: TcpListener, path: PathBuf) -> Result<Self> {
        listener
            .set_nonblocking(true)
            .map_err(|e| Error::io("configure cache relay listener", e))?;
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let accept = thread::spawn(move || {
            let mut connections: Vec<JoinHandle<()>> = Vec::new();
            while !worker_stop.load(Ordering::Acquire) {
                let mut index = 0;
                while index < connections.len() {
                    if connections[index].is_finished() {
                        let _ = connections.swap_remove(index).join();
                    } else {
                        index += 1;
                    }
                }
                match listener.accept() {
                    Ok((stream, _)) => {
                        if connections.len() >= MAX_CONNECTIONS {
                            let _ = stream.shutdown(Shutdown::Both);
                            continue;
                        }
                        let stop = Arc::clone(&worker_stop);
                        let path = path.clone();
                        connections.push(thread::spawn(move || {
                            let _ = relay_connection(stream, &path, stop);
                        }));
                    }
                    Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(15))
                    }
                    Err(_) => break,
                }
            }
            worker_stop.store(true, Ordering::Release);
            drop(listener);
            for connection in connections {
                let _ = connection.join();
            }
        });
        Ok(Self {
            stop,
            accept: Some(accept),
        })
    }

    pub fn shutdown(mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(accept) = self.accept.take() {
            let _ = accept.join();
        }
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(accept) = self.accept.take() {
            let _ = accept.join();
        }
    }
}

fn relay_connection(mut tcp: TcpStream, path: &Path, stop: Arc<AtomicBool>) -> Result<()> {
    tcp.set_read_timeout(Some(IO_WAIT))
        .map_err(|e| Error::io("set cache client read timeout", e))?;
    tcp.set_write_timeout(Some(IO_WAIT))
        .map_err(|e| Error::io("set cache client write timeout", e))?;
    let mut backend = connect_private(path)?;
    let mut tcp_return = tcp
        .try_clone()
        .map_err(|e| Error::io("clone cache client", e))?;
    let mut backend_return = backend
        .try_clone()
        .map_err(|e| Error::io("clone private cache client", e))?;
    let local_stop = Arc::new(AtomicBool::new(false));
    let reverse_local = Arc::clone(&local_stop);
    let reverse_stop = Arc::clone(&stop);
    let reverse = thread::spawn(move || {
        if transfer(
            &mut backend_return,
            &mut tcp_return,
            &reverse_stop,
            &reverse_local,
        )
        .is_err()
        {
            reverse_local.store(true, Ordering::Release);
        }
        let _ = tcp_return.shutdown(Shutdown::Write);
    });
    if transfer(&mut tcp, &mut backend, &stop, &local_stop).is_err() {
        local_stop.store(true, Ordering::Release);
    }
    let _ = backend.shutdown(Shutdown::Write);
    let _ = reverse.join();
    let _ = tcp.shutdown(Shutdown::Both);
    let _ = backend.shutdown(Shutdown::Both);
    Ok(())
}

fn transfer(
    reader: &mut impl Read,
    writer: &mut impl Write,
    stop: &AtomicBool,
    local_stop: &AtomicBool,
) -> io::Result<()> {
    let mut buffer = [0; 16 * 1024];
    while !stop.load(Ordering::Acquire) && !local_stop.load(Ordering::Acquire) {
        let count = match reader.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(count) => count,
            Err(err) if retryable(&err) => continue,
            Err(err) => return Err(err),
        };
        let mut written = 0;
        while written < count {
            if stop.load(Ordering::Acquire) || local_stop.load(Ordering::Acquire) {
                return Ok(());
            }
            match writer.write(&buffer[written..count]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "cache relay destination closed",
                    ))
                }
                Ok(count) => written += count,
                Err(err) if retryable(&err) => continue,
                Err(err) => return Err(err),
            }
        }
    }
    Ok(())
}

fn retryable(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn fixture_listener(private: &PrivateSocket) -> Socket {
        let listener = Socket::new(Domain::UNIX, Type::STREAM, None).unwrap();
        listener
            .bind(&SockAddr::unix(&private.path).unwrap())
            .unwrap();
        listener.listen(32).unwrap();
        listener
    }

    fn assert_connection_closed(stream: &mut TcpStream) {
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        match stream.read(&mut [0]) {
            Ok(0) => {}
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted
                ) => {}
            result => panic!("expected closed connection, got {result:?}"),
        }
    }

    #[test]
    fn exclusive_loopback_rejects_reuseaddr_contender() {
        let listener = exclusive_listener(0).unwrap();
        let address = listener.local_addr().unwrap();
        assert!(address.ip().is_loopback());
        let contender = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)).unwrap();
        contender.set_reuse_address(true).unwrap();
        assert!(contender.bind(&address.into()).is_err());
        assert!(exclusive_listener(address.port()).is_err());
    }

    #[test]
    fn relay_preserves_binary_bytes_and_half_close() {
        let private = PrivateSocket::create().unwrap();
        let backend = fixture_listener(&private);
        let bytes: Vec<u8> = (0..96 * 1024).map(|index| (index % 256) as u8).collect();
        let expected = bytes.clone();
        let server = thread::spawn(move || {
            let (mut peer, _) = backend.accept().unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut received = Vec::new();
            peer.read_to_end(&mut received).unwrap();
            assert_eq!(received, expected);
            received.reverse();
            peer.write_all(&received).unwrap();
        });
        let listener = exclusive_listener(0).unwrap();
        let address = listener.local_addr().unwrap();
        let relay = Relay::start(listener, private.path.clone()).unwrap();
        let mut client = TcpStream::connect(address).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        client.write_all(&bytes).unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        let mut received = Vec::new();
        client.read_to_end(&mut received).unwrap();
        assert_eq!(received, bytes.into_iter().rev().collect::<Vec<_>>());
        relay.shutdown();
        server.join().unwrap();
    }

    #[test]
    fn idle_relay_shutdown_releases_listener() {
        let private = PrivateSocket::create().unwrap();
        let listener = exclusive_listener(0).unwrap();
        let address = listener.local_addr().unwrap();
        let relay = Relay::start(listener, private.path.clone()).unwrap();
        relay.shutdown();
        // No TCP connections were made, so TIME_WAIT cannot prevent rebinding.
        let _rebound = exclusive_listener(address.port()).expect("relay released idle listener");
    }

    #[test]
    fn relay_bounds_connections_and_closes_inflight_on_shutdown() {
        let private = PrivateSocket::create().unwrap();
        let backend = fixture_listener(&private);
        backend.set_nonblocking(true).unwrap();
        let accepted = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&accepted);
        let (server_stop, worker_stop) = std::sync::mpsc::channel::<()>();
        let server = thread::spawn(move || {
            let mut peers = Vec::new();
            // Disconnect also stops the fixture when a test assertion panics.
            while matches!(
                worker_stop.try_recv(),
                Err(std::sync::mpsc::TryRecvError::Empty)
            ) {
                match backend.accept() {
                    Ok((mut peer, _)) => {
                        count.fetch_add(1, Ordering::Release);
                        peer.write_all(b"ready").unwrap();
                        peers.push(peer);
                    }
                    Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(err) => panic!("fixture accept: {err}"),
                }
            }
        });
        let listener = exclusive_listener(0).unwrap();
        let address = listener.local_addr().unwrap();
        let relay = Relay::start(listener, private.path.clone()).unwrap();
        let mut clients = Vec::new();
        for index in 0..MAX_CONNECTIONS {
            let mut client = TcpStream::connect(address).unwrap();
            client
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            // A successful TCP connect only enters the listen backlog. Wait
            // for a backend roundtrip before filling the next relay slot.
            let mut ready = [0; 5];
            client.read_exact(&mut ready).unwrap_or_else(|err| {
                panic!("relay connection {} was not ready: {err}", index + 1)
            });
            assert_eq!(&ready, b"ready");
            clients.push(client);
        }
        let mut excess = TcpStream::connect(address).unwrap();
        assert_connection_closed(&mut excess);
        assert_eq!(accepted.load(Ordering::Acquire), MAX_CONNECTIONS);
        let started = Instant::now();
        relay.shutdown();
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "relay shutdown took {:?}",
            started.elapsed()
        );
        for mut client in clients {
            assert_connection_closed(&mut client);
        }
        drop(server_stop);
        server.join().unwrap();
        // Closed connections may leave this port in TIME_WAIT, especially on
        // macOS. Refusing new connections proves the listener was released
        // without requiring immediate reuse of the same local address.
        let err = TcpStream::connect_timeout(&address, IO_WAIT)
            .expect_err("relay listener must be closed after shutdown");
        assert!(
            matches!(
                err.kind(),
                io::ErrorKind::ConnectionRefused | io::ErrorKind::TimedOut
            ),
            "unexpected error connecting to stopped relay: {err}"
        );
    }

    #[test]
    fn private_socket_cleanup_does_not_remove_unknown_files() {
        let private = PrivateSocket::create().unwrap();
        let root = private.root.clone();
        fs::write(root.join("unrelated"), "preserve").unwrap();
        drop(private);
        assert_eq!(
            fs::read_to_string(root.join("unrelated")).unwrap(),
            "preserve"
        );
        assert!(!root.join("owner").exists());
        fs::remove_file(root.join("unrelated")).unwrap();
        fs::remove_dir(root).unwrap();
    }
}

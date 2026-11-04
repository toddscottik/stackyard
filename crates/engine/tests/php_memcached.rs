//! Real Windows PHP/native-cache acceptance. Explicit verified archive paths
//! are required; the ordinary workspace suite visibly skips native acceptance.
#![cfg(windows)]

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command as ProcessCommand, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use stackyard_core::{roundtrip, Command, Outcome, PackageRecord, ProjectView, Store};

struct Engine {
    dir: PathBuf,
    thread: Option<thread::JoinHandle<stackyard_core::Result<()>>>,
}

impl Engine {
    async fn start() -> Self {
        let dir = stackyard_core::prepare_data_dir(&stackyard_core::disposable_dir("php-cache"))
            .expect("disposable engine directory");
        let owned = dir.clone();
        let engine = Self {
            dir,
            thread: Some(thread::spawn(move || stackyard_engine::serve(&owned))),
        };
        let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
        loop {
            if roundtrip(&engine.dir, Command::Status).await.is_ok() {
                return engine;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "engine startup timed out"
            );
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let dir = self.dir.clone();
        let stopper = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let _ = runtime.block_on(roundtrip(&dir, Command::Stop));
        });
        let _ = stopper.join();
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

struct External(Child);

impl Drop for External {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn install(dir: &Path, id: &str, archive: &Path) -> PackageRecord {
    let package = stackyard_core::find_package(id).unwrap_or_else(|_| {
        stackyard_core::php_project::php_extension_catalog()
            .unwrap()
            .into_iter()
            .find(|entry| entry.package.id == id)
            .unwrap_or_else(|| panic!("approved catalog entry {id} missing"))
            .package
    });
    stackyard_core::install_reader(
        dir,
        &package,
        fs::File::open(archive).expect("explicit archive"),
        &stackyard_core::Progress::new(),
    )
    .unwrap_or_else(|error| panic!("install verified {id}: {error}"))
}

async fn select(dir: &Path, project: &str, package: &str) {
    roundtrip(
        dir,
        Command::SelectPackage {
            project_id: project.into(),
            package_id: package.into(),
        },
    )
    .await
    .unwrap();
}

async fn project(dir: &Path, id: &str) -> ProjectView {
    let Outcome::Project { project } = roundtrip(
        dir,
        Command::ProjectStatus {
            project_id: id.into(),
        },
    )
    .await
    .unwrap() else {
        panic!("project status expected");
    };
    project
}

async fn running(dir: &Path, id: &str) -> ProjectView {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
    loop {
        let view = project(dir, id).await;
        if view
            .services
            .iter()
            .all(|service| service.state != "starting")
        {
            assert!(
                view.services
                    .iter()
                    .all(|service| service.state == "running"),
                "{view:?}"
            );
            return view;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "project did not start: {view:?}"
        );
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

async fn start(dir: &Path, id: &str) -> ProjectView {
    roundtrip(
        dir,
        Command::StartProject {
            project_id: id.into(),
        },
    )
    .await
    .unwrap();
    running(dir, id).await
}

fn web_port(view: &ProjectView) -> u16 {
    view.services
        .iter()
        .find(|service| service.id == "web")
        .unwrap()
        .port
        .unwrap()
}

async fn cli(dir: &Path, id: &str, args: &[&str]) -> (String, String) {
    let Outcome::Task { id, .. } = roundtrip(
        dir,
        Command::Exec {
            project_id: id.into(),
            service_id: "web".into(),
            args: args.iter().map(|argument| (*argument).into()).collect(),
        },
    )
    .await
    .unwrap() else {
        panic!("CLI task expected");
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let Outcome::Task { state, message, .. } = roundtrip(
            dir,
            Command::DepStatus {
                operation_id: id.clone(),
            },
        )
        .await
        .unwrap() else {
            panic!("task status expected");
        };
        if !matches!(state.as_str(), "running" | "queued") {
            return (state, message);
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "CLI task deadline exceeded"
        );
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
}

fn http(port: u16) -> String {
    let mut stream =
        TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(2)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn cache_connect(port: u16) -> TcpStream {
    let mut stream =
        TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(1)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream.write_all(b"version\r\n").unwrap();
    assert_eq!(cache_line(&mut stream), "VERSION 1.6.8");
    stream
}

fn cache_line(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        assert!(bytes.len() < 512, "cache response line exceeded limit");
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        bytes.push(byte[0]);
        if byte[0] == b'\n' {
            return String::from_utf8(bytes)
                .unwrap()
                .trim_end_matches(['\r', '\n'])
                .into();
        }
    }
}

fn write_project(root: &Path, id: &str, cache_port: u16) {
    fs::create_dir_all(root).unwrap();
    fs::write(
        root.join("stackyard.toml"),
        format!(
            r#"schema = 1
name = "{id}"
manager = "composer"
[[service]]
id = "web"
runtime = "php"
command = ["php", "-S", "127.0.0.1:{{port}}", "-t", "."]
ready_path = "/ready.php"
ready_timeout_secs = 5
"#
        ),
    )
    .unwrap();
    fs::write(root.join("ready.php"), "<?php echo 'ready';").unwrap();
    fs::write(root.join("data.txt"), "persistent-project-data").unwrap();
    fs::write(root.join("cache.php"), CACHE_SCRIPT).unwrap();
    fs::write(
        root.join("index.php"),
        format!("<?php $probePort={cache_port}; require __DIR__.'/cache.php';"),
    )
    .unwrap();
}

const CACHE_SCRIPT: &str = r#"<?php
header('Content-Type: text/plain');
if (!class_exists('Memcached')) { echo 'missing-extension|php:'.PHP_VERSION; exit(42); }
$m = new Memcached();
$m->setOption(Memcached::OPT_CONNECT_TIMEOUT, 500);
$m->setOption(Memcached::OPT_POLL_TIMEOUT, 500);
$m->setOption(Memcached::OPT_RECV_TIMEOUT, 500000);
$m->setOption(Memcached::OPT_SEND_TIMEOUT, 500000);
$m->addServer('127.0.0.1', $probePort ?? (int)$argv[1]);
$key = 'stackyard:php-acceptance:'.bin2hex(random_bytes(16));
$reserved = false;
try {
    if (!$m->add($key, 'owned', 30)) { echo 'endpoint-unavailable'; exit(43); }
    $reserved = true;
    if (!$m->set($key, 'native-php-client', 30) || $m->get($key) !== 'native-php-client') { throw new RuntimeException('set-get-failed'); }
    if (!$m->delete($key)) { throw new RuntimeException('delete-failed'); }
    $reserved = false;
    if ($m->get($key) !== false || $m->getResultCode() !== Memcached::RES_NOTFOUND) { throw new RuntimeException('cleanup-failed'); }
    echo 'set-get:ok|cleanup:ok|php:'.PHP_VERSION.'|memcached:'.phpversion('memcached').'|'.file_get_contents(__DIR__.'/data.txt');
} finally { if ($reserved) { $m->delete($key); } }
"#;

#[tokio::test]
async fn verified_native_cache_php_profiles_and_patch_update() {
    let names = [
        "STACKYARD_TEST_PHP8511_ARCHIVE",
        "STACKYARD_TEST_PHP8512_ARCHIVE",
        "STACKYARD_TEST_MEMCACHED_EXTENSION8511_ARCHIVE",
        "STACKYARD_TEST_MEMCACHED_EXTENSION8512_ARCHIVE",
        "STACKYARD_TEST_MEMCACHED_SERVER_ARCHIVE",
    ];
    let archives: Option<Vec<PathBuf>> = names
        .iter()
        .map(|name| std::env::var_os(name).map(PathBuf::from))
        .collect();
    let Some(archives) = archives else {
        eprintln!("SKIPPED native PHP memcached acceptance: set all five explicit verified archive variables; no native runtime evidence from this run");
        return;
    };
    let engine = Engine::start().await;
    let old_php = install(&engine.dir, "php-8.5.11-windows-x64", &archives[0]);
    let new_php = install(&engine.dir, "php-8.5.12-1-windows-x64", &archives[1]);
    let old_extension = install(
        &engine.dir,
        "php-memcached-3.4.0-php8.5.11-windows-x64",
        &archives[2],
    );
    // Deliberately incompatible reverse-load fixture: this approved publisher
    // pin is used only here, without advertising another product catalog item.
    let mut newer_descriptor = stackyard_core::php_project::php_extension_catalog()
        .unwrap()
        .into_iter()
        .find(|entry| entry.package.id == old_extension.id)
        .unwrap()
        .package;
    newer_descriptor.id = "php-memcached-newer-probe-artifact".into();
    newer_descriptor.version = "3.4.0-php8.5.12-probe".into();
    newer_descriptor.url = "https://github.com/ServBay/PHP-Windows-Portable/releases/download/php-8.5.12-x64-nts/php-8.5.12-windows-x64.zip".into();
    newer_descriptor.sha256 =
        "b2e21593123c4e53930b8afb4a6686e4e9f9d8481caccff43c545679992e58c9".into();
    newer_descriptor.digest_source = "https://api.github.com/repos/ServBay/PHP-Windows-Portable/releases/tags/php-8.5.12-x64-nts".into();
    let new_extension = stackyard_core::install_reader(
        &engine.dir,
        &newer_descriptor,
        fs::File::open(&archives[3]).unwrap(),
        &stackyard_core::Progress::new(),
    )
    .unwrap();
    let server = install(&engine.dir, "memcached-1.6.8-windows-x64", &archives[4]);
    select(&engine.dir, "default", &old_php.id).await;
    let Outcome::Caches { instances } = roundtrip(
        &engine.dir,
        Command::CacheCreate {
            package_id: server.id.clone(),
            port: 0,
        },
    )
    .await
    .unwrap() else {
        panic!("cache create result");
    };
    let managed = instances
        .into_iter()
        .find(|instance| instance.package_id == server.id)
        .expect("managed cache record");
    assert_eq!(managed.state, "running", "{managed:?}");
    assert!(managed.pid.is_some());
    let managed_port = managed.port;
    cache_connect(managed_port);
    let enabled_root = engine.dir.join("PHP projects/включен project");
    let disabled_root = engine.dir.join("PHP projects/disabled project");
    for (id, root) in [
        ("cache-enabled", &enabled_root),
        ("cache-disabled", &disabled_root),
    ] {
        write_project(root, id, managed_port);
        roundtrip(
            &engine.dir,
            Command::ImportProject {
                path: root.display().to_string(),
            },
        )
        .await
        .unwrap();
        select(&engine.dir, id, &old_php.id).await;
    }
    let inventory =
        stackyard_core::php_project::php_project_view(&engine.dir, "cache-enabled").unwrap();
    let old_artifact = inventory
        .inventory
        .iter()
        .find(|artifact| artifact.root == Path::new(&old_extension.path))
        .expect("installed old extension inventory")
        .clone();
    roundtrip(
        &engine.dir,
        Command::PhpApplyProfile {
            project_id: "cache-enabled".into(),
            extensions: vec![old_artifact.id.clone()],
        },
    )
    .await
    .unwrap();
    roundtrip(
        &engine.dir,
        Command::PhpApplyProfile {
            project_id: "cache-disabled".into(),
            extensions: Vec::new(),
        },
    )
    .await
    .unwrap();
    let original_profile = stackyard_core::php::read_php_profile(&engine.dir, "cache-enabled")
        .unwrap()
        .unwrap();
    // The newer extension has the same nominal API, but actually imports a
    // symbol absent from PHP8.5.11. Reject that native load before promotion.
    let mut bad = old_artifact.clone();
    bad.id = "newer-native-artifact".into();
    bad.root = PathBuf::from(&new_extension.path);
    bad.library = bad.root.join("ext/php_memcached.dll");
    bad.dependencies = vec![
        bad.root.join("libmemcached.dll"),
        bad.root.join("libhashkit.dll"),
    ];
    bad.abi = stackyard_core::php::probe_php(Path::new(&new_php.launch)).unwrap();
    assert!(stackyard_core::php::apply_php_profile(
        &engine.dir,
        "cache-enabled",
        Path::new(&old_php.launch),
        &[bad]
    )
    .is_err());
    assert_eq!(
        stackyard_core::php::read_php_profile(&engine.dir, "cache-enabled").unwrap(),
        Some(original_profile.clone())
    );
    let enabled = start(&engine.dir, "cache-enabled").await;
    let disabled = start(&engine.dir, "cache-disabled").await;
    assert!(http(web_port(&enabled))
        .contains("set-get:ok|cleanup:ok|php:8.5.11|memcached:3.4.0|persistent-project-data"));
    assert!(http(web_port(&disabled)).contains("missing-extension|php:8.5.11"));
    let (state, output) = cli(
        &engine.dir,
        "cache-enabled",
        &["php", "cache.php", &managed_port.to_string()],
    )
    .await;
    assert_eq!(state, "succeeded", "{output}");
    assert!(
        output.contains("set-get:ok|cleanup:ok|php:8.5.11"),
        "{output}"
    );
    let (state, output) = cli(
        &engine.dir,
        "cache-disabled",
        &["php", "cache.php", &managed_port.to_string()],
    )
    .await;
    assert_eq!(state, "failed", "{output}");
    assert!(output.contains("missing-extension"), "{output}");
    // Switch one stopped project to the actual second official patch. Its
    // already installed DLL must pass a real load on the new executable.
    roundtrip(
        &engine.dir,
        Command::StopProject {
            project_id: "cache-enabled".into(),
        },
    )
    .await
    .unwrap();
    select(&engine.dir, "cache-enabled", &new_php.id).await;
    let updated = start(&engine.dir, "cache-enabled").await;
    assert!(http(web_port(&updated)).contains("set-get:ok|cleanup:ok|php:8.5.12|memcached:3.4.0"));
    let (state, output) = cli(
        &engine.dir,
        "cache-enabled",
        &["php", "cache.php", &managed_port.to_string()],
    )
    .await;
    assert_eq!(state, "succeeded", "{output}");
    assert!(
        output.contains("set-get:ok|cleanup:ok|php:8.5.12"),
        "{output}"
    );
    let peer = project(&engine.dir, "cache-disabled").await;
    assert_eq!(peer.services[0].pid, disabled.services[0].pid);
    assert!(http(web_port(&peer)).contains("missing-extension|php:8.5.11"));
    assert_eq!(
        Store::open(&engine.dir)
            .unwrap()
            .get_selection("cache-disabled", "php")
            .unwrap(),
        Some(old_php.id)
    );
    assert_eq!(
        stackyard_core::php::read_php_profile(&engine.dir, "cache-enabled")
            .unwrap()
            .unwrap()
            .extensions[0]
            .root,
        old_artifact.root
    );
    // A distinct native process is an explicitly selected external endpoint.
    // It is never registered with the engine, and only this guard owns it.
    let external_port = free_port();
    let mut external = External(
        ProcessCommand::new(&server.launch)
            .args([
                "-l",
                "127.0.0.1",
                "-p",
                &external_port.to_string(),
                "-U",
                "0",
                "-m",
                "16",
                "-t",
                "1",
            ])
            .current_dir(&engine.dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while TcpStream::connect_timeout(
        &([127, 0, 0, 1], external_port).into(),
        Duration::from_millis(100),
    )
    .is_err()
    {
        assert!(
            external.0.try_wait().unwrap().is_none(),
            "external fixture exited"
        );
        assert!(
            Instant::now() < deadline,
            "external fixture startup timed out"
        );
        thread::sleep(Duration::from_millis(30));
    }
    let (state, output) = cli(
        &engine.dir,
        "cache-enabled",
        &["php", "cache.php", &external_port.to_string()],
    )
    .await;
    assert_eq!(state, "succeeded", "{output}");
    assert!(output.contains("set-get:ok|cleanup:ok"), "{output}");
    let mut before_restart = cache_connect(managed_port);
    before_restart
        .write_all(b"set stackyard:volatile-acceptance 0 30 3\r\nold\r\n")
        .unwrap();
    assert_eq!(cache_line(&mut before_restart), "STORED");
    drop(before_restart);
    roundtrip(
        &engine.dir,
        Command::CacheStop {
            instance_id: managed.id.clone(),
        },
    )
    .await
    .unwrap();
    assert!(TcpStream::connect_timeout(
        &([127, 0, 0, 1], managed_port).into(),
        Duration::from_millis(200)
    )
    .is_err());
    assert!(
        external.0.try_wait().unwrap().is_none(),
        "stopping managed cache killed external endpoint"
    );
    cache_connect(external_port);
    let (state, output) = cli(
        &engine.dir,
        "cache-enabled",
        &["php", "cache.php", &managed_port.to_string()],
    )
    .await;
    assert_eq!(state, "failed", "{output}");
    assert!(
        output.contains("endpoint-unavailable") && !output.contains("missing-extension"),
        "{output}"
    );
    roundtrip(
        &engine.dir,
        Command::CacheStart {
            instance_id: managed.id.clone(),
        },
    )
    .await
    .unwrap();
    let mut fresh = cache_connect(managed_port);
    fresh
        .write_all(b"get stackyard:volatile-acceptance\r\n")
        .unwrap();
    assert_eq!(
        cache_line(&mut fresh),
        "END",
        "cache restart unexpectedly preserved volatile memory"
    );
    drop(fresh);
    assert!(http(web_port(&updated)).contains("set-get:ok|cleanup:ok|php:8.5.12"));
    roundtrip(
        &engine.dir,
        Command::CacheStop {
            instance_id: managed.id.clone(),
        },
    )
    .await
    .unwrap();
    roundtrip(
        &engine.dir,
        Command::CacheForget {
            instance_id: managed.id,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        fs::read_to_string(enabled_root.join("data.txt")).unwrap(),
        "persistent-project-data"
    );
    eprintln!("Native acceptance passed: managed/external PHP memcached3.4.0 set/get; official PHP8.5.11->8.5.12; isolated profiles; rejected reverse ABI load; volatile restart; external ownership preserved");
}

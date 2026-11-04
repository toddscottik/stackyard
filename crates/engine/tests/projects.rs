use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use stackyard_core::{prepare_data_dir, roundtrip, Command as EngineCommand, Outcome};

fn temp_dir() -> PathBuf {
    prepare_data_dir(&stackyard_core::disposable_dir("projects")).expect("temp dir")
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_stackyard-fixture"))
}

fn engine_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_stackyard-engine"))
}

struct Engine {
    dir: PathBuf,
    thread: Option<thread::JoinHandle<stackyard_core::Result<()>>>,
}

impl Engine {
    fn start() -> Self {
        let dir = temp_dir();
        let owned = dir.clone();
        let thread = thread::spawn(move || stackyard_engine::serve(&owned));
        Self {
            dir,
            thread: Some(thread),
        }
    }

    async fn ready(&self) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if roundtrip(&self.dir, EngineCommand::Status).await.is_ok() {
                return;
            }
            if tokio::time::Instant::now() > deadline {
                panic!("engine did not accept a status request");
            }
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
                .expect("stop runtime");
            let _ = runtime.block_on(roundtrip(&dir, EngineCommand::Stop));
        });
        let _ = stopper.join();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn write_service(root: &Path, name: &str, services: &str) {
    fs::create_dir_all(root).expect("project dir");
    fs::write(
        root.join("stackyard.toml"),
        format!("schema = 1\nname = \"{name}\"\n{services}"),
    )
    .expect("manifest");
    fs::write(root.join("message.txt"), "one").expect("message");
}

async fn adopt(dir: &Path, role: &str) {
    roundtrip(
        dir,
        EngineCommand::UseExisting {
            role: role.to_string(),
            path: fixture().display().to_string(),
        },
    )
    .await
    .expect("adopt");
}

async fn select(dir: &Path, project: &str, role: &str) {
    roundtrip(
        dir,
        EngineCommand::SelectPackage {
            project_id: project.to_string(),
            package_id: format!("existing-{role}"),
        },
    )
    .await
    .expect("select");
}

async fn import(dir: &Path, root: &Path) {
    roundtrip(
        dir,
        EngineCommand::ImportProject {
            path: root.display().to_string(),
        },
    )
    .await
    .expect("import");
}

async fn start(dir: &Path, id: &str) {
    roundtrip(
        dir,
        EngineCommand::StartProject {
            project_id: id.to_string(),
        },
    )
    .await
    .expect("start");
}

async fn settled(dir: &Path, id: &str) -> stackyard_core::ProjectView {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        let outcome = roundtrip(
            dir,
            EngineCommand::ProjectStatus {
                project_id: id.to_string(),
            },
        )
        .await
        .expect("status");
        let Outcome::Project { project } = outcome else {
            panic!("expected project status");
        };
        if project
            .services
            .iter()
            .all(|service| service.state != "starting")
        {
            return project;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("project {id} stayed starting: {project:?}");
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

fn http_body(port: u16) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("timeout");
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .expect("write");
    let mut text = String::new();
    let _ = stream.read_to_string(&mut text);
    text
}

fn alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

const SERVE: &str = r#"
[[service]]
id = "web"
runtime = "node"
command = ["node", "serve"]
ready_timeout_secs = 5
"#;

#[tokio::test]
async fn two_projects_stop_independently_and_reload() {
    let engine = Engine::start();
    engine.ready().await;
    adopt(&engine.dir, "node").await;
    adopt(&engine.dir, "python").await;
    let node_root = engine.dir.join("node app");
    let python_root = engine.dir.join("пример");
    write_service(&node_root, "node-http", SERVE);
    write_service(
        &python_root,
        "py-http",
        r#"
[[service]]
id = "web"
runtime = "python"
command = ["python", "serve"]
ready_timeout_secs = 5
"#,
    );
    import(&engine.dir, &node_root).await;
    import(&engine.dir, &python_root).await;
    select(&engine.dir, "node-http", "node").await;
    select(&engine.dir, "py-http", "python").await;
    start(&engine.dir, "node-http").await;
    start(&engine.dir, "py-http").await;
    let node = settled(&engine.dir, "node-http").await;
    let python = settled(&engine.dir, "py-http").await;
    assert_eq!(node.services[0].state, "running");
    assert_eq!(python.services[0].state, "running");
    let node_port = node.services[0].port.expect("node port");
    let python_port = python.services[0].port.expect("python port");
    assert_ne!(node_port, python_port);
    assert!(http_body(node_port).contains("one"));
    fs::write(node_root.join("message.txt"), "two").expect("reload");
    let reloaded = http_body(node_port);
    assert!(reloaded.contains("two"), "{reloaded}");
    roundtrip(
        &engine.dir,
        EngineCommand::StopProject {
            project_id: "node-http".to_string(),
        },
    )
    .await
    .expect("stop node");
    assert!(http_body(python_port).contains("one"));
    let refused = TcpStream::connect(("127.0.0.1", node_port));
    assert!(refused.is_err(), "stopped project kept its port");
}

#[tokio::test]
async fn stop_kills_children_and_failure_names_the_error() {
    let engine = Engine::start();
    engine.ready().await;
    adopt(&engine.dir, "node").await;
    let root = engine.dir.join("child proj");
    write_service(
        &root,
        "with-child",
        r#"
[[service]]
id = "web"
runtime = "node"
command = ["node", "serve-child"]
ready_timeout_secs = 5
"#,
    );
    import(&engine.dir, &root).await;
    select(&engine.dir, "with-child", "node").await;
    start(&engine.dir, "with-child").await;
    let running = settled(&engine.dir, "with-child").await;
    assert_eq!(running.services[0].state, "running");
    let child_pid: u32 = fs::read_to_string(root.join("child.pid"))
        .expect("child pid")
        .trim()
        .parse()
        .expect("pid");
    assert!(alive(child_pid));
    roundtrip(
        &engine.dir,
        EngineCommand::StopProject {
            project_id: "with-child".to_string(),
        },
    )
    .await
    .expect("stop");
    assert!(!alive(child_pid), "child process was still running");
    assert!(running.services[0].port.is_some());
    let port = running.services[0].port.expect("port");
    assert!(TcpStream::connect(("127.0.0.1", port)).is_err());

    let fail_root = engine.dir.join("fail proj");
    write_service(
        &fail_root,
        "fails",
        r#"
[[service]]
id = "web"
runtime = "node"
command = ["node", "fail"]
ready_timeout_secs = 3
"#,
    );
    import(&engine.dir, &fail_root).await;
    select(&engine.dir, "fails", "node").await;
    start(&engine.dir, "fails").await;
    let failed = settled(&engine.dir, "fails").await;
    assert_eq!(failed.services[0].state, "failed");
    assert!(
        failed.services[0]
            .detail
            .contains("startup failed: missing table"),
        "{}",
        failed.services[0].detail
    );
}

#[tokio::test]
async fn partial_start_rolls_back_and_cancel_releases_the_process() {
    let engine = Engine::start();
    engine.ready().await;
    adopt(&engine.dir, "node").await;
    let root = engine.dir.join("partial");
    write_service(
        &root,
        "partial",
        r#"
[[service]]
id = "web"
runtime = "node"
command = ["node", "serve"]
ready_timeout_secs = 5

[[service]]
id = "worker"
runtime = "node"
command = ["node", "fail"]
depends_on = ["web"]
ready_timeout_secs = 3
"#,
    );
    import(&engine.dir, &root).await;
    select(&engine.dir, "partial", "node").await;
    start(&engine.dir, "partial").await;
    let project = settled(&engine.dir, "partial").await;
    let web = project
        .services
        .iter()
        .find(|service| service.id == "web")
        .expect("web");
    let worker = project
        .services
        .iter()
        .find(|service| service.id == "worker")
        .expect("worker");
    assert_eq!(web.state, "stopped", "{web:?}");
    assert_eq!(worker.state, "failed", "{worker:?}");
    assert!(
        worker.detail.contains("startup failed: missing table"),
        "{}",
        worker.detail
    );

    let slow = engine.dir.join("slow");
    write_service(
        &slow,
        "slow",
        r#"
[[service]]
id = "web"
runtime = "node"
command = ["node", "sleep"]
ready_timeout_secs = 20
"#,
    );
    import(&engine.dir, &slow).await;
    select(&engine.dir, "slow", "node").await;
    start(&engine.dir, "slow").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    roundtrip(
        &engine.dir,
        EngineCommand::CancelStart {
            project_id: "slow".to_string(),
        },
    )
    .await
    .expect("cancel");
    let cancelled = settled(&engine.dir, "slow").await;
    assert_eq!(cancelled.services[0].state, "stopped", "{cancelled:?}");
    assert!(
        cancelled.services[0].detail.contains("cancelled"),
        "{}",
        cancelled.services[0].detail
    );
}

#[tokio::test]
async fn occupied_port_restart_limit_and_exec() {
    let engine = Engine::start();
    engine.ready().await;
    adopt(&engine.dir, "node").await;
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
    let port = listener.local_addr().expect("addr").port();
    let root = engine.dir.join("busy");
    write_service(
        &root,
        "busy",
        &format!(
            r#"
[[service]]
id = "web"
runtime = "node"
command = ["node", "serve"]
port = {port}
ready_timeout_secs = 3
"#
        ),
    );
    import(&engine.dir, &root).await;
    select(&engine.dir, "busy", "node").await;
    start(&engine.dir, "busy").await;
    let busy = settled(&engine.dir, "busy").await;
    assert_eq!(busy.services[0].state, "failed", "{busy:?}");
    assert!(
        busy.services[0].detail.contains(&port.to_string()),
        "{}",
        busy.services[0].detail
    );
    assert!(listener.local_addr().is_ok());

    let crash_root = engine.dir.join("crash");
    write_service(
        &crash_root,
        "crasher",
        r#"
[[service]]
id = "web"
runtime = "node"
command = ["node", "crash"]
restart = "on-failure"
restart_max = 2
ready_timeout_secs = 2
"#,
    );
    import(&engine.dir, &crash_root).await;
    select(&engine.dir, "crasher", "node").await;
    start(&engine.dir, "crasher").await;
    let crashed = settled(&engine.dir, "crasher").await;
    assert_eq!(crashed.services[0].state, "failed", "{crashed:?}");
    let runs = fs::read_to_string(crash_root.join("runs.txt")).unwrap_or_default();
    assert_eq!(runs.lines().count(), 3, "{runs}");

    let exec_root = engine.dir.join("exec");
    write_service(&exec_root, "exec-proj", SERVE);
    import(&engine.dir, &exec_root).await;
    select(&engine.dir, "exec-proj", "node").await;
    let started = roundtrip(
        &engine.dir,
        EngineCommand::Exec {
            project_id: "exec-proj".to_string(),
            service_id: "web".to_string(),
            args: vec!["node".to_string(), "echo".to_string()],
        },
    )
    .await
    .expect("exec");
    let Outcome::Task { id, .. } = started else {
        panic!("expected task");
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        let status = roundtrip(
            &engine.dir,
            EngineCommand::DepStatus {
                operation_id: id.clone(),
            },
        )
        .await
        .expect("task");
        let Outcome::Task { state, message, .. } = status else {
            panic!("task status");
        };
        if state == "succeeded" {
            assert!(message.contains("service-ok"), "{message}");
            break;
        }
        if state != "running" || tokio::time::Instant::now() > deadline {
            panic!("exec {state}: {message}");
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

#[tokio::test]
async fn dependency_cancel_and_failure_are_reported() {
    let engine = Engine::start();
    engine.ready().await;
    adopt(&engine.dir, "python").await;
    let root = engine.dir.join("deps");
    write_service(
        &root,
        "deps-proj",
        r#"
schema = 1
name = "ignored"
manager = "pip"
[[service]]
id = "web"
runtime = "python"
command = ["python", "serve"]
"#,
    );
    // write_service already wrote schema and name; replace it with the manager form.
    fs::write(
        root.join("stackyard.toml"),
        r#"
schema = 1
name = "deps-proj"
manager = "pip"
[[service]]
id = "web"
runtime = "python"
command = ["python", "serve"]
ready_timeout_secs = 5
"#,
    )
    .expect("toml");
    fs::write(root.join("requirements.txt"), "stackyard-marker\n").expect("requirements");
    import(&engine.dir, &root).await;
    select(&engine.dir, "deps-proj", "python").await;
    fs::write(root.join("pip-slow"), "1").expect("slow");
    let started = roundtrip(
        &engine.dir,
        EngineCommand::DepInstall {
            project_id: "deps-proj".to_string(),
            scope: String::new(),
        },
    )
    .await
    .expect("dep");
    let Outcome::Task { id, .. } = started else {
        panic!("task");
    };
    tokio::time::sleep(Duration::from_millis(200)).await;
    roundtrip(
        &engine.dir,
        EngineCommand::DepCancel {
            operation_id: id.clone(),
        },
    )
    .await
    .expect("cancel");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        let status = roundtrip(
            &engine.dir,
            EngineCommand::DepStatus {
                operation_id: id.clone(),
            },
        )
        .await
        .expect("status");
        let Outcome::Task { state, message, .. } = status else {
            panic!("status");
        };
        if state == "cancelled" {
            assert!(message.contains("cancelled") || message.is_empty() || state == "cancelled");
            break;
        }
        if state != "running" || tokio::time::Instant::now() > deadline {
            panic!("dependency {state}: {message}");
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }

    fs::remove_file(root.join("pip-slow")).expect("remove slow");
    fs::write(root.join("pip-fail"), "1").expect("fail");
    let failed = roundtrip(
        &engine.dir,
        EngineCommand::DepInstall {
            project_id: "deps-proj".to_string(),
            scope: String::new(),
        },
    )
    .await
    .expect("fail start");
    let Outcome::Task { id, .. } = failed else {
        panic!("task");
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        let status = roundtrip(
            &engine.dir,
            EngineCommand::DepStatus {
                operation_id: id.clone(),
            },
        )
        .await
        .expect("status");
        let Outcome::Task { state, message, .. } = status else {
            panic!("status");
        };
        if state == "failed" {
            assert!(message.contains("pip failed: no distribution"), "{message}");
            assert!(message.contains("partial"), "{message}");
            break;
        }
        if state != "running" || tokio::time::Instant::now() > deadline {
            panic!("expected failure, got {state}: {message}");
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

#[tokio::test]
async fn crash_does_not_duplicate_or_kill_an_unrelated_process() {
    let dir = temp_dir();
    let mut engine = Command::new(engine_bin())
        .arg("--data-dir")
        .arg(&dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("engine");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while roundtrip(&dir, EngineCommand::Status).await.is_err() {
        if std::time::Instant::now() > deadline {
            panic!("subprocess engine did not start");
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    roundtrip(
        &dir,
        EngineCommand::UseExisting {
            role: "node".to_string(),
            path: fixture().display().to_string(),
        },
    )
    .await
    .expect("adopt");
    let root = dir.join("crash me");
    write_service(&root, "held", SERVE);
    roundtrip(
        &dir,
        EngineCommand::ImportProject {
            path: root.display().to_string(),
        },
    )
    .await
    .expect("import");
    roundtrip(
        &dir,
        EngineCommand::SelectPackage {
            project_id: "held".to_string(),
            package_id: "existing-node".to_string(),
        },
    )
    .await
    .expect("select");
    roundtrip(
        &dir,
        EngineCommand::StartProject {
            project_id: "held".to_string(),
        },
    )
    .await
    .expect("start");
    let running = settled(&dir, "held").await;
    let pid = running.services[0].pid.expect("pid");
    let port = running.services[0].port.expect("port");
    assert!(http_body(port).contains("one"));
    let mut unrelated = Command::new(fixture())
        .arg("sleep")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("unrelated");
    engine.kill().expect("kill engine");
    let _ = engine.wait();
    assert!(alive(unrelated.id()), "unrelated process was killed");
    #[cfg(unix)]
    {
        assert!(
            alive(pid),
            "managed process should remain after an engine crash"
        );
        assert!(http_body(port).contains("one"));
    }
    let mut restarted = Command::new(engine_bin())
        .arg("--data-dir")
        .arg(&dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("restart");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while roundtrip(&dir, EngineCommand::Status).await.is_err() {
        if std::time::Instant::now() > deadline {
            panic!("restarted engine did not start");
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    let after = settled(&dir, "held").await;
    #[cfg(unix)]
    {
        assert_eq!(after.services[0].state, "orphaned", "{after:?}");
        assert_eq!(after.services[0].pid, Some(pid));
        let duplicate = roundtrip(
            &dir,
            EngineCommand::StartProject {
                project_id: "held".to_string(),
            },
        )
        .await;
        assert!(duplicate.is_err(), "restart started a second process");
        roundtrip(
            &dir,
            EngineCommand::StopProject {
                project_id: "held".to_string(),
            },
        )
        .await
        .expect("stop orphan");
        assert!(!alive(pid));
        assert!(TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            Duration::from_millis(200)
        )
        .is_err());
    }
    #[cfg(windows)]
    {
        assert_ne!(after.services[0].state, "running");
        let _ = pid;
        let _ = port;
    }
    assert!(
        alive(unrelated.id()),
        "unrelated process died during recovery"
    );
    let _ = unrelated.kill();
    let _ = unrelated.wait();
    let _ = roundtrip(&dir, EngineCommand::Stop).await;
    let _ = restarted.wait();
    let _ = fs::remove_dir_all(&dir);
}

fn node_with_npm() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let node = dir.join(if cfg!(windows) { "node.exe" } else { "node" });
        if !node.is_file() {
            continue;
        }
        let sibling = dir.join(if cfg!(windows) { "npm.cmd" } else { "npm" });
        let cli = dir.join("../lib/node_modules/npm/bin/npm-cli.js");
        if sibling.is_file() || cli.is_file() {
            return Some(node);
        }
    }
    None
}

#[test]
fn fixture_pip_version_does_not_install_into_the_working_directory() {
    let root = temp_dir();
    let output = Command::new(fixture())
        .args(["-m", "pip", "--version"])
        .current_dir(&root)
        .output()
        .expect("fixture pip version");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "pip 1.0.0");
    assert!(!root.join("pip-installed").exists());
    fs::remove_dir_all(root).expect("remove version fixture");
}

#[tokio::test]
async fn npm_and_pip_keep_project_environments_separate() {
    let Some(node) = node_with_npm() else {
        eprintln!("skipping npm install; node+npm were not found");
        return;
    };
    let python = which("python3").or_else(|| which("python"));
    let engine = Engine::start();
    engine.ready().await;
    roundtrip(
        &engine.dir,
        EngineCommand::UseExisting {
            role: "node".into(),
            path: node.display().to_string(),
        },
    )
    .await
    .expect("node");
    let root = engine.dir.join("npm-proj");
    fs::create_dir_all(root.join("vendor/hello")).expect("vendor");
    fs::write(
        root.join("vendor/hello/package.json"),
        r#"{"name":"stackyard-hello","version":"1.0.0","main":"index.js"}"#,
    )
    .expect("pkg");
    fs::write(
        root.join("vendor/hello/index.js"),
        "module.exports = 'hello';\n",
    )
    .expect("js");
    fs::write(
        root.join("package.json"),
        r#"{"name":"stackyard-node-http","private":true,"dependencies":{"stackyard-hello":"file:vendor/hello"}}"#,
    )
    .expect("package");
    // Pack this local dependency instead of requiring Windows symlink privilege.
    // Keep the npm policy private to this disposable project.
    fs::write(root.join(".npmrc"), "install-links=true\n").expect("npm copy install policy");
    fs::write(
        root.join("server.js"),
        "require('http').createServer((req, res) => res.end('node')).listen(process.env.PORT, '127.0.0.1');\n",
    )
    .expect("server");
    fs::write(
        root.join("stackyard.toml"),
        r#"
schema = 1
name = "npm-app"
manager = "npm"
[[service]]
id = "web"
runtime = "node"
command = ["node", "server.js"]
ready_timeout_secs = 8
"#,
    )
    .expect("toml");
    import(&engine.dir, &root).await;
    select(&engine.dir, "npm-app", "node").await;
    let install = roundtrip(
        &engine.dir,
        EngineCommand::DepInstall {
            project_id: "npm-app".into(),
            scope: String::new(),
        },
    )
    .await
    .expect("npm");
    let message = wait_task(&engine.dir, install).await;
    assert!(message.contains("manager: npm"), "{message}");
    assert!(root.join("node_modules/stackyard-hello").exists());
    assert!(
        !fs::symlink_metadata(root.join("node_modules/stackyard-hello"))
            .expect("installed package metadata")
            .file_type()
            .is_symlink()
    );

    let Some(python) = python else {
        return;
    };
    if Command::new(&python)
        .args(["-m", "venv", "--help"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()
        .filter(|status| status.success())
        .is_none()
    {
        eprintln!("skipping pip; venv is unavailable");
        return;
    }
    roundtrip(
        &engine.dir,
        EngineCommand::UseExisting {
            role: "python".into(),
            path: python.display().to_string(),
        },
    )
    .await
    .expect("python");
    for (id, folder) in [("pip-a", "py-a"), ("pip-b", "py-b")] {
        let project = engine.dir.join(folder);
        fs::create_dir_all(&project).expect("py");
        fs::write(
            project.join("stackyard.toml"),
            format!(
                "schema = 1\nname = \"{id}\"\nmanager = \"pip\"\n[[service]]\nid = \"web\"\nruntime = \"python\"\ncommand = [\"python\", \"-c\", \"print(1)\"]\n"
            ),
        )
        .expect("toml");
        fs::write(project.join("requirements.txt"), "stackyard-marker\n").expect("req");
        import(&engine.dir, &project).await;
        select(&engine.dir, id, "python").await;
    }
    // The fake package name will fail against PyPI. Use a path requirement instead.
    let marker = engine.dir.join("marker");
    fs::create_dir_all(&marker).expect("marker");
    let wheel = write_wheel(&python, &marker);
    fs::write(
        engine.dir.join("py-a/requirements.txt"),
        format!("{}\n", wheel.display()),
    )
    .expect("req a");
    fs::write(engine.dir.join("py-b/requirements.txt"), "\n").expect("req b");
    let installed = roundtrip(
        &engine.dir,
        EngineCommand::DepInstall {
            project_id: "pip-a".into(),
            scope: String::new(),
        },
    )
    .await
    .expect("pip");
    let message = wait_task(&engine.dir, installed).await;
    assert!(message.contains("manager: pip"), "{message}");
    let venv_python = if cfg!(windows) {
        engine.dir.join("environments/pip-a/Scripts/python.exe")
    } else {
        engine.dir.join("environments/pip-a/bin/python")
    };
    let other = if cfg!(windows) {
        engine.dir.join("environments/pip-b/Scripts/python.exe")
    } else {
        engine.dir.join("environments/pip-b/bin/python")
    };
    let check = Command::new(&venv_python)
        .args([
            "-c",
            "import stackyard_marker; print(stackyard_marker.VALUE)",
        ])
        .output()
        .expect("import");
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    assert!(String::from_utf8_lossy(&check.stdout).contains("isolated"));
    let system = Command::new(&python)
        .args(["-c", "import stackyard_marker"])
        .output()
        .expect("system");
    assert!(
        !system.status.success(),
        "system python received the project package"
    );
    if other.is_file() {
        let second = Command::new(&other)
            .args(["-c", "import stackyard_marker"])
            .output()
            .expect("other");
        assert!(!second.status.success());
    }
}

async fn wait_task(dir: &Path, started: Outcome) -> String {
    let Outcome::Task { id, .. } = started else {
        panic!("expected task, got {started:?}");
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    loop {
        let status = roundtrip(
            dir,
            EngineCommand::DepStatus {
                operation_id: id.clone(),
            },
        )
        .await
        .expect("task");
        let Outcome::Task { state, message, .. } = status else {
            panic!("task");
        };
        if state == "succeeded" {
            return message;
        }
        if state != "running" || tokio::time::Instant::now() > deadline {
            panic!("task {state}: {message}");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn write_wheel(python: &Path, dir: &Path) -> PathBuf {
    let script = r#"
import base64, hashlib, pathlib, sys, zipfile
root = pathlib.Path(sys.argv[1])
payload = b"VALUE = 'isolated'\n"
dist = "stackyard_marker-1.0.0.dist-info"
files = {
    "stackyard_marker/__init__.py": payload,
    f"{dist}/METADATA": b"Metadata-Version: 2.1\nName: stackyard-marker\nVersion: 1.0.0\n",
    f"{dist}/WHEEL": b"Wheel-Version: 1.0\nGenerator: stackyard\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
}
lines = []
for name, data in files.items():
    digest = base64.urlsafe_b64encode(hashlib.sha256(data).digest()).decode().rstrip("=")
    lines.append(f"{name},sha256={digest},{len(data)}")
lines.append(f"{dist}/RECORD,,")
files[f"{dist}/RECORD"] = ("\n".join(lines) + "\n").encode()
wheel = root / "stackyard_marker-1.0.0-py3-none-any.whl"
with zipfile.ZipFile(wheel, "w") as archive:
    for name, data in files.items():
        archive.writestr(name, data)
print(wheel)
"#;
    let output = Command::new(python)
        .arg("-c")
        .arg(script)
        .arg(dir)
        .output()
        .expect("wheel");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    PathBuf::from(String::from_utf8_lossy(&output.stdout).trim())
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

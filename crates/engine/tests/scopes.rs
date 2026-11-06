//! Disposable scope acceptance. Native gates require previously verified artifacts.
use stackyard_core::scopes::*;
use stackyard_core::{DepAction, DepRequest, PackageRecord, RuntimeRole, Store, Toolchain};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Fixture {
    data: PathBuf,
    root: PathBuf,
}
impl Fixture {
    fn new(manifest: &str) -> Self {
        let data =
            stackyard_core::prepare_data_dir(&stackyard_core::disposable_dir("scopes")).unwrap();
        let root = data.join("project");
        for dir in ["web", "api", "jobs"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::write(root.join("stackyard.toml"), manifest).unwrap();
        Self { data, root }
    }
    fn import(&self) {
        stackyard_core::import_project(&self.data, &self.root).unwrap();
    }
    fn package(&self, id: &str, role: &str, launch: &Path) {
        Store::open(&self.data)
            .unwrap()
            .insert_package(&PackageRecord {
                id: id.into(),
                kind: if matches!(role, "node" | "python") {
                    "runtime"
                } else {
                    "tool"
                }
                .into(),
                name: role.into(),
                version: "native".into(),
                target: stackyard_core::current_target().into(),
                source: "existing".into(),
                path: launch.parent().unwrap().display().to_string(),
                launch: launch.display().to_string(),
                digest: None,
                removable: false,
            })
            .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.data);
    }
}
const MIXED: &str = r#"schema = 2
name = "mixed"
[[scope]]
id = "frontend"
directory = "web"
runtime = "node"
manager = "pnpm"
lockfile_root = "web"
[[scope]]
id = "backend"
directory = "api"
runtime = "python"
manager = "pip"
lockfile_root = "api"
environment = "managed"
[[service]]
id = "web"
scope = "frontend"
working_directory = "web"
command = ["node", "worker.js"]
readiness = {kind = "process", grace_ms = 100}
[[service]]
id = "api"
scope = "backend"
working_directory = "api"
command = ["python", "worker.py"]
readiness = {kind = "process", grace_ms = 100}
"#;

#[test]
fn scoped_selection_validation_and_view_are_read_only() {
    let f = Fixture::new(MIXED);
    f.import();
    let exe = PathBuf::from(env!("CARGO_BIN_EXE_stackyard-fixture"));
    f.package("node-a", "node", &exe);
    f.package("node-b", "node", &exe);
    f.package("python-a", "python", &exe);
    let store = Store::open(&f.data).unwrap();
    store
        .select_package(stackyard_core::DEFAULT_PROJECT, "node", "node-a")
        .unwrap();
    assert_eq!(
        resolve_scope_runtime(&f.data, "mixed", "frontend", RuntimeRole::Node)
            .unwrap()
            .id,
        "node-a"
    );
    assert!(store.scope_selections("mixed").unwrap().is_empty());
    select_for_scope(&f.data, "mixed", "frontend", "node-b").unwrap();
    assert_eq!(
        resolve_scope_runtime(&f.data, "mixed", "frontend", RuntimeRole::Node)
            .unwrap()
            .id,
        "node-b"
    );
    assert!(select_for_scope(&f.data, "mixed", "frontend", "python-a").is_err());
    assert!(select_for_scope(&f.data, "mixed", "missing", "node-a").is_err());
    assert!(resolve_scope_tool(&f.data, "mixed", "backend", "pnpm").is_err());
    let view = stackyard_core::project_view(&f.data, "mixed").unwrap();
    assert_eq!(view.scopes.len(), 2);
    assert_eq!(view.services[0].scope_id, "frontend");
    assert!(view
        .services
        .iter()
        .all(|service| service.endpoint.is_none()));
    assert!(!f.data.join("environments").exists());
    assert!(store
        .get_scope_environment("mixed", "backend")
        .unwrap()
        .is_none());
}

#[test]
fn canonical_ownership_rejects_escape_and_nested_lockfile() {
    let f = Fixture::new(MIXED);
    let manifest = stackyard_core::parse_manifest(MIXED).unwrap();
    assert!(resolve_dependency_scope(&f.root, &manifest, Some("missing"), ".").is_err());
    assert!(canonical_scope_dir(&f.root, Path::new("../outside")).is_err());
    assert!(resolve_dependency_scope(&f.root, &manifest, None, ".").is_err());
    assert_eq!(
        resolve_dependency_scope(&f.root, &manifest, None, "web")
            .unwrap()
            .spec
            .id,
        "frontend"
    );
    let mut nested = manifest.clone();
    nested.scopes[0].directory = "web/child".into();
    fs::create_dir_all(f.root.join("web/child")).unwrap();
    validate_scope_ownership(&f.root, &nested).unwrap();
    fs::write(
        f.root.join("web/child/pnpm-lock.yaml"),
        "lockfileVersion: '9.0'",
    )
    .unwrap();
    assert!(validate_scope_ownership(&f.root, &nested).is_err());
    fs::write(f.root.join("web/package-lock.json"), "{}").unwrap();
    assert!(validate_scope_ownership(&f.root, &manifest).is_err());
}

#[test]
fn workers_preserve_explicit_environment_and_missing_tools_fail() {
    let f = Fixture::new(MIXED);
    f.import();
    let exe = PathBuf::from(env!("CARGO_BIN_EXE_stackyard-fixture"));
    f.package("node", "node", &exe);
    select_for_scope(&f.data, "mixed", "frontend", "node").unwrap();
    let (project, manifest) = stackyard_core::load_project_manifest(&f.data, "mixed").unwrap();
    let mut service = manifest.services[0].clone();
    service.env.push(("PORT".into(), "explicit".into()));
    let launch =
        stackyard_core::service_launch(&f.data, &project, &service, 0, &service.command).unwrap();
    assert!(!launch.env_remove.contains(&"PORT".into()));
    assert!(launch.env_remove.contains(&"FLASK_RUN_PORT".into()));
    assert_eq!(
        launch.env.iter().find(|(key, _)| key == "PORT").unwrap().1,
        "explicit"
    );
    let missing = stackyard_core::plan_dependency(
        &f.data,
        "mixed",
        &f.root,
        &manifest,
        DepRequest {
            action: DepAction::Install,
            package: None,
            scope: ".",
            scope_id: Some("frontend"),
        },
        Toolchain {
            node: Some(&exe),
            python: None,
            pnpm: None,
            uv: None,
            venv_python: None,
            php: None,
            composer: None,
        },
    );
    assert!(missing.unwrap_err().to_string().contains("pnpm"));
    assert!(!f.data.join("environments").exists());
}

#[cfg(windows)]
#[test]
fn windows_junction_cannot_escape_declared_scope() {
    let f = Fixture::new(MIXED);
    let outside = f.data.join("outside");
    fs::create_dir_all(&outside).unwrap();
    let link = f.root.join("outside-link");
    let script = format!(
        "New-Item -ItemType Junction -Path '{}' -Target '{}' | Out-Null",
        link.display().to_string().replace('\'', "''"),
        outside.display().to_string().replace('\'', "''")
    );
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(canonical_scope_dir(&f.root, Path::new("outside-link")).is_err());
    let mut manifest = stackyard_core::parse_manifest(MIXED).unwrap();
    manifest.scopes[0].directory = "outside-link".into();
    manifest.scopes[0].lockfile_root = "outside-link".into();
    assert!(validate_scope_ownership(&f.root, &manifest).is_err());
}

fn native(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_file())
}
struct RunningEngine {
    data: PathBuf,
    thread: Option<std::thread::JoinHandle<stackyard_core::Result<()>>>,
}
impl RunningEngine {
    async fn start(data: &Path) -> Self {
        let owned = data.to_path_buf();
        let thread = std::thread::spawn(move || stackyard_engine::serve(&owned));
        let engine = Self {
            data: data.into(),
            thread: Some(thread),
        };
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        while stackyard_core::roundtrip(data, stackyard_core::Command::Status)
            .await
            .is_err()
        {
            assert!(
                tokio::time::Instant::now() < deadline,
                "engine did not become ready"
            );
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        }
        engine
    }
}
impl Drop for RunningEngine {
    fn drop(&mut self) {
        let data = self.data.clone();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let _ = runtime.block_on(stackyard_core::roundtrip(
                &data,
                stackyard_core::Command::Stop,
            ));
        })
        .join()
        .unwrap();
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap().unwrap();
        }
    }
}
async fn install_scope(data: &Path, scope: &str) {
    let outcome = stackyard_core::roundtrip(
        data,
        stackyard_core::Command::DepInstallScoped {
            project_id: "mixed".into(),
            scope_id: scope.into(),
        },
    )
    .await
    .unwrap();
    let stackyard_core::Outcome::Task { id, .. } = outcome else {
        panic!("expected dependency task")
    };
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let status = stackyard_core::roundtrip(
            data,
            stackyard_core::Command::DepStatus {
                operation_id: id.clone(),
            },
        )
        .await
        .unwrap();
        let stackyard_core::Outcome::Task { state, message, .. } = status else {
            panic!("expected task status")
        };
        if state != "running" {
            assert_eq!(state, "succeeded", "{scope}: {message}");
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "{scope} dependency install timed out: {message}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
}

fn scope_http(port: u16) -> String {
    use std::io::{Read, Write};
    let address = ([127, 0, 0, 1], port).into();
    let mut stream =
        std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_secs(2)).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    stream
        .write_all(b"GET / HTTP/1.0\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut response = String::new();
    stream.take(4096).read_to_string(&mut response).unwrap();
    assert_eq!(
        response.split_whitespace().nth(1),
        Some("200"),
        "{response}"
    );
    response.split_once("\r\n\r\n").unwrap().1.to_string()
}
async fn running_scopes(data: &Path) -> stackyard_core::ProjectView {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let outcome = stackyard_core::roundtrip(
            data,
            stackyard_core::Command::ProjectStatus {
                project_id: "mixed".into(),
            },
        )
        .await
        .unwrap();
        let stackyard_core::Outcome::Project { project } = outcome else {
            panic!("expected project")
        };
        if project
            .services
            .iter()
            .all(|service| service.state == "running" && service.readiness_state == "ready")
        {
            return project;
        }
        assert!(
            project
                .services
                .iter()
                .all(|service| !matches!(service.state.as_str(), "failed" | "crashed")),
            "scope launch failed: {project:?}"
        );
        assert!(
            tokio::time::Instant::now() < deadline,
            "scopes did not become ready: {project:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
}

#[tokio::test]
async fn native_pnpm_pip_scopes_install_and_launch_independently() {
    let (Some(node), Some(pnpm), Some(python)) = (
        native("STACKYARD_TEST_SCOPE_NODE"),
        native("STACKYARD_TEST_SCOPE_PNPM"),
        native("STACKYARD_TEST_SCOPE_PYTHON"),
    ) else {
        eprintln!("native scope gate skipped: set STACKYARD_TEST_SCOPE_NODE/PNPM/PYTHON");
        return;
    };
    let f = Fixture::new(MIXED);
    f.import();
    fs::write(
        f.root.join("web/package.json"),
        r#"{"name":"scope-web","version":"1.0.0","private":true}"#,
    )
    .unwrap();
    fs::create_dir_all(f.root.join("web/dependency")).unwrap();
    fs::write(
        f.root.join("web/dependency/package.json"),
        r#"{"name":"scope-local-node","version":"1.0.0","main":"index.js"}"#,
    )
    .unwrap();
    fs::write(
        f.root.join("web/dependency/index.js"),
        "module.exports = 'scope-node-ok';",
    )
    .unwrap();
    fs::write(f.root.join("web/package.json"), r#"{"name":"scope-web","version":"1.0.0","private":true,"dependencies":{"scope-local-node":"file:./dependency"}}"#).unwrap();
    let wheel = f.root.join("api/scope_local_python-1.0.0-py3-none-any.whl");
    let out = Command::new(&python).args(["-c", r#"import sys,zipfile; z=zipfile.ZipFile(sys.argv[1],'w'); z.writestr('scope_local_python.py', 'VALUE = "scope-python-ok"\n'); z.writestr('scope_local_python-1.0.0.dist-info/METADATA','Metadata-Version: 2.1\nName: scope-local-python\nVersion: 1.0.0\n'); z.writestr('scope_local_python-1.0.0.dist-info/WHEEL','Wheel-Version: 1.0\nGenerator: scope-test\nRoot-Is-Purelib: true\nTag: py3-none-any\n'); z.writestr('scope_local_python-1.0.0.dist-info/RECORD',''); z.close()"#])
        .arg(&wheel).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    fs::write(
        f.root.join("api/requirements.txt"),
        format!("{}\n", wheel.display()),
    )
    .unwrap();
    fs::write(
        f.root.join("web/worker.js"),
        "console.log(require('scope-local-node')); console.log(process.env.PORT || 'portless');",
    )
    .unwrap();
    fs::write(
        f.root.join("api/worker.py"),
        "import sys, scope_local_python; print(sys.prefix); print(scope_local_python.VALUE)",
    )
    .unwrap();
    f.package("node", "node", &node);
    f.package("pnpm", "pnpm", &pnpm);
    f.package("python", "python", &python);
    for (scope, id) in [
        ("frontend", "node"),
        ("frontend", "pnpm"),
        ("backend", "python"),
    ] {
        select_for_scope(&f.data, "mixed", scope, id).unwrap();
    }
    let (project, manifest) = stackyard_core::load_project_manifest(&f.data, "mixed").unwrap();
    let _engine = RunningEngine::start(&f.data).await;
    let api_before = stackyard_core::manifest_fingerprint(&f.root.join("api"));
    install_scope(&f.data, "frontend").await;
    assert_eq!(
        api_before,
        stackyard_core::manifest_fingerprint(&f.root.join("api"))
    );
    let web_before = stackyard_core::manifest_fingerprint(&f.root.join("web"));
    assert!(!f.data.join("environments").exists());
    install_scope(&f.data, "backend").await;
    assert_eq!(
        web_before,
        stackyard_core::manifest_fingerprint(&f.root.join("web"))
    );
    let backend_env = scope_environment_dir(&f.data, "mixed", "backend", 2);
    assert!(stackyard_core::venv_python(&backend_env).is_file());
    assert!(!f.root.join("api/.venv").exists());
    for service in &manifest.services {
        let launch =
            stackyard_core::service_launch(&f.data, &project, service, 0, &service.command)
                .unwrap();
        let mut command = Command::new(&launch.program);
        command
            .args(&launch.args)
            .current_dir(&launch.working_directory)
            .env("PORT", "inherited")
            .env("FLASK_RUN_PORT", "inherited");
        for key in &launch.env_remove {
            command.env_remove(key);
        }
        let out = command.envs(launch.env).output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8_lossy(&out.stdout);
        if service.runtime == RuntimeRole::Node {
            assert!(text.contains("portless"));
            assert!(text.contains("scope-node-ok"));
        } else {
            assert!(text.contains("backend"));
            assert!(text.contains("scope-python-ok"));
        }
    }
    drop(Store::open(&f.data).unwrap());
    assert_eq!(
        Store::open(&f.data)
            .unwrap()
            .get_scope_environment("mixed", "backend")
            .unwrap()
            .unwrap(),
        backend_env.display().to_string()
    );
    assert_eq!(
        resolve_scope_runtime(&f.data, "mixed", "frontend", RuntimeRole::Node)
            .unwrap()
            .id,
        "node"
    );
    // Run the dependency-backed applications through the real supervisor,
    // retaining the earlier short-lived import and portless-env checks.
    let http_manifest = MIXED.replace(
        "readiness = {kind = \"process\", grace_ms = 100}",
        "endpoint = {protocol = \"http\", port = \"auto\"}\nreadiness = {kind = \"http\", path = \"/\", statuses = [200], timeout_secs = 15}",
    );
    fs::write(f.root.join("stackyard.toml"), http_manifest).unwrap();
    fs::write(
        f.root.join("web/worker.js"),
        r#"const http = require('node:http');
const payload = require('scope-local-node');
http.createServer((req, res) => {
  res.writeHead(200, {'Content-Type':'text/plain', 'Content-Length':Buffer.byteLength(payload)});
  res.end(payload);
}).listen(Number(process.env.PORT), '127.0.0.1');
"#,
    )
    .unwrap();
    fs::write(f.root.join("api/persistent.txt"), "backend-data-preserved").unwrap();
    fs::write(
        f.root.join("api/worker.py"),
        r#"import os
from pathlib import Path
from http.server import BaseHTTPRequestHandler, HTTPServer
import scope_local_python
class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        payload = (scope_local_python.VALUE + ':' + Path('persistent.txt').read_text()).encode()
        self.send_response(200)
        self.send_header('Content-Length', str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)
HTTPServer(('127.0.0.1', int(os.environ['PORT'])), Handler).serve_forever()
"#,
    )
    .unwrap();
    stackyard_core::roundtrip(
        &f.data,
        stackyard_core::Command::StartProject {
            project_id: "mixed".into(),
        },
    )
    .await
    .unwrap();
    let running = running_scopes(&f.data).await;
    let web = running
        .services
        .iter()
        .find(|service| service.id == "web")
        .unwrap();
    let api = running
        .services
        .iter()
        .find(|service| service.id == "api")
        .unwrap();
    let web_port = web.port.unwrap();
    let api_port = api.port.unwrap();
    let api_pid = api.pid.unwrap();
    assert_ne!(web_port, api_port);
    assert_eq!(scope_http(web_port), "scope-node-ok");
    assert_eq!(
        scope_http(api_port),
        "scope-python-ok:backend-data-preserved"
    );
    let backend_fingerprint = stackyard_core::manifest_fingerprint(&f.root.join("api"));
    install_scope(&f.data, "frontend").await;
    let after = running_scopes(&f.data).await;
    let api_after = after
        .services
        .iter()
        .find(|service| service.id == "api")
        .unwrap();
    assert_eq!(api_after.pid, Some(api_pid));
    assert_eq!(api_after.port, Some(api_port));
    assert_eq!(
        backend_fingerprint,
        stackyard_core::manifest_fingerprint(&f.root.join("api"))
    );
    assert_eq!(
        fs::read_to_string(f.root.join("api/persistent.txt")).unwrap(),
        "backend-data-preserved"
    );
    assert_eq!(
        scope_http(api_port),
        "scope-python-ok:backend-data-preserved"
    );
    assert_eq!(scope_http(web_port), "scope-node-ok");
    stackyard_core::roundtrip(
        &f.data,
        stackyard_core::Command::StopProject {
            project_id: "mixed".into(),
        },
    )
    .await
    .unwrap();
    let closed_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let closed = [web_port, api_port].into_iter().all(|port| {
            std::net::TcpStream::connect_timeout(
                &([127, 0, 0, 1], port).into(),
                std::time::Duration::from_millis(100),
            )
            .is_err()
        });
        if closed {
            break;
        }
        assert!(
            tokio::time::Instant::now() < closed_deadline,
            "stopped scope ports remained open"
        );
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
    // The native pnpm operation has produced its real lockfile. A second
    // manager lockfile must fail before any dependency command can run.
    assert!(f.root.join("web/pnpm-lock.yaml").is_file());
    fs::write(f.root.join("web/package-lock.json"), "{}").unwrap();
    let error = resolve_dependency_scope(&f.root, &manifest, Some("frontend"), ".").unwrap_err();
    assert!(error.to_string().contains("manager_conflict"));
}

#[test]
fn native_legacy_venv_reopen_keeps_original_identity() {
    let Some(python) = native("STACKYARD_TEST_SCOPE_PYTHON") else {
        return;
    };
    let f = Fixture::new("schema=1\nname='legacy'\nmanager='pip'\n[[service]]\nid='api'\nruntime='python'\ncommand=['python','worker.py']\n");
    f.import();
    f.package("python", "python", &python);
    Store::open(&f.data)
        .unwrap()
        .select_package("legacy", "python", "python")
        .unwrap();
    let original = stackyard_core::environment_dir(&f.data, "legacy");
    stackyard_core::ensure_venv(&python, &original).unwrap();
    fs::write(original.join("identity.txt"), "preserved").unwrap();
    let (_, manifest) = stackyard_core::load_project_manifest(&f.data, "legacy").unwrap();
    assert_eq!(
        resolved_environment(&f.data, "legacy", &f.root, &manifest, &manifest.scopes[0]).unwrap(),
        original
    );
    stackyard_core::plan_dependency(
        &f.data,
        "legacy",
        &f.root,
        &manifest,
        DepRequest {
            action: DepAction::List,
            package: None,
            scope: ".",
            scope_id: None,
        },
        Toolchain {
            node: None,
            python: Some(&python),
            pnpm: None,
            uv: None,
            venv_python: None,
            php: None,
            composer: None,
        },
    )
    .unwrap();
    assert_eq!(
        Store::open(&f.data)
            .unwrap()
            .get_scope_environment("legacy", "default")
            .unwrap()
            .unwrap(),
        original.display().to_string()
    );
    stackyard_core::project_view(&f.data, "legacy").unwrap();
    assert_eq!(
        fs::read_to_string(original.join("identity.txt")).unwrap(),
        "preserved"
    );
    assert!(!original.join("scopes").exists());
}

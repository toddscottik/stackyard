use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use stackyard_core::{prepare_data_dir, roundtrip, Command as EngineCommand, Outcome, Store};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_stackyard-fixture"))
}

struct Engine {
    dir: PathBuf,
    thread: Option<thread::JoinHandle<stackyard_core::Result<()>>>,
}
impl Engine {
    fn start() -> Self {
        let dir = prepare_data_dir(&stackyard_core::disposable_dir("readiness")).unwrap();
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
            assert!(
                tokio::time::Instant::now() < deadline,
                "engine failed to start"
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
            let _ = runtime.block_on(roundtrip(&dir, EngineCommand::Stop));
        });
        let _ = stopper.join();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

async fn adopt(dir: &Path) {
    roundtrip(
        dir,
        EngineCommand::UseExisting {
            role: "node".into(),
            path: fixture().display().to_string(),
        },
    )
    .await
    .unwrap();
}
async fn start_project(dir: &Path, root: &Path, body: &str) -> String {
    fs::create_dir_all(root).unwrap();
    fs::write(root.join("stackyard.toml"), body).unwrap();
    let outcome = roundtrip(
        dir,
        EngineCommand::ImportProject {
            path: root.display().to_string(),
        },
    )
    .await
    .unwrap();
    let Outcome::Project { project } = outcome else {
        panic!("expected imported project")
    };
    roundtrip(
        dir,
        EngineCommand::SelectPackage {
            project_id: project.id.clone(),
            package_id: "existing-node".into(),
        },
    )
    .await
    .unwrap();
    roundtrip(
        dir,
        EngineCommand::StartProject {
            project_id: project.id.clone(),
        },
    )
    .await
    .unwrap();
    project.id
}
async fn wait_state(dir: &Path, id: &str) -> stackyard_core::ProjectView {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        let outcome = roundtrip(
            dir,
            EngineCommand::ProjectStatus {
                project_id: id.into(),
            },
        )
        .await
        .unwrap();
        let Outcome::Project { project } = outcome else {
            panic!("expected project status")
        };
        if project
            .services
            .first()
            .is_some_and(|service| !["starting"].contains(&service.state.as_str()))
        {
            return project;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "service did not settle: {project:?}"
        );
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

async fn wait_all_states(dir: &Path, id: &str) -> stackyard_core::ProjectView {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
    loop {
        let outcome = roundtrip(
            dir,
            EngineCommand::ProjectStatus {
                project_id: id.into(),
            },
        )
        .await
        .unwrap();
        let Outcome::Project { project } = outcome else {
            panic!("expected project status")
        };
        if project
            .services
            .iter()
            .all(|service| service.state != "starting")
        {
            return project;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "services did not settle: {project:?}"
        );
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

fn manifest(id: &str, command: &str, readiness: &str) -> String {
    format!(
        r#"schema = 2
name = "{id}"
[[scope]]
id = "app"
directory = "."
runtime = "node"
manager = "npm"
lockfile_root = "."
[[service]]
id = "app"
scope = "app"
command = ["node", {command}]
working_directory = "."
{readiness}
"#
    )
}

#[tokio::test]
async fn supervisor_persists_raw_redirect_status_and_strict_health_failure() {
    let engine = Engine::start();
    engine.ready().await;
    adopt(&engine.dir).await;
    let redirect = start_project(&engine.dir, &engine.dir.join("redirect"), &manifest("redirect", "\"status\", \"302\"", "endpoint = { protocol = \"http\", port = \"auto\" }\nreadiness = { kind = \"http\", path = \"/\", statuses = [302], timeout_secs = 3 }")).await;
    let view = wait_state(&engine.dir, &redirect).await;
    assert_eq!(view.services[0].readiness_state, "ready");
    assert_eq!(view.services[0].http_status, Some(302));
    roundtrip(
        &engine.dir,
        EngineCommand::StopProject {
            project_id: redirect,
        },
    )
    .await
    .unwrap();

    let strict = start_project(&engine.dir, &engine.dir.join("strict"), &manifest("strict", "\"status\", \"503\"", "endpoint = { protocol = \"http\", port = \"auto\" }\nreadiness = { kind = \"http\", path = \"/health\", statuses = [200], timeout_secs = 1 }")).await;
    let view = wait_state(&engine.dir, &strict).await;
    assert_eq!(view.services[0].state, "failed");
    assert_eq!(view.services[0].readiness_state, "failed");
    assert_eq!(view.services[0].http_status, Some(503));
}

#[tokio::test]
async fn portless_worker_has_no_endpoint_and_tcp_readiness_has_no_http_status() {
    let engine = Engine::start();
    engine.ready().await;
    adopt(&engine.dir).await;
    let worker_root = engine.dir.join("worker");
    let worker = start_project(&engine.dir, &worker_root, &manifest(
        "worker", "\"worker\"", "readiness = { kind = \"process\", grace_ms = 50 }\nenv = { WORKER_MARKER = \"worker-env.txt\" }",
    )).await;
    let view = wait_state(&engine.dir, &worker).await;
    assert_eq!(view.services[0].state, "running");
    assert_eq!(view.services[0].readiness_state, "not_checked");
    assert_eq!(view.services[0].endpoint, None);
    assert_eq!(view.services[0].port, None);
    let marker = fs::read_to_string(worker_root.join("worker-env.txt")).unwrap();
    assert!(
        marker.contains("PORT=;"),
        "inherited PORT leaked into worker: {marker}"
    );
    assert!(
        marker.contains("FLASK_RUN_PORT="),
        "inherited Flask port leaked into worker: {marker}"
    );
    roundtrip(
        &engine.dir,
        EngineCommand::StopProject { project_id: worker },
    )
    .await
    .unwrap();

    let tcp = start_project(&engine.dir, &engine.dir.join("tcp"), &manifest(
        "tcp", "\"tcp\"", "endpoint = { protocol = \"tcp\", port = \"auto\" }\nreadiness = { kind = \"tcp\", timeout_secs = 3 }",
    )).await;
    let view = wait_state(&engine.dir, &tcp).await;
    assert_eq!(view.services[0].readiness_state, "ready");
    assert_eq!(view.services[0].http_status, None);
    roundtrip(&engine.dir, EngineCommand::StopProject { project_id: tcp })
        .await
        .unwrap();
}

#[tokio::test]
async fn native_schema2_php_worker_profile_rollout_is_scoped_and_portless() {
    let Ok(php) = std::env::var("STACKYARD_TEST_PHP") else {
        eprintln!("SKIPPED native schema 2 PHP worker rollout: set STACKYARD_TEST_PHP");
        return;
    };
    let engine = Engine::start();
    engine.ready().await;
    roundtrip(
        &engine.dir,
        EngineCommand::UseExisting {
            role: "php".into(),
            path: php,
        },
    )
    .await
    .unwrap();

    let root = engine.dir.join("php-worker-scopes");
    for scope in ["worker-a", "worker-b"] {
        fs::create_dir_all(root.join(scope)).unwrap();
        fs::write(
            root.join(scope).join("worker.php"),
            "<?php file_put_contents(__DIR__.'/worker.pid',(string)getmypid()); while(true){usleep(100000);} ",
        )
        .unwrap();
    }
    fs::write(
        root.join("stackyard.toml"),
        r#"schema = 2
name = "php-worker-scopes"
[[scope]]
id = "worker-a"
directory = "worker-a"
runtime = "php"
manager = "composer"
lockfile_root = "worker-a"
[[scope]]
id = "worker-b"
directory = "worker-b"
runtime = "php"
manager = "composer"
lockfile_root = "worker-b"
[[service]]
id = "worker-a"
scope = "worker-a"
command = ["php", "worker.php"]
working_directory = "worker-a"
readiness = { kind = "process", grace_ms = 200 }
[[service]]
id = "worker-b"
scope = "worker-b"
command = ["php", "worker.php"]
working_directory = "worker-b"
readiness = { kind = "process", grace_ms = 200 }
"#,
    )
    .unwrap();
    let imported = roundtrip(
        &engine.dir,
        EngineCommand::ImportProject {
            path: root.display().to_string(),
        },
    )
    .await
    .unwrap();
    let Outcome::Project { project } = imported else {
        panic!("expected imported PHP project")
    };
    roundtrip(
        &engine.dir,
        EngineCommand::StartProject {
            project_id: project.id.clone(),
        },
    )
    .await
    .unwrap();
    let before = wait_all_states(&engine.dir, &project.id).await;
    assert!(
        before
            .services
            .iter()
            .all(|service| service.state == "running"),
        "{before:?}"
    );
    assert!(
        before
            .services
            .iter()
            .all(|service| service.endpoint.is_none() && service.port.is_none()),
        "{before:?}"
    );
    let pid_a = before
        .services
        .iter()
        .find(|service| service.id == "worker-a")
        .unwrap()
        .pid;
    let pid_b = before
        .services
        .iter()
        .find(|service| service.id == "worker-b")
        .unwrap()
        .pid;

    roundtrip(
        &engine.dir,
        EngineCommand::PhpApplyProfileScoped {
            project_id: project.id.clone(),
            scope_id: "worker-a".into(),
            extensions: Vec::new(),
        },
    )
    .await
    .unwrap();
    let after = wait_all_states(&engine.dir, &project.id).await;
    assert_eq!(
        after
            .services
            .iter()
            .find(|service| service.id == "worker-a")
            .unwrap()
            .readiness_state,
        "not_checked"
    );
    assert_ne!(
        after
            .services
            .iter()
            .find(|service| service.id == "worker-a")
            .unwrap()
            .pid,
        pid_a,
        "target worker should restart"
    );
    assert_eq!(
        after
            .services
            .iter()
            .find(|service| service.id == "worker-b")
            .unwrap()
            .pid,
        pid_b,
        "other PHP scope must remain running"
    );
    let (_, profile_key) =
        stackyard_core::php_project::php_scope_identity(&engine.dir, &project.id, Some("worker-a"))
            .unwrap();
    assert!(
        stackyard_core::php::read_php_profile(&engine.dir, &profile_key)
            .unwrap()
            .is_some()
    );
    roundtrip(
        &engine.dir,
        EngineCommand::StopProject {
            project_id: project.id,
        },
    )
    .await
    .unwrap();
}

#[test]
fn observations_are_stored_independently_from_process_rows() {
    let dir = prepare_data_dir(&stackyard_core::disposable_dir("readiness-store")).unwrap();
    let store = Store::open(&dir).unwrap();
    assert!(store
        .get_service_observation("missing", "worker")
        .unwrap()
        .is_none());
    let _ = fs::remove_dir_all(dir);
}

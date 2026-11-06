use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use stackyard_core::{
    disposable_dir, plain_path, prepare_data_dir, roundtrip, sqlite_read, Command, DbBindingRecord,
    DbInstanceRecord, Outcome, PublicationRecord, Store,
};

const ID: &str = "forget-fixture";
const MANIFEST: &str = r#"schema = 1
name = "forget-fixture"
[[service]]
id = "web"
runtime = "node"
command = ["node", "serve"]
port = 0
"#;

struct Engine {
    dir: PathBuf,
    thread: Option<JoinHandle<stackyard_core::Result<()>>>,
}

impl Engine {
    async fn start() -> Self {
        let dir = prepare_data_dir(&disposable_dir("forget")).unwrap();
        let owned = dir.clone();
        let engine = Self {
            dir,
            thread: Some(thread::spawn(move || stackyard_engine::serve(&owned))),
        };
        wait_for(|| async { roundtrip(&engine.dir, Command::Status).await.is_ok() }).await;
        engine
    }

    async fn call(&self, command: Command) -> Outcome {
        roundtrip(&self.dir, command).await.unwrap()
    }

    async fn assert_refused(&self, detail: &str) {
        let error = roundtrip(&self.dir, forget())
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains(detail), "{error}");
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let dir = self.dir.clone();
        let _ = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let _ = runtime.block_on(roundtrip(&dir, Command::Stop));
        })
        .join();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn forget() -> Command {
    Command::ForgetProject {
        project_id: ID.into(),
    }
}

fn import(path: &Path) -> Command {
    Command::ImportProject {
        path: path.display().to_string(),
    }
}

async fn wait_for<F: Future<Output = bool>>(mut check: impl FnMut() -> F) {
    tokio::time::timeout(Duration::from_secs(8), async {
        while !check().await {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .expect("condition did not settle");
}

#[tokio::test]
async fn forget_refuses_active_work_and_detaches_only_the_stopped_project() {
    let engine = Engine::start().await;
    let root = engine.dir.join("project");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("stackyard.toml"), MANIFEST).unwrap();
    fs::write(root.join("message.txt"), b"source").unwrap();
    engine.call(import(&root)).await;
    engine
        .call(Command::UseExisting {
            role: "node".into(),
            path: env!("CARGO_BIN_EXE_stackyard-fixture").into(),
        })
        .await;
    engine
        .call(Command::SelectPackage {
            project_id: ID.into(),
            package_id: "existing-node".into(),
        })
        .await;

    let Outcome::Task { id, .. } = engine
        .call(Command::Exec {
            project_id: ID.into(),
            service_id: "web".into(),
            args: vec!["node".into(), "worker".into()],
        })
        .await
    else {
        panic!("expected exec task");
    };
    let marker = root.join("worker-env.txt");
    wait_for(|| async { marker.is_file() }).await;
    engine.assert_refused("operations are still active").await;
    engine
        .call(Command::DepCancel {
            operation_id: id.clone(),
        })
        .await;
    wait_for(|| async {
        matches!(
            engine.call(Command::DepStatus { operation_id: id.clone() }).await,
            Outcome::Task { state, .. } if state == "cancelled"
        )
    })
    .await;

    engine
        .call(Command::StartProject {
            project_id: ID.into(),
        })
        .await;
    engine.assert_refused("operations are still active").await;
    wait_for(|| async {
        matches!(
            engine.call(Command::ProjectStatus { project_id: ID.into() }).await,
            Outcome::Project { project } if project.services[0].state == "running"
        )
    })
    .await;
    engine.assert_refused("operations are still active").await;
    engine
        .call(Command::StopProject {
            project_id: ID.into(),
        })
        .await;

    engine
        .call(Command::SqliteUse {
            project_id: ID.into(),
            path: "project.sqlite".into(),
        })
        .await;
    let store = Store::open(&engine.dir).unwrap();
    let publication = PublicationRecord {
        project_id: ID.into(),
        host: "forget-fixture.localhost".into(),
        mode: "proxy".into(),
        service_id: "web".into(),
        static_dir: String::new(),
    };
    store.upsert_publication(&publication).unwrap();
    engine.assert_refused("Unpublish the site").await;
    assert_eq!(store.list_publications().unwrap(), vec![publication]);
    store.delete_publication(ID).unwrap();

    let environment = engine.dir.join("environments").join(ID);
    let profile = engine.dir.join("php/profiles").join(ID);
    for path in [&environment, &profile] {
        fs::create_dir_all(path).unwrap();
        fs::write(path.join("retained"), b"local state").unwrap();
    }
    store
        .bind_scope_environment(ID, "default", environment.to_str().unwrap())
        .unwrap();
    let secret =
        stackyard_core::write_secret(&engine.dir.join("secrets"), "fixture", "kept").unwrap();
    let instance = DbInstanceRecord {
        id: "fixture".into(),
        engine: "postgres".into(),
        port: 5432,
        data_dir: engine.dir.join("fixture-data").display().to_string(),
        secret_path: secret.display().to_string(),
        state: "stopped".into(),
        pid: None,
        identity: None,
    };
    store.upsert_db_instance(&instance).unwrap();
    store
        .upsert_db_binding(&DbBindingRecord {
            project_id: ID.into(),
            connection_id: "fixture".into(),
            instance_id: instance.id.clone(),
            engine: "postgres".into(),
            host: "127.0.0.1".into(),
            port: 5432,
            database_name: "fixture".into(),
            user_name: "fixture".into(),
            secret_path: instance.secret_path.clone(),
            external: false,
        })
        .unwrap();
    let sqlite = store.get_sqlite(ID).unwrap().unwrap();
    let log = stackyard_core::service_log_path(&engine.dir, ID, "web");
    assert!(log.is_file());
    assert!(
        matches!(engine.call(forget()).await, Outcome::ProjectForgotten { project_id } if project_id == ID)
    );
    assert!(store.get_project(ID).unwrap().is_none());
    assert!(store.get_selection(ID, "node").unwrap().is_none());
    assert!(store
        .get_scope_environment(ID, "default")
        .unwrap()
        .is_none());
    assert!(store.get_service_observation(ID, "web").unwrap().is_none());
    assert!(store.list_processes(ID).unwrap().is_empty());
    assert!(store.list_db_bindings().unwrap().is_empty());
    assert_eq!(store.list_db_instances().unwrap(), vec![instance]);
    assert!(store.get_sqlite(ID).unwrap().is_none());
    assert_eq!(
        sqlite_read(Path::new(&sqlite.path), 1).unwrap().as_deref(),
        Some("kept")
    );
    assert_eq!(fs::read_to_string(&secret).unwrap(), "kept");
    assert_eq!(
        fs::read_to_string(root.join("stackyard.toml")).unwrap(),
        MANIFEST
    );
    assert_eq!(fs::read(root.join("message.txt")).unwrap(), b"source");
    assert!(log.is_file());
    for path in [environment, profile] {
        assert_eq!(fs::read(path.join("retained")).unwrap(), b"local state");
    }

    let other = engine.dir.join("other");
    fs::create_dir(&other).unwrap();
    fs::write(other.join("stackyard.toml"), MANIFEST).unwrap();
    let error = roundtrip(&engine.dir, import(&other)).await.unwrap_err();
    assert!(error.to_string().contains("reserved"), "{error}");
    engine.call(import(&root)).await;
    assert_eq!(
        store.get_project(ID).unwrap().unwrap().root,
        plain_path(root.canonicalize().unwrap())
            .display()
            .to_string()
    );
}

//! Disposable MySQL, PostgreSQL, and SQLite on this machine.
//! Windows and macOS runners do not compile this test.

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use stackyard_core::{prepare_data_dir, roundtrip, Command as EngineCommand, Outcome};

fn temp_dir(label: &str) -> PathBuf {
    prepare_data_dir(&stackyard_core::disposable_dir(label)).expect("temp dir")
}

struct Engine {
    dir: PathBuf,
    thread: Option<thread::JoinHandle<stackyard_core::Result<()>>>,
}

impl Engine {
    fn start() -> Self {
        let dir = temp_dir("db-engine");
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
            if roundtrip(&self.dir, EngineCommand::DbList).await.is_ok() {
                return;
            }
            if tokio::time::Instant::now() > deadline {
                panic!("engine did not accept a database request");
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

fn view(outcome: Outcome) -> stackyard_core::DbView {
    match outcome {
        Outcome::Databases { view } => view,
        other => panic!("expected databases, got {other:?}"),
    }
}

async fn call(dir: &Path, command: EngineCommand) -> stackyard_core::DbView {
    view(roundtrip(dir, command).await.expect("database command"))
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn mysql_postgres_and_sqlite_keep_a_row_across_restart_and_restore() {
    let mysqld = Path::new("/usr/sbin/mysqld");
    let postgres = Path::new("/usr/lib/postgresql/16/bin/postgres");
    assert!(mysqld.is_file(), "mysqld is required");
    assert!(postgres.is_file(), "postgres is required");
    let engine = Engine::start();
    engine.ready().await;

    let migrate = roundtrip(&engine.dir, EngineCommand::DbMigrate).await;
    let message = migrate.expect_err("migration").to_string();
    assert!(message.contains("not supported"), "{message}");

    let nonempty = temp_dir("nonempty");
    fs::write(nonempty.join("marker.txt"), "keep").unwrap();
    let refused = stackyard_core::require_empty_data_dir(&nonempty);
    assert!(refused.is_err());
    assert_eq!(
        fs::read_to_string(nonempty.join("marker.txt")).unwrap(),
        "keep"
    );

    roundtrip(
        &engine.dir,
        EngineCommand::UseExisting {
            role: "mysql".to_string(),
            path: mysqld.display().to_string(),
        },
    )
    .await
    .expect("adopt mysql");
    roundtrip(
        &engine.dir,
        EngineCommand::UseExisting {
            role: "postgres".to_string(),
            path: postgres.display().to_string(),
        },
    )
    .await
    .expect("adopt postgres");

    let root = temp_dir("projects");
    for name in ["alpha", "beta"] {
        let project = root.join(name);
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("stackyard.toml"),
            format!("schema = 1\nname = \"{name}\"\n\n[site]\nroot = \"public\"\n"),
        )
        .unwrap();
        fs::create_dir_all(project.join("public")).unwrap();
        roundtrip(
            &engine.dir,
            EngineCommand::ImportProject {
                path: project.display().to_string(),
            },
        )
        .await
        .expect("import");
    }

    for (engine_name, label) in [("mysql", "my"), ("postgres", "pg")] {
        let created = call(
            &engine.dir,
            EngineCommand::DbCreate {
                engine: engine_name.to_string(),
                port: 0,
            },
        )
        .await;
        let instance = created
            .instances
            .iter()
            .find(|row| row.engine == engine_name)
            .unwrap_or_else(|| panic!("missing {engine_name}: {created:?}"));
        assert_eq!(instance.state, "running");
        assert_eq!(instance.secret_path, "[redacted]");
        let id = instance.id.clone();
        let data_dir = StorePath::from_message(&created.message, &engine.dir, &id);
        call(
            &engine.dir,
            EngineCommand::DbAdd {
                instance_id: id.clone(),
                project_id: "alpha".to_string(),
                name: format!("{label}db"),
            },
        )
        .await;
        call(
            &engine.dir,
            EngineCommand::DbAdd {
                instance_id: id.clone(),
                project_id: "beta".to_string(),
                name: format!("{label}other"),
            },
        )
        .await;
        let backup = engine.dir.join(format!("{label}.sql"));
        call(
            &engine.dir,
            EngineCommand::DbBackup {
                instance_id: id.clone(),
                database: format!("{label}db"),
                destination: backup.display().to_string(),
            },
        )
        .await;
        call(
            &engine.dir,
            EngineCommand::DbRestore {
                instance_id: id.clone(),
                database: format!("{label}db"),
                source: backup.display().to_string(),
                into: format!("{label}restored"),
            },
        )
        .await;
        call(
            &engine.dir,
            EngineCommand::DbStop {
                instance_id: id.clone(),
            },
        )
        .await;
        let restarted = call(
            &engine.dir,
            EngineCommand::DbStart {
                instance_id: id.clone(),
            },
        )
        .await;
        assert!(restarted
            .instances
            .iter()
            .any(|row| row.id == id && row.state == "running"));
        let kept = call(
            &engine.dir,
            EngineCommand::DbProbe {
                project_id: "alpha".to_string(),
                name: format!("{label}db"),
            },
        )
        .await;
        assert_eq!(kept.message, "kept");
        call(
            &engine.dir,
            EngineCommand::DbDetach {
                project_id: "beta".to_string(),
                connection_id: format!("{label}other"),
            },
        )
        .await;
        let still = call(&engine.dir, EngineCommand::DbList).await;
        assert!(still
            .instances
            .iter()
            .any(|row| row.id == id && row.state == "running"));
        assert!(data_dir.exists(), "database files stay after detach");
        let _ = label;
    }

    roundtrip(
        &engine.dir,
        EngineCommand::StopProject {
            project_id: "alpha".to_string(),
        },
    )
    .await
    .expect("stop project");
    let shared = call(&engine.dir, EngineCommand::DbList).await;
    assert!(
        shared
            .instances
            .iter()
            .any(|row| row.engine == "mysql" && row.state == "running"),
        "stopping a project leaves the shared mysql instance running: {shared:?}"
    );
    assert!(
        shared
            .instances
            .iter()
            .any(|row| row.engine == "postgres" && row.state == "running"),
        "stopping a project leaves the shared postgres instance running: {shared:?}"
    );

    call(
        &engine.dir,
        EngineCommand::SqliteUse {
            project_id: "alpha".to_string(),
            path: "data/app.sqlite".to_string(),
        },
    )
    .await;
    let copy = engine.dir.join("app-copy.sqlite");
    call(
        &engine.dir,
        EngineCommand::SqliteBackup {
            project_id: "alpha".to_string(),
            destination: copy.display().to_string(),
        },
    )
    .await;
    let restored = engine.dir.join("app-restored.sqlite");
    let checked = call(
        &engine.dir,
        EngineCommand::SqliteRestore {
            project_id: "alpha".to_string(),
            source: copy.display().to_string(),
            destination: restored.display().to_string(),
        },
    )
    .await;
    assert!(checked.message.contains("verified"), "{checked:?}");
    let password = "super-secret-value";
    let attached = call(
        &engine.dir,
        EngineCommand::DbAttach {
            project_id: "alpha".to_string(),
            engine: "postgres".to_string(),
            host: "127.0.0.1".to_string(),
            port: 15432,
            database: "outside".to_string(),
            user: "outside".to_string(),
            password: password.to_string(),
        },
    )
    .await;
    let rendered = format!("{attached:?}");
    assert!(!rendered.contains(password), "{rendered}");
    assert!(attached
        .bindings
        .iter()
        .any(|row| row.external && row.instance_id.is_empty()));
    call(
        &engine.dir,
        EngineCommand::DbDetach {
            project_id: "alpha".to_string(),
            connection_id: "external-outside".to_string(),
        },
    )
    .await;
    let after = call(&engine.dir, EngineCommand::DbList).await;
    assert!(after.instances.iter().any(|row| row.state == "running"));
}

struct StorePath;

impl StorePath {
    fn from_message(message: &str, engine: &Path, id: &str) -> PathBuf {
        let _ = message;
        engine.join("databases").join(id).join("data")
    }
}

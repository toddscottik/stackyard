//! Interrupted installs and offline project state.
//! The bundled catalog is not downloaded during refresh.

use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use stackyard_core::{prepare_data_dir, roundtrip, Command, Outcome, Store};

fn temp_dir(label: &str) -> PathBuf {
    prepare_data_dir(&stackyard_core::disposable_dir(label)).expect("temp dir")
}

struct Engine {
    dir: PathBuf,
    thread: Option<thread::JoinHandle<stackyard_core::Result<()>>>,
}

impl Engine {
    fn start(dir: PathBuf) -> Self {
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
            if roundtrip(&self.dir, Command::RefreshUpdates).await.is_ok() {
                return;
            }
            if tokio::time::Instant::now() > deadline {
                panic!("engine did not accept an update request");
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
            let _ = runtime.block_on(roundtrip(&dir, Command::Stop));
        });
        let _ = stopper.join();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[tokio::test]
async fn interrupted_install_is_cleared_and_a_cached_project_stays_listed() {
    let dir = temp_dir("updates-engine");
    let kept = dir.join("packages").join("node").join("24.22.2");
    fs::create_dir_all(&kept).unwrap();
    fs::write(kept.join("node"), b"kept").unwrap();
    let partial = dir.join("incoming").join("node-next");
    fs::create_dir_all(&partial).unwrap();
    fs::write(partial.join("payload"), b"partial").unwrap();
    Store::open(&dir)
        .unwrap()
        .mark_install("node-next", "running", "downloading")
        .unwrap();

    let engine = Engine::start(dir.clone());
    engine.ready().await;
    assert!(!partial.exists(), "partial download remains");
    assert_eq!(fs::read(kept.join("node")).unwrap(), b"kept");

    let project = temp_dir("cached-project");
    fs::write(
        project.join("stackyard.toml"),
        "schema = 1\nname = \"cached\"\n\n[site]\nroot = \"public\"\n",
    )
    .unwrap();
    fs::create_dir_all(project.join("public")).unwrap();
    roundtrip(
        &engine.dir,
        Command::ImportProject {
            path: project.display().to_string(),
        },
    )
    .await
    .expect("import");
    let updates = roundtrip(&engine.dir, Command::RefreshUpdates)
        .await
        .expect("refresh");
    let Outcome::Updates { notices } = updates else {
        panic!("expected updates, got {updates:?}");
    };
    assert!(notices.iter().any(|notice| notice.kind == "app"));
    assert!(notices.iter().any(|notice| notice.kind == "news"));
    assert!(notices
        .iter()
        .any(|notice| notice.kind == "dependency" && notice.detail.contains("deps update")));
    assert!(notices
        .iter()
        .all(|notice| notice.kind != "installable" || !notice.detail.contains("http")));
    let listed = roundtrip(&engine.dir, Command::ListProjects)
        .await
        .expect("projects");
    let Outcome::Projects { projects } = listed else {
        panic!("expected projects, got {listed:?}");
    };
    assert!(projects.iter().any(|project| project.id == "cached"));
}

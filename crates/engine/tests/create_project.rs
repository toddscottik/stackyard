use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use stackyard_core::create::{ComponentChoice, CreateRequest};
use stackyard_core::{roundtrip, Command, Outcome, PackageRecord, Store};

struct Engine {
    root: PathBuf,
    data: PathBuf,
    thread: Option<std::thread::JoinHandle<stackyard_core::Result<()>>>,
}

impl Drop for Engine {
    fn drop(&mut self) {
        let data = self.data.clone();
        let _ = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let _ = runtime.block_on(roundtrip(&data, Command::Stop));
        })
        .join();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
async fn reviewed_source_and_scope_register_once_over_ipc() {
    let root = stackyard_core::disposable_dir("create-ipc");
    fs::create_dir_all(&root).unwrap();
    let data = stackyard_core::prepare_data_dir(&root.join("engine")).unwrap();
    let parent = root.join("projets générés");
    fs::create_dir(&parent).unwrap();
    let runtime = root.join("recorded-python");
    fs::write(
        &runtime,
        b"Preview and apply must not execute this fixture.",
    )
    .unwrap();
    let store = Store::open(&data).unwrap();
    store
        .insert_package(&PackageRecord {
            id: "recorded-python".into(),
            kind: "runtime".into(),
            name: "python".into(),
            version: "3.12.16".into(),
            target: stackyard_core::current_target().into(),
            source: "existing".into(),
            path: root.display().to_string(),
            launch: runtime.display().to_string(),
            digest: None,
            removable: false,
        })
        .unwrap();
    let owned = data.clone();
    let engine = Engine {
        root,
        data,
        thread: Some(std::thread::spawn(move || stackyard_engine::serve(&owned))),
    };
    let deadline = Instant::now() + Duration::from_secs(8);
    while roundtrip(&engine.data, Command::Status).await.is_err() {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    let request = CreateRequest {
        parent: parent.display().to_string(),
        name: "starter".into(),
        recipe: "flask-app".into(),
        manager: "pip".into(),
        runtime: ComponentChoice {
            kind: "installed".into(),
            id: "recorded-python".into(),
        },
        tool: None,
    };
    let Outcome::CreatePlan { plan } = roundtrip(&engine.data, Command::PlanCreate { request })
        .await
        .unwrap()
    else {
        panic!("expected a creation plan");
    };
    let target = PathBuf::from(&plan.target);
    assert!(!target.exists());
    assert!(store.get_project("starter").unwrap().is_none());

    apply(&engine.data, (*plan).clone()).await;
    assert!(target.join("app.py").is_file());
    assert_eq!(
        store
            .get_scope_selection("starter", "app", "python")
            .unwrap()
            .as_deref(),
        Some("recorded-python")
    );
    assert!(store.get_selection("default", "python").unwrap().is_none());
    let original = fs::read(target.join("app.py")).unwrap();
    apply(&engine.data, (*plan).clone()).await;
    assert_eq!(store.list_projects().unwrap().len(), 1);
    assert_eq!(original, fs::read(target.join("app.py")).unwrap());
    drop(store);
}

async fn apply(data: &Path, plan: stackyard_core::create::CreatePlan) {
    let Outcome::Task { id, .. } = roundtrip(data, Command::ApplyCreate { plan })
        .await
        .unwrap()
    else {
        panic!("expected a creation task");
    };
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let Outcome::Task { state, message, .. } = roundtrip(
            data,
            Command::DepStatus {
                operation_id: id.clone(),
            },
        )
        .await
        .unwrap() else {
            panic!("expected task status");
        };
        if state != "running" {
            assert_eq!(state, "succeeded", "{message}");
            return;
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use localdev_core::{prepare_data_dir, roundtrip, Error};
use localdev_core::{Command, Outcome};

fn temp_dir() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "localdev-engine-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    prepare_data_dir(&path).expect("temp dir")
}

async fn wait_status(dir: &std::path::Path) -> Outcome {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(outcome) = roundtrip(dir, Command::Status).await {
            return outcome;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("engine did not accept a status request");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn second_server_refuses_and_settings_survive_restart() {
    let dir = temp_dir();
    let dir_for_server = dir.clone();
    let first = std::thread::spawn(move || localdev_engine::serve(&dir_for_server));

    let status = wait_status(&dir).await;
    let Outcome::Status {
        pid,
        schema_version,
        ..
    } = status
    else {
        panic!("expected status, got {status:?}");
    };
    assert_eq!(schema_version, 2);
    assert!(pid > 0);

    let dir_for_second = dir.clone();
    let second = std::thread::spawn(move || localdev_engine::serve(&dir_for_second));
    let second_result = second.join().expect("second server thread");
    assert!(
        matches!(second_result, Err(Error::AlreadyRunning { .. })),
        "second server result: {second_result:?}"
    );

    let stored = roundtrip(
        &dir,
        Command::PutSetting {
            key: "display_name".into(),
            value: "Ada".into(),
        },
    )
    .await
    .expect("put");
    assert_eq!(
        stored,
        Outcome::Setting {
            key: "display_name".into(),
            value: Some("Ada".into()),
        }
    );

    let stopped = roundtrip(&dir, Command::Stop).await.expect("stop");
    assert_eq!(stopped, Outcome::Stopped);
    first
        .join()
        .expect("first server thread")
        .expect("first server");

    let dir_for_restart = dir.clone();
    let restarted = std::thread::spawn(move || localdev_engine::serve(&dir_for_restart));
    let status = wait_status(&dir).await;
    let Outcome::Status { last_event, .. } = &status else {
        panic!("expected status after restart");
    };
    assert_eq!(last_event.as_deref(), Some("setting_put display_name"));
    let value = roundtrip(
        &dir,
        Command::GetSetting {
            key: "display_name".into(),
        },
    )
    .await
    .expect("get");
    assert_eq!(
        value,
        Outcome::Setting {
            key: "display_name".into(),
            value: Some("Ada".into()),
        }
    );
    roundtrip(&dir, Command::Stop)
        .await
        .expect("stop restarted");
    restarted
        .join()
        .expect("restarted thread")
        .expect("restarted server");
    let _ = std::fs::remove_dir_all(&dir);
}

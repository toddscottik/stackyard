//! Native worker-only PHP rollout. Explicit runtime paths are opt-in; absent
//! variables produce a visible skip, not native compatibility evidence.
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use stackyard_core::{
    prepare_data_dir, roundtrip, roundtrip_within, Command, Outcome, ProjectView,
};

struct Engine {
    dir: PathBuf,
    thread: Option<thread::JoinHandle<stackyard_core::Result<()>>>,
}

impl Engine {
    async fn start() -> Self {
        let dir = prepare_data_dir(&stackyard_core::disposable_dir("php-reconfigure")).unwrap();
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
                "engine startup deadline"
            );
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
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

const INDEX: &str = "<?php if (file_exists(__DIR__.'/not-ready') || (file_exists(__DIR__.'/needs-openssl') && !extension_loaded('openssl')) || (file_exists(__DIR__.'/reject-openssl') && extension_loaded('openssl'))) { http_response_code(503); echo 'not-ready'; } else { echo extension_loaded('openssl') ? 'openssl:on' : 'openssl:off'; echo '|persistent-data'; }";

fn project(root: &Path, id: &str, mixed: bool) {
    fs::create_dir_all(root).unwrap();
    let mut manifest = format!(
        r#"schema = 1
name = "{id}"
manager = "composer"
[[service]]
id = "web"
runtime = "php"
command = ["php", "-S", "127.0.0.1:{{port}}", "-t", "."]
ready_timeout_secs = 2
[[service]]
id = "cgi"
runtime = "php"
command = ["php-cgi", "-b", "127.0.0.1:{{port}}"]
ready_timeout_secs = 2
"#
    );
    if mixed {
        manifest.push_str(
            r#"[[service]]
id = "node"
runtime = "node"
command = ["node", "server.js"]
ready_timeout_secs = 3
[[service]]
id = "python"
runtime = "python"
command = ["python", "-m", "http.server", "{port}", "--bind", "127.0.0.1"]
ready_timeout_secs = 3
"#,
        );
    }
    fs::write(root.join("stackyard.toml"), manifest).unwrap();
    fs::write(root.join("index.php"), INDEX).unwrap();
    fs::write(root.join("data.txt"), "persistent-data").unwrap();
    fs::write(root.join("server.js"), "require('http').createServer((q,s)=>s.end('node-unchanged')).listen(Number(process.env.PORT),'127.0.0.1');").unwrap();
}

async fn status(dir: &Path, id: &str) -> ProjectView {
    let Outcome::Project { project } = roundtrip(
        dir,
        Command::ProjectStatus {
            project_id: id.into(),
        },
    )
    .await
    .unwrap() else {
        panic!("project status");
    };
    project
}

async fn running(dir: &Path, id: &str) -> ProjectView {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let view = status(dir, id).await;
        if view.services.iter().all(|s| s.state != "starting") {
            assert!(
                view.services.iter().all(|s| s.state == "running"),
                "{view:?}"
            );
            return view;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "startup deadline {view:?}"
        );
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
}

fn pid(view: &ProjectView, id: &str) -> u32 {
    view.services
        .iter()
        .find(|s| s.id == id)
        .unwrap()
        .pid
        .unwrap()
}

fn port(view: &ProjectView, id: &str) -> u16 {
    view.services
        .iter()
        .find(|s| s.id == id)
        .unwrap()
        .port
        .unwrap()
}

fn http(port: u16) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

fn fcgi_record(stream: &mut TcpStream, kind: u8, body: &[u8]) {
    let len = u16::try_from(body.len()).unwrap().to_be_bytes();
    stream
        .write_all(&[1, kind, 0, 1, len[0], len[1], 0, 0])
        .unwrap();
    stream.write_all(body).unwrap();
}

fn fcgi_length(bytes: &mut Vec<u8>, len: usize) {
    if len < 128 {
        bytes.push(len as u8);
    } else {
        bytes.extend_from_slice(&(u32::try_from(len).unwrap() | 0x8000_0000).to_be_bytes());
    }
}

fn fastcgi(port: u16, root: &Path) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    fcgi_record(&mut stream, 1, &[0, 1, 0, 0, 0, 0, 0, 0]);
    let script = root.join("index.php").display().to_string();
    let mut params = Vec::new();
    for (key, value) in [
        ("SCRIPT_FILENAME", script.as_str()),
        ("SCRIPT_NAME", "/index.php"),
        ("REQUEST_METHOD", "GET"),
        ("REQUEST_URI", "/index.php"),
        ("SERVER_PROTOCOL", "HTTP/1.1"),
        ("GATEWAY_INTERFACE", "CGI/1.1"),
        ("REDIRECT_STATUS", "200"),
    ] {
        fcgi_length(&mut params, key.len());
        fcgi_length(&mut params, value.len());
        params.extend_from_slice(key.as_bytes());
        params.extend_from_slice(value.as_bytes());
    }
    fcgi_record(&mut stream, 4, &params);
    fcgi_record(&mut stream, 4, &[]);
    fcgi_record(&mut stream, 5, &[]);
    let mut response = Vec::new();
    loop {
        let mut header = [0; 8];
        stream.read_exact(&mut header).unwrap();
        let len = u16::from_be_bytes([header[4], header[5]]) as usize;
        let mut body = vec![0; len + usize::from(header[6])];
        stream.read_exact(&mut body).unwrap();
        assert!(response.len() + len < 64 * 1024, "FastCGI output limit");
        match header[1] {
            6 => response.extend_from_slice(&body[..len]),
            7 => assert!(
                body[..len].is_empty(),
                "FastCGI stderr: {}",
                String::from_utf8_lossy(&body[..len])
            ),
            3 => break,
            _ => {}
        }
    }
    String::from_utf8(response).unwrap()
}

async fn apply(dir: &Path, id: &str, extensions: &[&str]) -> stackyard_core::Result<Outcome> {
    roundtrip_within(
        dir,
        Command::PhpApplyProfile {
            project_id: id.into(),
            extensions: extensions.iter().map(|s| (*s).into()).collect(),
        },
        Duration::from_secs(30),
    )
    .await
}

async fn cli(dir: &Path, id: &str) -> String {
    let Outcome::Task { id, .. } = roundtrip(
        dir,
        Command::Exec {
            project_id: id.into(),
            service_id: "web".into(),
            args: vec![
                "php".into(),
                "-r".into(),
                "echo extension_loaded('openssl') ? 'openssl:on' : 'openssl:off';".into(),
            ],
        },
    )
    .await
    .unwrap() else {
        panic!("exec task");
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        let Outcome::Task { state, message, .. } = roundtrip(
            dir,
            Command::DepStatus {
                operation_id: id.clone(),
            },
        )
        .await
        .unwrap() else {
            panic!("task state");
        };
        if state != "running" {
            assert_eq!(state, "succeeded", "{message}");
            return message;
        }
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
}

fn unchanged(before: &ProjectView, after: &ProjectView, names: &[&str]) {
    for id in names {
        assert_eq!(
            pid(before, id),
            pid(after, id),
            "unrelated service {id} restarted"
        );
    }
}

fn restarted(before: &ProjectView, after: &ProjectView) {
    for id in ["web", "cgi"] {
        assert_ne!(pid(before, id), pid(after, id), "PHP {id} did not restart");
        assert_eq!(port(before, id), port(after, id), "PHP {id} port changed");
    }
}

#[tokio::test]
async fn profiles_restart_only_live_php_and_recover_previous_on_failure() {
    let paths: Option<Vec<_>> = [
        "STACKYARD_TEST_PHP",
        "STACKYARD_TEST_NODE",
        "STACKYARD_TEST_PYTHON",
    ]
    .iter()
    .map(std::env::var_os)
    .collect();
    let Some(paths) = paths else {
        eprintln!("SKIPPED native PHP reconfigure: set STACKYARD_TEST_PHP, STACKYARD_TEST_NODE, STACKYARD_TEST_PYTHON; no runtime evidence");
        return;
    };
    let engine = Engine::start().await;
    for (role, path) in ["php", "node", "python"].into_iter().zip(paths) {
        roundtrip(
            &engine.dir,
            Command::UseExisting {
                role: role.into(),
                path: PathBuf::from(path).display().to_string(),
            },
        )
        .await
        .unwrap();
    }
    let mixed_root = engine.dir.join("mixed PHP project пример");
    let other_root = engine.dir.join("other PHP project");
    project(&mixed_root, "mixed", true);
    project(&other_root, "other", false);
    for root in [&mixed_root, &other_root] {
        roundtrip(
            &engine.dir,
            Command::ImportProject {
                path: root.display().to_string(),
            },
        )
        .await
        .unwrap();
    }
    fs::write(mixed_root.join("not-ready"), b"hold startup").unwrap();
    roundtrip(
        &engine.dir,
        Command::StartProject {
            project_id: "mixed".into(),
        },
    )
    .await
    .unwrap();
    let error = apply(&engine.dir, "mixed", &["openssl"])
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("starting"), "{error}");
    fs::remove_file(mixed_root.join("not-ready")).unwrap();
    let initial = running(&engine.dir, "mixed").await;
    roundtrip(
        &engine.dir,
        Command::StartProject {
            project_id: "other".into(),
        },
    )
    .await
    .unwrap();
    let other = running(&engine.dir, "other").await;
    assert!(http(port(&initial, "web")).contains("openssl:off"));
    assert!(fastcgi(port(&initial, "cgi"), &mixed_root).contains("openssl:off"));
    let store = stackyard_core::Store::open(&engine.dir).unwrap();
    let row = store
        .list_processes("mixed")
        .unwrap()
        .into_iter()
        .find(|row| row.service_id == "web")
        .unwrap();
    let mut orphaned = row.clone();
    orphaned.state = "orphaned".into();
    store.upsert_process(&orphaned).unwrap();
    let error = apply(&engine.dir, "mixed", &["openssl"])
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("orphaned"), "{error}");
    store.upsert_process(&row).unwrap();
    unchanged(
        &initial,
        &running(&engine.dir, "mixed").await,
        &["web", "cgi", "node", "python"],
    );
    // Rollback restores the original -n selection without a phantom profile.
    fs::write(
        mixed_root.join("reject-openssl"),
        b"reject enabled candidate",
    )
    .unwrap();
    let error = apply(&engine.dir, "mixed", &["openssl"])
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("previous profile restored and PHP workers recovered"),
        "{error}"
    );
    let unprofiled_recovered = running(&engine.dir, "mixed").await;
    restarted(&initial, &unprofiled_recovered);
    unchanged(&initial, &unprofiled_recovered, &["node", "python"]);
    assert!(stackyard_core::php::read_php_profile(&engine.dir, "mixed")
        .unwrap()
        .is_none());
    assert!(http(port(&unprofiled_recovered, "web")).contains("openssl:off"));
    fs::remove_file(mixed_root.join("reject-openssl")).unwrap();
    apply(&engine.dir, "mixed", &["openssl"]).await.unwrap();
    let enabled = running(&engine.dir, "mixed").await;
    restarted(&unprofiled_recovered, &enabled);
    unchanged(&initial, &enabled, &["node", "python"]);
    unchanged(
        &other,
        &running(&engine.dir, "other").await,
        &["web", "cgi"],
    );
    assert!(http(port(&enabled, "web")).contains("openssl:on"));
    assert!(fastcgi(port(&enabled, "cgi"), &mixed_root).contains("openssl:on"));
    assert!(cli(&engine.dir, "mixed").await.contains("openssl:on"));
    assert!(cli(&engine.dir, "other").await.contains("openssl:off"));
    let active = stackyard_core::php::read_php_profile(&engine.dir, "mixed")
        .unwrap()
        .unwrap();
    // The public engine selection rejects missing artifacts before stopping.
    assert!(apply(&engine.dir, "mixed", &["missing_extension"])
        .await
        .is_err());
    let mut invalid = active.extensions[0].clone();
    invalid.root = engine.dir.clone();
    invalid.library = engine.dir.join("invalid-native.dll");
    fs::write(&invalid.library, b"not a native DLL").unwrap();
    assert!(stackyard_core::php::apply_php_profile(
        &engine.dir,
        "mixed",
        &active.executable,
        &[invalid]
    )
    .is_err());
    unchanged(
        &enabled,
        &running(&engine.dir, "mixed").await,
        &["web", "cgi", "node", "python"],
    );
    assert_eq!(
        stackyard_core::php::read_php_profile(&engine.dir, "mixed").unwrap(),
        Some(active.clone())
    );
    // Candidate probe succeeds, but application readiness requires OpenSSL.
    fs::write(mixed_root.join("needs-openssl"), b"require this extension").unwrap();
    let error = apply(&engine.dir, "mixed", &[])
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("previous profile restored and PHP workers recovered"),
        "{error}"
    );
    let recovered = running(&engine.dir, "mixed").await;
    restarted(&enabled, &recovered);
    unchanged(&initial, &recovered, &["node", "python"]);
    unchanged(
        &other,
        &running(&engine.dir, "other").await,
        &["web", "cgi"],
    );
    assert!(http(port(&recovered, "web")).contains("openssl:on"));
    assert!(fastcgi(port(&recovered, "cgi"), &mixed_root).contains("openssl:on"));
    assert_eq!(
        stackyard_core::php::read_php_profile(&engine.dir, "mixed").unwrap(),
        Some(active.clone())
    );
    fs::remove_file(mixed_root.join("needs-openssl")).unwrap();
    apply(&engine.dir, "mixed", &[]).await.unwrap();
    let disabled = running(&engine.dir, "mixed").await;
    restarted(&recovered, &disabled);
    unchanged(&initial, &disabled, &["node", "python"]);
    assert!(http(port(&disabled, "web")).contains("openssl:off"));
    assert!(fastcgi(port(&disabled, "cgi"), &mixed_root).contains("openssl:off"));
    assert!(cli(&engine.dir, "mixed").await.contains("openssl:off"));
    // Both candidate and old profile are valid, but a broken application means
    // recovery cannot pass readiness. Report that outcome and stop partial PHP.
    fs::write(mixed_root.join("not-ready"), b"force application failure").unwrap();
    let error = apply(&engine.dir, "mixed", &["openssl"])
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("previous profile restored, but PHP worker recovery failed"),
        "{error}"
    );
    let failed = status(&engine.dir, "mixed").await;
    for id in ["web", "cgi"] {
        assert_eq!(
            failed.services.iter().find(|s| s.id == id).unwrap().state,
            "stopped"
        );
        assert!(stackyard_core::port_is_free(port(&disabled, id)));
    }
    unchanged(&initial, &failed, &["node", "python"]);
    unchanged(
        &other,
        &running(&engine.dir, "other").await,
        &["web", "cgi"],
    );
    assert!(stackyard_core::php::read_php_profile(&engine.dir, "mixed")
        .unwrap()
        .unwrap()
        .extensions
        .is_empty());
    assert_eq!(
        fs::read_to_string(mixed_root.join("data.txt")).unwrap(),
        "persistent-data"
    );
}

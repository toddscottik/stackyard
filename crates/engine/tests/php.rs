//! Native PHP/Composer acceptance. Set both STACKYARD_TEST_PHP and
//! STACKYARD_TEST_COMPOSER to explicitly chosen, verified artifact paths.
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use stackyard_core::{prepare_data_dir, roundtrip, Command, Outcome, ProjectView};

struct Engine {
    dir: PathBuf,
    thread: Option<thread::JoinHandle<stackyard_core::Result<()>>>,
}

impl Engine {
    fn start() -> Self {
        let dir = prepare_data_dir(&stackyard_core::disposable_dir("php-engine")).unwrap();
        let owned = dir.clone();
        Self {
            dir,
            thread: Some(thread::spawn(move || stackyard_engine::serve(&owned))),
        }
    }

    async fn ready(&self) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
        loop {
            if roundtrip(&self.dir, Command::Status).await.is_ok() {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "engine did not start"
            );
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    }

    async fn restart(&mut self) {
        roundtrip(&self.dir, Command::Stop).await.unwrap();
        self.thread.take().unwrap().join().unwrap().unwrap();
        let owned = self.dir.clone();
        self.thread = Some(thread::spawn(move || stackyard_engine::serve(&owned)));
        self.ready().await;
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
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn write_project(root: &Path, id: &str, port: u16) {
    fs::create_dir_all(root.join("packages/local")).unwrap();
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
port = {port}
ready_timeout_secs = 5
[site]
service = "web"
host = "{id}.localhost"
"#
        ),
    )
    .unwrap();
    fs::write(root.join("data.txt"), "persistent-project-data").unwrap();
    fs::write(root.join("index.php"), "<?php header('Content-Type: text/plain'); echo extension_loaded('openssl') ? 'openssl:on' : 'openssl:off'; echo '|'.file_get_contents(__DIR__.'/data.txt');").unwrap();
    fs::write(root.join("packages/local/composer.json"), r#"{"name":"stackyard-fixture/local","version":"1.0.0","type":"library","autoload":{"files":["fixture.php"]}}"#).unwrap();
    fs::write(
        root.join("packages/local/fixture.php"),
        "<?php function stackyard_fixture_value() { return 'local-package-ok'; }",
    )
    .unwrap();
    fs::write(root.join("composer.json"), r#"{
  "name":"stackyard-fixture/application",
  "version":"1.0.0",
  "repositories":[{"type":"path","url":"packages/local","options":{"symlink":false}},{"packagist.org":false}],
  "require":{"php":">=8.5","ext-openssl":"*","stackyard-fixture/local":"1.0.0"},
  "config":{"allow-plugins":false}
}"#).unwrap();
}

async fn select(dir: &Path, id: &str, package: &str) {
    roundtrip(
        dir,
        Command::SelectPackage {
            project_id: id.into(),
            package_id: package.into(),
        },
    )
    .await
    .unwrap();
}

async fn import(dir: &Path, root: &Path) {
    roundtrip(
        dir,
        Command::ImportProject {
            path: root.display().to_string(),
        },
    )
    .await
    .unwrap();
}

async fn apply(dir: &Path, id: &str, extensions: &[&str]) -> stackyard_core::php::PhpProfile {
    let outcome = roundtrip(
        dir,
        Command::PhpApplyProfile {
            project_id: id.into(),
            extensions: extensions.iter().map(|s| (*s).into()).collect(),
        },
    )
    .await
    .unwrap();
    let Outcome::Php { view } = outcome else {
        panic!("expected PHP profile");
    };
    view.profile.unwrap()
}

async fn settled(dir: &Path, id: &str) -> ProjectView {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
    loop {
        let Outcome::Project { project } = roundtrip(
            dir,
            Command::ProjectStatus {
                project_id: id.into(),
            },
        )
        .await
        .unwrap() else {
            panic!("expected project");
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
            "project remained starting: {project:?}"
        );
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

async fn task(dir: &Path, command: Command) -> (String, String) {
    let Outcome::Task { id, .. } = roundtrip(dir, command).await.unwrap() else {
        panic!("expected task");
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(25);
    loop {
        let Outcome::Task { state, message, .. } = roundtrip(
            dir,
            Command::DepStatus {
                operation_id: id.clone(),
            },
        )
        .await
        .unwrap() else {
            panic!("expected task status");
        };
        if !matches!(state.as_str(), "running" | "queued") {
            return (state, message);
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "task exceeded deadline"
        );
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

async fn cli(dir: &Path, id: &str, script: &str) -> String {
    let (state, message) = task(
        dir,
        Command::Exec {
            project_id: id.into(),
            service_id: "web".into(),
            args: vec!["php".into(), "-r".into(), script.into()],
        },
    )
    .await;
    assert_eq!(state, "succeeded", "{message}");
    message
}

fn http(port: u16) -> String {
    http_host(port, "127.0.0.1")
}

fn http_host(port: u16, host: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .write_all(
            format!("GET / HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n").as_bytes(),
        )
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

async fn apache_proxy(engine: &Engine) {
    let Some(apache) = std::env::var_os("STACKYARD_TEST_APACHE") else {
        eprintln!("SKIPPED native Apache proxy acceptance: set STACKYARD_TEST_APACHE and STACKYARD_TEST_CURL");
        return;
    };
    let curl = std::env::var_os("STACKYARD_TEST_CURL")
        .expect("set explicit STACKYARD_TEST_CURL for generated-CA HTTPS verification");
    let http_port = free_port();
    let mut https_port = free_port();
    while https_port == http_port {
        https_port = free_port();
    }
    roundtrip(
        &engine.dir,
        Command::SetFrontPorts {
            http_port,
            https_port,
        },
    )
    .await
    .unwrap();
    roundtrip(
        &engine.dir,
        Command::UseExisting {
            role: "apache".into(),
            path: PathBuf::from(apache).display().to_string(),
        },
    )
    .await
    .unwrap();
    select(&engine.dir, "default", "existing-apache").await;
    roundtrip(
        &engine.dir,
        Command::StartProject {
            project_id: "php-second".into(),
        },
    )
    .await
    .unwrap();
    let second = settled(&engine.dir, "php-second").await;
    assert_eq!(second.services[0].state, "running", "{second:?}");
    let mut ca = String::new();
    for id in ["php-first", "php-second"] {
        let Outcome::Front { state, ca_path, .. } = roundtrip(
            &engine.dir,
            Command::PublishProject {
                project_id: id.into(),
            },
        )
        .await
        .unwrap() else {
            panic!("expected Apache status");
        };
        assert_eq!(state, "running");
        ca = ca_path;
    }
    for (id, expected) in [("php-first", "openssl:on"), ("php-second", "openssl:off")] {
        let host = format!("{id}.localhost");
        let response = http_host(http_port, &host);
        assert!(
            response.contains(expected) && response.contains("|persistent-project-data"),
            "Apache HTTP response for {host}: {response}"
        );
        let mut tls_probe = std::process::Command::new(&curl);
        // The disposable private CA publishes no revocation service. Schannel
        // still verifies its supplied CA and hostname with this flag.
        if cfg!(windows) {
            tls_probe.arg("--ssl-no-revoke");
        }
        let result = tls_probe
            .args([
                "--silent",
                "--show-error",
                "--fail",
                "--max-time",
                "5",
                "--noproxy",
                "*",
                "--cacert",
                &ca,
                "--resolve",
                &format!("{host}:{https_port}:127.0.0.1"),
                &format!("https://{host}:{https_port}/"),
            ])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "HTTPS probe failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(String::from_utf8_lossy(&result.stdout)
            .contains(&format!("{expected}|persistent-project-data")));
    }
    roundtrip(
        &engine.dir,
        Command::UnpublishProject {
            project_id: "php-first".into(),
        },
    )
    .await
    .unwrap();
    assert!(http_host(http_port, "php-second.localhost").contains("openssl:off"));
    roundtrip(
        &engine.dir,
        Command::UnpublishProject {
            project_id: "php-second".into(),
        },
    )
    .await
    .unwrap();
    assert!(stackyard_core::port_is_free(http_port));
    assert!(stackyard_core::port_is_free(https_port));
}

#[tokio::test]
async fn selected_php_profiles_composer_http_and_recovery() {
    let (Some(php), Some(composer)) = (
        std::env::var_os("STACKYARD_TEST_PHP"),
        std::env::var_os("STACKYARD_TEST_COMPOSER"),
    ) else {
        eprintln!("SKIPPED native PHP acceptance: set STACKYARD_TEST_PHP and STACKYARD_TEST_COMPOSER; no runtime evidence from this run");
        return;
    };
    let mut engine = Engine::start();
    engine.ready().await;
    roundtrip(
        &engine.dir,
        Command::UseExisting {
            role: "php".into(),
            path: PathBuf::from(&php).display().to_string(),
        },
    )
    .await
    .unwrap();
    // Composer adoption deliberately resolves the explicitly selected PHP.
    select(&engine.dir, "default", "existing-php").await;
    roundtrip(
        &engine.dir,
        Command::UseExisting {
            role: "composer".into(),
            path: PathBuf::from(composer).display().to_string(),
        },
    )
    .await
    .unwrap();
    let first = engine.dir.join("PHP apps/первый project");
    let second = engine.dir.join("PHP apps/second project");
    write_project(&first, "php-first", 0);
    write_project(&second, "php-second", 0);
    for root in [&first, &second] {
        import(&engine.dir, root).await;
    }
    for id in ["php-first", "php-second"] {
        select(&engine.dir, id, "existing-php").await;
        select(&engine.dir, id, "existing-composer").await;
    }
    let active = apply(&engine.dir, "php-first", &["openssl"]).await;
    apply(&engine.dir, "php-second", &[]).await;
    assert!(cli(
        &engine.dir,
        "php-first",
        "echo extension_loaded('openssl') ? 'profile:on' : 'profile:off';"
    )
    .await
    .contains("profile:on"));
    assert!(cli(
        &engine.dir,
        "php-second",
        "echo extension_loaded('openssl') ? 'profile:on' : 'profile:off';"
    )
    .await
    .contains("profile:off"));
    let (state, output) = task(
        &engine.dir,
        Command::DepInstall {
            project_id: "php-first".into(),
            scope: ".".into(),
        },
    )
    .await;
    assert_eq!(state, "succeeded", "{output}");
    assert!(output.contains("check-platform-reqs"), "{output}");
    assert!(first.join("composer.lock").is_file());
    assert!(first.join("vendor/autoload.php").is_file());
    assert!(!second.join("vendor/autoload.php").exists());
    let lock = fs::read(first.join("composer.lock")).unwrap();
    assert!(cli(
        &engine.dir,
        "php-first",
        "require 'vendor/autoload.php'; echo stackyard_fixture_value();"
    )
    .await
    .contains("local-package-ok"));
    let (state, output) = task(
        &engine.dir,
        Command::DepInstall {
            project_id: "php-first".into(),
            scope: ".".into(),
        },
    )
    .await;
    assert_eq!(state, "succeeded", "{output}");
    assert_eq!(fs::read(first.join("composer.lock")).unwrap(), lock);
    let (state, output) = task(
        &engine.dir,
        Command::DepInstall {
            project_id: "php-second".into(),
            scope: ".".into(),
        },
    )
    .await;
    assert_eq!(state, "failed", "{output}");
    assert!(output.contains("openssl"), "{output}");
    assert!(!second.join("vendor/autoload.php").exists());
    assert!(first.join("vendor/autoload.php").is_file());
    // Unknown selection and invalid native bytes must both preserve the active record.
    assert!(roundtrip(
        &engine.dir,
        Command::PhpApplyProfile {
            project_id: "php-first".into(),
            extensions: vec!["missing_extension".into()]
        }
    )
    .await
    .is_err());
    let mut invalid = active.extensions[0].clone();
    let mut mismatch = invalid.clone();
    mismatch.abi.thread_safe = !mismatch.abi.thread_safe;
    assert!(stackyard_core::php::apply_php_profile(
        &engine.dir,
        "php-first",
        Path::new(&active.executable),
        &[mismatch]
    )
    .is_err());
    invalid.root = engine.dir.clone();
    invalid.library = engine.dir.join("invalid-extension.dll");
    fs::write(&invalid.library, b"invalid native bytes").unwrap();
    assert!(stackyard_core::php::apply_php_profile(
        &engine.dir,
        "php-first",
        Path::new(&active.executable),
        &[invalid]
    )
    .is_err());
    assert_eq!(
        stackyard_core::php::read_php_profile(&engine.dir, "php-first").unwrap(),
        Some(active.clone())
    );
    let bad_override = roundtrip(
        &engine.dir,
        Command::Exec {
            project_id: "php-first".into(),
            service_id: "web".into(),
            args: vec![
                "php".into(),
                "-S".into(),
                "127.0.0.1:0".into(),
                "-c".into(),
                "foreign.ini".into(),
            ],
        },
    )
    .await;
    assert!(bad_override.is_err(), "late ini override was accepted");
    for id in ["php-first", "php-second"] {
        roundtrip(
            &engine.dir,
            Command::StartProject {
                project_id: id.into(),
            },
        )
        .await
        .unwrap();
    }
    let first_running = settled(&engine.dir, "php-first").await;
    let second_running = settled(&engine.dir, "php-second").await;
    assert_eq!(
        first_running.services[0].state, "running",
        "{first_running:?}"
    );
    assert_eq!(
        second_running.services[0].state, "running",
        "{second_running:?}"
    );
    let port_first = first_running.services[0].port.unwrap();
    let port_second = second_running.services[0].port.unwrap();
    assert_ne!(port_first, port_second);
    assert!(http(port_first).contains("openssl:on|persistent-project-data"));
    assert!(http(port_second).contains("openssl:off|persistent-project-data"));
    assert!(roundtrip(
        &engine.dir,
        Command::PhpApplyProfile {
            project_id: "php-first".into(),
            extensions: Vec::new()
        }
    )
    .await
    .is_err());
    roundtrip(
        &engine.dir,
        Command::StopProject {
            project_id: "php-first".into(),
        },
    )
    .await
    .unwrap();
    assert!(http(port_second).contains("openssl:off"));
    assert_eq!(
        fs::read_to_string(first.join("data.txt")).unwrap(),
        "persistent-project-data"
    );
    roundtrip(
        &engine.dir,
        Command::RestartProject {
            project_id: "php-first".into(),
        },
    )
    .await
    .unwrap();
    let restarted = settled(&engine.dir, "php-first").await;
    assert_eq!(restarted.services[0].state, "running", "{restarted:?}");
    assert!(
        http(restarted.services[0].port.unwrap()).contains("openssl:on|persistent-project-data")
    );
    engine.restart().await;
    assert_eq!(
        stackyard_core::php::read_php_profile(&engine.dir, "php-first").unwrap(),
        Some(active)
    );
    roundtrip(
        &engine.dir,
        Command::StartProject {
            project_id: "php-first".into(),
        },
    )
    .await
    .unwrap();
    let recovered = settled(&engine.dir, "php-first").await;
    assert_eq!(recovered.services[0].state, "running", "{recovered:?}");
    assert!(
        http(recovered.services[0].port.unwrap()).contains("openssl:on|persistent-project-data")
    );
    apache_proxy(&engine).await;
    // An unrelated listener stays owned and reachable after occupied-port refusal.
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let occupied = listener.local_addr().unwrap().port();
    let busy_root = engine.dir.join("occupied project");
    write_project(&busy_root, "php-occupied", occupied);
    import(&engine.dir, &busy_root).await;
    roundtrip(
        &engine.dir,
        Command::StartProject {
            project_id: "php-occupied".into(),
        },
    )
    .await
    .unwrap();
    let failed = settled(&engine.dir, "php-occupied").await;
    assert_eq!(failed.services[0].state, "failed", "{failed:?}");
    assert!(failed.services[0].detail.contains("in use"), "{failed:?}");
    assert_eq!(listener.local_addr().unwrap().port(), occupied);
    let _connection = TcpStream::connect(("127.0.0.1", occupied)).unwrap();
    assert_eq!(fs::read(first.join("composer.lock")).unwrap(), lock);
}

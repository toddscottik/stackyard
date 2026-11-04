//! Native end-to-end acceptance, gated by explicitly selected verified binaries.
use std::fs;
use std::net::TcpListener;
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
        let dir = prepare_data_dir(&stackyard_core::disposable_dir("php-fastcgi")).unwrap();
        let owned = dir.clone();
        Self {
            dir,
            thread: Some(thread::spawn(move || stackyard_engine::serve(&owned))),
        }
    }
    async fn ready(&self) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
        while roundtrip(&self.dir, Command::Status).await.is_err() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "engine readiness timed out"
            );
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let dir = self.dir.clone();
        let _ = thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(roundtrip(&dir, Command::Stop))
                .ok();
        })
        .join();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        if thread::panicking() {
            eprintln!("Retained failing native fixture: {}", self.dir.display());
        } else {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn write_project(root: &Path, id: &str) {
    fs::create_dir_all(root.join("public")).unwrap();
    fs::write(
        root.join("stackyard.toml"),
        format!(
            r#"schema = 1
name = "{id}"
manager = "composer"
[[service]]
id = "php"
runtime = "php"
command = ["php-cgi", "-b", "127.0.0.1:{{port}}"]
port = 0
ready_timeout_secs = 5
[site]
handler = "php"
service = "php"
root = "public"
host = "{id}.localhost"
"#
        ),
    )
    .unwrap();
    fs::write(
        root.join("public/.htaccess"),
        "RewriteEngine On\nRewriteRule ^front$ index.php?route=rewritten [QSA,L]\n",
    )
    .unwrap();
    fs::write(root.join("public/index.php"), format!(r#"<?php /* SOURCE_NEVER_LEAK */
header('Content-Type: text/plain');
echo '{id}|'.PHP_SAPI.'|openssl:'.(extension_loaded('openssl')?'on':'off');
echo '|method:'.$_SERVER['REQUEST_METHOD'].'|route:'.($_GET['route']??'index');
echo '|value:'.($_POST['value']??'none');
echo '|upload:'.(isset($_FILES['upload']) ? file_get_contents($_FILES['upload']['tmp_name']) : 'none');
echo '|filename:'.basename($_SERVER['SCRIPT_FILENAME']);
"#)).unwrap();
    fs::copy(root.join("public/index.php"), root.join("public/upper.PHP")).unwrap();
    fs::copy(
        root.join("public/index.php"),
        root.join("public/backup.php.bak"),
    )
    .unwrap();
    fs::write(root.join("public/asset.txt"), "static-asset-ok").unwrap();
    fs::write(root.join("outside.php"), "<?php echo 'OUTSIDE_NEVER_LEAK';").unwrap();
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
            "still starting: {project:?}"
        );
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

fn curl(
    curl: &Path,
    host: &str,
    port: u16,
    path: &str,
    ca: Option<&str>,
    extra: &[String],
) -> String {
    let mut command = std::process::Command::new(curl);
    command.args([
        "--silent",
        "--show-error",
        "--max-time",
        "5",
        "--noproxy",
        "*",
        "--resolve",
        &format!("{host}:{port}:127.0.0.1"),
        "--write-out",
        "\nSTATUS:%{http_code}",
    ]);
    if let Some(ca) = ca {
        if cfg!(windows) {
            command.arg("--ssl-no-revoke");
        }
        command.args(["--cacert", ca]);
    }
    let scheme = if ca.is_some() { "https" } else { "http" };
    let output = command
        .args(extra)
        .arg(format!("{scheme}://{host}:{port}{path}"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "curl failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[tokio::test]
async fn native_php_document_roots_rewrite_post_upload_https_and_fail_closed() {
    let (Some(php), Some(apache), Some(curl_path)) = (
        std::env::var_os("STACKYARD_TEST_PHP"),
        std::env::var_os("STACKYARD_TEST_APACHE"),
        std::env::var_os("STACKYARD_TEST_CURL"),
    ) else {
        eprintln!("SKIPPED native FastCGI acceptance: set STACKYARD_TEST_PHP/APACHE/CURL; this run provides no native runtime evidence");
        return;
    };
    let curl_path = PathBuf::from(curl_path);
    let engine = Engine::start();
    engine.ready().await;
    for (role, path) in [("php", php), ("apache", apache)] {
        roundtrip(
            &engine.dir,
            Command::UseExisting {
                role: role.into(),
                path: PathBuf::from(path).display().to_string(),
            },
        )
        .await
        .unwrap();
        roundtrip(
            &engine.dir,
            Command::SelectPackage {
                project_id: "default".into(),
                package_id: format!("existing-{role}"),
            },
        )
        .await
        .unwrap();
    }
    let first = engine.dir.join("PHP roots/первый project");
    let second = engine.dir.join("PHP roots/second project");
    let ids = ["fcgi-first", "fcgi-second"];
    for (id, root, extensions) in [
        (ids[0], &first, vec!["openssl".into()]),
        (ids[1], &second, Vec::new()),
    ] {
        write_project(root, id);
        roundtrip(
            &engine.dir,
            Command::ImportProject {
                path: root.display().to_string(),
            },
        )
        .await
        .unwrap();
        roundtrip(
            &engine.dir,
            Command::SelectPackage {
                project_id: id.into(),
                package_id: "existing-php".into(),
            },
        )
        .await
        .unwrap();
        roundtrip(
            &engine.dir,
            Command::PhpApplyProfile {
                project_id: id.into(),
                extensions,
            },
        )
        .await
        .unwrap();
        roundtrip(
            &engine.dir,
            Command::StartProject {
                project_id: id.into(),
            },
        )
        .await
        .unwrap();
        let project = settled(&engine.dir, id).await;
        assert_eq!(project.services[0].state, "running", "{project:?}");
    }
    let http = free_port();
    let mut https = free_port();
    while https == http {
        https = free_port();
    }
    roundtrip(
        &engine.dir,
        Command::SetFrontPorts {
            http_port: http,
            https_port: https,
        },
    )
    .await
    .unwrap();
    let mut ca = String::new();
    for id in ids {
        let Outcome::Front {
            state,
            ca_path,
            sites,
            ..
        } = roundtrip(
            &engine.dir,
            Command::PublishProject {
                project_id: id.into(),
            },
        )
        .await
        .unwrap()
        else {
            panic!("expected front");
        };
        assert_eq!(state, "running");
        assert!(sites
            .iter()
            .any(|site| site.project_id == id && site.mode == "php"));
        ca = ca_path;
    }
    for (id, expected) in [(ids[0], "on"), (ids[1], "off")] {
        let host = format!("{id}.localhost");
        for (port, ca) in [(http, None), (https, Some(ca.as_str()))] {
            let response = curl(&curl_path, &host, port, "/front", ca, &[]);
            assert!(
                response.contains(&format!(
                    "{id}|cgi-fcgi|openssl:{expected}|method:GET|route:rewritten"
                )),
                "{response}"
            );
            assert!(
                response.contains("|filename:index.php") && response.contains("STATUS:200"),
                "{response}"
            );
            assert!(!response.contains("SOURCE_NEVER_LEAK"));
        }
        let response = curl(
            &curl_path,
            &host,
            http,
            "/",
            None,
            &["--data-urlencode".into(), "value=posted value".into()],
        );
        assert!(
            response.contains("method:POST|route:index|value:posted value"),
            "{response}"
        );
        let upload = engine.dir.join("upload.txt");
        fs::write(&upload, "uploaded-content").unwrap();
        let response = curl(
            &curl_path,
            &host,
            https,
            "/front",
            Some(&ca),
            &[
                "--form".into(),
                "value=multipart".into(),
                "--form".into(),
                format!("upload=@{}", upload.display()),
            ],
        );
        assert!(
            response
                .contains("method:POST|route:rewritten|value:multipart|upload:uploaded-content"),
            "{response}"
        );
        assert!(curl(&curl_path, &host, http, "/asset.txt", None, &[]).contains("static-asset-ok"));
        let upper = curl(&curl_path, &host, http, "/upper.PHP", None, &[]);
        assert!(
            upper.contains("cgi-fcgi") && !upper.contains("SOURCE_NEVER_LEAK"),
            "{upper}"
        );
        let backup = curl(&curl_path, &host, http, "/backup.php.bak", None, &[]);
        assert!(
            backup.contains("STATUS:403") && !backup.contains("SOURCE_NEVER_LEAK"),
            "{backup}"
        );
        let outside = curl(&curl_path, &host, http, "/../outside.php", None, &[]);
        assert!(!outside.contains("OUTSIDE_NEVER_LEAK"), "{outside}");
    }
    let live_config = engine.dir.join("apache/httpd.conf");
    let previous = fs::read(&live_config).unwrap();
    let probe_front = stackyard_engine::Front::new(engine.dir.clone());
    assert!(probe_front
        .install_config_text("ThisIsNotAnApacheDirective\n")
        .is_err());
    assert_eq!(fs::read(&live_config).unwrap(), previous);
    let manifest_path = first.join("stackyard.toml");
    let original = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        original.replace("root = \"public\"", "root = \"missing-root\""),
    )
    .unwrap();
    assert!(roundtrip(
        &engine.dir,
        Command::PublishProject {
            project_id: ids[0].into()
        }
    )
    .await
    .is_err());
    assert_eq!(fs::read(&live_config).unwrap(), previous);
    fs::write(&manifest_path, &original).unwrap();
    // A persisted PHP publication must fail closed even if its manifest is
    // edited to a static document root without explicitly republishing it.
    let static_edit = original
        .replace("handler = \"php\"\n", "")
        .replace("service = \"php\"\n", "");
    fs::write(&manifest_path, static_edit).unwrap();
    roundtrip(&engine.dir, Command::RestartFront).await.unwrap();
    let source_guard = curl(
        &curl_path,
        "fcgi-first.localhost",
        http,
        "/index.php",
        None,
        &[],
    );
    assert!(
        source_guard.contains("STATUS:503") && !source_guard.contains("SOURCE_NEVER_LEAK"),
        "{source_guard}"
    );
    fs::write(&manifest_path, original).unwrap();
    roundtrip(&engine.dir, Command::RestartFront).await.unwrap();
    assert!(
        curl(&curl_path, "fcgi-second.localhost", http, "/", None, &[]).contains("openssl:off")
    );
    roundtrip(
        &engine.dir,
        Command::StopProject {
            project_id: ids[0].into(),
        },
    )
    .await
    .unwrap();
    for (port, ca) in [(http, None), (https, Some(ca.as_str()))] {
        let response = curl(
            &curl_path,
            "fcgi-first.localhost",
            port,
            "/index.php",
            ca,
            &[],
        );
        assert!(
            response.contains("STATUS:503")
                && !response.contains("SOURCE_NEVER_LEAK")
                && !response.contains("<?php"),
            "{response}"
        );
    }
    assert!(
        curl(&curl_path, "fcgi-second.localhost", http, "/", None, &[])
            .contains("cgi-fcgi|openssl:off")
    );
    roundtrip(
        &engine.dir,
        Command::StopProject {
            project_id: ids[1].into(),
        },
    )
    .await
    .unwrap();
    for id in ids {
        roundtrip(
            &engine.dir,
            Command::UnpublishProject {
                project_id: id.into(),
            },
        )
        .await
        .unwrap();
    }
    assert!(stackyard_core::port_is_free(http));
    assert!(stackyard_core::port_is_free(https));
}

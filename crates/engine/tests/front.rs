//! Shared Apache on this machine. Windows and macOS runners do not compile this file's
//! Linux-only test; those launches stay pending.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
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
        let dir = temp_dir("front");
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
            if roundtrip(&self.dir, EngineCommand::FrontStatus)
                .await
                .is_ok()
            {
                return;
            }
            if tokio::time::Instant::now() > deadline {
                panic!("engine did not accept a front status request");
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

fn free_pair(start: u16) -> (u16, u16) {
    for port in (start..start + 40).step_by(2) {
        if stackyard_core::port_is_free(port) && stackyard_core::port_is_free(port + 1) {
            return (port, port + 1);
        }
    }
    panic!("no free high port pair from {start}");
}

fn write_project(root: &Path, body: &str) {
    fs::create_dir_all(root).unwrap();
    fs::write(root.join("stackyard.toml"), body).unwrap();
}

fn curl(args: &[&str]) -> String {
    let output = Command::new("curl").args(args).output().expect("curl");
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn upstream(port: u16) {
    thread::spawn(move || {
        let listener = TcpListener::bind(("127.0.0.1", port)).expect("upstream");
        for stream in listener.incoming() {
            let mut stream = match stream {
                Ok(stream) => stream,
                Err(_) => continue,
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut buf = Vec::new();
            let mut tmp = [0_u8; 2048];
            loop {
                match stream.read(&mut tmp) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&tmp[..n]);
                        if buf.windows(4).any(|window| window == b"\r\n\r\n") {
                            break;
                        }
                    }
                }
            }
            let text = String::from_utf8_lossy(&buf);
            let response = if text.contains("Upgrade: websocket") {
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n"
                    .to_string()
            } else if text.starts_with("GET /go ") {
                format!(
                    "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{port}/land\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
            } else {
                let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
                let body = if body.is_empty() {
                    "upstream-ok".to_string()
                } else {
                    body
                };
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
            };
            let _ = stream.write_all(response.as_bytes());
        }
    });
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn shared_apache_isolates_sites_and_keeps_a_bad_config_off_the_live_one() {
    let apache = Path::new("/usr/sbin/apache2");
    assert!(apache.is_file(), "apache2 is required for this check");
    let engine = Engine::start();
    engine.ready().await;

    let trust = temp_dir("trust");
    fs::write(trust.join("other.pem"), "not-stackyard").unwrap();
    let missing = roundtrip(
        &engine.dir,
        EngineCommand::TrustRemove {
            directory: trust.display().to_string(),
        },
    )
    .await;
    assert!(missing.is_err(), "a foreign file must not be deleted");
    assert_eq!(
        fs::read_to_string(trust.join("other.pem")).unwrap(),
        "not-stackyard"
    );

    let installed = roundtrip(
        &engine.dir,
        EngineCommand::TrustInstall {
            directory: trust.display().to_string(),
        },
    )
    .await
    .expect("trust install");
    assert!(matches!(installed, Outcome::Front { .. }));
    assert!(fs::read_to_string(trust.join("stackyard-ca.pem"))
        .unwrap()
        .contains("BEGIN CERTIFICATE"));
    roundtrip(
        &engine.dir,
        EngineCommand::TrustRemove {
            directory: trust.display().to_string(),
        },
    )
    .await
    .expect("trust remove");
    assert!(!trust.join("stackyard-ca.pem").exists());
    assert_eq!(
        fs::read_to_string(trust.join("other.pem")).unwrap(),
        "not-stackyard"
    );

    let held = TcpListener::bind(("127.0.0.1", 18190)).expect("hold 18190");
    let busy = roundtrip(
        &engine.dir,
        EngineCommand::SetFrontPorts {
            http_port: 18190,
            https_port: 18191,
        },
    )
    .await;
    let err = busy.expect_err("occupied port").to_string();
    assert!(err.contains("18190"), "{err}");
    assert!(err.contains("did not stop"), "{err}");
    assert!(
        TcpListener::bind(("127.0.0.1", 18190)).is_err(),
        "the program already listening was still there"
    );
    drop(held);

    let (http_port, https_port) = free_pair(18192);
    roundtrip(
        &engine.dir,
        EngineCommand::SetFrontPorts {
            http_port,
            https_port,
        },
    )
    .await
    .expect("ports");
    roundtrip(
        &engine.dir,
        EngineCommand::UseExisting {
            role: "apache".to_string(),
            path: apache.display().to_string(),
        },
    )
    .await
    .expect("adopt apache");

    let root = temp_dir("sites");
    let site_a = root.join("site-a");
    let site_b = root.join("site-b");
    let rewrite = root.join("rewrite");
    let proxy = root.join("proxy");
    write_project(
        &site_a,
        "schema = 1\nname = \"site-a\"\n\n[site]\nroot = \"public\"\nhost = \"site-a.localhost\"\n",
    );
    fs::create_dir_all(site_a.join("public")).unwrap();
    fs::write(site_a.join("public").join("secret-a.txt"), "site-a-only").unwrap();
    write_project(
        &site_b,
        "schema = 1\nname = \"site-b\"\n\n[site]\nroot = \"public\"\nhost = \"site-b.localhost\"\n",
    );
    fs::create_dir_all(site_b.join("public")).unwrap();
    fs::write(site_b.join("public").join("other.txt"), "site-b-only").unwrap();
    write_project(
        &rewrite,
        "schema = 1\nname = \"rewrite\"\n\n[site]\nroot = \"public\"\nhost = \"rewrite.localhost\"\n",
    );
    fs::create_dir_all(rewrite.join("public")).unwrap();
    fs::write(rewrite.join("public").join("hello.txt"), "rewrite-ok").unwrap();
    fs::write(
        rewrite.join("public").join(".htaccess"),
        "RewriteEngine On\nRewriteRule ^hello$ hello.txt [L]\n",
    )
    .unwrap();
    let upstream_port = 18240_u16;
    assert!(stackyard_core::port_is_free(upstream_port));
    upstream(upstream_port);
    thread::sleep(Duration::from_millis(50));
    write_project(
        &proxy,
        &format!(
            "schema = 1\nname = \"proxy-app\"\n\n[[service]]\nid = \"web\"\nruntime = \"node\"\ncommand = [\"node\", \"server.js\"]\nport = {upstream_port}\n\n[site]\nservice = \"web\"\nhost = \"proxy.localhost\"\n"
        ),
    );

    for project in [&site_a, &site_b, &rewrite, &proxy] {
        roundtrip(
            &engine.dir,
            EngineCommand::ImportProject {
                path: project.display().to_string(),
            },
        )
        .await
        .expect("import");
    }
    for id in ["site-a", "site-b", "rewrite", "proxy-app"] {
        let view = roundtrip(
            &engine.dir,
            EngineCommand::PublishProject {
                project_id: id.to_string(),
            },
        )
        .await
        .unwrap_or_else(|err| panic!("publish {id}: {err}"));
        let Outcome::Front { restart_impact, .. } = &view else {
            panic!("expected front view");
        };
        assert!(restart_impact.contains("every published site"));
    }

    let base = format!("http://127.0.0.1:{http_port}");
    let secret = curl(&[
        "-s",
        "-H",
        "Host: site-a.localhost",
        &format!("{base}/secret-a.txt"),
    ]);
    assert_eq!(secret, "site-a-only");
    let leak = curl(&[
        "-s",
        "-D",
        "-",
        "-o",
        "/tmp/stackyard-leak-body",
        "-H",
        "Host: site-b.localhost",
        &format!("{base}/secret-a.txt"),
    ]);
    let leak_body = fs::read_to_string("/tmp/stackyard-leak-body").unwrap_or_default();
    assert!(!leak_body.contains("site-a-only"), "{leak}\n{leak_body}");
    assert!(leak.contains("404"), "{leak}");
    let other = curl(&[
        "-s",
        "-H",
        "Host: site-b.localhost",
        &format!("{base}/other.txt"),
    ]);
    assert_eq!(other, "site-b-only");
    let rewritten = curl(&[
        "-s",
        "-H",
        "Host: rewrite.localhost",
        &format!("{base}/hello"),
    ]);
    assert_eq!(rewritten, "rewrite-ok");

    let posted = curl(&[
        "-s",
        "-H",
        "Host: proxy.localhost",
        "-d",
        "hello-post",
        &format!("{base}/"),
    ]);
    assert_eq!(posted, "hello-post", "{posted}");
    let redirect = curl(&[
        "-s",
        "-D",
        "-",
        "-o",
        "/dev/null",
        "-H",
        "Host: proxy.localhost",
        &format!("{base}/go"),
    ]);
    assert!(redirect.contains("proxy.localhost"), "{redirect}");
    assert!(
        !redirect.contains(&format!("127.0.0.1:{upstream_port}")),
        "{redirect}"
    );

    let mut socket = TcpStream::connect(("127.0.0.1", http_port)).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let upgrade = "GET / HTTP/1.1\r\nHost: proxy.localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n";
    socket.write_all(upgrade.as_bytes()).unwrap();
    let mut received = Vec::new();
    let mut tmp = [0_u8; 1024];
    loop {
        match socket.read(&mut tmp) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                received.extend_from_slice(&tmp[..n]);
                if received.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
        }
    }
    let received = String::from_utf8_lossy(&received);
    assert!(received.contains("101"), "{received}");

    let status = roundtrip(&engine.dir, EngineCommand::FrontStatus)
        .await
        .unwrap();
    let Outcome::Front {
        ca_path,
        resolves_localhost,
        ..
    } = status
    else {
        panic!("status");
    };
    assert!(
        !resolves_localhost,
        "this Linux did not resolve *.localhost"
    );
    let https = curl(&[
        "-s",
        "--cacert",
        &ca_path,
        "--resolve",
        &format!("rewrite.localhost:{https_port}:127.0.0.1"),
        &format!("https://rewrite.localhost:{https_port}/hello.txt"),
    ]);
    assert_eq!(https, "rewrite-ok", "{https}");

    let hosts = root.join("hosts");
    fs::write(&hosts, "10.9.9.9 kept.example\n").unwrap();
    roundtrip(
        &engine.dir,
        EngineCommand::WriteHosts {
            path: hosts.display().to_string(),
        },
    )
    .await
    .expect("hosts");
    let hosts_text = fs::read_to_string(&hosts).unwrap();
    assert!(hosts_text.contains("kept.example"));
    assert!(hosts_text.contains("site-a.localhost # stackyard"));
    assert!(!hosts_text.contains("10.9.9.9 site-a"));

    let broken = stackyard_engine::Front::new(engine.dir.clone());
    let err = broken
        .install_config_text("Listen nope\n")
        .unwrap_err()
        .to_string();
    assert!(err.contains("previous sites"), "{err}");
    let still = curl(&[
        "-s",
        "-H",
        "Host: site-a.localhost",
        &format!("{base}/secret-a.txt"),
    ]);
    assert_eq!(still, "site-a-only");

    let _ = fs::remove_dir_all(&trust);
    let _ = fs::remove_dir_all(&root);
}

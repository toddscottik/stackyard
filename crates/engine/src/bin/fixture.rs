use std::env;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("-v") => println!("v1.0.0"),
        Some("-c") => println!("3.12.0"),
        Some("--version") => println!("1.0.0"),
        Some("serve") => serve(false),
        Some("serve-child") => serve(true),
        Some("fail") => {
            eprintln!("startup failed: missing table");
            std::process::exit(1);
        }
        Some("crash") => crash(),
        Some("sleep") => thread::sleep(Duration::from_secs(120)),
        Some("echo") => println!("service-ok"),
        Some("-m") => python_module(&args),
        Some(other) => {
            eprintln!("unknown fixture command {other}");
            std::process::exit(2);
        }
        None => std::process::exit(2),
    }
}

fn serve(spawn_child: bool) {
    if spawn_child {
        let child = Command::new(env::current_exe().expect("fixture exe"))
            .arg("sleep")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("child");
        fs::write("child.pid", child.id().to_string()).expect("child pid");
        // The process group, not this handle, owns the child lifetime.
        std::mem::forget(child);
    }
    let port: u16 = env::var("PORT")
        .expect("PORT")
        .parse()
        .expect("port number");
    let listener = TcpListener::bind(("127.0.0.1", port)).unwrap_or_else(|err| {
        eprintln!("fixture bind {port}: {err}");
        std::process::exit(1);
    });
    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(stream) => stream,
            Err(_) => continue,
        };
        let mut buf = [0_u8; 1024];
        let _ = stream.read(&mut buf);
        let body = fs::read_to_string("message.txt").unwrap_or_else(|_| "ok".to_string());
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
    }
}

fn crash() {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("runs.txt")
        .expect("runs");
    writeln!(file, "run").expect("write");
    eprintln!("crash: boom");
    std::process::exit(1);
}

fn python_module(args: &[String]) {
    if args.get(1).map(String::as_str) == Some("venv") {
        let dest = PathBuf::from(args.get(2).map(String::as_str).unwrap_or("."));
        let bin = if cfg!(windows) {
            dest.join("Scripts")
        } else {
            dest.join("bin")
        };
        fs::create_dir_all(&bin).expect("venv bin");
        let target = if cfg!(windows) {
            bin.join("python.exe")
        } else {
            bin.join("python")
        };
        let source = env::current_exe().expect("exe");
        fs::copy(&source, &target).expect("copy python");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&target).expect("meta").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&target, permissions).expect("mode");
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("pip") {
        if args.iter().any(|arg| arg == "--version") {
            println!("pip 1.0.0");
            return;
        }
        if PathBuf::from("pip-slow").is_file() {
            thread::sleep(Duration::from_secs(30));
        }
        if PathBuf::from("pip-fail").is_file() {
            eprintln!("pip failed: no distribution");
            std::process::exit(1);
        }
        fs::write("pip-installed", b"installed").expect("marker");
        println!("installed");
        return;
    }
    eprintln!("unsupported python module");
    std::process::exit(2);
}

//! Bounded readiness checks. HTTP status is observed without following redirects;
//! TCP only connects; a process worker never needs a listening endpoint.
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use stackyard_core::{log_tail, ReadinessSpec, ServiceSpec};

use crate::group::GroupProcess;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Check {
    pub ready: bool,
    pub http_status: Option<u16>,
}

pub(crate) enum Wait {
    Ready {
        http_status: Option<u16>,
    },
    Cancelled {
        http_status: Option<u16>,
    },
    Exited {
        detail: String,
        reaped: bool,
        http_status: Option<u16>,
    },
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn wait_ready(
    group: &mut GroupProcess,
    service: &ServiceSpec,
    port: u16,
    log_path: &Path,
    cancel: &AtomicBool,
    shutdown: &AtomicBool,
    stop: &AtomicBool,
) -> Wait {
    let started = Instant::now();
    let timeout = match &service.readiness {
        ReadinessSpec::Http { timeout_secs, .. } | ReadinessSpec::Tcp { timeout_secs } => {
            Duration::from_secs(*timeout_secs)
        }
        ReadinessSpec::Process { grace_ms } => Duration::from_millis(*grace_ms),
    };
    let deadline = started + timeout;
    let cancelled = || {
        cancel.load(Ordering::Relaxed)
            || shutdown.load(Ordering::Relaxed)
            || stop.load(Ordering::Acquire)
    };
    let mut http_status = None;
    loop {
        if cancelled() {
            return Wait::Cancelled { http_status };
        }
        match group.child.try_wait() {
            Ok(Some(status)) => {
                group.finish_io();
                return Wait::Exited {
                    detail: format!(
                        "exited {status} before readiness\n{}",
                        log_tail(log_path, 800)
                    ),
                    reaped: true,
                    http_status,
                };
            }
            Err(err) => {
                return Wait::Exited {
                    detail: err.to_string(),
                    reaped: false,
                    http_status,
                };
            }
            Ok(None) => {}
        }
        let check = match &service.readiness {
            ReadinessSpec::Process { .. } => Check {
                ready: Instant::now() >= deadline,
                http_status: None,
            },
            ReadinessSpec::Tcp { .. } => Check {
                ready: port != 0 && tcp_ready(port, deadline),
                http_status: None,
            },
            ReadinessSpec::Http { path, statuses, .. } => {
                let status = if port == 0 {
                    None
                } else {
                    http_status_line(port, path, deadline, &cancelled)
                };
                Check {
                    ready: status.is_some_and(|status| statuses.contains(&status)),
                    http_status: status,
                }
            }
        };
        if check.http_status.is_some() {
            http_status = check.http_status;
        }
        if cancelled() {
            return Wait::Cancelled { http_status };
        }
        if check.ready {
            return Wait::Ready { http_status };
        }
        if Instant::now() >= deadline {
            return Wait::Exited {
                detail: format!(
                    "readiness {:?} failed on {} within {}ms; last HTTP status: {}\n{}",
                    service.readiness,
                    if port == 0 {
                        "a portless process".into()
                    } else {
                        format!("127.0.0.1:{port}")
                    },
                    timeout.as_millis(),
                    http_status.map_or_else(|| "not observed".into(), |status| status.to_string()),
                    log_tail(log_path, 800)
                ),
                reaped: false,
                http_status,
            };
        }
        thread::sleep(
            Duration::from_millis(40).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
}

fn remaining(deadline: Instant) -> Option<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .map(|duration| duration.min(Duration::from_millis(200)))
}

fn tcp_ready(port: u16, deadline: Instant) -> bool {
    remaining(deadline).is_some_and(|timeout| {
        TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], port)), timeout).is_ok()
    })
}

fn http_status_line(
    port: u16,
    path: &str,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> Option<u16> {
    let mut stream = TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], port)),
        remaining(deadline)?,
    )
    .ok()?;
    stream.set_write_timeout(Some(remaining(deadline)?)).ok()?;
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .ok()?;
    let mut line = Vec::with_capacity(64);
    while line.len() < 256 && !cancelled() {
        stream.set_read_timeout(Some(remaining(deadline)?)).ok()?;
        let mut byte = [0];
        stream.read_exact(&mut byte).ok()?;
        line.push(byte[0]);
        if line.ends_with(b"\r\n") {
            return parse_status(&line);
        }
        if byte[0] == b'\n' {
            return None;
        }
    }
    None
}

fn parse_status(line: &[u8]) -> Option<u16> {
    let text = std::str::from_utf8(line).ok()?;
    let mut fields = text.split_ascii_whitespace();
    if !matches!(fields.next()?, "HTTP/1.0" | "HTTP/1.1") {
        return None;
    }
    let code = fields.next()?;
    if code.len() != 3 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    code.parse()
        .ok()
        .filter(|status| (100..600).contains(status))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn status_parser_rejects_malformed_protocol_or_code() {
        assert_eq!(parse_status(b"HTTP/1.1 302 Found\r\n"), Some(302));
        assert_eq!(parse_status(b"HTTP/1.0 503 Unavailable\r\n"), Some(503));
        for bad in [
            b"other 200 OK\r\n".as_slice(),
            b"HTTP/1.1 20 OK\r\n",
            b"HTTP/1.1 999 No\r\n",
        ] {
            assert_eq!(parse_status(bad), None);
        }
    }

    #[test]
    fn raw_http_status_is_not_followed_to_location() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 512];
            let count = stream.read(&mut request).unwrap();
            assert!(count > 0);
            stream.write_all(b"HTTP/1.1 307 Temporary Redirect\r\nLocation: http://127.0.0.1:1/never-follow\r\nContent-Length: 0\r\n\r\n").unwrap();
        });
        assert_eq!(
            http_status_line(port, "/", Instant::now() + Duration::from_secs(1), &|| {
                false
            }),
            Some(307)
        );
        server.join().unwrap();
    }

    #[test]
    fn tcp_probe_does_not_read_http_or_require_a_path() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let _accepted = listener.accept().unwrap();
        });
        assert!(tcp_ready(port, Instant::now() + Duration::from_secs(1)));
        server.join().unwrap();
    }

    #[test]
    fn oversized_or_silent_status_is_bounded() {
        for oversized in [true, false] {
            let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 512];
                let count = stream.read(&mut request).unwrap();
                assert!(count > 0);
                if oversized {
                    stream.write_all(&[b'X'; 300]).unwrap();
                } else {
                    thread::sleep(Duration::from_millis(150));
                }
            });
            let started = Instant::now();
            assert_eq!(
                http_status_line(port, "/", started + Duration::from_millis(80), &|| false),
                None
            );
            assert!(started.elapsed() < Duration::from_secs(1));
            server.join().unwrap();
        }
    }
}

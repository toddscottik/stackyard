//! Explicit endpoint checks only: this module never adopts or stops a server.

use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

const MAX_LINE: usize = 512;
const PROBE_VALUE: &str = "stackyard-cache-probe";
const PROBE_TTL: u32 = 30;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MemcachedEndpoint {
    /// Numeric IP avoids DNS resolution outside the operation's deadline.
    pub address: IpAddr,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemcachedProbe {
    pub endpoint: MemcachedEndpoint,
    pub version: String,
    pub roundtrip_verified: bool,
    pub cleanup_verified: bool,
}

/// Windows uses the guarded engine provider. Other targets require independent
/// artifact/dependency/relocation evidence, not inference from Windows.
pub fn managed_memcached_unavailable(target: &str) -> Option<&'static str> {
    match target {
        "windows-x64" => None,
        "macos-arm64" => Some("Homebrew publishes a Sonoma arm64 Memcached bottle, but its libevent/OpenSSL dependency closure, relocation and macOS 14.5 launch are unverified. Managed installation is unavailable; an existing endpoint is separate."),
        _ => Some("No approved Memcached server artifact for this target."),
    }
}

fn failed(detail: impl AsRef<str>) -> Error {
    Error::OperationFailed(format!("Memcached endpoint probe: {}", detail.as_ref()))
}

struct Wire {
    stream: TcpStream,
    deadline: Instant,
}

impl Wire {
    fn connect(endpoint: MemcachedEndpoint, timeout: Duration) -> Result<Self> {
        let deadline = Instant::now() + timeout;
        let stream =
            TcpStream::connect_timeout(&SocketAddr::new(endpoint.address, endpoint.port), timeout)
                .map_err(|err| {
                    failed(format!(
                        "connect to {}:{} failed: {err}",
                        endpoint.address, endpoint.port
                    ))
                })?;
        Ok(Self { stream, deadline })
    }

    fn remaining(&self) -> Result<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| failed("operation deadline exceeded"))
    }

    fn send(&mut self, mut bytes: &[u8]) -> Result<()> {
        while !bytes.is_empty() {
            self.stream
                .set_write_timeout(Some(self.remaining()?))
                .map_err(|err| Error::io("set Memcached write timeout", err))?;
            let count = self
                .stream
                .write(bytes)
                .map_err(|err| failed(format!("write failed: {err}")))?;
            if count == 0 {
                return Err(failed("endpoint closed while writing"));
            }
            bytes = &bytes[count..];
        }
        Ok(())
    }

    fn line(&mut self) -> Result<String> {
        let mut bytes = Vec::with_capacity(80);
        while bytes.len() < MAX_LINE {
            self.stream
                .set_read_timeout(Some(self.remaining()?))
                .map_err(|err| Error::io("set Memcached read timeout", err))?;
            let mut byte = [0];
            self.stream
                .read_exact(&mut byte)
                .map_err(|err| failed(format!("response read failed: {err}")))?;
            bytes.push(byte[0]);
            if byte[0] == b'\n' {
                if !bytes.ends_with(b"\r\n") {
                    return Err(failed("response requires CRLF"));
                }
                bytes.truncate(bytes.len() - 2);
                if !bytes.iter().all(|byte| matches!(byte, 32..=126)) {
                    return Err(failed("response contains non-ASCII or control bytes"));
                }
                return String::from_utf8(bytes).map_err(|_| failed("invalid ASCII response"));
            }
        }
        Err(failed("response line exceeds 512 bytes"))
    }

    fn expect(&mut self, expected: &str) -> Result<()> {
        if self.line()? != expected {
            // Do not reflect arbitrary endpoint output into logs or diagnostics.
            return Err(failed(format!("unexpected response; expected {expected}")));
        }
        Ok(())
    }

    fn cleanup(&mut self, key: &str) -> Result<()> {
        self.send(format!("delete {key}\r\n").as_bytes())?;
        let reply = self.line()?;
        if reply != "DELETED" && reply != "NOT_FOUND" {
            return Err(failed("probe key cleanup was rejected"));
        }
        self.send(format!("get {key}\r\n").as_bytes())?;
        self.expect("END")
    }
}

/// Intentionally writes a unique temporary key to an explicitly selected endpoint.
/// Plain TCP text protocol only (no TLS/SASL); timeout is 1 ms..=30 s for the
/// complete exchange, with up to 250 ms extra best-effort cleanup on failure.
/// This does not establish native runtime or PHP extension compatibility.
pub fn probe_memcached(endpoint: MemcachedEndpoint, timeout: Duration) -> Result<MemcachedProbe> {
    if endpoint.port == 0 || endpoint.address.is_unspecified() || endpoint.address.is_multicast() {
        return Err(failed(
            "select a specific unicast IP address and nonzero port",
        ));
    }
    if timeout < Duration::from_millis(1) || timeout > Duration::from_secs(30) {
        return Err(failed("timeout must be between 1 ms and 30 seconds"));
    }
    let mut random = [0; 16];
    getrandom::getrandom(&mut random)
        .map_err(|err| failed(format!("probe key entropy failed: {err}")))?;
    let key = format!("stackyard:probe:{}", hex::encode(random));
    let mut wire = Wire::connect(endpoint, timeout)?;
    wire.send(b"version\r\n")?;
    let version = wire.line()?;
    let version = version
        .strip_prefix("VERSION ")
        .filter(|version| !version.is_empty() && !version.contains(' '))
        .ok_or_else(|| failed("endpoint did not return a Memcached version"))?
        .to_owned();
    // Reserve without overwriting even in the unlikely event of a key collision.
    wire.send(
        format!(
            "add {key} 0 {PROBE_TTL} {}\r\n{PROBE_VALUE}\r\n",
            PROBE_VALUE.len()
        )
        .as_bytes(),
    )?;
    wire.expect("STORED").map_err(|err| {
        failed(format!("{err}; reservation unconfirmed; any accepted probe key expires within {PROBE_TTL} seconds"))
    })?;
    let result = (|| {
        wire.send(
            format!(
                "set {key} 0 {PROBE_TTL} {}\r\n{PROBE_VALUE}\r\n",
                PROBE_VALUE.len()
            )
            .as_bytes(),
        )?;
        wire.expect("STORED")?;
        wire.send(format!("get {key}\r\n").as_bytes())?;
        wire.expect(&format!("VALUE {key} 0 {}", PROBE_VALUE.len()))?;
        wire.expect(PROBE_VALUE)?;
        wire.expect("END")?;
        wire.cleanup(&key)
    })();
    if let Err(err) = result {
        let cleanup = Wire::connect(endpoint, Duration::from_millis(250))
            .and_then(|mut cleanup| cleanup.cleanup(&key));
        return Err(failed(format!(
            "{err}; cleanup {}; probe key expires within {PROBE_TTL} seconds",
            if cleanup.is_ok() {
                "verified"
            } else {
                "unconfirmed"
            }
        )));
    }
    Ok(MemcachedProbe {
        endpoint,
        version,
        roundtrip_verified: true,
        cleanup_verified: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::thread;

    fn fixture(
        action: impl FnOnce(TcpStream) + Send + 'static,
    ) -> (MemcachedEndpoint, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = MemcachedEndpoint {
            address: "127.0.0.1".parse().unwrap(),
            port: listener.local_addr().unwrap().port(),
        };
        (
            endpoint,
            thread::spawn(move || {
                action(listener.accept().unwrap().0);
            }),
        )
    }

    #[test]
    fn fixture_roundtrip_and_cleanup() {
        let (endpoint, server) = fixture(|stream| {
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut reader = BufReader::new(stream);
            let mut key = String::new();
            for step in 0..6 {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let reply = match step {
                    0 => {
                        assert_eq!(line, "version\r\n");
                        "VERSION fixture-only\r\n".to_owned()
                    }
                    1 | 2 => {
                        let fields: Vec<_> = line.split_whitespace().collect();
                        assert_eq!(fields[0], if step == 1 { "add" } else { "set" });
                        if step == 1 {
                            key = fields[1].to_owned();
                        }
                        assert_eq!(fields[1], key);
                        assert_eq!(fields[3], "30");
                        line.clear();
                        reader.read_line(&mut line).unwrap();
                        assert_eq!(line, format!("{PROBE_VALUE}\r\n"));
                        "STORED\r\n".to_owned()
                    }
                    3 => {
                        assert_eq!(line, format!("get {key}\r\n"));
                        format!(
                            "VALUE {key} 0 {}\r\n{PROBE_VALUE}\r\nEND\r\n",
                            PROBE_VALUE.len()
                        )
                    }
                    4 => {
                        assert_eq!(line, format!("delete {key}\r\n"));
                        "DELETED\r\n".to_owned()
                    }
                    5 => {
                        assert_eq!(line, format!("get {key}\r\n"));
                        "END\r\n".to_owned()
                    }
                    _ => break,
                };
                reader.get_mut().write_all(reply.as_bytes()).unwrap();
            }
        });
        let probe = probe_memcached(endpoint, Duration::from_secs(2)).unwrap();
        assert_eq!(probe.version, "fixture-only");
        assert!(probe.roundtrip_verified && probe.cleanup_verified);
        server.join().unwrap();
    }

    #[test]
    fn rejects_oversized_malformed_and_error_responses() {
        for response in [
            vec![b'x'; MAX_LINE + 1],
            b"VERSION bad\n".to_vec(),
            b"SERVER_ERROR unavailable\r\n".to_vec(),
            b"VERSION \r\n".to_vec(),
        ] {
            let (endpoint, server) = fixture(move |mut stream| {
                let mut request = [0; 9];
                stream.read_exact(&mut request).unwrap();
                stream.write_all(&response).unwrap();
            });
            assert!(probe_memcached(endpoint, Duration::from_secs(1)).is_err());
            server.join().unwrap();
        }
    }

    #[test]
    fn silent_endpoint_times_out() {
        let (endpoint, server) = fixture(|mut stream| {
            let mut request = [0; 9];
            stream.read_exact(&mut request).unwrap();
            thread::sleep(Duration::from_millis(150));
        });
        let started = Instant::now();
        assert!(probe_memcached(endpoint, Duration::from_millis(30)).is_err());
        assert!(started.elapsed() < Duration::from_secs(1));
        server.join().unwrap();
    }

    #[test]
    fn reserved_key_is_cleaned_after_invalid_value_header() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = MemcachedEndpoint {
            address: "127.0.0.1".parse().unwrap(),
            port: listener.local_addr().unwrap().port(),
        };
        let server = thread::spawn(move || {
            let mut reader = BufReader::new(listener.accept().unwrap().0);
            reader
                .get_mut()
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            reader.get_mut().write_all(b"VERSION fixture\r\n").unwrap();
            line.clear();
            reader.read_line(&mut line).unwrap();
            let key = line.split_whitespace().nth(1).unwrap().to_owned();
            for _ in 0..2 {
                line.clear();
                reader.read_line(&mut line).unwrap(); // storage body
                reader.get_mut().write_all(b"STORED\r\n").unwrap();
                line.clear();
                reader.read_line(&mut line).unwrap(); // set or get
            }
            reader
                .get_mut()
                .write_all(format!("VALUE {key} 0 999999999\r\n").as_bytes())
                .unwrap();
            drop(reader);
            let mut cleanup = BufReader::new(listener.accept().unwrap().0);
            cleanup
                .get_mut()
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            line.clear();
            cleanup.read_line(&mut line).unwrap();
            assert_eq!(line, format!("delete {key}\r\n"));
            cleanup.get_mut().write_all(b"DELETED\r\n").unwrap();
            line.clear();
            cleanup.read_line(&mut line).unwrap();
            assert_eq!(line, format!("get {key}\r\n"));
            cleanup.get_mut().write_all(b"END\r\n").unwrap();
        });
        let error = probe_memcached(endpoint, Duration::from_secs(2))
            .unwrap_err()
            .to_string();
        assert!(error.contains("cleanup verified"));
        server.join().unwrap();
    }

    #[test]
    fn reservation_collision_does_not_delete_existing_key() {
        let (endpoint, server) = fixture(|stream| {
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            reader.get_mut().write_all(b"VERSION fixture\r\n").unwrap();
            line.clear();
            reader.read_line(&mut line).unwrap();
            assert!(line.starts_with("add stackyard:probe:"));
            line.clear();
            reader.read_line(&mut line).unwrap();
            reader.get_mut().write_all(b"NOT_STORED\r\n").unwrap();
            line.clear();
            assert_eq!(reader.read_line(&mut line).unwrap(), 0);
        });
        assert!(probe_memcached(endpoint, Duration::from_secs(2)).is_err());
        server.join().unwrap();
    }

    #[test]
    fn closed_endpoint_is_not_a_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = MemcachedEndpoint {
            address: "127.0.0.1".parse().unwrap(),
            port: listener.local_addr().unwrap().port(),
        };
        drop(listener);
        assert!(probe_memcached(endpoint, Duration::from_millis(100)).is_err());
    }

    #[test]
    fn invalid_endpoint_and_timeout_are_rejected() {
        let endpoint = MemcachedEndpoint {
            address: "127.0.0.1".parse().unwrap(),
            port: 0,
        };
        assert!(probe_memcached(endpoint, Duration::from_secs(1)).is_err());
        assert!(probe_memcached(
            MemcachedEndpoint {
                port: 11211,
                ..endpoint
            },
            Duration::ZERO
        )
        .is_err());
        assert!(managed_memcached_unavailable("windows-x64").is_none());
        assert!(managed_memcached_unavailable("macos-arm64").is_some());
    }
}

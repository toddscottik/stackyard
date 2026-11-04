//! One shared Apache configuration, generated from published projects.
//!
//! The engine never edits a system `httpd.conf`. Certificates are a local CA
//! created here. mkcert is not downloaded: its payload was not checked.

use std::net::TcpListener;
use std::path::{Path, PathBuf};

use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair, KeyUsagePurpose,
};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::manifest::{Manifest, SiteKind};

pub const DEFAULT_HTTP_PORT: u16 = 18080;
pub const DEFAULT_HTTPS_PORT: u16 = 18443;
pub const HOSTS_MARK: &str = "# stackyard";
pub const TRUST_FILE: &str = "stackyard-ca.pem";
pub const RESTART_IMPACT: &str = "Restarting Apache interrupts every published site.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenPorts {
    pub http: u16,
    pub https: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Upstream {
    Proxy { port: u16 },
    Static { root: PathBuf },
    FastCgi { port: u16, root: PathBuf },
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualSite {
    pub host: String,
    pub upstream: Upstream,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpdSpec {
    pub server_root: PathBuf,
    pub module_dir: PathBuf,
    pub modules: Vec<(String, String)>,
    pub mime_types: Option<PathBuf>,
    pub ports: ListenPorts,
    pub log_dir: PathBuf,
    pub empty_dir: PathBuf,
    pub cert_dir: PathBuf,
    pub sites: Vec<VirtualSite>,
}

pub fn default_host(name: &str) -> Result<String> {
    Ok(format!("{}.localhost", slug(name)?))
}

pub fn validate_host(host: &str, allow_test: bool) -> Result<String> {
    let host = host.trim().to_ascii_lowercase();
    let Some((label, suffix)) = host.rsplit_once('.') else {
        return Err(Error::InvalidManifest {
            detail: format!("host {host:?} must be one label under .localhost"),
        });
    };
    if label.is_empty() || label.contains('.') || label.len() > 63 {
        return Err(Error::InvalidManifest {
            detail: format!("host {host:?} must be a single label"),
        });
    }
    let chars_ok = label
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !label.starts_with('-')
        && !label.ends_with('-');
    if !chars_ok {
        return Err(Error::InvalidManifest {
            detail: format!("host {host:?} has characters Apache would treat as syntax"),
        });
    }
    match suffix {
        "localhost" => Ok(host),
        "test" if allow_test => Ok(host),
        "test" => Err(Error::InvalidManifest {
            detail: "a .test host has to be written in the manifest; the default is .localhost"
                .to_string(),
        }),
        _ => Err(Error::InvalidManifest {
            detail: format!("host {host:?} must end in .localhost or .test"),
        }),
    }
}

pub fn slug(name: &str) -> Result<String> {
    let mut out = String::new();
    let mut hyphen = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            hyphen = false;
        } else if !hyphen && !out.is_empty() {
            out.push('-');
            hyphen = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() || out.len() > 63 {
        return Err(Error::InvalidManifest {
            detail: format!("project name {name:?} cannot become a host label"),
        });
    }
    Ok(out)
}

pub fn accept_listen_port(port: u16) -> Result<u16> {
    if port < 1024 {
        return Err(Error::InvalidMessage(format!(
            "port {port} is privileged. This build listens on high ports; 80 and 443 stay a separate setup step"
        )));
    }
    Ok(port)
}

pub fn port_is_free(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

pub fn free_high_ports(blocked: &[u16], limit: usize) -> Vec<u16> {
    let mut found = Vec::new();
    for port in DEFAULT_HTTP_PORT..=18180 {
        if blocked.contains(&port) {
            continue;
        }
        if port_is_free(port) {
            found.push(port);
            if found.len() == limit {
                break;
            }
        }
    }
    found
}

pub fn require_free(port: u16) -> Result<()> {
    if port_is_free(port) {
        return Ok(());
    }
    let alternatives = free_high_ports(&[port], 3);
    Err(Error::FrontPortBusy {
        port,
        alternatives: if alternatives.is_empty() {
            "none between 18080 and 18180".to_string()
        } else {
            alternatives
                .iter()
                .map(|port| port.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        },
    })
}

/// `.localhost` names are not written here. The caller adds a `.test` name, or a
/// `.localhost` name only after checking that this machine does not resolve it.
pub fn merge_hosts(existing: &str, entries: &[(&str, &str)]) -> String {
    let mut kept: Vec<String> = existing
        .lines()
        .filter(|line| !line.trim_end().ends_with(HOSTS_MARK))
        .map(str::to_string)
        .collect();
    for (host, ip) in entries {
        kept.push(format!("{ip} {host} {HOSTS_MARK}"));
    }
    if kept.is_empty() {
        return String::new();
    }
    let mut text = kept.join("\n");
    text.push('\n');
    text
}

pub fn label_resolves_to_loopback(host: &str) -> bool {
    const TTL: std::time::Duration = std::time::Duration::from_secs(30);
    const WAIT: std::time::Duration = std::time::Duration::from_millis(400);
    if let Some(value) = cached_resolution(host, TTL) {
        return value;
    }
    let host_owned = host.to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    let worker_host = host_owned.clone();
    let spawned = std::thread::Builder::new()
        .name("stackyard-resolve".to_string())
        .spawn(move || {
            let value = resolve_host_now(&worker_host);
            remember_resolution(&worker_host, value);
            let _ = tx.send(value);
        })
        .is_ok();
    if !spawned {
        return false;
    }
    match rx.recv_timeout(WAIT) {
        Ok(value) => {
            remember_resolution(&host_owned, value);
            value
        }
        Err(_) => {
            // Windows waits out the DNS client timeout for an unknown
            // `.localhost` name. That stall is longer than an engine client
            // will wait, and the eventual answer is still "not loopback".
            remember_resolution(&host_owned, false);
            false
        }
    }
}

fn resolve_host_now(host: &str) -> bool {
    use std::net::ToSocketAddrs;
    match (host, 80_u16).to_socket_addrs() {
        Ok(mut addrs) => addrs.any(|addr| addr.ip().is_loopback()),
        Err(_) => false,
    }
}

struct Resolution {
    host: String,
    value: bool,
    at: std::time::Instant,
}

fn resolution_cache() -> &'static std::sync::Mutex<Vec<Resolution>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<Vec<Resolution>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

fn cached_resolution(host: &str, ttl: std::time::Duration) -> Option<bool> {
    let guard = resolution_cache().lock().ok()?;
    guard
        .iter()
        .find(|item| item.host == host && item.at.elapsed() < ttl)
        .map(|item| item.value)
}

fn remember_resolution(host: &str, value: bool) {
    let Ok(mut guard) = resolution_cache().lock() else {
        return;
    };
    if let Some(item) = guard.iter_mut().find(|item| item.host == host) {
        item.value = value;
        item.at = std::time::Instant::now();
        return;
    }
    guard.push(Resolution {
        host: host.to_string(),
        value,
        at: std::time::Instant::now(),
    });
}

pub fn module_directory(httpd: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(bin) = httpd.parent() {
        if let Some(root) = bin.parent() {
            candidates.push(root.join("modules"));
            candidates.push(root.join("lib").join("apache2").join("modules"));
        }
    }
    candidates.push(PathBuf::from("/usr/lib/apache2/modules"));
    candidates
        .into_iter()
        .find(|path| path.join("mod_proxy.so").is_file())
}

pub fn parse_apache_version(text: &str) -> Option<String> {
    let token = text
        .split_whitespace()
        .find(|part| part.starts_with("Apache/"))?;
    let version = token.trim_start_matches("Apache/");
    let version = version.trim_end_matches([')', ',']);
    if version.is_empty() {
        None
    } else {
        Some(version.to_string())
    }
}

pub fn render_httpd(spec: &HttpdSpec) -> Result<String> {
    let mut lines = Vec::new();
    lines.push("ServerTokens Prod".to_string());
    lines.push("ServerSignature Off".to_string());
    lines.push(format!("ServerRoot {}", quote(&spec.server_root)?));
    for (name, file) in &spec.modules {
        let path = spec.module_dir.join(file);
        lines.push(format!("LoadModule {name} {}", quote(&path)?));
    }
    lines.push(format!("Listen 127.0.0.1:{}", spec.ports.http));
    lines.push(format!("Listen 127.0.0.1:{}", spec.ports.https));
    lines.push(format!("ServerName 127.0.0.1:{}", spec.ports.http));
    lines.push("AcceptFilter http none".to_string());
    lines.push("AcceptFilter https none".to_string());
    lines.push("EnableSendfile Off".to_string());
    lines.push("EnableMMAP Off".to_string());
    lines.push("HostnameLookups Off".to_string());
    lines.push(format!(
        "PidFile {}",
        quote(&spec.log_dir.join("httpd.pid"))?
    ));
    lines.push(format!(
        "ErrorLog {}",
        quote(&spec.log_dir.join("error.log"))?
    ));
    lines.push("LogLevel info".to_string());
    let mutex = spec.log_dir.to_string_lossy().replace('\\', "/");
    if !mutex.contains(' ') && !mutex.contains('"') {
        if cfg!(windows) {
            lines.push("Mutex default".to_string());
        } else {
            lines.push(format!("Mutex file:{mutex} default"));
        }
    }
    lines.push("LogFormat \"%h %l %u %t \\\"%r\\\" %>s %b\" common".to_string());
    lines.push(format!(
        "CustomLog {} common",
        quote(&spec.log_dir.join("access.log"))?
    ));
    if let Some(mime) = &spec.mime_types {
        lines.push(format!("TypesConfig {}", quote(mime)?));
    }
    lines.push("AddType text/plain .txt".to_string());
    lines.push("AddType text/html .html".to_string());
    lines.push("SSLProtocol all -SSLv3 -TLSv1 -TLSv1.1".to_string());
    lines.push("ProxyRequests Off".to_string());
    lines.push("<Directory />".to_string());
    lines.push("    AllowOverride none".to_string());
    lines.push("    Require all denied".to_string());
    lines.push("</Directory>".to_string());
    push_vhost(
        &mut lines,
        spec.ports.http,
        false,
        "default.localhost",
        &Upstream::Static {
            root: spec.empty_dir.clone(),
        },
        &spec.cert_dir,
        true,
    )?;
    push_vhost(
        &mut lines,
        spec.ports.https,
        true,
        "default.localhost",
        &Upstream::Static {
            root: spec.empty_dir.clone(),
        },
        &spec.cert_dir,
        true,
    )?;
    for site in &spec.sites {
        push_vhost(
            &mut lines,
            spec.ports.http,
            false,
            &site.host,
            &site.upstream,
            &spec.cert_dir,
            false,
        )?;
        push_vhost(
            &mut lines,
            spec.ports.https,
            true,
            &site.host,
            &site.upstream,
            &spec.cert_dir,
            false,
        )?;
    }
    Ok(lines.join("\n") + "\n")
}

fn push_vhost(
    lines: &mut Vec<String>,
    port: u16,
    https: bool,
    host: &str,
    upstream: &Upstream,
    cert_dir: &Path,
    default_site: bool,
) -> Result<()> {
    lines.push(format!("<VirtualHost 127.0.0.1:{port}>"));
    lines.push(format!("    ServerName {host}"));
    if https {
        lines.push("    SSLEngine on".to_string());
        lines.push(format!(
            "    SSLCertificateFile {}",
            quote(&cert_dir.join(format!("{host}.pem")))?
        ));
        lines.push(format!(
            "    SSLCertificateKeyFile {}",
            quote(&cert_dir.join(format!("{host}.key")))?
        ));
    }
    let proto = if https { "https" } else { "http" };
    match upstream {
        Upstream::FastCgi {
            port: backend,
            root,
        } => {
            lines.push(format!("    DocumentRoot {}", quote(root)?));
            lines.push("    DirectoryIndex index.php index.html".to_string());
            // Handler mode preserves Apache's normal filename translation and
            // per-directory rewrites. GENERIC removes the PHP-FPM-specific
            // proxy prefix from SCRIPT_FILENAME for the php-cgi SAPI.
            lines.push("    ProxyFCGIBackendType GENERIC".to_string());
            if cfg!(windows) {
                // Handler URLs need a slash after the authority. On Windows
                // GENERIC consequently supplies /C:/...; php-cgi needs C:/....
                lines.push("    ProxyFCGISetEnvIf \"reqenv('SCRIPT_FILENAME') =~ m|^/([A-Za-z]:/.*)$|\" SCRIPT_FILENAME \"$1\"".to_string());
            }
            lines.push("    ProxyTimeout 30".to_string());
            lines.push(format!("    <Directory {}>", quote(root)?));
            lines.push("        Options -Indexes +FollowSymLinks".to_string());
            lines.push("        AllowOverride FileInfo".to_string());
            lines.push("        Require all granted".to_string());
            lines.push("        <FilesMatch \"(?i:\\.php)$\">".to_string());
            lines.push(format!(
                "            SetHandler \"proxy:fcgi://127.0.0.1:{backend}/\""
            ));
            lines.push("        </FilesMatch>".to_string());
            // Alternate PHP suffixes/backups have no approved handler. Never
            // fall through to serving their source as a static asset.
            lines.push(
                "        <FilesMatch \"(?i:\\.(?:php[0-9]+|phtml|phps)$|\\.php[.~])\">".to_string(),
            );
            lines.push("            Require all denied".to_string());
            lines.push("        </FilesMatch>".to_string());
            lines.push("    </Directory>".to_string());
        }
        Upstream::Static { root } => {
            lines.push(format!("    DocumentRoot {}", quote(root)?));
            lines.push(format!("    <Directory {}>", quote(root)?));
            if default_site {
                lines.push("        Options -Indexes".to_string());
                lines.push("        AllowOverride none".to_string());
                lines.push("        Require all granted".to_string());
                lines.push("    </Directory>".to_string());
                lines.push("    RewriteEngine On".to_string());
                lines.push("    RewriteRule ^ - [R=404,L]".to_string());
            } else {
                lines.push("        Options FollowSymLinks".to_string());
                lines.push("        AllowOverride FileInfo".to_string());
                lines.push("        Require all granted".to_string());
                lines.push("    </Directory>".to_string());
            }
        }
        Upstream::Proxy {
            port: upstream_port,
        } => {
            lines.push("    ProxyPreserveHost On".to_string());
            lines.push(format!("    RequestHeader set X-Forwarded-Proto {proto:?}"));
            lines.push(format!("    RequestHeader set X-Forwarded-Port {port:?}"));
            lines.push(format!(
                "    ProxyPass / http://127.0.0.1:{upstream_port}/ upgrade=websocket"
            ));
            lines.push(format!(
                "    ProxyPassReverse / http://127.0.0.1:{upstream_port}/"
            ));
        }
        Upstream::Unavailable => {
            lines.push("    RewriteEngine On".to_string());
            lines.push("    RewriteRule ^ - [R=503,L]".to_string());
        }
    }
    lines.push("</VirtualHost>".to_string());
    Ok(())
}

pub fn quote(path: &Path) -> Result<String> {
    let text = path.to_string_lossy().replace('\\', "/");
    if text.is_empty()
        || text.contains('"')
        || text.contains('\n')
        || text.contains('\r')
        || text.contains('#')
    {
        return Err(Error::InvalidMessage(format!(
            "path {} cannot be placed in an Apache config",
            path.display()
        )));
    }
    Ok(format!("\"{text}\""))
}

pub fn site_root(project_root: &Path, relative: &str) -> Result<PathBuf> {
    if Path::new(relative).is_absolute()
        || relative.contains(':')
        || relative.starts_with(['/', '\\'])
        || relative.split(['/', '\\']).any(|part| part == "..")
    {
        return Err(Error::InvalidManifest {
            detail: format!("site root {relative:?} must stay inside the project"),
        });
    }
    let root = crate::project::contained_dir(project_root, Path::new(relative))?;
    if !root.is_dir() {
        return Err(Error::InvalidManifest {
            detail: format!("site root {relative:?} must be an existing directory"),
        });
    }
    Ok(root)
}

pub fn upstream_for(
    manifest: &Manifest,
    project_root: &Path,
    live_port: Option<u16>,
) -> Result<(String, Upstream)> {
    let site = manifest
        .site
        .as_ref()
        .ok_or_else(|| Error::InvalidManifest {
            detail: "this project has no [site] table".to_string(),
        })?;
    let upstream = match &site.kind {
        SiteKind::Php { service_id, root } => {
            let root = site_root(project_root, root)?;
            let service = manifest
                .service(service_id)
                .ok_or_else(|| Error::InvalidManifest {
                    detail: format!("PHP site references unknown service {service_id}"),
                })?;
            if service.runtime != crate::RuntimeRole::Php
                || service.command.first().map(String::as_str) != Some("php-cgi")
            {
                return Err(Error::InvalidManifest {
                    detail: "PHP document roots require a selected php-cgi service".to_string(),
                });
            }
            match live_port
                .filter(|port| *port >= 1024)
                .or((service.port >= 1024).then_some(service.port))
            {
                Some(port) => Upstream::FastCgi { port, root },
                None => Upstream::Unavailable,
            }
        }
        SiteKind::Static { root } => Upstream::Static {
            root: site_root(project_root, root)?,
        },
        SiteKind::Proxy { service_id } => {
            let service = manifest
                .service(service_id)
                .ok_or_else(|| Error::InvalidManifest {
                    detail: format!("proxy site references unknown service {service_id}"),
                })?;
            if !service.is_http_endpoint() {
                return Err(Error::InvalidManifest {
                    detail: format!("proxy site service {service_id} must expose an HTTP endpoint"),
                });
            }
            let declared = service
                .endpoint
                .as_ref()
                .map(|endpoint| endpoint.port)
                .unwrap_or(service.port);
            let port = live_port
                .filter(|port| *port >= 1024)
                .or(if declared >= 1024 {
                    Some(declared)
                } else {
                    None
                });
            match port {
                Some(port) => Upstream::Proxy { port },
                None => Upstream::Unavailable,
            }
        }
    };
    Ok((site.host.clone(), upstream))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Material {
    pub cert_pem: String,
    pub key_pem: String,
}

pub fn create_ca() -> Result<Material> {
    let key = KeyPair::generate().map_err(cert_err)?;
    let mut params = CertificateParams::new(Vec::<String>::new()).map_err(cert_err)?;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, "Stackyard local CA");
    params.distinguished_name = name;
    let cert = params.self_signed(&key).map_err(cert_err)?;
    Ok(Material {
        cert_pem: cert.pem(),
        key_pem: key.serialize_pem(),
    })
}

pub fn issue_leaf(ca: &Material, host: &str) -> Result<Material> {
    let issuer_key = KeyPair::from_pem(&ca.key_pem).map_err(cert_err)?;
    let mut issuer_params = CertificateParams::from_ca_cert_pem(&ca.cert_pem).map_err(cert_err)?;
    issuer_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    issuer_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let issuer = issuer_params.self_signed(&issuer_key).map_err(cert_err)?;
    let leaf_key = KeyPair::generate().map_err(cert_err)?;
    let mut params = CertificateParams::new(vec![host.to_string()]).map_err(cert_err)?;
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, host);
    params.distinguished_name = name;
    let cert = params
        .signed_by(&leaf_key, &issuer, &issuer_key)
        .map_err(cert_err)?;
    Ok(Material {
        cert_pem: cert.pem(),
        key_pem: leaf_key.serialize_pem(),
    })
}

pub fn fingerprint(pem: &str) -> String {
    hex::encode(Sha256::digest(pem.as_bytes()))
}

pub fn os_trust_command(remove: bool, ca: &Path) -> Result<Vec<String>> {
    let path = ca.display().to_string();
    if cfg!(windows) {
        if remove {
            Ok(vec![
                "certutil".to_string(),
                "-user".to_string(),
                "-delstore".to_string(),
                "Root".to_string(),
                "Stackyard local CA".to_string(),
            ])
        } else {
            Ok(vec![
                "certutil".to_string(),
                "-user".to_string(),
                "-addstore".to_string(),
                "-f".to_string(),
                "Root".to_string(),
                path,
            ])
        }
    } else if cfg!(target_os = "macos") {
        if remove {
            Ok(vec![
                "security".to_string(),
                "delete-certificate".to_string(),
                "-c".to_string(),
                "Stackyard local CA".to_string(),
            ])
        } else {
            Ok(vec![
                "security".to_string(),
                "add-trusted-cert".to_string(),
                "-d".to_string(),
                "-r".to_string(),
                "trustRoot".to_string(),
                "-k".to_string(),
                "login.keychain".to_string(),
                path,
            ])
        }
    } else {
        Err(Error::InvalidMessage(
            "this build does not change the system trust store. Install the CA file into a directory you choose"
                .to_string(),
        ))
    }
}

fn cert_err(err: rcgen::Error) -> Error {
    Error::InvalidMessage(format!("certificate: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localhost_resolves_and_an_unknown_name_does_not_stall() {
        assert!(label_resolves_to_loopback("localhost"));
        let started = std::time::Instant::now();
        let resolved = label_resolves_to_loopback("stackyard-missing.localhost");
        assert!(!resolved);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "lookup took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn hosts_are_one_label_and_test_is_explicit() {
        assert_eq!(default_host("Next App").unwrap(), "next-app.localhost");
        assert!(validate_host("notes.localhost", false).is_ok());
        assert!(validate_host("notes.test", false).is_err());
        assert!(validate_host("notes.test", true).is_ok());
        assert!(validate_host("a.b.localhost", false).is_err());
        assert!(validate_host("localhost", false).is_err());
        assert!(validate_host("bad host.localhost", false).is_err());
    }

    #[test]
    fn privileged_ports_are_refused() {
        assert!(accept_listen_port(80).is_err());
        assert!(accept_listen_port(443).is_err());
        assert_eq!(accept_listen_port(18080).unwrap(), 18080);
    }

    #[test]
    fn hosts_merge_drops_only_marked_lines() {
        let existing = "127.0.0.1 kept.example\n10.0.0.8 old.test # stackyard\n# comment\n";
        let merged = merge_hosts(existing, &[("notes.test", "127.0.0.1")]);
        assert!(merged.contains("127.0.0.1 kept.example"));
        assert!(merged.contains("# comment"));
        assert!(!merged.contains("old.test"));
        assert!(merged.contains("127.0.0.1 notes.test # stackyard\n"));
    }

    #[test]
    fn occupied_port_lists_other_high_ports() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        if !(18080..=18180).contains(&port) {
            drop(listener);
            return;
        }
        let err = require_free(port).unwrap_err();
        let Error::FrontPortBusy { alternatives, .. } = err else {
            panic!("expected choices, got {err}");
        };
        assert!(!alternatives.is_empty());
        drop(listener);
    }

    #[test]
    fn rendered_sites_do_not_share_roots_and_unknown_is_first() {
        let spec = HttpdSpec {
            server_root: PathBuf::from("/data/apache"),
            module_dir: PathBuf::from("/mods"),
            modules: vec![("proxy_module".to_string(), "mod_proxy.so".to_string())],
            mime_types: None,
            ports: ListenPorts {
                http: 18080,
                https: 18443,
            },
            log_dir: PathBuf::from("/data/apache/logs"),
            empty_dir: PathBuf::from("/data/apache/empty"),
            cert_dir: PathBuf::from("/data/certs"),
            sites: vec![
                VirtualSite {
                    host: "a.localhost".to_string(),
                    upstream: Upstream::Static {
                        root: PathBuf::from("/projects/a/public"),
                    },
                },
                VirtualSite {
                    host: "b.localhost".to_string(),
                    upstream: Upstream::Proxy { port: 19001 },
                },
                VirtualSite {
                    host: "php.localhost".to_string(),
                    upstream: Upstream::FastCgi {
                        port: 19002,
                        root: PathBuf::from("/projects/php/public"),
                    },
                },
            ],
        };
        let text = render_httpd(&spec).unwrap();
        assert!(text.find("LoadModule proxy_module").unwrap() < text.find("LogFormat").unwrap());
        if cfg!(windows) {
            assert!(text.contains("Mutex default"));
            assert!(!text.contains("Mutex file:"));
        }
        let a = text.find("ServerName a.localhost").unwrap();
        let b = text.find("ServerName b.localhost").unwrap();
        let a_block = &text[a..b];
        assert!(a_block.contains("/projects/a/public"));
        assert!(!a_block.contains("19001"));
        assert!(text[b..].contains("127.0.0.1:19001"));
        assert!(!text[b..].contains("/projects/a/public"));
        assert!(text.find("ServerName default.localhost").unwrap() < a);
        assert!(text.contains("upgrade=websocket"));
        assert!(text.contains("X-Forwarded-Proto"));
        let php = &text[text.find("ServerName php.localhost").unwrap()..];
        assert!(php.contains("DocumentRoot \"/projects/php/public\""));
        assert!(php.contains("DirectoryIndex index.php index.html"));
        assert!(php.contains("AllowOverride FileInfo"));
        assert!(php.contains("ProxyFCGIBackendType GENERIC"));
        assert!(php.contains("SetHandler \"proxy:fcgi://127.0.0.1:19002/\""));
        assert!(!php.contains("19001"));
        assert!(!php.contains("/projects/a/public"));
    }

    #[test]
    fn document_roots_must_exist_and_stay_inside_project() {
        let dir = crate::disposable_dir("document-roots");
        std::fs::create_dir_all(dir.join("public")).unwrap();
        std::fs::write(dir.join("file.txt"), "not a directory").unwrap();
        let inside = site_root(&dir, "public").unwrap();
        assert!(inside.is_dir());
        assert!(site_root(&dir, "../outside").is_err());
        assert!(site_root(&dir, "missing").is_err());
        assert!(site_root(&dir, "file.txt").is_err());
        assert!(site_root(&dir, "/outside").is_err());
        assert!(site_root(&dir, "C:/outside").is_err());
        assert!(site_root(&dir, "\\\\server\\share").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_ca_signs_a_leaf() {
        let ca = create_ca().unwrap();
        let leaf = issue_leaf(&ca, "notes.localhost").unwrap();
        assert!(leaf.cert_pem.contains("BEGIN CERTIFICATE"));
        assert_ne!(fingerprint(&ca.cert_pem), fingerprint(&leaf.cert_pem));
    }

    #[test]
    fn version_token_comes_from_httpd() {
        assert_eq!(
            parse_apache_version("Server version: Apache/2.4.58 (Ubuntu)\n").as_deref(),
            Some("2.4.58")
        );
    }
}

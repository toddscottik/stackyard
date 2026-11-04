//! The shared Apache process. Configuration is generated under the data directory
//! and checked with `httpd -t` before it replaces the running file.

use std::fs;
use std::io::Write;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use stackyard_core::{
    accept_listen_port, create_ca, fingerprint, issue_leaf, label_resolves_to_loopback,
    load_project_manifest, merge_hosts, module_directory, port_is_free, render_httpd, site_root,
    upstream_for, Error, HttpdSpec, ListenPorts, Material, Outcome, PublicationRecord, Result,
    SiteKind, SiteView, Store, Upstream, VirtualSite, DEFAULT_HTTPS_PORT, DEFAULT_HTTP_PORT,
    RESTART_IMPACT, TRUST_FILE,
};

use crate::group::{self, GroupProcess};

const STATE_FILE: &str = "state.txt";

pub struct Front {
    data_dir: PathBuf,
    running: Mutex<Option<GroupProcess>>,
    orphan: Mutex<Option<(u32, String)>>,
}

impl Front {
    pub fn new(data_dir: PathBuf) -> Self {
        let front = Self {
            data_dir,
            running: Mutex::new(None),
            orphan: Mutex::new(None),
        };
        front.note_orphan();
        front
    }

    pub fn shutdown(&self) {
        self.stop_owned();
    }

    pub fn status(&self) -> Result<Outcome> {
        self.view()
    }

    pub fn publish(&self, project_id: &str) -> Result<Outcome> {
        let (project, manifest) = load_project_manifest(&self.data_dir, project_id)?;
        let site = manifest
            .site
            .clone()
            .ok_or_else(|| Error::InvalidManifest {
                detail: "this project has no [site] table".to_string(),
            })?;
        let (mode, service_id, static_dir) = match &site.kind {
            SiteKind::Proxy { service_id } => ("proxy", service_id.clone(), String::new()),
            SiteKind::Static { root } => ("static", String::new(), root.clone()),
            SiteKind::Php { service_id, root } => ("php", service_id.clone(), root.clone()),
        };
        let store = Store::open(&self.data_dir)?;
        if store
            .list_publications()?
            .iter()
            .any(|row| row.host == site.host && row.project_id != project.id)
        {
            return Err(Error::InvalidMessage(format!(
                "{} is already published by another project",
                site.host
            )));
        }
        let record = PublicationRecord {
            project_id: project.id.clone(),
            host: site.host,
            mode: mode.to_string(),
            service_id,
            static_dir,
        };
        let mut rows = store.list_publications()?;
        rows.retain(|row| row.project_id != record.project_id);
        rows.push(record.clone());
        self.commit(&rows)?;
        store.upsert_publication(&record)?;
        store.record_event("site_publish", &project.id)?;
        self.view()
    }

    pub fn unpublish(&self, project_id: &str) -> Result<Outcome> {
        let store = Store::open(&self.data_dir)?;
        if store.get_project(project_id)?.is_none() {
            return Err(Error::ProjectNotFound {
                id: project_id.to_string(),
            });
        }
        let rows: Vec<_> = store
            .list_publications()?
            .into_iter()
            .filter(|row| row.project_id != project_id)
            .collect();
        if rows.is_empty() {
            self.stop_owned();
            let _ = fs::remove_file(self.apache_dir().join("httpd.conf"));
        } else {
            self.commit(&rows)?;
        }
        store.delete_publication(project_id)?;
        store.record_event("site_unpublish", project_id)?;
        self.view()
    }

    pub fn restart(&self) -> Result<Outcome> {
        let rows = Store::open(&self.data_dir)?.list_publications()?;
        if rows.is_empty() {
            return Err(Error::InvalidMessage(
                "no site is published, so Apache is not running".to_string(),
            ));
        }
        self.commit(&rows)?;
        self.view()
    }

    pub fn set_ports(&self, http_port: u16, https_port: u16) -> Result<Outcome> {
        let http_port = accept_listen_port(http_port)?;
        let https_port = accept_listen_port(https_port)?;
        if http_port == https_port {
            return Err(Error::InvalidMessage(
                "the HTTP and HTTPS ports have to be different".to_string(),
            ));
        }
        let current = self.ports()?;
        for port in [http_port, https_port] {
            if port != current.http && port != current.https && !port_is_free(port) {
                stackyard_core::require_free(port)?;
            }
        }
        let store = Store::open(&self.data_dir)?;
        store.put("http_port", &http_port.to_string())?;
        store.put("https_port", &https_port.to_string())?;
        let rows = store.list_publications()?;
        if !rows.is_empty() && self.running.lock().expect("apache").is_some() {
            self.commit(&rows)?;
        }
        self.view()
    }

    pub fn trust_install(&self, directory: &Path) -> Result<Outcome> {
        if !directory.is_dir() {
            return Err(Error::InvalidMessage(format!(
                "{} is not a directory",
                directory.display()
            )));
        }
        let ca = self.ensure_ca()?;
        fs::write(directory.join(TRUST_FILE), &ca.cert_pem)
            .map_err(|source| Error::io("write trusted CA", source))?;
        fs::write(
            directory.join(format!("{TRUST_FILE}.fingerprint")),
            fingerprint(&ca.cert_pem),
        )
        .map_err(|source| Error::io("write CA fingerprint", source))?;
        Store::open(&self.data_dir)?
            .record_event("trust_install", &directory.display().to_string())?;
        self.view()
    }

    pub fn trust_remove(&self, directory: &Path) -> Result<Outcome> {
        let ca_path = self.cert_dir().join("ca.pem");
        let ca_pem = fs::read_to_string(&ca_path)
            .map_err(|_| Error::InvalidMessage("there is no Stackyard CA to remove".to_string()))?;
        let marker = directory.join(format!("{TRUST_FILE}.fingerprint"));
        let found = fs::read_to_string(&marker).unwrap_or_default();
        if found.trim() != fingerprint(&ca_pem) {
            return Err(Error::InvalidMessage(
                "refusing to remove a certificate Stackyard did not install in this directory"
                    .to_string(),
            ));
        }
        let _ = fs::remove_file(directory.join(TRUST_FILE));
        let _ = fs::remove_file(marker);
        Store::open(&self.data_dir)?
            .record_event("trust_remove", &directory.display().to_string())?;
        self.view()
    }

    pub fn write_hosts(&self, path: &Path) -> Result<Outcome> {
        let existing = fs::read_to_string(path).unwrap_or_default();
        let resolves = label_resolves_to_loopback("stackyard.localhost");
        let mut entries = Vec::new();
        for row in Store::open(&self.data_dir)?.list_publications()? {
            let needs_line =
                row.host.ends_with(".test") || (row.host.ends_with(".localhost") && !resolves);
            if needs_line {
                entries.push((row.host, "127.0.0.1".to_string()));
            }
        }
        let borrowed: Vec<(&str, &str)> = entries
            .iter()
            .map(|(host, ip)| (host.as_str(), ip.as_str()))
            .collect();
        let merged = merge_hosts(&existing, &borrowed);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|source| Error::io("create hosts directory", source))?;
        }
        fs::write(path, merged).map_err(|source| Error::io("write hosts file", source))?;
        self.view()
    }

    /// Replace the live config only when `httpd -t` accepts `text`.
    pub fn install_config_text(&self, text: &str) -> Result<()> {
        self.swap_and_reload(text)
    }

    fn commit(&self, rows: &[PublicationRecord]) -> Result<()> {
        let text = self.render(rows)?;
        self.swap_and_reload(&text)
    }

    fn swap_and_reload(&self, text: &str) -> Result<()> {
        let httpd = self.httpd_binary()?;
        let dir = self.apache_dir();
        fs::create_dir_all(dir.join("logs"))
            .map_err(|source| Error::io("create apache log dir", source))?;
        fs::create_dir_all(dir.join("empty"))
            .map_err(|source| Error::io("create default site", source))?;
        let candidate = dir.join("candidate.conf");
        let live = dir.join("httpd.conf");
        let previous = dir.join("previous.conf");
        fs::write(&candidate, text)
            .map_err(|source| Error::io("write candidate config", source))?;
        if let Err(err) = syntax_ok(&httpd, &candidate) {
            let _ = fs::remove_file(&candidate);
            return Err(err);
        }
        if live.is_file() {
            fs::copy(&live, &previous)
                .map_err(|source| Error::io("keep previous config", source))?;
        }
        fs::rename(&candidate, &live)
            .map_err(|source| Error::io("install apache config", source))?;
        if let Err(err) = self.reload(&httpd, &live) {
            if previous.is_file() {
                let _ = fs::copy(&previous, &live);
                let _ = self.reload(&httpd, &live);
            } else {
                self.stop_owned();
            }
            return Err(err);
        }
        Ok(())
    }

    fn reload(&self, httpd: &Path, conf: &Path) -> Result<()> {
        self.stop_owned();
        let log = self.data_dir.join("logs").join("apache.log");
        if let Some(parent) = log.parent() {
            fs::create_dir_all(parent)
                .map_err(|source| Error::io("create engine log dir", source))?;
        }
        let cwd = httpd.parent().unwrap_or(httpd);
        let mut process = group::spawn_grouped(
            httpd,
            &[
                "-D".to_string(),
                "FOREGROUND".to_string(),
                "-f".to_string(),
                conf.display().to_string(),
            ],
            cwd,
            &[],
            &log,
        )?;
        let ports = self.ports()?;
        let started = Instant::now();
        let mut opened = false;
        while started.elapsed() < Duration::from_secs(8) {
            if TcpStream::connect(("127.0.0.1", ports.http)).is_ok() {
                opened = true;
                break;
            }
            thread::sleep(Duration::from_millis(40));
        }
        if !opened {
            process.request_stop();
            let _ = process.wait_timeout(Duration::from_secs(3));
            let detail = fs::read_to_string(self.apache_dir().join("logs").join("error.log"))
                .unwrap_or_default();
            let tail: String = detail
                .chars()
                .rev()
                .take(1200)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            return Err(Error::InvalidMessage(format!(
                "Apache did not listen on 127.0.0.1:{}. {tail}",
                ports.http
            )));
        }
        fs::write(
            self.apache_dir().join(STATE_FILE),
            format!("{} {}", process.pid, process.identity),
        )
        .map_err(|source| Error::io("record apache identity", source))?;
        *self.orphan.lock().expect("orphan") = None;
        *self.running.lock().expect("apache") = Some(process);
        Ok(())
    }

    fn render(&self, rows: &[PublicationRecord]) -> Result<String> {
        let httpd = self.httpd_binary()?;
        let module_dir = module_directory(&httpd).ok_or_else(|| {
            Error::InvalidMessage(format!(
                "Apache modules were not found next to {}",
                httpd.display()
            ))
        })?;
        let modules = load_lines(
            &httpd,
            &module_dir,
            rows.iter().any(|row| row.mode == "php"),
        )?;
        let ports = self.ports()?;
        let dir = self.apache_dir();
        let cert_dir = self.cert_dir();
        fs::create_dir_all(&cert_dir).map_err(|source| Error::io("create cert dir", source))?;
        let ca = self.ensure_ca()?;
        issue_to_disk(&ca, &cert_dir, "default.localhost")?;
        let mut sites = Vec::new();
        let store = Store::open(&self.data_dir)?;
        for row in rows {
            let Some(project) = store.get_project(&row.project_id)? else {
                continue;
            };
            issue_to_disk(&ca, &cert_dir, &row.host)?;
            let root = PathBuf::from(&project.root);
            let upstream = if row.mode == "static" {
                Upstream::Static {
                    root: site_root(&root, &row.static_dir)?,
                }
            } else {
                let live = live_port(&store, &row.project_id, &row.service_id);
                match load_project_manifest(&self.data_dir, &row.project_id) {
                    Ok((_, manifest)) => {
                        // A previously published PHP root must never silently
                        // become a static root after a manifest edit.
                        if row.mode == "php"
                            && !matches!(manifest.site.as_ref().map(|site| &site.kind), Some(SiteKind::Php { service_id, root }) if service_id == &row.service_id && root == &row.static_dir)
                        {
                            Upstream::Unavailable
                        } else {
                            upstream_for(&manifest, &root, live)?.1
                        }
                    }
                    Err(_) => Upstream::Unavailable,
                }
            };
            sites.push(VirtualSite {
                host: row.host.clone(),
                upstream,
            });
        }
        let mime = mime_types(&httpd);
        render_httpd(&HttpdSpec {
            server_root: dir.clone(),
            module_dir,
            modules,
            mime_types: mime,
            ports,
            log_dir: dir.join("logs"),
            empty_dir: dir.join("empty"),
            cert_dir,
            sites,
        })
    }

    fn view(&self) -> Result<Outcome> {
        let ports = self.ports()?;
        let store = Store::open(&self.data_dir)?;
        let (state, pid) = if let Some(process) = self.running.lock().expect("apache").as_ref() {
            ("running".to_string(), Some(process.pid))
        } else if let Some((pid, _)) = *self.orphan.lock().expect("orphan") {
            ("orphaned".to_string(), Some(pid))
        } else {
            ("stopped".to_string(), None)
        };
        let mut sites = Vec::new();
        for row in store.list_publications()? {
            let upstream = match row.mode.as_str() {
                "static" => row.static_dir.clone(),
                "php" => {
                    if let Some(port) = live_port(&store, &row.project_id, &row.service_id) {
                        format!("fcgi://127.0.0.1:{port} ({})", row.static_dir)
                    } else {
                        "PHP backend not running (503; source protected)".to_string()
                    }
                }
                _ => {
                    if let Some(port) = live_port(&store, &row.project_id, &row.service_id) {
                        format!("127.0.0.1:{port}")
                    } else {
                        "not running".to_string()
                    }
                }
            };
            sites.push(SiteView {
                project_id: row.project_id,
                host: row.host.clone(),
                mode: row.mode,
                upstream,
                http_url: format!("http://{}:{}/", row.host, ports.http),
                https_url: format!("https://{}:{}/", row.host, ports.https),
            });
        }
        Ok(Outcome::Front {
            state,
            pid,
            http_port: ports.http,
            https_port: ports.https,
            restart_impact: RESTART_IMPACT.to_string(),
            ca_path: stackyard_core::plain_path(self.cert_dir().join("ca.pem"))
                .display()
                .to_string(),
            resolves_localhost: label_resolves_to_loopback("stackyard.localhost"),
            sites,
        })
    }

    fn ports(&self) -> Result<ListenPorts> {
        let store = Store::open(&self.data_dir)?;
        Ok(ListenPorts {
            http: read_port(&store, "http_port", DEFAULT_HTTP_PORT)?,
            https: read_port(&store, "https_port", DEFAULT_HTTPS_PORT)?,
        })
    }

    fn ensure_ca(&self) -> Result<Material> {
        let dir = self.cert_dir();
        fs::create_dir_all(&dir).map_err(|source| Error::io("create cert dir", source))?;
        let cert_path = dir.join("ca.pem");
        let key_path = dir.join("ca.key");
        if cert_path.is_file() && key_path.is_file() {
            return Ok(Material {
                cert_pem: fs::read_to_string(&cert_path)
                    .map_err(|source| Error::io("read CA certificate", source))?,
                key_pem: fs::read_to_string(&key_path)
                    .map_err(|source| Error::io("read CA key", source))?,
            });
        }
        let created = create_ca()?;
        write_secret(&key_path, &created.key_pem)?;
        fs::write(&cert_path, &created.cert_pem)
            .map_err(|source| Error::io("write CA certificate", source))?;
        Ok(created)
    }

    fn httpd_binary(&self) -> Result<PathBuf> {
        let store = Store::open(&self.data_dir)?;
        let Some(id) = store.get_selection(stackyard_core::DEFAULT_PROJECT, "apache")? else {
            return Err(Error::InvalidMessage(
                "no Apache is selected. Record an existing httpd, or install the Windows catalog package"
                    .to_string(),
            ));
        };
        let Some(package) = store.get_package(&id)? else {
            return Err(Error::PackageNotFound { id });
        };
        let path = PathBuf::from(package.launch);
        if !path.is_file() {
            return Err(Error::ProbeFailed {
                path,
                detail: "Apache executable is missing".to_string(),
            });
        }
        Ok(path)
    }

    fn apache_dir(&self) -> PathBuf {
        self.data_dir.join("apache")
    }

    fn cert_dir(&self) -> PathBuf {
        self.data_dir.join("certs")
    }

    fn note_orphan(&self) {
        let path = self.apache_dir().join(STATE_FILE);
        let Ok(text) = fs::read_to_string(path) else {
            return;
        };
        let mut parts = text.split_whitespace();
        let Some(pid) = parts.next().and_then(|value| value.parse().ok()) else {
            return;
        };
        let Some(identity) = parts.next() else {
            return;
        };
        if group::process_identity(pid).as_deref() == Some(identity) {
            *self.orphan.lock().expect("orphan") = Some((pid, identity.to_string()));
        }
    }

    fn stop_owned(&self) {
        if let Some(mut process) = self.running.lock().expect("apache").take() {
            process.request_stop();
            let _ = process.wait_timeout(Duration::from_secs(3));
        }
        if let Some((pid, identity)) = self.orphan.lock().expect("orphan").take() {
            group::stop_recorded(pid, pid, &identity);
        }
        let _ = fs::remove_file(self.apache_dir().join(STATE_FILE));
    }
}

fn read_port(store: &Store, key: &str, default: u16) -> Result<u16> {
    match store.get(key)? {
        Some(value) => {
            let port = value
                .parse::<u16>()
                .map_err(|_| Error::InvalidMessage(format!("setting {key} is not a port")))?;
            accept_listen_port(port)
        }
        None => Ok(default),
    }
}

fn live_port(store: &Store, project_id: &str, service_id: &str) -> Option<u16> {
    let rows = store.list_processes(project_id).ok()?;
    let row = rows.iter().find(|row| row.service_id == service_id)?;
    if !matches!(row.state.as_str(), "running" | "starting") {
        return None;
    }
    let port = u16::try_from(row.port?).ok()?;
    (port >= 1024).then_some(port)
}

fn issue_to_disk(ca: &Material, dir: &Path, host: &str) -> Result<()> {
    let cert = dir.join(format!("{host}.pem"));
    let key = dir.join(format!("{host}.key"));
    if cert.is_file() && key.is_file() {
        return Ok(());
    }
    let leaf = issue_leaf(ca, host)?;
    write_secret(&key, &leaf.key_pem)?;
    fs::write(cert, leaf.cert_pem).map_err(|source| Error::io("write leaf certificate", source))?;
    Ok(())
}

fn write_secret(path: &Path, text: &str) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|source| Error::io("create private key", source))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
    }
    file.write_all(text.as_bytes())
        .map_err(|source| Error::io("write private key", source))?;
    Ok(())
}

fn mime_types(httpd: &Path) -> Option<PathBuf> {
    let packaged = httpd
        .parent()
        .and_then(|bin| bin.parent())
        .map(|root| root.join("conf").join("mime.types"));
    if let Some(path) = packaged {
        if path.is_file() {
            return Some(path);
        }
    }
    let system = PathBuf::from("/etc/mime.types");
    system.is_file().then_some(system)
}

fn load_lines(httpd: &Path, module_dir: &Path, php_sites: bool) -> Result<Vec<(String, String)>> {
    let output = Command::new(httpd)
        .arg("-l")
        .output()
        .map_err(|source| Error::io("list compiled Apache modules", source))?;
    let compiled = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);
    let mpm = if cfg!(windows) {
        ("mpm_winnt_module", "mod_mpm_winnt.so", "mpm_winnt")
    } else {
        ("mpm_event_module", "mod_mpm_event.so", "mpm_event")
    };
    let modules = [
        mpm,
        ("log_config_module", "mod_log_config.so", "mod_log_config"),
        ("authn_core_module", "mod_authn_core.so", "mod_authn_core"),
        ("authz_core_module", "mod_authz_core.so", "mod_authz_core"),
        ("authz_host_module", "mod_authz_host.so", "mod_authz_host"),
        ("mime_module", "mod_mime.so", "mod_mime"),
        ("alias_module", "mod_alias.so", "mod_alias"),
        ("dir_module", "mod_dir.so", "mod_dir"),
        ("rewrite_module", "mod_rewrite.so", "mod_rewrite"),
        ("proxy_module", "mod_proxy.so", "mod_proxy.c"),
        ("proxy_http_module", "mod_proxy_http.so", "mod_proxy_http"),
        ("proxy_fcgi_module", "mod_proxy_fcgi.so", "mod_proxy_fcgi"),
        (
            "proxy_wstunnel_module",
            "mod_proxy_wstunnel.so",
            "mod_proxy_wstunnel",
        ),
        ("headers_module", "mod_headers.so", "mod_headers"),
        ("filter_module", "mod_filter.so", "mod_filter"),
        ("setenvif_module", "mod_setenvif.so", "mod_setenvif"),
        (
            "socache_shmcb_module",
            "mod_socache_shmcb.so",
            "mod_socache_shmcb",
        ),
        ("ssl_module", "mod_ssl.so", "mod_ssl"),
    ];
    let required = [
        "log_config_module",
        "mime_module",
        "proxy_module",
        "proxy_http_module",
        "ssl_module",
        "rewrite_module",
        "headers_module",
        "authz_core_module",
    ];
    let mut loaded = Vec::new();
    for (name, file, hint) in modules {
        if name == "proxy_fcgi_module" && !php_sites {
            continue;
        }
        if compiled.contains(hint) {
            continue;
        }
        let path = module_dir.join(file);
        if path.is_file() {
            loaded.push((name.to_string(), file.to_string()));
        } else if required.contains(&name) || (php_sites && name == "proxy_fcgi_module") {
            return Err(Error::InvalidMessage(format!(
                "required Apache module {file} is missing from {}",
                module_dir.display()
            )));
        }
    }
    Ok(loaded)
}

fn syntax_ok(httpd: &Path, conf: &Path) -> Result<()> {
    let output = Command::new(httpd)
        .arg("-t")
        .arg("-f")
        .arg(conf)
        .output()
        .map_err(|source| Error::io("check apache config", source))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if output.status.success() && text.contains("Syntax OK") {
        Ok(())
    } else {
        Err(Error::InvalidMessage(format!(
            "Apache refused the new config, so the previous sites were left in place. {text}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marked_hosts_lines_round_trip_without_touching_other_names() {
        let dir = stackyard_core::disposable_dir("hosts");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hosts");
        fs::write(&path, "10.1.1.1 kept.example\n").unwrap();
        let merged = merge_hosts(
            &fs::read_to_string(&path).unwrap(),
            &[("notes.test", "127.0.0.1")],
        );
        fs::write(&path, merged).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("kept.example"));
        assert!(text.contains("notes.test"));
        assert!(text.contains(stackyard_core::HOSTS_MARK));
        let _ = fs::remove_dir_all(dir);
    }
}

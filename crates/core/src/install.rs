use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::archive::{extract, read_to_digest};
use crate::catalog::{require_host, CatalogPackage};
use crate::error::{Error, Result};
use crate::store::{PackageRecord, Store};

pub const DEFAULT_PROJECT: &str = "default";

#[derive(Debug, Clone)]
pub struct Progress {
    pub bytes: Arc<AtomicU64>,
    pub cancel: Arc<AtomicBool>,
}

impl Progress {
    pub fn new() -> Self {
        Self {
            bytes: Arc::new(AtomicU64::new(0)),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

impl Default for Progress {
    fn default() -> Self {
        Self::new()
    }
}

pub fn install_reader<R: Read>(
    data_dir: &Path,
    package: &CatalogPackage,
    reader: R,
    progress: &Progress,
) -> Result<PackageRecord> {
    require_host(package)?;
    validate_id(&package.id)?;
    let store = Store::open(data_dir)?;
    if store.get_package(&package.id)?.is_some() {
        return Err(Error::AlreadyInstalled {
            id: package.id.clone(),
        });
    }
    let root = storage_root(data_dir, &package.kind);
    let final_dir = root.join(&package.name).join(&package.version);
    if final_dir.exists() {
        return Err(Error::AlreadyInstalled {
            id: package.id.clone(),
        });
    }
    let incoming = data_dir.join("incoming").join(&package.id);
    let _ = fs::remove_dir_all(&incoming);
    fs::create_dir_all(&incoming)
        .map_err(|source| Error::io("create incoming directory", source))?;
    let payload = incoming.join("payload");
    let read = read_to_digest(
        CountingRead {
            inner: reader,
            bytes: Arc::clone(&progress.bytes),
        },
        &payload,
        &|| progress.cancelled(),
    );
    let (_size, digest) = match read {
        Ok(value) => value,
        Err(err) => {
            let _ = fs::remove_dir_all(&incoming);
            return Err(match err {
                Error::Cancelled { .. } => Error::Cancelled {
                    id: package.id.clone(),
                },
                other => other,
            });
        }
    };
    if !digest.eq_ignore_ascii_case(&package.sha256) {
        let _ = fs::remove_dir_all(&incoming);
        return Err(Error::DigestMismatch {
            id: package.id.clone(),
            expected: package.sha256.clone(),
            actual: digest,
        });
    }
    let tree = incoming.join("tree");
    fs::create_dir_all(&tree).map_err(|source| Error::io("create extract directory", source))?;
    if let Err(err) = extract(&package.format, &payload, &tree) {
        let _ = fs::remove_dir_all(&incoming);
        return Err(err);
    }
    let launch = tree.join(Path::new(&package.launch));
    if !launch.is_file() {
        let _ = fs::remove_dir_all(&incoming);
        return Err(Error::ProbeFailed {
            path: launch,
            detail: "catalog launch path was not extracted".to_string(),
        });
    }
    if let Some(parent) = final_dir.parent() {
        fs::create_dir_all(parent)
            .map_err(|source| Error::io("create package directory", source))?;
    }
    if let Err(source) = fs::rename(&tree, &final_dir) {
        let _ = fs::remove_dir_all(&incoming);
        let _ = fs::remove_dir_all(&final_dir);
        return Err(Error::io("commit package directory", source));
    }
    let _ = fs::remove_dir_all(&incoming);
    let record = PackageRecord {
        id: package.id.clone(),
        kind: package.kind.clone(),
        name: package.name.clone(),
        version: package.version.clone(),
        target: package.target.clone(),
        source: "managed".to_string(),
        path: final_dir.display().to_string(),
        launch: final_dir.join(&package.launch).display().to_string(),
        digest: Some(digest),
        removable: true,
    };
    if let Err(err) = store.insert_package(&record) {
        let _ = fs::remove_dir_all(&final_dir);
        return Err(err);
    }
    Ok(record)
}

/// pnpm is the default Node package manager. Existing npm lockfiles are left alone;
/// this only installs and selects the pinned pnpm tool.
pub fn ensure_default_node_manager(data_dir: &Path) -> Result<PackageRecord> {
    let Some(pnpm) = crate::catalog::catalog_for_host()?
        .into_iter()
        .find(|package| package.name == "pnpm")
    else {
        return Err(Error::PackageNotFound {
            id: "pnpm".to_string(),
        });
    };
    let store = Store::open(data_dir)?;
    if let Some(existing) = store.get_package(&pnpm.id)? {
        store.select_package(DEFAULT_PROJECT, "pnpm", &existing.id)?;
        return Ok(existing);
    }
    let installed = download_and_install(data_dir, &pnpm, &Progress::new())?;
    select_for_project(data_dir, DEFAULT_PROJECT, &installed.id)?;
    Ok(installed)
}

pub fn download_and_install(
    data_dir: &Path,
    package: &CatalogPackage,
    progress: &Progress,
) -> Result<PackageRecord> {
    require_host(package)?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(30))
        .timeout_read(Duration::from_secs(120))
        .build();
    let response = agent
        .get(&package.url)
        .set("user-agent", "stackyard/0.1")
        .call()
        .map_err(|err| Error::InvalidMessage(format!("download {}: {err}", package.url)))?;
    install_reader(data_dir, package, response.into_reader(), progress)
}

pub fn adopt_existing(data_dir: &Path, role: &str, executable: &Path) -> Result<PackageRecord> {
    if !matches!(
        role,
        "node" | "python" | "pnpm" | "uv" | "apache" | "mysql" | "postgres"
    ) {
        return Err(Error::InvalidMessage(format!(
            "existing selection for {role} is not supported"
        )));
    }
    if !executable.is_file() {
        return Err(Error::ProbeFailed {
            path: executable.to_path_buf(),
            detail: "executable does not exist".to_string(),
        });
    }
    let args = match role {
        "node" => vec!["-v".to_string()],
        "python" => vec![
            "-c".to_string(),
            "import sys; print(sys.version.split()[0])".to_string(),
        ],
        "uv" => vec!["--version".to_string()],
        "apache" => vec!["-v".to_string()],
        "mysql" => vec!["--no-defaults".to_string(), "--version".to_string()],
        "postgres" => vec!["--version".to_string()],
        _ => vec!["--version".to_string()],
    };
    let probed = command_line(executable, &args)?;
    let version = if role == "apache" {
        crate::apache::parse_apache_version(&probed).ok_or_else(|| Error::ProbeFailed {
            path: executable.to_path_buf(),
            detail: format!("could not read an Apache version from {probed:?}"),
        })?
    } else if role == "uv" {
        probed
            .split_whitespace()
            .nth(1)
            .unwrap_or(probed.as_str())
            .to_string()
    } else {
        probed
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let id = format!("existing-{role}");
    let store = Store::open(data_dir)?;
    if store.get_package(&id)?.is_some() {
        return Err(Error::AlreadyInstalled { id });
    }
    let kind = if role == "apache" || role == "mysql" || role == "postgres" {
        "server"
    } else if role == "node" || role == "python" {
        "runtime"
    } else {
        "tool"
    };
    let record = PackageRecord {
        id: id.clone(),
        kind: kind.to_string(),
        name: role.to_string(),
        version,
        target: crate::catalog::current_target().to_string(),
        source: "existing".to_string(),
        path: executable.display().to_string(),
        launch: executable.display().to_string(),
        digest: None,
        removable: false,
    };
    store.insert_package(&record)?;
    store.select_package(DEFAULT_PROJECT, role, &id)?;
    Ok(record)
}

pub fn remove_package(data_dir: &Path, id: &str) -> Result<()> {
    let store = Store::open(data_dir)?;
    let Some(package) = store.get_package(id)? else {
        return Err(Error::PackageNotFound { id: id.to_string() });
    };
    if !package.removable || package.source == "existing" {
        return Err(Error::NotRemovable {
            id: id.to_string(),
            reason: "existing installations are referenced, not owned".to_string(),
        });
    }
    if store.package_is_selected(id)? {
        return Err(Error::NotRemovable {
            id: id.to_string(),
            reason: "a project selection still uses it".to_string(),
        });
    }
    let path = PathBuf::from(&package.path);
    let packages = data_dir.join("packages");
    let tools = data_dir.join("tools");
    if !path.starts_with(&packages) && !path.starts_with(&tools) {
        return Err(Error::NotRemovable {
            id: id.to_string(),
            reason: "path is outside app-owned package storage".to_string(),
        });
    }
    fs::remove_dir_all(&path).map_err(|source| Error::io("remove package directory", source))?;
    store.delete_package(id)
}

pub fn select_for_project(data_dir: &Path, project_id: &str, package_id: &str) -> Result<()> {
    let store = Store::open(data_dir)?;
    let Some(package) = store.get_package(package_id)? else {
        return Err(Error::PackageNotFound {
            id: package_id.to_string(),
        });
    };
    store.select_package(project_id, &package.name, package_id)
}

pub fn probe_tool(launch: &Path, args: &[String], runtime_dir: Option<&Path>) -> Result<String> {
    command_output(launch, args, runtime_dir)
}

pub fn discover_candidates() -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    if cfg!(windows) {
        let node = PathBuf::from(r"C:\Program Files\nodejs\node.exe");
        let python = PathBuf::from(r"C:\Python\Python312\python.exe");
        if node.is_file() {
            found.push(("node".to_string(), node));
        }
        if python.is_file() {
            found.push(("python".to_string(), python));
        }
    }
    found
}

fn storage_root(data_dir: &Path, kind: &str) -> PathBuf {
    if kind == "tool" {
        data_dir.join("tools")
    } else {
        data_dir.join("packages")
    }
}

fn validate_id(id: &str) -> Result<()> {
    let ok = !id.is_empty()
        && id.len() <= 80
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_');
    if ok {
        Ok(())
    } else {
        Err(Error::InvalidMessage(format!(
            "package id {id:?} is not safe"
        )))
    }
}

fn command_line(executable: &Path, args: &[String]) -> Result<String> {
    let output = Command::new(executable)
        .args(args)
        .output()
        .map_err(|source| Error::io("run package executable", source))?;
    if !output.status.success() {
        return Err(Error::ProbeFailed {
            path: executable.to_path_buf(),
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.lines().next().unwrap_or("").trim().to_string())
}

fn command_output(
    executable: &Path,
    args: &[String],
    runtime_dir: Option<&Path>,
) -> Result<String> {
    let mut command = Command::new(executable);
    command.args(args);
    if let Some(runtime) = runtime_dir {
        let mut path = runtime.display().to_string();
        if let Some(current) = std::env::var_os("PATH") {
            path.push(';');
            path.push_str(&current.to_string_lossy());
        }
        command.env("PATH", path);
    }
    if executable.file_name().and_then(|name| name.to_str()) == Some("uv.exe")
        || executable.file_name().and_then(|name| name.to_str()) == Some("uv")
    {
        command.env("UV_PYTHON_DOWNLOADS", "never");
    }
    let output = command
        .output()
        .map_err(|source| Error::io("run package executable", source))?;
    if !output.status.success() {
        return Err(Error::ProbeFailed {
            path: executable.to_path_buf(),
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim()
        .to_string())
}

struct CountingRead<R> {
    inner: R,
    bytes: Arc<AtomicU64>,
}

impl<R: Read> Read for CountingRead<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buf)?;
        self.bytes.fetch_add(read as u64, Ordering::Relaxed);
        Ok(read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::prepare_data_dir;
    use std::io::{Cursor, Write};

    fn temp_dir() -> PathBuf {
        prepare_data_dir(&crate::paths::disposable_dir("install")).expect("temp")
    }

    fn zip_with(name: &str, bytes: &[u8]) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file(name, options).expect("start");
            writer.write_all(bytes).expect("write");
            writer.finish().expect("finish");
        }
        cursor.into_inner()
    }

    fn package_for(_bytes: &[u8], sha: &str) -> CatalogPackage {
        CatalogPackage {
            id: "fixture-windows-x64".to_string(),
            kind: "runtime".to_string(),
            name: "fixture".to_string(),
            version: "1".to_string(),
            target: crate::catalog::current_target().to_string(),
            format: "zip".to_string(),
            url: "https://example.invalid/fixture.zip".to_string(),
            sha256: sha.to_string(),
            digest_source: "test".to_string(),
            launch: "payload.txt".to_string(),
            version_args: vec!["-v".to_string()],
        }
    }

    #[test]
    fn bad_digest_leaves_no_package_directory() {
        let dir = temp_dir();
        let bytes = zip_with("payload.txt", b"hello");
        let package = package_for(&bytes, &"0".repeat(64));
        let error = install_reader(&dir, &package, Cursor::new(bytes), &Progress::new())
            .expect_err("digest");
        assert!(matches!(error, Error::DigestMismatch { .. }));
        assert!(!dir.join("packages").join("fixture").exists());
        assert!(!dir.join("incoming").join("fixture-windows-x64").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancel_leaves_no_package_directory() {
        let dir = temp_dir();
        let bytes = zip_with("payload.txt", b"hello");
        let digest = {
            use sha2::{Digest, Sha256};
            hex::encode(Sha256::digest(&bytes))
        };
        let package = package_for(&bytes, &digest);
        let progress = Progress::new();
        progress.cancel.store(true, Ordering::Relaxed);
        let error =
            install_reader(&dir, &package, Cursor::new(bytes), &progress).expect_err("cancel");
        assert!(matches!(error, Error::Cancelled { .. }));
        assert!(!dir.join("packages").exists());
        assert!(!dir.join("incoming").join("fixture-windows-x64").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn selected_package_cannot_be_removed_and_existing_reference_is_kept() {
        let dir = temp_dir();
        let bytes = zip_with("payload.txt", b"hello");
        let digest = {
            use sha2::{Digest, Sha256};
            hex::encode(Sha256::digest(&bytes))
        };
        let package = package_for(&bytes, &digest);
        let installed =
            install_reader(&dir, &package, Cursor::new(bytes), &Progress::new()).expect("install");
        select_for_project(&dir, DEFAULT_PROJECT, &installed.id).expect("select");
        let blocked = remove_package(&dir, &installed.id).expect_err("selected");
        assert!(matches!(blocked, Error::NotRemovable { .. }));
        assert!(PathBuf::from(&installed.path).exists());

        let marker = dir.join("outside-python.txt");
        fs::write(&marker, b"not a real python").expect("marker");
        let adopted = adopt_existing_for_test(&dir, &marker);
        assert!(!adopted.removable);
        let before = fs::read(&marker).expect("before");
        let refused = remove_package(&dir, &adopted.id).expect_err("existing");
        assert!(matches!(refused, Error::NotRemovable { .. }));
        assert_eq!(fs::read(&marker).expect("after"), before);
        let _ = fs::remove_dir_all(&dir);
    }

    fn adopt_existing_for_test(data_dir: &Path, executable: &Path) -> PackageRecord {
        let store = Store::open(data_dir).expect("store");
        let record = PackageRecord {
            id: "existing-python".to_string(),
            kind: "runtime".to_string(),
            name: "python".to_string(),
            version: "test".to_string(),
            target: crate::catalog::current_target().to_string(),
            source: "existing".to_string(),
            path: executable.display().to_string(),
            launch: executable.display().to_string(),
            digest: None,
            removable: false,
        };
        store.insert_package(&record).expect("insert");
        record
    }
}

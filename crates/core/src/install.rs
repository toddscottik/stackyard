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
    pub total: Arc<AtomicU64>,
    pub cancel: Arc<AtomicBool>,
}

impl Progress {
    pub fn new() -> Self {
        Self {
            bytes: Arc::new(AtomicU64::new(0)),
            total: Arc::new(AtomicU64::new(0)),
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
    let (_size, sha256, md5) = match read {
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
    let (expected, digest) = if !package.sha256.is_empty() {
        (package.sha256.clone(), sha256)
    } else if !package.md5.is_empty() {
        (package.md5.clone(), md5)
    } else {
        let _ = fs::remove_dir_all(&incoming);
        return Err(Error::InvalidMessage(format!(
            "catalog package {} has no digest",
            package.id
        )));
    };
    if !digest.eq_ignore_ascii_case(&expected) {
        let _ = fs::remove_dir_all(&incoming);
        return Err(Error::DigestMismatch {
            id: package.id.clone(),
            expected,
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
        path: crate::paths::plain_path(final_dir.clone())
            .display()
            .to_string(),
        launch: crate::paths::plain_path(final_dir.join(&package.launch))
            .display()
            .to_string(),
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
    if let Some(total) = response
        .header("Content-Length")
        .and_then(|value| value.trim().parse::<u64>().ok())
    {
        progress.total.store(total, Ordering::Relaxed);
    }
    install_reader(data_dir, package, response.into_reader(), progress)
}

pub fn adopt_existing(data_dir: &Path, role: &str, executable: &Path) -> Result<PackageRecord> {
    if !matches!(
        role,
        "node" | "python" | "php" | "composer" | "pnpm" | "uv" | "apache" | "mysql" | "postgres"
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
    let probed = if role == "php" {
        crate::php::probe_php(executable)?.version
    } else if role == "composer" {
        if executable.extension().and_then(|ext| ext.to_str()) != Some("phar") {
            return Err(Error::InvalidMessage(
                "reference composer.phar so the selected PHP runs Composer".to_string(),
            ));
        }
        let php = crate::project::resolve_runtime(
            data_dir,
            DEFAULT_PROJECT,
            crate::manifest::RuntimeRole::Php,
        )?;
        crate::php_project::probe_composer(Path::new(&php.launch), executable)?
    } else {
        command_line(executable, &args)?
    };
    let version = if role == "apache" {
        crate::apache::parse_apache_version(&probed).ok_or_else(|| Error::ProbeFailed {
            path: executable.to_path_buf(),
            detail: format!("could not read an Apache version from {probed:?}"),
        })?
    } else if role == "uv" || role == "composer" {
        probed
            .split_whitespace()
            .nth(if role == "composer" { 2 } else { 1 })
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
    } else if role == "node" || role == "python" || role == "php" {
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
    if role == "php" {
        if let Err(error) = crate::updates::switch_project(data_dir, DEFAULT_PROJECT, &id) {
            store.delete_package(&id)?;
            return Err(error);
        }
    } else {
        store.select_package(DEFAULT_PROJECT, role, &id)?;
    }
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
        let running = store.package_has_running_project(id)?;
        return Err(Error::NotRemovable {
            id: id.to_string(),
            reason: if running {
                "a running project still uses it".to_string()
            } else {
                "a project selection still uses it".to_string()
            },
        });
    }
    let path = PathBuf::from(&package.path);
    if store
        .list_cache_instances()?
        .iter()
        .any(|instance| instance.package_id == id)
    {
        return Err(Error::NotRemovable {
            id: id.to_string(),
            reason:
                "a cache instance still references this server; stop and forget the instance first"
                    .into(),
        });
    }
    for key in crate::php_project::active_profile_keys(data_dir)? {
        if let Some(profile) = crate::php::read_php_profile(data_dir, &key)? {
            if profile.executable.starts_with(&path)
                || profile
                    .extensions
                    .iter()
                    .any(|extension| extension.root.starts_with(&path))
            {
                return Err(Error::NotRemovable {
                    id: id.to_string(),
                    reason: format!("PHP profile for {} still references this package", key),
                });
            }
        }
    }
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
    crate::updates::switch_project(data_dir, project_id, package_id)
}

pub fn probe_tool(launch: &Path, args: &[String], runtime_dir: Option<&Path>) -> Result<String> {
    command_output(launch, args, runtime_dir)
}

pub fn discover_candidates() -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    if cfg!(windows) {
        push_file(
            &mut found,
            "node",
            PathBuf::from(r"C:\Program Files\nodejs\node.exe"),
        );
        push_file(
            &mut found,
            "python",
            PathBuf::from(r"C:\Python\Python312\python.exe"),
        );
        for root in [
            r"C:\xampp",
            r"C:\XAMPP",
            r"D:\xampp",
            r"D:\XAMPP",
            r"D:\_work\XAMPP",
        ] {
            push_file(&mut found, "php", PathBuf::from(root).join(r"php\php.exe"));
            push_file(
                &mut found,
                "mysql",
                PathBuf::from(root).join(r"mysql\bin\mysqld.exe"),
            );
        }
        if let Ok(entries) = fs::read_dir(r"C:\Program Files\MySQL") {
            for entry in entries.flatten() {
                push_file(&mut found, "mysql", entry.path().join(r"bin\mysqld.exe"));
            }
        }
        if let Ok(entries) = fs::read_dir(r"C:\Program Files\PostgreSQL") {
            for entry in entries.flatten() {
                push_file(
                    &mut found,
                    "postgres",
                    entry.path().join(r"bin\postgres.exe"),
                );
            }
        }
    }
    found
}

fn push_file(found: &mut Vec<(String, PathBuf)>, role: &str, path: PathBuf) {
    if path.is_file() {
        found.push((role.to_string(), path));
    }
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
            md5: String::new(),
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

    #[test]
    fn checked_macos_archives_install_and_launch() {
        if crate::catalog::current_target() != "macos-arm64" {
            return;
        }
        let downloads = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.artifacts/S0-MAC-PACKAGES/downloads");
        let cases = [
            (
                "node-24.22.2-macos-arm64",
                "node-v24.22.2-darwin-arm64.tar.gz",
            ),
            (
                "cpython-3.12.16-macos-arm64",
                "cpython-3.12.16+20261026-aarch64-apple-darwin-install_only_stripped.tar.gz",
            ),
            ("uv-0.13.6-macos-arm64", "uv-aarch64-apple-darwin.tar.gz"),
            ("pnpm-12.16.1-macos-arm64", "pnpm-darwin-arm64.tar.gz"),
        ];
        let mut ran = 0;
        for (id, filename) in cases {
            let archive = downloads.join(filename);
            if !archive.is_file() {
                continue;
            }
            ran += 1;
            let package = crate::catalog::find_package(id).expect(id);
            let dir = temp_dir();
            let file = fs::File::open(&archive).expect("payload");
            let installed =
                install_reader(&dir, &package, file, &Progress::new()).unwrap_or_else(|err| {
                    panic!("{id} install failed: {err}");
                });
            let output = Command::new(&installed.launch)
                .args(&package.version_args)
                .output()
                .unwrap_or_else(|err| panic!("{id} launch failed: {err}"));
            assert!(
                output.status.success(),
                "{id} status {:?} {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
            let text = String::from_utf8_lossy(&output.stdout);
            let expected = package
                .version
                .split('+')
                .next()
                .unwrap_or(&package.version);
            assert!(text.contains(expected), "{id} printed {text:?}");
            if package.name == "node" {
                let (program, mut args) =
                    crate::project::npm_launch(Path::new(&installed.launch)).expect("npm");
                args.push("--version".to_string());
                let npm = Command::new(program).args(args).output().expect("npm");
                assert!(
                    npm.status.success(),
                    "{}",
                    String::from_utf8_lossy(&npm.stderr)
                );
            }
            if package.name == "python" {
                let venv =
                    PathBuf::from(&installed.path).join("python/lib/python3.12/venv/__init__.py");
                assert!(venv.is_file(), "venv module missing at {}", venv.display());
            }
            let _ = fs::remove_dir_all(&dir);
        }
        if ran == 0 {
            eprintln!("skipping checked macOS archive install; payloads are not in .artifacts");
        } else {
            assert_eq!(ran, cases.len(), "partial payload set in {downloads:?}");
        }
    }
}

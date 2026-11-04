//! PHP project adapters. The installer owns artifacts; activation owns no binaries.
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::php::{self, PhpExtensionArtifact, PhpExtensionKind, PhpIdentity, PhpProfile};
use crate::{Error, Result, RuntimeRole, Store};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhpProjectView {
    pub identity: PhpIdentity,
    pub profile: Option<PhpProfile>,
    pub inventory: Vec<PhpExtensionArtifact>,
    pub available: Vec<PhpExtensionDownload>,
    pub limitations: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhpExtensionDownload {
    pub package: crate::CatalogPackage,
    pub name: String,
    pub kind: PhpExtensionKind,
    pub library: PathBuf,
    pub abi: PhpIdentity,
    pub dependencies: Vec<PathBuf>,
}

pub fn php_extension_catalog() -> Result<Vec<PhpExtensionDownload>> {
    let entries: Vec<PhpExtensionDownload> =
        serde_json::from_str(include_str!("../../../catalog/php-extensions.json"))
            .map_err(|e| Error::InvalidMessage(format!("PHP extension catalog: {e}")))?;
    for entry in &entries {
        validate_extension_download(entry)?;
    }
    Ok(entries)
}

fn validate_extension_download(entry: &PhpExtensionDownload) -> Result<()> {
    let package = &entry.package;
    php::validate_php_extension_name(&entry.name)?;
    let target = match entry.abi.target.as_str() {
        "x86_64-pc-windows-msvc" => "windows-x64",
        "aarch64-apple-darwin" => "macos-arm64",
        _ => "unsupported",
    };
    if package.kind != "php-extension"
        || !package.name.starts_with("php-")
        || package.sha256.len() != 64
        || !package.sha256.bytes().all(|c| c.is_ascii_hexdigit())
        || !package.url.starts_with("https://")
        || !package.digest_source.starts_with("https://")
        || package.target != target
        || target == "unsupported"
        || package.launch != entry.library.to_string_lossy()
    {
        return Err(Error::InvalidMessage(format!(
            "PHP extension {} needs approved SHA256 provenance, ABI/target and library metadata",
            package.id
        )));
    }
    for path in std::iter::once(&entry.library).chain(entry.dependencies.iter()) {
        if path.as_os_str().is_empty()
            || path.to_string_lossy().contains([':', '\\'])
            || !path
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_)))
        {
            return Err(Error::InvalidMessage(
                "extension catalog paths must be contained package-relative files".into(),
            ));
        }
    }
    Ok(())
}

pub type PhpLaunchConfig = (Vec<String>, Vec<(String, String)>);

pub fn php_launch_config(
    data_dir: &Path,
    project_id: &str,
    executable: &Path,
) -> Result<PhpLaunchConfig> {
    match php::read_php_profile(data_dir, project_id)? {
        Some(profile) => {
            php::verify_php_profile(executable, &profile)?;
            Ok((
                php::php_profile_args(&profile),
                vec![
                    ("PHPRC".into(), profile.ini_path.display().to_string()),
                    (
                        "PHP_INI_SCAN_DIR".into(),
                        profile.scan_dir.display().to_string(),
                    ),
                    ("PHP_CLI_SERVER_WORKERS".into(), String::new()),
                ],
            ))
        }
        None => Ok((
            vec!["-n".into()],
            vec![
                ("PHPRC".into(), String::new()),
                ("PHP_INI_SCAN_DIR".into(), String::new()),
                ("PHP_CLI_SERVER_WORKERS".into(), String::new()),
            ],
        )),
    }
}

pub fn php_project_view(data_dir: &Path, project_id: &str) -> Result<PhpProjectView> {
    let _ = crate::load_project_manifest(data_dir, project_id)?;
    let runtime = crate::resolve_runtime(data_dir, project_id, RuntimeRole::Php)?;
    let identity = php::probe_php(Path::new(&runtime.launch))?;
    let root = if runtime.source == "existing" {
        Path::new(&runtime.launch)
            .parent()
            .ok_or_else(|| Error::InvalidMessage("PHP has no directory".into()))?
            .to_path_buf()
    } else {
        PathBuf::from(&runtime.path)
    };
    let mut inventory = Vec::new();
    // Windows distributions keep bundled shared modules in ext/. No guessing
    // paths for external macOS builds: those require an explicit reference.
    let ext = root.join("ext");
    if identity.target == "x86_64-pc-windows-msvc" && ext.is_dir() {
        for entry in fs::read_dir(&ext).map_err(|e| Error::io("read PHP extension inventory", e))? {
            let entry = entry.map_err(|e| Error::io("read PHP extension", e))?;
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(name) = name
                .strip_prefix("php_")
                .and_then(|s| s.strip_suffix(".dll"))
            {
                php::validate_php_extension_name(name)?;
                inventory.push(PhpExtensionArtifact {
                    id: format!("bundled-{name}"),
                    name: name.into(),
                    version: runtime.version.clone(),
                    kind: if name == "opcache" {
                        PhpExtensionKind::ZendExtension
                    } else {
                        PhpExtensionKind::Extension
                    },
                    root: root.clone(),
                    library: entry.path(),
                    abi: identity.clone(),
                    dependencies: Vec::new(),
                });
            }
        }
    }
    let installed = Store::open(data_dir)?;
    let catalog = php_extension_catalog()?;
    for item in &catalog {
        if !php::php_abi_compatible(&item.abi, &identity) {
            continue;
        }
        if let Some(record) = installed.get_package(&item.package.id)? {
            inventory.push(PhpExtensionArtifact {
                id: format!("downloaded-{}", item.package.sha256),
                name: item.name.clone(),
                version: record.version,
                kind: item.kind,
                root: PathBuf::from(&record.path),
                library: PathBuf::from(&record.path).join(&item.library),
                abi: item.abi.clone(),
                dependencies: item.dependencies.clone(),
            });
        }
    }
    inventory.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(PhpProjectView {
        identity: identity.clone(), profile: php::read_php_profile(data_dir, project_id)?, inventory,
        available: catalog.into_iter().filter(|item| php::php_abi_compatible(&item.abi, &identity)).collect(),
        limitations: "Extension downloads require a publisher digest and exact ABI metadata. No memcached client pin is approved yet. Managed Memcached is a separate server capability. macOS PHP/artifact launches remain pending.".into(),
    })
}

pub fn apply_project_php_profile(
    data_dir: &Path,
    project_id: &str,
    names: &[String],
) -> Result<PhpProjectView> {
    let project = crate::project_view(data_dir, project_id)?;
    crate::require_startable(&project)?;
    let view = php_project_view(data_dir, project_id)?;
    let mut artifacts = Vec::new();
    for name in names {
        php::validate_php_extension_name(name)?;
        let matches: Vec<_> = view.inventory.iter().filter(|a| a.name == *name).collect();
        if matches.len() != 1 {
            return Err(Error::InvalidMessage(format!(
                "extension {name} is missing or ambiguous; choose an installed compatible artifact"
            )));
        }
        artifacts.push(matches[0].clone());
    }
    let runtime = crate::resolve_runtime(data_dir, project_id, RuntimeRole::Php)?;
    php::apply_php_profile(data_dir, project_id, Path::new(&runtime.launch), &artifacts)?;
    php_project_view(data_dir, project_id)
}

/// Version-only probe; dependency scripts use the engine's cancellable process group.
pub fn probe_composer(php: &Path, phar: &Path) -> Result<String> {
    let mut command = Command::new(php);
    php::configure_php_unprofiled(&mut command);
    command
        .arg(phar)
        .args(["--no-ansi", "--version"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|e| Error::io("probe Composer with selected PHP", e))?;
    let output = child.stdout.take().expect("piped");
    let (sender, receiver) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = output
            .take(64 * 1024)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = sender.send(result);
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| Error::io("wait for Composer probe", e))?
        {
            break status;
        }
        if started.elapsed() > Duration::from_secs(10) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::OperationFailed(
                "Composer version probe timed out".into(),
            ));
        }
        thread::sleep(Duration::from_millis(20));
    };
    let bytes = receiver
        .recv_timeout(Duration::from_secs(10).saturating_sub(started.elapsed()))
        .map_err(|_| Error::OperationFailed("Composer output probe timed out".into()))?
        .map_err(|e| Error::io("read Composer version", e))?;
    let version = String::from_utf8_lossy(&bytes)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if !status.success() || !version.starts_with("Composer version ") {
        return Err(Error::ProbeFailed {
            path: phar.to_path_buf(),
            detail: "not a working Composer phar for the selected PHP".into(),
        });
    }
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate() -> PhpExtensionDownload {
        PhpExtensionDownload {
            package: crate::CatalogPackage {
                id: "php-extension-fixture".into(),
                kind: "php-extension".into(),
                name: "php-fixture".into(),
                version: "1.0".into(),
                target: "windows-x64".into(),
                format: "zip".into(),
                url: "https://example.invalid/fixture.zip".into(),
                sha256: "a".repeat(64),
                md5: String::new(),
                digest_source: "https://example.invalid/checksums".into(),
                launch: "ext/fixture.dll".into(),
                version_args: Vec::new(),
            },
            name: "fixture".into(),
            kind: PhpExtensionKind::Extension,
            library: "ext/fixture.dll".into(),
            dependencies: vec!["native/dependency.dll".into()],
            abi: PhpIdentity {
                version: "8.5.12".into(),
                api: "20250925".into(),
                zend_api: "420250925".into(),
                target: "x86_64-pc-windows-msvc".into(),
                architecture: "x86_64".into(),
                thread_safe: false,
                debug: false,
                compiler: "Visual C++ 2022".into(),
                extension_build: "API20250925,NTS,VS17".into(),
                zend_extension_build: "API420250925,NTS,VS17".into(),
            },
        }
    }

    #[test]
    fn extension_catalog_requires_sha256_target_and_contained_native_files() {
        assert!(validate_extension_download(&candidate()).is_ok());
        let mut entry = candidate();
        entry.package.sha256.clear();
        entry.package.md5 = "a".repeat(32);
        assert!(validate_extension_download(&entry).is_err());
        let mut entry = candidate();
        entry.package.target = "macos-arm64".into();
        assert!(validate_extension_download(&entry).is_err());
        let mut entry = candidate();
        entry.dependencies = vec!["../outside.dll".into()];
        assert!(validate_extension_download(&entry).is_err());
        let mut entry = candidate();
        entry.library = "C:/outside.dll".into();
        entry.package.launch = "C:/outside.dll".into();
        assert!(validate_extension_download(&entry).is_err());
    }

    #[test]
    fn composer_plan_pins_manifest_and_removes_ambient_output_directories() {
        let root = crate::prepare_data_dir(&crate::disposable_dir("composer-plan")).unwrap();
        fs::write(root.join("composer.json"), "{}").unwrap();
        let commands = composer_plan_fixture(&root);
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].program, PathBuf::from("chosen-php"));
        assert_eq!(&commands[0].args[..2], &["-n", "composer.phar"]);
        assert!(commands[0].env.iter().any(
            |(key, value)| key == "COMPOSER" && Path::new(value) == root.join("composer.json")
        ));
        assert!(commands[0]
            .env_remove
            .iter()
            .any(|key| key == "COMPOSER_VENDOR_DIR"));
        assert!(commands[0]
            .env_remove
            .iter()
            .any(|key| key == "COMPOSER_BIN_DIR"));
        assert_eq!(commands[1].args.last().unwrap(), "check-platform-reqs");
        fs::remove_dir_all(root).unwrap();
    }

    fn composer_plan_fixture(root: &Path) -> Vec<crate::PlannedCommand> {
        let text = "schema=1\nname='composer-plan'\n[[service]]\nid='web'\nruntime='php'\ncommand=['php','-S','127.0.0.1:{port}']\n";
        let manifest = crate::manifest::parse_manifest(text).unwrap();
        crate::plan_dependency(
            root,
            "composer-plan",
            root,
            &manifest,
            crate::DepRequest {
                action: crate::DepAction::Install,
                package: None,
                scope: ".",
            },
            crate::Toolchain {
                node: None,
                python: None,
                pnpm: None,
                uv: None,
                venv_python: None,
                php: Some(Path::new("chosen-php")),
                composer: Some(Path::new("composer.phar")),
            },
        )
        .unwrap()
        .commands
    }
}

//! PHP ABI probes and project-owned extension profiles.
//!
//! Artifact installation/verification belongs to the package provider. These
//! services only reference installed files and never edit a PHP installation.

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::{plain_path, Error, Result};

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const OUTPUT_LIMIT: usize = 256 * 1024;
const IDENTITY_SCRIPT: &str = r#"ob_start(); phpinfo(INFO_GENERAL); $info=ob_get_clean(); echo json_encode(['version'=>PHP_VERSION,'thread_safe'=>(bool)PHP_ZTS,'debug'=>(bool)PHP_DEBUG,'os'=>PHP_OS_FAMILY,'machine'=>php_uname('m'),'int_size'=>PHP_INT_SIZE,'info'=>$info], JSON_THROW_ON_ERROR);"#;
const PROFILE_SCRIPT: &str = r#"echo json_encode(['modules'=>get_loaded_extensions(),'zend_modules'=>get_loaded_extensions(true),'loaded_ini'=>php_ini_loaded_file() ?: null,'scanned_ini'=>php_ini_scanned_files() ?: null], JSON_THROW_ON_ERROR);"#;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhpIdentity {
    pub version: String,
    pub api: String,
    pub zend_api: String,
    pub target: String,
    pub architecture: String,
    pub thread_safe: bool,
    pub debug: bool,
    pub compiler: String,
    pub extension_build: String,
    pub zend_extension_build: String,
}

/// Patch releases may share an ABI. Version-line compatibility is necessary
/// alongside all build fields, and still requires a successful native load.
pub fn php_abi_compatible(left: &PhpIdentity, right: &PhpIdentity) -> bool {
    fn version_line(version: &str) -> Option<(u32, u32)> {
        let mut parts = version.split('.');
        let major_text = parts.next()?;
        let minor_text = parts.next()?;
        let patch = parts.next()?;
        if !major_text.bytes().all(|c| c.is_ascii_digit())
            || !minor_text.bytes().all(|c| c.is_ascii_digit())
            || !patch.as_bytes().first()?.is_ascii_digit()
            || parts.next().is_some()
        {
            return None;
        }
        let major = major_text.parse().ok()?;
        let minor = minor_text.parse().ok()?;
        Some((major, minor))
    }
    let Some(line) = version_line(&left.version) else {
        return false;
    };
    version_line(&right.version) == Some(line)
        && left.api == right.api
        && left.zend_api == right.zend_api
        && left.target == right.target
        && left.architecture == right.architecture
        && left.thread_safe == right.thread_safe
        && left.debug == right.debug
        && left.compiler == right.compiler
        && left.extension_build == right.extension_build
        && left.zend_extension_build == right.zend_extension_build
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhpExtensionKind {
    Extension,
    ZendExtension,
}

/// Inventory is independent of activation. `root` must be an installed,
/// verified package root supplied by the provider, never a download candidate.
/// A bundled DLL is an artifact too; statically compiled modules need no entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhpExtensionArtifact {
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: PhpExtensionKind,
    pub root: PathBuf,
    pub library: PathBuf,
    pub abi: PhpIdentity,
    /// Native libraries, recorded relative to or inside `root`.
    pub dependencies: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhpProfile {
    pub project_id: String,
    pub revision: String,
    pub executable: PathBuf,
    pub identity: PhpIdentity,
    pub extensions: Vec<PhpExtensionArtifact>,
    pub ini_path: PathBuf,
    pub scan_dir: PathBuf,
}

#[derive(Deserialize)]
struct IdentityOutput {
    version: String,
    thread_safe: bool,
    debug: bool,
    os: String,
    machine: String,
    int_size: usize,
    info: String,
}

#[derive(Deserialize)]
struct ProfileOutput {
    modules: Vec<String>,
    zend_modules: Vec<String>,
    loaded_ini: Option<String>,
    scanned_ini: Option<String>,
}

/// Probe the selected executable with `-n`; inherited ini settings cannot load
/// native code or alter the identity this reports.
pub fn probe_php(executable: &Path) -> Result<PhpIdentity> {
    let mut command = isolated_php_command(executable);
    command.args(["-n", "-r", IDENTITY_SCRIPT]);
    let output = run_probe(&mut command, executable, PROBE_TIMEOUT)?;
    let identity = parse_identity(&output).map_err(|detail| probe_error(executable, detail))?;
    // php_uname describes the running OS. Cross-check the executable header so
    // an emulated or 32-bit runtime cannot inherit the host's architecture.
    let mut header = Vec::new();
    fs::File::open(executable)
        .and_then(|file| file.take(4096).read_to_end(&mut header))
        .map_err(|e| Error::io("inspect selected PHP executable architecture", e))?;
    if executable_architecture(&header).is_some_and(|arch| arch != identity.architecture) {
        return Err(probe_error(
            executable,
            "PHP executable architecture disagrees with its runtime probe",
        ));
    }
    Ok(identity)
}

fn executable_architecture(header: &[u8]) -> Option<&'static str> {
    let machine = if header.starts_with(b"MZ") {
        let offset = u32::from_le_bytes(header.get(60..64)?.try_into().ok()?) as usize;
        if header.get(offset..offset.checked_add(4)?)? != b"PE\0\0" {
            return None;
        }
        match u16::from_le_bytes(header.get(offset + 4..offset + 6)?.try_into().ok()?) {
            0x8664 => return Some("x86_64"),
            0xaa64 => return Some("aarch64"),
            0x014c => return Some("x86"),
            _ => return None,
        }
    } else if header.starts_with(b"\x7fELF") {
        let bytes = header.get(18..20)?.try_into().ok()?;
        match header.get(5)? {
            1 => u16::from_le_bytes(bytes) as u32,
            2 => u16::from_be_bytes(bytes) as u32,
            _ => return None,
        }
    } else if header.starts_with(&[0xcf, 0xfa, 0xed, 0xfe]) {
        match u32::from_le_bytes(header.get(4..8)?.try_into().ok()?) {
            0x0100_0007 => return Some("x86_64"),
            0x0100_000c => return Some("aarch64"),
            _ => return None,
        }
    } else {
        return None;
    };
    match machine {
        62 => Some("x86_64"),
        183 => Some("aarch64"),
        3 => Some("x86"),
        _ => None,
    }
}

/// Configure an unprofiled process. Callers append their script/CGI arguments.
/// Apply project environments first, so they cannot override this isolation.
pub fn configure_php_unprofiled(command: &mut Command) {
    command.arg("-n");
    clean_php_environment(command);
}

/// A conservative filter: all observed ABI fields must match. A subsequent
/// real load probe is mandatory even when provider metadata matches.
pub fn compatible_extensions<'a>(
    identity: &PhpIdentity,
    inventory: &'a [PhpExtensionArtifact],
) -> Vec<&'a PhpExtensionArtifact> {
    inventory
        .iter()
        .filter(|item| php_abi_compatible(&item.abi, identity))
        .collect()
}

pub fn validate_php_extension_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 64
        || !name.as_bytes()[0].is_ascii_alphabetic()
        || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err(Error::InvalidMessage(format!(
            "invalid PHP extension name {name:?}; use an ASCII letter followed by letters, digits, or underscores"
        )));
    }
    Ok(())
}

/// Create an immutable generation, validate it in a separate PHP process, then
/// atomically replace the sole active pointer. A failed probe leaves the prior
/// profile and its ini intact, including on Windows.
pub fn apply_php_profile(
    data_dir: &Path,
    project_id: &str,
    executable: &Path,
    artifacts: &[PhpExtensionArtifact],
) -> Result<PhpProfile> {
    validate_component(project_id)?;
    let executable = canonical_file(executable)?;
    let identity = probe_php(&executable)?;
    let artifacts = validate_artifacts(&identity, artifacts)?;
    let root = profile_root(data_dir, project_id);
    fs::create_dir_all(&root).map_err(|e| Error::io("create PHP profile root", e))?;
    let root = canonical(&root)?;
    // An OS-owned lock also prevents independent callers from promoting
    // competing generations. It is released on process exit, including crash.
    let lock_path = root.join(".lock");
    let _lock = ProfileLock::acquire(lock_path)?;
    let revision = unique_revision()?;
    let generation = root.join(&revision);
    fs::create_dir(&generation).map_err(|e| Error::io("create PHP profile generation", e))?;
    let result = (|| {
        let scan_dir = generation.join("scan");
        fs::create_dir(&scan_dir)
            .map_err(|e| Error::io("create empty PHP ini scan directory", e))?;
        let ini_path = generation.join("php.ini");
        write_new(&ini_path, render_ini(&artifacts)?.as_bytes())?;
        let profile = PhpProfile {
            project_id: project_id.to_string(),
            revision,
            executable,
            identity,
            extensions: artifacts,
            ini_path,
            scan_dir,
        };
        verify_php_profile(&profile.executable, &profile)?;
        let temporary = root.join(format!("{}.json", profile.revision));
        let bytes = serde_json::to_vec_pretty(&profile)
            .map_err(|e| Error::InvalidMessage(format!("serialize PHP profile: {e}")))?;
        if bytes.len() > OUTPUT_LIMIT {
            return Err(Error::InvalidMessage(
                "PHP profile record exceeds 256 KiB".into(),
            ));
        }
        write_new(&temporary, &bytes)?;
        if let Err(error) = replace_file(&temporary, &root.join("active.json")) {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        Ok(profile)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&generation);
    }
    result
}

pub fn read_php_profile(data_dir: &Path, project_id: &str) -> Result<Option<PhpProfile>> {
    validate_component(project_id)?;
    let root = profile_root(data_dir, project_id);
    let path = root.join("active.json");
    let bytes = match read_bounded_file(&path) {
        Ok(bytes) => bytes,
        Err(Error::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None)
        }
        Err(e) => return Err(e),
    };
    let profile: PhpProfile = serde_json::from_slice(&bytes)
        .map_err(|e| Error::InvalidMessage(format!("read PHP profile record: {e}")))?;
    validate_component(&profile.revision)?;
    let generation = canonical(&root.join(&profile.revision))?;
    if profile.project_id != project_id
        || canonical_file(&profile.ini_path)? != generation.join("php.ini")
        || canonical(&profile.scan_dir)? != generation.join("scan")
        || !generation.starts_with(canonical(&root)?)
    {
        return Err(Error::InvalidMessage(
            "PHP profile record escapes its project generation".into(),
        ));
    }
    Ok(Some(profile))
}

/// Revalidate both the actual executable and actual extension loads before a
/// runtime switch or worker launch. This does not promote or modify a profile.
pub fn verify_php_profile(executable: &Path, profile: &PhpProfile) -> Result<()> {
    let actual = probe_php(executable)?;
    if !php_abi_compatible(&actual, &profile.identity) {
        return Err(probe_error(
            executable,
            "selected PHP ABI changed; resolve a new extension profile",
        ));
    }
    let artifacts = validate_artifacts(&actual, &profile.extensions)?;
    let expected_ini = render_ini(&artifacts)?;
    if read_bounded_file(&profile.ini_path)? != expected_ini.as_bytes() {
        return Err(probe_error(
            executable,
            "generated PHP ini differs from the recorded extension profile",
        ));
    }
    if fs::read_dir(&profile.scan_dir)
        .map_err(|e| Error::io("inspect isolated PHP scan directory", e))?
        .next()
        .is_some()
    {
        return Err(probe_error(
            executable,
            "isolated PHP scan directory must be empty",
        ));
    }
    let mut command = isolated_php_command(executable);
    configure_php_command(&mut command, profile)?;
    command.args(["-r", PROFILE_SCRIPT]);
    let output = run_probe(&mut command, executable, PROBE_TIMEOUT)?;
    let loaded: ProfileOutput = serde_json::from_str(&output).map_err(|e| {
        probe_error(
            executable,
            format!("invalid extension-load probe output: {e}"),
        )
    })?;
    if loaded.scanned_ini.as_deref().is_some_and(|s| !s.is_empty()) {
        return Err(probe_error(
            executable,
            "PHP loaded additional ini files outside the generated profile",
        ));
    }
    let loaded_ini = loaded
        .loaded_ini
        .ok_or_else(|| probe_error(executable, "PHP did not load the candidate ini"))?;
    if canonical_file(Path::new(&loaded_ini))? != canonical_file(&profile.ini_path)? {
        return Err(probe_error(
            executable,
            "PHP loaded a different ini than the candidate",
        ));
    }
    for artifact in artifacts {
        let modules = match artifact.kind {
            PhpExtensionKind::Extension => &loaded.modules,
            PhpExtensionKind::ZendExtension => &loaded.zend_modules,
        };
        if !modules.iter().any(|name| {
            name.eq_ignore_ascii_case(&artifact.name)
                || name
                    .strip_prefix("Zend ")
                    .is_some_and(|name| name.eq_ignore_ascii_case(&artifact.name))
        }) {
            return Err(probe_error(
                executable,
                format!(
                    "extension {} did not load; prior profile retained",
                    artifact.name
                ),
            ));
        }
    }
    Ok(())
}

/// Arguments work for both CLI and CGI. The scan environment must also be
/// supplied with `configure_php_command` or equivalent environment removals.
pub fn php_profile_args(profile: &PhpProfile) -> Vec<String> {
    vec![
        "-c".to_string(),
        profile.ini_path.to_string_lossy().into_owned(),
    ]
}

/// Apply after project environment overrides. `-c` selects the generated ini;
/// an explicit empty directory overrides PHP's compiled-in scan directory.
pub fn configure_php_command(command: &mut Command, profile: &PhpProfile) -> Result<()> {
    if !profile.ini_path.is_file() || !profile.scan_dir.is_dir() {
        return Err(Error::InvalidMessage(
            "PHP profile ini or isolated scan directory is missing".into(),
        ));
    }
    clean_php_environment(command);
    command.args(php_profile_args(profile));
    command.env("PHP_INI_SCAN_DIR", &profile.scan_dir);
    Ok(())
}

fn isolated_php_command(executable: &Path) -> Command {
    let mut command = Command::new(executable);
    command.current_dir(executable.parent().unwrap_or(Path::new(".")));
    clean_php_environment(&mut command);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command
}

fn clean_php_environment(command: &mut Command) {
    command.env_remove("PHPRC");
    command.env_remove("PHP_INI_SCAN_DIR");
    command.env_remove("PHP_CLI_SERVER_WORKERS");
}

fn parse_identity(output: &str) -> std::result::Result<PhpIdentity, String> {
    let raw: IdentityOutput = serde_json::from_str(output)
        .map_err(|e| format!("invalid isolated PHP identity output: {e}"))?;
    let field = |name: &str| {
        raw.info.lines().find_map(|line| {
            let (key, value) = line.split_once(" => ")?;
            (key.trim() == name).then(|| value.trim().to_string())
        })
    };
    let required = |name: &str| {
        field(name)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("PHP identity is missing {name}"))
    };
    let extension_build = required("PHP Extension Build")?;
    let zend_extension_build = required("Zend Extension Build")?;
    let os = match raw.os.as_str() {
        "Windows" => "pc-windows-msvc",
        "Darwin" => "apple-darwin",
        "Linux" => "unknown-linux-gnu",
        _ => return Err(format!("unsupported PHP operating system {}", raw.os)),
    };
    let machine = field("Architecture")
        .unwrap_or(raw.machine)
        .to_ascii_lowercase();
    let architecture = match (machine.as_str(), raw.int_size) {
        ("x64" | "amd64" | "x86_64", 8) => "x86_64",
        ("arm64" | "aarch64", 8) => "aarch64",
        ("x86" | "i386" | "i686" | "x64" | "amd64" | "x86_64", 4) => "x86",
        _ => {
            return Err(format!(
                "unsupported PHP architecture {machine} with {}-byte integers",
                raw.int_size
            ))
        }
    }
    .to_string();
    let compiler = field("Compiler").unwrap_or_else(|| format!("build:{extension_build}"));
    if raw.version.is_empty() {
        return Err("PHP did not report a version".into());
    }
    Ok(PhpIdentity {
        version: raw.version,
        api: required("PHP API")?,
        zend_api: required("Zend Extension")?,
        target: format!("{architecture}-{os}"),
        architecture,
        thread_safe: raw.thread_safe,
        debug: raw.debug,
        compiler,
        extension_build,
        zend_extension_build,
    })
}

fn validate_artifacts(
    identity: &PhpIdentity,
    artifacts: &[PhpExtensionArtifact],
) -> Result<Vec<PhpExtensionArtifact>> {
    if artifacts.len() > 128 {
        return Err(Error::InvalidMessage(
            "a PHP profile cannot activate more than 128 artifacts".into(),
        ));
    }
    let mut names = BTreeSet::new();
    let mut ids = BTreeSet::new();
    artifacts.iter().map(|artifact| {
        validate_php_extension_name(&artifact.name)?;
        validate_component(&artifact.id)?;
        if !names.insert(artifact.name.to_ascii_lowercase()) || !ids.insert(&artifact.id) {
            return Err(Error::InvalidMessage(format!("duplicate PHP extension {}", artifact.name)));
        }
            if !php_abi_compatible(&artifact.abi, identity) {
            return Err(Error::InvalidMessage(format!("extension {} ABI does not match selected PHP {}; resolve a compatible artifact before activation", artifact.name, identity.version)));
        }
        let root = canonical(&artifact.root)?;
        let resolve = |path: &Path| -> Result<PathBuf> {
            let path = if path.is_absolute() { path.to_path_buf() } else { root.join(path) };
            let path = canonical_file(&path)?;
            if !path.starts_with(&root) {
                return Err(Error::InvalidMessage(format!("PHP extension {} file escapes installed package root", artifact.name)));
            }
            ini_value(&path)?;
            Ok(path)
        };
        let mut artifact = artifact.clone();
        artifact.library = resolve(&artifact.library)?;
        artifact.dependencies = artifact.dependencies.iter().map(|path| resolve(path)).collect::<Result<_>>()?;
        artifact.root = root;
        Ok(artifact)
    }).collect()
}

fn render_ini(artifacts: &[PhpExtensionArtifact]) -> Result<String> {
    let mut ini = String::from("; Generated by Stackyard. Edit the project's extension selection instead.\n[PHP]\ndisplay_errors=stderr\ndisplay_startup_errors=1\nlog_errors=0\nhtml_errors=0\n");
    for artifact in artifacts {
        let directive = match artifact.kind {
            PhpExtensionKind::Extension => "extension",
            PhpExtensionKind::ZendExtension => "zend_extension",
        };
        ini.push_str(&format!(
            "{directive}=\"{}\"\n",
            ini_value(&artifact.library)?
        ));
    }
    Ok(ini)
}

fn ini_value(path: &Path) -> Result<String> {
    let value = path
        .to_str()
        .ok_or_else(|| Error::InvalidMessage("PHP ini paths must be valid Unicode".into()))?;
    // Reject expansion/interpolation as well as line and quote injection.
    if value
        .chars()
        .any(|c| c.is_control() || matches!(c, '"' | '$' | ';'))
    {
        return Err(Error::InvalidMessage(
            "PHP ini path contains unsafe characters".into(),
        ));
    }
    Ok(value.replace('\\', "/"))
}

fn canonical(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .map(plain_path)
        .map_err(|e| Error::io("resolve PHP profile path", e))
}

fn canonical_file(path: &Path) -> Result<PathBuf> {
    let path = canonical(path)?;
    if !path.is_file() {
        return Err(Error::InvalidMessage(format!(
            "PHP file {} is missing",
            path.display()
        )));
    }
    Ok(path)
}

fn profile_root(data_dir: &Path, project_id: &str) -> PathBuf {
    data_dir.join("php").join("profiles").join(project_id)
}

fn validate_component(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
    {
        return Err(Error::InvalidMessage(format!(
            "invalid PHP profile identifier {value:?}"
        )));
    }
    Ok(())
}

fn unique_revision() -> Result<String> {
    let mut bytes = [0; 16];
    getrandom::getrandom(&mut bytes)
        .map_err(|e| Error::OperationFailed(format!("create PHP profile revision: {e}")))?;
    Ok(format!("r-{}", hex::encode(bytes)))
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| Error::io("create PHP profile file", e))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| Error::io("write PHP profile file", e))
}

fn read_bounded_file(path: &Path) -> Result<Vec<u8>> {
    let file = fs::File::open(path).map_err(|e| Error::io("open PHP profile file", e))?;
    let mut bytes = Vec::new();
    file.take((OUTPUT_LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::io("read PHP profile file", e))?;
    if bytes.len() > OUTPUT_LIMIT {
        return Err(Error::InvalidMessage(
            "PHP profile file exceeds 256 KiB".into(),
        ));
    }
    Ok(bytes)
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> Result<()> {
    fs::rename(source, destination).map_err(|e| Error::io("promote PHP profile", e))
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH, on one volume.
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 0x1 | 0x8) } == 0 {
        return Err(Error::io(
            "promote PHP profile",
            std::io::Error::last_os_error(),
        ));
    }
    Ok(())
}

struct ProfileLock {
    _file: fs::File,
}

impl ProfileLock {
    fn acquire(path: PathBuf) -> Result<Self> {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0);
        }
        let file = options
            .open(path)
            .map_err(|e| Error::io("acquire exclusive PHP profile lock", e))?;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return Err(Error::io(
                    "acquire exclusive PHP profile lock",
                    std::io::Error::last_os_error(),
                ));
            }
        }
        Ok(Self { _file: file })
    }
}

fn probe_error(executable: &Path, detail: impl Into<String>) -> Error {
    Error::ProbeFailed {
        path: executable.to_path_buf(),
        detail: detail.into(),
    }
}

/// Readers stop at the limit rather than accumulating unbounded diagnostics.
/// The controller never joins a pipe reader: a descendant retaining a pipe
/// cannot make the caller wait past its deadline.
fn run_probe(command: &mut Command, executable: &Path, timeout: Duration) -> Result<String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|e| Error::io("start isolated PHP probe", e))?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let (send, receive) = mpsc::channel();
    for (index, mut reader) in [
        (0, Box::new(stdout) as Box<dyn Read + Send>),
        (1, Box::new(stderr)),
    ] {
        let send = send.clone();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = (&mut reader)
                .take((OUTPUT_LIMIT + 1) as u64)
                .read_to_end(&mut bytes);
            let _ = send.send((index, result.map(|_| bytes)));
        });
    }
    drop(send);
    let start = Instant::now();
    let mut streams = [None, None];
    let mut status = None;
    let result = loop {
        while let Ok((index, result)) = receive.try_recv() {
            let bytes = match result {
                Ok(bytes) => bytes,
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Error::io("read PHP probe output", e));
                }
            };
            if bytes.len() > OUTPUT_LIMIT {
                let _ = child.kill();
                let _ = child.wait();
                return Err(probe_error(
                    executable,
                    "PHP probe exceeded the 256 KiB output limit",
                ));
            }
            streams[index] = Some(bytes);
        }
        if status.is_none() {
            match child.try_wait() {
                Ok(value) => status = value,
                Err(e) => break Err(Error::io("poll PHP probe", e)),
            }
        }
        if let (Some(status), [Some(stdout), Some(stderr)]) = (&status, &streams) {
            let stdout = String::from_utf8_lossy(stdout).into_owned();
            let stderr = String::from_utf8_lossy(stderr);
            if !status.success() || !stderr.trim().is_empty() {
                break Err(probe_error(
                    executable,
                    format!("isolated PHP probe exited {status}; {}", stderr.trim()),
                ));
            }
            break Ok(stdout);
        }
        if start.elapsed() >= timeout {
            break Err(probe_error(
                executable,
                format!(
                    "isolated PHP probe timed out after {} ms",
                    timeout.as_millis()
                ),
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> PhpIdentity {
        PhpIdentity {
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
        }
    }

    fn artifact(root: &Path, abi: PhpIdentity) -> PhpExtensionArtifact {
        PhpExtensionArtifact {
            id: "php-mysqli".into(),
            name: "mysqli".into(),
            version: abi.version.clone(),
            kind: PhpExtensionKind::Extension,
            root: root.to_path_buf(),
            library: root.join("ext").join("php_mysqli.dll"),
            abi,
            dependencies: Vec::new(),
        }
    }

    #[test]
    fn compatibility_checks_every_abi_dimension() {
        let expected = identity();
        let root = Path::new("unused");
        let good = artifact(root, expected.clone());
        let mut variants = Vec::new();
        for field in 0..10 {
            let mut changed = expected.clone();
            match field {
                0 => changed.version = "8.6.0".into(),
                1 => changed.api.push('x'),
                2 => changed.zend_api.push('x'),
                3 => changed.target.push('x'),
                4 => changed.architecture.push('x'),
                5 => changed.thread_safe = !changed.thread_safe,
                6 => changed.debug = !changed.debug,
                7 => changed.compiler.push('x'),
                8 => changed.extension_build.push('x'),
                _ => changed.zend_extension_build.push('x'),
            }
            variants.push(artifact(root, changed));
        }
        variants.push(good.clone());
        assert_eq!(compatible_extensions(&expected, &variants), vec![&good]);
        let mut patch = expected.clone();
        patch.version = "8.5.13".into();
        assert!(php_abi_compatible(&expected, &patch));
        for version in [
            "", "8", "8.5", "8.x.13", "8.5.", "8.5.evil", "+8.5.13", "8.5.13.0",
        ] {
            patch.version = version.into();
            assert!(!php_abi_compatible(&expected, &patch));
        }
        // Mismatch rejection precedes reading any library file.
        assert!(validate_artifacts(&expected, &variants[..1])
            .unwrap_err()
            .to_string()
            .contains("ABI"));
    }

    #[test]
    fn identifiers_and_ini_paths_reject_injection() {
        for name in [
            "",
            "../memcached",
            "extension=evil",
            "a\nb",
            "1ext",
            "Zend OPcache",
            "éxt",
        ] {
            assert!(validate_php_extension_name(name).is_err(), "{name}");
        }
        for name in ["memcached", "pdo_mysql", "Xdebug", "opcache"] {
            validate_php_extension_name(name).unwrap();
        }
        for value in ["../project", "a/b", "a.b", "", "C:\\project"] {
            assert!(validate_component(value).is_err());
        }
        for path in [
            "/tmp/evil\".so",
            "/tmp/${EXT}.so",
            "/tmp/a\nb.so",
            "/tmp/a;b.so",
        ] {
            assert!(ini_value(Path::new(path)).is_err());
        }
    }

    #[test]
    fn identity_is_read_from_observed_php_information() {
        let output = serde_json::json!({
            "version":"8.5.12", "thread_safe":false, "debug":false,
            "os":"Windows", "machine":"AMD64", "int_size":8,
            "info":"PHP API => 20250925\nZend Extension => 420250925\nPHP Extension Build => API20250925,NTS,VS17\nZend Extension Build => API420250925,NTS,VS17\nCompiler => Visual C++ 2022\nArchitecture => x64\n"
        });
        assert_eq!(parse_identity(&output.to_string()).unwrap(), identity());
        assert!(parse_identity("{}").is_err());
        let mut missing = output;
        missing["info"] = serde_json::Value::String("PHP API => 20250925".into());
        assert!(parse_identity(&missing.to_string()).is_err());
    }

    #[test]
    fn immutable_package_paths_and_dependencies_are_checked() {
        let root = crate::disposable_dir("php-artifact");
        let outside = crate::disposable_dir("php-outside");
        fs::create_dir_all(root.join("ext")).unwrap();
        fs::write(root.join("ext/php_mysqli.dll"), b"fixture").unwrap();
        fs::write(&outside, b"outside").unwrap();
        let mut entry = artifact(&root, identity());
        assert!(validate_artifacts(&identity(), &[entry.clone()]).is_ok());
        entry.dependencies = vec![outside.clone()];
        assert!(validate_artifacts(&identity(), &[entry.clone()]).is_err());
        entry.dependencies.clear();
        entry.library = outside.clone();
        assert!(validate_artifacts(&identity(), &[entry]).is_err());
        fs::remove_dir_all(root).unwrap();
        fs::remove_file(outside).unwrap();
    }

    #[test]
    fn promotion_replaces_pointer_without_mutating_old_generation() {
        let root = crate::disposable_dir("php-promotion");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("old.ini"), b"old generation").unwrap();
        fs::write(root.join("active.json"), b"old pointer").unwrap();
        write_new(&root.join("new.json"), b"new pointer").unwrap();
        replace_file(&root.join("new.json"), &root.join("active.json")).unwrap();
        assert_eq!(fs::read(root.join("active.json")).unwrap(), b"new pointer");
        assert_eq!(fs::read(root.join("old.ini")).unwrap(), b"old generation");
        assert!(replace_file(&root.join("absent.json"), &root.join("active.json")).is_err());
        assert_eq!(fs::read(root.join("active.json")).unwrap(), b"new pointer");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_lock_rejects_concurrent_promotion() {
        let root = crate::disposable_dir("php-lock");
        fs::create_dir_all(&root).unwrap();
        let lock = ProfileLock::acquire(root.join(".lock")).unwrap();
        assert!(ProfileLock::acquire(root.join(".lock")).is_err());
        drop(lock);
        drop(ProfileLock::acquire(root.join(".lock")).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    /// Opt-in evidence using the artifact worker's digest-checked executable.
    /// An absent variable means no native PHP compatibility claim is made.
    #[test]
    fn real_php_profiles_keep_previous_on_failure_and_isolate_projects() {
        let Some(executable) = std::env::var_os("STACKYARD_TEST_PHP").map(PathBuf::from) else {
            return;
        };
        let root = crate::disposable_dir("php-runtime");
        fs::create_dir_all(&root).unwrap();
        let abi = probe_php(&executable).unwrap();
        let mut entry = artifact(executable.parent().unwrap(), abi.clone());
        entry.id = "php-openssl".into();
        entry.name = "openssl".into();
        entry.library = entry.root.join("ext/php_openssl.dll");
        let first = apply_php_profile(&root, "first", &executable, &[entry.clone()]).unwrap();
        let second = apply_php_profile(&root, "second", &executable, &[]).unwrap();
        verify_php_profile(&executable, &first).unwrap();
        verify_php_profile(&executable, &second).unwrap();
        assert!(second.extensions.is_empty());
        for (profile, expected) in [(&first, "yes"), (&second, "no")] {
            let mut command = isolated_php_command(&executable);
            configure_php_command(&mut command, profile).unwrap();
            command.args(["-r", "echo extension_loaded('openssl') ? 'yes' : 'no';"]);
            assert_eq!(
                run_probe(&mut command, &executable, PROBE_TIMEOUT).unwrap(),
                expected
            );
        }
        let before = fs::read(profile_root(&root, "first").join("active.json")).unwrap();
        let original_ini = fs::read(&first.ini_path).unwrap();
        let mut bad_abi = entry.clone();
        bad_abi.abi.thread_safe = !bad_abi.abi.thread_safe;
        assert!(apply_php_profile(&root, "first", &executable, &[bad_abi]).is_err());
        fs::write(root.join("not-a-library.dll"), b"invalid native library").unwrap();
        let mut unloadable = entry;
        unloadable.root = root.clone();
        unloadable.library = root.join("not-a-library.dll");
        assert!(apply_php_profile(&root, "first", &executable, &[unloadable]).is_err());
        assert_eq!(
            fs::read(profile_root(&root, "first").join("active.json")).unwrap(),
            before
        );
        assert_eq!(fs::read(&first.ini_path).unwrap(), original_ini);
        assert_eq!(
            read_php_profile(&root, "first").unwrap(),
            Some(first.clone())
        );
        // Exercise replace-existing promotion, not only first-profile creation.
        let replacement = apply_php_profile(&root, "first", &executable, &[]).unwrap();
        assert_ne!(replacement.revision, first.revision);
        assert_eq!(
            read_php_profile(&root, "first").unwrap(),
            Some(replacement.clone())
        );
        assert_eq!(fs::read(&first.ini_path).unwrap(), original_ini);
        fs::write(
            replacement.scan_dir.join("injected.ini"),
            b"memory_limit=1M",
        )
        .unwrap();
        assert!(verify_php_profile(&executable, &replacement).is_err());
        fs::remove_file(replacement.scan_dir.join("injected.ini")).unwrap();
        fs::write(&replacement.ini_path, b"; externally changed").unwrap();
        assert!(verify_php_profile(&executable, &replacement).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn real_php_probe_deadline_and_output_are_bounded() {
        let Some(executable) = std::env::var_os("STACKYARD_TEST_PHP").map(PathBuf::from) else {
            return;
        };
        let mut command = isolated_php_command(&executable);
        command.args(["-n", "-r", "usleep(2000000);"]);
        let start = Instant::now();
        assert!(
            run_probe(&mut command, &executable, Duration::from_millis(100))
                .unwrap_err()
                .to_string()
                .contains("timed out")
        );
        assert!(start.elapsed() < Duration::from_secs(2));
        let mut command = isolated_php_command(&executable);
        command.args(["-n", "-r", "echo str_repeat('x', 300000);"]);
        assert!(run_probe(&mut command, &executable, PROBE_TIMEOUT)
            .unwrap_err()
            .to_string()
            .contains("output limit"));
        let mut command = isolated_php_command(&executable);
        command.args(["-n", "-r", "exit(3);"]);
        assert!(run_probe(&mut command, &executable, PROBE_TIMEOUT).is_err());
    }
}

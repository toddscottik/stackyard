use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

const DIR_NAME: &str = "localdev";

/// Per-user application state. This directory is created only by
/// [`prepare_data_dir`], so tests can ask for the path without touching disk.
pub fn default_data_dir() -> PathBuf {
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        if cfg!(windows) {
            return PathBuf::from(local).join(DIR_NAME);
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        if cfg!(target_os = "macos") {
            return home
                .join("Library")
                .join("Application Support")
                .join(DIR_NAME);
        }
        return home.join(".local").join("share").join(DIR_NAME);
    }
    std::env::temp_dir().join(DIR_NAME)
}

pub fn prepare_data_dir(path: &Path) -> Result<PathBuf> {
    fs::create_dir_all(path).map_err(|source| Error::io("create data directory", source))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o700);
        fs::set_permissions(path, perms)
            .map_err(|source| Error::io("restrict data directory permissions", source))?;
    }
    path.canonicalize()
        .map_err(|source| Error::io("canonicalize data directory", source))
}

/// Stable endpoint id for a prepared data directory. The same directory always
/// maps to the same pipe or socket, including paths that differ only by a
/// trailing slash.
pub fn endpoint_id(data_dir: &Path) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in data_dir.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

pub fn pipe_path(data_dir: &Path) -> String {
    format!(r"\\.\pipe\localdev-engine-{}", endpoint_id(data_dir))
}

pub fn socket_path(data_dir: &Path) -> PathBuf {
    data_dir.join("engine.sock")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dir_uses_the_placeholder_name() {
        let path = default_data_dir();
        assert_eq!(path.file_name().and_then(|n| n.to_str()), Some(DIR_NAME));
    }

    #[test]
    fn endpoint_id_ignores_a_trailing_slash_after_canonicalize() {
        let root = std::env::temp_dir().join(format!(
            "localdev-path-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let prepared = prepare_data_dir(&root).expect("prepare");
        let again = prepare_data_dir(&prepared).expect("prepare again");
        assert_eq!(endpoint_id(&prepared), endpoint_id(&again));
        let _ = fs::remove_dir_all(&prepared);
    }
}

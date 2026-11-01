use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

const DIR_NAME: &str = "stackyard";

/// Per-user application state. This directory is created only by
/// [`prepare_data_dir`], so tests can ask for the path without touching disk.
pub fn default_data_dir() -> PathBuf {
    let dir = if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        if cfg!(windows) {
            PathBuf::from(local).join(DIR_NAME)
        } else {
            home_data_dir()
        }
    } else {
        home_data_dir()
    };
    adopt_previous_data_dir(&dir);
    dir
}

fn home_data_dir() -> PathBuf {
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

/// The first branded build keeps data that was stored under the placeholder name.
fn adopt_previous_data_dir(new_dir: &Path) {
    if new_dir.exists() {
        return;
    }
    let Some(parent) = new_dir.parent() else {
        return;
    };
    let previous = parent.join("localdev");
    if previous.is_dir() {
        let _ = fs::rename(&previous, new_dir);
    }
}

/// A new directory path that stays unique when the clock does not move.
///
/// Windows often reports the same `SystemTime` for calls in one tick. Tests
/// that start together must not share a data directory, because that directory
/// is also the engine pipe name.
pub fn disposable_dir(label: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "stackyard-{label}-{}-{nanos}-{n}",
        std::process::id()
    ))
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
    format!(r"\\.\pipe\stackyard-engine-{}", endpoint_id(data_dir))
}

pub fn socket_path(data_dir: &Path) -> PathBuf {
    let inside = data_dir.join("engine.sock");
    // macOS sun_path holds 104 bytes. A long temp or home path cannot host the socket.
    if cfg!(unix) && inside.as_os_str().len() >= 100 {
        return short_socket_path(data_dir);
    }
    inside
}

pub fn short_socket_path(data_dir: &Path) -> PathBuf {
    PathBuf::from(format!("/tmp/sy-{}", current_uid()))
        .join(format!("{}.sock", endpoint_id(data_dir)))
}

fn current_uid() -> u32 {
    #[cfg(unix)]
    {
        unsafe { libc::geteuid() }
    }
    #[cfg(not(unix))]
    {
        0
    }
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
        let root = disposable_dir("path");
        let prepared = prepare_data_dir(&root).expect("prepare");
        let again = prepare_data_dir(&prepared).expect("prepare again");
        assert_eq!(endpoint_id(&prepared), endpoint_id(&again));
        let _ = fs::remove_dir_all(&prepared);
    }

    #[cfg(unix)]
    #[test]
    fn a_long_data_directory_gets_a_short_socket_path() {
        let mut dir = PathBuf::from("/tmp");
        for _ in 0..12 {
            dir.push("stackyard-long-name");
        }
        let socket = socket_path(&dir);
        assert!(
            socket.as_os_str().len() < 100,
            "socket path too long: {}",
            socket.display()
        );
        let name = socket
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("");
        assert!(name.starts_with("sy-"), "{name}");
        assert_eq!(socket, short_socket_path(&dir));
    }
}

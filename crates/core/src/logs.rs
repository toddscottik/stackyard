use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

const DEFAULT_LIMIT: u64 = 1_048_576;
const ROTATIONS: u8 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogChunk {
    pub offset: u64,
    pub next_offset: u64,
    pub text: String,
}

/// A bounded follow result. `generation` is opaque to callers and must be sent
/// back with the next request; `reset` means the previous cursor was invalidated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowLogChunk {
    pub offset: u64,
    pub next_offset: u64,
    pub text: String,
    pub generation: String,
    pub reset: bool,
}

pub fn append_log(path: &Path, bytes: &[u8]) -> Result<()> {
    append_log_limited(path, bytes, DEFAULT_LIMIT)
}

pub fn append_log_limited(path: &Path, bytes: &[u8], limit: u64) -> Result<()> {
    if bytes.is_empty() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::io("create log directory", source))?;
    }
    let len = fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    if len >= limit {
        rotate(path)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|source| Error::io("open service log", source))?;
    file.write_all(bytes)
        .map_err(|source| Error::io("write service log", source))?;
    Ok(())
}

pub fn read_log(path: &Path, offset: u64, limit: usize) -> Result<LogChunk> {
    let limit = limit.clamp(1, 24_000);
    let file = match File::open(path) {
        Ok(file) => file,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LogChunk {
                offset: 0,
                next_offset: 0,
                text: String::new(),
            });
        }
        Err(source) => return Err(Error::io("open service log", source)),
    };
    read_open(file, offset, limit)
}

/// Reads a bounded UTF-8-safe chunk and detects replacement, truncation, or a
/// rewrite around the caller's cursor. A generation token is deliberately
/// opaque; callers should persist it only for the lifetime of a log view.
pub fn read_log_follow(
    path: &Path,
    offset: u64,
    generation: Option<&str>,
    limit: usize,
) -> Result<FollowLogChunk> {
    let limit = limit.clamp(1, 24_000);
    let file = match File::open(path) {
        Ok(file) => file,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            let token = "missing".to_owned();
            return Ok(FollowLogChunk {
                offset: 0,
                next_offset: 0,
                text: String::new(),
                reset: generation.is_some_and(|previous| previous != token),
                generation: token,
            });
        }
        Err(source) => return Err(Error::io("open service log", source)),
    };
    let len = file
        .metadata()
        .map_err(|source| Error::io("read service log length", source))?
        .len();
    let cursor = offset.min(len);
    let mut reset = offset > len;
    if !reset {
        if let Some(previous) = generation {
            reset = generation_at(&file, path, cursor)? != previous;
        }
    }
    let start = if reset { 0 } else { cursor };
    let mut file = file;
    file.seek(SeekFrom::Start(start))
        .map_err(|source| Error::io("seek service log", source))?;
    let mut bytes = vec![0_u8; limit];
    let read = file
        .read(&mut bytes)
        .map_err(|source| Error::io("read service log", source))?;
    bytes.truncate(read);
    // Keep an incomplete trailing scalar for the next poll, so it is never
    // permanently replaced by U+FFFD when a process writes UTF-8 in pieces.
    let safe_len = match std::str::from_utf8(&bytes) {
        Ok(_) => bytes.len(),
        Err(error) if error.error_len().is_none() => error.valid_up_to(),
        Err(_) => bytes.len(),
    };
    bytes.truncate(safe_len);
    let next_offset = start + bytes.len() as u64;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let token = generation_at(&file, path, next_offset)?;
    Ok(FollowLogChunk {
        offset: start,
        next_offset,
        text,
        generation: token,
        reset,
    })
}

fn generation_at(file: &File, path: &Path, offset: u64) -> Result<String> {
    let metadata = file
        .metadata()
        .map_err(|source| Error::io("read service log identity", source))?;
    let mut identity = Vec::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        identity.extend_from_slice(&metadata.dev().to_le_bytes());
        identity.extend_from_slice(&metadata.ino().to_le_bytes());
    }
    #[cfg(windows)]
    {
        // Standard-library metadata has no file index on Windows. Created time
        // plus a cursor anchor below distinguishes replacement and regrowth.
        if let Ok(created) = metadata.created().and_then(|time| {
            time.duration_since(std::time::UNIX_EPOCH)
                .map_err(std::io::Error::other)
        }) {
            identity.extend_from_slice(&created.as_nanos().to_le_bytes());
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        if let Ok(created) = metadata.created() {
            if let Ok(created) = created.duration_since(std::time::UNIX_EPOCH) {
                identity.extend_from_slice(&created.as_nanos().to_le_bytes());
            }
        }
    }
    // Include bytes immediately before the cursor. This catches same-file
    // truncate-and-regrow even if it exceeds the old cursor before the next
    // poll, while remaining stable when bytes are appended after the cursor.
    let mut anchor = Vec::new();
    let mut reader = file
        .try_clone()
        .map_err(|source| Error::io("clone service log handle", source))?;
    let anchor_start = offset.saturating_sub(64);
    reader
        .seek(SeekFrom::Start(anchor_start))
        .map_err(|source| Error::io("seek service log cursor anchor", source))?;
    let mut near_cursor = [0_u8; 64];
    let count = reader
        .take(offset.saturating_sub(anchor_start))
        .read(&mut near_cursor)
        .map_err(|source| Error::io("read service log cursor anchor", source))?;
    anchor.extend_from_slice(&near_cursor[..count]);
    identity.extend_from_slice(
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .as_bytes(),
    );
    identity.extend_from_slice(&anchor);
    let hash = identity.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    Ok(format!("{hash:016x}"))
}

pub fn log_tail(path: &Path, limit: usize) -> String {
    let Ok(file) = File::open(path) else {
        return String::new();
    };
    let Ok(meta) = file.metadata() else {
        return String::new();
    };
    let len = meta.len();
    let start = len.saturating_sub(limit as u64);
    read_open(file, start, limit)
        .map(|chunk| chunk.text)
        .unwrap_or_default()
}

pub fn service_log_path(data_dir: &Path, project_id: &str, service_id: &str) -> PathBuf {
    data_dir
        .join("logs")
        .join(project_id)
        .join(format!("{service_id}.log"))
}

fn read_open(mut file: File, offset: u64, limit: usize) -> Result<LogChunk> {
    let len = file
        .metadata()
        .map_err(|source| Error::io("read service log length", source))?
        .len();
    let start = if offset > len { 0 } else { offset };
    file.seek(SeekFrom::Start(start))
        .map_err(|source| Error::io("seek service log", source))?;
    let mut buf = vec![0_u8; limit];
    let read = file
        .read(&mut buf)
        .map_err(|source| Error::io("read service log", source))?;
    buf.truncate(read);
    let text = String::from_utf8_lossy(&buf).into_owned();
    Ok(LogChunk {
        offset: start,
        next_offset: start + read as u64,
        text,
    })
}

fn rotate(path: &Path) -> Result<()> {
    let oldest = rotated(path, ROTATIONS);
    if oldest.exists() {
        fs::remove_file(&oldest).map_err(|source| Error::io("remove rotated log", source))?;
    }
    for index in (1..ROTATIONS).rev() {
        let from = rotated(path, index);
        if from.exists() {
            let to = rotated(path, index + 1);
            fs::rename(&from, &to).map_err(|source| Error::io("rotate service log", source))?;
        }
    }
    if path.exists() {
        fs::rename(path, rotated(path, 1))
            .map_err(|source| Error::io("rotate service log", source))?;
    }
    Ok(())
}

fn rotated(path: &Path, index: u8) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    name.push(format!(".{index}"));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_rotates_and_reads_from_an_offset() {
        let dir = crate::paths::disposable_dir("log");
        fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("web.log");
        append_log_limited(&path, b"aaaa", 8).expect("first");
        append_log_limited(&path, b"bbbb", 8).expect("second");
        append_log_limited(&path, b"cccc", 8).expect("rotate");
        let chunk = read_log(&path, 0, 20).expect("read");
        assert_eq!(chunk.text, "cccc");
        assert!(rotated(&path, 1).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn follow_resets_after_rotation_and_reuses_offsets_for_utf8() {
        let dir = crate::paths::disposable_dir("log-follow");
        fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("web.log");
        fs::write(&path, "hé".as_bytes()).expect("write log");
        let first = read_log_follow(&path, 0, None, 2).expect("first chunk");
        assert_eq!(first.text, "h");
        assert_eq!(first.next_offset, 1);
        let second = read_log_follow(&path, first.next_offset, Some(&first.generation), 8)
            .expect("finish scalar");
        assert_eq!(second.text, "é");
        assert_eq!(second.next_offset, 3);

        fs::rename(&path, rotated(&path, 1)).expect("rotate old log");
        fs::write(&path, "new output").expect("new log");
        let after_rotation =
            read_log_follow(&path, second.next_offset, Some(&second.generation), 32)
                .expect("read replacement");
        assert!(after_rotation.reset);
        assert_eq!(after_rotation.offset, 0);
        assert_eq!(after_rotation.text, "new output");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn follow_resets_when_a_log_is_truncated_and_regrown_past_cursor() {
        let dir = crate::paths::disposable_dir("log-regrow");
        fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("web.log");
        fs::write(&path, b"original payload").expect("write log");
        let initial = read_log_follow(&path, 0, None, 8).expect("initial chunk");
        fs::write(&path, b"replacement bytes").expect("truncate and regrow");
        let replacement =
            read_log_follow(&path, initial.next_offset, Some(&initial.generation), 32)
                .expect("read regrown file");
        assert!(replacement.reset);
        assert_eq!(replacement.offset, 0);
        assert_eq!(replacement.text, "replacement bytes");
        let _ = fs::remove_dir_all(&dir);
    }
}

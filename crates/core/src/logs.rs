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
        let dir = std::env::temp_dir().join(format!(
            "stackyard-log-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
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
}

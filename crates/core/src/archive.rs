use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use crate::error::{Error, Result};

pub fn extract(format: &str, archive: &Path, destination: &Path) -> Result<()> {
    match format {
        "zip" => extract_zip(archive, destination),
        "tar.gz" => extract_tar_gz(archive, destination),
        other => Err(Error::InvalidMessage(format!(
            "archive format {other:?} is not supported"
        ))),
    }
}

fn safe_relative(entry: &str) -> Result<PathBuf> {
    let path = Path::new(entry);
    if entry.is_empty() || path.is_absolute() || entry.contains(':') {
        return Err(Error::UnsafeArchivePath {
            entry: entry.to_string(),
        });
    }
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(Error::UnsafeArchivePath {
                    entry: entry.to_string(),
                });
            }
        }
    }
    if relative.as_os_str().is_empty() {
        return Err(Error::UnsafeArchivePath {
            entry: entry.to_string(),
        });
    }
    Ok(relative)
}

fn extract_zip(archive: &Path, destination: &Path) -> Result<()> {
    let file = File::open(archive).map_err(|source| Error::io("open zip", source))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|err| Error::InvalidMessage(format!("open zip {}: {err}", archive.display())))?;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|err| Error::InvalidMessage(format!("read zip entry {index}: {err}")))?;
        let name = entry.name().to_string();
        if name.ends_with('/') || name.ends_with('\\') {
            let relative = safe_relative(name.trim_end_matches(['/', '\\']))?;
            fs::create_dir_all(destination.join(relative))
                .map_err(|source| Error::io("create archive directory", source))?;
            continue;
        }
        let relative = safe_relative(&name)?;
        if entry.enclosed_name().is_none() {
            return Err(Error::UnsafeArchivePath { entry: name });
        }
        let target = destination.join(&relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|source| Error::io("create archive parent", source))?;
        }
        let mut output =
            File::create(&target).map_err(|source| Error::io("create extracted file", source))?;
        io::copy(&mut entry, &mut output)
            .map_err(|source| Error::io("extract zip file", source))?;
    }
    Ok(())
}

fn extract_tar_gz(archive: &Path, destination: &Path) -> Result<()> {
    let file = File::open(archive).map_err(|source| Error::io("open tar.gz", source))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    let entries = tar
        .entries()
        .map_err(|source| Error::io("read tar entries", source))?;
    for entry in entries {
        let mut entry = entry.map_err(|source| Error::io("read tar entry", source))?;
        let path = entry
            .path()
            .map_err(|source| Error::io("read tar path", source))?;
        let name = path.to_string_lossy().to_string();
        if entry.header().entry_type().is_dir() {
            let relative = safe_relative(&name)?;
            fs::create_dir_all(destination.join(relative))
                .map_err(|source| Error::io("create tar directory", source))?;
            continue;
        }
        if !entry.header().entry_type().is_file() {
            return Err(Error::UnsafeArchivePath { entry: name });
        }
        let relative = safe_relative(&name)?;
        let target = destination.join(&relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::io("create tar parent", source))?;
        }
        let mut output =
            File::create(&target).map_err(|source| Error::io("create tar file", source))?;
        io::copy(&mut entry, &mut output)
            .map_err(|source| Error::io("extract tar file", source))?;
    }
    Ok(())
}

pub fn read_to_digest<R: Read>(
    mut reader: R,
    output: &Path,
    cancel: &dyn Fn() -> bool,
) -> Result<(u64, String, String)> {
    use sha2::Digest;
    let mut file = File::create(output).map_err(|source| Error::io("create download", source))?;
    let mut sha = sha2::Sha256::new();
    let mut md5_hash = md5::Md5::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        if cancel() {
            return Err(Error::Cancelled {
                id: output.display().to_string(),
            });
        }
        let read = reader
            .read(&mut buffer)
            .map_err(|source| Error::io("read download", source))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|source| Error::io("write download", source))?;
        sha.update(&buffer[..read]);
        md5_hash.update(&buffer[..read]);
        total += read as u64;
    }
    file.sync_all()
        .map_err(|source| Error::io("sync download", source))?;
    Ok((
        total,
        hex::encode(sha.finalize()),
        hex::encode(md5_hash.finalize()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    #[test]
    fn rejects_parent_and_absolute_entries() {
        assert!(safe_relative("../escape.txt").is_err());
        assert!(safe_relative("/tmp/escape.txt").is_err());
        assert!(safe_relative("C:/escape.txt").is_err());
        assert_eq!(
            safe_relative("nested/hello.txt").expect("safe"),
            PathBuf::from("nested").join("hello.txt")
        );
    }

    #[test]
    fn unsafe_zip_does_not_write_outside_destination() {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("../escape.txt", options).expect("start");
            writer.write_all(b"nope").expect("write");
            writer.finish().expect("finish");
        }
        let root = crate::paths::disposable_dir("zip");
        fs::create_dir_all(&root).expect("root");
        let archive = root.join("bad.zip");
        fs::write(&archive, cursor.into_inner()).expect("archive");
        let destination = root.join("out");
        fs::create_dir_all(&destination).expect("dest");
        let error = extract("zip", &archive, &destination).expect_err("unsafe");
        assert!(matches!(error, Error::UnsafeArchivePath { .. }));
        assert!(!root.join("escape.txt").exists());
        assert!(!destination.join("escape.txt").exists());
        let _ = fs::remove_dir_all(&root);
    }
}

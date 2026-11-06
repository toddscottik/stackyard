use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use crate::error::{Error, Result};

pub fn extract(format: &str, archive: &Path, destination: &Path) -> Result<()> {
    match format {
        "zip" => extract_zip(archive, destination),
        "tar.gz" => extract_tar_gz(archive, destination),
        "phar" => {
            fs::copy(archive, destination.join("composer.phar"))
                .map_err(|source| Error::io("store verified Composer phar", source))?;
            Ok(())
        }
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
        if entry.header().entry_type().is_symlink() {
            extract_symlink(&entry, &name, destination)?;
            continue;
        }
        if entry.header().entry_type().is_hard_link() {
            extract_hard_link(&entry, &name, destination)?;
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
        set_extracted_mode(&entry, &target)?;
    }
    Ok(())
}

fn link_stays_inside(entry: &Path, link: &str) -> bool {
    let link_path = Path::new(link);
    if link.is_empty() || link_path.is_absolute() || link.contains(':') {
        return false;
    }
    let mut parts = Vec::new();
    if let Some(parent) = entry.parent() {
        for component in parent.components() {
            match component {
                Component::Normal(part) => parts.push(part.to_owned()),
                Component::CurDir => {}
                Component::ParentDir => {
                    if parts.pop().is_none() {
                        return false;
                    }
                }
                Component::RootDir | Component::Prefix(_) => return false,
            }
        }
    }
    for component in link_path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.pop().is_none() {
                    return false;
                }
            }
            Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    true
}

fn extract_symlink<R: Read>(
    entry: &tar::Entry<'_, R>,
    name: &str,
    destination: &Path,
) -> Result<()> {
    let relative = safe_relative(name)?;
    let link = entry
        .link_name()
        .map_err(|source| Error::io("read tar symlink", source))?
        .ok_or_else(|| Error::UnsafeArchivePath {
            entry: name.to_string(),
        })?;
    let link = link.to_string_lossy();
    if !link_stays_inside(&relative, &link) {
        return Err(Error::UnsafeArchivePath {
            entry: name.to_string(),
        });
    }
    let target = destination.join(&relative);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::io("create tar parent", source))?;
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(link.as_ref(), &target)
            .map_err(|source| Error::io("create tar symlink", source))?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = target;
        Err(Error::UnsafeArchivePath {
            entry: format!("{name} symlink is not extracted on this operating system"),
        })
    }
}

fn extract_hard_link<R: Read>(
    entry: &tar::Entry<'_, R>,
    name: &str,
    destination: &Path,
) -> Result<()> {
    let relative = safe_relative(name)?;
    let link = entry
        .link_name()
        .map_err(|source| Error::io("read tar hard link", source))?
        .ok_or_else(|| Error::UnsafeArchivePath {
            entry: name.to_string(),
        })?;
    let source_rel =
        safe_relative(&link.to_string_lossy()).map_err(|_| Error::UnsafeArchivePath {
            entry: name.to_string(),
        })?;
    let source = destination.join(&source_rel);
    if !source.is_file() {
        return Err(Error::InvalidMessage(format!(
            "hard link {name} points at {}, which is not in the archive yet",
            source_rel.display()
        )));
    }
    let target = destination.join(&relative);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::io("create tar parent", source))?;
    }
    fs::hard_link(&source, &target).map_err(|source| Error::io("create tar hard link", source))?;
    Ok(())
}

fn set_extracted_mode<R: Read>(entry: &tar::Entry<'_, R>, target: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(mode) = entry.header().mode() {
            let perm = mode & 0o777;
            if perm != 0 {
                fs::set_permissions(target, fs::Permissions::from_mode(perm))
                    .map_err(|source| Error::io("set extracted mode", source))?;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (entry, target);
    }
    Ok(())
}

pub(crate) const READ_DOWNLOAD: &str = "read download";

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
            .map_err(|source| Error::io(READ_DOWNLOAD, source))?;
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

    fn gzip_tar(
        write_members: impl FnOnce(&mut tar::Builder<flate2::write::GzEncoder<Vec<u8>>>),
    ) -> Vec<u8> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        write_members(&mut builder);
        let encoder = builder.into_inner().expect("tar");
        encoder.finish().expect("gzip")
    }

    fn append_file(
        builder: &mut tar::Builder<flate2::write::GzEncoder<Vec<u8>>>,
        path: &str,
        mode: u32,
        bytes: &[u8],
    ) {
        let mut header = tar::Header::new_gnu();
        header.set_path(path).expect("path");
        header.set_size(bytes.len() as u64);
        header.set_mode(mode);
        header.set_cksum();
        builder.append(&header, bytes).expect("file");
    }

    fn append_hard_link(
        builder: &mut tar::Builder<flate2::write::GzEncoder<Vec<u8>>>,
        path: &str,
        target: &str,
    ) {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Link);
        header.set_path(path).expect("path");
        header.set_link_name(target).expect("link");
        header.set_size(0);
        header.set_cksum();
        builder
            .append(&header, std::io::empty())
            .expect("hard link");
    }

    fn append_symlink(
        builder: &mut tar::Builder<flate2::write::GzEncoder<Vec<u8>>>,
        path: &str,
        target: &str,
    ) {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_path(path).expect("path");
        header.set_link_name(target).expect("link");
        header.set_size(0);
        header.set_mode(0o777);
        header.set_cksum();
        builder.append(&header, std::io::empty()).expect("symlink");
    }

    #[test]
    #[cfg(unix)]
    fn tar_gz_keeps_relative_symlinks_and_executable_mode() {
        let bytes = gzip_tar(|builder| {
            append_file(builder, "python/bin/python3.12", 0o755, b"python-bytes\n");
            append_symlink(builder, "python/bin/python3", "python3.12");
            append_file(
                builder,
                "node/lib/node_modules/npm/bin/npm-cli.js",
                0o644,
                b"npm-cli\n",
            );
            append_symlink(
                builder,
                "node/bin/npm",
                "../lib/node_modules/npm/bin/npm-cli.js",
            );
        });
        let root = crate::paths::disposable_dir("tar-links");
        fs::create_dir_all(&root).expect("root");
        let archive = root.join("runtime.tar.gz");
        fs::write(&archive, bytes).expect("archive");
        let destination = root.join("out");
        fs::create_dir_all(&destination).expect("dest");
        extract("tar.gz", &archive, &destination).expect("extract");
        assert_eq!(
            fs::read(destination.join("python/bin/python3")).expect("python"),
            b"python-bytes\n"
        );
        assert_eq!(
            fs::read(destination.join("node/bin/npm")).expect("npm"),
            b"npm-cli\n"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let mode = fs::metadata(destination.join("python/bin/python3.12"))
                .expect("mode")
                .mode();
            assert_eq!(mode & 0o111, 0o111, "python3.12 mode {mode:o}");
            let link = fs::symlink_metadata(destination.join("python/bin/python3")).expect("link");
            assert!(link.file_type().is_symlink());
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn tar_gz_keeps_hard_links_inside_the_archive() {
        let bytes = gzip_tar(|builder| {
            append_file(builder, "dist/original.txt", 0o644, b"shared\n");
            append_hard_link(builder, "dist/copy.txt", "dist/original.txt");
        });
        let root = crate::paths::disposable_dir("tar-hard");
        fs::create_dir_all(&root).expect("root");
        let archive = root.join("links.tar.gz");
        fs::write(&archive, bytes).expect("archive");
        let destination = root.join("out");
        fs::create_dir_all(&destination).expect("dest");
        extract("tar.gz", &archive, &destination).expect("extract");
        assert_eq!(
            fs::read(destination.join("dist/copy.txt")).expect("copy"),
            b"shared\n"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn tar_gz_rejects_symlink_that_escapes_destination() {
        let bytes = gzip_tar(|builder| {
            append_symlink(builder, "bin/escape", "../../outside");
        });
        let root = crate::paths::disposable_dir("tar-escape");
        fs::create_dir_all(&root).expect("root");
        let archive = root.join("bad.tar.gz");
        fs::write(&archive, bytes).expect("archive");
        let destination = root.join("out");
        fs::create_dir_all(&destination).expect("dest");
        let error = extract("tar.gz", &archive, &destination).expect_err("unsafe");
        assert!(matches!(error, Error::UnsafeArchivePath { .. }));
        assert!(!root.join("outside").exists());
        assert!(!destination.join("outside").exists());
        let _ = fs::remove_dir_all(&root);
    }
}

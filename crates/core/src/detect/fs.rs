use std::ffi::OsStr;
use std::fs::{self, File, Metadata};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use crate::{Error, Result};

const MAX_PATH_COMPONENTS: usize = 128;
pub(super) const MAX_TEXT_BYTES: usize = 64 * 1024;

pub(super) struct Directory {
    path: PathBuf,
    file: File,
    _ancestors: Vec<File>,
}

impl Directory {
    pub(super) fn root(root: &Path) -> Result<(Self, PathBuf)> {
        for part in root.components() {
            if matches!(part, Component::ParentDir) {
                return Err(Error::OperationFailed(
                    "Choose the import directory without parent-directory components.".into(),
                ));
            }
            #[cfg(windows)]
            match part {
                Component::Prefix(prefix)
                    if !matches!(
                        prefix.kind(),
                        std::path::Prefix::Disk(_)
                            | std::path::Prefix::VerbatimDisk(_)
                            | std::path::Prefix::UNC(_, _)
                            | std::path::Prefix::VerbatimUNC(_, _)
                    ) =>
                {
                    return Err(Error::OperationFailed(
                        "Choose an ordinary filesystem directory.".into(),
                    ));
                }
                Component::Normal(name) => validate_windows_name(name)
                    .map_err(|source| Error::io("validate import directory", source))?,
                _ => {}
            }
        }
        let absolute = std::path::absolute(root)
            .map_err(|source| Error::io("resolve import directory", source))?;
        let mut path = PathBuf::new();
        let mut opened = Vec::new();
        for (index, part) in absolute.components().enumerate() {
            if index >= MAX_PATH_COMPONENTS {
                return Err(Error::OperationFailed(
                    "Import directory is too deeply nested.".into(),
                ));
            }
            match part {
                Component::Prefix(_) => path.push(part),
                Component::RootDir => {
                    path.push(part);
                    let file = open_path(&path, true)
                        .map_err(|source| Error::io("open import volume", source))?;
                    opened.push(Self {
                        path: path.clone(),
                        file,
                        _ancestors: Vec::new(),
                    });
                }
                Component::Normal(name) => {
                    let parent = opened
                        .last()
                        .expect("absolute directory begins at a volume");
                    let child = parent
                        .directory(name)
                        .map_err(|source| Error::io("open import directory", source))?
                        .ok_or_else(|| {
                            Error::OperationFailed(format!(
                                "Import directory {} does not exist.",
                                parent.path.join(name).display()
                            ))
                        })?;
                    opened.push(child);
                }
                _ => {
                    return Err(Error::OperationFailed(
                        "Choose the import directory without parent-directory components.".into(),
                    ));
                }
            }
        }
        let mut dir = opened.pop().expect("absolute directory has a volume");
        #[cfg(windows)]
        let canonical = crate::plain_path(
            dir.path
                .canonicalize()
                .map_err(|source| Error::io("canonicalize import directory", source))?,
        );
        // Each Unix component was opened relative to the previous directory
        // descriptor without following links. The assembled path is absolute.
        #[cfg(unix)]
        let canonical = dir.path.clone();
        dir._ancestors = opened.into_iter().map(|directory| directory.file).collect();
        Ok((dir, canonical))
    }

    pub(super) fn directory(&self, name: &OsStr) -> io::Result<Option<Self>> {
        let path = self.path.join(name);
        self.open(name, true).map(|file| {
            file.map(|file| Self {
                path,
                file,
                _ancestors: Vec::new(),
            })
        })
    }

    pub(super) fn exists(&self, name: &str) -> io::Result<bool> {
        self.open(name.as_ref(), false).map(|file| file.is_some())
    }

    pub(super) fn text(&self, name: &str) -> io::Result<Option<String>> {
        let Some(file) = self.open(name.as_ref(), false)? else {
            return Ok(None);
        };
        if file.metadata()?.len() > MAX_TEXT_BYTES as u64 {
            return Err(oversized());
        }
        let mut bytes = Vec::new();
        file.take(MAX_TEXT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_TEXT_BYTES {
            return Err(oversized());
        }
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "file is not UTF-8"))
    }

    fn open(&self, name: &OsStr, directory: bool) -> io::Result<Option<File>> {
        #[cfg(unix)]
        let result = open_at(&self.file, name, directory);
        #[cfg(windows)]
        let result = {
            // Hold this ordinary parent open without share-delete while opening
            // its immediate child. The caller retains ancestors during descent.
            let _parent = &self.file;
            open_path(&self.path.join(name), directory)
        };
        match result {
            Ok(file) => Ok(Some(file)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }
}

fn oversized() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "file exceeds the 64 KiB inspection limit",
    )
}

fn check_kind(metadata: &Metadata, directory: bool) -> io::Result<()> {
    #[cfg(windows)]
    let redirected = {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    };
    #[cfg(unix)]
    let redirected = metadata.file_type().is_symlink();

    let expected_kind = if directory {
        metadata.is_dir()
    } else {
        metadata.is_file()
    };
    if redirected || !expected_kind {
        let kind = if directory { "directory" } else { "file" };
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected an ordinary {kind} without redirects"),
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn validate_windows_name(name: &OsStr) -> io::Result<()> {
    // Win32 normalizes trailing dots and spaces in ordinary paths. Reject
    // aliases so the components checked below are the ones actually opened.
    let text = name.to_string_lossy();
    if text.ends_with(['.', ' ']) || text.contains(':') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ambiguous Windows path",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn open_path(path: &Path, directory: bool) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ_WRITE: u32 = 0x3;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x02000000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x00200000;

    if let Some(name) = path.file_name() {
        validate_windows_name(name)?;
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    check_kind(&file.metadata()?, directory)?;
    Ok(file)
}

#[cfg(unix)]
fn open_path(path: &Path, directory: bool) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(open_flags(directory))
        .open(path)?;
    check_kind(&file.metadata()?, directory)?;
    Ok(file)
}

#[cfg(unix)]
fn open_at(parent: &File, name: &OsStr, directory: bool) -> io::Result<File> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;

    let name = std::ffi::CString::new(name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid path component"))?;
    // SAFETY: name is a NUL-terminated component and the directory handle is live.
    let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), open_flags(directory)) };
    if fd == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful openat returned a new descriptor owned by this function.
    let file = unsafe { File::from_raw_fd(fd) };
    check_kind(&file.metadata()?, directory)?;
    Ok(file)
}

#[cfg(unix)]
fn open_flags(directory: bool) -> libc::c_int {
    libc::O_NOFOLLOW
        | libc::O_CLOEXEC
        | libc::O_NONBLOCK
        | if directory { libc::O_DIRECTORY } else { 0 }
}

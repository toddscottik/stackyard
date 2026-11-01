//! Database names, secret files, and SQLite backups.
//!
//! MySQL and PostgreSQL processes live in the engine. This module keeps the
//! checks that must not depend on a running server: names, nonempty data
//! directories, and the fact that a password never belongs in a status string.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::{backup::Backup, Connection};

use crate::error::{Error, Result};

pub const DEFAULT_MYSQL_PORT: u16 = 13306;
pub const DEFAULT_POSTGRES_PORT: u16 = 15432;

pub fn require_ident(value: &str, what: &str) -> Result<()> {
    let ok = !value.is_empty()
        && value.len() <= 63
        && value.starts_with(|c: char| c.is_ascii_lowercase())
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if ok {
        Ok(())
    } else {
        Err(Error::InvalidMessage(format!(
            "{what} {value:?} must be a lowercase identifier"
        )))
    }
}

pub fn require_empty_data_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let mut entries =
        fs::read_dir(path).map_err(|source| Error::io("read data directory", source))?;
    if entries.next().is_some() {
        return Err(Error::DataDirNotEmpty {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

pub fn random_secret() -> Result<String> {
    let mut bytes = [0_u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|err| {
        Error::InvalidMessage(format!("could not generate a database password: {err}"))
    })?;
    Ok(hex::encode(bytes))
}

pub fn write_secret(dir: &Path, id: &str, secret: &str) -> Result<PathBuf> {
    fs::create_dir_all(dir).map_err(|source| Error::io("create secrets directory", source))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    }
    let path = dir.join(id);
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
        .map_err(|source| Error::io("create secret file", source))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
    }
    file.write_all(secret.as_bytes())
        .map_err(|source| Error::io("write secret file", source))?;
    Ok(path)
}

pub fn read_secret(path: &Path) -> Result<String> {
    let text = fs::read_to_string(path).map_err(|source| Error::io("read secret file", source))?;
    Ok(text.trim().to_string())
}

pub fn redact(text: &str, secret: &str) -> String {
    if secret.is_empty() {
        return text.to_string();
    }
    text.replace(secret, "[redacted]")
}

pub fn sqlite_init(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|source| Error::io("create sqlite directory", source))?;
    }
    let conn = Connection::open(path)
        .map_err(|err| Error::InvalidMessage(format!("open sqlite: {err}")))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS stackyard_probe (id INTEGER PRIMARY KEY, note TEXT NOT NULL);",
    )
    .map_err(|err| Error::InvalidMessage(format!("initialize sqlite: {err}")))?;
    Ok(())
}

pub fn sqlite_write(path: &Path, id: i64, note: &str) -> Result<()> {
    let conn = Connection::open(path)
        .map_err(|err| Error::InvalidMessage(format!("open sqlite: {err}")))?;
    conn.execute(
        "INSERT INTO stackyard_probe (id, note) VALUES (?1, ?2)",
        rusqlite::params![id, note],
    )
    .map_err(|err| Error::InvalidMessage(format!("write sqlite: {err}")))?;
    Ok(())
}

pub fn sqlite_read(path: &Path, id: i64) -> Result<Option<String>> {
    let conn = Connection::open(path)
        .map_err(|err| Error::InvalidMessage(format!("open sqlite: {err}")))?;
    let mut stmt = conn
        .prepare("SELECT note FROM stackyard_probe WHERE id = ?1")
        .map_err(|err| Error::InvalidMessage(format!("read sqlite: {err}")))?;
    let mut rows = stmt
        .query(rusqlite::params![id])
        .map_err(|err| Error::InvalidMessage(format!("read sqlite: {err}")))?;
    match rows
        .next()
        .map_err(|err| Error::InvalidMessage(format!("read sqlite: {err}")))?
    {
        Some(row) => {
            Ok(Some(row.get(0).map_err(|err| {
                Error::InvalidMessage(format!("read sqlite: {err}"))
            })?))
        }
        None => Ok(None),
    }
}

pub fn sqlite_backup(source: &Path, destination: &Path) -> Result<()> {
    if source == destination {
        return Err(Error::InvalidMessage(
            "a sqlite backup must not overwrite the database it copies".to_string(),
        ));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|source| Error::io("create backup directory", source))?;
    }
    let src = Connection::open(source)
        .map_err(|err| Error::InvalidMessage(format!("open sqlite: {err}")))?;
    let mut dst = Connection::open(destination)
        .map_err(|err| Error::InvalidMessage(format!("open sqlite backup: {err}")))?;
    let backup = Backup::new(&src, &mut dst)
        .map_err(|err| Error::InvalidMessage(format!("sqlite backup: {err}")))?;
    backup
        .step(-1)
        .map_err(|err| Error::InvalidMessage(format!("sqlite backup: {err}")))?;
    Ok(())
}

pub fn contained_sqlite(project_root: &Path, requested: &Path) -> Result<PathBuf> {
    if requested
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(Error::InvalidMessage(
            "sqlite path must stay inside the project".to_string(),
        ));
    }
    let joined = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        project_root.join(requested)
    };
    let root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    if let Ok(canonical) = joined.canonicalize() {
        if !canonical.starts_with(&root) {
            return Err(Error::InvalidMessage(
                "sqlite path must stay inside the project".to_string(),
            ));
        }
    }
    Ok(joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonempty_directory_is_refused_and_kept() {
        let dir = crate::paths::disposable_dir("db-empty");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("marker.txt"), "keep").unwrap();
        let err = require_empty_data_dir(&dir).unwrap_err();
        assert!(matches!(err, Error::DataDirNotEmpty { .. }));
        assert_eq!(fs::read_to_string(dir.join("marker.txt")).unwrap(), "keep");
    }

    #[test]
    fn sqlite_round_trip_survives_a_backup() {
        let dir = crate::paths::disposable_dir("sqlite");
        fs::create_dir_all(&dir).unwrap();
        let db = dir.join("app.sqlite");
        let copy = dir.join("app-copy.sqlite");
        sqlite_init(&db).unwrap();
        sqlite_write(&db, 1, "kept").unwrap();
        sqlite_backup(&db, &copy).unwrap();
        assert_eq!(sqlite_read(&copy, 1).unwrap().as_deref(), Some("kept"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn status_text_does_not_keep_a_password() {
        let secret = "abc123secret";
        let shown = redact(&format!("failed for password {secret}"), secret);
        assert!(!shown.contains(secret));
        assert!(shown.contains("[redacted]"));
    }

    #[test]
    fn names_reject_sql_punctuation() {
        assert!(require_ident("yard", "database").is_ok());
        assert!(require_ident("yard-1", "database").is_err());
        assert!(require_ident("Yard", "database").is_err());
        assert!(require_ident("yard;drop", "database").is_err());
    }
}

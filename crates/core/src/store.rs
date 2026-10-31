use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use crate::error::{Error, Result};
use crate::protocol::{validate_setting_key, validate_setting_value};

const SCHEMA_VERSION: u32 = 1;

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(data_dir: &Path) -> Result<Self> {
        let path = data_dir.join("state.sqlite3");
        let conn = Connection::open(&path)
            .map_err(|source| Error::database("open state database", source))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|source| Error::database("set database busy timeout", source))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|source| Error::database("enable foreign keys", source))?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn schema_version(&self) -> Result<u32> {
        let version: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .map_err(|source| Error::database("read schema version", source))?;
        u32::try_from(version).map_err(|_| Error::SchemaTooNew {
            found: u32::MAX,
            supported: SCHEMA_VERSION,
        })
    }

    pub fn get(&self, key: &str) -> Result<Option<String>> {
        validate_setting_key(key)?;
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM settings WHERE key = ?1")
            .map_err(|source| Error::database("prepare setting read", source))?;
        let mut rows = stmt
            .query(params![key])
            .map_err(|source| Error::database("query setting", source))?;
        match rows
            .next()
            .map_err(|source| Error::database("read setting row", source))?
        {
            Some(row) => {
                Ok(Some(row.get(0).map_err(|source| {
                    Error::database("read setting value", source)
                })?))
            }
            None => Ok(None),
        }
    }

    pub fn put(&self, key: &str, value: &str) -> Result<()> {
        validate_setting_key(key)?;
        validate_setting_value(value)?;
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|source| Error::database("begin setting write", source))?;
        tx.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(|source| Error::database("write setting", source))?;
        tx.execute(
            "INSERT INTO events (recorded_at, kind, detail) VALUES (?1, 'setting_put', ?2)",
            params![now_unix(), key],
        )
        .map_err(|source| Error::database("record setting event", source))?;
        tx.commit()
            .map_err(|source| Error::database("commit setting write", source))?;
        Ok(())
    }

    pub fn last_event(&self) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT kind, detail FROM events ORDER BY id DESC LIMIT 1")
            .map_err(|source| Error::database("prepare event read", source))?;
        let mut rows = stmt
            .query([])
            .map_err(|source| Error::database("query event", source))?;
        match rows
            .next()
            .map_err(|source| Error::database("read event row", source))?
        {
            Some(row) => {
                let kind: String = row
                    .get(0)
                    .map_err(|source| Error::database("read event kind", source))?;
                let detail: String = row
                    .get(1)
                    .map_err(|source| Error::database("read event detail", source))?;
                Ok(Some(format!("{kind} {detail}")))
            }
            None => Ok(None),
        }
    }

    fn migrate(&self) -> Result<()> {
        self.conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at INTEGER NOT NULL
                );",
            )
            .map_err(|source| Error::database("create schema_migrations", source))?;
        let current = self.schema_version()?;
        if current > SCHEMA_VERSION {
            return Err(Error::SchemaTooNew {
                found: current,
                supported: SCHEMA_VERSION,
            });
        }
        if current < 1 {
            let tx = self
                .conn
                .unchecked_transaction()
                .map_err(|source| Error::database("begin migration 1", source))?;
            tx.execute_batch(
                "CREATE TABLE settings (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );
                CREATE TABLE events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    recorded_at INTEGER NOT NULL,
                    kind TEXT NOT NULL,
                    detail TEXT NOT NULL
                );",
            )
            .map_err(|source| Error::database("apply migration 1", source))?;
            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (1, ?1)",
                params![now_unix()],
            )
            .map_err(|source| Error::database("record migration 1", source))?;
            tx.commit()
                .map_err(|source| Error::database("commit migration 1", source))?;
        }
        Ok(())
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(0))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::prepare_data_dir;

    fn temp_dir() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "localdev-store-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        prepare_data_dir(&path).expect("temp dir")
    }

    #[test]
    fn settings_and_schema_survive_reopen() {
        let dir = temp_dir();
        {
            let store = Store::open(&dir).expect("open");
            assert_eq!(store.schema_version().expect("version"), 1);
            store.put("display_name", "Ada").expect("put");
            assert_eq!(
                store.last_event().expect("event").as_deref(),
                Some("setting_put display_name")
            );
        }
        let store = Store::open(&dir).expect("reopen");
        assert_eq!(store.schema_version().expect("version"), 1);
        assert_eq!(
            store.get("display_name").expect("get").as_deref(),
            Some("Ada")
        );
        assert!(store.get("missing").expect("missing").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_key_does_not_create_a_row() {
        let dir = temp_dir();
        let store = Store::open(&dir).expect("open");
        assert!(store.put("../x", "no").is_err());
        assert!(store.last_event().expect("event").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

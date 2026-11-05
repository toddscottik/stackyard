use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use crate::error::{Error, Result};
use crate::protocol::{validate_setting_key, validate_setting_value};

const SCHEMA_VERSION: u32 = 8;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScopeSelectionRecord {
    pub project_id: String,
    pub scope_id: String,
    pub role: String,
    pub package_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServiceObservationRecord {
    pub project_id: String,
    pub service_id: String,
    pub readiness_state: String,
    pub http_status: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CacheInstanceRecord {
    pub id: String,
    pub package_id: String,
    pub port: u16,
    pub state: String,
    pub pid: Option<u32>,
    pub identity: Option<String>,
    pub socket_path: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PackageRecord {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub version: String,
    pub target: String,
    pub source: String,
    pub path: String,
    pub launch: String,
    pub digest: Option<String>,
    pub removable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicationRecord {
    pub project_id: String,
    pub host: String,
    pub mode: String,
    pub service_id: String,
    pub static_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DbInstanceRecord {
    pub id: String,
    pub engine: String,
    pub port: i64,
    pub data_dir: String,
    pub secret_path: String,
    pub state: String,
    pub pid: Option<i64>,
    pub identity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DbBindingRecord {
    pub project_id: String,
    pub connection_id: String,
    pub instance_id: String,
    pub engine: String,
    pub host: String,
    pub port: i64,
    pub database_name: String,
    pub user_name: String,
    pub secret_path: String,
    pub external: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SqliteRecord {
    pub project_id: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub root: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRecord {
    pub project_id: String,
    pub service_id: String,
    pub state: String,
    pub pid: Option<i64>,
    pub process_group: Option<i64>,
    pub started_at: Option<i64>,
    pub identity: Option<String>,
    pub port: Option<i64>,
    pub exit_code: Option<i64>,
    pub detail: String,
}

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
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|source| Error::database("enable write-ahead log", source))?;
        let store = Self { conn };
        store.migrate()?;
        store.retarget_moved_packages()?;
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

    pub fn insert_package(&self, package: &PackageRecord) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO packages (
                    id, kind, name, version, target, source, path, launch, digest, removable
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    package.id,
                    package.kind,
                    package.name,
                    package.version,
                    package.target,
                    package.source,
                    package.path,
                    package.launch,
                    package.digest,
                    i64::from(package.removable),
                ],
            )
            .map_err(|source| Error::database("insert package", source))?;
        self.conn
            .execute(
                "INSERT INTO events (recorded_at, kind, detail) VALUES (?1, 'package_install', ?2)",
                params![now_unix(), package.id],
            )
            .map_err(|source| Error::database("record package event", source))?;
        Ok(())
    }

    pub fn list_packages(&self) -> Result<Vec<PackageRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, kind, name, version, target, source, path, launch, digest, removable
                 FROM packages ORDER BY name, version",
            )
            .map_err(|source| Error::database("prepare package list", source))?;
        let rows = stmt
            .query_map([], map_package)
            .map_err(|source| Error::database("query packages", source))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|source| Error::database("read package", source))
    }

    pub fn get_package(&self, id: &str) -> Result<Option<PackageRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, kind, name, version, target, source, path, launch, digest, removable
                 FROM packages WHERE id = ?1",
            )
            .map_err(|source| Error::database("prepare package read", source))?;
        let mut rows = stmt
            .query(params![id])
            .map_err(|source| Error::database("query package", source))?;
        match rows
            .next()
            .map_err(|source| Error::database("read package row", source))?
        {
            Some(row) => map_package(row)
                .map(Some)
                .map_err(|source| Error::database("read package", source)),
            None => Ok(None),
        }
    }

    pub fn delete_package(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM packages WHERE id = ?1", params![id])
            .map_err(|source| Error::database("delete package", source))?;
        Ok(())
    }

    pub fn select_package(&self, project_id: &str, role: &str, package_id: &str) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO selections (project_id, role, package_id) VALUES (?1, ?2, ?3)
                 ON CONFLICT(project_id, role) DO UPDATE SET package_id = excluded.package_id",
                params![project_id, role, package_id],
            )
            .map_err(|source| Error::database("write selection", source))?;
        Ok(())
    }

    pub fn package_has_running_project(&self, package_id: &str) -> Result<bool> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM (SELECT project_id,package_id FROM selections UNION ALL SELECT project_id,package_id FROM scope_selections) AS s
                 JOIN processes AS p ON p.project_id = s.project_id
                 WHERE s.package_id = ?1 AND p.state IN ('starting', 'running', 'orphaned')",
                params![package_id],
                |row| row.get(0),
            )
            .map_err(|source| Error::database("count running selections", source))?;
        Ok(count > 0)
    }

    pub fn record_selection_history(
        &self,
        project_id: &str,
        role: &str,
        package_id: &str,
        previous_package_id: Option<&str>,
    ) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO selection_history (
                    project_id, role, package_id, previous_package_id, recorded_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    project_id,
                    role,
                    package_id,
                    previous_package_id,
                    now_unix()
                ],
            )
            .map_err(|source| Error::database("record selection history", source))?;
        Ok(())
    }

    pub fn previous_selection(&self, project_id: &str, role: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT previous_package_id FROM selection_history
                 WHERE project_id = ?1 AND role = ?2
                 ORDER BY id DESC LIMIT 1",
            )
            .map_err(|source| Error::database("prepare selection history", source))?;
        let mut rows = stmt
            .query(params![project_id, role])
            .map_err(|source| Error::database("query selection history", source))?;
        match rows
            .next()
            .map_err(|source| Error::database("read selection history", source))?
        {
            Some(row) => row
                .get(0)
                .map_err(|source| Error::database("read previous selection", source)),
            None => Ok(None),
        }
    }

    pub fn clear_generated_notices(&self) -> Result<()> {
        self.conn
            .execute("DELETE FROM update_notices WHERE kind != 'external'", [])
            .map_err(|source| Error::database("clear update notices", source))?;
        Ok(())
    }

    pub fn insert_notice(&self, kind: &str, subject: &str, detail: &str) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO update_notices (kind, subject, detail, recorded_at) VALUES (?1, ?2, ?3, ?4)",
                params![kind, subject, detail, now_unix()],
            )
            .map_err(|source| Error::database("insert update notice", source))?;
        Ok(())
    }

    pub fn replace_external_notice(&self, subject: &str, detail: &str) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM update_notices WHERE kind = 'external' AND subject = ?1",
                params![subject],
            )
            .map_err(|source| Error::database("clear external notice", source))?;
        self.insert_notice("external", subject, detail)
    }

    pub fn list_notices(&self) -> Result<Vec<crate::updates::UpdateNotice>> {
        let mut stmt = self
            .conn
            .prepare("SELECT kind, subject, detail FROM update_notices ORDER BY id")
            .map_err(|source| Error::database("prepare update notices", source))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(crate::updates::UpdateNotice {
                    kind: row.get(0)?,
                    subject: row.get(1)?,
                    detail: row.get(2)?,
                })
            })
            .map_err(|source| Error::database("query update notices", source))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|source| Error::database("read update notice", source))
    }

    pub fn mark_install(&self, catalog_id: &str, state: &str, message: &str) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO install_journal (catalog_id, state, message, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(catalog_id) DO UPDATE SET
                    state = excluded.state,
                    message = excluded.message,
                    updated_at = excluded.updated_at",
                params![catalog_id, state, message, now_unix()],
            )
            .map_err(|source| Error::database("write install journal", source))?;
        Ok(())
    }

    pub fn running_installs(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT catalog_id FROM install_journal WHERE state = 'running' ORDER BY catalog_id",
            )
            .map_err(|source| Error::database("prepare install journal", source))?;
        let rows = stmt
            .query_map([], |row| row.get(0))
            .map_err(|source| Error::database("query install journal", source))?;
        rows.collect::<std::result::Result<Vec<String>, _>>()
            .map_err(|source| Error::database("read install journal", source))
    }

    fn retarget_moved_packages(&self) -> Result<()> {
        self.conn
            .execute(
                "UPDATE packages
                 SET path = REPLACE(path, ?1, ?2),
                     launch = REPLACE(launch, ?1, ?2)",
                params![r"\localdev\", r"\stackyard\"],
            )
            .map_err(|source| Error::database("retarget moved packages", source))?;
        Ok(())
    }

    pub fn record_event(&self, kind: &str, detail: &str) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO events (recorded_at, kind, detail) VALUES (?1, ?2, ?3)",
                params![now_unix(), kind, detail],
            )
            .map_err(|source| Error::database("record event", source))?;
        Ok(())
    }

    pub fn insert_project(&self, project: &ProjectRecord) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO projects (id, name, root, imported_at) VALUES (?1, ?2, ?3, ?4)",
                params![project.id, project.name, project.root, now_unix()],
            )
            .map_err(|source| Error::database("insert project", source))?;
        Ok(())
    }

    /// Register a generated project and its exact scope bindings together.
    /// Reapplying an already registered project preserves subsequent selections.
    pub fn register_created_project(
        &self,
        project: &ProjectRecord,
        packages: &[PackageRecord],
    ) -> Result<()> {
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|source| Error::database("begin project creation", source))?;
        if let Some(existing) = self.get_project(&project.id)? {
            if existing.root == project.root {
                return Ok(());
            }
            return Err(Error::InvalidMessage(format!(
                "Project ID {} is already registered at another path",
                project.id
            )));
        }
        self.insert_project(project)?;
        for package in packages {
            tx.execute(
                "INSERT INTO scope_selections (project_id,scope_id,role,package_id) VALUES (?1,'app',?2,?3)",
                params![project.id, package.name, package.id],
            ).map_err(|source| Error::database("write created project selection", source))?;
            tx.execute(
                "INSERT INTO scope_selection_history (project_id,scope_id,role,package_id,previous_package_id,recorded_at) VALUES (?1,'app',?2,?3,NULL,?4)",
                params![project.id, package.name, package.id, now_unix()],
            ).map_err(|source| Error::database("record created project selection", source))?;
        }
        self.record_event("project_create", &project.id)?;
        tx.commit()
            .map_err(|source| Error::database("commit project creation", source))
    }

    pub fn rename_project(&self, id: &str, name: &str) -> Result<()> {
        self.conn
            .execute(
                "UPDATE projects SET name = ?1 WHERE id = ?2",
                params![name, id],
            )
            .map_err(|source| Error::database("rename project", source))?;
        Ok(())
    }

    pub fn get_project(&self, id: &str) -> Result<Option<ProjectRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, root FROM projects WHERE id = ?1")
            .map_err(|source| Error::database("prepare project read", source))?;
        let mut rows = stmt
            .query(params![id])
            .map_err(|source| Error::database("query project", source))?;
        match rows
            .next()
            .map_err(|source| Error::database("read project row", source))?
        {
            Some(row) => map_project(row)
                .map(Some)
                .map_err(|source| Error::database("read project", source)),
            None => Ok(None),
        }
    }

    pub fn project_by_root(&self, root: &str) -> Result<Option<ProjectRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, root FROM projects WHERE root = ?1")
            .map_err(|source| Error::database("prepare project root", source))?;
        let mut rows = stmt
            .query(params![root])
            .map_err(|source| Error::database("query project root", source))?;
        match rows
            .next()
            .map_err(|source| Error::database("read project root", source))?
        {
            Some(row) => map_project(row)
                .map(Some)
                .map_err(|source| Error::database("read project", source)),
            None => Ok(None),
        }
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, root FROM projects ORDER BY name")
            .map_err(|source| Error::database("prepare project list", source))?;
        let rows = stmt
            .query_map([], map_project)
            .map_err(|source| Error::database("query projects", source))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|source| Error::database("read project", source))
    }

    pub fn upsert_process(&self, process: &ProcessRecord) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO processes (
                    project_id, service_id, state, pid, process_group, started_at,
                    identity, port, exit_code, detail
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(project_id, service_id) DO UPDATE SET
                    state = excluded.state,
                    pid = excluded.pid,
                    process_group = excluded.process_group,
                    started_at = excluded.started_at,
                    identity = excluded.identity,
                    port = excluded.port,
                    exit_code = excluded.exit_code,
                    detail = excluded.detail",
                params![
                    process.project_id,
                    process.service_id,
                    process.state,
                    process.pid,
                    process.process_group,
                    process.started_at,
                    process.identity,
                    process.port,
                    process.exit_code,
                    process.detail,
                ],
            )
            .map_err(|source| Error::database("write process", source))?;
        Ok(())
    }

    pub fn list_processes(&self, project_id: &str) -> Result<Vec<ProcessRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT project_id, service_id, state, pid, process_group, started_at,
                        identity, port, exit_code, detail
                 FROM processes WHERE project_id = ?1 ORDER BY service_id",
            )
            .map_err(|source| Error::database("prepare process list", source))?;
        let rows = stmt
            .query_map(params![project_id], map_process)
            .map_err(|source| Error::database("query processes", source))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|source| Error::database("read process", source))
    }

    pub fn active_processes(&self) -> Result<Vec<ProcessRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT project_id, service_id, state, pid, process_group, started_at,
                        identity, port, exit_code, detail
                 FROM processes WHERE state IN ('starting', 'running', 'orphaned')",
            )
            .map_err(|source| Error::database("prepare active processes", source))?;
        let rows = stmt
            .query_map([], map_process)
            .map_err(|source| Error::database("query active processes", source))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|source| Error::database("read process", source))
    }

    pub fn get_selection(&self, project_id: &str, role: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT package_id FROM selections WHERE project_id = ?1 AND role = ?2")
            .map_err(|source| Error::database("prepare selection", source))?;
        let mut rows = stmt
            .query(params![project_id, role])
            .map_err(|source| Error::database("query selection", source))?;
        match rows
            .next()
            .map_err(|source| Error::database("read selection", source))?
        {
            Some(row) => {
                Ok(Some(row.get(0).map_err(|source| {
                    Error::database("read selection id", source)
                })?))
            }
            None => Ok(None),
        }
    }

    pub fn package_is_selected(&self, package_id: &str) -> Result<bool> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM selections WHERE package_id=?1) + (SELECT COUNT(*) FROM scope_selections WHERE package_id=?1)",
                params![package_id],
                |row| row.get(0),
            )
            .map_err(|source| Error::database("count selections", source))?;
        Ok(count > 0)
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
        if self.schema_version()? < 2 {
            let tx = self
                .conn
                .unchecked_transaction()
                .map_err(|source| Error::database("begin migration 2", source))?;
            tx.execute_batch(
                "CREATE TABLE packages (
                    id TEXT PRIMARY KEY,
                    kind TEXT NOT NULL,
                    name TEXT NOT NULL,
                    version TEXT NOT NULL,
                    target TEXT NOT NULL,
                    source TEXT NOT NULL,
                    path TEXT NOT NULL,
                    launch TEXT NOT NULL,
                    digest TEXT,
                    removable INTEGER NOT NULL
                );
                CREATE TABLE selections (
                    project_id TEXT NOT NULL,
                    role TEXT NOT NULL,
                    package_id TEXT NOT NULL,
                    PRIMARY KEY (project_id, role)
                );",
            )
            .map_err(|source| Error::database("apply migration 2", source))?;
            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (2, ?1)",
                params![now_unix()],
            )
            .map_err(|source| Error::database("record migration 2", source))?;
            tx.commit()
                .map_err(|source| Error::database("commit migration 2", source))?;
        }
        if self.schema_version()? < 3 {
            let tx = self
                .conn
                .unchecked_transaction()
                .map_err(|source| Error::database("begin migration 3", source))?;
            tx.execute_batch(
                "CREATE TABLE projects (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    root TEXT NOT NULL UNIQUE,
                    imported_at INTEGER NOT NULL
                );
                CREATE TABLE processes (
                    project_id TEXT NOT NULL,
                    service_id TEXT NOT NULL,
                    state TEXT NOT NULL,
                    pid INTEGER,
                    process_group INTEGER,
                    started_at INTEGER,
                    identity TEXT,
                    port INTEGER,
                    exit_code INTEGER,
                    detail TEXT NOT NULL,
                    PRIMARY KEY (project_id, service_id)
                );",
            )
            .map_err(|source| Error::database("apply migration 3", source))?;
            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (3, ?1)",
                params![now_unix()],
            )
            .map_err(|source| Error::database("record migration 3", source))?;
            tx.commit()
                .map_err(|source| Error::database("commit migration 3", source))?;
        }
        if self.schema_version()? < 4 {
            let tx = self
                .conn
                .unchecked_transaction()
                .map_err(|source| Error::database("begin migration 4", source))?;
            tx.execute_batch(
                "CREATE TABLE publications (
                    project_id TEXT PRIMARY KEY,
                    host TEXT NOT NULL UNIQUE,
                    mode TEXT NOT NULL,
                    service_id TEXT NOT NULL,
                    static_dir TEXT NOT NULL,
                    FOREIGN KEY (project_id) REFERENCES projects(id)
                );",
            )
            .map_err(|source| Error::database("apply migration 4", source))?;
            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (4, ?1)",
                params![now_unix()],
            )
            .map_err(|source| Error::database("record migration 4", source))?;
            tx.commit()
                .map_err(|source| Error::database("commit migration 4", source))?;
        }
        if self.schema_version()? < 5 {
            let tx = self
                .conn
                .unchecked_transaction()
                .map_err(|source| Error::database("begin migration 5", source))?;
            tx.execute_batch(
                "CREATE TABLE db_instances (
                    id TEXT PRIMARY KEY,
                    engine TEXT NOT NULL,
                    port INTEGER NOT NULL,
                    data_dir TEXT NOT NULL,
                    secret_path TEXT NOT NULL,
                    state TEXT NOT NULL,
                    pid INTEGER,
                    identity TEXT
                );
                CREATE TABLE db_bindings (
                    project_id TEXT NOT NULL,
                    connection_id TEXT NOT NULL,
                    instance_id TEXT NOT NULL,
                    engine TEXT NOT NULL,
                    host TEXT NOT NULL,
                    port INTEGER NOT NULL,
                    database_name TEXT NOT NULL,
                    user_name TEXT NOT NULL,
                    secret_path TEXT NOT NULL,
                    external INTEGER NOT NULL,
                    PRIMARY KEY (project_id, connection_id)
                );
                CREATE TABLE project_sqlite (
                    project_id TEXT PRIMARY KEY,
                    path TEXT NOT NULL
                );",
            )
            .map_err(|source| Error::database("apply migration 5", source))?;
            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (5, ?1)",
                params![now_unix()],
            )
            .map_err(|source| Error::database("record migration 5", source))?;
            tx.commit()
                .map_err(|source| Error::database("commit migration 5", source))?;
        }
        if self.schema_version()? < 6 {
            let tx = self
                .conn
                .unchecked_transaction()
                .map_err(|source| Error::database("begin migration 6", source))?;
            tx.execute_batch(
                "CREATE TABLE update_notices (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    kind TEXT NOT NULL,
                    subject TEXT NOT NULL,
                    detail TEXT NOT NULL,
                    recorded_at INTEGER NOT NULL
                );
                CREATE TABLE selection_history (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    project_id TEXT NOT NULL,
                    role TEXT NOT NULL,
                    package_id TEXT NOT NULL,
                    previous_package_id TEXT,
                    recorded_at INTEGER NOT NULL
                );
                CREATE TABLE install_journal (
                    catalog_id TEXT PRIMARY KEY,
                    state TEXT NOT NULL,
                    message TEXT NOT NULL,
                    updated_at INTEGER NOT NULL
                );",
            )
            .map_err(|source| Error::database("apply migration 6", source))?;
            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (6, ?1)",
                params![now_unix()],
            )
            .map_err(|source| Error::database("record migration 6", source))?;
            tx.commit()
                .map_err(|source| Error::database("commit migration 6", source))?;
        }
        if self.schema_version()? < 7 {
            let tx = self
                .conn
                .unchecked_transaction()
                .map_err(|source| Error::database("begin migration 7", source))?;
            tx.execute_batch(
                "CREATE TABLE cache_instances (
                id TEXT PRIMARY KEY, package_id TEXT NOT NULL, port INTEGER NOT NULL,
                state TEXT NOT NULL, pid INTEGER, identity TEXT, socket_path TEXT NOT NULL,
                detail TEXT NOT NULL, FOREIGN KEY(package_id) REFERENCES packages(id)
            );",
            )
            .map_err(|source| Error::database("apply migration 7", source))?;
            tx.execute(
                "INSERT INTO schema_migrations (version,applied_at) VALUES (7,?1)",
                params![now_unix()],
            )
            .map_err(|source| Error::database("record migration 7", source))?;
            tx.commit()
                .map_err(|source| Error::database("commit migration 7", source))?;
        }
        if self.schema_version()? < 8 {
            let tx = self
                .conn
                .unchecked_transaction()
                .map_err(|source| Error::database("begin migration 8", source))?;
            tx.execute_batch("CREATE TABLE scope_selections (
                project_id TEXT NOT NULL,scope_id TEXT NOT NULL,role TEXT NOT NULL,package_id TEXT NOT NULL,
                PRIMARY KEY(project_id,scope_id,role),FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE,FOREIGN KEY(package_id) REFERENCES packages(id));
                CREATE TABLE scope_selection_history (id INTEGER PRIMARY KEY AUTOINCREMENT,project_id TEXT NOT NULL,scope_id TEXT NOT NULL,role TEXT NOT NULL,package_id TEXT NOT NULL,previous_package_id TEXT,recorded_at INTEGER NOT NULL);
                CREATE TABLE scope_environments (project_id TEXT NOT NULL,scope_id TEXT NOT NULL,path TEXT NOT NULL,PRIMARY KEY(project_id,scope_id),FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE);
                CREATE TABLE service_observations (project_id TEXT NOT NULL,service_id TEXT NOT NULL,readiness_state TEXT NOT NULL,http_status INTEGER,PRIMARY KEY(project_id,service_id),FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE);")
                .map_err(|source|Error::database("apply migration 8",source))?;
            tx.execute(
                "INSERT INTO schema_migrations(version,applied_at) VALUES(8,?1)",
                params![now_unix()],
            )
            .map_err(|source| Error::database("record migration 8", source))?;
            tx.commit()
                .map_err(|source| Error::database("commit migration 8", source))?;
        }
        Ok(())
    }

    /// Preview snapshots never create/migrate/retarget local state.
    pub fn open_read_only(data_dir: &Path) -> Result<Option<Self>> {
        let path = data_dir.join("state.sqlite3");
        if !path.is_file() {
            return Ok(None);
        }
        let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|source| Error::database("open read-only state snapshot", source))?;
        let store = Self { conn };
        if store.schema_version()? > SCHEMA_VERSION {
            return Err(Error::SchemaTooNew {
                found: store.schema_version()?,
                supported: SCHEMA_VERSION,
            });
        }
        Ok(Some(store))
    }

    pub fn get_scope_selection(
        &self,
        project: &str,
        scope: &str,
        role: &str,
    ) -> Result<Option<String>> {
        if self.schema_version()? < 8 {
            return Ok(None);
        }
        use rusqlite::OptionalExtension;
        self.conn.query_row("SELECT package_id FROM scope_selections WHERE project_id=?1 AND scope_id=?2 AND role=?3",params![project,scope,role],|row|row.get(0)).optional()
            .map_err(|source|Error::database("read scope selection",source))
    }

    pub fn select_scope_package(
        &self,
        project: &str,
        scope: &str,
        role: &str,
        package: &str,
    ) -> Result<()> {
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|source| Error::database("begin scope selection", source))?;
        let previous = self.get_scope_selection(project, scope, role)?;
        tx.execute("INSERT INTO scope_selections (project_id,scope_id,role,package_id) VALUES (?1,?2,?3,?4) ON CONFLICT(project_id,scope_id,role) DO UPDATE SET package_id=excluded.package_id",params![project,scope,role,package])
            .map_err(|source|Error::database("write scope selection",source))?;
        tx.execute("INSERT INTO scope_selection_history (project_id,scope_id,role,package_id,previous_package_id,recorded_at) VALUES (?1,?2,?3,?4,?5,?6)",params![project,scope,role,package,previous,now_unix()])
            .map_err(|source|Error::database("record scope selection",source))?;
        tx.commit()
            .map_err(|source| Error::database("commit scope selection", source))
    }

    pub fn scope_selections(&self, project: &str) -> Result<Vec<ScopeSelectionRecord>> {
        let mut stmt=self.conn.prepare("SELECT project_id,scope_id,role,package_id FROM scope_selections WHERE project_id=?1 ORDER BY scope_id,role").map_err(|source|Error::database("prepare scope selections",source))?;
        let rows = stmt
            .query_map(params![project], |row| {
                Ok(ScopeSelectionRecord {
                    project_id: row.get(0)?,
                    scope_id: row.get(1)?,
                    role: row.get(2)?,
                    package_id: row.get(3)?,
                })
            })
            .map_err(|source| Error::database("query scope selections", source))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|source| Error::database("read scope selections", source))
    }

    pub fn get_scope_environment(&self, project: &str, scope: &str) -> Result<Option<String>> {
        use rusqlite::OptionalExtension;
        self.conn
            .query_row(
                "SELECT path FROM scope_environments WHERE project_id=?1 AND scope_id=?2",
                params![project, scope],
                |row| row.get(0),
            )
            .optional()
            .map_err(|source| Error::database("read scope environment", source))
    }

    pub fn bind_scope_environment(&self, project: &str, scope: &str, path: &str) -> Result<()> {
        self.conn.execute("INSERT INTO scope_environments (project_id,scope_id,path) VALUES (?1,?2,?3) ON CONFLICT(project_id,scope_id) DO UPDATE SET path=excluded.path",params![project,scope,path]).map_err(|source|Error::database("bind scope environment",source))?;
        Ok(())
    }

    pub fn get_service_observation(
        &self,
        project: &str,
        service: &str,
    ) -> Result<Option<ServiceObservationRecord>> {
        use rusqlite::OptionalExtension;
        self.conn.query_row("SELECT project_id,service_id,readiness_state,http_status FROM service_observations WHERE project_id=?1 AND service_id=?2",params![project,service],|row|Ok(ServiceObservationRecord{project_id:row.get(0)?,service_id:row.get(1)?,readiness_state:row.get(2)?,http_status:row.get(3)?})).optional().map_err(|source|Error::database("read service observation",source))
    }

    pub fn upsert_service_observation(&self, record: &ServiceObservationRecord) -> Result<()> {
        self.conn.execute("INSERT INTO service_observations (project_id,service_id,readiness_state,http_status) VALUES (?1,?2,?3,?4) ON CONFLICT(project_id,service_id) DO UPDATE SET readiness_state=excluded.readiness_state,http_status=excluded.http_status",params![record.project_id,record.service_id,record.readiness_state,record.http_status]).map_err(|source|Error::database("write service observation",source))?;
        Ok(())
    }

    pub fn list_cache_instances(&self) -> Result<Vec<CacheInstanceRecord>> {
        let mut statement = self.conn.prepare("SELECT id,package_id,port,state,pid,identity,socket_path,detail FROM cache_instances ORDER BY id")
            .map_err(|source| Error::database("prepare cache instances",source))?;
        let rows = statement
            .query_map([], |row| {
                Ok(CacheInstanceRecord {
                    id: row.get(0)?,
                    package_id: row.get(1)?,
                    port: row.get(2)?,
                    state: row.get(3)?,
                    pid: row.get(4)?,
                    identity: row.get(5)?,
                    socket_path: row.get(6)?,
                    detail: row.get(7)?,
                })
            })
            .map_err(|source| Error::database("query cache instances", source))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|source| Error::database("read cache instances", source))
    }

    pub fn get_cache_instance(&self, id: &str) -> Result<Option<CacheInstanceRecord>> {
        Ok(self
            .list_cache_instances()?
            .into_iter()
            .find(|instance| instance.id == id))
    }

    pub fn upsert_cache_instance(&self, record: &CacheInstanceRecord) -> Result<()> {
        self.conn.execute("INSERT INTO cache_instances (id,package_id,port,state,pid,identity,socket_path,detail) VALUES (?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(id) DO UPDATE SET package_id=excluded.package_id,port=excluded.port,state=excluded.state,pid=excluded.pid,identity=excluded.identity,socket_path=excluded.socket_path,detail=excluded.detail",
            params![record.id,record.package_id,record.port,record.state,record.pid,record.identity,record.socket_path,record.detail])
            .map_err(|source|Error::database("write cache instance",source))?;
        Ok(())
    }

    pub fn delete_cache_instance(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM cache_instances WHERE id=?1", params![id])
            .map_err(|source| Error::database("forget cache instance", source))?;
        Ok(())
    }

    pub fn upsert_db_instance(&self, record: &DbInstanceRecord) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO db_instances (id, engine, port, data_dir, secret_path, state, pid, identity)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET
                    state = excluded.state,
                    pid = excluded.pid,
                    identity = excluded.identity,
                    port = excluded.port",
                params![
                    record.id,
                    record.engine,
                    record.port,
                    record.data_dir,
                    record.secret_path,
                    record.state,
                    record.pid,
                    record.identity
                ],
            )
            .map_err(|source| Error::database("write database instance", source))?;
        Ok(())
    }

    pub fn list_db_instances(&self) -> Result<Vec<DbInstanceRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, engine, port, data_dir, secret_path, state, pid, identity
                 FROM db_instances ORDER BY id",
            )
            .map_err(|source| Error::database("prepare database instances", source))?;
        let rows = stmt
            .query_map([], map_instance)
            .map_err(|source| Error::database("query database instances", source))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|source| Error::database("read database instance", source))
    }

    pub fn get_db_instance(&self, id: &str) -> Result<Option<DbInstanceRecord>> {
        Ok(self
            .list_db_instances()?
            .into_iter()
            .find(|row| row.id == id))
    }

    pub fn bindings_for_instance(&self, instance_id: &str) -> Result<Vec<DbBindingRecord>> {
        Ok(self
            .list_db_bindings()?
            .into_iter()
            .filter(|row| row.instance_id == instance_id)
            .collect())
    }

    pub fn upsert_db_binding(&self, record: &DbBindingRecord) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO db_bindings (
                    project_id, connection_id, instance_id, engine, host, port,
                    database_name, user_name, secret_path, external
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(project_id, connection_id) DO UPDATE SET
                    instance_id = excluded.instance_id,
                    engine = excluded.engine,
                    host = excluded.host,
                    port = excluded.port,
                    database_name = excluded.database_name,
                    user_name = excluded.user_name,
                    secret_path = excluded.secret_path,
                    external = excluded.external",
                params![
                    record.project_id,
                    record.connection_id,
                    record.instance_id,
                    record.engine,
                    record.host,
                    record.port,
                    record.database_name,
                    record.user_name,
                    record.secret_path,
                    i64::from(record.external)
                ],
            )
            .map_err(|source| Error::database("write database binding", source))?;
        Ok(())
    }

    pub fn delete_db_binding(&self, project_id: &str, connection_id: &str) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM db_bindings WHERE project_id = ?1 AND connection_id = ?2",
                params![project_id, connection_id],
            )
            .map_err(|source| Error::database("delete database binding", source))?;
        Ok(())
    }

    pub fn list_db_bindings(&self) -> Result<Vec<DbBindingRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT project_id, connection_id, instance_id, engine, host, port,
                        database_name, user_name, secret_path, external
                 FROM db_bindings ORDER BY project_id, connection_id",
            )
            .map_err(|source| Error::database("prepare database bindings", source))?;
        let rows = stmt
            .query_map([], |row| {
                let external: i64 = row.get(9)?;
                Ok(DbBindingRecord {
                    project_id: row.get(0)?,
                    connection_id: row.get(1)?,
                    instance_id: row.get(2)?,
                    engine: row.get(3)?,
                    host: row.get(4)?,
                    port: row.get(5)?,
                    database_name: row.get(6)?,
                    user_name: row.get(7)?,
                    secret_path: row.get(8)?,
                    external: external != 0,
                })
            })
            .map_err(|source| Error::database("query database bindings", source))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|source| Error::database("read database binding", source))
    }

    pub fn upsert_sqlite(&self, record: &SqliteRecord) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO project_sqlite (project_id, path) VALUES (?1, ?2)
                 ON CONFLICT(project_id) DO UPDATE SET path = excluded.path",
                params![record.project_id, record.path],
            )
            .map_err(|source| Error::database("write sqlite path", source))?;
        Ok(())
    }

    pub fn get_sqlite(&self, project_id: &str) -> Result<Option<SqliteRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT project_id, path FROM project_sqlite WHERE project_id = ?1")
            .map_err(|source| Error::database("prepare sqlite path", source))?;
        let mut rows = stmt
            .query(params![project_id])
            .map_err(|source| Error::database("query sqlite path", source))?;
        match rows
            .next()
            .map_err(|source| Error::database("read sqlite path", source))?
        {
            Some(row) => Ok(Some(SqliteRecord {
                project_id: row
                    .get(0)
                    .map_err(|source| Error::database("read sqlite project", source))?,
                path: row
                    .get(1)
                    .map_err(|source| Error::database("read sqlite file", source))?,
            })),
            None => Ok(None),
        }
    }

    pub fn upsert_publication(&self, record: &PublicationRecord) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO publications (project_id, host, mode, service_id, static_dir)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(project_id) DO UPDATE SET
                    host = excluded.host,
                    mode = excluded.mode,
                    service_id = excluded.service_id,
                    static_dir = excluded.static_dir",
                params![
                    record.project_id,
                    record.host,
                    record.mode,
                    record.service_id,
                    record.static_dir
                ],
            )
            .map_err(|source| Error::database("write publication", source))?;
        Ok(())
    }

    pub fn delete_publication(&self, project_id: &str) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM publications WHERE project_id = ?1",
                params![project_id],
            )
            .map_err(|source| Error::database("delete publication", source))?;
        Ok(())
    }

    pub fn list_publications(&self) -> Result<Vec<PublicationRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT project_id, host, mode, service_id, static_dir
                 FROM publications ORDER BY host",
            )
            .map_err(|source| Error::database("prepare publications", source))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(PublicationRecord {
                    project_id: row.get(0)?,
                    host: row.get(1)?,
                    mode: row.get(2)?,
                    service_id: row.get(3)?,
                    static_dir: row.get(4)?,
                })
            })
            .map_err(|source| Error::database("query publications", source))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|source| Error::database("read publication", source))
    }
}

fn map_instance(row: &rusqlite::Row<'_>) -> rusqlite::Result<DbInstanceRecord> {
    Ok(DbInstanceRecord {
        id: row.get(0)?,
        engine: row.get(1)?,
        port: row.get(2)?,
        data_dir: row.get(3)?,
        secret_path: row.get(4)?,
        state: row.get(5)?,
        pid: row.get(6)?,
        identity: row.get(7)?,
    })
}

fn map_package(row: &rusqlite::Row<'_>) -> rusqlite::Result<PackageRecord> {
    let removable: i64 = row.get(9)?;
    Ok(PackageRecord {
        id: row.get(0)?,
        kind: row.get(1)?,
        name: row.get(2)?,
        version: row.get(3)?,
        target: row.get(4)?,
        source: row.get(5)?,
        path: row.get(6)?,
        launch: row.get(7)?,
        digest: row.get(8)?,
        removable: removable != 0,
    })
}

fn map_project(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectRecord> {
    Ok(ProjectRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        root: row.get(2)?,
    })
}

fn map_process(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProcessRecord> {
    Ok(ProcessRecord {
        project_id: row.get(0)?,
        service_id: row.get(1)?,
        state: row.get(2)?,
        pid: row.get(3)?,
        process_group: row.get(4)?,
        started_at: row.get(5)?,
        identity: row.get(6)?,
        port: row.get(7)?,
        exit_code: row.get(8)?,
        detail: row.get(9)?,
    })
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
        prepare_data_dir(&crate::paths::disposable_dir("store")).expect("temp dir")
    }

    #[test]
    fn settings_and_schema_survive_reopen() {
        let dir = temp_dir();
        {
            let store = Store::open(&dir).expect("open");
            assert_eq!(store.schema_version().expect("version"), 8);
            store.put("display_name", "Ada").expect("put");
            assert_eq!(
                store.last_event().expect("event").as_deref(),
                Some("setting_put display_name")
            );
        }
        let store = Store::open(&dir).expect("reopen");
        assert_eq!(store.schema_version().expect("version"), 8);
        assert_eq!(
            store.get("display_name").expect("get").as_deref(),
            Some("Ada")
        );
        assert!(store.get("missing").expect("missing").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn migration8_preserves_legacy_state_and_scope_bindings_survive_reopen() {
        let dir = temp_dir();
        let env = dir.join("environments/legacy");
        std::fs::create_dir_all(&env).unwrap();
        std::fs::write(env.join("marker"), b"preserved").unwrap();
        let active = dir.join("php/profiles/legacy/active.json");
        std::fs::create_dir_all(active.parent().unwrap()).unwrap();
        std::fs::write(&active, b"unchanged-active-pointer").unwrap();
        {
            let store = Store::open(&dir).unwrap();
            store
                .insert_project(&ProjectRecord {
                    id: "legacy".into(),
                    name: "legacy".into(),
                    root: "project-root".into(),
                })
                .unwrap();
            store
                .insert_package(&PackageRecord {
                    id: "python-old".into(),
                    kind: "runtime".into(),
                    name: "python".into(),
                    version: "3.12.16".into(),
                    target: crate::current_target().into(),
                    source: "existing".into(),
                    path: "/external".into(),
                    launch: "/external/python".into(),
                    digest: None,
                    removable: false,
                })
                .unwrap();
            store
                .select_package("legacy", "python", "python-old")
                .unwrap();
            store
                .record_selection_history("legacy", "python", "python-old", None)
                .unwrap();
            store
                .upsert_process(&ProcessRecord {
                    project_id: "legacy".into(),
                    service_id: "api".into(),
                    state: "stopped".into(),
                    pid: None,
                    process_group: Some(47),
                    started_at: Some(1),
                    identity: Some("original".into()),
                    port: Some(8123),
                    exit_code: Some(0),
                    detail: "kept".into(),
                })
                .unwrap();
            // Remove precisely migration 8's additions to reproduce the schema-7 input.
            store.conn.execute_batch("DROP TABLE scope_selections; DROP TABLE scope_selection_history; DROP TABLE scope_environments; DROP TABLE service_observations; DELETE FROM schema_migrations WHERE version=8;").unwrap();
        }
        {
            let readonly = Store::open_read_only(&dir).unwrap().unwrap();
            assert_eq!(readonly.schema_version().unwrap(), 7);
            assert_eq!(
                readonly
                    .get_scope_selection("legacy", "default", "python")
                    .unwrap(),
                None
            );
            assert!(readonly.put("forbidden", "write").is_err());
        }
        {
            let store = Store::open(&dir).unwrap();
            assert_eq!(store.schema_version().unwrap(), 8);
            assert_eq!(
                store.get_selection("legacy", "python").unwrap().as_deref(),
                Some("python-old")
            );
            assert_eq!(
                store
                    .conn
                    .query_row("SELECT count(*) FROM selection_history", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                1
            );
            let process = store.list_processes("legacy").unwrap().remove(0);
            assert_eq!(process.identity.as_deref(), Some("original"));
            assert_eq!(process.port, Some(8123));
            assert!(store
                .get_service_observation("legacy", "api")
                .unwrap()
                .is_none());
            store
                .bind_scope_environment("legacy", "default", env.to_str().unwrap())
                .unwrap();
            store
                .select_scope_package("legacy", "default", "python", "python-old")
                .unwrap();
        }
        let store = Store::open(&dir).unwrap();
        assert_eq!(
            store
                .get_scope_environment("legacy", "default")
                .unwrap()
                .as_deref(),
            env.to_str()
        );
        assert_eq!(
            store
                .get_scope_selection("legacy", "default", "python")
                .unwrap()
                .as_deref(),
            Some("python-old")
        );
        assert!(store.package_is_selected("python-old").unwrap());
        assert_eq!(std::fs::read(env.join("marker")).unwrap(), b"preserved");
        assert_eq!(std::fs::read(active).unwrap(), b"unchanged-active-pointer");
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
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

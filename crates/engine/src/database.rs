//! Managed MySQL and PostgreSQL instances, plus project SQLite files.
//!
//! A project stop does not stop a database. An external connection has no
//! process to stop. Data directories stay outside the package tree.

use std::fs;
use std::io::Write;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use stackyard_core::{
    contained_sqlite, port_is_free, random_secret, read_secret, redact, require_empty_data_dir,
    require_ident, sqlite_backup, sqlite_init, sqlite_read, sqlite_write, write_secret, Error,
    Outcome, Result, Store, DEFAULT_MYSQL_PORT, DEFAULT_POSTGRES_PORT,
};

use crate::group::{self, GroupProcess};

pub struct Databases {
    data_dir: PathBuf,
    running: Mutex<Vec<(String, GroupProcess)>>,
}

impl Databases {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            running: Mutex::new(Vec::new()),
        }
    }

    pub fn shutdown(&self) {
        let store = Store::open(&self.data_dir).ok();
        let ids: Vec<String> = store
            .as_ref()
            .and_then(|store| store.list_db_instances().ok())
            .unwrap_or_default()
            .into_iter()
            .map(|row| row.id)
            .collect();
        for id in ids {
            let _ = self.stop(&id);
        }
    }

    pub fn list(&self) -> Result<Outcome> {
        self.view("databases")
    }

    pub fn migrate(&self) -> Result<Outcome> {
        Err(Error::DatabaseMigrationUnsupported)
    }

    pub fn create(&self, engine: &str, port: u16) -> Result<Outcome> {
        let engine = match engine {
            "mysql" | "postgres" => engine,
            other => {
                return Err(Error::InvalidMessage(format!(
                    "database engine {other:?} must be mysql or postgres"
                )))
            }
        };
        let port = if port == 0 {
            let start = if engine == "mysql" {
                DEFAULT_MYSQL_PORT
            } else {
                DEFAULT_POSTGRES_PORT
            };
            (start..start + 20)
                .find(|port| port_is_free(*port))
                .ok_or_else(|| Error::InvalidMessage("no free database port".to_string()))?
        } else {
            if !(1024..65535).contains(&port) {
                return Err(Error::InvalidMessage(
                    "database port must be a high port".to_string(),
                ));
            }
            if !port_is_free(port) {
                stackyard_core::require_free(port)?;
            }
            port
        };
        let id = format!("{engine}-{port}");
        let root = self.data_dir.join("databases").join(&id);
        let data = root.join("data");
        require_empty_data_dir(&data)?;
        fs::create_dir_all(&root)
            .map_err(|source| Error::io("create database directory", source))?;
        let secret = random_secret()?;
        let secret_path = write_secret(&self.data_dir.join("secrets"), &id, &secret)?;
        let server = self.server_binary(engine)?;
        if engine == "mysql" {
            init_mysql(&server, &data)?;
        } else {
            init_postgres(&server, &data, &secret_path, port)?;
        }
        let mut record = stackyard_core::DbInstanceRecord {
            id: id.clone(),
            engine: engine.to_string(),
            port: i64::from(port),
            data_dir: data.display().to_string(),
            secret_path: secret_path.display().to_string(),
            state: "stopped".to_string(),
            pid: None,
            identity: None,
        };
        Store::open(&self.data_dir)?.upsert_db_instance(&record)?;
        self.start_record(&mut record)?;
        if engine == "mysql" {
            bootstrap_mysql(&server, &record, &secret)?;
        }
        self.view(&format!("created {id}"))
    }

    pub fn start(&self, id: &str) -> Result<Outcome> {
        let mut record = self.instance(id)?;
        self.start_record(&mut record)?;
        self.view(&format!("started {id}"))
    }

    pub fn stop(&self, id: &str) -> Result<Outcome> {
        let mut record = self.instance(id)?;
        self.stop_record(&mut record)?;
        self.view(&format!("stopped {id}"))
    }

    pub fn add(&self, instance_id: &str, project_id: &str, name: &str) -> Result<Outcome> {
        require_ident(name, "database")?;
        let instance = self.instance(instance_id)?;
        if instance.state != "running" {
            return Err(Error::InvalidMessage(format!(
                "database {instance_id} is {}",
                instance.state
            )));
        }
        let _ = Store::open(&self.data_dir)?
            .get_project(project_id)?
            .ok_or_else(|| Error::ProjectNotFound {
                id: project_id.to_string(),
            })?;
        let password = random_secret()?;
        let secret_path = write_secret(
            &self.data_dir.join("secrets"),
            &format!("{instance_id}-{name}"),
            &password,
        )?;
        let server = self.server_binary(&instance.engine)?;
        let admin = read_secret(Path::new(&instance.secret_path))?;
        if instance.engine == "mysql" {
            let sql = format!(
                "CREATE DATABASE IF NOT EXISTS {name}; CREATE USER IF NOT EXISTS '{name}'@'127.0.0.1' IDENTIFIED BY '{password}'; GRANT ALL ON {name}.* TO '{name}'@'127.0.0.1';"
            );
            mysql_sql(&server, &instance, "root", &admin, "mysql", &sql)?;
        } else {
            postgres_sql(
                &server,
                &instance,
                "stackyard",
                &admin,
                "postgres",
                &format!("CREATE ROLE {name} LOGIN PASSWORD '{password}'"),
            )?;
            postgres_sql(
                &server,
                &instance,
                "stackyard",
                &admin,
                "postgres",
                &format!("CREATE DATABASE {name} OWNER {name}"),
            )?;
        }
        Store::open(&self.data_dir)?.upsert_db_binding(&stackyard_core::DbBindingRecord {
            project_id: project_id.to_string(),
            connection_id: name.to_string(),
            instance_id: instance.id,
            engine: instance.engine,
            host: "127.0.0.1".to_string(),
            port: instance.port,
            database_name: name.to_string(),
            user_name: name.to_string(),
            secret_path: secret_path.display().to_string(),
            external: false,
        })?;
        self.probe_binding(project_id, name)?;
        self.view(&format!("added {name}"))
    }

    pub fn probe(&self, project_id: &str, name: &str) -> Result<String> {
        self.probe_binding(project_id, name)
    }

    pub fn backup(&self, instance_id: &str, database: &str, destination: &Path) -> Result<Outcome> {
        require_ident(database, "database")?;
        let instance = self.instance(instance_id)?;
        let server = self.server_binary(&instance.engine)?;
        let admin = read_secret(Path::new(&instance.secret_path))?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|source| Error::io("create backup directory", source))?;
        }
        if instance.engine == "mysql" {
            let dump = tool_near(&server, &["mysqldump", "mysqldump.exe"])?;
            let defaults = mysql_defaults(&instance, "root", &admin)?;
            let output = Command::new(dump)
                .arg(format!("--defaults-file={}", defaults.display()))
                .arg("--protocol=tcp")
                .arg(database)
                .arg(format!("--result-file={}", destination.display()))
                .output()
                .map_err(|source| Error::io("run mysqldump", source))?;
            command_ok("mysqldump", &output, &admin)?;
        } else {
            let dump = tool_near(&server, &["pg_dump", "pg_dump.exe"])?;
            let pass = pgpass(&instance, "stackyard", &admin)?;
            let output = Command::new(dump)
                .env("PGPASSFILE", &pass)
                .arg("-h")
                .arg("127.0.0.1")
                .arg("-p")
                .arg(instance.port.to_string())
                .arg("-U")
                .arg("stackyard")
                .arg("-d")
                .arg(database)
                .arg("-f")
                .arg(destination)
                .output()
                .map_err(|source| Error::io("run pg_dump", source))?;
            command_ok("pg_dump", &output, &admin)?;
        }
        self.view(&format!("backed up {database}"))
    }

    pub fn restore(
        &self,
        instance_id: &str,
        database: &str,
        source: &Path,
        into: &str,
    ) -> Result<Outcome> {
        require_ident(database, "database")?;
        require_ident(into, "database")?;
        if database == into {
            return Err(Error::InvalidMessage(
                "restore into a new database name".to_string(),
            ));
        }
        let instance = self.instance(instance_id)?;
        let server = self.server_binary(&instance.engine)?;
        let admin = read_secret(Path::new(&instance.secret_path))?;
        if instance.engine == "mysql" {
            mysql_sql(
                &server,
                &instance,
                "root",
                &admin,
                "mysql",
                &format!("CREATE DATABASE {into}"),
            )?;
            let defaults = mysql_defaults(&instance, "root", &admin)?;
            let sql = fs::read(source).map_err(|err| Error::io("read mysql backup", err))?;
            let mut child = Command::new(tool_near(&server, &["mysql", "mysql.exe"])?)
                .arg(format!("--defaults-file={}", defaults.display()))
                .arg("--protocol=tcp")
                .arg(into)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|err| Error::io("run mysql restore", err))?;
            child
                .stdin
                .take()
                .ok_or_else(|| Error::InvalidMessage("mysql restore had no stdin".to_string()))?
                .write_all(&sql)
                .map_err(|err| Error::io("write mysql restore", err))?;
            let output = child
                .wait_with_output()
                .map_err(|err| Error::io("wait for mysql restore", err))?;
            command_ok("mysql restore", &output, &admin)?;
        } else {
            postgres_sql(
                &server,
                &instance,
                "stackyard",
                &admin,
                "postgres",
                &format!("CREATE DATABASE {into} OWNER stackyard"),
            )?;
            let psql = tool_near(&server, &["psql", "psql.exe"])?;
            let pass = pgpass(&instance, "stackyard", &admin)?;
            let output = Command::new(psql)
                .env("PGPASSFILE", &pass)
                .arg("-h")
                .arg("127.0.0.1")
                .arg("-p")
                .arg(instance.port.to_string())
                .arg("-U")
                .arg("stackyard")
                .arg("-d")
                .arg(into)
                .arg("-f")
                .arg(source)
                .output()
                .map_err(|err| Error::io("run psql restore", err))?;
            command_ok("psql restore", &output, &admin)?;
        }
        let check = if instance.engine == "mysql" {
            mysql_sql(
                &server,
                &instance,
                "root",
                &admin,
                into,
                "SELECT note FROM stackyard_probe WHERE id = 1",
            )?
        } else {
            postgres_sql(
                &server,
                &instance,
                "stackyard",
                &admin,
                into,
                "SELECT note FROM stackyard_probe WHERE id = 1",
            )?
        };
        if !check.contains("kept") {
            return Err(Error::InvalidMessage(format!(
                "restored database {into} did not contain the saved row"
            )));
        }
        self.view(&format!("restored {database} into {into}"))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn attach(
        &self,
        project_id: &str,
        engine: &str,
        host: &str,
        port: u16,
        database: &str,
        user: &str,
        password: &str,
    ) -> Result<Outcome> {
        if !matches!(engine, "mysql" | "postgres") {
            return Err(Error::InvalidMessage(
                "an external database must be mysql or postgres".to_string(),
            ));
        }
        if host != "127.0.0.1" && host != "localhost" {
            return Err(Error::InvalidMessage(
                "an external database host must be loopback in this build".to_string(),
            ));
        }
        require_ident(database, "database")?;
        require_ident(user, "user")?;
        let _ = Store::open(&self.data_dir)?
            .get_project(project_id)?
            .ok_or_else(|| Error::ProjectNotFound {
                id: project_id.to_string(),
            })?;
        let secret_path = write_secret(
            &self.data_dir.join("secrets"),
            &format!("external-{project_id}-{database}"),
            password,
        )?;
        Store::open(&self.data_dir)?.upsert_db_binding(&stackyard_core::DbBindingRecord {
            project_id: project_id.to_string(),
            connection_id: format!("external-{database}"),
            instance_id: String::new(),
            engine: engine.to_string(),
            host: host.to_string(),
            port: i64::from(port),
            database_name: database.to_string(),
            user_name: user.to_string(),
            secret_path: secret_path.display().to_string(),
            external: true,
        })?;
        self.view("attached an external connection; it has no process to stop")
    }

    pub fn detach(&self, project_id: &str, connection_id: &str) -> Result<Outcome> {
        Store::open(&self.data_dir)?.delete_db_binding(project_id, connection_id)?;
        self.view(&format!(
            "detached {connection_id}; no database process was stopped"
        ))
    }

    pub fn sqlite_use(&self, project_id: &str, path: &str) -> Result<Outcome> {
        let project = Store::open(&self.data_dir)?
            .get_project(project_id)?
            .ok_or_else(|| Error::ProjectNotFound {
                id: project_id.to_string(),
            })?;
        let file = contained_sqlite(Path::new(&project.root), Path::new(path))?;
        sqlite_init(&file)?;
        sqlite_write(&file, 1, "kept")?;
        Store::open(&self.data_dir)?.upsert_sqlite(&stackyard_core::SqliteRecord {
            project_id: project_id.to_string(),
            path: file.display().to_string(),
        })?;
        self.view("sqlite file is ready")
    }

    pub fn sqlite_backup(&self, project_id: &str, destination: &Path) -> Result<Outcome> {
        let record = self.sqlite(project_id)?;
        sqlite_backup(Path::new(&record.path), destination)?;
        self.view("sqlite backup written")
    }

    pub fn sqlite_restore(
        &self,
        project_id: &str,
        source: &Path,
        destination: &Path,
    ) -> Result<Outcome> {
        let _ = self.sqlite(project_id)?;
        sqlite_backup(source, destination)?;
        let note = sqlite_read(destination, 1)?.unwrap_or_default();
        if note != "kept" {
            return Err(Error::InvalidMessage(
                "restored sqlite file did not contain the saved row".to_string(),
            ));
        }
        self.view("sqlite restore verified")
    }

    fn probe_binding(&self, project_id: &str, name: &str) -> Result<String> {
        let binding = Store::open(&self.data_dir)?
            .list_db_bindings()?
            .into_iter()
            .find(|row| row.project_id == project_id && row.connection_id == name)
            .ok_or_else(|| {
                Error::InvalidMessage(format!("database {name} is not on {project_id}"))
            })?;
        if binding.external {
            return Err(Error::InvalidMessage(
                "an external connection is not probed or stopped by Stackyard".to_string(),
            ));
        }
        let instance = self.instance(&binding.instance_id)?;
        let password = read_secret(Path::new(&binding.secret_path))?;
        let server = self.server_binary(&instance.engine)?;
        let sql = if instance.engine == "postgres" {
            "CREATE TABLE IF NOT EXISTS stackyard_probe (id INT PRIMARY KEY, note TEXT); INSERT INTO stackyard_probe (id, note) VALUES (1, 'kept') ON CONFLICT (id) DO UPDATE SET note = 'kept'; SELECT note FROM stackyard_probe WHERE id = 1;"
        } else {
            "CREATE TABLE IF NOT EXISTS stackyard_probe (id INT PRIMARY KEY, note VARCHAR(40)); INSERT INTO stackyard_probe (id, note) VALUES (1, 'kept') ON DUPLICATE KEY UPDATE note = 'kept'; SELECT note FROM stackyard_probe WHERE id = 1;"
        };
        let output = if instance.engine == "mysql" {
            mysql_sql(
                &server,
                &instance,
                &binding.user_name,
                &password,
                &binding.database_name,
                sql,
            )?
        } else {
            postgres_sql(
                &server,
                &instance,
                &binding.user_name,
                &password,
                &binding.database_name,
                sql,
            )?
        };
        if !output.contains("kept") {
            return Err(Error::InvalidMessage(format!(
                "database probe did not return the saved row: {}",
                redact(&output, &password)
            )));
        }
        Ok("kept".to_string())
    }

    fn start_record(&self, record: &mut stackyard_core::DbInstanceRecord) -> Result<()> {
        if self
            .running
            .lock()
            .expect("databases")
            .iter()
            .any(|(id, _)| id == &record.id)
        {
            return Ok(());
        }
        if let (Some(pid), Some(identity)) = (record.pid, record.identity.clone()) {
            if let Ok(pid) = u32::try_from(pid) {
                if group::process_identity(pid).as_deref() == Some(identity.as_str()) {
                    return Err(Error::InvalidMessage(format!(
                        "database {} is still running from a previous engine; stop it before starting another",
                        record.id
                    )));
                }
            }
        }
        let server = self.server_binary(&record.engine)?;
        let data = PathBuf::from(&record.data_dir);
        let log = self
            .data_dir
            .join("logs")
            .join(format!("{}.log", record.id));
        if let Some(parent) = log.parent() {
            fs::create_dir_all(parent)
                .map_err(|source| Error::io("create database log dir", source))?;
        }
        let args;
        let program;
        if record.engine == "mysql" {
            program = server.clone();
            let basedir = mysql_basedir(&server);
            args = vec![
                "--no-defaults".to_string(),
                format!("--datadir={}", data.display()),
                format!("--basedir={}", basedir.display()),
                "--bind-address=127.0.0.1".to_string(),
                format!("--port={}", record.port),
                "--mysqlx=0".to_string(),
                format!(
                    "--socket={}",
                    data.parent().unwrap_or(&data).join("mysql.sock").display()
                ),
                format!(
                    "--pid-file={}",
                    data.parent().unwrap_or(&data).join("mysql.pid").display()
                ),
            ];
        } else {
            program = server.clone();
            args = vec!["-D".to_string(), data.display().to_string()];
        }
        let cwd = program.parent().unwrap_or(Path::new(".")).to_path_buf();
        let mut process = group::spawn_grouped(&program, &args, &cwd, &[], &log)?;
        let port = u16::try_from(record.port).unwrap_or(0);
        let started = Instant::now();
        let mut open = false;
        while started.elapsed() < Duration::from_secs(30) {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                open = true;
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        if !open {
            process.request_stop();
            let _ = process.wait_timeout(Duration::from_secs(3));
            let tail = fs::read_to_string(&log).unwrap_or_default();
            return Err(Error::InvalidMessage(format!(
                "database {} did not listen on 127.0.0.1:{port}. {}",
                record.id,
                tail.chars()
                    .rev()
                    .take(800)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>()
            )));
        }
        record.state = "running".to_string();
        record.pid = Some(i64::from(process.pid));
        record.identity = Some(process.identity.clone());
        Store::open(&self.data_dir)?.upsert_db_instance(record)?;
        self.running
            .lock()
            .expect("databases")
            .push((record.id.clone(), process));
        Ok(())
    }

    fn stop_record(&self, record: &mut stackyard_core::DbInstanceRecord) -> Result<()> {
        let process = {
            let mut running = self.running.lock().expect("databases");
            let index = running.iter().position(|(id, _)| id == &record.id);
            index.map(|index| running.remove(index).1)
        };
        if let Some(mut process) = process {
            process.request_stop();
            let _ = process.wait_timeout(Duration::from_secs(8));
        } else if let (Some(pid), Some(identity)) = (record.pid, record.identity.clone()) {
            if let Ok(pid) = u32::try_from(pid) {
                group::stop_recorded(pid, pid, &identity);
            }
        }
        record.state = "stopped".to_string();
        record.pid = None;
        record.identity = None;
        Store::open(&self.data_dir)?.upsert_db_instance(record)?;
        Ok(())
    }

    fn instance(&self, id: &str) -> Result<stackyard_core::DbInstanceRecord> {
        Store::open(&self.data_dir)?
            .get_db_instance(id)?
            .ok_or_else(|| Error::InvalidMessage(format!("database {id} was not found")))
    }

    fn sqlite(&self, project_id: &str) -> Result<stackyard_core::SqliteRecord> {
        Store::open(&self.data_dir)?
            .get_sqlite(project_id)?
            .ok_or_else(|| {
                Error::InvalidMessage(format!("project {project_id} has no sqlite file"))
            })
    }

    fn server_binary(&self, engine: &str) -> Result<PathBuf> {
        let store = Store::open(&self.data_dir)?;
        let Some(id) = store.get_selection(stackyard_core::DEFAULT_PROJECT, engine)? else {
            return Err(Error::InvalidMessage(format!(
                "no {engine} server is selected. Record the existing server binary first"
            )));
        };
        let package = store
            .get_package(&id)?
            .ok_or_else(|| Error::PackageNotFound { id })?;
        let path = PathBuf::from(package.launch);
        if !path.is_file() {
            return Err(Error::ProbeFailed {
                path,
                detail: "database server executable is missing".to_string(),
            });
        }
        Ok(path)
    }

    fn view(&self, message: &str) -> Result<Outcome> {
        let store = Store::open(&self.data_dir)?;
        let mut instances = store.list_db_instances()?;
        for instance in &mut instances {
            instance.secret_path = "[redacted]".to_string();
        }
        let mut bindings = store.list_db_bindings()?;
        for binding in &mut bindings {
            binding.secret_path = "[redacted]".to_string();
        }
        let mut sqlite = Vec::new();
        for project in store.list_projects()? {
            if let Some(record) = store.get_sqlite(&project.id)? {
                sqlite.push(record);
            }
        }
        Ok(Outcome::Databases {
            view: stackyard_core::DbView {
                instances,
                bindings,
                sqlite,
                message: message.to_string(),
            },
        })
    }
}

fn init_mysql(server: &Path, data: &Path) -> Result<()> {
    let basedir = mysql_basedir(server);
    let output = Command::new(server)
        .arg("--no-defaults")
        .arg("--initialize-insecure")
        .arg(format!("--datadir={}", data.display()))
        .arg(format!("--basedir={}", basedir.display()))
        .output()
        .map_err(|source| Error::io("initialize mysql", source))?;
    command_ok("mysqld --initialize-insecure", &output, "")?;
    Ok(())
}

fn bootstrap_mysql(
    server: &Path,
    instance: &stackyard_core::DbInstanceRecord,
    password: &str,
) -> Result<()> {
    let sql = format!(
        "ALTER USER 'root'@'localhost' IDENTIFIED BY '{password}'; CREATE USER IF NOT EXISTS 'root'@'127.0.0.1' IDENTIFIED BY '{password}'; GRANT ALL ON *.* TO 'root'@'127.0.0.1' WITH GRANT OPTION; FLUSH PRIVILEGES;"
    );
    let client = tool_near(server, &["mysql", "mysql.exe"])?;
    let mut command = Command::new(client);
    command
        .arg("--no-defaults")
        .arg("--protocol=tcp")
        .arg("-h")
        .arg("127.0.0.1")
        .arg("-P")
        .arg(instance.port.to_string())
        .arg("-u")
        .arg("root");
    let output = command_with_stdin(command, &sql)?;
    command_ok("mysql bootstrap", &output, password)?;
    Ok(())
}

fn init_postgres(server: &Path, data: &Path, secret: &Path, port: u16) -> Result<()> {
    let initdb = tool_near(server, &["initdb", "initdb.exe"])?;
    if let Some(parent) = data.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::io("create postgres parent", source))?;
    }
    let output = Command::new(initdb)
        .arg("--no-locale")
        .arg("--encoding=UTF8")
        .arg("-D")
        .arg(data)
        .arg("-U")
        .arg("stackyard")
        .arg(format!("--pwfile={}", secret.display()))
        .arg("--auth-host=scram-sha-256")
        .arg("--auth-local=scram-sha-256")
        .output()
        .map_err(|source| Error::io("initdb", source))?;
    command_ok("initdb", &output, "")?;
    let socket = data.parent().unwrap_or(data);
    let mut conf = fs::OpenOptions::new()
        .append(true)
        .open(data.join("postgresql.conf"))
        .map_err(|source| Error::io("open postgresql.conf", source))?;
    writeln!(
        conf,
        "\nlisten_addresses = '127.0.0.1'\nport = {port}\nunix_socket_directories = '{}'\n",
        socket.display()
    )
    .map_err(|source| Error::io("write postgresql.conf", source))?;
    Ok(())
}

fn mysql_sql(
    server: &Path,
    instance: &stackyard_core::DbInstanceRecord,
    user: &str,
    password: &str,
    database: &str,
    sql: &str,
) -> Result<String> {
    let defaults = mysql_defaults(instance, user, password)?;
    let mut command = Command::new(tool_near(server, &["mysql", "mysql.exe"])?);
    command
        .arg(format!("--defaults-file={}", defaults.display()))
        .arg("--protocol=tcp")
        .arg("--batch")
        .arg(database);
    let output = command_with_stdin(command, sql)?;
    let text = command_ok("mysql", &output, password)?;
    Ok(text)
}

fn postgres_sql(
    server: &Path,
    instance: &stackyard_core::DbInstanceRecord,
    user: &str,
    password: &str,
    database: &str,
    sql: &str,
) -> Result<String> {
    let pass = pgpass(instance, user, password)?;
    let mut command = Command::new(tool_near(server, &["psql", "psql.exe"])?);
    command
        .env("PGPASSFILE", &pass)
        .arg("-h")
        .arg("127.0.0.1")
        .arg("-p")
        .arg(instance.port.to_string())
        .arg("-U")
        .arg(user)
        .arg("-d")
        .arg(database)
        .arg("-v")
        .arg("ON_ERROR_STOP=1");
    let output = command_with_stdin(command, sql)?;
    command_ok("psql", &output, password)
}

fn mysql_defaults(
    instance: &stackyard_core::DbInstanceRecord,
    user: &str,
    password: &str,
) -> Result<PathBuf> {
    let path = PathBuf::from(&instance.secret_path).with_extension(format!("client-{user}"));
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
        .map_err(|source| Error::io("create mysql defaults", source))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
    }
    writeln!(
        file,
        "[client]\nprotocol=tcp\nhost=127.0.0.1\nport={}\nuser={user}\npassword={password}\n",
        instance.port
    )
    .map_err(|source| Error::io("write mysql defaults", source))?;
    Ok(path)
}

fn pgpass(
    instance: &stackyard_core::DbInstanceRecord,
    user: &str,
    password: &str,
) -> Result<PathBuf> {
    let path = PathBuf::from(&instance.secret_path).with_extension(format!("pgpass-{user}"));
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
        .map_err(|source| Error::io("create pgpass", source))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
    }
    writeln!(file, "127.0.0.1:{}:*:{user}:{password}", instance.port)
        .map_err(|source| Error::io("write pgpass", source))?;
    Ok(path)
}

fn mysql_basedir(server: &Path) -> PathBuf {
    server
        .parent()
        .and_then(|bin| bin.parent())
        .unwrap_or(Path::new("/usr"))
        .to_path_buf()
}

fn tool_near(server: &Path, names: &[&str]) -> Result<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = server.parent() {
        dirs.push(dir.to_path_buf());
        if let Some(parent) = dir.parent() {
            dirs.push(parent.join("bin"));
        }
    }
    dirs.push(PathBuf::from("/usr/bin"));
    dirs.push(PathBuf::from("/usr/sbin"));
    for dir in dirs {
        for name in names {
            let path = dir.join(name);
            if path.is_file() {
                return Ok(path);
            }
        }
    }
    Err(Error::InvalidMessage(format!(
        "could not find {} next to {}",
        names.join(" or "),
        server.display()
    )))
}

fn command_with_stdin(mut command: Command, sql: &str) -> Result<std::process::Output> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| Error::io("start database client", source))?;
    child
        .stdin
        .take()
        .ok_or_else(|| Error::InvalidMessage("database client had no stdin".to_string()))?
        .write_all(sql.as_bytes())
        .map_err(|source| Error::io("write database sql", source))?;
    child
        .wait_with_output()
        .map_err(|source| Error::io("wait for database client", source))
}

fn command_ok(name: &str, output: &std::process::Output, secret: &str) -> Result<String> {
    let mut text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    text = redact(&text, secret);
    if output.status.success() {
        Ok(text)
    } else {
        Err(Error::InvalidMessage(format!("{name} failed: {text}")))
    }
}

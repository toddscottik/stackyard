//! Redacted diagnostics export.
//!
//! The report lists projects, packages, and database instances. It omits
//! setting values and secret paths. Known secret file contents are replaced
//! before the report is returned.

use std::fs;
use std::path::Path;

use crate::catalog::current_target;
use crate::database::redact;
use crate::error::{Error, Result};
use crate::project::list_project_views;
use crate::store::Store;

pub const LIMITATIONS: &str = "\
Windows 10 x64 and macOS 14.5 arm64 are the acceptance machines. A report from another OS does not replace those checks.\n\
Database major versions are not migrated.\n\
External installations are not overwritten or removed.\n\
Application updates are not downloaded by Stackyard.\n\
Ports below 1024 are refused. The system trust store is not modified.\n\
MySQL has no SHA256 catalog entry. macOS Apache, MySQL, and PostgreSQL have no approved catalog archive.\n\
Passwords are files in the data directory, not Credential Manager or Keychain.\n\
Mailpit is not included.\n\
The user's own projects were not part of this export.";

pub fn export_diagnostics(data_dir: &Path) -> Result<String> {
    let store = Store::open(data_dir)?;
    let mut report = String::new();
    report.push_str("stackyard diagnostics\n");
    report.push_str(&format!("host: {}\n", current_target()));
    report.push_str(&format!("schema: {}\n", store.schema_version()?));
    report.push_str(&format!("data_dir: {}\n", data_dir.display()));

    report.push_str("\npackages:\n");
    let packages = store.list_packages()?;
    if packages.is_empty() {
        report.push_str("none\n");
    }
    for package in &packages {
        report.push_str(&format!(
            "{} {} {} {} {}\n",
            package.id, package.name, package.version, package.source, package.launch
        ));
    }

    report.push_str("\ncaches:\n");
    for instance in store.list_cache_instances()? {
        report.push_str(&format!(
            "{} {} 127.0.0.1:{} {}\n",
            instance.id, instance.package_id, instance.port, instance.state
        ));
    }
    report.push_str("\nprojects:\n");
    let projects = list_project_views(data_dir)?;
    if projects.is_empty() {
        report.push_str("none\n");
    }
    for project in &projects {
        report.push_str(&format!(
            "{} {} {} manager={}\n",
            project.id, project.name, project.root, project.manager
        ));
        for service in &project.services {
            report.push_str(&format!(
                "  service {} {} {}\n",
                service.id, service.state, service.detail
            ));
        }
        for process in store.list_processes(&project.id)? {
            report.push_str(&format!(
                "  process {} {} {}\n",
                process.service_id, process.state, process.detail
            ));
        }
    }

    report.push_str("\ndatabases:\n");
    let instances = store.list_db_instances()?;
    if instances.is_empty() {
        report.push_str("none\n");
    }
    for instance in &instances {
        report.push_str(&format!(
            "{} {} 127.0.0.1:{} {} {}\n",
            instance.id, instance.engine, instance.port, instance.state, instance.data_dir
        ));
    }
    for binding in store.list_db_bindings()? {
        report.push_str(&format!(
            "binding {} {} {} {}@127.0.0.1:{} external={}\n",
            binding.project_id,
            binding.database_name,
            binding.user_name,
            binding.engine,
            binding.port,
            binding.external
        ));
    }
    for project in &projects {
        if let Some(file) = store.get_sqlite(&project.id)? {
            report.push_str(&format!("sqlite {} {}\n", file.project_id, file.path));
        }
    }

    report.push_str("\nsetting_keys:\n");
    let mut keys = Vec::new();
    for key in ["display_name", "editor_command", "background_mode"] {
        if store.get(key)?.is_some() {
            keys.push(key);
        }
    }
    if keys.is_empty() {
        report.push_str("none\n");
    } else {
        report.push_str(&keys.join("\n"));
        report.push('\n');
    }

    report.push_str("\nlimitations:\n");
    report.push_str(LIMITATIONS);
    report.push('\n');

    let mut redacted = report;
    for secret in secret_bodies(data_dir)? {
        redacted = redact(&redacted, &secret);
    }
    Ok(scrub_private_keys(&redacted))
}

fn secret_bodies(data_dir: &Path) -> Result<Vec<String>> {
    let mut secrets = Vec::new();
    for dir in [data_dir.join("secrets"), data_dir.join("certs")] {
        if !dir.is_dir() {
            continue;
        }
        let entries =
            fs::read_dir(&dir).map_err(|source| Error::io("read secret directory", source))?;
        for entry in entries {
            let entry = entry.map_err(|source| Error::io("read secret entry", source))?;
            if !entry.path().is_file() {
                continue;
            }
            if let Ok(text) = fs::read_to_string(entry.path()) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    secrets.push(trimmed.to_string());
                }
            }
        }
    }
    Ok(secrets)
}

fn scrub_private_keys(text: &str) -> String {
    let mut output = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("-----BEGIN ") {
        let tail = &rest[start..];
        let private_key = tail.starts_with("-----BEGIN ") && tail.contains("PRIVATE KEY-----");
        if !private_key {
            output.push_str(&rest[..start + "-----BEGIN ".len()]);
            rest = &rest[start + "-----BEGIN ".len()..];
            continue;
        }
        let Some(end) = tail.find("-----END ") else {
            output.push_str(&rest[..start]);
            output.push_str("[redacted]");
            break;
        };
        let after = &tail[end..];
        let Some(line) = after.find('\n') else {
            output.push_str(&rest[..start]);
            output.push_str("[redacted]");
            break;
        };
        output.push_str(&rest[..start]);
        output.push_str("[redacted]");
        rest = &after[line + 1..];
    }
    output.push_str(rest);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::write_secret;
    use crate::paths::prepare_data_dir;
    use crate::project::import_project;
    use crate::store::{PackageRecord, ProcessRecord, Store};
    use std::fs;

    fn temp_dir() -> std::path::PathBuf {
        prepare_data_dir(&crate::paths::disposable_dir("diagnostics")).expect("temp")
    }

    #[test]
    fn export_redacts_secrets_and_keeps_a_unicode_project_path() {
        let dir = temp_dir();
        let secret = "super-secret-value";
        write_secret(&dir.join("secrets"), "db-alpha", secret).unwrap();
        let private_key = "-----BEGIN PRIVATE KEY-----\nQUIET\n-----END PRIVATE KEY-----";
        fs::create_dir_all(dir.join("certs")).unwrap();
        fs::write(dir.join("certs").join("leaf.key"), private_key).unwrap();

        let root = dir.join("projects").join("café");
        fs::create_dir_all(root.join("public")).unwrap();
        fs::write(
            root.join("stackyard.toml"),
            "schema = 1\nname = \"unicode-path\"\n\n[site]\nroot = \"public\"\n",
        )
        .unwrap();
        import_project(&dir, &root).unwrap();
        Store::open(&dir)
            .unwrap()
            .upsert_process(&ProcessRecord {
                project_id: "unicode-path".to_string(),
                service_id: "web".to_string(),
                state: "stopped".to_string(),
                pid: None,
                process_group: None,
                started_at: None,
                identity: None,
                port: None,
                exit_code: None,
                detail: format!("{secret}\n{private_key}"),
            })
            .unwrap();
        Store::open(&dir)
            .unwrap()
            .put("display_name", secret)
            .unwrap();

        let package_dir = dir.join("packages").join("node").join("1");
        fs::create_dir_all(&package_dir).unwrap();
        let launch = package_dir.join("node");
        fs::write(&launch, b"runtime").unwrap();
        Store::open(&dir)
            .unwrap()
            .insert_package(&PackageRecord {
                id: "node-1".to_string(),
                kind: "runtime".to_string(),
                name: "node".to_string(),
                version: "1".to_string(),
                target: "test".to_string(),
                source: "managed".to_string(),
                path: package_dir.display().to_string(),
                launch: launch.display().to_string(),
                digest: None,
                removable: true,
            })
            .unwrap();
        let marker = dir.join("databases").join("mysql-1").join("data");
        fs::create_dir_all(&marker).unwrap();
        fs::write(marker.join("marker.txt"), b"keep-data").unwrap();

        let report = export_diagnostics(&dir).unwrap();
        assert!(report.contains("unicode-path"), "{report}");
        assert!(report.contains("café"), "{report}");
        assert!(report.contains("node 1 managed"), "{report}");
        assert!(!report.contains(secret), "{report}");
        assert!(!report.contains("PRIVATE KEY"), "{report}");
        assert!(report.contains("[redacted]"), "{report}");
        assert!(report.contains("display_name"), "{report}");
        assert!(!report.contains(&format!("display_name: {secret}")));
        assert!(report.contains("Database major versions are not migrated."));

        crate::install::remove_package(&dir, "node-1").unwrap();
        assert!(!package_dir.exists());
        assert_eq!(fs::read(marker.join("marker.txt")).unwrap(), b"keep-data");
        let _ = fs::remove_dir_all(&dir);
    }
}

//! Catalog notices, side-by-side switching, and interrupted-install recovery.
//!
//! Refresh reads the bundled catalog only. It does not download a release feed.
//! Project dependency commands stay on `deps`. Application updates are a notice.

use std::fs;
use std::path::Path;

use crate::catalog::{current_target, load_catalog, CatalogFile, CatalogPackage};
use crate::error::{Error, Result};
use crate::store::{PackageRecord, Store};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UpdateNotice {
    pub kind: String,
    pub subject: String,
    pub detail: String,
}

pub fn refresh_updates(data_dir: &Path) -> Result<Vec<UpdateNotice>> {
    let store = Store::open(data_dir)?;
    let installed = store.list_packages()?;
    let catalog = load_catalog()?;
    let mut notices = plan_notices(&installed, &catalog, current_target());
    for package in &installed {
        if !Path::new(&package.launch).is_file() {
            notices.push(UpdateNotice {
                kind: "health".to_string(),
                subject: package.id.clone(),
                detail: format!(
                    "{} {} failed its health check because {} is missing. The selection was left unchanged. Reinstall from the catalog into a new directory; this does not replace an external file in place.",
                    package.name, package.version, package.launch
                ),
            });
        }
    }
    store.clear_generated_notices()?;
    for notice in &notices {
        store.insert_notice(&notice.kind, &notice.subject, &notice.detail)?;
    }
    store.list_notices()
}

pub fn record_external_version(
    data_dir: &Path,
    package_id: &str,
    observed: &str,
) -> Result<Vec<UpdateNotice>> {
    let store = Store::open(data_dir)?;
    let Some(package) = store.get_package(package_id)? else {
        return Err(Error::PackageNotFound {
            id: package_id.to_string(),
        });
    };
    if package.source != "existing" {
        return Err(Error::InvalidMessage(format!(
            "{package_id} is managed by Stackyard. An external note applies to a recorded installation."
        )));
    }
    let detail = if package.version == observed {
        format!(
            "external {} is still {}. Stackyard did not change {}.",
            package.name, package.version, package.launch
        )
    } else {
        format!(
            "external {} changed from {} to {observed} outside Stackyard. The recorded file was not overwritten. Compatibility limit: projects keep using {} until you select a different package.",
            package.name, package.version, package.launch
        )
    };
    store.replace_external_notice(&package.id, &detail)?;
    store.list_notices()
}

pub fn switch_project(data_dir: &Path, project_id: &str, package_id: &str) -> Result<()> {
    let store = Store::open(data_dir)?;
    let Some(package) = store.get_package(package_id)? else {
        return Err(Error::PackageNotFound {
            id: package_id.to_string(),
        });
    };
    let previous = store.get_selection(project_id, &package.name)?;
    if let Some(previous_id) = previous.as_deref() {
        if previous_id != package.id {
            if let Some(previous_package) = store.get_package(previous_id)? {
                if package.name == "mysql" || package.name == "postgres" {
                    if previous_package.version != package.version {
                        return Err(Error::DatabaseMigrationUnsupported);
                    }
                } else if !same_line(&package.name, &previous_package.version, &package.version)
                    && previous_package.version != package.version
                {
                    return Err(Error::InvalidMessage(format!(
                        "{} {} cannot replace {} {} for {project_id}. The current selection was kept.",
                        package.name, package.version, package.name, previous_package.version
                    )));
                }
            }
        }
    }
    store.select_package(project_id, &package.name, &package.id)?;
    store.record_selection_history(project_id, &package.name, &package.id, previous.as_deref())?;
    Ok(())
}

pub fn cleanup_unused(data_dir: &Path) -> Result<Vec<UpdateNotice>> {
    let ids: Vec<String> = {
        let store = Store::open(data_dir)?;
        store
            .list_packages()?
            .into_iter()
            .filter(|package| package.source == "managed" && package.removable)
            .map(|package| package.id)
            .collect()
    };
    let mut removed = Vec::new();
    for id in ids {
        match crate::install::remove_package(data_dir, &id) {
            Ok(()) => removed.push(id),
            Err(Error::NotRemovable { .. }) => {}
            Err(err) => return Err(err),
        }
    }
    let mut notices = refresh_updates(data_dir)?;
    let detail = if removed.is_empty() {
        "no unused package was removed. A package selected by a project stays on disk, including when that project is running.".to_string()
    } else {
        format!("removed unused packages: {}", removed.join(", "))
    };
    Store::open(data_dir)?.insert_notice("cleanup", "unused packages", &detail)?;
    notices.push(UpdateNotice {
        kind: "cleanup".to_string(),
        subject: "unused packages".to_string(),
        detail,
    });
    Ok(notices)
}

pub fn mark_install_running(data_dir: &Path, catalog_id: &str) -> Result<()> {
    Store::open(data_dir)?.mark_install(catalog_id, "running", "downloading")
}

pub fn mark_install_finished(
    data_dir: &Path,
    catalog_id: &str,
    state: &str,
    message: &str,
) -> Result<()> {
    Store::open(data_dir)?.mark_install(catalog_id, state, message)
}

pub fn reconcile_interrupted(data_dir: &Path) -> Result<Vec<String>> {
    let store = Store::open(data_dir)?;
    let running = store.running_installs()?;
    let incoming = data_dir.join("incoming");
    if incoming.is_dir() {
        for entry in fs::read_dir(&incoming)
            .map_err(|source| Error::io("read incoming directory", source))?
        {
            let entry = entry.map_err(|source| Error::io("read incoming entry", source))?;
            let _ = fs::remove_dir_all(entry.path());
        }
    }
    for catalog_id in &running {
        store.mark_install(
            catalog_id,
            "interrupted",
            "the install was interrupted and the partial download was removed. Installed packages were left in place.",
        )?;
        store.insert_notice(
            "interrupted",
            catalog_id,
            "the install was interrupted and the partial download was removed. Installed packages were left in place.",
        )?;
    }
    Ok(running)
}

pub fn plan_notices(
    installed: &[PackageRecord],
    catalog: &CatalogFile,
    host: &str,
) -> Vec<UpdateNotice> {
    let mut notices = vec![
        UpdateNotice {
            kind: "app".to_string(),
            subject: "stackyard".to_string(),
            detail: "Stackyard application updates are separate from runtime updates and are not downloaded by this command.".to_string(),
        },
        UpdateNotice {
            kind: "dependency".to_string(),
            subject: "project dependencies".to_string(),
            detail: "Project dependency updates stay on deps update for one project. Reinstalling from the lockfile is recovery, not a rollback of package scripts.".to_string(),
        },
    ];
    for item in &catalog.news {
        notices.push(UpdateNotice {
            kind: "news".to_string(),
            subject: item.id.clone(),
            detail: format!("{} {}", item.title, item.detail),
        });
    }
    for candidate in catalog
        .packages
        .iter()
        .filter(|package| package.target == host)
    {
        if let Some(notice) = notice_for_candidate(installed, candidate) {
            notices.push(notice);
        }
    }
    notices
}

fn notice_for_candidate(
    installed: &[PackageRecord],
    candidate: &CatalogPackage,
) -> Option<UpdateNotice> {
    let current = installed
        .iter()
        .filter(|package| package.name == candidate.name)
        .max_by(|left, right| numeric_parts(&left.version).cmp(&numeric_parts(&right.version)))?;
    if installed.iter().any(|package| package.id == candidate.id) {
        return None;
    }
    if candidate.name == "mysql" || candidate.name == "postgres" {
        return Some(UpdateNotice {
            kind: "database".to_string(),
            subject: candidate.id.clone(),
            detail: format!(
                "{} {} is in the catalog beside {}. Database versions are not switched by the runtime updater.",
                candidate.name, candidate.version, current.version
            ),
        });
    }
    if !newer(
        &numeric_parts(&candidate.version),
        &numeric_parts(&current.version),
    ) {
        return None;
    }
    if same_line(&candidate.name, &current.version, &candidate.version) {
        let owned = if current.source == "existing" {
            format!(
                " The existing {} at {} is not overwritten.",
                current.version, current.launch
            )
        } else {
            String::new()
        };
        return Some(UpdateNotice {
            kind: "installable".to_string(),
            subject: candidate.id.clone(),
            detail: format!(
                "{} {} can be installed beside {}.{owned} Switch one project after the install.",
                candidate.name, candidate.version, current.version
            ),
        });
    }
    Some(UpdateNotice {
        kind: "news".to_string(),
        subject: candidate.id.clone(),
        detail: format!(
            "{} {} is not a compatible update from {}. It is not installed or switched automatically.",
            candidate.name, candidate.version, current.version
        ),
    })
}

fn same_line(name: &str, installed: &str, candidate: &str) -> bool {
    let installed = numeric_parts(installed);
    let candidate = numeric_parts(candidate);
    if installed.is_empty() || candidate.is_empty() {
        return false;
    }
    match name {
        "node" | "pnpm" | "uv" => installed[0] == candidate[0],
        "python" | "apache" => {
            installed.len() >= 2
                && candidate.len() >= 2
                && installed[0] == candidate[0]
                && installed[1] == candidate[1]
        }
        _ => false,
    }
}

fn newer(candidate: &[u64], installed: &[u64]) -> bool {
    let len = candidate.len().max(installed.len());
    for index in 0..len {
        let next = candidate.get(index).copied().unwrap_or(0);
        let current = installed.get(index).copied().unwrap_or(0);
        if next != current {
            return next > current;
        }
    }
    false
}

fn numeric_parts(version: &str) -> Vec<u64> {
    let head = version.split(['+', '-', ' ']).next().unwrap_or(version);
    head.split('.')
        .map(|part| part.parse::<u64>())
        .take_while(|part| part.is_ok())
        .filter_map(|part| part.ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::prepare_data_dir;
    use crate::store::ProcessRecord;

    fn temp_dir() -> std::path::PathBuf {
        prepare_data_dir(&crate::paths::disposable_dir("updates")).expect("temp")
    }

    fn record(id: &str, name: &str, version: &str, source: &str) -> PackageRecord {
        PackageRecord {
            id: id.to_string(),
            kind: "runtime".to_string(),
            name: name.to_string(),
            version: version.to_string(),
            target: "windows-x64".to_string(),
            source: source.to_string(),
            path: format!("/pkg/{id}"),
            launch: format!("/pkg/{id}/launch"),
            digest: None,
            removable: source == "managed",
        }
    }

    fn candidate(id: &str, name: &str, version: &str) -> CatalogPackage {
        CatalogPackage {
            id: id.to_string(),
            kind: "runtime".to_string(),
            name: name.to_string(),
            version: version.to_string(),
            target: "windows-x64".to_string(),
            format: "zip".to_string(),
            url: "https://example.invalid/package.zip".to_string(),
            sha256: "ab".repeat(32),
            digest_source: "test".to_string(),
            launch: "launch".to_string(),
            version_args: vec!["-v".to_string()],
        }
    }

    #[test]
    fn news_is_not_an_install_and_a_compatible_node_is() {
        let catalog = CatalogFile {
            revision: 2,
            news: vec![crate::catalog::NewsItem {
                id: "upstream-releases-are-not-packages".to_string(),
                title: "Upstream release announcements are not installable packages".to_string(),
                detail: "not a download".to_string(),
            }],
            packages: vec![
                candidate("node-new", "node", "24.23.0"),
                candidate("node-major", "node", "25.0.0"),
                candidate("python-minor", "python", "3.13.0"),
                candidate("postgres-next", "postgres", "19.1"),
            ],
        };
        let installed = vec![
            record("node-old", "node", "24.22.2", "managed"),
            record("python-old", "python", "3.12.16", "existing"),
            record("postgres-old", "postgres", "18.6", "managed"),
        ];
        let notices = plan_notices(&installed, &catalog, "windows-x64");
        assert!(notices
            .iter()
            .any(|notice| notice.kind == "app" && notice.detail.contains("application updates")));
        assert!(notices
            .iter()
            .any(|notice| notice.kind == "dependency" && notice.detail.contains("deps update")));
        assert!(notices.iter().any(|notice| notice.kind == "news"
            && notice.subject == "upstream-releases-are-not-packages"));
        assert!(notices.iter().any(|notice| {
            notice.kind == "installable"
                && notice.subject == "node-new"
                && notice.detail.contains("beside")
        }));
        assert!(notices
            .iter()
            .any(|notice| notice.kind == "news" && notice.subject == "node-major"));
        assert!(notices
            .iter()
            .any(|notice| notice.kind == "news" && notice.subject == "python-minor"));
        assert!(notices
            .iter()
            .any(|notice| notice.kind == "database" && notice.subject == "postgres-next"));
        assert!(!notices
            .iter()
            .any(|notice| notice.detail.contains("https://")));
    }

    #[test]
    fn one_project_switches_and_the_other_keeps_its_runtime() {
        let dir = temp_dir();
        let old = record("node-24.22.2", "node", "24.22.2", "managed");
        let new = record("node-24.23.0", "node", "24.23.0", "managed");
        let store = Store::open(&dir).expect("store");
        store.insert_package(&old).expect("old");
        store.insert_package(&new).expect("new");
        switch_project(&dir, "alpha", &old.id).expect("alpha old");
        switch_project(&dir, "beta", &old.id).expect("beta old");
        switch_project(&dir, "alpha", &new.id).expect("alpha new");
        let store = Store::open(&dir).expect("reopen");
        assert_eq!(
            store.get_selection("alpha", "node").unwrap().as_deref(),
            Some(new.id.as_str())
        );
        assert_eq!(
            store.get_selection("beta", "node").unwrap().as_deref(),
            Some(old.id.as_str())
        );
        assert_eq!(
            store
                .previous_selection("alpha", "node")
                .unwrap()
                .as_deref(),
            Some(old.id.as_str())
        );
        assert!(store.get_package(&old.id).unwrap().is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn database_versions_do_not_switch_and_a_running_selection_stays() {
        let dir = temp_dir();
        let store = Store::open(&dir).expect("store");
        let current = record("postgres-18", "postgres", "18.6", "managed");
        let next = record("postgres-19", "postgres", "19.1", "managed");
        store.insert_package(&current).expect("current");
        store.insert_package(&next).expect("next");
        switch_project(&dir, "alpha", &current.id).expect("select");
        let refused = switch_project(&dir, "alpha", &next.id).expect_err("major");
        assert!(matches!(refused, Error::DatabaseMigrationUnsupported));
        assert_eq!(
            Store::open(&dir)
                .unwrap()
                .get_selection("alpha", "postgres")
                .unwrap()
                .as_deref(),
            Some(current.id.as_str())
        );

        let node = record("node-kept", "node", "24.22.2", "managed");
        Store::open(&dir).unwrap().insert_package(&node).unwrap();
        switch_project(&dir, "alpha", &node.id).unwrap();
        Store::open(&dir)
            .unwrap()
            .upsert_process(&ProcessRecord {
                project_id: "alpha".to_string(),
                service_id: "web".to_string(),
                state: "running".to_string(),
                pid: Some(42),
                process_group: Some(42),
                started_at: Some(1),
                identity: None,
                port: Some(3000),
                exit_code: None,
                detail: String::new(),
            })
            .unwrap();
        let blocked = crate::install::remove_package(&dir, &node.id).expect_err("running");
        let Error::NotRemovable { reason, .. } = blocked else {
            panic!("expected not removable, got {blocked:?}");
        };
        assert!(reason.contains("running"));
        assert!(Store::open(&dir)
            .unwrap()
            .get_package(&node.id)
            .unwrap()
            .is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_external_change_is_recorded_and_the_file_stays() {
        let dir = temp_dir();
        let marker = dir.join("python.txt");
        fs::write(&marker, b"original").unwrap();
        let package = PackageRecord {
            launch: marker.display().to_string(),
            path: marker.display().to_string(),
            ..record("existing-python", "python", "3.12.3", "existing")
        };
        Store::open(&dir).unwrap().insert_package(&package).unwrap();
        fs::write(&marker, b"changed-outside").unwrap();
        let notices = record_external_version(&dir, &package.id, "3.12.4").unwrap();
        assert!(notices
            .iter()
            .any(|notice| notice.kind == "external" && notice.detail.contains("not overwritten")));
        assert_eq!(fs::read(&marker).unwrap(), b"changed-outside");
        assert_eq!(
            Store::open(&dir)
                .unwrap()
                .get_package(&package.id)
                .unwrap()
                .unwrap()
                .version,
            "3.12.3"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_interrupted_install_removes_only_the_partial_download() {
        let dir = temp_dir();
        let kept = dir.join("packages").join("node").join("24.22.2");
        fs::create_dir_all(&kept).unwrap();
        fs::write(kept.join("node"), b"kept").unwrap();
        let partial = dir.join("incoming").join("node-24.23.0");
        fs::create_dir_all(&partial).unwrap();
        fs::write(partial.join("payload"), b"partial").unwrap();
        Store::open(&dir)
            .unwrap()
            .mark_install("node-24.23.0", "running", "downloading")
            .unwrap();
        let interrupted = reconcile_interrupted(&dir).unwrap();
        assert_eq!(interrupted, vec!["node-24.23.0".to_string()]);
        assert!(!partial.exists());
        assert_eq!(fs::read(kept.join("node")).unwrap(), b"kept");
        let notices = Store::open(&dir).unwrap().list_notices().unwrap();
        assert!(notices.iter().any(|notice| notice.kind == "interrupted"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_launch_file_keeps_the_selection() {
        let dir = temp_dir();
        let package = record("node-missing", "node", "24.22.2", "managed");
        let store = Store::open(&dir).unwrap();
        store.insert_package(&package).unwrap();
        store.select_package("alpha", "node", &package.id).unwrap();
        let notices = refresh_updates(&dir).unwrap();
        assert!(notices
            .iter()
            .any(|notice| notice.kind == "health" && notice.subject == package.id));
        assert_eq!(
            Store::open(&dir)
                .unwrap()
                .get_selection("alpha", "node")
                .unwrap()
                .as_deref(),
            Some(package.id.as_str())
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

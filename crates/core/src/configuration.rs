//! Read-only import/configuration review and checked publication of stackyard.toml.
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use toml_edit::{value, Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};

use crate::create::{canonical_parent, file_identity, is_redirect, open_directory, publish};
use crate::detect::{detect_project, ImportEvidence};
use crate::manifest::{Manifest, SiteKind};
use crate::preview::{self, Diagnostic, PreviewRequest, ProjectPreview};
use crate::{Error, ProjectRecord, ProjectView, Result, Store};

const MANIFEST: &str = "stackyard.toml";
const MAX_TEXT: usize = 24 * 1024;
const MAX_ROWS: usize = 16;
const MAX_RESPONSE: usize = crate::protocol::MAX_MESSAGE_BYTES - 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigScope {
    pub id: String,
    pub directory: String,
    pub runtime: String,
    pub manager: String,
    pub lockfile_root: String,
    pub environment: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigService {
    pub id: String,
    pub scope: String,
    pub command: Vec<String>,
    pub working_directory: String,
    pub protocol: String,
    pub port: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigSite {
    pub host: String,
    pub handler: String,
    pub service: String,
    pub root: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigForm {
    pub name: String,
    pub scopes: Vec<ConfigScope>,
    pub services: Vec<ConfigService>,
    pub site: Option<ConfigSite>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigInspection {
    pub root: String,
    pub project_id: Option<String>,
    pub revision: String,
    pub existing: bool,
    pub form: Option<ConfigForm>,
    pub evidence: Vec<ImportEvidence>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigDraft {
    pub root: String,
    pub revision: String,
    pub form: Option<ConfigForm>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigReview {
    pub fingerprint: String,
    pub manifest_text: String,
    pub preview: ProjectPreview,
}

struct Snapshot {
    root: PathBuf,
    anchor: File,
    inspection: ConfigInspection,
    base: String,
}

pub fn inspect_config(store: &Store, path: &Path) -> Result<ConfigInspection> {
    Ok(snapshot(store, path)?.inspection)
}

pub fn registered_project(store: &Store, path: &Path) -> Result<Option<ProjectRecord>> {
    let root = canonical_parent(path)?;
    store.project_by_root(&root_text(root)?)
}

pub fn preview_config(store: &Store, draft: &ConfigDraft) -> Result<ConfigReview> {
    let snapshot = snapshot(store, Path::new(&draft.root))?;
    review(store, &snapshot, draft)
}

pub fn apply_config(
    data: &Path,
    store: &Store,
    draft: &ConfigDraft,
    fingerprint: &str,
) -> Result<ProjectView> {
    let snapshot = snapshot(store, Path::new(&draft.root))?;
    let reviewed = review(store, &snapshot, draft)?;
    if fingerprint != reviewed.fingerprint || !reviewed.preview.can_apply {
        return Err(failure(
            "Configuration changed or is invalid. Inspect and preview it again.",
        ));
    }
    if snapshot.root.starts_with(data) || data.starts_with(&snapshot.root) {
        return Err(failure(
            "The project folder overlaps the engine data directory.",
        ));
    }
    let manifest = crate::parse_manifest(&reviewed.manifest_text)?;
    if reviewed.manifest_text != snapshot.base || !snapshot.inspection.existing {
        write_reviewed(store, &snapshot, reviewed.manifest_text.as_bytes())?;
    }
    register(data, store, &snapshot, &manifest)
}

pub fn reload_config(data: &Path, store: &Store, project_id: &str) -> Result<ProjectView> {
    let project = store
        .get_project(project_id)?
        .ok_or_else(|| Error::ProjectNotFound {
            id: project_id.into(),
        })?;
    let snapshot = snapshot(store, Path::new(&project.root))?;
    if !snapshot.inspection.existing {
        return Err(failure(
            "stackyard.toml is missing. Restore it in the project folder before reloading.",
        ));
    }
    let manifest = crate::parse_manifest(&snapshot.base)?;
    let checked = validate(store, &snapshot, &snapshot.base)?;
    if let Some(problem) = checked
        .diagnostics
        .iter()
        .find(|item| item.severity == "error")
    {
        return Err(failure(&problem.message));
    }
    register(data, store, &snapshot, &manifest)
}

fn snapshot(store: &Store, path: &Path) -> Result<Snapshot> {
    let detection = detect_project(path)?;
    let root = canonical_parent(Path::new(&detection.root))?;
    let anchor = open_directory(&root)?;
    let root_text = root_text(root.clone())?;
    let project = store.project_by_root(&root_text)?;
    let mut digest = Sha256::new();
    digest_input(&mut digest, b"stackyard-config-v1");
    digest_input(&mut digest, root_text.as_bytes());
    digest_input(&mut digest, file_identity(&anchor)?.as_bytes());
    if let Some(project) = &project {
        digest_input(&mut digest, project.id.as_bytes());
    }
    let (base, form, evidence, diagnostics, existing) = match read_manifest(&root)? {
        Some((text, identity)) => {
            digest_input(&mut digest, b"existing");
            digest_input(&mut digest, identity.as_bytes());
            let (form, diagnostics) = match crate::parse_manifest(&text) {
                Ok(manifest) if manifest.schema == 2 => match editable_form(&manifest, &text) {
                    Some(form) => (Some(form), Vec::new()),
                    None => (None, vec![notice("editor_required", "Use your editor for this configuration format.")]),
                },
                Ok(_) => (None, vec![notice("editor_required", "Legacy manifests can be imported as written. Use your editor to change their configuration.")]),
                Err(error) => (None, vec![Diagnostic::error("manifest_invalid", MANIFEST, error.to_string())]),
            };
            (text, form, Vec::new(), diagnostics, true)
        }
        None => {
            digest_input(&mut digest, b"missing");
            if project.is_some() {
                return Err(failure(
                    "This project is missing stackyard.toml. Restore it in the project folder.",
                ));
            }
            if detection.has_manifest {
                return Err(stale());
            }
            let text = detection.manifest_text;
            let form = crate::parse_manifest(&text)
                .ok()
                .and_then(|manifest| editable_form(&manifest, &text))
                .unwrap_or_else(|| ConfigForm {
                    name: suggested_name(&root),
                    scopes: Vec::new(),
                    services: Vec::new(),
                    site: None,
                });
            (
                text,
                Some(form),
                detection.evidence,
                detection.diagnostics,
                false,
            )
        }
    };
    digest_input(&mut digest, base.as_bytes());
    let mut inspection = ConfigInspection {
        root: root_text,
        project_id: project.map(|item| item.id),
        revision: hex::encode(digest.finalize()),
        existing,
        form,
        evidence,
        diagnostics,
    };
    if wire_size(&inspection)? > MAX_RESPONSE {
        if !inspection.existing {
            return Err(failure(
                "The detection result is too large to review in the form. Use your editor.",
            ));
        }
        inspection.form = None;
        inspection.diagnostics.push(notice(
            "editor_required",
            "Use your editor for a configuration of this size.",
        ));
    }
    Ok(Snapshot {
        root,
        anchor,
        inspection,
        base,
    })
}

fn review(store: &Store, snapshot: &Snapshot, draft: &ConfigDraft) -> Result<ConfigReview> {
    if draft.revision != snapshot.inspection.revision || draft.root != snapshot.inspection.root {
        return Err(stale());
    }
    let text = match (&snapshot.inspection.form, &draft.form) {
        (Some(base), Some(form)) => render(snapshot, base, form)?,
        (None, None) if snapshot.inspection.existing => snapshot.base.clone(),
        _ => {
            return Err(failure(
                "This configuration needs an editable form or its existing manifest.",
            ))
        }
    };
    if text.len() > MAX_TEXT {
        return Err(failure(
            "Configuration exceeds the 24 KiB review limit. Use your editor.",
        ));
    }
    let preview = validate(store, snapshot, &text)?;
    let mut digest = Sha256::new();
    digest_input(&mut digest, draft.revision.as_bytes());
    digest_input(&mut digest, text.as_bytes());
    let review = ConfigReview {
        fingerprint: hex::encode(digest.finalize()),
        manifest_text: text,
        preview,
    };
    if wire_size(&review)? > MAX_RESPONSE {
        return Err(failure(
            "This preview is too large for the form. Use your editor.",
        ));
    }
    Ok(review)
}

fn validate(store: &Store, snapshot: &Snapshot, text: &str) -> Result<ProjectPreview> {
    let manifest = match crate::parse_manifest(text) {
        Ok(manifest) => manifest,
        Err(error) => {
            return Ok(ProjectPreview {
                can_apply: false,
                scopes: Vec::new(),
                services: Vec::new(),
                diagnostics: vec![Diagnostic::error(
                    "manifest_invalid",
                    MANIFEST,
                    error.to_string(),
                )],
            })
        }
    };
    let id = snapshot
        .inspection
        .project_id
        .as_deref()
        .unwrap_or(&manifest.name);
    let guarded = validate_paths(&snapshot.root, &manifest)
        .and_then(|()| store.validate_project_root_id(id, &snapshot.inspection.root))
        .and_then(|()| {
            if snapshot.inspection.project_id.is_none() {
                if id == crate::DEFAULT_PROJECT
                    && !(snapshot.inspection.existing && manifest.schema == 1)
                {
                    return Err(failure(
                        "The project name default is reserved. Choose a different name.",
                    ));
                }
                if store.get_project(id)?.is_some() {
                    return Err(failure(
                        "This project name is already registered. Choose a different name.",
                    ));
                }
            }
            Ok(())
        })
        .and_then(|()| crate::scopes::validate_scope_ownership(&snapshot.root, &manifest))
        .and_then(|()| validate_bindings(store, snapshot, &manifest));
    if let Err(error) = guarded {
        return Ok(ProjectPreview {
            can_apply: false,
            scopes: Vec::new(),
            services: Vec::new(),
            diagnostics: vec![Diagnostic::error(
                "configuration_invalid",
                MANIFEST,
                error.to_string(),
            )],
        });
    }
    let mut checked = preview::preview_request(
        &PreviewRequest {
            root: snapshot.inspection.root.clone(),
            manifest_text: text.into(),
            project_id: snapshot.inspection.project_id.clone(),
            selection_overrides: Vec::new(),
        },
        Some(store),
    )?;
    for item in &mut checked.diagnostics {
        if matches!(item.code.as_str(), "runtime_missing" | "tool_missing") {
            item.severity = "warning".into();
        }
    }
    checked.can_apply = !checked
        .diagnostics
        .iter()
        .any(|item| item.severity == "error");
    Ok(checked)
}

fn validate_bindings(store: &Store, snapshot: &Snapshot, manifest: &Manifest) -> Result<()> {
    if let Some(id) = snapshot.inspection.project_id.as_deref() {
        if let Ok(original) = crate::parse_manifest(&snapshot.base) {
            for scope in &original.scopes {
                if manifest.scope(&scope.id) != Some(scope)
                    && store.get_scope_environment(id, &scope.id)?.is_some()
                {
                    return Err(failure("This scope has a recorded Python environment. Keep its ownership settings or add a new scope."));
                }
            }
        }
        for binding in store.scope_selections(id)? {
            let scope = manifest.scope(&binding.scope_id);
            if !scope.is_some_and(|scope| {
                binding.role == scope.runtime.as_str()
                    || matches!(
                        (scope.runtime.as_str(), binding.role.as_str()),
                        ("node", "pnpm") | ("python", "uv") | ("php", "composer")
                    )
            }) {
                return Err(failure("A recorded component selection would lose its scope or change runtime. Restore that scope or forget the stopped project before reimporting."));
            }
        }
    }
    Ok(())
}

fn validate_paths(root: &Path, manifest: &Manifest) -> Result<()> {
    let mut owners = BTreeSet::new();
    for scope in &manifest.scopes {
        ordinary_directory(root, &scope.directory)?;
        owners.insert(ordinary_directory(root, &scope.lockfile_root)?);
    }
    for service in &manifest.services {
        ordinary_directory(root, &service.working_directory)?;
    }
    if let Some(site) = &manifest.site {
        if let SiteKind::Static { root: relative } | SiteKind::Php { root: relative, .. } =
            &site.kind
        {
            ordinary_directory(root, Path::new(relative))?;
        }
    }
    for owner in owners {
        for name in [
            "package.json",
            "package-lock.json",
            "npm-shrinkwrap.json",
            "pnpm-lock.yaml",
            "yarn.lock",
            "bun.lock",
            "bun.lockb",
            "requirements.txt",
            "pyproject.toml",
            "uv.lock",
            "Pipfile",
            "Pipfile.lock",
            "poetry.lock",
            "composer.json",
            "composer.lock",
        ] {
            let path = owner.join(name);
            if let Some(meta) = metadata(&path)? {
                if is_redirect(&meta)
                    || !meta.is_file()
                    || (name == "package.json" && meta.len() > 64 * 1024)
                {
                    return Err(failure(&format!(
                        "{} must be an ordinary bounded dependency metadata file.",
                        path.display()
                    )));
                }
            }
        }
    }
    Ok(())
}

fn ordinary_directory(root: &Path, relative: &Path) -> Result<PathBuf> {
    let mut path = root.to_path_buf();
    for component in relative.components() {
        match component {
            Component::CurDir => continue,
            Component::Normal(name) => path.push(name),
            _ => {
                return Err(failure(
                    "Configuration paths must stay inside the selected folder.",
                ))
            }
        }
        let meta = fs::symlink_metadata(&path)
            .map_err(|source| Error::io("inspect configuration directory", source))?;
        if !meta.is_dir() || is_redirect(&meta) {
            return Err(failure(&format!(
                "{} is not an ordinary directory. Choose a path without symlinks or junctions.",
                path.display()
            )));
        }
    }
    Ok(path)
}

fn register(
    data: &Path,
    store: &Store,
    snapshot: &Snapshot,
    manifest: &Manifest,
) -> Result<ProjectView> {
    let id = snapshot
        .inspection
        .project_id
        .as_deref()
        .unwrap_or(&manifest.name);
    if snapshot.inspection.project_id.is_some() {
        store.rename_project(id, &manifest.name)?;
        store.record_event("project_configuration", id)?;
    } else {
        store.insert_project(&ProjectRecord {
            id: id.into(),
            name: manifest.name.clone(),
            root: snapshot.inspection.root.clone(),
        })?;
        store.record_event("project_import", id)?;
    }
    crate::project_view(data, id)
}

fn read_manifest(root: &Path) -> Result<Option<(String, String)>> {
    let path = root.join(MANIFEST);
    let Some(meta) = metadata(&path)? else {
        return Ok(None);
    };
    if !meta.is_file() || is_redirect(&meta) {
        return Err(failure(
            "stackyard.toml must be an ordinary file. Symlinks and junctions are refused.",
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|source| Error::io("open stackyard.toml", source))?;
    let opened = file
        .metadata()
        .map_err(|source| Error::io("inspect stackyard.toml", source))?;
    if !opened.is_file() || is_redirect(&opened) {
        return Err(stale());
    }
    let identity = file_identity(&file)?;
    let mut bytes = Vec::new();
    file.take((MAX_TEXT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|source| Error::io("read stackyard.toml", source))?;
    if bytes.len() > MAX_TEXT {
        return Err(failure(
            "stackyard.toml exceeds the 24 KiB review limit. Use your editor.",
        ));
    }
    let text = String::from_utf8(bytes).map_err(|_| failure("stackyard.toml must be UTF-8."))?;
    Ok(Some((text, identity)))
}

fn write_reviewed(store: &Store, original: &Snapshot, bytes: &[u8]) -> Result<()> {
    let mut nonce = [0u8; 12];
    getrandom::getrandom(&mut nonce)
        .map_err(|error| failure(&format!("Cannot name staged configuration: {error}")))?;
    let path = original
        .root
        .join(format!(".stackyard-config-{}.tmp", hex::encode(nonce)));
    let target = original.root.join(MANIFEST);
    let mut stage = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .map_err(|source| Error::io("stage stackyard.toml", source))?;
    let result = (|| {
        stage
            .write_all(bytes)
            .and_then(|()| stage.sync_all())
            .map_err(|source| Error::io("sync staged stackyard.toml", source))?;
        drop(stage);
        let current = snapshot(store, &original.root)?;
        if current.inspection.revision != original.inspection.revision {
            return Err(stale());
        }
        if current.inspection.existing {
            let permissions = fs::symlink_metadata(&target)
                .map_err(|source| Error::io("read manifest permissions", source))?
                .permissions();
            fs::set_permissions(&path, permissions)
                .map_err(|source| Error::io("preserve manifest permissions", source))?;
            fs::rename(&path, &target)
        } else {
            publish(&original.anchor, &path, &target)
        }
        .map_err(|source| Error::io("publish reviewed stackyard.toml", source))
    })();
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}

fn metadata(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(Some(meta)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Error::io("inspect configuration path", error)),
    }
}

fn root_text(path: PathBuf) -> Result<String> {
    crate::plain_path(path)
        .into_os_string()
        .into_string()
        .map_err(|_| failure("The project path must be valid UTF-8."))
}

fn digest_input(digest: &mut Sha256, bytes: &[u8]) {
    digest.update((bytes.len() as u64).to_le_bytes());
    digest.update(bytes);
}

fn wire_size(value: &impl Serialize) -> Result<usize> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .map_err(|error| failure(&format!("Could not encode configuration response: {error}")))
}

fn notice(code: &str, message: &str) -> Diagnostic {
    let mut diagnostic = Diagnostic::error(code, MANIFEST, message);
    diagnostic.severity = "info".into();
    diagnostic
}

fn stale() -> Error {
    failure("The project or manifest changed after review. Inspect it again before saving.")
}

fn failure(message: &str) -> Error {
    Error::OperationFailed(message.into())
}

fn suggested_name(root: &Path) -> String {
    let name: String = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .chars()
        .map(|ch| match ch.to_ascii_lowercase() {
            ch if ch.is_ascii_alphanumeric() || ch == '-' => ch,
            _ => '-',
        })
        .take(48)
        .collect();
    let name = name.trim_matches('-');
    if !name.starts_with(|ch: char| ch.is_ascii_lowercase()) || name == crate::DEFAULT_PROJECT {
        "imported-app".into()
    } else {
        name.into()
    }
}

fn editable_form(manifest: &Manifest, text: &str) -> Option<ConfigForm> {
    if manifest.schema != 2
        || manifest.scopes.len() > MAX_ROWS
        || manifest.services.len() > MAX_ROWS
    {
        return None;
    }
    let doc = text.parse::<DocumentMut>().ok()?;
    for key in ["scope", "service"] {
        if doc
            .get(key)
            .is_some_and(|value| !value.is_array_of_tables())
        {
            return None;
        }
    }
    if manifest.site.is_some() && !doc.get("site").is_some_and(Item::is_table) {
        return None;
    }
    let scopes = manifest
        .scopes
        .iter()
        .map(|scope| ConfigScope {
            id: scope.id.clone(),
            directory: scope.directory.to_string_lossy().into_owned(),
            runtime: scope.runtime.as_str().into(),
            manager: scope
                .manager
                .map(|manager| manager.as_str())
                .unwrap_or_default()
                .into(),
            lockfile_root: scope.lockfile_root.to_string_lossy().into_owned(),
            environment: match scope.environment {
                crate::EnvironmentOwnership::Project => "project",
                crate::EnvironmentOwnership::Managed => "managed",
            }
            .into(),
        })
        .collect();
    let services = manifest
        .services
        .iter()
        .map(|service| {
            let (protocol, port) = match &service.endpoint {
                None => (String::new(), String::new()),
                Some(endpoint) => {
                    let protocol = match endpoint.protocol {
                        crate::EndpointProtocol::Http => "http",
                        crate::EndpointProtocol::Tcp => "tcp",
                        crate::EndpointProtocol::FastCgi => "fastcgi",
                    };
                    let port = if endpoint.port == 0 {
                        "auto".into()
                    } else {
                        endpoint.port.to_string()
                    };
                    (protocol.into(), port)
                }
            };
            ConfigService {
                id: service.id.clone(),
                scope: service.scope_id.clone(),
                command: service.command.clone(),
                working_directory: service.working_directory.to_string_lossy().into_owned(),
                protocol,
                port,
            }
        })
        .collect();
    let site = manifest.site.as_ref().map(|site| {
        let (handler, service, root) = match &site.kind {
            SiteKind::Proxy { service_id } => ("proxy", service_id.as_str(), ""),
            SiteKind::Static { root } => ("static", "", root.as_str()),
            SiteKind::Php { service_id, root } => ("php", service_id.as_str(), root.as_str()),
        };
        ConfigSite {
            host: site.host.clone(),
            handler: handler.into(),
            service: service.into(),
            root: root.into(),
        }
    });
    Some(ConfigForm {
        name: manifest.name.clone(),
        scopes,
        services,
        site,
    })
}

fn render(snapshot: &Snapshot, base: &ConfigForm, next: &ConfigForm) -> Result<String> {
    if next.scopes.len() > MAX_ROWS || next.services.len() > MAX_ROWS {
        return Err(failure("The form supports up to 16 scopes and 16 services. Use your editor for larger configurations."));
    }
    let new_ids = |items: Vec<&str>| {
        let count = items.len();
        if items.into_iter().collect::<BTreeSet<_>>().len() != count {
            Err(failure("Scope and service IDs must be unique."))
        } else {
            Ok(())
        }
    };
    new_ids(next.scopes.iter().map(|row| row.id.as_str()).collect())?;
    new_ids(next.services.iter().map(|row| row.id.as_str()).collect())?;
    if snapshot.inspection.project_id.is_some() {
        for scope in &base.scopes {
            let edited = next.scopes.iter().find(|item| item.id == scope.id);
            if edited.is_none_or(|item| item.runtime != scope.runtime) {
                return Err(failure("Existing scope IDs and runtimes must be preserved in the form. Use your editor for advanced changes."));
            }
        }
        if base
            .services
            .iter()
            .any(|row| !next.services.iter().any(|item| item.id == row.id))
        {
            return Err(failure("Existing service IDs must be preserved in the form. Use your editor for advanced changes."));
        }
    }
    if base == next && crate::parse_manifest(&snapshot.base).is_ok() {
        return Ok(snapshot.base.clone());
    }
    let mut doc = if crate::parse_manifest(&snapshot.base).is_ok() {
        snapshot
            .base
            .parse::<DocumentMut>()
            .map_err(|error| failure(&error.to_string()))?
    } else {
        DocumentMut::new()
    };
    let top = doc.as_table_mut();
    if !top.contains_key("schema") {
        top.insert("schema", value(2));
    }
    set_changed(top, "name", &next.name, Some(&base.name));
    if !top.contains_key("name") {
        top.insert("name", value(&next.name));
    }

    edit_rows(
        top,
        "scope",
        &next.scopes,
        |row| &row.id,
        |table, row| {
            let prior = base.scopes.iter().find(|item| item.id == row.id);
            set_changed(
                table,
                "directory",
                &row.directory,
                prior.map(|v| &v.directory),
            );
            set_changed(table, "runtime", &row.runtime, prior.map(|v| &v.runtime));
            set_changed(table, "manager", &row.manager, prior.map(|v| &v.manager));
            let owner = if row.lockfile_root.is_empty() {
                &row.directory
            } else {
                &row.lockfile_root
            };
            set_changed(
                table,
                "lockfile_root",
                owner,
                prior.map(|v| &v.lockfile_root),
            );
            if row.environment.is_empty() {
                table.remove("environment");
            } else {
                set_changed(
                    table,
                    "environment",
                    &row.environment,
                    prior.map(|v| &v.environment),
                );
            }
            Ok(())
        },
    )?;
    edit_rows(
        top,
        "service",
        &next.services,
        |row| &row.id,
        |table, row| {
            let prior = base.services.iter().find(|item| item.id == row.id);
            set_changed(table, "scope", &row.scope, prior.map(|v| &v.scope));
            set_changed(
                table,
                "working_directory",
                &row.working_directory,
                prior.map(|v| &v.working_directory),
            );
            if prior.is_none_or(|old| old.command != row.command) {
                let command: Array = row.command.iter().cloned().map(Value::from).collect();
                set(table, "command", value(command));
            }
            if prior.is_none_or(|old| old.protocol != row.protocol || old.port != row.port) {
                if row.protocol.is_empty() {
                    table.remove("endpoint");
                } else {
                    let port = if row.port == "auto" {
                        Value::from("auto")
                    } else {
                        Value::from(
                            row.port
                                .parse::<i64>()
                                .map_err(|_| failure("Enter auto or a numeric endpoint port."))?,
                        )
                    };
                    let mut endpoint = InlineTable::new();
                    endpoint.insert("protocol", Value::from(row.protocol.as_str()));
                    endpoint.insert("port", port);
                    set(table, "endpoint", value(endpoint));
                }
            }
            if prior.is_none_or(|old| old.protocol != row.protocol) {
                let mut readiness = InlineTable::new();
                if row.protocol.is_empty() {
                    readiness.insert("kind", Value::from("process"));
                    readiness.insert("grace_ms", Value::from(300));
                } else {
                    readiness.insert("kind", Value::from("tcp"));
                    readiness.insert("timeout_secs", Value::from(30));
                }
                set(table, "readiness", value(readiness));
            }
            Ok(())
        },
    )?;
    if next.site != base.site {
        match &next.site {
            None => {
                top.remove("site");
            }
            Some(site) => {
                if !top.contains_key("site") {
                    top.insert("site", Item::Table(Table::new()));
                }
                let table = top["site"].as_table_mut().expect("editable site table");
                let prior = base.site.as_ref();
                if site.host.is_empty() {
                    table.remove("host");
                } else {
                    set_changed(table, "host", &site.host, prior.map(|v| &v.host));
                }
                set_changed(table, "handler", &site.handler, prior.map(|v| &v.handler));
                if site.handler == "static" {
                    table.remove("service");
                } else {
                    set_changed(table, "service", &site.service, prior.map(|v| &v.service));
                }
                if site.handler == "proxy" {
                    table.remove("root");
                } else {
                    set_changed(table, "root", &site.root, prior.map(|v| &v.root));
                }
            }
        }
    }
    Ok(doc.to_string())
}

fn set_changed(table: &mut Table, key: &str, next: &String, prior: Option<&String>) {
    if prior != Some(next) {
        set(table, key, value(next));
    }
}

fn set(table: &mut Table, key: &str, mut next: Item) {
    if let (Some(prior), Some(next)) =
        (table.get(key).and_then(Item::as_value), next.as_value_mut())
    {
        *next.decor_mut() = prior.decor().clone();
    }
    table.insert(key, next);
}

fn edit_rows<T>(
    top: &mut Table,
    key: &str,
    next: &[T],
    id: impl Fn(&T) -> &str,
    edit: impl Fn(&mut Table, &T) -> Result<()>,
) -> Result<()> {
    if next.is_empty() {
        top.remove(key);
        return Ok(());
    }
    if !top.contains_key(key) {
        top.insert(key, Item::ArrayOfTables(ArrayOfTables::new()));
    }
    let rows = top[key]
        .as_array_of_tables_mut()
        .expect("editable array of tables");
    rows.retain(|table| {
        table
            .get("id")
            .and_then(Item::as_str)
            .is_some_and(|key| next.iter().any(|row| id(row) == key))
    });
    for row in next {
        let key = id(row);
        let position = rows
            .iter()
            .position(|table| table.get("id").and_then(Item::as_str) == Some(key));
        let table = if let Some(position) = position {
            rows.get_mut(position).expect("row from array")
        } else {
            let mut table = Table::new();
            table.insert("id", value(key));
            rows.push(table);
            rows.get_mut(rows.len() - 1).expect("inserted row")
        };
        edit(table, row)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXISTING: &str = r#"# keep this heading
schema = 2
name = "review-app"

[[scope]]
id = "app"
directory = "."
runtime = "node"
manager = "npm"

[[service]]
id = "web"
scope = "app"
command = ["node", "index.js"] # keep this note
readiness = { kind = "process", grace_ms = 300 }

[service.env]
SETTING = "keep this setting"
"#;

    #[test]
    fn preview_does_not_write_and_rejects_a_file_created_after_review() {
        let area = crate::paths::disposable_dir("s12-new-config");
        let root = area.join("project");
        let data = area.join("engine");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&data).unwrap();
        let root = crate::plain_path(root.canonicalize().unwrap());
        let store = Store::open(&data).unwrap();
        let inspected = inspect_config(&store, &root).unwrap();
        let form = editable_form(&crate::parse_manifest(EXISTING).unwrap(), EXISTING).unwrap();
        let draft = ConfigDraft {
            root: inspected.root,
            revision: inspected.revision,
            form: Some(form),
        };
        let reviewed = preview_config(&store, &draft).unwrap();
        assert!(
            reviewed.preview.can_apply,
            "{:?}",
            reviewed.preview.diagnostics
        );
        assert!(!root.join(MANIFEST).exists());
        assert!(store.list_projects().unwrap().is_empty());
        fs::write(root.join(MANIFEST), "# arrived during review").unwrap();
        assert!(apply_config(&data, &store, &draft, &reviewed.fingerprint).is_err());
        assert_eq!(
            fs::read_to_string(root.join(MANIFEST)).unwrap(),
            "# arrived during review"
        );
        drop(store);
        fs::remove_dir_all(area).unwrap();
    }

    #[test]
    fn checked_edit_preserves_comments_and_external_changes() {
        let area = crate::paths::disposable_dir("s12-existing-config");
        let root = area.join("project");
        let data = area.join("engine");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&data).unwrap();
        let root = crate::plain_path(root.canonicalize().unwrap());
        let target = root.join(MANIFEST);
        fs::write(&target, EXISTING).unwrap();
        let store = Store::open(&data).unwrap();
        let project = crate::import_project(&data, &root).unwrap();
        let inspected = inspect_config(&store, &root).unwrap();
        let mut form = inspected.form.unwrap();
        form.services[0].command[1] = "server.js".into();
        let draft = ConfigDraft {
            root: inspected.root,
            revision: inspected.revision,
            form: Some(form),
        };
        let reviewed = preview_config(&store, &draft).unwrap();
        assert!(
            reviewed.preview.can_apply,
            "{:?}",
            reviewed.preview.diagnostics
        );
        assert!(reviewed.manifest_text.contains("server.js"));
        assert!(reviewed.manifest_text.contains("# keep this heading"));
        assert!(reviewed.manifest_text.contains("# keep this note"));
        assert!(reviewed
            .manifest_text
            .contains("SETTING = \"keep this setting\""));
        let saved = apply_config(&data, &store, &draft, &reviewed.fingerprint).unwrap();
        assert_eq!(saved.id, project.id);
        assert_eq!(fs::read_to_string(&target).unwrap(), reviewed.manifest_text);

        let inspected = inspect_config(&store, &root).unwrap();
        let stale = ConfigDraft {
            root: inspected.root,
            revision: inspected.revision,
            form: inspected.form,
        };
        let reviewed = preview_config(&store, &stale).unwrap();
        fs::write(
            &target,
            format!("# external edit\n{}", reviewed.manifest_text),
        )
        .unwrap();
        assert!(apply_config(&data, &store, &stale, &reviewed.fingerprint).is_err());
        assert!(fs::read_to_string(target)
            .unwrap()
            .starts_with("# external edit"));
        drop(store);
        fs::remove_dir_all(area).unwrap();
    }
}

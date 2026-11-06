//! Read-only evidence and editable suggestions for applications without a manifest.

mod fs;
mod node;
mod python;
#[cfg(test)]
mod tests;

use std::path::Path;

use serde::{Deserialize, Serialize};

use self::fs::Directory;
use crate::preview::Diagnostic;
use crate::{Error, ManagerKind, Result, RuntimeRole};

const APPLICATION_DIRS: &[&str] = &["frontend", "backend", "web", "api", "client", "server"];
const CONTAINER_DIRS: &[&str] = &["apps", "packages"];
const MANIFEST: &str = "stackyard.toml";
const MAX_ROOT_BYTES: usize = 2048;
const MAX_EVIDENCE: usize = 64;
const MAX_DIAGNOSTICS: usize = 32;
const MAX_SCOPES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportEvidence {
    pub path: String,
    pub detail: String,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportDetection {
    pub root: String,
    pub evidence: Vec<ImportEvidence>,
    pub diagnostics: Vec<Diagnostic>,
    pub manifest_text: String,
    pub has_manifest: bool,
}

/// Inspect a fixed set of ordinary files and directories. No project command
/// runs and no path or dependency environment is created during detection.
pub fn detect_project(root: &Path) -> Result<ImportDetection> {
    let (directory, canonical) = Directory::root(root)?;
    let name = project_name(&canonical);
    let root = canonical
        .into_os_string()
        .into_string()
        .map_err(|_| Error::OperationFailed("Import directory must have a Unicode path.".into()))?;
    if root.len() > MAX_ROOT_BYTES {
        return Err(Error::OperationFailed(
            "Import directory path is too long.".into(),
        ));
    }
    let has_manifest = directory
        .exists(MANIFEST)
        .map_err(|source| Error::io("inspect existing stackyard.toml", source))?;
    let mut detection = ImportDetection {
        root,
        evidence: Vec::new(),
        diagnostics: Vec::new(),
        manifest_text: String::new(),
        has_manifest,
    };
    if has_manifest {
        detection.record(MANIFEST, "Existing Stackyard manifest", "high");
        return Ok(detection);
    }

    let mut draft = Draft {
        name,
        ..Draft::default()
    };
    let root = Candidate {
        directory: &directory,
        path: ".",
    };
    let workspace = node::inspect_root(&mut detection, &mut draft, &root);
    scan_non_node(&mut detection, &mut draft, &root);
    for name in APPLICATION_DIRS {
        if let Ok(Some(directory)) = root.child(&mut detection, name) {
            let candidate = Candidate {
                directory: &directory,
                path: name,
            };
            scan_candidate(&mut detection, &mut draft, &candidate, &workspace);
        }
    }
    for name in CONTAINER_DIRS {
        if let Ok(Some(directory)) = root.child(&mut detection, name) {
            let container = Candidate {
                directory: &directory,
                path: name,
            };
            for name in APPLICATION_DIRS {
                if let Ok(Some(directory)) = container.child(&mut detection, name) {
                    let path = container.join(name);
                    let candidate = Candidate {
                        directory: &directory,
                        path: &path,
                    };
                    scan_candidate(&mut detection, &mut draft, &candidate, &workspace);
                }
            }
        }
    }
    if draft.services.is_empty() {
        detection.warn(
            "manual_setup",
            ".",
            "No launch suggestion could be completed. Add an explicit scope and service in the editor.",
        );
    }
    detection.manifest_text = draft.render();
    Ok(detection)
}

fn scan_candidate(
    detection: &mut ImportDetection,
    draft: &mut Draft,
    candidate: &Candidate<'_>,
    workspace: &node::Workspace,
) {
    node::inspect_child(detection, draft, candidate, workspace);
    scan_non_node(detection, draft, candidate);
}

fn scan_non_node(detection: &mut ImportDetection, draft: &mut Draft, candidate: &Candidate<'_>) {
    let _ = python::inspect(detection, draft, candidate);
    let _ = inspect_php(detection, draft, candidate);
}

impl ImportDetection {
    fn record(&mut self, path: impl Into<String>, detail: impl Into<String>, confidence: &str) {
        if self.evidence.len() >= MAX_EVIDENCE {
            return;
        }
        self.evidence.push(ImportEvidence {
            path: path.into(),
            detail: detail.into(),
            confidence: confidence.into(),
        });
        if self.evidence.len() == MAX_EVIDENCE {
            self.warn(
                "inspection_limit",
                ".",
                "Additional evidence may be omitted from this preview.",
            );
        }
    }

    fn warn(&mut self, code: &str, path: &str, message: impl Into<String>) {
        let (code, path, message) = match self.diagnostics.len() {
            len if len < MAX_DIAGNOSTICS - 1 => (code, path, message.into()),
            len if len == MAX_DIAGNOSTICS - 1 => (
                "inspection_limit",
                ".",
                "Additional inspection warnings may be omitted from this preview.".into(),
            ),
            _ => return,
        };
        self.diagnostics.push(Diagnostic {
            code: code.into(),
            severity: "warning".into(),
            field: path.into(),
            scope_id: None,
            service_id: None,
            message,
        });
    }
}

struct Candidate<'a> {
    directory: &'a Directory,
    path: &'a str,
}

type Inspection<T> = std::result::Result<T, ()>;

impl Candidate<'_> {
    fn join(&self, name: &str) -> String {
        if self.path == "." {
            name.into()
        } else {
            format!("{}/{name}", self.path)
        }
    }

    fn checked<T>(
        &self,
        detection: &mut ImportDetection,
        name: &str,
        result: std::io::Result<T>,
    ) -> Inspection<T> {
        result.map_err(|error| {
            detection.warn("inspection_skipped", &self.join(name), error.to_string())
        })
    }

    fn child(&self, detection: &mut ImportDetection, name: &str) -> Inspection<Option<Directory>> {
        self.checked(detection, name, self.directory.directory(name.as_ref()))
    }

    fn text(&self, detection: &mut ImportDetection, name: &str) -> Inspection<Option<String>> {
        self.checked(detection, name, self.directory.text(name))
    }

    fn exists(&self, detection: &mut ImportDetection, name: &str) -> Inspection<bool> {
        self.checked(detection, name, self.directory.exists(name))
    }

    fn json(
        &self,
        detection: &mut ImportDetection,
        name: &str,
    ) -> Inspection<Option<serde_json::Value>> {
        let Some(text) = self.text(detection, name)? else {
            return Ok(None);
        };
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(value) if value.is_object() => Ok(Some(value)),
            _ => {
                detection.warn(
                    "malformed_metadata",
                    &self.join(name),
                    "Expected a JSON object.",
                );
                Err(())
            }
        }
    }

    fn marker(
        &self,
        detection: &mut ImportDetection,
        name: &str,
        detail: &str,
    ) -> Inspection<bool> {
        let found = self.exists(detection, name)?;
        if found {
            detection.record(self.join(name), detail, "high");
        }
        Ok(found)
    }
}

#[derive(Default)]
struct Draft {
    name: String,
    scopes: Vec<DraftScope>,
    services: Vec<DraftService>,
    site: Option<DraftSite>,
}

struct DraftScope {
    id: String,
    directory: String,
    manager: ManagerKind,
    runtime: RuntimeRole,
    lockfile_root: String,
}

struct DraftService {
    id: String,
    directory: String,
    command: Vec<String>,
    endpoint: DraftEndpoint,
}

enum DraftEndpoint {
    Http,
    FastCgi,
    Manual,
}

struct DraftSite {
    service: String,
    root: String,
}

impl Draft {
    fn add(
        &mut self,
        detection: &mut ImportDetection,
        mut scope: DraftScope,
        command: Vec<String>,
        endpoint: DraftEndpoint,
    ) -> Option<String> {
        if self.scopes.len() >= MAX_SCOPES {
            detection.warn(
                "scope_limit",
                &scope.directory,
                "The draft has reached its suggestion limit. Add this scope manually if needed.",
            );
            return None;
        }
        if self.scopes.iter().any(|other| {
            (other.lockfile_root == scope.lockfile_root || other.directory == scope.directory)
                && other.manager != scope.manager
        }) {
            detection.warn(
                "scope_owner_conflict",
                &scope.directory,
                "Two ecosystems share an application directory or lockfile owner. Review scope ownership and add the other service manually.",
            );
            return None;
        }
        if scope.directory != "." {
            scope.id = format!("{}-{}", scope.directory.replace('/', "-"), scope.id);
        }
        let id = scope.id.clone();
        self.services.push(DraftService {
            id: id.clone(),
            directory: scope.directory.clone(),
            command,
            endpoint,
        });
        self.scopes.push(scope);
        Some(id)
    }

    fn render(&self) -> String {
        use std::fmt::Write;

        // Values interpolated into TOML strings are fixed tokens or sanitized
        // names; arguments are escaped through TOML's own string formatter.
        let mut text = format!("schema = 2\nname = \"{}\"\n", self.name);
        for scope in &self.scopes {
            writeln!(
                text,
                "\n[[scope]]\nid = \"{}\"\ndirectory = \"{}\"\nruntime = \"{}\"\nmanager = \"{}\"\nlockfile_root = \"{}\"",
                scope.id, scope.directory, scope.runtime.as_str(), scope.manager.as_str(),
                scope.lockfile_root,
            ).expect("write to string");
        }
        for service in &self.services {
            let command = toml::Value::Array(
                service
                    .command
                    .iter()
                    .cloned()
                    .map(toml::Value::String)
                    .collect(),
            );
            writeln!(
                text,
                "\n[[service]]\nid = \"{}\"\nscope = \"{}\"\ncommand = {}\nworking_directory = \"{}\"",
                service.id, service.id, command, service.directory,
            ).expect("write to string");
            match service.endpoint {
                DraftEndpoint::Http | DraftEndpoint::FastCgi => {
                    let protocol = if matches!(service.endpoint, DraftEndpoint::Http) {
                        "http"
                    } else {
                        "fastcgi"
                    };
                    writeln!(
                        text,
                        "endpoint = {{ protocol = \"{protocol}\", port = \"auto\" }}"
                    )
                    .expect("write to string");
                    text.push_str("readiness = { kind = \"tcp\", timeout_secs = 60 }\n");
                }
                DraftEndpoint::Manual => {
                    text.push_str("readiness = { kind = \"process\", grace_ms = 1500 }\n");
                }
            }
        }
        if let Some(site) = &self.site {
            writeln!(
                text,
                "\n[site]\nhandler = \"php\"\nservice = \"{}\"\nroot = \"{}\"",
                site.service, site.root,
            )
            .expect("write to string");
        }
        text
    }
}

fn project_name(root: &Path) -> String {
    let mut name: String = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .skip_while(|c| !c.is_ascii_lowercase())
        .take(63)
        .collect();
    name.truncate(name.trim_end_matches('-').len());
    if name.is_empty() {
        "imported-project".into()
    } else {
        name
    }
}

fn inspect_php(
    detection: &mut ImportDetection,
    draft: &mut Draft,
    candidate: &Candidate<'_>,
) -> Inspection<()> {
    let composer = candidate.json(detection, "composer.json")?.is_some();
    if composer {
        detection.record(
            candidate.join("composer.json"),
            "Composer dependency metadata",
            "high",
        );
    }
    let lock = candidate.marker(detection, "composer.lock", "Composer lockfile")?;
    if lock && !composer {
        detection.warn(
            "missing_metadata",
            &candidate.join("composer.lock"),
            "Composer lockfile needs its composer.json.",
        );
        return Ok(());
    }
    let mut document_root = None;
    if let Some(public) = candidate.child(detection, "public")? {
        let path = candidate.join("public");
        let public = Candidate {
            directory: &public,
            path: &path,
        };
        if public.marker(detection, "index.php", "Conventional PHP front controller")? {
            document_root = Some(path);
        }
    }
    if document_root.is_none()
        && candidate.marker(
            detection,
            "index.php",
            "PHP entry point in application directory",
        )?
    {
        document_root = Some(candidate.path.into());
    }
    let Some(root) = document_root else {
        if composer {
            detection.warn("unknown_php_root", candidate.path, "No public/index.php or index.php was found. Choose the PHP document root manually.");
        }
        return Ok(());
    };
    if !composer {
        detection.warn("missing_metadata", candidate.path, "This PHP application has no composer.json. The FastCGI suggestion can be reviewed independently of Composer dependency installation.");
    }
    detection.warn(
        "review_php_root",
        &root,
        "Review the FastCGI document root and ensure it only exposes files intended for HTTP.",
    );
    let scope = DraftScope {
        id: "php".into(),
        directory: candidate.path.into(),
        runtime: RuntimeRole::Php,
        manager: ManagerKind::Composer,
        lockfile_root: candidate.path.into(),
    };
    let service = draft.add(
        detection,
        scope,
        strings(&["php-cgi", "-b", "127.0.0.1:{port}"]),
        DraftEndpoint::FastCgi,
    );
    if let Some(service) = service {
        if draft.site.is_some() {
            detection.warn("multiple_php_roots", &root, "Only one project site can be suggested. Select the desired document root in the editor.");
        } else {
            draft.site = Some(DraftSite { service, root });
        }
    }
    Ok(())
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

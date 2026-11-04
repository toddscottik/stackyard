//! A pure evaluator over explicit snapshots, separate from read-only collection.
//! No planning/execution helper is called: those may probe tools or create venvs.
use crate::{EndpointSpec, ManagerKind, Manifest, ReadinessSpec, RuntimeRole, Store};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub severity: String,
    pub field: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_id: Option<String>,
    pub message: String,
}
impl Diagnostic {
    pub fn error(code: &str, field: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity: "error".into(),
            field: field.into(),
            scope_id: None,
            service_id: None,
            message: message.into(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewSelection {
    pub scope_id: String,
    pub role: String,
    pub package_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewRequest {
    pub root: String,
    pub manifest_text: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub selection_overrides: Vec<PreviewSelection>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageSnapshot {
    pub id: String,
    pub name: String,
    pub version: String,
    pub target: String,
    pub available: bool,
    pub bundled_tools: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathSnapshot {
    pub field: String,
    pub scope_id: Option<String>,
    pub service_id: Option<String>,
    pub contained: bool,
    pub exists: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeFiles {
    pub scope_id: String,
    pub files: Vec<String>,
    pub package_manager: Option<String>,
    pub metadata_valid: bool,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewContext {
    pub target: String,
    pub packages: Vec<PackageSnapshot>,
    pub selections: Vec<PreviewSelection>,
    pub paths: Vec<PathSnapshot>,
    pub files: Vec<ScopeFiles>,
    pub diagnostics: Vec<Diagnostic>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopePreview {
    pub id: String,
    pub runtime: String,
    pub manager: String,
    pub runtime_package_id: Option<String>,
    pub tool_package_id: Option<String>,
    pub environment_state: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServicePreview {
    pub id: String,
    pub scope_id: String,
    pub endpoint: Option<EndpointSpec>,
    pub resolved_port: Option<u16>,
    pub readiness: ReadinessSpec,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectPreview {
    pub can_apply: bool,
    pub scopes: Vec<ScopePreview>,
    pub services: Vec<ServicePreview>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn preview_request(
    draft: &PreviewRequest,
    store: Option<&Store>,
) -> crate::Result<ProjectPreview> {
    if draft.manifest_text.len() > 64 * 1024 {
        return Ok(preview_project(
            &draft.manifest_text,
            &PreviewContext::default(),
            &draft.selection_overrides,
        ));
    }
    let context = match crate::parse_manifest(&draft.manifest_text) {
        Ok(manifest) => {
            let root = Path::new(&draft.root);
            let imported = match (store, root.canonicalize()) {
                (Some(store), Ok(path)) => {
                    store.project_by_root(&crate::plain_path(path).display().to_string())?
                }
                _ => None,
            };
            let project = draft
                .project_id
                .as_deref()
                .or(imported.as_ref().map(|p| p.id.as_str()));
            collect_context(root, &manifest, store, project)?
        }
        Err(_) => PreviewContext::default(),
    };
    Ok(preview_project(
        &draft.manifest_text,
        &context,
        &draft.selection_overrides,
    ))
}

pub fn preview_project(
    text: &str,
    context: &PreviewContext,
    overrides: &[PreviewSelection],
) -> ProjectPreview {
    let mut out = ProjectPreview {
        can_apply: false,
        scopes: Vec::new(),
        services: Vec::new(),
        diagnostics: context.diagnostics.clone(),
    };
    if text.len() > 64 * 1024 {
        out.diagnostics.push(Diagnostic::error(
            "manifest_too_large",
            "manifest",
            "Manifest exceeds the 64 KiB preview limit.",
        ));
        return out;
    }
    let manifest = match crate::parse_manifest(text) {
        Ok(value) => value,
        Err(_) => {
            out.diagnostics.push(Diagnostic::error("manifest_invalid","manifest","Manifest is invalid. Check its schema, scope references, paths and readiness settings."));
            return out;
        }
    };
    for item in overrides {
        let scope = manifest.scope(&item.scope_id);
        let role_valid = scope.is_some_and(|s| {
            item.role == s.runtime.as_str()
                || matches!(
                    (s.runtime, item.role.as_str()),
                    (RuntimeRole::Node, "pnpm")
                        | (RuntimeRole::Python, "uv")
                        | (RuntimeRole::Php, "composer")
                )
        });
        if !role_valid
            || !context.packages.iter().any(|p| {
                p.id == item.package_id
                    && p.name == item.role
                    && p.available
                    && p.target == context.target
            })
        {
            out.diagnostics.push(Diagnostic::error(
                "selection_invalid",
                "selection_overrides",
                "Selection must identify a compatible, available runtime or tool for its scope.",
            ));
        }
        if scope.is_none() {
            out.diagnostics.push(Diagnostic::error(
                "scope_unknown",
                "selection_overrides",
                "Selection refers to an unknown scope.",
            ));
        }
    }
    for path in &context.paths {
        if !path.contained || !path.exists {
            let mut d = Diagnostic::error(
                if !path.contained {
                    "path_outside_project"
                } else {
                    "directory_missing"
                },
                &path.field,
                if !path.contained {
                    "Directory escapes the project or its dependency owner."
                } else {
                    "Directory does not exist; preview did not create it."
                },
            );
            d.scope_id = path.scope_id.clone();
            d.service_id = path.service_id.clone();
            out.diagnostics.push(d);
        }
    }
    for scope in &manifest.scopes {
        let files = context.files.iter().find(|item| item.scope_id == scope.id);
        if manifest.schema == 1 && manifest.scopes.len() > 1 && scope.manager.is_none() {
            let mut d = Diagnostic::error(
                "scope_required",
                "scope.manager",
                "Mixed legacy dependencies need an explicit scope and manager choice.",
            );
            d.scope_id = Some(scope.id.clone());
            out.diagnostics.push(d);
        }
        let manager = scope.manager.unwrap_or(match scope.runtime {
            RuntimeRole::Node => {
                if let Some(recorded) = files
                    .and_then(|f| f.package_manager.as_deref())
                    .and_then(|v| ManagerKind::parse(v.split('@').next().unwrap_or(v)))
                    .filter(|m| m.is_node())
                {
                    recorded
                } else if files.is_some_and(|f| {
                    f.files
                        .iter()
                        .any(|n| n == "package-lock.json" || n == "npm-shrinkwrap.json")
                }) {
                    ManagerKind::Npm
                } else {
                    ManagerKind::Pnpm
                }
            }
            RuntimeRole::Python => {
                if files.is_some_and(|f| f.files.iter().any(|n| n == "uv.lock")) {
                    ManagerKind::Uv
                } else {
                    ManagerKind::Pip
                }
            }
            RuntimeRole::Php => ManagerKind::Composer,
        });
        let choose = |role: &str| {
            overrides
                .iter()
                .rev()
                .chain(context.selections.iter())
                .find(|item| item.scope_id == scope.id && item.role == role)
                .map(|item| item.package_id.clone())
        };
        let runtime = choose(scope.runtime.as_str());
        let tool = match manager {
            ManagerKind::Pnpm => Some("pnpm"),
            ManagerKind::Uv => Some("uv"),
            ManagerKind::Composer => Some("composer"),
            _ => None,
        };
        let selected_tool = tool.and_then(choose);
        for (role, selected, is_runtime) in [
            (scope.runtime.as_str(), runtime.as_ref(), true),
            (tool.unwrap_or(""), selected_tool.as_ref(), false),
        ] {
            if role.is_empty() {
                continue;
            }
            let installed = selected.and_then(|id| context.packages.iter().find(|p| p.id == *id));
            if !installed
                .is_some_and(|p| p.name == role && p.target == context.target && p.available)
            {
                let mut d = Diagnostic::error(
                    if is_runtime {
                        "runtime_missing"
                    } else {
                        "tool_missing"
                    },
                    &format!("scope.{}.{}", scope.id, role),
                    format!("Select an available {role} for scope {}.", scope.id),
                );
                d.scope_id = Some(scope.id.clone());
                out.diagnostics.push(d);
            }
        }
        if manager == ManagerKind::Npm {
            if let Some(package) = runtime
                .as_ref()
                .and_then(|id| context.packages.iter().find(|p| p.id == *id))
            {
                if !package.bundled_tools.iter().any(|t| t == "npm") {
                    let mut d = Diagnostic::error(
                        "tool_missing",
                        &format!("scope.{}.manager", scope.id),
                        "Selected Node has no recorded npm launch file.",
                    );
                    d.scope_id = Some(scope.id.clone());
                    out.diagnostics.push(d);
                }
            }
        }
        if let Some(files) = files {
            let has = |name: &str| files.files.iter().any(|f| f == name);
            let node_conflict = (manager == ManagerKind::Npm
                && has("pnpm-lock.yaml")
                && !has("package-lock.json")
                && !has("npm-shrinkwrap.json"))
                || (manager == ManagerKind::Pnpm
                    && (has("package-lock.json") || has("npm-shrinkwrap.json"))
                    && !has("pnpm-lock.yaml"));
            let conflict = (manifest.schema == 2
                && scope.runtime == RuntimeRole::Node
                && has("pnpm-lock.yaml")
                && (has("package-lock.json") || has("npm-shrinkwrap.json")))
                || node_conflict
                || (manager == ManagerKind::Pip && has("uv.lock"))
                || (scope.manager.is_none() && has("pnpm-lock.yaml") && has("package-lock.json"));
            if conflict || !files.metadata_valid {
                let mut d = Diagnostic::error(
                    "manager_conflict",
                    &format!("scope.{}.manager", scope.id),
                    "Dependency metadata or lockfiles conflict with the selected manager.",
                );
                d.scope_id = Some(scope.id.clone());
                out.diagnostics.push(d);
            }
            if let Some(field) = files
                .package_manager
                .as_ref()
                .filter(|_| scope.runtime == RuntimeRole::Node)
            {
                if field.split('@').next().unwrap_or(field) != manager.as_str() {
                    let mut d = Diagnostic::error(
                        "manager_conflict",
                        &format!("scope.{}.manager", scope.id),
                        "packageManager does not match the selected manager.",
                    );
                    d.scope_id = Some(scope.id.clone());
                    out.diagnostics.push(d);
                }
                if let Some((name, version)) = field.split_once('@') {
                    let recorded = selected_tool
                        .as_ref()
                        .and_then(|id| context.packages.iter().find(|p| p.id == *id));
                    if name != manager.as_str() || recorded.is_some_and(|p| p.version != version) {
                        let mut d = Diagnostic::error(
                            "manager_conflict",
                            &format!("scope.{}.manager", scope.id),
                            "packageManager does not match the selected manager/version.",
                        );
                        d.scope_id = Some(scope.id.clone());
                        out.diagnostics.push(d);
                    }
                }
            }
        }
        out.scopes.push(ScopePreview {
            id: scope.id.clone(),
            runtime: scope.runtime.as_str().into(),
            manager: manager.as_str().into(),
            runtime_package_id: runtime,
            tool_package_id: selected_tool,
            environment_state: if scope.runtime == RuntimeRole::Python {
                "not_probed_or_created"
            } else {
                "project_owned"
            }
            .into(),
        });
    }
    for service in &manifest.services {
        let auto = service.endpoint.as_ref().is_some_and(|e| e.port == 0);
        if service.endpoint.is_some() {
            let mut d = Diagnostic::error(
                if auto {
                    "auto_port_unresolved"
                } else {
                    "port_unchecked"
                },
                &format!("service.{}.endpoint.port", service.id),
                if auto {
                    "Port will be chosen at start; preview does not bind sockets."
                } else {
                    "Fixed port availability is checked at start, not by preview."
                },
            );
            d.severity = "info".into();
            d.service_id = Some(service.id.clone());
            out.diagnostics.push(d);
        }
        out.services.push(ServicePreview {
            id: service.id.clone(),
            scope_id: service.scope_id.clone(),
            endpoint: service.endpoint.clone(),
            resolved_port: None,
            readiness: service.readiness.clone(),
        });
    }
    out.can_apply = !out.diagnostics.iter().any(|d| d.severity == "error");
    out
}

/// Bounded read-only inputs. Uses an already-open engine store; no migration or
/// environment creation is performed here. Evaluator above also works without IO.
pub fn collect_context(
    root: &Path,
    manifest: &Manifest,
    store: Option<&Store>,
    project: Option<&str>,
) -> crate::Result<PreviewContext> {
    let mut context = PreviewContext {
        target: crate::current_target().into(),
        ..PreviewContext::default()
    };
    let root_canon = root.canonicalize().ok().map(crate::plain_path);
    if let Some(store) = store {
        for package in store.list_packages()? {
            let bundled_tools = if package.name == "node"
                && crate::project::npm_launch(Path::new(&package.launch)).is_ok()
            {
                vec!["npm".into()]
            } else {
                Vec::new()
            };
            context.packages.push(PackageSnapshot {
                id: package.id,
                name: package.name,
                version: package.version,
                target: package.target,
                available: Path::new(&package.launch).is_file(),
                bundled_tools,
            });
        }
        for scope in &manifest.scopes {
            for role in [scope.runtime.as_str(), "pnpm", "uv", "composer"] {
                let scoped = match project {
                    Some(project) => store.get_scope_selection(project, &scope.id, role)?,
                    None => None,
                };
                let local = match project {
                    Some(project) => store.get_selection(project, role)?,
                    None => None,
                };
                if let Some(package_id) = scoped
                    .or(local)
                    .or(store.get_selection(crate::DEFAULT_PROJECT, role)?)
                {
                    context.selections.push(PreviewSelection {
                        scope_id: scope.id.clone(),
                        role: role.into(),
                        package_id,
                    });
                }
            }
        }
    }
    let mut add_path = |field: String,
                        scope: Option<String>,
                        service: Option<String>,
                        relative: &Path,
                        owner: Option<&Path>| {
        let actual = root
            .join(relative)
            .canonicalize()
            .ok()
            .map(crate::plain_path);
        let contained = actual
            .as_ref()
            .zip(root_canon.as_ref())
            .is_some_and(|(path, base)| path.starts_with(base))
            && owner
                .is_none_or(|owner| actual.as_ref().is_some_and(|path| path.starts_with(owner)));
        context.paths.push(PathSnapshot {
            field,
            scope_id: scope,
            service_id: service,
            contained: actual.is_none() || contained,
            exists: actual.as_ref().is_some_and(|p| p.is_dir()),
        });
    };
    add_path("root".into(), None, None, Path::new("."), None);
    for scope in &manifest.scopes {
        let owner = root
            .join(&scope.lockfile_root)
            .canonicalize()
            .ok()
            .map(crate::plain_path);
        add_path(
            format!("scope.{}.directory", scope.id),
            Some(scope.id.clone()),
            None,
            &scope.directory,
            owner.as_deref(),
        );
        add_path(
            format!("scope.{}.lockfile_root", scope.id),
            Some(scope.id.clone()),
            None,
            &scope.lockfile_root,
            None,
        );
    }
    for service in &manifest.services {
        let owner = manifest
            .scope(&service.scope_id)
            .and_then(|s| root.join(&s.directory).canonicalize().ok())
            .map(crate::plain_path);
        add_path(
            format!("service.{}.working_directory", service.id),
            Some(service.scope_id.clone()),
            Some(service.id.clone()),
            &service.working_directory,
            if manifest.schema == 1 {
                None
            } else {
                owner.as_deref()
            },
        );
    }
    if let Some(site) = &manifest.site {
        match &site.kind {
            crate::manifest::SiteKind::Static { root: site_root }
            | crate::manifest::SiteKind::Php {
                root: site_root, ..
            } => add_path("site.root".into(), None, None, Path::new(site_root), None),
            _ => {}
        }
    }
    let mut canonical_owners = std::collections::BTreeMap::new();
    for scope in &manifest.scopes {
        let directory = root.join(&scope.lockfile_root);
        if manifest.schema == 2 {
            if let Ok(owner) = directory.canonicalize() {
                if let Some((role, manager)) =
                    canonical_owners.insert(owner, (scope.runtime, scope.manager))
                {
                    if role != scope.runtime || manager != scope.manager {
                        context.diagnostics.push(Diagnostic::error(
                            "manager_conflict",
                            "scope.lockfile_root",
                            "Canonical dependency owner is shared by incompatible scopes.",
                        ));
                    }
                }
            }
        }
        let safe = directory
            .canonicalize()
            .ok()
            .map(crate::plain_path)
            .zip(root_canon.as_ref())
            .is_some_and(|(path, base)| path.starts_with(base));
        let files: Vec<String> = if safe {
            [
                "package.json",
                "package-lock.json",
                "npm-shrinkwrap.json",
                "pnpm-lock.yaml",
                "requirements.txt",
                "pyproject.toml",
                "uv.lock",
                "composer.json",
                "composer.lock",
            ]
            .into_iter()
            .filter(|name| directory.join(name).is_file())
            .map(str::to_string)
            .collect()
        } else {
            Vec::new()
        };
        if safe {
            let owner = directory.canonicalize().ok().map(crate::plain_path);
            for name in &files {
                let file = directory.join(name);
                if file
                    .canonicalize()
                    .ok()
                    .map(crate::plain_path)
                    .zip(owner.as_ref())
                    .is_none_or(|(p, o)| !p.starts_with(o))
                {
                    let mut d = Diagnostic::error(
                        "path_outside_project",
                        "scope.lockfile_root",
                        "Dependency metadata escapes its declared owner.",
                    );
                    d.scope_id = Some(scope.id.clone());
                    context.diagnostics.push(d);
                }
            }
        }
        let mut package_manager = None;
        let mut metadata_valid = true;
        if safe && scope.runtime == RuntimeRole::Node && directory.join("package.json").is_file() {
            let metadata = directory.join("package.json");
            let metadata_safe = metadata
                .canonicalize()
                .ok()
                .map(crate::plain_path)
                .zip(root_canon.as_ref())
                .is_some_and(|(path, base)| path.starts_with(base));
            match metadata_safe
                .then(|| read_metadata(&metadata))
                .flatten()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            {
                Some(value) => {
                    package_manager = value
                        .get("packageManager")
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                }
                None => metadata_valid = false,
            }
        }
        context.files.push(ScopeFiles {
            scope_id: scope.id.clone(),
            files,
            package_manager,
            metadata_valid,
        });
    }
    Ok(context)
}
fn read_metadata(path: &Path) -> Option<String> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes).ok()?;
    if bytes.len() > 65536 {
        return None;
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    const DRAFT:&str="schema=2\nname='draft'\n[[scope]]\nid='web'\ndirectory='.'\nruntime='node'\nmanager='pnpm'\n[[service]]\nid='web'\nscope='web'\ncommand=['node','app.js']\nendpoint={protocol='http',port='auto'}\nreadiness={kind='http',path='/',statuses=[200],timeout_secs=3}\n";
    #[test]
    fn evaluator_is_pure_and_reports_missing_inputs_without_resolving_ports() {
        let ctx = PreviewContext::default();
        let before = ctx.clone();
        let first = preview_project(DRAFT, &ctx, &[]);
        assert_eq!(ctx, before);
        assert_eq!(first, preview_project(DRAFT, &ctx, &[]));
        assert!(!first.can_apply);
        assert!(first
            .diagnostics
            .iter()
            .any(|d| d.code == "runtime_missing"));
        assert!(first.diagnostics.iter().any(|d| d.code == "tool_missing"));
        assert!(first
            .diagnostics
            .iter()
            .any(|d| d.code == "auto_port_unresolved"));
        assert!(first.services[0].resolved_port.is_none());
    }
    #[test]
    fn readonly_collection_does_not_create_missing_data_or_environments() {
        let root = crate::disposable_dir("preview-root");
        std::fs::create_dir_all(&root).unwrap();
        let state = root.join("absent-engine");
        assert!(Store::open_read_only(&state).unwrap().is_none());
        let manifest = crate::parse_manifest(DRAFT).unwrap();
        let context = collect_context(&root, &manifest, None, None).unwrap();
        let result = preview_project(DRAFT, &context, &[]);
        assert!(!result.can_apply);
        assert!(!state.exists());
        assert!(!root.join("environments").exists());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        std::fs::remove_dir(&root).unwrap();
    }
    #[test]
    fn conflicting_lockfiles_and_ignored_overrides_block_preview() {
        let ctx = PreviewContext {
            target: "test".into(),
            packages: vec![
                PackageSnapshot {
                    id: "node".into(),
                    name: "node".into(),
                    version: "24".into(),
                    target: "test".into(),
                    available: true,
                    bundled_tools: vec![],
                },
                PackageSnapshot {
                    id: "pnpm".into(),
                    name: "pnpm".into(),
                    version: "12".into(),
                    target: "test".into(),
                    available: true,
                    bundled_tools: vec![],
                },
            ],
            selections: vec![
                PreviewSelection {
                    scope_id: "web".into(),
                    role: "node".into(),
                    package_id: "node".into(),
                },
                PreviewSelection {
                    scope_id: "web".into(),
                    role: "pnpm".into(),
                    package_id: "pnpm".into(),
                },
            ],
            ..PreviewContext::default()
        };
        assert!(preview_project(DRAFT, &ctx, &[]).can_apply);
        let invalid = preview_project(
            DRAFT,
            &ctx,
            &[PreviewSelection {
                scope_id: "web".into(),
                role: "python".into(),
                package_id: "node".into(),
            }],
        );
        assert!(!invalid.can_apply);
        assert!(invalid
            .diagnostics
            .iter()
            .any(|d| d.code == "selection_invalid"));
        let mut conflict = ctx;
        conflict.files.push(ScopeFiles {
            scope_id: "web".into(),
            files: vec!["pnpm-lock.yaml".into(), "package-lock.json".into()],
            package_manager: None,
            metadata_valid: true,
        });
        assert!(preview_project(DRAFT, &conflict, &[])
            .diagnostics
            .iter()
            .any(|d| d.code == "manager_conflict"));
    }

    #[test]
    fn invalid_manifest_never_echoes_secret_values() {
        let result = preview_project(
            "schema=99\nsecret='DO_NOT_ECHO_SECRET'",
            &PreviewContext::default(),
            &[],
        );
        assert!(!serde_json::to_string(&result)
            .unwrap()
            .contains("DO_NOT_ECHO_SECRET"));
        assert_eq!(result.diagnostics[0].code, "manifest_invalid");
    }
}

//! Scope resolution shares the existing store, package records and launchers.
//! Reads do not persist inherited defaults or create environments.
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use crate::manifest::{EnvironmentOwnership, ManagerKind, Manifest, RuntimeRole, ScopeSpec};
use crate::{Error, PackageRecord, Result, Store};

#[derive(Debug, Clone)]
pub struct ResolvedScope {
    pub spec: ScopeSpec,
    pub directory: PathBuf,
    pub lockfile_root: PathBuf,
    pub manager: ManagerKind,
}

pub fn scope_environment_dir(data: &Path, project: &str, scope: &str, schema: u32) -> PathBuf {
    let root = data.join("environments").join(project);
    if schema == 1 {
        root
    } else {
        root.join("scopes").join(scope)
    }
}

pub fn scope_profile_key(project: &str, scope: &str, schema: u32) -> Result<String> {
    let key = if schema == 1 {
        project.into()
    } else {
        format!("p{}-{project}-{scope}", project.len())
    };
    if key.is_empty()
        || key.len() > 128
        || !key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
    {
        return Err(Error::InvalidMessage(
            "scope profile identity is invalid or exceeds 128 bytes".into(),
        ));
    }
    Ok(key)
}

pub fn scope_spec<'a>(manifest: &'a Manifest, scope: &str) -> Result<&'a ScopeSpec> {
    manifest
        .scopes
        .iter()
        .find(|item| item.id == scope)
        .ok_or_else(|| {
            Error::InvalidMessage(format!("scope_unknown: scope {scope:?} is not declared"))
        })
}

pub fn canonical_scope_dir(root: &Path, relative: &Path) -> Result<PathBuf> {
    if relative.is_absolute()
        || relative.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(Error::InvalidMessage(
            "path_outside_project: scope paths must stay inside the project".into(),
        ));
    }
    crate::contained_dir(root, relative)
}

pub fn resolve_scope_manager(
    root: &Path,
    manifest: &Manifest,
    scope: &ScopeSpec,
) -> Result<ManagerKind> {
    let owner = canonical_scope_dir(root, &scope.lockfile_root)?;
    if manifest.schema == 1 && manifest.scopes.len() > 1 && scope.manager.is_none() {
        return Err(Error::InvalidMessage(format!(
            "scope_required: choose a dependency manager for legacy scope {}",
            scope.id
        )));
    }
    if manifest.schema == 2 && scope.runtime == RuntimeRole::Node {
        let npm_lock = owner.join("package-lock.json").is_file()
            || owner.join("npm-shrinkwrap.json").is_file();
        if npm_lock && owner.join("pnpm-lock.yaml").is_file() {
            return Err(Error::InvalidMessage(
                "manager_conflict: npm and pnpm lockfiles share the scope owner".into(),
            ));
        }
        let field = crate::deps::package_manager_name(&owner)?;
        if field.is_some() && scope.manager.is_some() && field != scope.manager {
            return Err(Error::InvalidMessage(
                "manager_conflict: packageManager differs from the scope manager".into(),
            ));
        }
    }
    match scope.runtime {
        RuntimeRole::Node => crate::deps::resolve_node_manager(&owner, scope.manager),
        RuntimeRole::Python => crate::deps::resolve_python_manager(&owner, scope.manager),
        RuntimeRole::Php => match scope.manager {
            None | Some(ManagerKind::Composer) => Ok(ManagerKind::Composer),
            _ => Err(Error::InvalidMessage(
                "manager_conflict: PHP scopes require Composer".into(),
            )),
        },
    }
}

/// Canonical paths, rather than textual aliases, define a dependency owner.
pub fn validate_scope_ownership(root: &Path, manifest: &Manifest) -> Result<()> {
    let mut owners = BTreeMap::new();
    let mut directories = BTreeMap::new();
    for scope in &manifest.scopes {
        let directory = canonical_scope_dir(root, &scope.directory)?;
        let owner = canonical_scope_dir(root, &scope.lockfile_root)?;
        if !directory.starts_with(&owner) {
            return Err(Error::InvalidMessage(format!(
                "manager_conflict: lockfile root does not own scope {}",
                scope.id
            )));
        }
        if manifest.schema == 1 {
            continue;
        }
        let manager = resolve_scope_manager(root, manifest, scope)?;
        validate_lockfile_authority(root, &directory, &owner, manager)?;
        if let Some((runtime, previous)) = owners.insert(owner, (scope.runtime, manager)) {
            if runtime != scope.runtime || previous != manager {
                return Err(Error::InvalidMessage(
                    "manager_conflict: incompatible managers share a canonical lockfile owner"
                        .into(),
                ));
            }
        }
        if let Some(previous) =
            directories.insert(directory, (scope.runtime, manager, scope.environment))
        {
            if previous != (scope.runtime, manager, scope.environment) {
                return Err(Error::InvalidMessage(
                    "manager_conflict: incompatible scopes share a canonical directory".into(),
                ));
            }
        }
    }
    Ok(())
}

pub fn resolve_dependency_scope(
    root: &Path,
    manifest: &Manifest,
    scope_id: Option<&str>,
    directory: &str,
) -> Result<ResolvedScope> {
    validate_scope_ownership(root, manifest)?;
    let spec = match scope_id {
        Some(id) => scope_spec(manifest, id)?,
        None if manifest.schema == 1 => {
            if manifest.scopes.len() == 1 {
                &manifest.scopes[0]
            } else {
                let candidates: Vec<_> = manifest
                    .scopes
                    .iter()
                    .filter(|scope| scope.manager.is_some())
                    .collect();
                if candidates.len() != 1 {
                    return Err(Error::InvalidMessage(
                        "scope_required: mixed legacy dependency actions need an explicit scope"
                            .into(),
                    ));
                }
                candidates[0]
            }
        }
        None => {
            let directory = crate::deps::scope_dir(root, directory)?;
            let candidates: Vec<_> = manifest
                .scopes
                .iter()
                .filter(|scope| {
                    canonical_scope_dir(root, &scope.directory)
                        .is_ok_and(|root| directory.starts_with(root))
                })
                .collect();
            if candidates.len() != 1 {
                return Err(Error::InvalidMessage("scope_required: directory has no unambiguous dependency owner; provide scope_id".into()));
            }
            candidates[0]
        }
    };
    let scope_directory = canonical_scope_dir(root, &spec.directory)?;
    let requested = if scope_id.is_some() && (directory.is_empty() || directory == ".") {
        scope_directory.clone()
    } else {
        crate::deps::scope_dir(root, directory)?
    };
    if manifest.schema != 1 && !requested.starts_with(&scope_directory) {
        return Err(Error::InvalidMessage(
            "path_outside_project: dependency directory is outside its scope".into(),
        ));
    }
    Ok(ResolvedScope {
        spec: spec.clone(),
        directory: requested,
        lockfile_root: canonical_scope_dir(root, &spec.lockfile_root)?,
        manager: resolve_scope_manager(root, manifest, spec)?,
    })
}

pub fn resolve_scope_runtime(
    data: &Path,
    project: &str,
    scope_id: &str,
    role: RuntimeRole,
) -> Result<PackageRecord> {
    let (_, manifest) = crate::load_project_manifest(data, project)?;
    let scope = scope_spec(&manifest, scope_id)?;
    if scope.runtime != role {
        return Err(Error::InvalidMessage(
            "runtime_missing: scope runtime does not match requested role".into(),
        ));
    }
    resolve_scope_tool(data, project, scope_id, role.as_str())?.ok_or_else(|| {
        Error::InvalidMessage(format!(
            "runtime_missing: select {} for scope {scope_id}",
            role.as_str()
        ))
    })
}

pub fn resolve_scope_tool(
    data: &Path,
    project: &str,
    scope_id: &str,
    role: &str,
) -> Result<Option<PackageRecord>> {
    let (_, manifest) = crate::load_project_manifest(data, project)?;
    let scope = scope_spec(&manifest, scope_id)?;
    let allowed = role == scope.runtime.as_str()
        || matches!(
            (scope.runtime, role),
            (RuntimeRole::Node, "pnpm")
                | (RuntimeRole::Python, "uv")
                | (RuntimeRole::Php, "composer")
        );
    if !allowed {
        return Err(Error::InvalidMessage(
            "tool_missing: tool does not belong to the requested scope runtime".into(),
        ));
    }
    let store = Store::open(data)?;
    let package = store
        .get_scope_selection(project, scope_id, role)?
        .or(store.get_selection(project, role)?)
        .or(store.get_selection(crate::DEFAULT_PROJECT, role)?);
    let Some(id) = package else {
        return Ok(None);
    };
    let package = store
        .get_package(&id)?
        .ok_or_else(|| Error::PackageNotFound { id })?;
    if package.name != role
        || package.target != crate::current_target()
        || !Path::new(&package.launch).is_file()
    {
        return Err(Error::InvalidMessage(format!(
            "selected package is incompatible with scope role {role}"
        )));
    }
    Ok(Some(package))
}

pub fn select_for_scope(
    data: &Path,
    project: &str,
    scope_id: &str,
    package_id: &str,
) -> Result<()> {
    let (_, manifest) = crate::load_project_manifest(data, project)?;
    let scope = scope_spec(&manifest, scope_id)?;
    let store = Store::open(data)?;
    let package = store
        .get_package(package_id)?
        .ok_or_else(|| Error::PackageNotFound {
            id: package_id.into(),
        })?;
    let allowed = package.name == scope.runtime.as_str()
        || match scope.runtime {
            RuntimeRole::Node => package.name == "pnpm",
            RuntimeRole::Python => package.name == "uv",
            RuntimeRole::Php => package.name == "composer",
        };
    if !allowed
        || package.target != crate::current_target()
        || !Path::new(&package.launch).is_file()
    {
        return Err(Error::InvalidMessage("selected package does not match the scope runtime/tool/host or its executable is missing".into()));
    }
    for row in store.list_processes(project)? {
        if matches!(row.state.as_str(), "starting" | "running" | "orphaned")
            && manifest
                .services
                .iter()
                .any(|service| service.id == row.service_id && service.scope_id == scope_id)
        {
            return Err(Error::ProjectBusy {
                id: project.into(),
                state: format!("scope {scope_id} is active; stop it before changing selections"),
            });
        }
    }
    if package.name == "php" {
        let key = scope_profile_key(project, scope_id, manifest.schema)?;
        if let Some(profile) = crate::php::read_php_profile(data, &key)? {
            crate::php::verify_php_profile(Path::new(&package.launch), &profile)?;
        } else {
            let _ = crate::php::probe_php(Path::new(&package.launch))?;
        }
    }
    store.select_scope_package(project, scope_id, &package.name, package_id)
}

pub fn resolved_environment(
    data: &Path,
    project: &str,
    root: &Path,
    manifest: &Manifest,
    scope: &ScopeSpec,
) -> Result<PathBuf> {
    let expected = match scope.environment {
        EnvironmentOwnership::Managed => {
            scope_environment_dir(data, project, &scope.id, manifest.schema)
        }
        EnvironmentOwnership::Project => canonical_scope_dir(root, &scope.directory)?.join(".venv"),
    };
    if scope.environment == EnvironmentOwnership::Managed {
        let data_canonical = data
            .canonicalize()
            .map(crate::plain_path)
            .map_err(|source| Error::io("resolve environment owner", source))?;
        let mut existing = expected.as_path();
        while !existing.exists() {
            existing = existing.parent().ok_or_else(|| {
                Error::InvalidMessage("path_outside_project: environment has no owner".into())
            })?;
        }
        let canonical = existing
            .canonicalize()
            .map(crate::plain_path)
            .map_err(|source| Error::io("resolve scope environment", source))?;
        if !canonical.starts_with(data_canonical) {
            return Err(Error::InvalidMessage(
                "path_outside_project: managed environment escapes engine data directory".into(),
            ));
        }
    }
    if let Some(path) = Store::open(data)?.get_scope_environment(project, &scope.id)? {
        if Path::new(&path) != expected {
            return Err(Error::InvalidMessage(
                "scope environment binding does not match its declared ownership".into(),
            ));
        }
    }
    Ok(expected)
}

fn validate_lockfile_authority(
    root: &Path,
    directory: &Path,
    owner: &Path,
    manager: ManagerKind,
) -> Result<()> {
    let root = root
        .canonicalize()
        .map(crate::plain_path)
        .map_err(|source| Error::io("resolve lockfile project", source))?;
    let mut ancestor = owner.parent();
    while let Some(path) = ancestor.filter(|path| path.starts_with(&root)) {
        let workspace = match manager {
            ManagerKind::Pnpm => path.join("pnpm-workspace.yaml").is_file(),
            ManagerKind::Npm => {
                let package = path.join("package.json");
                if package.is_file() {
                    let content = std::fs::read(&package)
                        .map_err(|source| Error::io("read workspace authority", source))?;
                    serde_json::from_slice::<serde_json::Value>(&content)
                        .map_err(|error| {
                            Error::InvalidMessage(format!(
                                "manager_conflict: invalid ancestor package.json: {error}"
                            ))
                        })?
                        .get("workspaces")
                        .is_some()
                } else {
                    false
                }
            }
            ManagerKind::Uv => {
                !owner.join("pyproject.toml").is_file() && path.join("pyproject.toml").is_file()
            }
            _ => false,
        };
        if workspace {
            return Err(Error::InvalidMessage("manager_conflict: an ancestor workspace owns this directory; declare its lockfile root".into()));
        }
        if path == root {
            break;
        }
        ancestor = path.parent();
    }
    let names: &[&str] = match manager {
        ManagerKind::Npm | ManagerKind::Pnpm => {
            &["package-lock.json", "npm-shrinkwrap.json", "pnpm-lock.yaml"]
        }
        ManagerKind::Pip | ManagerKind::Uv => &["uv.lock"],
        ManagerKind::Composer => &["composer.lock"],
    };
    let metadata: &[&str] = match manager {
        ManagerKind::Npm | ManagerKind::Pnpm => &["package.json", "pnpm-workspace.yaml"],
        ManagerKind::Pip | ManagerKind::Uv => &["pyproject.toml", "requirements.txt"],
        ManagerKind::Composer => &["composer.json"],
    };
    let mut current = directory;
    loop {
        for name in metadata {
            let file = current.join(name);
            if file.exists() {
                let canonical = file
                    .canonicalize()
                    .map(crate::plain_path)
                    .map_err(|source| Error::io("resolve scope dependency metadata", source))?;
                if !canonical.starts_with(owner) {
                    return Err(Error::InvalidMessage(
                        "path_outside_project: dependency metadata escapes its declared owner"
                            .into(),
                    ));
                }
            }
        }
        for name in names {
            let file = current.join(name);
            if file.exists() {
                let canonical = file
                    .canonicalize()
                    .map(crate::plain_path)
                    .map_err(|source| Error::io("resolve scope lockfile", source))?;
                if !canonical.starts_with(owner) || current != owner {
                    return Err(Error::InvalidMessage("manager_conflict: actual lockfile authority differs from the declared lockfile root".into()));
                }
            }
        }
        if current == owner {
            break;
        }
        current = current.parent().ok_or_else(|| {
            Error::InvalidMessage("manager_conflict: missing lockfile owner".into())
        })?;
    }
    Ok(())
}

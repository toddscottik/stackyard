use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::deps::resolve_manager;
use crate::error::{Error, Result};
use crate::install::DEFAULT_PROJECT;
use crate::manifest::{load_manifest, ManagerKind, Manifest, RuntimeRole, ServiceSpec};
use crate::store::{PackageRecord, ProcessRecord, ProjectRecord, Store};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceView {
    pub id: String,
    pub runtime: String,
    pub state: String,
    pub pid: Option<u32>,
    pub port: Option<u16>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionView {
    pub role: String,
    pub package_id: String,
    pub version: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectView {
    pub id: String,
    pub name: String,
    pub root: String,
    pub manager: String,
    pub manager_detail: String,
    pub selections: Vec<SelectionView>,
    pub services: Vec<ServiceView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub working_directory: PathBuf,
    pub env: Vec<(String, String)>,
}

pub fn environment_dir(data_dir: &Path, project_id: &str) -> PathBuf {
    data_dir.join("environments").join(project_id)
}

pub fn venv_python(env_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        env_dir.join("Scripts").join("python.exe")
    } else {
        env_dir.join("bin").join("python")
    }
}

pub fn import_project(data_dir: &Path, path: &Path) -> Result<ProjectView> {
    let root = path
        .canonicalize()
        .map_err(|source| Error::io("read project directory", source))?;
    if !root.is_dir() {
        return Err(Error::InvalidManifest {
            detail: format!("{} is not a directory", root.display()),
        });
    }
    let manifest = load_manifest(&root)?;
    let store = Store::open(data_dir)?;
    let root_text = root.display().to_string();
    if let Some(existing) = store.project_by_root(&root_text)? {
        store.rename_project(&existing.id, &manifest.name)?;
        store.record_event("project_import", &existing.id)?;
        return project_view(data_dir, &existing.id);
    }
    if store.get_project(&manifest.name)?.is_some() {
        return Err(Error::InvalidManifest {
            detail: format!(
                "project id {} is already imported from another directory",
                manifest.name
            ),
        });
    }
    store.insert_project(&ProjectRecord {
        id: manifest.name.clone(),
        name: manifest.name.clone(),
        root: root_text,
    })?;
    store.record_event("project_import", &manifest.name)?;
    project_view(data_dir, &manifest.name)
}

pub fn project_view(data_dir: &Path, id: &str) -> Result<ProjectView> {
    let store = Store::open(data_dir)?;
    let project = store
        .get_project(id)?
        .ok_or_else(|| Error::ProjectNotFound { id: id.to_string() })?;
    view_one(&store, &project)
}

pub fn list_project_views(data_dir: &Path) -> Result<Vec<ProjectView>> {
    let store = Store::open(data_dir)?;
    let mut views = Vec::new();
    for project in store.list_projects()? {
        views.push(match view_one(&store, &project) {
            Ok(view) => view,
            Err(err) => ProjectView {
                id: project.id,
                name: project.name,
                root: project.root,
                manager: String::new(),
                manager_detail: String::new(),
                selections: Vec::new(),
                services: vec![ServiceView {
                    id: "-".to_string(),
                    runtime: String::new(),
                    state: "invalid".to_string(),
                    pid: None,
                    port: None,
                    detail: err.to_string(),
                }],
            },
        });
    }
    Ok(views)
}

pub fn require_startable(view: &ProjectView) -> Result<()> {
    if let Some(service) = view
        .services
        .iter()
        .find(|service| matches!(service.state.as_str(), "starting" | "running" | "orphaned"))
    {
        return Err(Error::ProjectBusy {
            id: view.id.clone(),
            state: service.state.clone(),
        });
    }
    Ok(())
}

pub fn resolve_runtime(
    data_dir: &Path,
    project_id: &str,
    role: RuntimeRole,
) -> Result<PackageRecord> {
    let store = Store::open(data_dir)?;
    let role_name = role.as_str();
    let package_id = store
        .get_selection(project_id, role_name)?
        .or(store.get_selection(DEFAULT_PROJECT, role_name)?)
        .ok_or_else(|| {
            Error::InvalidMessage(format!(
                "select a {role_name} runtime for project {project_id}"
            ))
        })?;
    store
        .get_package(&package_id)?
        .ok_or_else(|| Error::PackageNotFound { id: package_id })
}

pub fn resolve_tool(
    data_dir: &Path,
    project_id: &str,
    name: &str,
) -> Result<Option<PackageRecord>> {
    let store = Store::open(data_dir)?;
    let Some(package_id) = store
        .get_selection(project_id, name)?
        .or(store.get_selection(DEFAULT_PROJECT, name)?)
    else {
        return Ok(None);
    };
    Ok(Some(store.get_package(&package_id)?.ok_or_else(|| {
        Error::PackageNotFound { id: package_id }
    })?))
}

pub fn load_project_manifest(
    data_dir: &Path,
    project_id: &str,
) -> Result<(ProjectRecord, Manifest)> {
    let store = Store::open(data_dir)?;
    let project = store
        .get_project(project_id)?
        .ok_or_else(|| Error::ProjectNotFound {
            id: project_id.to_string(),
        })?;
    let manifest = load_manifest(Path::new(&project.root))?;
    Ok((project, manifest))
}

pub fn service_launch(
    data_dir: &Path,
    project: &ProjectRecord,
    service: &ServiceSpec,
    port: u16,
    command: &[String],
) -> Result<Launch> {
    let root = PathBuf::from(&project.root);
    let working_directory = contained_dir(&root, &service.working_directory)?;
    let runtime = resolve_runtime(data_dir, &project.id, service.runtime)?;
    let runtime_path = PathBuf::from(&runtime.launch);
    let python = if service.runtime == RuntimeRole::Python {
        let venv = venv_python(&environment_dir(data_dir, &project.id));
        if venv.is_file() {
            venv
        } else {
            runtime_path.clone()
        }
    } else {
        runtime_path.clone()
    };
    let pnpm = resolve_tool(data_dir, &project.id, "pnpm")?;
    let (program, args) = rewrite_command(
        &root,
        &working_directory,
        command,
        port,
        &runtime_path,
        &python,
        pnpm.as_ref()
            .map(|package| PathBuf::from(&package.launch))
            .as_deref(),
    )?;
    let mut front = Vec::new();
    if let Some(parent) = python.parent() {
        if service.runtime == RuntimeRole::Python {
            front.push(parent.to_path_buf());
        }
    }
    if let Some(parent) = runtime_path.parent() {
        front.push(parent.to_path_buf());
    }
    front.push(working_directory.join("node_modules").join(".bin"));
    let mut env = service.env.clone();
    env.push(("HOST".to_string(), "127.0.0.1".to_string()));
    env.push(("PORT".to_string(), port.to_string()));
    env.push(("FLASK_RUN_HOST".to_string(), "127.0.0.1".to_string()));
    env.push(("FLASK_RUN_PORT".to_string(), port.to_string()));
    env.push(("STACKYARD_PROJECT".to_string(), project.id.clone()));
    env.push(("STACKYARD_SERVICE".to_string(), service.id.clone()));
    env.push(("PATH".to_string(), prepend_path(&front)));
    if service.runtime == RuntimeRole::Python && python != runtime_path {
        env.push((
            "VIRTUAL_ENV".to_string(),
            environment_dir(data_dir, &project.id).display().to_string(),
        ));
    }
    Ok(Launch {
        program,
        args,
        working_directory,
        env,
    })
}

pub fn ensure_venv(python: &Path, env_dir: &Path) -> Result<PathBuf> {
    let target = venv_python(env_dir);
    if target.is_file() {
        return Ok(target);
    }
    if env_dir.exists() {
        fs::remove_dir_all(env_dir)
            .map_err(|source| Error::io("remove partial virtual environment", source))?;
    }
    if let Some(parent) = env_dir.parent() {
        fs::create_dir_all(parent)
            .map_err(|source| Error::io("create environment directory", source))?;
    }
    let output = Command::new(python)
        .args(["-m", "venv"])
        .arg(env_dir)
        .output()
        .map_err(|source| Error::io("create virtual environment", source))?;
    if !output.status.success() || !target.is_file() {
        let detail = String::from_utf8_lossy(&output.stderr);
        let _ = fs::remove_dir_all(env_dir);
        return Err(Error::OperationFailed(format!(
            "could not create a virtual environment with {}: {}",
            python.display(),
            detail.trim()
        )));
    }
    Ok(target)
}

pub fn contained_dir(root: &Path, relative: &Path) -> Result<PathBuf> {
    if relative
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err(Error::InvalidManifest {
            detail: format!("{} escapes the project", relative.display()),
        });
    }
    let joined = if relative.as_os_str().is_empty() || relative == Path::new(".") {
        root.to_path_buf()
    } else {
        root.join(relative)
    };
    let canon = joined
        .canonicalize()
        .map_err(|source| Error::io("open service directory", source))?;
    let root_canon = root
        .canonicalize()
        .map_err(|source| Error::io("open project directory", source))?;
    if canon.starts_with(&root_canon) {
        Ok(canon)
    } else {
        Err(Error::InvalidManifest {
            detail: format!("{} is outside the project", joined.display()),
        })
    }
}

fn view_one(store: &Store, project: &ProjectRecord) -> Result<ProjectView> {
    let manifest = load_manifest(Path::new(&project.root))?;
    let processes = store.list_processes(&project.id)?;
    let services = manifest
        .services
        .iter()
        .map(|service| {
            service_view(
                service,
                processes.iter().find(|row| row.service_id == service.id),
            )
        })
        .collect();
    let (manager, manager_detail) = manager_fields(Path::new(&project.root), &manifest);
    Ok(ProjectView {
        id: project.id.clone(),
        name: manifest.name,
        root: project.root.clone(),
        manager,
        manager_detail,
        selections: project_selections(store, &project.id)?,
        services,
    })
}

fn manager_fields(root: &Path, manifest: &Manifest) -> (String, String) {
    match resolve_manager(root, manifest) {
        Ok(kind) => (
            kind.as_str().to_string(),
            manager_detail(root, manifest, kind),
        ),
        Err(err) => (String::new(), err.to_string()),
    }
}

fn manager_detail(root: &Path, manifest: &Manifest, kind: ManagerKind) -> String {
    if manifest.manager == Some(kind) {
        return format!("{} is chosen in stackyard.toml", kind.as_str());
    }
    match kind {
        ManagerKind::Npm => "npm because package-lock.json is present".to_string(),
        ManagerKind::Pnpm if root.join("pnpm-lock.yaml").is_file() => {
            "pnpm because pnpm-lock.yaml is present".to_string()
        }
        ManagerKind::Pnpm => "pnpm, the default for a Node project without a lockfile".to_string(),
        ManagerKind::Pip => "pip in this project's virtual environment".to_string(),
        ManagerKind::Uv => "uv because uv.lock is present".to_string(),
    }
}

fn project_selections(store: &Store, project_id: &str) -> Result<Vec<SelectionView>> {
    let mut found = Vec::new();
    for role in ["node", "python", "pnpm", "uv"] {
        let Some(package_id) = store
            .get_selection(project_id, role)?
            .or(store.get_selection(DEFAULT_PROJECT, role)?)
        else {
            continue;
        };
        let Some(package) = store.get_package(&package_id)? else {
            continue;
        };
        found.push(SelectionView {
            role: role.to_string(),
            package_id,
            version: package.version,
            source: package.source,
        });
    }
    Ok(found)
}

fn service_view(service: &ServiceSpec, row: Option<&ProcessRecord>) -> ServiceView {
    let Some(row) = row else {
        return ServiceView {
            id: service.id.clone(),
            runtime: service.runtime.as_str().to_string(),
            state: "stopped".to_string(),
            pid: None,
            port: None,
            detail: String::new(),
        };
    };
    ServiceView {
        id: service.id.clone(),
        runtime: service.runtime.as_str().to_string(),
        state: row.state.clone(),
        pid: row.pid.and_then(|pid| u32::try_from(pid).ok()),
        port: row.port.and_then(|port| u16::try_from(port).ok()),
        detail: row.detail.clone(),
    }
}

fn rewrite_command(
    root: &Path,
    working_directory: &Path,
    command: &[String],
    port: u16,
    node: &Path,
    python: &Path,
    pnpm: Option<&Path>,
) -> Result<(PathBuf, Vec<String>)> {
    let mut args: Vec<String> = command
        .iter()
        .map(|arg| {
            if arg == "{port}" {
                port.to_string()
            } else {
                arg.clone()
            }
        })
        .collect();
    let program_name = args.remove(0);
    match program_name.as_str() {
        "node" => Ok((node.to_path_buf(), args)),
        "python" | "python3" => Ok((python.to_path_buf(), args)),
        "npm" => {
            let (program, prefix) = npm_launch(node)?;
            let mut full = prefix;
            full.extend(args);
            Ok((program, full))
        }
        "pnpm" => {
            let Some(pnpm) = pnpm else {
                return Err(Error::InvalidMessage(
                    "select a pnpm tool before running a pnpm command".to_string(),
                ));
            };
            Ok((pnpm.to_path_buf(), args))
        }
        other => {
            let relative = Path::new(other);
            if relative.is_absolute()
                || relative
                    .components()
                    .any(|component| component == Component::ParentDir)
            {
                return Err(Error::InvalidManifest {
                    detail: format!("command {other:?} must be a project-relative program"),
                });
            }
            let candidate = working_directory.join(relative);
            let inside = root.join(relative);
            if candidate.is_file() {
                Ok((candidate, args))
            } else if inside.is_file() {
                Ok((inside, args))
            } else {
                Err(Error::InvalidMessage(format!(
                    "command {other:?} is not the selected runtime or a file in the project"
                )))
            }
        }
    }
}

pub fn npm_launch(node: &Path) -> Result<(PathBuf, Vec<String>)> {
    let Some(parent) = node.parent() else {
        return Err(Error::InvalidMessage(format!(
            "node runtime {} has no directory",
            node.display()
        )));
    };
    let cli_names = [
        parent
            .join("node_modules")
            .join("npm")
            .join("bin")
            .join("npm-cli.js"),
        parent
            .join("..")
            .join("lib")
            .join("node_modules")
            .join("npm")
            .join("bin")
            .join("npm-cli.js"),
    ];
    for cli in cli_names {
        if cli.is_file() {
            return Ok((node.to_path_buf(), vec![cli.display().to_string()]));
        }
    }
    let npm_name = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let npm = parent.join(npm_name);
    if npm.is_file() {
        return Ok((npm, Vec::new()));
    }
    Err(Error::InvalidMessage(format!(
        "npm was not found next to {}",
        node.display()
    )))
}

pub fn prepend_path(front: &[PathBuf]) -> String {
    let mut parts: Vec<String> = front
        .iter()
        .filter(|path| path.exists())
        .map(|path| path.display().to_string())
        .collect();
    if let Some(current) = std::env::var_os("PATH") {
        parts.push(current.to_string_lossy().into_owned());
    }
    let separator = if cfg!(windows) { ";" } else { ":" };
    parts.join(separator)
}

pub fn active_state(state: &str) -> bool {
    matches!(state, "starting" | "running" | "orphaned")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::prepare_data_dir;
    use std::fs;

    fn temp_dir() -> PathBuf {
        let path = crate::paths::disposable_dir("project");
        prepare_data_dir(&path).expect("dir")
    }

    #[test]
    fn import_keeps_the_manifest_id_and_rereads_it() {
        let data = temp_dir();
        let root = data.join("proj ect").join("пример");
        fs::create_dir_all(&root).expect("root");
        fs::write(
            root.join("stackyard.toml"),
            r#"
schema = 1
name = "node-http"
[[service]]
id = "web"
runtime = "node"
command = ["node", "server.js"]
"#,
        )
        .expect("manifest");
        let view = import_project(&data, &root).expect("import");
        assert_eq!(view.id, "node-http");
        assert!(view.root.contains("пример"));
        assert_eq!(view.manager, "pnpm");
        assert!(view.manager_detail.contains("default"));
        let again = import_project(&data, &root).expect("reimport");
        assert_eq!(again.id, view.id);
        let _ = fs::remove_dir_all(&data);
    }
}

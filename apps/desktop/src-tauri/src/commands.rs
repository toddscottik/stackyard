use std::path::PathBuf;

use serde::Serialize;
use stackyard_core::{default_data_dir, ensure_engine, roundtrip, Command, Outcome};
use tauri::{AppHandle, Manager};

use crate::open::{
    accept_editor, canonical_project_root, folder_program, loopback_url, quit_stops_projects,
    shell_program, spawn_detached,
};

async fn engine_dir() -> Result<PathBuf, String> {
    ensure_engine(&default_data_dir())
        .await
        .map_err(|err| err.to_string())
}

async fn call(command: Command) -> Result<Outcome, String> {
    let dir = engine_dir().await?;
    roundtrip(&dir, command)
        .await
        .map_err(|err| err.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusView {
    pid: u32,
    data_dir: String,
    api_version: u32,
    schema_version: u32,
    last_event: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageView {
    id: String,
    source: String,
    version: String,
    path: String,
    launch: String,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogView {
    id: String,
    name: String,
    version: String,
    target: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceView {
    id: String,
    runtime: String,
    state: String,
    pid: Option<u32>,
    port: Option<u16>,
    detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionView {
    role: String,
    package_id: String,
    version: String,
    source: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView {
    id: String,
    name: String,
    root: String,
    manager: String,
    manager_detail: String,
    selections: Vec<SelectionView>,
    services: Vec<ServiceView>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationView {
    id: String,
    catalog_id: String,
    state: String,
    bytes: u64,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskView {
    id: String,
    label: String,
    state: String,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogView {
    offset: u64,
    next_offset: u64,
    text: String,
}

fn project_view(project: stackyard_core::ProjectView) -> ProjectView {
    ProjectView {
        id: project.id,
        name: project.name,
        root: project.root,
        manager: project.manager,
        manager_detail: project.manager_detail,
        selections: project
            .selections
            .into_iter()
            .map(|item| SelectionView {
                role: item.role,
                package_id: item.package_id,
                version: item.version,
                source: item.source,
            })
            .collect(),
        services: project
            .services
            .into_iter()
            .map(|service| ServiceView {
                id: service.id,
                runtime: service.runtime,
                state: service.state,
                pid: service.pid,
                port: service.port,
                detail: service.detail,
            })
            .collect(),
    }
}

fn one_project(outcome: Outcome) -> Result<ProjectView, String> {
    match outcome {
        Outcome::Project { project } => Ok(project_view(project)),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn engine_status() -> Result<StatusView, String> {
    match call(Command::Status).await? {
        Outcome::Status {
            pid,
            data_dir,
            api_version,
            schema_version,
            last_event,
        } => Ok(StatusView {
            pid,
            data_dir,
            api_version,
            schema_version,
            last_event,
        }),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn engine_setting(key: String) -> Result<Option<String>, String> {
    match call(Command::GetSetting { key }).await? {
        Outcome::Setting { value, .. } => Ok(value),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn put_setting(key: String, value: String) -> Result<(), String> {
    call(Command::PutSetting { key, value }).await.map(|_| ())
}

#[tauri::command]
pub async fn list_packages() -> Result<Vec<PackageView>, String> {
    match call(Command::ListPackages).await? {
        Outcome::Packages { packages } => Ok(packages
            .into_iter()
            .map(|package| PackageView {
                id: package.id,
                name: package.name,
                source: package.source,
                version: package.version,
                path: package.path,
                launch: package.launch,
            })
            .collect()),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn list_catalog() -> Result<Vec<CatalogView>, String> {
    match call(Command::ListCatalog).await? {
        Outcome::Catalog { packages } => Ok(packages
            .into_iter()
            .map(|package| CatalogView {
                id: package.id,
                name: package.name,
                version: package.version,
                target: package.target,
            })
            .collect()),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn start_install(catalog_id: String) -> Result<OperationView, String> {
    operation(call(Command::StartInstall { catalog_id }).await?)
}

#[tauri::command]
pub async fn install_status(operation_id: String) -> Result<OperationView, String> {
    operation(call(Command::InstallStatus { operation_id }).await?)
}

#[tauri::command]
pub async fn cancel_install(operation_id: String) -> Result<OperationView, String> {
    operation(call(Command::CancelInstall { operation_id }).await?)
}

fn operation(outcome: Outcome) -> Result<OperationView, String> {
    match outcome {
        Outcome::Operation {
            id,
            catalog_id,
            state,
            bytes,
            message,
        } => Ok(OperationView {
            id,
            catalog_id,
            state,
            bytes,
            message,
        }),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn use_existing(role: String, path: String) -> Result<String, String> {
    match call(Command::UseExisting { role, path }).await? {
        Outcome::Packages { packages } => Ok(packages
            .first()
            .map(|package| format!("{} {}", package.version, package.path))
            .unwrap_or_else(|| "recorded".to_string())),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn select_package(project_id: String, package_id: String) -> Result<(), String> {
    call(Command::SelectPackage {
        project_id,
        package_id,
    })
    .await
    .map(|_| ())
}

#[tauri::command]
pub async fn list_projects() -> Result<Vec<ProjectView>, String> {
    match call(Command::ListProjects).await? {
        Outcome::Projects { projects } => Ok(projects.into_iter().map(project_view).collect()),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn import_project(path: String) -> Result<ProjectView, String> {
    one_project(call(Command::ImportProject { path }).await?)
}

#[tauri::command]
pub async fn project_status(project_id: String) -> Result<ProjectView, String> {
    one_project(call(Command::ProjectStatus { project_id }).await?)
}

#[tauri::command]
pub async fn start_project(project_id: String) -> Result<ProjectView, String> {
    one_project(call(Command::StartProject { project_id }).await?)
}

#[tauri::command]
pub async fn stop_project(project_id: String) -> Result<ProjectView, String> {
    one_project(call(Command::StopProject { project_id }).await?)
}

#[tauri::command]
pub async fn restart_project(project_id: String) -> Result<ProjectView, String> {
    one_project(call(Command::RestartProject { project_id }).await?)
}

#[tauri::command]
pub async fn cancel_start(project_id: String) -> Result<ProjectView, String> {
    one_project(call(Command::CancelStart { project_id }).await?)
}

#[tauri::command]
pub async fn read_logs(
    project_id: String,
    service_id: String,
    offset: u64,
) -> Result<LogView, String> {
    match call(Command::ReadLogs {
        project_id,
        service_id,
        offset,
    })
    .await?
    {
        Outcome::Logs {
            offset,
            next_offset,
            text,
            ..
        } => Ok(LogView {
            offset,
            next_offset,
            text,
        }),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn start_dependency(
    project_id: String,
    action: String,
    package: String,
    scope: String,
) -> Result<TaskView, String> {
    let command = match action.as_str() {
        "install" => Command::DepInstall { project_id, scope },
        "add" => Command::DepAdd {
            project_id,
            package,
            scope,
        },
        "remove" => Command::DepRemove {
            project_id,
            package,
            scope,
        },
        "update" => Command::DepUpdate {
            project_id,
            package,
            scope,
        },
        "list" => Command::DepList { project_id, scope },
        other => {
            return Err(format!(
                "dependency action {other:?} is not install, add, remove, update, or list"
            ))
        }
    };
    task(call(command).await?)
}

#[tauri::command]
pub async fn task_status(operation_id: String) -> Result<TaskView, String> {
    task(call(Command::DepStatus { operation_id }).await?)
}

#[tauri::command]
pub async fn cancel_task(operation_id: String) -> Result<TaskView, String> {
    task(call(Command::DepCancel { operation_id }).await?)
}

fn task(outcome: Outcome) -> Result<TaskView, String> {
    match outcome {
        Outcome::Task {
            id,
            label,
            state,
            message,
        } => Ok(TaskView {
            id,
            label,
            state,
            message,
        }),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn open_project(
    project_id: String,
    action: String,
    service_id: String,
) -> Result<(), String> {
    let project = one_project(call(Command::ProjectStatus { project_id }).await?)?;
    let root = canonical_project_root(&project.root)?;
    match action.as_str() {
        "folder" => {
            let (program, args) = folder_program(&root);
            spawn_detached(&program, &args, None)
        }
        "url" => {
            let service = project
                .services
                .iter()
                .find(|service| service.id == service_id)
                .ok_or_else(|| format!("service {service_id} was not found"))?;
            let port = service
                .port
                .ok_or_else(|| "this service has no loopback port yet".to_string())?;
            let url = loopback_url(port);
            if cfg!(windows) {
                spawn_detached("explorer", &[url], None)
            } else if cfg!(target_os = "macos") {
                spawn_detached("open", &[url], None)
            } else {
                spawn_detached("xdg-open", &[url], None)
            }
        }
        "editor" => {
            let saved = match call(Command::GetSetting {
                key: "editor_command".to_string(),
            })
            .await?
            {
                Outcome::Setting { value, .. } => value.unwrap_or_default(),
                other => return Err(format!("unexpected engine reply: {other:?}")),
            };
            let program = accept_editor(&saved)?;
            spawn_detached(&program, &[root.display().to_string()], Some(&root))
        }
        "shell" => {
            let (program, args) = shell_program(&root);
            spawn_detached(&program, &args, Some(&root))
        }
        other => Err(format!(
            "open action {other:?} is not folder, url, editor, or shell"
        )),
    }
}

pub async fn quit_from_tray(app: &AppHandle) {
    let background = match call(Command::GetSetting {
        key: "background_mode".to_string(),
    })
    .await
    {
        Ok(Outcome::Setting { value, .. }) => value,
        _ => None,
    };
    if quit_stops_projects(background.as_deref()) {
        let _ = call(Command::Stop).await;
    }
    app.exit(0);
}

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

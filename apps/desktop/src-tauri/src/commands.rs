use std::path::PathBuf;

use serde::Serialize;
use std::time::Duration;

use stackyard_core::{default_data_dir, ensure_engine, roundtrip_within, Command, Outcome};
use tauri::{AppHandle, Manager};

use crate::open::{
    accept_editor, accept_site_url, canonical_project_root, folder_program, loopback_url,
    quit_stops_projects, shell_program, spawn_detached,
};

async fn engine_dir() -> Result<PathBuf, String> {
    ensure_engine(&default_data_dir())
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn php_status(
    project_id: String,
) -> Result<stackyard_core::php_project::PhpProjectView, String> {
    match call_within(Command::PhpStatus { project_id }, Duration::from_secs(45)).await? {
        Outcome::Php { view } => Ok(*view),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn php_apply_profile(
    project_id: String,
    extensions: Vec<String>,
) -> Result<stackyard_core::php_project::PhpProjectView, String> {
    match call_within(
        Command::PhpApplyProfile {
            project_id,
            extensions,
        },
        Duration::from_secs(120),
    )
    .await?
    {
        Outcome::Php { view } => Ok(*view),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn php_install_extension(
    project_id: String,
    catalog_id: String,
) -> Result<Outcome, String> {
    call_within(
        Command::PhpInstallExtension {
            project_id,
            catalog_id,
        },
        Duration::from_secs(45),
    )
    .await
}

#[tauri::command]
pub async fn memcached_probe(
    address: String,
    port: u16,
) -> Result<stackyard_core::memcached::MemcachedProbe, String> {
    let endpoint = stackyard_core::memcached::MemcachedEndpoint {
        address: address
            .parse()
            .map_err(|_| "Memcached requires a numeric IP address".to_string())?,
        port,
    };
    match call(Command::MemcachedProbe { endpoint }).await? {
        Outcome::Memcached { probe } => Ok(probe),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

async fn call(command: Command) -> Result<Outcome, String> {
    call_within(command, Duration::from_secs(5)).await
}

async fn query(command: Command) -> Result<Outcome, String> {
    roundtrip_within(&default_data_dir(), command, Duration::from_secs(5))
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn inspect_config(
    path: String,
) -> Result<stackyard_core::configuration::ConfigInspection, String> {
    match call(Command::InspectConfig { path }).await? {
        Outcome::ConfigInspection { inspection } => Ok(*inspection),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn preview_config(
    draft: stackyard_core::configuration::ConfigDraft,
) -> Result<stackyard_core::configuration::ConfigReview, String> {
    match call(Command::PreviewConfig {
        draft: Box::new(draft),
    })
    .await?
    {
        Outcome::ConfigReview { review } => Ok(*review),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn apply_config(
    draft: stackyard_core::configuration::ConfigDraft,
    fingerprint: String,
) -> Result<ProjectView, String> {
    one_project(
        call(Command::ApplyConfig {
            draft: Box::new(draft),
            fingerprint,
        })
        .await?,
    )
}

#[tauri::command]
pub async fn reload_project(project_id: String) -> Result<ProjectView, String> {
    one_project(call(Command::ReloadProject { project_id }).await?)
}

#[tauri::command]
pub async fn forget_project(project_id: String) -> Result<(), String> {
    match call(Command::ForgetProject { project_id }).await? {
        Outcome::ProjectForgotten { .. } => Ok(()),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub fn list_recipes() -> Vec<stackyard_core::recipes::RecipeInfo> {
    stackyard_core::recipes::recipes()
}

#[tauri::command]
pub async fn plan_create(
    request: stackyard_core::create::CreateRequest,
) -> Result<stackyard_core::create::CreatePlan, String> {
    match query(Command::PlanCreate { request }).await? {
        Outcome::CreatePlan { plan } => Ok(*plan),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn apply_create(plan: stackyard_core::create::CreatePlan) -> Result<TaskView, String> {
    task(call(Command::ApplyCreate { plan }).await?)
}

#[tauri::command]
pub async fn record_create_runtime(role: String, path: String) -> Result<String, String> {
    match call_within(
        Command::RecordCreateRuntime { role, path },
        Duration::from_secs(45),
    )
    .await?
    {
        Outcome::Packages { packages } => packages
            .into_iter()
            .next()
            .map(|package| package.id)
            .ok_or_else(|| "No recorded package was returned".to_string()),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

async fn cache_outcome(
    command: Command,
) -> Result<Vec<stackyard_core::CacheInstanceRecord>, String> {
    match call_within(command, Duration::from_secs(30)).await? {
        Outcome::Caches { instances } => Ok(instances),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn caches_status() -> Result<Vec<stackyard_core::CacheInstanceRecord>, String> {
    cache_outcome(Command::CacheList).await
}
#[tauri::command]
pub async fn cache_create(
    package_id: String,
    port: u16,
) -> Result<Vec<stackyard_core::CacheInstanceRecord>, String> {
    cache_outcome(Command::CacheCreate { package_id, port }).await
}
#[tauri::command]
pub async fn cache_start(
    instance_id: String,
) -> Result<Vec<stackyard_core::CacheInstanceRecord>, String> {
    cache_outcome(Command::CacheStart { instance_id }).await
}
#[tauri::command]
pub async fn cache_stop(
    instance_id: String,
) -> Result<Vec<stackyard_core::CacheInstanceRecord>, String> {
    cache_outcome(Command::CacheStop { instance_id }).await
}
#[tauri::command]
pub async fn cache_forget(
    instance_id: String,
) -> Result<Vec<stackyard_core::CacheInstanceRecord>, String> {
    cache_outcome(Command::CacheForget { instance_id }).await
}

async fn call_within(command: Command, limit: Duration) -> Result<Outcome, String> {
    let dir = engine_dir().await?;
    roundtrip_within(&dir, command, limit)
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
    scope_id: String,
    readiness_state: String,
    readiness: Option<stackyard_core::ReadinessSpec>,
    http_status: Option<u16>,
    endpoint: Option<EndpointView>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointView {
    protocol: String,
    requested_port: u16,
    resolved_port: Option<u16>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeView {
    id: String,
    directory: String,
    runtime: String,
    manager: String,
    manager_detail: String,
    selections: Vec<SelectionView>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionView {
    role: String,
    package_id: String,
    version: String,
    source: String,
    origin: String,
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
    scopes: Vec<ScopeView>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationView {
    id: String,
    catalog_id: String,
    state: String,
    bytes: u64,
    total: u64,
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

fn show_path(value: impl AsRef<str>) -> String {
    stackyard_core::plain_path(PathBuf::from(value.as_ref()))
        .display()
        .to_string()
}

fn project_view(project: stackyard_core::ProjectView) -> ProjectView {
    ProjectView {
        id: project.id,
        name: project.name,
        root: show_path(project.root),
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
                origin: item.origin,
            })
            .collect(),
        scopes: project
            .scopes
            .into_iter()
            .map(|s| ScopeView {
                id: s.id,
                directory: s.directory,
                runtime: s.runtime,
                manager: s.manager,
                manager_detail: s.manager_detail,
                selections: s
                    .selections
                    .into_iter()
                    .map(|v| SelectionView {
                        role: v.role,
                        package_id: v.package_id,
                        version: v.version,
                        source: v.source,
                        origin: v.origin,
                    })
                    .collect(),
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
                scope_id: service.scope_id,
                readiness_state: service.readiness_state,
                readiness: service.readiness,
                http_status: service.http_status,
                endpoint: service.endpoint.map(|e| EndpointView {
                    protocol: e.protocol,
                    requested_port: e.requested_port,
                    resolved_port: e.resolved_port,
                }),
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
    status_view(call(Command::Status).await?)
}

#[tauri::command]
pub async fn probe_engine() -> Result<StatusView, String> {
    status_view(query(Command::Status).await?)
}

fn status_view(outcome: Outcome) -> Result<StatusView, String> {
    match outcome {
        Outcome::Status {
            pid,
            data_dir,
            api_version,
            schema_version,
            last_event,
        } => Ok(StatusView {
            pid,
            data_dir: show_path(data_dir),
            api_version,
            schema_version,
            last_event,
        }),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn engine_setting(key: String) -> Result<Option<String>, String> {
    match query(Command::GetSetting { key }).await? {
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
    match query(Command::ListPackages).await? {
        Outcome::Packages { packages } => Ok(packages
            .into_iter()
            .map(|package| PackageView {
                id: package.id,
                name: package.name,
                source: package.source,
                version: package.version,
                path: show_path(package.path),
                launch: show_path(package.launch),
            })
            .collect()),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn list_catalog() -> Result<Vec<CatalogView>, String> {
    match query(Command::ListCatalog).await? {
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
    operation(query(Command::InstallStatus { operation_id }).await?)
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
            total,
            message,
        } => Ok(OperationView {
            id,
            catalog_id,
            state,
            bytes,
            total,
            message,
        }),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn use_existing(role: String, path: String) -> Result<String, String> {
    match call_within(Command::UseExisting { role, path }, Duration::from_secs(45)).await? {
        Outcome::Packages { packages } => Ok(packages
            .first()
            .map(|package| format!("{} {}", package.version, package.path))
            .unwrap_or_else(|| "recorded".to_string())),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn select_package(project_id: String, package_id: String) -> Result<(), String> {
    call_within(
        Command::SelectPackage {
            project_id,
            package_id,
        },
        Duration::from_secs(45),
    )
    .await
    .map(|_| ())
}

#[tauri::command]
pub async fn list_projects() -> Result<Vec<ProjectView>, String> {
    match query(Command::ListProjects).await? {
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
pub async fn dependency_overview(
    project_id: String,
    scope: String,
    scope_id: Option<String>,
) -> Result<stackyard_core::dependency_overview::DependencyOverview, String> {
    match query(Command::DependencyOverview {
        project_id,
        scope,
        scope_id,
    })
    .await?
    {
        Outcome::DependencyOverview { overview } => Ok(overview),
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
    task(query(Command::DepStatus { operation_id }).await?)
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

#[derive(Serialize)]
pub struct ActivityView {
    operations: Vec<OperationView>,
    tasks: Vec<TaskView>,
}

#[tauri::command]
pub async fn list_activity() -> Result<ActivityView, String> {
    match query(Command::ListActivity).await? {
        Outcome::Activity { operations, tasks } => Ok(ActivityView {
            operations: operations
                .into_iter()
                .map(operation)
                .collect::<Result<_, _>>()?,
            tasks: tasks.into_iter().map(task).collect::<Result<_, _>>()?,
        }),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FollowLogView {
    offset: u64,
    next_offset: u64,
    text: String,
    generation: String,
    reset: bool,
}

#[tauri::command]
pub async fn follow_logs(
    project_id: String,
    service_id: String,
    offset: u64,
    generation: Option<String>,
) -> Result<FollowLogView, String> {
    match query(Command::FollowLogs {
        project_id,
        service_id,
        offset,
        generation,
    })
    .await?
    {
        Outcome::FollowLogs {
            offset,
            next_offset,
            text,
            generation,
            reset,
        } => Ok(FollowLogView {
            offset,
            next_offset,
            text,
            generation,
            reset,
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
    let status = call(Command::ProjectStatus {
        project_id: project_id.clone(),
    })
    .await
    .and_then(one_project);
    let project = match status {
        Ok(project) => project,
        Err(error) if matches!(action.as_str(), "folder" | "editor" | "shell") => list_projects()
            .await?
            .into_iter()
            .find(|project| project.id == project_id)
            .ok_or(error)?,
        Err(error) => return Err(error),
    };
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
            if !service
                .endpoint
                .as_ref()
                .is_some_and(|e| e.protocol == "http")
            {
                return Err("this service has no HTTP endpoint".into());
            }
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteLink {
    project_id: String,
    host: String,
    mode: String,
    upstream: String,
    http_url: String,
    https_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontView {
    state: String,
    pid: Option<u32>,
    http_port: u16,
    https_port: u16,
    restart_impact: String,
    ca_path: String,
    resolves_localhost: bool,
    sites: Vec<SiteLink>,
}

fn front_view(outcome: Outcome) -> Result<FrontView, String> {
    match outcome {
        Outcome::Front {
            state,
            pid,
            http_port,
            https_port,
            restart_impact,
            ca_path,
            resolves_localhost,
            sites,
        } => Ok(FrontView {
            state,
            pid,
            http_port,
            https_port,
            restart_impact,
            ca_path: show_path(ca_path),
            resolves_localhost,
            sites: sites
                .into_iter()
                .map(|site| SiteLink {
                    project_id: site.project_id,
                    host: site.host,
                    mode: site.mode,
                    upstream: site.upstream,
                    http_url: site.http_url,
                    https_url: site.https_url,
                })
                .collect(),
        }),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn front_status() -> Result<FrontView, String> {
    front_view(query(Command::FrontStatus).await?)
}

#[tauri::command]
pub async fn publish_project(project_id: String) -> Result<FrontView, String> {
    front_view(call(Command::PublishProject { project_id }).await?)
}

#[tauri::command]
pub async fn unpublish_project(project_id: String) -> Result<FrontView, String> {
    front_view(call(Command::UnpublishProject { project_id }).await?)
}

#[tauri::command]
pub async fn restart_front() -> Result<FrontView, String> {
    front_view(call(Command::RestartFront).await?)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbInstanceView {
    id: String,
    engine: String,
    port: i64,
    data_dir: String,
    state: String,
    pid: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbBindingView {
    project_id: String,
    connection_id: String,
    instance_id: String,
    engine: String,
    host: String,
    port: i64,
    database_name: String,
    user_name: String,
    external: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SqliteView {
    project_id: String,
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabasesView {
    instances: Vec<DbInstanceView>,
    bindings: Vec<DbBindingView>,
    sqlite: Vec<SqliteView>,
    message: String,
}

fn databases_view(outcome: Outcome) -> Result<DatabasesView, String> {
    match outcome {
        Outcome::Databases { view } => Ok(DatabasesView {
            instances: view
                .instances
                .into_iter()
                .map(|row| DbInstanceView {
                    id: row.id,
                    engine: row.engine,
                    port: row.port,
                    data_dir: show_path(row.data_dir),
                    state: row.state,
                    pid: row.pid,
                })
                .collect(),
            bindings: view
                .bindings
                .into_iter()
                .map(|row| DbBindingView {
                    project_id: row.project_id,
                    connection_id: row.connection_id,
                    instance_id: row.instance_id,
                    engine: row.engine,
                    host: row.host,
                    port: row.port,
                    database_name: row.database_name,
                    user_name: row.user_name,
                    external: row.external,
                })
                .collect(),
            sqlite: view
                .sqlite
                .into_iter()
                .map(|row| SqliteView {
                    project_id: row.project_id,
                    path: show_path(row.path),
                })
                .collect(),
            message: view.message,
        }),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn databases_status() -> Result<DatabasesView, String> {
    databases_view(query(Command::DbList).await?)
}

#[tauri::command]
pub async fn database_create(engine: String, port: u16) -> Result<DatabasesView, String> {
    databases_view(call_within(Command::DbCreate { engine, port }, Duration::from_secs(180)).await?)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredView {
    role: String,
    path: String,
}

#[tauri::command]
pub async fn discover_runtimes() -> Result<Vec<DiscoveredView>, String> {
    match call(Command::Discover).await? {
        Outcome::Discovered { items } => Ok(items
            .into_iter()
            .map(|item| DiscoveredView {
                role: item.role,
                path: show_path(item.path),
            })
            .collect()),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn database_start(instance_id: String) -> Result<DatabasesView, String> {
    databases_view(call_within(Command::DbStart { instance_id }, Duration::from_secs(180)).await?)
}

#[tauri::command]
pub async fn database_stop(instance_id: String) -> Result<DatabasesView, String> {
    databases_view(call(Command::DbStop { instance_id }).await?)
}

#[tauri::command]
pub async fn database_attach(
    project_id: String,
    engine: String,
    host: String,
    port: u16,
    database: String,
    user: String,
    password: String,
) -> Result<DatabasesView, String> {
    databases_view(
        call(Command::DbAttach {
            project_id,
            engine,
            host,
            port,
            database,
            user,
            password,
        })
        .await?,
    )
}

#[tauri::command]
pub async fn database_detach(
    project_id: String,
    connection_id: String,
) -> Result<DatabasesView, String> {
    databases_view(
        call(Command::DbDetach {
            project_id,
            connection_id,
        })
        .await?,
    )
}

#[tauri::command]
pub async fn sqlite_use(project_id: String, path: String) -> Result<DatabasesView, String> {
    databases_view(call(Command::SqliteUse { project_id, path }).await?)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNoticeView {
    kind: String,
    subject: String,
    detail: String,
}

fn update_notices(outcome: Outcome) -> Result<Vec<UpdateNoticeView>, String> {
    match outcome {
        Outcome::Updates { notices } => Ok(notices
            .into_iter()
            .map(|notice| UpdateNoticeView {
                kind: notice.kind,
                subject: notice.subject,
                detail: notice.detail,
            })
            .collect()),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn export_diagnostics() -> Result<String, String> {
    match call(Command::Diagnostics).await? {
        Outcome::Diagnostics { report } => Ok(report),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
pub async fn list_updates() -> Result<Vec<UpdateNoticeView>, String> {
    update_notices(call(Command::RefreshUpdates).await?)
}

#[tauri::command]
pub async fn cleanup_unused() -> Result<Vec<UpdateNoticeView>, String> {
    update_notices(call(Command::CleanupUnused).await?)
}

#[tauri::command]
pub async fn open_site(url: String) -> Result<(), String> {
    let url = accept_site_url(&url)?;
    if cfg!(windows) {
        spawn_detached("explorer", &[url], None)
    } else if cfg!(target_os = "macos") {
        spawn_detached("open", &[url], None)
    } else {
        spawn_detached("xdg-open", &[url], None)
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

// Preview deliberately bypasses engine autostart and opens existing state read-only.
#[tauri::command]
pub fn preview_project(
    draft: stackyard_core::preview::PreviewRequest,
) -> Result<stackyard_core::preview::ProjectPreview, String> {
    let store =
        stackyard_core::Store::open_read_only(&default_data_dir()).map_err(|e| e.to_string())?;
    stackyard_core::preview::preview_request(&draft, store.as_ref()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn select_scope_package(
    project_id: String,
    scope_id: String,
    package_id: String,
) -> Result<ProjectView, String> {
    one_project(
        call_within(
            Command::SelectScopePackage {
                project_id,
                scope_id,
                package_id,
            },
            Duration::from_secs(45),
        )
        .await?,
    )
}

#[tauri::command]
pub async fn start_dependency_scoped(
    project_id: String,
    scope_id: String,
    action: String,
    package: String,
) -> Result<TaskView, String> {
    let command = match action.as_str() {
        "install" => Command::DepInstallScoped {
            project_id,
            scope_id,
        },
        "list" => Command::DepListScoped {
            project_id,
            scope_id,
        },
        "recover" => Command::DepRecoverScoped {
            project_id,
            scope_id,
        },
        "add" => Command::DepAddScoped {
            project_id,
            scope_id,
            package,
        },
        "remove" => Command::DepRemoveScoped {
            project_id,
            scope_id,
            package,
        },
        "update" => Command::DepUpdateScoped {
            project_id,
            scope_id,
            package,
        },
        _ => return Err("unknown dependency action".into()),
    };
    task(call(command).await?)
}

#[tauri::command]
pub async fn php_status_scoped(
    project_id: String,
    scope_id: String,
) -> Result<stackyard_core::php_project::PhpProjectView, String> {
    match call_within(
        Command::PhpStatusScoped {
            project_id,
            scope_id,
        },
        Duration::from_secs(45),
    )
    .await?
    {
        Outcome::Php { view } => Ok(*view),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}
#[tauri::command]
pub async fn php_apply_profile_scoped(
    project_id: String,
    scope_id: String,
    extensions: Vec<String>,
) -> Result<stackyard_core::php_project::PhpProjectView, String> {
    match call_within(
        Command::PhpApplyProfileScoped {
            project_id,
            scope_id,
            extensions,
        },
        Duration::from_secs(120),
    )
    .await?
    {
        Outcome::Php { view } => Ok(*view),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}
#[tauri::command]
pub async fn php_install_extension_scoped(
    project_id: String,
    scope_id: String,
    catalog_id: String,
) -> Result<Outcome, String> {
    call_within(
        Command::PhpInstallExtensionScoped {
            project_id,
            scope_id,
            catalog_id,
        },
        Duration::from_secs(45),
    )
    .await
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    #[test]
    fn scope_endpoint_observations_survive_the_desktop_adapter() {
        let project = stackyard_core::ProjectView {
            id: "demo".into(),
            name: "demo".into(),
            root: ".".into(),
            manager: "".into(),
            manager_detail: "multiple scopes".into(),
            selections: vec![],
            scopes: vec![stackyard_core::ScopeView {
                id: "web".into(),
                directory: "web".into(),
                runtime: "node".into(),
                manager: "pnpm".into(),
                manager_detail: "explicit".into(),
                selections: vec![],
            }],
            services: vec![stackyard_core::ServiceView {
                id: "web".into(),
                runtime: "node".into(),
                state: "running".into(),
                pid: Some(20),
                port: Some(3100),
                detail: "ready".into(),
                scope_id: "web".into(),
                readiness_state: "ready".into(),
                readiness: Some(stackyard_core::ReadinessSpec::Http {
                    path: "/".into(),
                    statuses: vec![302],
                    timeout_secs: 15,
                }),
                http_status: Some(302),
                endpoint: Some(stackyard_core::EndpointView {
                    protocol: "http".into(),
                    requested_port: 0,
                    resolved_port: Some(3100),
                }),
            }],
        };
        let value = serde_json::to_value(project_view(project)).unwrap();
        assert_eq!(value["scopes"][0]["id"], "web");
        assert_eq!(value["services"][0]["scopeId"], "web");
        assert_eq!(value["services"][0]["httpStatus"], 302);
        assert_eq!(value["services"][0]["readinessState"], "ready");
        assert_eq!(value["services"][0]["endpoint"]["resolvedPort"], 3100);
        assert_eq!(value["services"][0]["endpoint"]["requestedPort"], 0);
        assert!(value["services"][0].get("scope_id").is_none());
    }
}

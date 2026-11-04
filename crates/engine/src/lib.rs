//! Per-user engine process. Desktop and CLI are clients of [`serve`].

mod cache;
mod control;
mod database;
mod endpoint;
mod front;
mod group;
mod jobs;
mod readiness;
mod supervisor;
mod tasks;

use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;

use jobs::Jobs;
use stackyard_core::{
    adopt_existing, catalog_for_host, check_api_version, cleanup_unused, discover_candidates,
    download_and_install, ensure_default_node_manager, export_diagnostics, find_package,
    import_project, list_project_views, mark_install_finished, mark_install_running,
    prepare_data_dir, project_view, reconcile_interrupted, record_external_version, recv,
    refresh_updates, remove_package, select_for_project, send, Command, DepAction, Error, Outcome,
    Progress, Request, Response, Result, Store, API_VERSION,
};
use supervisor::Supervisor;
use tasks::TaskBoard;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

pub use cache::Caches;
pub use front::Front;
pub use stackyard_core::Error as EngineError;

pub fn serve(data_dir: &Path) -> Result<()> {
    let data_dir = prepare_data_dir(data_dir)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|source| Error::io("create engine runtime", source))?;
    runtime.block_on(listen(data_dir))
}

async fn listen(data_dir: PathBuf) -> Result<()> {
    let store = Store::open(&data_dir)?;
    let jobs = Jobs::new();
    let tasks = TaskBoard::new();
    let supervisor = supervisor::new(data_dir.clone());
    let front = Arc::new(front::Front::new(data_dir.clone()));
    let databases = Arc::new(database::Databases::new(data_dir.clone()));
    let caches = cache::Caches::new(data_dir.clone());
    supervisor.reconcile()?;
    reconcile_interrupted(&data_dir)?;
    let mut endpoint = endpoint::Endpoint::bind(&data_dir)?;
    caches.reconcile()?;
    loop {
        let mut client = match endpoint.accept().await {
            Ok(client) => client,
            Err(err) => {
                eprintln!("engine accept: {err}");
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                continue;
            }
        };
        let stop = match handle_client(
            &mut client,
            &store,
            &data_dir,
            &jobs,
            &tasks,
            &supervisor,
            &front,
            &databases,
            &caches,
        )
        .await
        {
            Ok(stop) => stop,
            Err(err) => {
                eprintln!("engine client: {err}");
                false
            }
        };
        if stop {
            break;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle_client<S>(
    stream: &mut S,
    store: &Store,
    data_dir: &Path,
    jobs: &Arc<Jobs>,
    tasks: &Arc<TaskBoard>,
    supervisor: &Arc<Supervisor>,
    front: &Arc<front::Front>,
    databases: &Arc<database::Databases>,
    caches: &Arc<cache::Caches>,
) -> Result<bool>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let request: Request = {
        let mut reader = BufReader::new(&mut *stream);
        recv(&mut reader).await?
    };
    let response = match dispatch(
        &request, store, data_dir, jobs, tasks, supervisor, front, databases, caches,
    )
    .await
    {
        Ok(outcome) => Response {
            v: API_VERSION,
            id: request.id.clone(),
            result: Some(outcome),
            error: None,
            error_detail: None,
        },
        Err(err) => Response {
            v: API_VERSION,
            id: request.id.clone(),
            result: None,
            error_detail: Some(error_diagnostic(&err)),
            error: Some(err.to_string()),
        },
    };
    send(&mut *stream, &response).await?;
    stream
        .shutdown()
        .await
        .map_err(|source| Error::io("close engine reply", source))?;
    Ok(matches!(request.cmd, Command::Stop) && response.error.is_none())
}

#[allow(clippy::too_many_arguments)]
async fn dispatch(
    request: &Request,
    store: &Store,
    data_dir: &Path,
    jobs: &Arc<Jobs>,
    tasks: &Arc<TaskBoard>,
    supervisor: &Arc<Supervisor>,
    front: &front::Front,
    databases: &database::Databases,
    caches: &Arc<cache::Caches>,
) -> Result<Outcome> {
    check_api_version(request.v)?;
    match &request.cmd {
        Command::Status => Ok(Outcome::Status {
            pid: process::id(),
            data_dir: stackyard_core::plain_path(data_dir.to_path_buf())
                .display()
                .to_string(),
            api_version: API_VERSION,
            schema_version: store.schema_version()?,
            last_event: store.last_event()?,
        }),
        Command::GetSetting { key } => Ok(Outcome::Setting {
            key: key.clone(),
            value: store.get(key)?,
        }),
        Command::PutSetting { key, value } => {
            store.put(key, value)?;
            Ok(Outcome::Setting {
                key: key.clone(),
                value: Some(value.clone()),
            })
        }
        Command::Stop => {
            caches.shutdown();
            databases.shutdown();
            front.shutdown();
            supervisor.shutdown();
            Ok(Outcome::Stopped)
        }
        Command::ListCatalog => Ok(Outcome::Catalog {
            packages: catalog_for_host()?,
        }),
        Command::ListPackages => Ok(Outcome::Packages {
            packages: store.list_packages()?,
        }),
        Command::PhpStatus { project_id } => Ok(Outcome::Php {
            view: Box::new(stackyard_core::php_project::php_project_view(
                data_dir, project_id,
            )?),
        }),
        Command::PhpApplyProfile {
            project_id,
            extensions,
        } => Ok(Outcome::Php {
            view: Box::new(supervisor.reconfigure_php(project_id, || {
                stackyard_core::php_project::apply_project_php_profile(
                    data_dir, project_id, extensions,
                )
            })?),
        }),
        Command::PhpInstallExtension {
            project_id,
            catalog_id,
        } => {
            let view = stackyard_core::php_project::php_project_view(data_dir, project_id)?;
            let item = view
                .available
                .iter()
                .find(|item| item.package.id == *catalog_id)
                .ok_or_else(|| {
                    Error::InvalidMessage(format!(
                        "extension {catalog_id} has no approved artifact for this PHP ABI"
                    ))
                })?;
            start_package_install(jobs, data_dir, item.package.clone())
        }
        Command::MemcachedProbe { endpoint } => Ok(Outcome::Memcached {
            probe: stackyard_core::memcached::probe_memcached(
                *endpoint,
                std::time::Duration::from_secs(3),
            )?,
        }),
        Command::CacheList => caches.list(),
        Command::CacheCreate { package_id, port } => caches.create(package_id, *port),
        Command::CacheStart { instance_id } => caches.start(instance_id),
        Command::CacheStop { instance_id } => caches.stop(instance_id),
        Command::CacheForget { instance_id } => caches.forget(instance_id),
        Command::Discover => Ok(Outcome::Discovered {
            items: discover_candidates()
                .into_iter()
                .map(|(role, path)| stackyard_core::DiscoveredRuntime {
                    role,
                    path: stackyard_core::plain_path(path).display().to_string(),
                })
                .collect(),
        }),
        Command::StartInstall { catalog_id } => start_install(jobs, data_dir, catalog_id),
        Command::InstallStatus { operation_id } => operation_outcome(jobs, operation_id),
        Command::CancelInstall { operation_id } => {
            if !jobs.cancel(operation_id) {
                return Err(Error::InvalidMessage(format!(
                    "install {operation_id} was not found"
                )));
            }
            operation_outcome(jobs, operation_id)
        }
        Command::UseExisting { role, path } => {
            let record = adopt_existing(data_dir, role, Path::new(path))?;
            Ok(Outcome::Packages {
                packages: vec![record],
            })
        }
        Command::SelectPackage {
            project_id,
            package_id,
        } => {
            select_for_project(data_dir, project_id, package_id)?;
            Ok(Outcome::Packages {
                packages: store.list_packages()?,
            })
        }
        Command::Diagnostics => Ok(Outcome::Diagnostics {
            report: export_diagnostics(data_dir)?,
        }),
        Command::RefreshUpdates => Ok(Outcome::Updates {
            notices: refresh_updates(data_dir)?,
        }),
        Command::CleanupUnused => Ok(Outcome::Updates {
            notices: cleanup_unused(data_dir)?,
        }),
        Command::NoteExternal {
            package_id,
            observed_version,
        } => Ok(Outcome::Updates {
            notices: record_external_version(data_dir, package_id, observed_version)?,
        }),
        Command::DepRecover { project_id, scope } => {
            control::start_recovery(tasks, data_dir, project_id, scope)
        }
        Command::RemovePackage { package_id } => {
            remove_package(data_dir, package_id)?;
            Ok(Outcome::Packages {
                packages: store.list_packages()?,
            })
        }
        Command::ImportProject { path } => Ok(Outcome::Project {
            project: import_project(data_dir, Path::new(path))?,
        }),
        Command::ListProjects => Ok(Outcome::Projects {
            projects: list_project_views(data_dir)?,
        }),
        Command::SelectScopePackage {
            project_id,
            scope_id,
            package_id,
        } => {
            stackyard_core::scopes::select_for_scope(data_dir, project_id, scope_id, package_id)?;
            Ok(Outcome::Project {
                project: project_view(data_dir, project_id)?,
            })
        }
        Command::PreviewProject { draft } => Ok(Outcome::ProjectPreview {
            preview: stackyard_core::preview::preview_request(draft, Some(store))?,
        }),
        Command::DepInstallScoped {
            project_id,
            scope_id,
        } => control::start_dependency_scoped(
            tasks,
            data_dir,
            project_id,
            DepAction::Install,
            None,
            scope_id,
        ),
        Command::DepAddScoped {
            project_id,
            scope_id,
            package,
        } => control::start_dependency_scoped(
            tasks,
            data_dir,
            project_id,
            DepAction::Add,
            Some(package),
            scope_id,
        ),
        Command::DepRemoveScoped {
            project_id,
            scope_id,
            package,
        } => control::start_dependency_scoped(
            tasks,
            data_dir,
            project_id,
            DepAction::Remove,
            Some(package),
            scope_id,
        ),
        Command::DepUpdateScoped {
            project_id,
            scope_id,
            package,
        } => control::start_dependency_scoped(
            tasks,
            data_dir,
            project_id,
            DepAction::Update,
            Some(package),
            scope_id,
        ),
        Command::DepListScoped {
            project_id,
            scope_id,
        } => control::start_dependency_scoped(
            tasks,
            data_dir,
            project_id,
            DepAction::List,
            None,
            scope_id,
        ),
        Command::DepRecoverScoped {
            project_id,
            scope_id,
        } => control::start_recovery_scoped(tasks, data_dir, project_id, scope_id),
        Command::PhpStatusScoped {
            project_id,
            scope_id,
        } => Ok(Outcome::Php {
            view: Box::new(stackyard_core::php_project::php_scope_view(
                data_dir,
                project_id,
                Some(scope_id),
            )?),
        }),
        Command::PhpApplyProfileScoped {
            project_id,
            scope_id,
            extensions,
        } => Ok(Outcome::Php {
            view: Box::new(supervisor.reconfigure_php_scope(project_id, scope_id, || {
                stackyard_core::php_project::apply_scope_php_profile(
                    data_dir,
                    project_id,
                    Some(scope_id),
                    extensions,
                )
            })?),
        }),
        Command::PhpInstallExtensionScoped {
            project_id,
            scope_id,
            catalog_id,
        } => {
            let view =
                stackyard_core::php_project::php_scope_view(data_dir, project_id, Some(scope_id))?;
            let item = view
                .available
                .into_iter()
                .find(|entry| entry.package.id == *catalog_id)
                .ok_or_else(|| {
                    Error::InvalidMessage(
                        "profile_incompatible: no approved extension for this PHP scope".into(),
                    )
                })?;
            start_package_install(jobs, data_dir, item.package)
        }
        Command::ProjectStatus { project_id } => Ok(Outcome::Project {
            project: project_view(data_dir, project_id)?,
        }),
        Command::StartProject { project_id } => Ok(Outcome::Project {
            project: supervisor.start(project_id)?,
        }),
        Command::StopProject { project_id } => Ok(Outcome::Project {
            project: supervisor.stop(project_id)?,
        }),
        Command::RestartProject { project_id } => Ok(Outcome::Project {
            project: supervisor.restart(project_id)?,
        }),
        Command::CancelStart { project_id } => {
            supervisor.cancel(project_id)?;
            Ok(Outcome::Project {
                project: project_view(data_dir, project_id)?,
            })
        }
        Command::ReadLogs {
            project_id,
            service_id,
            offset,
        } => control::read_service_log(data_dir, project_id, service_id, *offset),
        Command::Exec {
            project_id,
            service_id,
            args,
        } => control::start_exec(tasks, data_dir, project_id, service_id, args),
        Command::DepInstall { project_id, scope } => {
            control::start_dependency(tasks, data_dir, project_id, DepAction::Install, None, scope)
        }
        Command::DepAdd {
            project_id,
            package,
            scope,
        } => control::start_dependency(
            tasks,
            data_dir,
            project_id,
            DepAction::Add,
            Some(package),
            scope,
        ),
        Command::DepRemove {
            project_id,
            package,
            scope,
        } => control::start_dependency(
            tasks,
            data_dir,
            project_id,
            DepAction::Remove,
            Some(package),
            scope,
        ),
        Command::DepUpdate {
            project_id,
            package,
            scope,
        } => control::start_dependency(
            tasks,
            data_dir,
            project_id,
            DepAction::Update,
            Some(package),
            scope,
        ),
        Command::DepList { project_id, scope } => {
            control::start_dependency(tasks, data_dir, project_id, DepAction::List, None, scope)
        }
        Command::DepStatus { operation_id } => control::task_outcome(tasks, operation_id),
        Command::DepCancel { operation_id } => {
            if !tasks.cancel(operation_id) {
                return Err(Error::InvalidMessage(format!(
                    "task {operation_id} was not found"
                )));
            }
            control::task_outcome(tasks, operation_id)
        }
        Command::FrontStatus => front.status(),
        Command::PublishProject { project_id } => front.publish(project_id),
        Command::UnpublishProject { project_id } => front.unpublish(project_id),
        Command::RestartFront => front.restart(),
        Command::SetFrontPorts {
            http_port,
            https_port,
        } => front.set_ports(*http_port, *https_port),
        Command::TrustInstall { directory } => front.trust_install(Path::new(directory)),
        Command::TrustRemove { directory } => front.trust_remove(Path::new(directory)),
        Command::WriteHosts { path } => front.write_hosts(Path::new(path)),
        Command::DbList => databases.list(),
        Command::DbCreate { engine, port } => databases.create(engine, *port),
        Command::DbStart { instance_id } => databases.start(instance_id),
        Command::DbStop { instance_id } => databases.stop(instance_id),
        Command::DbAdd {
            instance_id,
            project_id,
            name,
        } => databases.add(instance_id, project_id, name),
        Command::DbBackup {
            instance_id,
            database,
            destination,
        } => databases.backup(instance_id, database, Path::new(destination)),
        Command::DbRestore {
            instance_id,
            database,
            source,
            into,
        } => databases.restore(instance_id, database, Path::new(source), into),
        Command::DbAttach {
            project_id,
            engine,
            host,
            port,
            database,
            user,
            password,
        } => databases.attach(project_id, engine, host, *port, database, user, password),
        Command::DbDetach {
            project_id,
            connection_id,
        } => databases.detach(project_id, connection_id),
        Command::DbProbe { project_id, name } => {
            let note = databases.probe(project_id, name)?;
            databases.list().map(|outcome| match outcome {
                Outcome::Databases { mut view } => {
                    view.message = note;
                    Outcome::Databases { view }
                }
                other => other,
            })
        }
        Command::DbMigrate => databases.migrate(),
        Command::SqliteUse { project_id, path } => databases.sqlite_use(project_id, path),
        Command::SqliteBackup {
            project_id,
            destination,
        } => databases.sqlite_backup(project_id, Path::new(destination)),
        Command::SqliteRestore {
            project_id,
            source,
            destination,
        } => databases.sqlite_restore(project_id, Path::new(source), Path::new(destination)),
    }
}

fn error_diagnostic(error: &Error) -> stackyard_core::preview::Diagnostic {
    let code = match error {
        Error::InvalidManifest { .. } => "manifest_invalid",
        Error::PackageNotFound { .. } => "package_missing",
        Error::ProjectNotFound { .. } => "project_missing",
        Error::ProjectBusy { .. } => "project_busy",
        Error::PortInUse { .. } | Error::FrontPortBusy { .. } => "port_in_use",
        Error::Cancelled { .. } => "cancelled",
        Error::InvalidMessage(message) => match message.split(':').next().unwrap_or("") {
            "scope_unknown" => "scope_unknown",
            "scope_required" => "scope_required",
            "path_outside_project" => "path_outside_project",
            "manager_conflict" => "manager_conflict",
            "runtime_missing" => "runtime_missing",
            "tool_missing" => "tool_missing",
            "profile_incompatible" => "profile_incompatible",
            _ => "invalid_request",
        },
        _ => "operation_failed",
    };
    stackyard_core::preview::Diagnostic::error(
        code,
        "request",
        if matches!(error, Error::InvalidManifest { .. }) {
            "Manifest is invalid. Check the affected fields.".into()
        } else {
            error.to_string()
        },
    )
}

fn start_install(jobs: &Arc<Jobs>, data_dir: &Path, catalog_id: &str) -> Result<Outcome> {
    if let Some(existing) = jobs.running_for(catalog_id) {
        return operation_outcome(jobs, &existing);
    }
    let package = find_package(catalog_id)?;
    start_package_install(jobs, data_dir, package)
}

fn start_package_install(
    jobs: &Arc<Jobs>,
    data_dir: &Path,
    package: stackyard_core::CatalogPackage,
) -> Result<Outcome> {
    if let Some(existing) = jobs.running_for(&package.id) {
        return operation_outcome(jobs, &existing);
    }
    mark_install_running(data_dir, &package.id)?;
    let (id, bytes, total, cancel) = jobs.start(package.id.clone());
    let progress = Progress {
        bytes,
        total,
        cancel,
    };
    let jobs_for_task = Arc::clone(jobs);
    let operation_id = id.clone();
    let directory = data_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let result = download_and_install(&directory, &package, &progress);
        if result.is_ok() && (package.name == "mysql" || package.name == "postgres") {
            if let Err(err) =
                select_for_project(&directory, stackyard_core::DEFAULT_PROJECT, &package.id)
            {
                eprintln!("database server was installed but not selected: {err}");
            }
        }
        if result.is_ok() && package.name == "node" {
            if let Err(err) = ensure_default_node_manager(&directory) {
                eprintln!("pnpm was not installed with Node: {err}");
            }
        }
        let (state, message) = match &result {
            Ok(_) => ("succeeded".to_string(), "installed".to_string()),
            Err(err) => ("failed".to_string(), err.to_string()),
        };
        let _ = mark_install_finished(&directory, &package.id, &state, &message);
        jobs_for_task.finish(&operation_id, result);
    });
    operation_outcome(jobs, &id)
}

fn operation_outcome(jobs: &Jobs, id: &str) -> Result<Outcome> {
    let Some(operation) = jobs.snapshot(id) else {
        return Err(Error::InvalidMessage(format!("install {id} was not found")));
    };
    Ok(Outcome::Operation {
        id: id.to_string(),
        catalog_id: operation.catalog_id,
        state: operation.state,
        bytes: operation.bytes.load(std::sync::atomic::Ordering::Relaxed),
        total: operation.total.load(std::sync::atomic::Ordering::Relaxed),
        message: operation.message,
    })
}

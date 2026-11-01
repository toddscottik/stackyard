//! Per-user engine process. Desktop and CLI are clients of [`serve`].

mod endpoint;
mod jobs;

use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;

use jobs::Jobs;
use stackyard_core::{
    adopt_existing, catalog_for_host, check_api_version, discover_candidates, download_and_install,
    ensure_default_node_manager, find_package, prepare_data_dir, recv, remove_package,
    select_for_project, send, Command, Error, Outcome, Progress, Request, Response, Result, Store,
    API_VERSION,
};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

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
    let mut endpoint = endpoint::Endpoint::bind(&data_dir)?;
    loop {
        let mut client = endpoint.accept().await?;
        let stop = match handle_client(&mut client, &store, &data_dir, &jobs).await {
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

async fn handle_client<S>(
    stream: &mut S,
    store: &Store,
    data_dir: &Path,
    jobs: &Arc<Jobs>,
) -> Result<bool>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let request: Request = {
        let mut reader = BufReader::new(&mut *stream);
        recv(&mut reader).await?
    };
    let response = match dispatch(&request, store, data_dir, jobs).await {
        Ok(outcome) => Response {
            v: API_VERSION,
            id: request.id.clone(),
            result: Some(outcome),
            error: None,
        },
        Err(err) => Response {
            v: API_VERSION,
            id: request.id.clone(),
            result: None,
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

async fn dispatch(
    request: &Request,
    store: &Store,
    data_dir: &Path,
    jobs: &Arc<Jobs>,
) -> Result<Outcome> {
    check_api_version(request.v)?;
    match &request.cmd {
        Command::Status => Ok(Outcome::Status {
            pid: process::id(),
            data_dir: data_dir.display().to_string(),
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
        Command::Stop => Ok(Outcome::Stopped),
        Command::ListCatalog => Ok(Outcome::Catalog {
            packages: catalog_for_host()?,
        }),
        Command::ListPackages => Ok(Outcome::Packages {
            packages: store.list_packages()?,
        }),
        Command::Discover => Ok(Outcome::Discovered {
            items: discover_candidates()
                .into_iter()
                .map(|(role, path)| stackyard_core::DiscoveredRuntime {
                    role,
                    path: path.display().to_string(),
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
        Command::RemovePackage { package_id } => {
            remove_package(data_dir, package_id)?;
            Ok(Outcome::Packages {
                packages: store.list_packages()?,
            })
        }
    }
}

fn start_install(jobs: &Arc<Jobs>, data_dir: &Path, catalog_id: &str) -> Result<Outcome> {
    if let Some(existing) = jobs.running_for(catalog_id) {
        return operation_outcome(jobs, &existing);
    }
    let package = find_package(catalog_id)?;
    let (id, bytes, cancel) = jobs.start(package.id.clone());
    let progress = Progress { bytes, cancel };
    let jobs_for_task = Arc::clone(jobs);
    let operation_id = id.clone();
    let directory = data_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let result = download_and_install(&directory, &package, &progress);
        if result.is_ok() && package.name == "node" {
            if let Err(err) = ensure_default_node_manager(&directory) {
                eprintln!("pnpm was not installed with Node: {err}");
            }
        }
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
        message: operation.message,
    })
}

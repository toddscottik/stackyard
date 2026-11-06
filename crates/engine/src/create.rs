use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use stackyard_core::create::{generate_create, verify_create_plan, CreatePlan};
use stackyard_core::{CatalogPackage, Error, Outcome, PackageRecord, ProjectRecord, Result, Store};

use crate::{control, jobs::Jobs, queue_package_install, tasks::TaskBoard};

pub fn start(
    data_dir: &Path,
    jobs: &Arc<Jobs>,
    tasks: &Arc<TaskBoard>,
    plan: &CreatePlan,
) -> Result<Outcome> {
    let claim = Claim::acquire(data_dir, &plan.request.name)?;
    let plan = verify_create_plan(data_dir, plan)?;
    Store::open(data_dir)?.validate_project_root_id(&plan.request.name, &plan.target)?;
    let (id, cancel) = tasks.start(
        format!("create project {}", plan.request.name),
        &plan.request.name,
    );
    let worker_id = id.clone();
    let data_dir = data_dir.to_path_buf();
    let jobs = Arc::clone(jobs);
    let board = Arc::clone(tasks);
    tokio::task::spawn_blocking(move || {
        let result = apply(&data_dir, &jobs, &plan, &cancel, |message| {
            board.progress(&worker_id, message);
        });
        drop(claim);
        match result {
            Ok(message) => board.finish(&worker_id, "succeeded", &message),
            Err(Error::Cancelled { .. }) => board.finish(
                &worker_id,
                "cancelled",
                "Creation cancelled. Verified packages and any published source remain available. Apply the reviewed plan again to retry.",
            ),
            Err(error) => board.finish(&worker_id, "failed", &error.to_string()),
        }
    });
    control::task_outcome(tasks, &id)
}

fn apply(
    data_dir: &Path,
    jobs: &Arc<Jobs>,
    plan: &CreatePlan,
    cancel: &AtomicBool,
    report: impl Fn(&str),
) -> Result<String> {
    let name = &plan.request.name;
    let store = Store::open(data_dir)?;
    for artifact in &plan.downloads {
        check_cancel(cancel, name)?;
        if cached(&store, artifact)? {
            continue;
        }
        report(&format!(
            "Installing {} {}",
            artifact.name, artifact.version
        ));
        let (operation, owned) = queue_package_install(jobs, data_dir, artifact.clone(), false)?;
        let deadline = Instant::now() + Duration::from_secs(900);
        loop {
            if cancel.load(Ordering::Relaxed) || Instant::now() >= deadline {
                // Leave a download that was already running for another caller alone.
                if owned {
                    jobs.cancel(&operation);
                }
                check_cancel(cancel, name)?;
                return Err(Error::OperationFailed(format!(
                    "Install of {} is still running. Check Activity before retrying creation.",
                    artifact.id
                )));
            }
            if cached(&store, artifact)? {
                break;
            }
            let snapshot = jobs.snapshot(&operation).ok_or_else(|| {
                Error::OperationFailed(format!("Install result for {} is unavailable", artifact.id))
            })?;
            match snapshot.state.as_str() {
                "running" => std::thread::sleep(Duration::from_millis(150)),
                "succeeded" => {
                    return Err(Error::OperationFailed(format!(
                        "Install succeeded but {} is missing from the package store",
                        artifact.id
                    )));
                }
                _ => {
                    return Err(Error::OperationFailed(format!(
                        "Install of {} {}. {}",
                        artifact.id, snapshot.state, snapshot.message
                    )));
                }
            }
        }
    }
    check_cancel(cancel, name)?;
    report("Generating source");
    let root = generate_create(data_dir, plan, cancel)?;
    check_cancel(cancel, name)?;
    report("Registering project");
    verify_create_plan(data_dir, plan)
        .and_then(|_| register(data_dir, plan, &root))
        .map_err(|error| {
        Error::OperationFailed(format!(
            "Source saved at {}. Registration failed: {error}. Apply the same reviewed plan to retry.",
            root.display()
        ))
        })?;
    Ok(format!(
        "Created {name} at {}. Install project dependencies, then start the web service.",
        root.display()
    ))
}

fn register(data_dir: &Path, plan: &CreatePlan, root: &Path) -> Result<()> {
    let store = Store::open(data_dir)?;
    let request = &plan.request;
    let mut packages: Vec<PackageRecord> = Vec::new();
    for choice in std::iter::once(&request.runtime).chain(request.tool.iter()) {
        let package = store
            .get_package(&choice.id)?
            .filter(|entry| Path::new(&entry.launch).is_file())
            .ok_or_else(|| Error::PackageNotFound {
                id: choice.id.clone(),
            })?;
        if choice.kind == "catalog" {
            let artifact = stackyard_core::find_package(&choice.id)?;
            require_verified_cache(&package, &artifact)?;
        }
        packages.push(package);
    }
    let project = ProjectRecord {
        id: request.name.clone(),
        name: request.name.clone(),
        root: stackyard_core::plain_path(root.to_path_buf())
            .display()
            .to_string(),
    };
    store.register_created_project(&project, &packages)
}

fn cached(store: &Store, artifact: &CatalogPackage) -> Result<bool> {
    let Some(package) = store.get_package(&artifact.id)? else {
        return Ok(false);
    };
    require_verified_cache(&package, artifact)?;
    Ok(true)
}

fn require_verified_cache(package: &PackageRecord, artifact: &CatalogPackage) -> Result<()> {
    if package.name == artifact.name
        && package.kind == artifact.kind
        && package.version == artifact.version
        && package.source == "managed"
        && package.target == artifact.target
        && package
            .digest
            .as_ref()
            .is_some_and(|hash| hash.eq_ignore_ascii_case(&artifact.sha256))
        && Path::new(&package.launch).is_file()
    {
        return Ok(());
    }
    Err(Error::OperationFailed(format!(
        "The recorded package {} does not match its verified catalog entry or its executable is missing",
        artifact.id
    )))
}

fn check_cancel(cancel: &AtomicBool, name: &str) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled { id: name.into() });
    }
    Ok(())
}

struct Claim((PathBuf, String));

impl Claim {
    fn acquire(data_dir: &Path, name: &str) -> Result<Self> {
        let key = (data_dir.to_path_buf(), name.to_string());
        if !claims()
            .lock()
            .expect("creation claims")
            .insert(key.clone())
        {
            return Err(Error::ProjectBusy {
                id: name.into(),
                state: "creation in progress".into(),
            });
        }
        Ok(Self(key))
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        claims().lock().expect("creation claims").remove(&self.0);
    }
}

fn claims() -> &'static Mutex<HashSet<(PathBuf, String)>> {
    static CLAIMS: OnceLock<Mutex<HashSet<(PathBuf, String)>>> = OnceLock::new();
    CLAIMS.get_or_init(Mutex::default)
}

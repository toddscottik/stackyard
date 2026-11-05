use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use stackyard_core::{
    log_tail, manifest_fingerprint, plan_dependency, project_view, read_log, service_launch,
    service_log_path, tool_version, DepAction, DepRequest, Error, Outcome, Result, Toolchain,
};

use crate::group;
use crate::tasks::TaskBoard;

pub fn read_service_log(
    data_dir: &Path,
    project_id: &str,
    service_id: &str,
    offset: u64,
) -> Result<Outcome> {
    if !valid_id(project_id) || !valid_id(service_id) {
        return Err(Error::InvalidMessage(
            "project and service ids must be lowercase identifiers".to_string(),
        ));
    }
    let path = service_log_path(data_dir, project_id, service_id);
    let chunk = read_log(&path, offset, 24_000)?;
    Ok(Outcome::Logs {
        project_id: project_id.to_string(),
        service_id: service_id.to_string(),
        offset: chunk.offset,
        next_offset: chunk.next_offset,
        text: chunk.text,
    })
}

pub fn follow_service_log(
    data_dir: &Path,
    project_id: &str,
    service_id: &str,
    offset: u64,
    generation: Option<&str>,
) -> Result<Outcome> {
    if !valid_id(project_id) || !valid_id(service_id) {
        return Err(Error::InvalidMessage(
            "project and service ids must be lowercase identifiers".to_string(),
        ));
    }
    let path = service_log_path(data_dir, project_id, service_id);
    // JSON escaping can multiply log bytes by six. Leave room for the envelope.
    let chunk = stackyard_core::read_log_follow(&path, offset, generation, 8_000)?;
    Ok(Outcome::FollowLogs {
        offset: chunk.offset,
        next_offset: chunk.next_offset,
        text: chunk.text,
        generation: chunk.generation,
        reset: chunk.reset,
    })
}

pub fn start_exec(
    tasks: &Arc<TaskBoard>,
    data_dir: &Path,
    project_id: &str,
    service_id: &str,
    args: &[String],
) -> Result<Outcome> {
    if args.is_empty() || args.iter().any(|arg| arg.is_empty() || arg.contains('\0')) {
        return Err(Error::InvalidMessage(
            "exec needs an argument list and does not use a shell".to_string(),
        ));
    }
    let (project, manifest) = stackyard_core::load_project_manifest(data_dir, project_id)?;
    let service = manifest
        .service(service_id)
        .cloned()
        .ok_or_else(|| Error::InvalidMessage(format!("service {service_id} was not found")))?;
    let port = project_view(data_dir, project_id)?
        .services
        .into_iter()
        .find(|item| item.id == service_id)
        .and_then(|item| item.port)
        .unwrap_or(0);
    let launch = service_launch(data_dir, &project, &service, port, args)?;
    let log_path = service_log_path(data_dir, project_id, &format!("{service_id}-exec"));
    let label = format!("exec {project_id} {service_id}");
    spawn_task(tasks, label, move |cancel| {
        run_launch(&launch, &log_path, &cancel, Duration::from_secs(20))
    })
}

pub fn start_recovery(
    tasks: &Arc<TaskBoard>,
    data_dir: &Path,
    project_id: &str,
    scope: &str,
) -> Result<Outcome> {
    let data_dir = data_dir.to_path_buf();
    let project_id = project_id.to_string();
    let scope = scope.to_string();
    let label = format!(
        "recover dependencies {project_id} from the lockfile; this does not roll back package scripts"
    );
    spawn_task(tasks, label, move |cancel| {
        let message = run_dependency(
            &data_dir,
            &project_id,
            DepAction::Install,
            None,
            &scope,
            None,
            &cancel,
        )?;
        Ok(format!(
            "recovered dependencies from the recorded lockfile. Package scripts were not rolled back. {message}"
        ))
    })
}

pub fn start_dependency(
    tasks: &Arc<TaskBoard>,
    data_dir: &Path,
    project_id: &str,
    action: DepAction,
    package: Option<&str>,
    scope: &str,
) -> Result<Outcome> {
    let data_dir = data_dir.to_path_buf();
    let project_id = project_id.to_string();
    let package = package.map(str::to_string);
    let scope = scope.to_string();
    let label = format!("deps {} {project_id}", action_name(action));
    let (id, cancel) = tasks.start(label.clone());
    let tasks_for_job = Arc::clone(tasks);
    let operation = id.clone();
    tokio::task::spawn_blocking(move || {
        let result = run_dependency(
            &data_dir,
            &project_id,
            action,
            package.as_deref(),
            &scope,
            None,
            &cancel,
        );
        match result {
            Ok(message) => tasks_for_job.finish(&operation, "succeeded", &message),
            Err(Error::Cancelled { .. }) => {
                tasks_for_job.finish(&operation, "cancelled", "cancelled")
            }
            Err(err) => tasks_for_job.finish(&operation, "failed", &err.to_string()),
        }
    });
    task_outcome(tasks, &id)
}

pub fn task_outcome(tasks: &TaskBoard, id: &str) -> Result<Outcome> {
    let Some(task) = tasks.snapshot(id) else {
        return Err(Error::InvalidMessage(format!("task {id} was not found")));
    };
    Ok(Outcome::Task {
        id: id.to_string(),
        label: task.label,
        state: task.state,
        message: task.message,
    })
}

fn spawn_task<F>(tasks: &Arc<TaskBoard>, label: String, work: F) -> Result<Outcome>
where
    F: FnOnce(Arc<AtomicBool>) -> std::result::Result<String, Error> + Send + 'static,
{
    let (id, cancel) = tasks.start(label);
    let tasks_for_job = Arc::clone(tasks);
    let operation = id.clone();
    tokio::task::spawn_blocking(move || match work(cancel) {
        Ok(message) => tasks_for_job.finish(&operation, "succeeded", &message),
        Err(Error::Cancelled { .. }) => tasks_for_job.finish(&operation, "cancelled", "cancelled"),
        Err(err) => tasks_for_job.finish(&operation, "failed", &err.to_string()),
    });
    task_outcome(tasks, &id)
}

fn run_launch(
    launch: &stackyard_core::Launch,
    log_path: &Path,
    cancel: &AtomicBool,
    limit: Duration,
) -> Result<String> {
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(Error::Cancelled {
            id: "exec".to_string(),
        });
    }
    let mut group = group::spawn_grouped_scoped(
        &launch.program,
        &launch.args,
        &launch.working_directory,
        &launch.env,
        &launch.env_remove,
        log_path,
    )?;
    let started = Instant::now();
    loop {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            group.request_stop();
            let _ = group.wait_timeout(Duration::from_secs(2));
            return Err(Error::Cancelled {
                id: "exec".to_string(),
            });
        }
        if let Some(status) = group
            .child
            .try_wait()
            .map_err(|source| Error::io("wait for command", source))?
        {
            group.finish_io();
            let output = log_tail(log_path, 4_000);
            if status.success() {
                return Ok(output);
            }
            return Err(Error::OperationFailed(format!("exited {status}\n{output}")));
        }
        if started.elapsed() > limit {
            group.force_stop();
            let _ = group.wait_timeout(Duration::from_secs(2));
            group.finish_io();
            let output = log_tail(log_path, 4_000);
            return Err(Error::OperationFailed(format!(
                "command exceeded {}s\n{output}",
                limit.as_secs()
            )));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn run_dependency(
    data_dir: &Path,
    project_id: &str,
    action: DepAction,
    package: Option<&str>,
    scope: &str,
    scope_id: Option<&str>,
    cancel: &AtomicBool,
) -> Result<String> {
    let (project, manifest) = stackyard_core::load_project_manifest(data_dir, project_id)?;
    let root = Path::new(&project.root);
    let resolved =
        stackyard_core::scopes::resolve_dependency_scope(root, &manifest, scope_id, scope)?;
    let env_dir = stackyard_core::scopes::resolved_environment(
        data_dir,
        project_id,
        root,
        &manifest,
        &resolved.spec,
    )?;
    let _guard = DependencyGuard::acquire(
        vec![resolved.lockfile_root.clone(), env_dir.clone()],
        cancel,
    )?;
    let runtime = stackyard_core::scopes::resolve_scope_runtime(
        data_dir,
        project_id,
        &resolved.spec.id,
        resolved.spec.runtime,
    )?;
    let runtime_path = PathBuf::from(runtime.launch);
    let node = (resolved.spec.runtime == stackyard_core::RuntimeRole::Node)
        .then_some(runtime_path.clone());
    let python = (resolved.spec.runtime == stackyard_core::RuntimeRole::Python)
        .then_some(runtime_path.clone());
    let php = (resolved.spec.runtime == stackyard_core::RuntimeRole::Php).then_some(runtime_path);
    let tool = |name| {
        stackyard_core::scopes::resolve_scope_tool(data_dir, project_id, &resolved.spec.id, name)
    };
    let composer = if resolved.manager == stackyard_core::ManagerKind::Composer {
        tool("composer")?
    } else {
        None
    };
    let pnpm = if resolved.manager == stackyard_core::ManagerKind::Pnpm {
        tool("pnpm")?
    } else {
        None
    };
    let uv = if resolved.manager == stackyard_core::ManagerKind::Uv {
        tool("uv")?
    } else {
        None
    };
    let venv = stackyard_core::venv_python(&env_dir);
    let venv_ref = venv.is_file().then_some(venv.as_path());
    let node_path = node.as_deref();
    let python_path = python.as_deref();
    let pnpm_path = pnpm
        .as_ref()
        .map(|package| Path::new(package.launch.as_str()));
    let uv_path = uv
        .as_ref()
        .map(|package| Path::new(package.launch.as_str()));
    let plan = plan_dependency(
        data_dir,
        project_id,
        root,
        &manifest,
        DepRequest {
            action,
            package,
            scope,
            scope_id,
        },
        Toolchain {
            node: node_path,
            python: python_path,
            pnpm: pnpm_path,
            uv: uv_path,
            venv_python: venv_ref,
            php: php.as_deref(),
            composer: composer.as_ref().map(|package| Path::new(&package.launch)),
        },
    )?;
    let before = manifest_fingerprint(&plan.scope_dir);
    let log_path = data_dir
        .join("logs")
        .join(project_id)
        .join(format!("dependencies-{}.log", resolved.spec.id));
    let mut version = String::new();
    for command in &plan.commands {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(Error::Cancelled {
                id: project_id.to_string(),
            });
        }
        if version.is_empty() {
            version = if plan.manager == stackyard_core::ManagerKind::Composer {
                stackyard_core::php_project::probe_composer(
                    php.as_deref().expect("validated PHP"),
                    composer
                        .as_ref()
                        .map(|package| Path::new(&package.launch))
                        .expect("validated Composer"),
                )?
            } else {
                tool_version(
                    &command.program,
                    &version_args(&command.program, &command.args),
                )
                .unwrap_or_else(|_| "unknown".to_string())
            };
        }
        let mut env = command.env.clone();
        let mut path_front = vec![
            command
                .program
                .parent()
                .unwrap_or(Path::new("."))
                .to_path_buf(),
            plan.scope_dir.join("node_modules").join(".bin"),
        ];
        if let Some(node) = &node {
            if let Some(parent) = node.parent() {
                path_front.push(parent.into());
            }
        }
        if let Some((_, value)) = command.env.iter().find(|(key, _)| key == "PATH") {
            path_front.extend(std::env::split_paths(value));
        }
        env.push((
            "PATH".to_string(),
            stackyard_core::prepend_path(&path_front),
        ));
        let _ = stackyard_core::append_log(
            &log_path,
            format!(
                "stackyard: {} {}\n",
                command.program.display(),
                command.args.join(" ")
            )
            .as_bytes(),
        );
        let status = run_command(command, &env, &log_path, cancel, Duration::from_secs(180))?;
        if !status.success() {
            let tail = log_tail(&log_path, 4_000);
            let partial = plan.scope_dir.join("node_modules").exists()
                || venv.is_dir()
                || plan.scope_dir.join("vendor").is_dir();
            return Err(Error::OperationFailed(format!(
                "{} {} failed ({status}). manifest changes may be partial: {partial}\n{tail}",
                plan.manager.as_str(),
                action_name(action)
            )));
        }
    }
    let after = manifest_fingerprint(&plan.scope_dir);
    let tail = log_tail(&log_path, 4_000);
    Ok(format!(
        "manager: {}\ntool: {version}\nscope: {}\nmanifest_changed: {}\n{}\n{tail}",
        plan.manager.as_str(),
        plan.scope_dir.display(),
        before != after,
        plan.note
    ))
}

fn run_command(
    command: &stackyard_core::PlannedCommand,
    env: &[(String, String)],
    log_path: &Path,
    cancel: &AtomicBool,
    limit: Duration,
) -> Result<std::process::ExitStatus> {
    let mut group = group::spawn_grouped_scoped(
        &command.program,
        &command.args,
        &command.working_directory,
        env,
        &command.env_remove,
        log_path,
    )?;
    let started = Instant::now();
    loop {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            group.request_stop();
            let _ = group.wait_timeout(Duration::from_secs(2));
            return Err(Error::Cancelled {
                id: "dependency".to_string(),
            });
        }
        if let Some(status) = group
            .child
            .try_wait()
            .map_err(|source| Error::io("wait for dependency command", source))?
        {
            group.finish_io();
            return Ok(status);
        }
        if started.elapsed() > limit {
            group.force_stop();
            return Err(Error::OperationFailed(
                "dependency command timed out; rerun install to recover the partial environment"
                    .to_string(),
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

static DEPENDENCY_WRITERS: OnceLock<Mutex<BTreeSet<PathBuf>>> = OnceLock::new();

struct DependencyGuard(Vec<PathBuf>);
impl DependencyGuard {
    fn acquire(mut keys: Vec<PathBuf>, cancel: &AtomicBool) -> Result<Self> {
        // Existing ancestors are canonicalized so absent managed environments and
        // junction aliases resolve to the same writer identity.
        for key in &mut keys {
            *key = canonical_identity(key)?;
        }
        loop {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(Error::Cancelled {
                    id: "dependency".into(),
                });
            }
            {
                let mut held = DEPENDENCY_WRITERS
                    .get_or_init(Default::default)
                    .lock()
                    .expect("dependency writer mutex");
                if keys.iter().all(|key| !held.contains(key)) {
                    held.extend(keys.iter().cloned());
                    return Ok(Self(keys));
                }
            }
            thread::sleep(Duration::from_millis(30));
        }
    }
}
impl Drop for DependencyGuard {
    fn drop(&mut self) {
        let mut held = DEPENDENCY_WRITERS
            .get_or_init(Default::default)
            .lock()
            .expect("dependency writer mutex");
        for key in &self.0 {
            held.remove(key);
        }
    }
}
fn canonical_identity(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return path
            .canonicalize()
            .map(stackyard_core::plain_path)
            .map_err(|source| Error::io("resolve dependency owner", source));
    }
    let parent = path
        .parent()
        .ok_or_else(|| Error::InvalidMessage("dependency owner has no existing ancestor".into()))?;
    Ok(canonical_identity(parent)?.join(
        path.file_name()
            .ok_or_else(|| Error::InvalidMessage("invalid dependency owner".into()))?,
    ))
}

pub fn start_dependency_scoped(
    tasks: &Arc<TaskBoard>,
    data_dir: &Path,
    project_id: &str,
    action: DepAction,
    package: Option<&str>,
    scope_id: &str,
) -> Result<Outcome> {
    let data = data_dir.to_path_buf();
    let project = project_id.to_string();
    let scope = scope_id.to_string();
    let package = package.map(str::to_string);
    spawn_task(
        tasks,
        format!("deps {} {project}/{scope}", action_name(action)),
        move |cancel| {
            run_dependency(
                &data,
                &project,
                action,
                package.as_deref(),
                ".",
                Some(&scope),
                &cancel,
            )
        },
    )
}

pub fn start_recovery_scoped(
    tasks: &Arc<TaskBoard>,
    data_dir: &Path,
    project_id: &str,
    scope_id: &str,
) -> Result<Outcome> {
    let data = data_dir.to_path_buf();
    let project = project_id.to_string();
    let scope = scope_id.to_string();
    spawn_task(
        tasks,
        format!("recover dependencies {project}/{scope} from the lockfile"),
        move |cancel| {
            let message = run_dependency(
                &data,
                &project,
                DepAction::Install,
                None,
                ".",
                Some(&scope),
                &cancel,
            )?;
            Ok(format!("recovered dependencies from the recorded lockfile. Package scripts were not rolled back. {message}"))
        },
    )
}

fn version_args(program: &Path, args: &[String]) -> Vec<String> {
    if let Some(script) = args.first() {
        if script.contains("npm-cli") {
            return vec![script.clone(), "-v".to_string()];
        }
    }
    if args
        .windows(2)
        .any(|pair| pair[0] == "-m" && pair[1] == "pip")
    {
        return vec!["-m".to_string(), "pip".to_string(), "--version".to_string()];
    }
    let name = program
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if name.contains("python") {
        vec![
            "-c".to_string(),
            "import sys; print(sys.version.split()[0])".to_string(),
        ]
    } else {
        vec!["--version".to_string()]
    }
}

fn action_name(action: DepAction) -> &'static str {
    match action {
        DepAction::Install => "install",
        DepAction::Add => "add",
        DepAction::Remove => "remove",
        DepAction::Update => "update",
        DepAction::List => "list",
    }
}

fn valid_id(value: &str) -> bool {
    (1..=63).contains(&value.len())
        && value.starts_with(|c: char| c.is_ascii_lowercase())
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[cfg(test)]
mod scope_writer_tests {
    use super::*;
    #[test]
    fn shared_owner_waits_and_cancels_while_independent_owner_proceeds() {
        let root =
            stackyard_core::prepare_data_dir(&stackyard_core::disposable_dir("scope-writers"))
                .unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let shared = root.join("shared");
        let first = DependencyGuard::acquire(vec![shared.clone()], &cancelled).unwrap();
        let independent =
            DependencyGuard::acquire(vec![root.join("independent")], &cancelled).unwrap();
        let flag = Arc::clone(&cancelled);
        let waiter = thread::spawn(move || DependencyGuard::acquire(vec![shared], &flag).is_err());
        thread::sleep(Duration::from_millis(70));
        cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(waiter.join().unwrap());
        drop(first);
        drop(independent);
        let flag = AtomicBool::new(false);
        drop(DependencyGuard::acquire(vec![root.join("shared")], &flag).unwrap());
        let _ = std::fs::remove_dir_all(root);
    }
}

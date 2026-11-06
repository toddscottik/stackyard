use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use stackyard_core::{
    log_tail, manifest_fingerprint, project_view, read_log, service_launch, service_log_path,
    DepAction, DepRequest, Error, Outcome, Result, Toolchain,
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
    spawn_task(tasks, project_id, label, move |cancel, _| {
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
    let owner = project_id.clone();
    let task_board = Arc::clone(tasks);
    spawn_task(tasks, &owner, label, move |cancel, operation| {
        let message = run_dependency(
            &data_dir,
            &project_id,
            DepRequest {
                action: DepAction::Install,
                package: None,
                scope: &scope,
                scope_id: None,
            },
            &cancel,
            &|message| task_board.progress(operation, message),
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
    let project = project_id.to_string();
    let package = package.map(str::to_string);
    let scope = scope.to_string();
    let label = format!("deps {} {project_id}", action_name(action));
    let task_board = Arc::clone(tasks);
    spawn_task(tasks, project_id, label, move |cancel, operation| {
        run_dependency(
            &data_dir,
            &project,
            DepRequest {
                action,
                package: package.as_deref(),
                scope: &scope,
                scope_id: None,
            },
            &cancel,
            &|message| task_board.progress(operation, message),
        )
    })
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

fn spawn_task<F>(
    tasks: &Arc<TaskBoard>,
    project_id: &str,
    label: String,
    work: F,
) -> Result<Outcome>
where
    F: FnOnce(Arc<AtomicBool>, &str) -> std::result::Result<String, Error> + Send + 'static,
{
    let (id, cancel) = tasks.start(label, project_id);
    let tasks_for_job = Arc::clone(tasks);
    let operation = id.clone();
    tokio::task::spawn_blocking(move || match work(cancel, &operation) {
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
    request: DepRequest<'_>,
    cancel: &AtomicBool,
    progress: &dyn Fn(&str),
) -> Result<String> {
    let DepRequest {
        action,
        scope,
        scope_id,
        ..
    } = request;
    progress("resolving dependency scope and selected tools");
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
    progress("waiting for dependency scope lock");
    let _guard = DependencyGuard::acquire(
        vec![resolved.lockfile_root.clone(), env_dir.clone()],
        cancel,
    )?;
    progress("dependency scope lock acquired");
    let venv = stackyard_core::venv_python(&env_dir);
    let venv_ref = venv.is_file().then_some(venv.as_path());
    if action == DepAction::List
        && matches!(
            resolved.manager,
            stackyard_core::ManagerKind::Pip | stackyard_core::ManagerKind::Uv
        )
        && venv_ref.is_none()
    {
        return Ok(format!(
            "manager: {}\nscope: {}\nNo installed dependencies.",
            resolved.manager.as_str(),
            resolved.directory.display()
        ));
    }
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
    let log_path = data_dir
        .join("logs")
        .join(project_id)
        .join(format!("dependencies-{}.log", resolved.spec.id));
    let log_start = std::fs::metadata(&log_path)
        .map(|meta| meta.len())
        .unwrap_or(0);
    let preparation_log = data_dir.join("logs").join(project_id).join(format!(
        "dependencies-{}-prepare-{}.log",
        resolved.spec.id, "active"
    ));
    let _ = std::fs::remove_file(&preparation_log);
    let node_path = node.as_deref();
    let python_path = python.as_deref();
    let pnpm_path = pnpm
        .as_ref()
        .map(|package| Path::new(package.launch.as_str()));
    let uv_path = uv
        .as_ref()
        .map(|package| Path::new(package.launch.as_str()));
    let preparation = stackyard_core::DependencyPreparation {
        probe_version: &|program, args| {
            let phase = "preparing: checking selected tool version";
            progress("preparing: checking selected tool version (10s limit)");
            let offset = std::fs::metadata(&preparation_log)
                .map(|meta| meta.len())
                .unwrap_or(0);
            let command = stackyard_core::PlannedCommand {
                program: program.to_path_buf(),
                args: args.to_vec(),
                env: Vec::new(),
                env_remove: Vec::new(),
                working_directory: program.parent().unwrap_or(Path::new(".")).to_path_buf(),
            };
            let status = run_command(
                &command,
                &[],
                cancel,
                Duration::from_secs(10),
                CommandOutput {
                    path: &preparation_log,
                    start: offset,
                    phase,
                    progress,
                },
            )?;
            if !status.success() {
                return Err(Error::OperationFailed(format!(
                    "tool version probe failed ({status})\n{}",
                    log_since(&preparation_log, offset, 2_000)
                )));
            }
            Ok(log_since(&preparation_log, offset, 2_000)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string())
        },
        create_venv: &|python, target| {
            let phase = "preparing: creating the selected scope virtual environment";
            progress("preparing: creating the selected scope virtual environment (90s limit)");
            let offset = std::fs::metadata(&preparation_log)
                .map(|meta| meta.len())
                .unwrap_or(0);
            let command = stackyard_core::PlannedCommand {
                program: python.to_path_buf(),
                args: vec!["-m".into(), "venv".into(), target.display().to_string()],
                env: Vec::new(),
                env_remove: Vec::new(),
                working_directory: target.parent().unwrap_or(Path::new(".")).to_path_buf(),
            };
            let status = run_command(
                &command,
                &[],
                cancel,
                Duration::from_secs(90),
                CommandOutput {
                    path: &preparation_log,
                    start: offset,
                    phase,
                    progress,
                },
            )?;
            if status.success() {
                Ok(())
            } else {
                Err(Error::OperationFailed(format!(
                    "virtual environment creation failed ({status})\n{}",
                    log_since(&preparation_log, offset, 2_000)
                )))
            }
        },
    };
    let plan_result = stackyard_core::plan_dependency_with(
        data_dir,
        project_id,
        root,
        &manifest,
        request,
        Toolchain {
            node: node_path,
            python: python_path,
            pnpm: pnpm_path,
            uv: uv_path,
            venv_python: venv_ref,
            php: php.as_deref(),
            composer: composer.as_ref().map(|package| Path::new(&package.launch)),
        },
        preparation,
    );
    let _ = std::fs::remove_file(&preparation_log);
    let plan = plan_result?;
    let before = manifest_fingerprint(&plan.scope_dir);
    let version = match resolved.manager {
        stackyard_core::ManagerKind::Pnpm => pnpm.as_ref().map(|record| record.version.as_str()),
        stackyard_core::ManagerKind::Uv => uv.as_ref().map(|record| record.version.as_str()),
        stackyard_core::ManagerKind::Composer => {
            composer.as_ref().map(|record| record.version.as_str())
        }
        stackyard_core::ManagerKind::Npm | stackyard_core::ManagerKind::Pip => {
            Some(runtime.version.as_str())
        }
    }
    .unwrap_or("unknown");
    let mut output_start = log_start;
    for command in &plan.commands {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(Error::Cancelled {
                id: project_id.to_string(),
            });
        }
        progress(&format!(
            "running {} dependency command",
            plan.manager.as_str()
        ));
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
        if action == DepAction::List {
            output_start = std::fs::metadata(&log_path)
                .map(|meta| meta.len())
                .unwrap_or(output_start);
        }
        let status = run_command(
            command,
            &env,
            cancel,
            Duration::from_secs(180),
            CommandOutput {
                path: &log_path,
                start: output_start,
                phase: "running dependency command",
                progress,
            },
        )?;
        if !status.success() {
            let tail = log_since(&log_path, output_start, 4_000);
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
    let tail = log_since(&log_path, output_start, 4_000);
    let tail = if action == DepAction::List && installed_list_is_empty(plan.manager, &tail) {
        "No installed dependencies.".to_string()
    } else {
        tail
    };
    Ok(format!(
        "manager: {}\ntool: {version}\nscope: {}\nmanifest_changed: {}\n{}\n{tail}",
        plan.manager.as_str(),
        plan.scope_dir.display(),
        before != after,
        plan.note
    ))
}

fn installed_list_is_empty(manager: stackyard_core::ManagerKind, output: &str) -> bool {
    use stackyard_core::ManagerKind::*;
    if output.trim().is_empty() {
        return true;
    }
    match manager {
        Npm | Pnpm => output.lines().any(|line| line.trim().ends_with("(empty)")),
        Pip | Uv => output.lines().map(str::trim).all(|line| {
            line.is_empty()
                || line.split_whitespace().eq(["Package", "Version"])
                || line.bytes().all(|byte| matches!(byte, b' ' | b'-'))
        }),
        Composer => output.split_whitespace().collect::<String>() == r#"{"installed":[]}"#,
    }
}

struct CommandOutput<'a> {
    path: &'a Path,
    start: u64,
    phase: &'a str,
    progress: &'a dyn Fn(&str),
}

fn run_command(
    command: &stackyard_core::PlannedCommand,
    env: &[(String, String)],
    cancel: &AtomicBool,
    limit: Duration,
    output: CommandOutput<'_>,
) -> Result<std::process::ExitStatus> {
    let CommandOutput {
        path: log_path,
        start: log_start,
        phase,
        progress,
    } = output;
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
            let stop_started = Instant::now();
            while group.child.try_wait().ok().flatten().is_none()
                && stop_started.elapsed() < Duration::from_secs(2)
            {
                thread::sleep(Duration::from_millis(50));
            }
            if group.child.try_wait().ok().flatten().is_none() {
                group.force_stop();
            }
            group.finish_io_timeout(Duration::from_millis(250));
            return Err(Error::Cancelled {
                id: "dependency preparation or command".to_string(),
            });
        }
        if let Some(status) = group
            .child
            .try_wait()
            .map_err(|source| Error::io("wait for dependency command", source))?
        {
            group.finish_io_timeout(Duration::from_secs(2));
            return Ok(status);
        }
        if started.elapsed() > limit {
            group.force_stop();
            return Err(Error::OperationFailed(format!(
                "{phase} exceeded {}s; rerun the dependency operation to recover the partial environment\n{}",
                limit.as_secs(),
                log_since(log_path, log_start, 2_000)
            )));
        }
        progress(&format!(
            "{phase}; latest output:\n{}",
            log_since(log_path, log_start, 2_000)
        ));
        thread::sleep(Duration::from_millis(250));
    }
}

static DEPENDENCY_WRITERS: OnceLock<Mutex<BTreeSet<PathBuf>>> = OnceLock::new();

struct DependencyGuard(Vec<PathBuf>);
impl DependencyGuard {
    fn acquire(keys: Vec<PathBuf>, cancel: &AtomicBool) -> Result<Self> {
        Self::acquire_with_timeout(keys, cancel, Duration::from_secs(180))
    }

    fn acquire_with_timeout(
        mut keys: Vec<PathBuf>,
        cancel: &AtomicBool,
        limit: Duration,
    ) -> Result<Self> {
        // Existing ancestors are canonicalized so absent managed environments and
        // junction aliases resolve to the same writer identity.
        for key in &mut keys {
            *key = canonical_identity(key)?;
        }
        let started = Instant::now();
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
            if started.elapsed() >= limit {
                return Err(Error::OperationFailed(
                    "timed out waiting for another dependency operation to release this scope; retry when it finishes".into(),
                ));
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
    let task_board = Arc::clone(tasks);
    let package = package.map(str::to_string);
    spawn_task(
        tasks,
        project_id,
        format!("deps {} {project}/{scope}", action_name(action)),
        move |cancel, operation| {
            run_dependency(
                &data,
                &project,
                DepRequest {
                    action,
                    package: package.as_deref(),
                    scope: ".",
                    scope_id: Some(&scope),
                },
                &cancel,
                &|message| task_board.progress(operation, message),
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
    let task_board = Arc::clone(tasks);
    spawn_task(
        tasks,
        project_id,
        format!("recover dependencies {project}/{scope} from the lockfile"),
        move |cancel, operation| {
            let message = run_dependency(
                &data,
                &project,
                DepRequest {
                    action: DepAction::Install,
                    package: None,
                    scope: ".",
                    scope_id: Some(&scope),
                },
                &cancel,
                &|message| task_board.progress(operation, message),
            )?;
            Ok(format!(
                "recovered dependencies from the recorded lockfile. Package scripts were not rolled back. {message}"
            ))
        },
    )
}

fn log_since(path: &Path, offset: u64, max_bytes: usize) -> String {
    let Ok(mut file) = std::fs::File::open(path) else {
        return String::new();
    };
    use std::io::{Read, Seek, SeekFrom};
    let Ok(metadata) = file.metadata() else {
        return String::new();
    };
    let len = metadata.len();
    let offset = if len < offset { 0 } else { offset };
    let start = offset.max(len.saturating_sub(max_bytes as u64));
    if file.seek(SeekFrom::Start(start)).is_err() {
        return String::new();
    }
    let mut bytes = Vec::new();
    if file.take(max_bytes as u64).read_to_end(&mut bytes).is_err() {
        return String::new();
    }
    String::from_utf8_lossy(&bytes).into_owned()
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
    use std::io::Write;

    #[test]
    fn invocation_output_starts_at_captured_log_offset() {
        let root = stackyard_core::disposable_dir("dependency-log-offset");
        std::fs::create_dir_all(&root).unwrap();
        let log = root.join("deps.log");
        std::fs::write(&log, "previous invocation\n").unwrap();
        let offset = std::fs::metadata(&log).unwrap().len();
        std::fs::OpenOptions::new()
            .append(true)
            .open(&log)
            .unwrap()
            .write_all(b"current invocation\n")
            .unwrap();
        let output = log_since(&log, offset, 4000);
        assert_eq!(output, "current invocation\n");
        let _ = std::fs::remove_dir_all(root);
    }

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

    #[test]
    fn shared_owner_wait_is_bounded() {
        let root = stackyard_core::prepare_data_dir(&stackyard_core::disposable_dir(
            "scope-writer-timeout",
        ))
        .unwrap();
        let cancelled = AtomicBool::new(false);
        let shared = root.join("shared");
        let _first = DependencyGuard::acquire(vec![shared.clone()], &cancelled).unwrap();
        let started = Instant::now();
        let result = DependencyGuard::acquire_with_timeout(
            vec![shared],
            &cancelled,
            Duration::from_millis(80),
        );
        assert!(result.is_err());
        assert!(started.elapsed() < Duration::from_secs(1));
        let _ = std::fs::remove_dir_all(root);
    }
}

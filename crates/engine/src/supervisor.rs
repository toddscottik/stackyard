use std::collections::{HashMap, HashSet};
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use stackyard_core::{
    append_log, load_project_manifest, log_tail, project_view, require_startable, service_launch,
    service_log_path, start_order, Error, ProcessRecord, ProjectRecord, ProjectView, RestartPolicy,
    Result, RuntimeRole, ServiceObservationRecord, ServiceSpec, Store,
};

use crate::group::{self, GroupProcess};
use crate::readiness::{self, Wait};

pub struct Supervisor {
    data_dir: PathBuf,
    shutdown: AtomicBool,
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
    stops: Mutex<HashMap<String, Arc<AtomicBool>>>,
    live: Mutex<HashMap<String, Live>>,
    operations: Mutex<HashSet<String>>,
}

struct Live {
    pid: u32,
    group_id: u32,
    identity: String,
    #[cfg(windows)]
    job: Arc<group::JobGuard>,
}

struct ProjectOperation<'a> {
    operations: &'a Mutex<HashSet<String>>,
    project: String,
}

impl Drop for ProjectOperation<'_> {
    fn drop(&mut self) {
        self.operations
            .lock()
            .expect("operations")
            .remove(&self.project);
    }
}

pub fn new(data_dir: PathBuf) -> Arc<Supervisor> {
    Arc::new(Supervisor {
        data_dir,
        shutdown: AtomicBool::new(false),
        cancels: Mutex::new(HashMap::new()),
        stops: Mutex::new(HashMap::new()),
        live: Mutex::new(HashMap::new()),
        operations: Mutex::new(HashSet::new()),
    })
}

impl Supervisor {
    pub fn project_is_busy(&self, project_id: &str) -> bool {
        self.operations
            .lock()
            .expect("operations")
            .contains(project_id)
            || self.project_has_workers(project_id)
    }

    fn project_has_workers(&self, project_id: &str) -> bool {
        self.cancels
            .lock()
            .expect("cancels")
            .contains_key(project_id)
            || self.live.lock().expect("live").keys().any(|key| {
                key.strip_prefix(project_id)
                    .is_some_and(|tail| tail.starts_with('\0'))
            })
    }

    pub fn clear_stop_flags(&self, project_id: &str) {
        let prefix = format!("{project_id}\0");
        self.stops
            .lock()
            .expect("stops")
            .retain(|key, _| !key.starts_with(&prefix));
    }

    pub fn reconcile(&self) -> Result<()> {
        let store = Store::open(&self.data_dir)?;
        for process in store.active_processes()? {
            if recorded_process_is_alive(&process) {
                self.write(process_row(&process, "orphaned", process.detail.clone()))?;
            } else {
                let mut stopped = process;
                stopped.state = "stopped".to_string();
                stopped.pid = None;
                stopped.detail = "process was not running after the engine restarted".to_string();
                self.write(stopped)?;
            }
        }
        Ok(())
    }

    pub fn start(self: &Arc<Self>, project_id: &str) -> Result<ProjectView> {
        let _operation = self.operation(project_id)?;
        if self.project_has_workers(project_id) {
            return Err(Error::ProjectBusy {
                id: project_id.to_owned(),
                state: "service workers are still active".into(),
            });
        }
        if self.shutdown.load(Ordering::Relaxed) {
            return Err(Error::OperationFailed("the engine is stopping".to_string()));
        }
        let (project, manifest) = load_project_manifest(&self.data_dir, project_id)?;
        let view = project_view(&self.data_dir, project_id)?;
        require_startable(&view)?;
        let order = start_order(&manifest.services)?;
        for id in &order {
            let service = manifest.service(id).expect("service from order");
            let root = PathBuf::from(&project.root);
            let _ = stackyard_core::contained_dir(&root, &service.working_directory)?;
            let _ = stackyard_core::scopes::resolve_scope_runtime(
                &self.data_dir,
                project_id,
                &service.scope_id,
                service.runtime,
            )?;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        for id in &order {
            self.mark(
                &project.id,
                id,
                "starting",
                None,
                None,
                None,
                None,
                None,
                "",
            )?;
        }
        self.cancels
            .lock()
            .expect("cancels")
            .insert(project_id.to_string(), Arc::clone(&cancel));
        let supervisor = Arc::clone(self);
        let id = project_id.to_string();
        tokio::task::spawn_blocking(move || supervisor.run_start(id, cancel));
        project_view(&self.data_dir, project_id)
    }

    pub fn cancel(&self, project_id: &str) -> Result<()> {
        let flag = self
            .cancels
            .lock()
            .expect("cancels")
            .get(project_id)
            .cloned();
        let Some(flag) = flag else {
            return Err(Error::InvalidMessage(format!(
                "project {project_id} is not starting"
            )));
        };
        flag.store(true, Ordering::Relaxed);
        self.signal_project(project_id);
        Ok(())
    }

    pub fn stop(&self, project_id: &str) -> Result<ProjectView> {
        let _operation = self.operation(project_id)?;
        self.stop_inner(project_id)
    }

    fn stop_inner(&self, project_id: &str) -> Result<ProjectView> {
        self.request_halt(project_id);
        self.signal_project(project_id);
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(3) {
            if !self.project_has_workers(project_id) {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        let store = Store::open(&self.data_dir)?;
        for service in store.list_processes(project_id)? {
            if matches!(service.state.as_str(), "starting" | "running" | "orphaned") {
                if service.state == "orphaned" && recorded_process_is_alive(&service) {
                    return Err(Error::ProjectBusy {
                        id: project_id.to_owned(),
                        state: format!(
                            "waiting for orphaned service {} to stop",
                            service.service_id
                        ),
                    });
                }
                self.mark(
                    project_id,
                    &service.service_id,
                    "stopped",
                    None,
                    None,
                    None,
                    service.port,
                    service.exit_code,
                    "stopped",
                )?;
            }
        }
        project_view(&self.data_dir, project_id)
    }

    pub fn restart(self: &Arc<Self>, project_id: &str) -> Result<ProjectView> {
        self.stop(project_id)?;
        self.start(project_id)
    }

    /// Validate/promote a candidate before disturbing a worker. Only PHP
    /// workers live at entry are restarted, on their existing observed ports.
    pub fn reconfigure_php<T, F>(self: &Arc<Self>, project_id: &str, apply: F) -> Result<T>
    where
        F: FnOnce() -> Result<T>,
    {
        let (_, manifest) = load_project_manifest(&self.data_dir, project_id)?;
        let php_scopes: Vec<_> = manifest
            .scopes
            .iter()
            .filter(|scope| scope.runtime == RuntimeRole::Php)
            .collect();
        if manifest.schema > 1 && php_scopes.len() > 1 {
            return Err(Error::InvalidMessage("PHP reconfigure requires an explicit scope when the project has multiple PHP scopes".into()));
        }
        let (scope_id, profile_key) = match php_scopes.first() {
            Some(scope) if manifest.schema > 1 => (
                Some(scope.id.as_str()),
                stackyard_core::scopes::scope_profile_key(project_id, &scope.id, manifest.schema)?,
            ),
            _ => (None, project_id.to_string()),
        };
        self.reconfigure_php_key(project_id, scope_id, &profile_key, apply)
    }

    pub fn reconfigure_php_scope<T, F>(
        self: &Arc<Self>,
        project_id: &str,
        scope_id: &str,
        apply: F,
    ) -> Result<T>
    where
        F: FnOnce() -> Result<T>,
    {
        let (_, manifest) = load_project_manifest(&self.data_dir, project_id)?;
        let scope = manifest
            .scopes
            .iter()
            .find(|scope| scope.id == scope_id)
            .ok_or_else(|| Error::InvalidMessage(format!("unknown PHP scope {scope_id}")))?;
        if scope.runtime != RuntimeRole::Php {
            return Err(Error::InvalidMessage(format!(
                "scope {scope_id} is not a PHP scope"
            )));
        }
        let key = stackyard_core::scopes::scope_profile_key(project_id, scope_id, manifest.schema)?;
        self.reconfigure_php_key(project_id, Some(scope_id), &key, apply)
    }

    fn reconfigure_php_key<T, F>(
        self: &Arc<Self>,
        project_id: &str,
        scope_id: Option<&str>,
        profile_key: &str,
        apply: F,
    ) -> Result<T>
    where
        F: FnOnce() -> Result<T>,
    {
        let _operation = self.operation(project_id)?;
        if self.shutdown.load(Ordering::Relaxed) {
            return Err(Error::OperationFailed("the engine is stopping".into()));
        }
        let (project, manifest) = load_project_manifest(&self.data_dir, project_id)?;
        let rows = Store::open(&self.data_dir)?.list_processes(project_id)?;
        if self
            .cancels
            .lock()
            .expect("cancels")
            .contains_key(project_id)
            || rows
                .iter()
                .any(|row| matches!(row.state.as_str(), "starting" | "orphaned"))
        {
            return Err(Error::ProjectBusy {
                id: project_id.into(),
                state:
                    "starting or orphaned; settle or stop the project before changing PHP profiles"
                        .into(),
            });
        }
        let mut workers = Vec::new();
        for id in start_order(&manifest.services)? {
            let service = manifest.service(&id).expect("ordered service");
            if service.runtime != RuntimeRole::Php
                || scope_id.is_some_and(|scope| service.scope_id != scope)
            {
                continue;
            }
            let Some(row) = rows
                .iter()
                .find(|row| row.service_id == id && row.state == "running")
            else {
                continue;
            };
            let key = live_key(project_id, &id);
            let owned = self
                .live
                .lock()
                .expect("live")
                .get(&key)
                .is_some_and(|live| {
                    Some(i64::from(live.pid)) == row.pid
                        && group::same_process(
                            &live.identity,
                            group::process_identity(live.pid).as_deref(),
                        )
                });
            if !owned {
                return Err(Error::ProjectBusy {
                    id: project_id.into(),
                    state: format!(
                        "PHP service {id} is not owned or is exiting; refresh status before retrying"
                    ),
                });
            }
            let port = php_reconfigure_port(service.endpoint.is_some(), row.port, &id)?;
            let mut service = service.clone();
            service.port = port;
            if let Some(endpoint) = service.endpoint.as_mut() {
                endpoint.port = port;
            }
            workers.push(service);
        }
        let previous = stackyard_core::php::read_php_profile(&self.data_dir, profile_key)?;
        let result = match apply() {
            Ok(result) => result,
            Err(error) => {
                if stackyard_core::php::read_php_profile(&self.data_dir, profile_key)? != previous {
                    stackyard_core::php::rollback_php_profile(&self.data_dir, profile_key, previous.as_ref())
                        .map_err(|rollback| Error::OperationFailed(format!("PHP candidate failed: {error}; restoring the prior profile also failed: {rollback}; current workers were left running")))?;
                }
                return Err(error);
            }
        };
        if workers.is_empty() {
            return Ok(result);
        }
        let rollout = self
            .stop_php_workers(&project, &workers)
            .and_then(|()| self.start_php_workers(&project, &workers));
        if let Err(error) = rollout {
            let cleanup = self.stop_php_workers(&project, &workers);
            let restored = stackyard_core::php::rollback_php_profile(
                &self.data_dir,
                profile_key,
                previous.as_ref(),
            );
            let outcome = match (cleanup, restored) {
                (Ok(()), Ok(())) => match self.start_php_workers(&project, &workers) {
                    Ok(()) => "previous profile restored and PHP workers recovered on their original ports".to_string(),
                    Err(recovery) => {
                        let _ = self.stop_php_workers(&project, &workers);
                        format!("previous profile restored, but PHP worker recovery failed: {recovery}; inspect project logs and start the project again")
                    }
                },
                (cleanup, restore) => format!("rollback could not recover workers (stop: {}; profile restore: {}); inspect project logs and stop the project before retrying",
                    cleanup.map(|()| "succeeded".into()).unwrap_or_else(|e| e.to_string()),
                    restore.map(|()| "succeeded".into()).unwrap_or_else(|e| e.to_string())),
            };
            return Err(Error::OperationFailed(format!(
                "PHP profile rollout failed: {error}; {outcome}"
            )));
        }
        Ok(result)
    }

    fn operation(&self, project_id: &str) -> Result<ProjectOperation<'_>> {
        if !self
            .operations
            .lock()
            .expect("operations")
            .insert(project_id.into())
        {
            return Err(Error::ProjectBusy {
                id: project_id.into(),
                state: "being reconfigured or changed by another lifecycle operation".into(),
            });
        }
        Ok(ProjectOperation {
            operations: &self.operations,
            project: project_id.into(),
        })
    }

    fn stop_php_workers(&self, project: &ProjectRecord, workers: &[ServiceSpec]) -> Result<()> {
        let rows = Store::open(&self.data_dir)?.list_processes(&project.id)?;
        for service in workers.iter().rev() {
            if let Some(row) = rows.iter().find(|row| row.service_id == service.id) {
                self.stop_row(row);
            }
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let live = self.live.lock().expect("live");
            let busy = workers
                .iter()
                .any(|service| live.contains_key(&live_key(&project.id, &service.id)));
            drop(live);
            if !busy {
                break;
            }
            if Instant::now() >= deadline {
                return Err(Error::OperationFailed(
                    "PHP workers did not release their owned processes within 5s".into(),
                ));
            }
            thread::sleep(Duration::from_millis(20));
        }
        for service in workers {
            self.mark_state(
                project,
                service,
                "stopped",
                None,
                None,
                None,
                recorded_port(service.port),
                None,
                "PHP profile rollout",
            )?;
        }
        Ok(())
    }

    fn start_php_workers(
        self: &Arc<Self>,
        project: &ProjectRecord,
        workers: &[ServiceSpec],
    ) -> Result<()> {
        let cancel = AtomicBool::new(false);
        for service in workers {
            self.mark_state(
                project,
                service,
                "starting",
                None,
                None,
                None,
                recorded_port(service.port),
                None,
                "PHP profile rollout",
            )?;
            if !self.bring_up(project, service, &cancel)? {
                return Err(Error::OperationFailed(
                    "PHP profile rollout was cancelled while starting workers".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let project_ids: Vec<String> = self
            .cancels
            .lock()
            .expect("cancels")
            .keys()
            .cloned()
            .collect();
        for id in &project_ids {
            if let Some(flag) = self.cancels.lock().expect("cancels").get(id) {
                flag.store(true, Ordering::Relaxed);
            }
        }
        let store = match Store::open(&self.data_dir) {
            Ok(store) => store,
            Err(_) => return,
        };
        let Ok(projects) = store.list_projects() else {
            return;
        };
        for project in projects {
            let _ = self.stop_inner(&project.id);
        }
    }

    fn run_start(self: &Arc<Self>, project_id: String, cancel: Arc<AtomicBool>) {
        if let Err(err) = self.start_ordered(&project_id, &cancel) {
            let detail = err.to_string();
            if let Ok((project, manifest)) = load_project_manifest(&self.data_dir, &project_id) {
                for service in &manifest.services {
                    if let Ok(store) = Store::open(&self.data_dir) {
                        if let Ok(rows) = store.list_processes(&project.id) {
                            if let Some(row) = rows.iter().find(|row| row.service_id == service.id)
                            {
                                if row.state == "starting" {
                                    let _ = self.mark(
                                        &project.id,
                                        &service.id,
                                        "failed",
                                        None,
                                        None,
                                        None,
                                        None,
                                        None,
                                        &detail,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        self.cancels.lock().expect("cancels").remove(&project_id);
    }

    fn start_ordered(self: &Arc<Self>, project_id: &str, cancel: &AtomicBool) -> Result<()> {
        let (project, manifest) = load_project_manifest(&self.data_dir, project_id)?;
        let order = start_order(&manifest.services)?;
        let mut started = Vec::new();
        for id in order {
            if self.halted(cancel) {
                self.rollback(&project, &started, "cancelled")?;
                return Ok(());
            }
            let service = manifest.service(&id).expect("service from order").clone();
            match self.bring_up(&project, &service, cancel) {
                Ok(true) => started.push(service.id),
                Ok(false) => {
                    self.rollback(&project, &started, "cancelled")?;
                    return Ok(());
                }
                Err(err) => {
                    self.rollback(&project, &started, "stopped because another service failed")?;
                    return Err(err);
                }
            }
        }
        Ok(())
    }

    fn bring_up(
        self: &Arc<Self>,
        project: &ProjectRecord,
        service: &ServiceSpec,
        cancel: &AtomicBool,
    ) -> Result<bool> {
        let port = match service.endpoint.as_ref() {
            Some(endpoint) => reserve_port(endpoint.port)?,
            None => 0,
        };
        let log_path = service_log_path(&self.data_dir, &project.id, &service.id);
        let stop = Arc::new(AtomicBool::new(false));
        let key = live_key(&project.id, &service.id);
        self.stops
            .lock()
            .expect("stops")
            .insert(key.clone(), Arc::clone(&stop));
        let mut restarts_left = service.restart_max;
        loop {
            if self.halted(cancel) || stop.load(Ordering::Acquire) {
                self.mark_state(
                    project,
                    service,
                    "stopped",
                    None,
                    None,
                    None,
                    None,
                    None,
                    "cancelled",
                )?;
                return Ok(false);
            }
            let launch = service_launch(&self.data_dir, project, service, port, &service.command)?;
            self.observe(project, service, "pending", None)?;
            let _ = append_log(
                &log_path,
                format!(
                    "stackyard: starting {} {}\n",
                    launch.program.display(),
                    launch.args.join(" ")
                )
                .as_bytes(),
            );
            let mut group = group::spawn_grouped_scoped(
                &launch.program,
                &launch.args,
                &launch.working_directory,
                &launch.env,
                &launch.env_remove,
                &log_path,
            )?;
            self.remember(&key, &group);
            match wait_ready(
                &mut group,
                service,
                port,
                &log_path,
                cancel,
                &self.shutdown,
                &stop,
            ) {
                Wait::Ready { http_status } => {
                    let observation = match service.readiness {
                        stackyard_core::ReadinessSpec::Process { .. } => "not_checked",
                        _ => "ready",
                    };
                    self.observe(project, service, observation, http_status)?;
                    self.mark_state(
                        project,
                        service,
                        "running",
                        Some(i64::from(group.pid)),
                        Some(i64::from(group.group_id)),
                        Some(group.identity.clone()),
                        recorded_port(port),
                        None,
                        "",
                    )?;
                    if self.halted(cancel) || stop.load(Ordering::Acquire) {
                        group.request_stop();
                        let _ = group.wait_timeout(Duration::from_secs(2));
                        self.forget(&key);
                        self.mark_state(
                            project,
                            service,
                            "stopped",
                            None,
                            None,
                            None,
                            recorded_port(port),
                            None,
                            "cancelled",
                        )?;
                        return Ok(false);
                    }
                    self.watch(
                        group,
                        project.clone(),
                        service.clone(),
                        port,
                        restarts_left,
                        stop,
                        log_path,
                    );
                    return Ok(true);
                }
                Wait::Cancelled { http_status } => {
                    let _ = self.observe(project, service, "not_checked", http_status);
                    group.request_stop();
                    let _ = group.wait_timeout(Duration::from_secs(2));
                    self.forget(&key);
                    self.mark_state(
                        project,
                        service,
                        "stopped",
                        None,
                        None,
                        None,
                        recorded_port(port),
                        None,
                        "cancelled",
                    )?;
                    return Ok(false);
                }
                Wait::Exited {
                    detail,
                    reaped,
                    http_status,
                } => {
                    let _ = self.observe(project, service, "failed", http_status);
                    if !reaped {
                        group.request_stop();
                        let _ = group.wait_timeout(Duration::from_secs(2));
                    }
                    self.forget(&key);
                    let can_restart = service.restart == RestartPolicy::OnFailure
                        && restarts_left > 0
                        && !self.halted(cancel)
                        && !stop.load(Ordering::Acquire);
                    if can_restart {
                        restarts_left -= 1;
                        thread::sleep(Duration::from_millis(200));
                        continue;
                    }
                    // Leave this service `starting` until the caller has stopped
                    // every service that already came up. Publishing `failed`
                    // first lets a status poll observe a running sibling.
                    return Err(Error::ServiceFailed {
                        project: project.id.clone(),
                        service: service.id.clone(),
                        detail,
                    });
                }
            }
        }
    }

    fn observe(
        &self,
        project: &ProjectRecord,
        service: &ServiceSpec,
        state: &str,
        http_status: Option<u16>,
    ) -> Result<()> {
        Store::open(&self.data_dir)?.upsert_service_observation(&ServiceObservationRecord {
            project_id: project.id.clone(),
            service_id: service.id.clone(),
            readiness_state: state.to_string(),
            http_status,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn watch(
        self: &Arc<Self>,
        group: GroupProcess,
        project: ProjectRecord,
        service: ServiceSpec,
        port: u16,
        mut restarts_left: u32,
        stop: Arc<AtomicBool>,
        log_path: PathBuf,
    ) {
        let supervisor = Arc::clone(self);
        let key = live_key(&project.id, &service.id);
        thread::spawn(move || {
            let mut group = group;
            loop {
                let status = group.child.wait();
                group.finish_io();
                if stop.load(Ordering::Acquire) || supervisor.shutdown.load(Ordering::Relaxed) {
                    let _ = supervisor.mark_state(
                        &project,
                        &service,
                        "stopped",
                        None,
                        None,
                        None,
                        recorded_port(port),
                        None,
                        "stopped",
                    );
                    supervisor.forget(&key);
                    return;
                }
                let detail = match status {
                    Ok(status) => format!("exited {status}\n{}", log_tail(&log_path, 800)),
                    Err(err) => err.to_string(),
                };
                if service.restart == RestartPolicy::OnFailure
                    && restarts_left > 0
                    && !stop.load(Ordering::Acquire)
                    && !supervisor.shutdown.load(Ordering::Relaxed)
                {
                    restarts_left -= 1;
                    thread::sleep(Duration::from_millis(200));
                    if stop.load(Ordering::Acquire) || supervisor.shutdown.load(Ordering::Relaxed) {
                        let _ = supervisor.mark_state(
                            &project,
                            &service,
                            "stopped",
                            None,
                            None,
                            None,
                            recorded_port(port),
                            None,
                            "stopped",
                        );
                        supervisor.forget(&key);
                        return;
                    }
                    let Ok(launch) = service_launch(
                        &supervisor.data_dir,
                        &project,
                        &service,
                        port,
                        &service.command,
                    ) else {
                        let _ = supervisor.mark_state(
                            &project,
                            &service,
                            "failed",
                            None,
                            None,
                            None,
                            recorded_port(port),
                            None,
                            &detail,
                        );
                        supervisor.forget(&key);
                        return;
                    };
                    let _ = supervisor.observe(&project, &service, "pending", None);
                    let Ok(mut next) = group::spawn_grouped_scoped(
                        &launch.program,
                        &launch.args,
                        &launch.working_directory,
                        &launch.env,
                        &launch.env_remove,
                        &log_path,
                    ) else {
                        let _ = supervisor.mark_state(
                            &project,
                            &service,
                            "failed",
                            None,
                            None,
                            None,
                            recorded_port(port),
                            None,
                            &detail,
                        );
                        supervisor.forget(&key);
                        return;
                    };
                    match wait_ready(
                        &mut next,
                        &service,
                        port,
                        &log_path,
                        &AtomicBool::new(false),
                        &supervisor.shutdown,
                        &stop,
                    ) {
                        Wait::Ready { http_status } => {
                            let state = if matches!(
                                service.readiness,
                                stackyard_core::ReadinessSpec::Process { .. }
                            ) {
                                "not_checked"
                            } else {
                                "ready"
                            };
                            let _ = supervisor.observe(&project, &service, state, http_status);
                        }
                        Wait::Cancelled { http_status } => {
                            let _ =
                                supervisor.observe(&project, &service, "not_checked", http_status);
                            next.request_stop();
                            let _ = next.wait_timeout(Duration::from_secs(2));
                            let _ = supervisor.mark_state(
                                &project,
                                &service,
                                "stopped",
                                None,
                                None,
                                None,
                                recorded_port(port),
                                None,
                                "cancelled",
                            );
                            supervisor.forget(&key);
                            return;
                        }
                        Wait::Exited {
                            detail: failed,
                            reaped,
                            http_status,
                        } => {
                            let _ = supervisor.observe(&project, &service, "failed", http_status);
                            if !reaped {
                                next.request_stop();
                                let _ = next.wait_timeout(Duration::from_secs(2));
                            }
                            let _ = supervisor.mark_state(
                                &project,
                                &service,
                                "failed",
                                None,
                                None,
                                None,
                                recorded_port(port),
                                None,
                                &failed,
                            );
                            supervisor.forget(&key);
                            return;
                        }
                    }
                    supervisor.remember(&key, &next);
                    let _ = supervisor.mark_state(
                        &project,
                        &service,
                        "running",
                        Some(i64::from(next.pid)),
                        Some(i64::from(next.group_id)),
                        Some(next.identity.clone()),
                        recorded_port(port),
                        None,
                        "",
                    );
                    group = next;
                    continue;
                }
                let _ = supervisor.mark_state(
                    &project,
                    &service,
                    "failed",
                    None,
                    None,
                    None,
                    recorded_port(port),
                    None,
                    &detail,
                );
                let _ = supervisor.observe(&project, &service, "failed", None);
                supervisor.forget(&key);
                return;
            }
        });
    }

    fn rollback(&self, project: &ProjectRecord, started: &[String], detail: &str) -> Result<()> {
        let store = Store::open(&self.data_dir)?;
        for id in started.iter().rev() {
            let key = live_key(&project.id, id);
            if let Some(flag) = self.stops.lock().expect("stops").get(&key) {
                flag.store(true, Ordering::Release);
            }
            if let Some(row) = store
                .list_processes(&project.id)?
                .into_iter()
                .find(|row| row.service_id == *id)
            {
                self.stop_row(&row);
            }
            self.mark(
                &project.id,
                id,
                "stopped",
                None,
                None,
                None,
                None,
                None,
                detail,
            )?;
        }
        Ok(())
    }

    fn stop_row(&self, process: &ProcessRecord) {
        let Some(pid) = process.pid.and_then(|pid| u32::try_from(pid).ok()) else {
            return;
        };
        let group_id = process
            .process_group
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(pid);
        let identity = process.identity.clone().unwrap_or_default();
        let key = live_key(&process.project_id, &process.service_id);
        if let Some(flag) = self.stops.lock().expect("stops").get(&key) {
            flag.store(true, Ordering::Release);
        }
        #[cfg(windows)]
        if let Some(live) = self.live.lock().expect("live").get(&key) {
            live.job.terminate();
        }
        group::stop_recorded(pid, group_id, &identity);
    }

    fn request_halt(&self, project_id: &str) {
        if let Some(flag) = self.cancels.lock().expect("cancels").get(project_id) {
            flag.store(true, Ordering::Relaxed);
        }
        let prefix = format!("{project_id}\0");
        for (key, flag) in self.stops.lock().expect("stops").iter() {
            if key.starts_with(&prefix) {
                flag.store(true, Ordering::Release);
            }
        }
    }

    fn signal_project(&self, project_id: &str) {
        let prefix = format!("{project_id}\0");
        let lives: Vec<(u32, u32, String)> = self
            .live
            .lock()
            .expect("live")
            .iter()
            .filter(|(key, _)| key.starts_with(&prefix))
            .map(|(_, live)| (live.pid, live.group_id, live.identity.clone()))
            .collect();
        #[cfg(windows)]
        {
            for (key, live) in self.live.lock().expect("live").iter() {
                if key.starts_with(&prefix) {
                    live.job.terminate();
                }
            }
        }
        for (pid, group_id, identity) in lives {
            group::stop_recorded(pid, group_id, &identity);
        }
        let store = match Store::open(&self.data_dir) {
            Ok(store) => store,
            Err(_) => return,
        };
        let Ok(rows) = store.list_processes(project_id) else {
            return;
        };
        for row in rows {
            if matches!(row.state.as_str(), "starting" | "running" | "orphaned") {
                self.stop_row(&row);
            }
        }
    }

    fn remember(&self, key: &str, group: &GroupProcess) {
        self.live.lock().expect("live").insert(
            key.to_string(),
            Live {
                pid: group.pid,
                group_id: group.group_id,
                identity: group.identity.clone(),
                #[cfg(windows)]
                job: group.job(),
            },
        );
    }

    fn forget(&self, key: &str) {
        self.live.lock().expect("live").remove(key);
    }

    fn halted(&self, cancel: &AtomicBool) -> bool {
        cancel.load(Ordering::Relaxed) || self.shutdown.load(Ordering::Relaxed)
    }

    #[allow(clippy::too_many_arguments)]
    fn mark_state(
        &self,
        project: &ProjectRecord,
        service: &ServiceSpec,
        state: &str,
        pid: Option<i64>,
        group_id: Option<i64>,
        identity: Option<String>,
        port: Option<i64>,
        exit_code: Option<i64>,
        detail: &str,
    ) -> Result<()> {
        self.mark(
            &project.id,
            &service.id,
            state,
            pid,
            group_id,
            identity,
            port,
            exit_code,
            detail,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn mark(
        &self,
        project_id: &str,
        service_id: &str,
        state: &str,
        pid: Option<i64>,
        process_group: Option<i64>,
        identity: Option<String>,
        port: Option<i64>,
        exit_code: Option<i64>,
        detail: &str,
    ) -> Result<()> {
        self.write(ProcessRecord {
            project_id: project_id.to_string(),
            service_id: service_id.to_string(),
            state: state.to_string(),
            pid,
            process_group,
            started_at: Some(now_unix()),
            identity,
            port,
            exit_code,
            detail: clip(detail),
        })
    }

    fn write(&self, process: ProcessRecord) -> Result<()> {
        Store::open(&self.data_dir)?.upsert_process(&process)
    }
}

fn wait_ready(
    group: &mut GroupProcess,
    service: &ServiceSpec,
    port: u16,
    log_path: &Path,
    cancel: &AtomicBool,
    shutdown: &AtomicBool,
    stop: &AtomicBool,
) -> Wait {
    readiness::wait_ready(group, service, port, log_path, cancel, shutdown, stop)
}

fn reserve_port(requested: u16) -> Result<u16> {
    let bind_port = if requested == 0 { 0 } else { requested };
    let listener =
        TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], bind_port))).map_err(|source| {
            if requested != 0 {
                Error::PortInUse { port: requested }
            } else {
                Error::io("choose a loopback port", source)
            }
        })?;
    let port = listener
        .local_addr()
        .map_err(|source| Error::io("read loopback port", source))?
        .port();
    drop(listener);
    Ok(port)
}

fn recorded_process_is_alive(process: &ProcessRecord) -> bool {
    let current = process
        .pid
        .and_then(|pid| u32::try_from(pid).ok())
        .and_then(group::process_identity);
    process
        .identity
        .as_deref()
        .is_some_and(|recorded| group::same_process(recorded, current.as_deref()))
}

fn live_key(project: &str, service: &str) -> String {
    format!("{project}\0{service}")
}

fn recorded_port(port: u16) -> Option<i64> {
    (port != 0).then_some(i64::from(port))
}

fn php_reconfigure_port(
    has_endpoint: bool,
    observed: Option<i64>,
    service_id: &str,
) -> Result<u16> {
    if !has_endpoint {
        return Ok(0);
    }
    observed
        .and_then(|port| u16::try_from(port).ok())
        .filter(|port| *port != 0)
        .ok_or_else(|| {
            Error::OperationFailed(format!(
                "PHP service {service_id} has no valid observed port"
            ))
        })
}

fn process_row(process: &ProcessRecord, state: &str, detail: String) -> ProcessRecord {
    let mut next = process.clone();
    next.state = state.to_string();
    next.detail = clip(&detail);
    next
}

fn clip(text: &str) -> String {
    const MAX: usize = 1_800;
    if text.chars().count() <= MAX {
        text.to_string()
    } else {
        text.chars().take(MAX).collect()
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_secs()).unwrap_or(0))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{php_reconfigure_port, recorded_port};

    #[test]
    fn scoped_php_reconfigure_accepts_workers_without_endpoints() {
        assert_eq!(php_reconfigure_port(false, None, "queue").unwrap(), 0);
        assert_eq!(recorded_port(0), None);
        assert_eq!(
            php_reconfigure_port(true, Some(18321), "web").unwrap(),
            18321
        );
        assert!(php_reconfigure_port(true, None, "web").is_err());
    }
}

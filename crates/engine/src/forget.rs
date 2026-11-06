use stackyard_core::{Error, Result, Store};

use crate::{supervisor::Supervisor, tasks::TaskBoard};

/// Dispatch serializes new commands. Background tasks must keep their active
/// markers until their final write, including after a cancellation request.
pub fn require_idle(
    store: &Store,
    tasks: &TaskBoard,
    supervisor: &Supervisor,
    project_id: &str,
) -> Result<()> {
    if tasks.project_is_busy(project_id) || supervisor.project_is_busy(project_id) {
        return Err(Error::ProjectBusy {
            id: project_id.to_owned(),
            state: "project operations are still active. Stop its services and wait for Activity to finish".into(),
        });
    }
    store.require_stopped_project(project_id).map(|_| ())
}

pub fn forget_project(
    store: &Store,
    tasks: &TaskBoard,
    supervisor: &Supervisor,
    project_id: &str,
) -> Result<()> {
    require_idle(store, tasks, supervisor, project_id)?;
    store.forget_project(project_id)?;
    supervisor.clear_stop_flags(project_id);
    Ok(())
}

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use stackyard_core::Outcome;

use crate::jobs::{activity_sequence, clip, json_size, ActivityHistory, SUMMARY_BYTES};

pub struct TaskSnapshot {
    pub label: String,
    pub state: String,
    pub message: String,
}

impl TaskSnapshot {
    pub fn into_outcome(self, id: &str) -> Outcome {
        Outcome::Task {
            id: id.to_string(),
            label: self.label,
            state: self.state,
            message: self.message,
        }
    }
}

struct Task {
    label: String,
    state: String,
    message: String,
    cancel: Arc<AtomicBool>,
}

impl Task {
    fn snapshot(&self) -> TaskSnapshot {
        TaskSnapshot {
            label: self.label.clone(),
            state: self.state.clone(),
            message: self.message.clone(),
        }
    }
}

pub struct TaskBoard {
    next: AtomicU64,
    inner: Mutex<ActivityHistory<Task>>,
}

impl TaskBoard {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            next: activity_sequence(),
            inner: Mutex::new(ActivityHistory::new()),
        })
    }

    pub fn start(&self, label: String) -> (String, Arc<AtomicBool>) {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let cancel = Arc::new(AtomicBool::new(false));
        let task = Task {
            label,
            state: "running".to_string(),
            message: "running".to_string(),
            cancel: Arc::clone(&cancel),
        };
        self.inner.lock().expect("tasks").entries.insert(id, task);
        (format!("task-{id}"), cancel)
    }

    pub fn finish(&self, id: &str, state: &str, message: &str) {
        let Some(id) = parse_id(id) else {
            return;
        };
        let mut guard = self.inner.lock().expect("tasks");
        let Some(task) = guard.entries.get_mut(&id) else {
            return;
        };
        if task.state != "running" {
            return;
        }
        if task.cancel.load(Ordering::Relaxed) && state == "running" {
            task.state = "cancelled".to_string();
            task.message = "cancelled".to_string();
        } else {
            task.state = state.to_string();
            task.message = clip(message, 6_000);
        }
        if task.state != "running" {
            guard.finished(id);
        }
    }

    pub fn cancel(&self, id: &str) -> bool {
        let Some(id) = parse_id(id) else {
            return false;
        };
        let guard = self.inner.lock().expect("tasks");
        let Some(task) = guard.entries.get(&id) else {
            return false;
        };
        if task.state != "running" {
            return false;
        }
        task.cancel.store(true, Ordering::Relaxed);
        true
    }

    pub fn progress(&self, id: &str, message: &str) {
        let Some(id) = parse_id(id) else { return };
        let mut guard = self.inner.lock().expect("tasks");
        if let Some(task) = guard.entries.get_mut(&id) {
            if task.state == "running" {
                task.message = clip(message, 6_000);
            }
        }
    }

    pub fn snapshot(&self, id: &str) -> Option<TaskSnapshot> {
        let id = parse_id(id)?;
        self.inner
            .lock()
            .expect("tasks")
            .entries
            .get(&id)
            .map(Task::snapshot)
    }

    pub fn list(&self) -> Vec<Outcome> {
        self.inner.lock().expect("tasks").list(
            |task| task.state == "running",
            |id, task| {
                let summary = TaskSnapshot {
                    label: clip(&task.label, SUMMARY_BYTES),
                    state: task.state.clone(),
                    message: clip(&task.message, SUMMARY_BYTES),
                };
                let size = 112 + json_size(&summary.label) + json_size(&summary.message);
                (summary.into_outcome(&format!("task-{id}")), size)
            },
        )
    }
}

fn parse_id(id: &str) -> Option<u64> {
    id.strip_prefix("task-")?.parse().ok()
}

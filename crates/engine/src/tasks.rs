use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub struct TaskSnapshot {
    pub label: String,
    pub state: String,
    pub message: String,
}

struct Task {
    label: String,
    state: String,
    message: String,
    cancel: Arc<AtomicBool>,
}

pub struct TaskBoard {
    next: AtomicU64,
    inner: Mutex<HashMap<String, Task>>,
}

impl TaskBoard {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            next: AtomicU64::new(1),
            inner: Mutex::new(HashMap::new()),
        })
    }

    pub fn start(&self, label: String) -> (String, Arc<AtomicBool>) {
        let id = format!("task-{}", self.next.fetch_add(1, Ordering::Relaxed));
        let cancel = Arc::new(AtomicBool::new(false));
        let task = Task {
            label,
            state: "running".to_string(),
            message: "running".to_string(),
            cancel: Arc::clone(&cancel),
        };
        self.inner.lock().expect("tasks").insert(id.clone(), task);
        (id, cancel)
    }

    pub fn finish(&self, id: &str, state: &str, message: &str) {
        let mut guard = self.inner.lock().expect("tasks");
        let Some(task) = guard.get_mut(id) else {
            return;
        };
        if task.cancel.load(Ordering::Relaxed) && state == "running" {
            task.state = "cancelled".to_string();
            task.message = "cancelled".to_string();
            return;
        }
        task.state = state.to_string();
        task.message = clip(message);
    }

    pub fn cancel(&self, id: &str) -> bool {
        let guard = self.inner.lock().expect("tasks");
        let Some(task) = guard.get(id) else {
            return false;
        };
        if task.state != "running" {
            return false;
        }
        task.cancel.store(true, Ordering::Relaxed);
        true
    }

    pub fn snapshot(&self, id: &str) -> Option<TaskSnapshot> {
        self.inner
            .lock()
            .expect("tasks")
            .get(id)
            .map(|task| TaskSnapshot {
                label: task.label.clone(),
                state: task.state.clone(),
                message: task.message.clone(),
            })
    }
}

fn clip(message: &str) -> String {
    const MAX: usize = 6_000;
    if message.len() <= MAX {
        return message.to_string();
    }
    let tail = &message[message.len() - MAX..];
    format!("…{tail}")
}

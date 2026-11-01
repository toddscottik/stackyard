use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use localdev_core::{Error, PackageRecord};

#[derive(Clone)]
pub struct Operation {
    pub catalog_id: String,
    pub state: String,
    pub bytes: Arc<AtomicU64>,
    pub cancel: Arc<AtomicBool>,
    pub message: String,
}

pub struct Jobs {
    next: AtomicU64,
    inner: Mutex<HashMap<String, Operation>>,
}

impl Jobs {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            next: AtomicU64::new(1),
            inner: Mutex::new(HashMap::new()),
        })
    }

    pub fn running_for(&self, catalog_id: &str) -> Option<String> {
        let guard = self.inner.lock().expect("install jobs");
        guard.iter().find_map(|(id, operation)| {
            if operation.catalog_id == catalog_id && operation.state == "running" {
                Some(id.clone())
            } else {
                None
            }
        })
    }

    pub fn start(&self, catalog_id: String) -> (String, Arc<AtomicU64>, Arc<AtomicBool>) {
        let id = self.next.fetch_add(1, Ordering::Relaxed).to_string();
        let bytes = Arc::new(AtomicU64::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let operation = Operation {
            catalog_id,
            state: "running".to_string(),
            bytes: Arc::clone(&bytes),
            cancel: Arc::clone(&cancel),
            message: "downloading".to_string(),
        };
        self.inner
            .lock()
            .expect("install jobs")
            .insert(id.clone(), operation);
        (id, bytes, cancel)
    }

    pub fn finish(&self, id: &str, result: std::result::Result<PackageRecord, Error>) {
        let mut guard = self.inner.lock().expect("install jobs");
        let Some(operation) = guard.get_mut(id) else {
            return;
        };
        if operation.cancel.load(Ordering::Relaxed) {
            operation.state = "cancelled".to_string();
            operation.message = "cancelled".to_string();
            return;
        }
        match result {
            Ok(package) => {
                operation.state = "succeeded".to_string();
                operation.message = format!("{} {}", package.path, package.version);
            }
            Err(err) => {
                operation.state = "failed".to_string();
                operation.message = err.to_string();
            }
        }
    }

    pub fn cancel(&self, id: &str) -> bool {
        let guard = self.inner.lock().expect("install jobs");
        let Some(operation) = guard.get(id) else {
            return false;
        };
        operation.cancel.store(true, Ordering::Relaxed);
        true
    }

    pub fn snapshot(&self, id: &str) -> Option<Operation> {
        self.inner.lock().expect("install jobs").get(id).cloned()
    }
}

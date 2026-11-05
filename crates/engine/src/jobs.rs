use std::collections::{BTreeMap, VecDeque};
use std::hash::{BuildHasher, RandomState};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use stackyard_core::{Error, Outcome, PackageRecord};

const HISTORY_LIMIT: usize = 64;
const LIST_BUDGET: usize = 28 * 1024;
pub(crate) const SUMMARY_BYTES: usize = 512;

pub(crate) fn activity_sequence() -> AtomicU64 {
    // Keep the decimal ID format, with a fresh 40-bit namespace for each board.
    let namespace = RandomState::new().hash_one(std::process::id()) & !0xff_ffff;
    AtomicU64::new(namespace + 1)
}

#[derive(Clone)]
pub struct Operation {
    pub catalog_id: String,
    pub state: String,
    pub bytes: Arc<AtomicU64>,
    pub total: Arc<AtomicU64>,
    pub cancel: Arc<AtomicBool>,
    pub message: String,
}

impl Operation {
    pub fn outcome(&self, id: &str) -> Outcome {
        Outcome::Operation {
            id: id.to_string(),
            catalog_id: self.catalog_id.clone(),
            state: self.state.clone(),
            bytes: self.bytes.load(Ordering::Relaxed),
            total: self.total.load(Ordering::Relaxed),
            message: self.message.clone(),
        }
    }
}

pub struct Jobs {
    next: AtomicU64,
    inner: Mutex<ActivityHistory<Operation>>,
}

impl Jobs {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            next: activity_sequence(),
            inner: Mutex::new(ActivityHistory::new()),
        })
    }

    pub fn running_for(&self, catalog_id: &str) -> Option<String> {
        let guard = self.inner.lock().expect("install jobs");
        guard.entries.iter().find_map(|(id, operation)| {
            if operation.catalog_id == catalog_id && operation.state == "running" {
                Some(id.to_string())
            } else {
                None
            }
        })
    }

    pub fn start(
        &self,
        catalog_id: String,
    ) -> (String, Arc<AtomicU64>, Arc<AtomicU64>, Arc<AtomicBool>) {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let bytes = Arc::new(AtomicU64::new(0));
        let total = Arc::new(AtomicU64::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let operation = Operation {
            catalog_id,
            state: "running".to_string(),
            bytes: Arc::clone(&bytes),
            total: Arc::clone(&total),
            cancel: Arc::clone(&cancel),
            message: "downloading".to_string(),
        };
        self.inner
            .lock()
            .expect("install jobs")
            .entries
            .insert(id, operation);
        (id.to_string(), bytes, total, cancel)
    }

    pub fn finish(&self, id: &str, result: std::result::Result<PackageRecord, Error>) {
        let Ok(id) = id.parse() else {
            return;
        };
        let mut guard = self.inner.lock().expect("install jobs");
        let Some(operation) = guard.entries.get_mut(&id) else {
            return;
        };
        if operation.state != "running" {
            return;
        }
        match result {
            Ok(package) => {
                operation.state = "succeeded".to_string();
                operation.message = format!("{} {}", package.path, package.version);
            }
            Err(err @ Error::Cancelled { .. }) => {
                operation.state = "cancelled".to_string();
                operation.message = clip(&err.to_string(), 6_000);
            }
            Err(err) => {
                operation.state = "failed".to_string();
                operation.message = clip(&err.to_string(), 6_000);
            }
        }
        guard.finished(id);
    }

    pub fn cancel(&self, id: &str) -> bool {
        let Ok(id) = id.parse() else {
            return false;
        };
        let guard = self.inner.lock().expect("install jobs");
        let Some(operation) = guard.entries.get(&id) else {
            return false;
        };
        if operation.state == "running" {
            operation.cancel.store(true, Ordering::Relaxed);
        }
        true
    }

    pub fn snapshot(&self, id: &str) -> Option<Operation> {
        let id = id.parse().ok()?;
        self.inner
            .lock()
            .expect("install jobs")
            .entries
            .get(&id)
            .cloned()
    }

    pub fn list(&self) -> Vec<Outcome> {
        self.inner.lock().expect("install jobs").list(
            |operation| operation.state == "running",
            |id, operation| {
                let mut summary = operation.clone();
                summary.catalog_id = clip(&summary.catalog_id, SUMMARY_BYTES);
                summary.message = clip(&summary.message, SUMMARY_BYTES);
                // Reserve room for fixed keys, the ID, state, byte counts and punctuation.
                let size = 192 + json_size(&summary.catalog_id) + json_size(&summary.message);
                (summary.outcome(&id.to_string()), size)
            },
        )
    }
}

pub(crate) struct ActivityHistory<T> {
    pub entries: BTreeMap<u64, T>,
    finished: VecDeque<u64>,
}

impl<T> ActivityHistory<T> {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            finished: VecDeque::new(),
        }
    }

    pub fn finished(&mut self, id: u64) {
        self.finished.push_back(id);
        if self.finished.len() > HISTORY_LIMIT {
            if let Some(oldest) = self.finished.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }

    pub fn list(
        &self,
        running: impl Fn(&T) -> bool,
        summarize: impl Fn(u64, &T) -> (Outcome, usize),
    ) -> Vec<Outcome> {
        let active = self
            .entries
            .iter()
            .rev()
            .filter(|(_, entry)| running(entry));
        let finished = self.finished.iter().rev().map(|id| (id, &self.entries[id]));
        let mut budget = LIST_BUDGET;
        let mut outcomes = Vec::new();
        for (&id, entry) in active.chain(finished) {
            let (outcome, size) = summarize(id, entry);
            if size > budget {
                break;
            }
            outcomes.push(outcome);
            budget -= size;
        }
        outcomes
    }
}

pub(crate) fn clip(message: &str, max: usize) -> String {
    if message.len() <= max {
        return message.to_string();
    }
    let start = message.ceil_char_boundary(message.len() - (max - '…'.len_utf8()));
    format!("…{}", &message[start..])
}

pub(crate) fn json_size(value: &str) -> usize {
    value
        .bytes()
        .map(|byte| match byte {
            0..=31 => 6,
            b'"' | b'\\' => 2,
            _ => 1,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::TaskBoard;

    #[test]
    fn activity_history_keeps_running_ids_and_recent_completions() {
        let jobs = Jobs::new();
        let (first, bytes, total, cancel) = jobs.start("active".to_string());
        let (other_engine, ..) = Jobs::new().start("active".to_string());
        assert_ne!(first, other_engine);
        assert!(first.parse::<u64>().is_ok());
        let mut completed = Vec::new();
        for index in 0..=HISTORY_LIMIT {
            let (id, ..) = jobs.start(format!("failure-{index}"));
            jobs.finish(&id, Err(Error::InvalidMessage("failed".to_string())));
            completed.push(id);
        }
        assert!(jobs.snapshot(&completed[0]).is_none());
        bytes.store(5, Ordering::Relaxed);
        total.store(10, Ordering::Relaxed);
        assert!(matches!(jobs.list().first(), Some(Outcome::Operation {
            id, state, bytes: 5, total: 10, ..
        }) if id == &first && state == "running"));
        assert!(jobs.cancel(&first));
        assert!(cancel.load(Ordering::Relaxed));
        jobs.finish(
            &first,
            Err(Error::Cancelled {
                id: "active".to_string(),
            }),
        );
        assert!(
            matches!(jobs.list().first(), Some(Outcome::Operation { id, state, .. })
            if id == &first && state == "cancelled")
        );
    }

    #[tokio::test]
    async fn activity_summaries_fit_transport_and_keep_utf8_output() {
        let jobs = Jobs::new();
        let tasks = TaskBoard::new();
        let (running, _) = tasks.start("deps install fixture".to_string());
        let output = "\0\0\0😀".repeat(1_100);
        let mut latest = String::new();
        for _ in 0..HISTORY_LIMIT {
            let (operation, ..) = jobs.start("fixture".to_string());
            jobs.finish(&operation, Err(Error::InvalidMessage(output.clone())));
            (latest, _) = tasks.start("deps list fixture".to_string());
            tasks.finish(&latest, "succeeded", &output);
        }
        let full_output = tasks.snapshot(&latest).unwrap().message;
        assert!(full_output.starts_with('…') && full_output.ends_with('😀'));
        assert!(full_output.len() <= 6_000);
        assert!(clip(&"😀".repeat(2_000), 6_000).ends_with('😀'));
        let tasks = tasks.list();
        assert!(matches!(&tasks[0], Outcome::Task { id, .. } if id == &running));
        assert!(matches!(&tasks[1], Outcome::Task { id, message, .. }
            if id == &latest && message.len() <= SUMMARY_BYTES));
        stackyard_core::send(
            &mut tokio::io::sink(),
            &Outcome::Activity {
                operations: jobs.list(),
                tasks,
            },
        )
        .await
        .unwrap();
    }
}

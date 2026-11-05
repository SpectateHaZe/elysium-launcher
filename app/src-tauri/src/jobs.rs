//! Background jobs (content installs, pack imports, Horizon updates) the UI polls.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use elysium_core::Progress;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobState {
    Running,
    Done { result: Value },
    Failed { message: String },
}

pub struct Job {
    pub title: String,
    pub progress: Arc<Progress>,
    pub state: Arc<Mutex<JobState>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobView {
    pub id: u64,
    pub title: String,
    pub stage: String,
    pub done: u64,
    pub total: u64,
    pub state: JobState,
}

#[derive(Default)]
pub struct Jobs {
    next: AtomicU64,
    jobs: Mutex<HashMap<u64, Job>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Jobs {
    /// Registers a job and returns its id, progress and state handles.
    pub fn start(&self, title: impl Into<String>) -> (u64, Arc<Progress>, Arc<Mutex<JobState>>) {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let progress = Arc::new(Progress::new());
        let state = Arc::new(Mutex::new(JobState::Running));
        lock(&self.jobs).insert(
            id,
            Job {
                title: title.into(),
                progress: Arc::clone(&progress),
                state: Arc::clone(&state),
            },
        );
        (id, progress, state)
    }

    pub fn views(&self) -> Vec<JobView> {
        let mut v: Vec<JobView> = lock(&self.jobs)
            .iter()
            .map(|(id, j)| {
                let (done, total) = j.progress.counts();
                JobView {
                    id: *id,
                    title: j.title.clone(),
                    stage: j.progress.stage(),
                    done,
                    total,
                    state: lock(&j.state).clone(),
                }
            })
            .collect();
        v.sort_by_key(|j| j.id);
        v
    }

    pub fn cancel(&self, id: u64) {
        if let Some(j) = lock(&self.jobs).get(&id) {
            j.progress.cancel();
        }
    }

    pub fn dismiss(&self, id: u64) {
        let mut jobs = lock(&self.jobs);
        if jobs
            .get(&id)
            .is_some_and(|j| !matches!(*lock(&j.state), JobState::Running))
        {
            jobs.remove(&id);
        }
    }
}

/// Finishes a job from a result.
pub fn finish<T: Serialize>(state: &Mutex<JobState>, result: elysium_core::Result<T>) {
    *lock(state) = match result {
        Ok(v) => JobState::Done {
            result: serde_json::to_value(v).unwrap_or(Value::Null),
        },
        Err(elysium_core::Error::Cancelled) => JobState::Failed {
            message: "Cancelled".into(),
        },
        Err(e) => JobState::Failed {
            message: e.to_string(),
        },
    };
}

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::error::{Error, Result};

/// Progress of a long-running task, shared between a worker and the UI.
///
/// Designed for polling from an immediate-mode UI: all fields are cheap to read.
#[derive(Debug, Default)]
pub struct Progress {
    stage: Mutex<String>,
    done: AtomicU64,
    total: AtomicU64,
    cancelled: AtomicBool,
}

impl Progress {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a new stage and resets the counters.
    pub fn set_stage(&self, stage: impl Into<String>, total: u64) {
        *self.stage.lock().unwrap_or_else(|e| e.into_inner()) = stage.into();
        self.done.store(0, Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
    }

    pub fn add_total(&self, n: u64) {
        self.total.fetch_add(n, Ordering::Relaxed);
    }

    pub fn advance(&self, n: u64) {
        self.done.fetch_add(n, Ordering::Relaxed);
    }

    pub fn stage(&self) -> String {
        self.stage.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn counts(&self) -> (u64, u64) {
        (
            self.done.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
        )
    }

    /// Fraction in `0.0..=1.0`, or `None` when the total is unknown.
    pub fn fraction(&self) -> Option<f32> {
        let (done, total) = self.counts();
        (total > 0).then(|| (done as f64 / total as f64).min(1.0) as f32)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Returns `Err(Cancelled)` if cancellation was requested.
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
}

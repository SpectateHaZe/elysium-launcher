//! Game sessions: install + run of one instance, with progress and a sequenced log
//! buffer the UI reads incrementally.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use elysium_core::Progress;
use elysium_core::logs::{LogLevel, LogLine, LogSource};
use serde::Serialize;
use tokio::sync::oneshot;

/// Lines kept per session; older lines are dropped.
const LOG_CAPACITY: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Phase {
    Preparing,
    Running { since: DateTime<Utc> },
    Finished,
    Crashed { code: Option<i32> },
    Failed { message: String },
    Stopped,
}

impl Phase {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Preparing | Self::Running { .. })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub seq: u64,
    pub level: &'static str,
    pub source: &'static str,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct LogBuffer {
    lines: VecDeque<LogEntry>,
    next_seq: u64,
}

impl LogBuffer {
    pub fn push(&mut self, line: LogLine) {
        if self.lines.len() >= LOG_CAPACITY {
            self.lines.pop_front();
        }
        self.lines.push_back(LogEntry {
            seq: self.next_seq,
            level: match line.level {
                LogLevel::Debug => "debug",
                LogLevel::Info => "info",
                LogLevel::Warn => "warn",
                LogLevel::Error => "error",
            },
            source: match line.source {
                LogSource::Game => "game",
                LogSource::Launcher => "launcher",
            },
            text: line.text,
        });
        self.next_seq += 1;
    }

    /// Lines with `seq >= after`, at most `limit`.
    pub fn since(&self, after: u64, limit: usize) -> Vec<LogEntry> {
        let first = self.lines.front().map_or(0, |l| l.seq);
        let skip = after.saturating_sub(first) as usize;
        self.lines.iter().skip(skip).take(limit).cloned().collect()
    }

    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }
}

pub struct Session {
    pub phase: Arc<Mutex<Phase>>,
    pub progress: Arc<Progress>,
    pub logs: Arc<Mutex<LogBuffer>>,
    pub kill: Option<oneshot::Sender<()>>,
}

impl Session {
    pub fn phase(&self) -> Phase {
        self.phase.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionView {
    pub phase: Phase,
    pub stage: String,
    pub done: u64,
    pub total: u64,
    pub log_seq: u64,
}

pub type Sessions = Mutex<HashMap<String, Session>>;

pub fn view(session: &Session) -> SessionView {
    let (done, total) = session.progress.counts();
    SessionView {
        phase: session.phase(),
        stage: session.progress.stage(),
        done,
        total,
        log_seq: session
            .logs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .next_seq(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_buffer_is_incremental_and_bounded() {
        let mut buf = LogBuffer::default();
        for i in 0..LOG_CAPACITY + 5 {
            buf.push(LogLine::launcher(LogLevel::Info, format!("line {i}")));
        }
        assert_eq!(buf.next_seq(), (LOG_CAPACITY + 5) as u64);
        // The first five lines were dropped; asking for them starts at the oldest kept.
        assert_eq!(buf.since(0, 1)[0].seq, 5);
        let tail = buf.since(buf.next_seq() - 2, 10);
        assert_eq!(
            tail.iter().map(|l| l.seq).collect::<Vec<_>>(),
            [LOG_CAPACITY as u64 + 3, LOG_CAPACITY as u64 + 4]
        );
        assert!(buf.since(buf.next_seq(), 10).is_empty());
    }
}

//! Bridge between the UI thread and async work on a Tokio runtime.
//!
//! The UI never blocks: long operations run on the runtime and report back through an
//! event channel (drained once per frame) or shared state the UI polls (progress, logs).

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use eframe::egui;
use elysium_core::account::Account;
use elysium_core::http::{Http, Verify};
use elysium_core::install::{self, InstallOptions};
use elysium_core::instance::{Instance, InstanceManager};
use elysium_core::java::{self, JavaInfo};
use elysium_core::launch::{self, GameExit};
use elysium_core::logs::{LogLevel, LogLine};
use elysium_core::meta::VersionManifest;
use elysium_core::settings::Settings;
use elysium_core::{Error, Paths, Progress, versions};
use tokio::sync::oneshot;

/// Lines kept per game session; older lines are dropped.
const LOG_CAPACITY: usize = 20_000;

#[derive(Debug)]
pub enum Event {
    Manifest(Result<VersionManifest, String>),
    SessionEnded {
        instance_id: String,
        outcome: Result<GameExit, String>,
    },
    GameStarted,
    JavaProbed {
        path: PathBuf,
        result: Result<JavaInfo, String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    Preparing,
    Running { since: DateTime<Utc> },
    Finished,
    Failed(String),
}

/// One play session of an instance: install, run, and its log.
pub struct Session {
    pub phase: Arc<Mutex<Phase>>,
    pub progress: Arc<Progress>,
    pub logs: Arc<Mutex<VecDeque<LogLine>>>,
    kill: Option<oneshot::Sender<()>>,
}

impl Session {
    pub fn phase(&self) -> Phase {
        self.phase.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn is_active(&self) -> bool {
        matches!(self.phase(), Phase::Preparing | Phase::Running { .. })
    }
}

pub struct Backend {
    runtime: tokio::runtime::Runtime,
    pub paths: Paths,
    http: Http,
    pub instances: InstanceManager,
    ctx: egui::Context,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    pub sessions: HashMap<String, Session>,
}

impl Backend {
    pub fn new(ctx: egui::Context) -> Result<Self, Error> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("elysium-worker")
            .build()
            .map_err(|e| Error::Invalid(format!("could not start async runtime: {e}")))?;
        let paths = Paths::from_env()?;
        std::fs::create_dir_all(paths.root()).map_err(|source| Error::Io {
            path: paths.root().to_owned(),
            source,
        })?;
        let (tx, rx) = channel();
        Ok(Self {
            http: Http::new()?,
            instances: InstanceManager::new(paths.instances_dir()),
            runtime,
            paths,
            ctx,
            tx,
            rx,
            sessions: HashMap::new(),
        })
    }

    /// Events produced since the last call.
    pub fn poll(&self) -> Vec<Event> {
        self.rx.try_iter().collect()
    }

    fn emitter(&self) -> impl Fn(Event) + Send + Sync + Clone + 'static {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        move |event| {
            // The receiver only disappears when the app is closing.
            let _ = tx.send(event);
            ctx.request_repaint();
        }
    }

    pub fn load_manifest(&self) {
        let (paths, http, emit) = (self.paths.clone(), self.http.clone(), self.emitter());
        self.runtime.spawn(async move {
            let result = versions::fetch_manifest(&paths, &http)
                .await
                .map_err(|e| e.to_string());
            emit(Event::Manifest(result));
        });
    }

    pub fn probe_java(&self, path: PathBuf) {
        let emit = self.emitter();
        self.runtime.spawn(async move {
            let result = java::probe(&path).await.map_err(|e| e.to_string());
            emit(Event::JavaProbed { path, result });
        });
    }

    pub fn is_running(&self, instance_id: &str) -> bool {
        self.sessions
            .get(instance_id)
            .is_some_and(Session::is_active)
    }

    pub fn any_running(&self) -> bool {
        self.sessions.values().any(Session::is_active)
    }

    /// Installs (if needed) and starts `instance`. `repair` re-hashes every file.
    pub fn launch(
        &mut self,
        instance: &Instance,
        account: &Account,
        settings: &Settings,
        repair: bool,
    ) {
        if self.is_running(&instance.id) {
            return;
        }
        let (kill_tx, kill_rx) = oneshot::channel();
        let session = Session {
            phase: Arc::new(Mutex::new(Phase::Preparing)),
            progress: Arc::new(Progress::new()),
            logs: Arc::new(Mutex::new(VecDeque::new())),
            kill: Some(kill_tx),
        };
        let phase = Arc::clone(&session.phase);
        let progress = Arc::clone(&session.progress);
        let logs = Arc::clone(&session.logs);
        self.sessions.insert(instance.id.clone(), session);

        let effective = settings.effective(&instance.config.overrides);
        let options = InstallOptions {
            java_path: effective.java_path.clone(),
            concurrency: settings.concurrent_downloads.clamp(1, 64),
            verify: if repair { Verify::Hash } else { Verify::Size },
        };
        let (paths, http, emit) = (self.paths.clone(), self.http.clone(), self.emitter());
        let instances = self.instances.clone();
        let (instance, account) = (instance.clone(), account.clone());
        let ctx = self.ctx.clone();

        self.runtime.spawn(async move {
            let push = {
                let logs = Arc::clone(&logs);
                let ctx = ctx.clone();
                Arc::new(move |line: LogLine| {
                    let mut logs = logs.lock().unwrap_or_else(|e| e.into_inner());
                    if logs.len() >= LOG_CAPACITY {
                        logs.pop_front();
                    }
                    logs.push_back(line);
                    drop(logs);
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                })
            };
            let set_phase = |p: Phase| *phase.lock().unwrap_or_else(|e| e.into_inner()) = p;
            push(LogLine::launcher(
                LogLevel::Info,
                format!("Preparing Minecraft {}", instance.config.game_version),
            ));

            let prepared =
                match install::prepare(&paths, &http, &instance, &options, Arc::clone(&progress))
                    .await
                {
                    Ok(p) => p,
                    Err(e) => {
                        let msg = e.to_string();
                        push(LogLine::launcher(
                            LogLevel::Error,
                            format!("Launch failed: {msg}"),
                        ));
                        set_phase(Phase::Failed(msg.clone()));
                        emit(Event::SessionEnded {
                            instance_id: instance.id,
                            outcome: Err(msg),
                        });
                        return;
                    }
                };
            let command = launch::build_command(&prepared, &instance, &account, &effective);
            push(LogLine::launcher(
                LogLevel::Info,
                format!(
                    "Java: {}  |  Memory: {}–{} MB",
                    prepared.java.display(),
                    effective.min_memory_mb,
                    effective.max_memory_mb
                ),
            ));

            let started = Utc::now();
            set_phase(Phase::Running { since: started });
            emit(Event::GameStarted);
            let outcome = launch::run(&command, push.clone(), kill_rx).await;

            match &outcome {
                Ok(exit) => {
                    if let Err(e) =
                        instances.record_session(&instance.id, started, exit.duration.as_secs())
                    {
                        tracing::warn!("could not save playtime: {e}");
                    }
                    let (level, text) = if exit.killed {
                        (LogLevel::Warn, "Game was stopped".to_owned())
                    } else if exit.success() {
                        (LogLevel::Info, "Game closed".to_owned())
                    } else {
                        (
                            LogLevel::Error,
                            format!(
                                "Game exited with code {}",
                                exit.code.map_or("?".into(), |c| c.to_string())
                            ),
                        )
                    };
                    push(LogLine::launcher(level, text));
                    set_phase(if exit.success() {
                        Phase::Finished
                    } else {
                        Phase::Failed("The game crashed".into())
                    });
                }
                Err(e) => {
                    push(LogLine::launcher(
                        LogLevel::Error,
                        format!("Could not start the game: {e}"),
                    ));
                    set_phase(Phase::Failed(e.to_string()));
                }
            }
            emit(Event::SessionEnded {
                instance_id: instance.id,
                outcome: outcome.map_err(|e| e.to_string()),
            });
        });
    }

    /// Cancels installation or kills the running game.
    pub fn stop(&mut self, instance_id: &str) {
        if let Some(session) = self.sessions.get_mut(instance_id) {
            session.progress.cancel();
            if let Some(kill) = session.kill.take() {
                let _ = kill.send(());
            }
        }
    }

    pub fn clear_logs(&self, instance_id: &str) {
        if let Some(s) = self.sessions.get(instance_id) {
            s.logs.lock().unwrap_or_else(|e| e.into_inner()).clear();
        }
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        // Running games are separate processes and keep running; only unfinished
        // installs are cancelled.
        for session in self.sessions.values() {
            if session.phase() == Phase::Preparing {
                session.progress.cancel();
            }
        }
    }
}

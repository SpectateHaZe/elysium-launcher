//! Commands invoked from the web UI. Errors are returned as user-facing strings.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use elysium_core::account::{Account, AccountStore};
use elysium_core::http::{Http, Verify};
use elysium_core::install::{self, InstallOptions};
use elysium_core::instance::{Instance, InstanceManager, InstanceOverrides};
use elysium_core::launch;
use elysium_core::logs::{LogLevel, LogLine};
use elysium_core::meta::VersionManifest;
use elysium_core::settings::Settings;
use elysium_core::{Paths, Progress, java, versions};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::sessions::{self, LogBuffer, LogEntry, Phase, Session, SessionView, Sessions};

pub struct AppState {
    pub paths: Paths,
    pub http: Http,
    pub instances: InstanceManager,
    pub settings: Mutex<Settings>,
    pub accounts: Mutex<AccountStore>,
    pub sessions: Sessions,
}

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Debug, Clone, Serialize)]
pub struct InstanceView {
    pub id: String,
    pub name: String,
    pub game_version: String,
    pub material: Option<String>,
    pub created: DateTime<Utc>,
    pub last_played: Option<DateTime<Utc>>,
    pub playtime_secs: u64,
    pub overrides: InstanceOverrides,
}

impl From<Instance> for InstanceView {
    fn from(i: Instance) -> Self {
        Self {
            id: i.id,
            name: i.config.name,
            game_version: i.config.game_version,
            material: i.config.material,
            created: i.config.created,
            last_played: i.config.last_played,
            playtime_secs: i.config.playtime_secs,
            overrides: i.config.overrides,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Bootstrap {
    pub settings: Settings,
    pub accounts: AccountStore,
    pub instances: Vec<InstanceView>,
    pub data_dir: PathBuf,
    pub version: &'static str,
}

#[tauri::command]
pub fn bootstrap(state: State<'_, AppState>) -> CmdResult<Bootstrap> {
    Ok(Bootstrap {
        settings: lock(&state.settings).clone(),
        accounts: lock(&state.accounts).clone(),
        instances: list(&state)?,
        data_dir: state.paths.root().to_owned(),
        version: env!("CARGO_PKG_VERSION"),
    })
}

fn list(state: &AppState) -> CmdResult<Vec<InstanceView>> {
    Ok(state
        .instances
        .list()
        .map_err(err)?
        .into_iter()
        .map(Into::into)
        .collect())
}

#[tauri::command]
pub fn instances(state: State<'_, AppState>) -> CmdResult<Vec<InstanceView>> {
    list(&state)
}

#[tauri::command]
pub async fn versions(state: State<'_, AppState>) -> CmdResult<VersionManifest> {
    versions::fetch_manifest(&state.paths, &state.http)
        .await
        .map_err(err)
}

#[tauri::command]
pub fn create_instance(
    state: State<'_, AppState>,
    name: String,
    version: String,
    material: Option<String>,
) -> CmdResult<InstanceView> {
    let inst = state.instances.create(&name, &version).map_err(err)?;
    let inst = state
        .instances
        .update(&inst.id, |c| c.material = material)
        .map_err(err)?;
    Ok(inst.into())
}

#[tauri::command]
pub fn update_instance(
    state: State<'_, AppState>,
    id: String,
    name: String,
    material: Option<String>,
    overrides: InstanceOverrides,
) -> CmdResult<InstanceView> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err("Give the instance a name".into());
    }
    state
        .instances
        .update(&id, |c| {
            c.name = name;
            c.material = material;
            c.overrides = overrides;
        })
        .map(Into::into)
        .map_err(err)
}

#[tauri::command]
pub fn delete_instance(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let mut sessions = lock(&state.sessions);
    if sessions.get(&id).is_some_and(|s| s.phase().is_active()) {
        return Err("Stop the game before deleting this instance".into());
    }
    state.instances.delete(&id).map_err(err)?;
    sessions.remove(&id);
    Ok(())
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, mut settings: Settings) -> CmdResult<Settings> {
    settings.appearance.sanitize();
    settings.save(&state.paths).map_err(err)?;
    *lock(&state.settings) = settings.clone();
    Ok(settings)
}

#[tauri::command]
pub fn add_offline_account(
    state: State<'_, AppState>,
    username: String,
) -> CmdResult<AccountStore> {
    let account = Account::offline(&username).map_err(err)?;
    let mut store = lock(&state.accounts);
    store.add(account);
    store.save(&state.paths).map_err(err)?;
    Ok(store.clone())
}

#[tauri::command]
pub fn remove_account(state: State<'_, AppState>, uuid: Uuid) -> CmdResult<AccountStore> {
    let mut store = lock(&state.accounts);
    store.remove(uuid);
    store.save(&state.paths).map_err(err)?;
    Ok(store.clone())
}

#[tauri::command]
pub fn set_active_account(state: State<'_, AppState>, uuid: Uuid) -> CmdResult<AccountStore> {
    let mut store = lock(&state.accounts);
    if !store.accounts.iter().any(|a| a.uuid == uuid) {
        return Err("That account no longer exists".into());
    }
    store.active = Some(uuid);
    store.save(&state.paths).map_err(err)?;
    Ok(store.clone())
}

#[derive(Debug, Serialize)]
pub struct JavaProbe {
    pub version: String,
    pub major: u32,
    pub arch: String,
}

#[tauri::command]
pub async fn probe_java(path: PathBuf) -> CmdResult<JavaProbe> {
    let info = java::probe(&path).await.map_err(err)?;
    Ok(JavaProbe {
        version: info.version,
        major: info.major,
        arch: info.arch,
    })
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderTarget {
    Instance,
    Logs,
    Data,
}

#[tauri::command]
pub fn open_folder(
    state: State<'_, AppState>,
    target: FolderTarget,
    id: Option<String>,
) -> CmdResult<()> {
    let path = match target {
        FolderTarget::Data => state.paths.root().to_owned(),
        FolderTarget::Instance | FolderTarget::Logs => {
            let id = id.ok_or("missing instance id")?;
            let game_dir = state.instances.get(&id).map_err(err)?.game_dir();
            if matches!(target, FolderTarget::Logs) {
                game_dir.join("logs")
            } else {
                game_dir
            }
        }
    };
    std::fs::create_dir_all(&path).map_err(err)?;
    open::that_detached(&path).map_err(|e| format!("Couldn't open {}: {e}", path.display()))
}

#[tauri::command]
pub fn sessions(state: State<'_, AppState>) -> std::collections::HashMap<String, SessionView> {
    lock(&state.sessions)
        .iter()
        .map(|(id, s)| (id.clone(), sessions::view(s)))
        .collect()
}

#[tauri::command]
pub fn logs(state: State<'_, AppState>, id: String, after: u64) -> Vec<LogEntry> {
    lock(&state.sessions)
        .get(&id)
        .map(|s| lock(&s.logs).since(after, 5_000))
        .unwrap_or_default()
}

#[tauri::command]
pub fn clear_logs(state: State<'_, AppState>, id: String) {
    if let Some(s) = lock(&state.sessions).get(&id) {
        lock(&s.logs).clear();
    }
}

#[tauri::command]
pub fn stop(state: State<'_, AppState>, id: String) {
    if let Some(s) = lock(&state.sessions).get_mut(&id) {
        s.progress.cancel();
        if let Some(kill) = s.kill.take() {
            let _ = kill.send(());
        }
    }
}

/// Installs anything missing, then starts the game. `repair` re-hashes every file.
#[tauri::command]
pub fn launch(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    repair: bool,
) -> CmdResult<()> {
    let instance = state.instances.get(&id).map_err(err)?;
    let account = lock(&state.accounts)
        .active()
        .cloned()
        .ok_or("Add an account before playing")?;
    let settings = lock(&state.settings).clone();

    let (kill_tx, kill_rx) = oneshot::channel();
    let session = Session {
        phase: Arc::new(Mutex::new(Phase::Preparing)),
        progress: Arc::new(Progress::new()),
        logs: Arc::new(Mutex::new(LogBuffer::default())),
        kill: Some(kill_tx),
    };
    let (phase, progress, logs) = (
        Arc::clone(&session.phase),
        Arc::clone(&session.progress),
        Arc::clone(&session.logs),
    );
    {
        let mut sessions = lock(&state.sessions);
        if sessions.get(&id).is_some_and(|s| s.phase().is_active()) {
            return Ok(());
        }
        sessions.insert(id.clone(), session);
    }

    let effective = settings.effective(&instance.config.overrides);
    let options = InstallOptions {
        java_path: effective.java_path.clone(),
        concurrency: settings.concurrent_downloads.clamp(1, 64),
        verify: if repair { Verify::Hash } else { Verify::Size },
    };
    let (paths, http, instances) = (
        state.paths.clone(),
        state.http.clone(),
        state.instances.clone(),
    );
    let minimize = settings.hide_on_launch;

    tauri::async_runtime::spawn(async move {
        let push = {
            let logs = Arc::clone(&logs);
            Arc::new(move |line: LogLine| lock(&logs).push(line))
        };
        let set_phase = |p: Phase| *lock(&phase) = p;
        let notify = |app: &AppHandle| {
            let _ = app.emit("session-ended", &instance.id);
        };
        push(LogLine::launcher(
            LogLevel::Info,
            format!("Preparing Minecraft {}", instance.config.game_version),
        ));

        let prepared =
            match install::prepare(&paths, &http, &instance, &options, Arc::clone(&progress)).await
            {
                Ok(p) => p,
                Err(elysium_core::Error::Cancelled) => {
                    push(LogLine::launcher(LogLevel::Warn, "Launch cancelled"));
                    set_phase(Phase::Stopped);
                    notify(&app);
                    return;
                }
                Err(e) => {
                    push(LogLine::launcher(
                        LogLevel::Error,
                        format!("Launch failed: {e}"),
                    ));
                    set_phase(Phase::Failed {
                        message: e.to_string(),
                    });
                    notify(&app);
                    return;
                }
            };
        let command = launch::build_command(&prepared, &instance, &account, &effective);
        push(LogLine::launcher(
            LogLevel::Info,
            format!(
                "Java {}, memory {}–{} MB",
                prepared.java.display(),
                effective.min_memory_mb,
                effective.max_memory_mb
            ),
        ));

        let started = Utc::now();
        set_phase(Phase::Running { since: started });
        let window = app.get_webview_window("main");
        if minimize && let Some(w) = &window {
            let _ = w.minimize();
        }
        let outcome = launch::run(&command, push.clone(), kill_rx).await;
        if minimize && let Some(w) = &window {
            let _ = w.unminimize();
        }

        match outcome {
            Ok(exit) => {
                if let Err(e) =
                    instances.record_session(&instance.id, started, exit.duration.as_secs())
                {
                    tracing::warn!("could not save playtime: {e}");
                }
                let phase = if exit.killed {
                    push(LogLine::launcher(LogLevel::Warn, "Game stopped"));
                    Phase::Stopped
                } else if exit.success() {
                    push(LogLine::launcher(LogLevel::Info, "Game closed"));
                    Phase::Finished
                } else {
                    push(LogLine::launcher(
                        LogLevel::Error,
                        format!(
                            "Game exited with code {}",
                            exit.code.map_or("unknown".into(), |c| c.to_string())
                        ),
                    ));
                    Phase::Crashed { code: exit.code }
                };
                set_phase(phase);
            }
            Err(e) => {
                push(LogLine::launcher(
                    LogLevel::Error,
                    format!("Couldn't start the game: {e}"),
                ));
                set_phase(Phase::Failed {
                    message: e.to_string(),
                });
            }
        }
        notify(&app);
    });
    Ok(())
}

pub fn any_active(state: &AppState) -> bool {
    lock(&state.sessions)
        .values()
        .any(|s| s.phase().is_active())
}

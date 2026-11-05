//! Commands invoked from the web UI. Errors are returned as user-facing strings.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use elysium_core::account::{Account, AccountKind, AccountStore, LaunchSession};
use elysium_core::auth::{DeviceCode, MicrosoftAuth, MicrosoftTokens};
use elysium_core::content::install::{
    self as content_install, ContentService, InstalledContent, ManualDownload, SourceFilter,
};
use elysium_core::content::modpack;
use elysium_core::content::{
    ContentKind, ProjectDetails, ProjectVersion, SearchPage, SearchQuery, Source,
};
use elysium_core::http::{Http, Verify};
use elysium_core::install::{self, InstallOptions};
use elysium_core::instance::{Instance, InstanceManager, InstanceOverrides, ModLoader, PackOrigin};
use elysium_core::launch;
use elysium_core::loaders::{self, LoaderVersion};
use elysium_core::logs::{LogLevel, LogLine};
use elysium_core::meta::VersionManifest;
use elysium_core::secrets::SecretStore;
use elysium_core::settings::Settings;
use elysium_core::{Paths, Progress, horizon, java, versions};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::jobs::{self, JobView, Jobs};
use crate::sessions::{self, LogBuffer, LogEntry, Phase, Session, SessionView, Sessions};

const CURSEFORGE_KEY: &str = "curseforge-api-key";

fn refresh_key(uuid: Uuid) -> String {
    format!("msa-refresh:{}", uuid.simple())
}

#[derive(Default)]
pub struct MsaLogin {
    pub code: Option<DeviceCode>,
    pub cancel: Arc<AtomicBool>,
    pub result: Option<Result<(), String>>,
}

pub struct AppState {
    pub paths: Paths,
    pub http: Http,
    pub instances: InstanceManager,
    pub settings: Mutex<Settings>,
    pub accounts: Mutex<AccountStore>,
    pub sessions: Sessions,
    pub secrets: SecretStore,
    pub tokens: Mutex<HashMap<Uuid, MicrosoftTokens>>,
    pub jobs: Jobs,
    pub msa: Mutex<MsaLogin>,
}

impl AppState {
    fn content(&self) -> ContentService {
        ContentService::new(
            self.http.clone(),
            &self.secrets.get(CURSEFORGE_KEY).unwrap_or_default(),
        )
    }

    fn msa(&self) -> CmdResult<MicrosoftAuth> {
        MicrosoftAuth::new(self.http.clone(), &lock(&self.settings).msa_client_id).map_err(err)
    }

    fn install_options(&self, java_path: Option<PathBuf>, verify: Verify) -> InstallOptions {
        InstallOptions {
            java_path,
            concurrency: lock(&self.settings).concurrent_downloads.clamp(1, 64),
            verify,
        }
    }

    /// The launch session for `account`, refreshing Microsoft tokens when needed.
    async fn session_for(&self, account: &Account) -> CmdResult<LaunchSession> {
        if account.kind == AccountKind::Offline {
            return Ok(LaunchSession::offline(account));
        }
        if let Some(t) = lock(&self.tokens)
            .get(&account.uuid)
            .filter(|t| t.is_fresh())
        {
            return Ok(t.session(account));
        }
        let refresh = self
            .secrets
            .get(&refresh_key(account.uuid))
            .ok_or("Sign in to this Microsoft account again")?;
        let (fresh, tokens) = self.msa()?.refresh(&refresh).await.map_err(err)?;
        self.store_microsoft(fresh.clone(), tokens.clone())?;
        Ok(tokens.session(&fresh))
    }

    fn store_microsoft(
        &self,
        account: Account,
        tokens: MicrosoftTokens,
    ) -> CmdResult<AccountStore> {
        self.secrets
            .set(&refresh_key(account.uuid), &tokens.refresh_token);
        lock(&self.tokens).insert(account.uuid, tokens);
        let mut store = lock(&self.accounts);
        let active = store.active;
        store.add(account.clone());
        // Refreshing shouldn't switch the active account.
        if active.is_some()
            && active != Some(account.uuid)
            && store.accounts.iter().any(|a| Some(a.uuid) == active)
        {
            store.active = active;
        }
        store.save(&self.paths).map_err(err)?;
        Ok(store.clone())
    }
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
    pub loader: ModLoader,
    pub loader_version: Option<String>,
    pub pack: Option<PackOrigin>,
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
            loader: i.config.loader,
            loader_version: i.config.loader_version,
            pack: i.config.pack,
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
    pub curseforge_configured: bool,
    pub horizon_supported: bool,
    pub accounts: AccountStore,
    pub instances: Vec<InstanceView>,
    pub data_dir: PathBuf,
    pub version: &'static str,
}

#[tauri::command]
pub fn bootstrap(state: State<'_, AppState>) -> CmdResult<Bootstrap> {
    Ok(Bootstrap {
        settings: lock(&state.settings).clone(),
        curseforge_configured: state
            .secrets
            .get(CURSEFORGE_KEY)
            .is_some_and(|k| !k.is_empty()),
        horizon_supported: horizon::platform_archive().is_some(),
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
    loader: Option<ModLoader>,
    loader_version: Option<String>,
) -> CmdResult<InstanceView> {
    let inst = state
        .instances
        .create_with(&name, &version, loader.unwrap_or_default(), loader_version)
        .map_err(err)?;
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
    state.secrets.delete(&refresh_key(uuid));
    lock(&state.tokens).remove(&uuid);
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
pub async fn launch(
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
    let horizon_instance = instance.config.loader == ModLoader::Horizon;
    if horizon_instance && !(account.kind == AccountKind::Microsoft && account.owns_game) {
        return Err("Horizon needs a Microsoft account that owns Minecraft: Java Edition".into());
    }
    let launch_session = state.session_for(&account).await?;
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
    let options = state.install_options(
        effective.java_path.clone(),
        if repair { Verify::Hash } else { Verify::Size },
    );
    let (paths, http, instances) = (
        state.paths.clone(),
        state.http.clone(),
        state.instances.clone(),
    );
    let minimize = settings.hide_on_launch;
    let auto_update = settings.horizon_auto_update;

    tauri::async_runtime::spawn(async move {
        let push = {
            let logs = Arc::clone(&logs);
            Arc::new(move |line: LogLine| lock(&logs).push(line))
        };
        let set_phase = |p: Phase| *lock(&phase) = p;
        let notify = |app: &AppHandle| {
            let _ = app.emit("session-ended", &instance.id);
        };
        let what = if horizon_instance {
            "Horizon".to_owned()
        } else {
            format!("Minecraft {}", instance.config.game_version)
        };
        push(LogLine::launcher(
            LogLevel::Info,
            format!("Preparing {what}"),
        ));

        let command = async {
            if horizon_instance {
                let installed = horizon::state(&paths)?.current.is_some();
                if auto_update || !installed {
                    match horizon::install_latest(&paths, &http, Arc::clone(&progress)).await {
                        Ok(v) => push(LogLine::launcher(LogLevel::Info, format!("Horizon {v}"))),
                        // An installed build still works if the update check fails.
                        Err(e) if installed && !matches!(e, elysium_core::Error::Cancelled) => {
                            push(LogLine::launcher(
                                LogLevel::Warn,
                                format!("Couldn't update Horizon: {e}"),
                            ))
                        }
                        Err(e) => return Err(e),
                    }
                }
                let build = horizon::build_info(&paths).await?;
                let resources = horizon::ensure_prepared(
                    &paths,
                    &http,
                    &build,
                    true,
                    &options,
                    Arc::clone(&progress),
                )
                .await?;
                Ok(horizon::build_command(
                    &build,
                    &resources,
                    &instance,
                    &launch_session,
                    &effective,
                ))
            } else {
                let prepared =
                    install::prepare(&paths, &http, &instance, &options, Arc::clone(&progress))
                        .await?;
                push(LogLine::launcher(
                    LogLevel::Info,
                    format!(
                        "Java {}, memory {}–{} MB",
                        prepared.java.display(),
                        effective.min_memory_mb,
                        effective.max_memory_mb
                    ),
                ));
                Ok(launch::build_command(
                    &prepared,
                    &instance,
                    &launch_session,
                    &effective,
                ))
            }
        }
        .await;
        let command = match command {
            Ok(c) => c,
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
                    let detail = if horizon_instance {
                        horizon::exit_message(exit.code)
                    } else {
                        None
                    };
                    push(LogLine::launcher(
                        LogLevel::Error,
                        format!(
                            "Game exited with code {}{}",
                            exit.code.map_or("unknown".into(), |c| c.to_string()),
                            detail.map(|d| format!(". {d}")).unwrap_or_default()
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

// ---------------------------------------------------------------------------
// Loaders

#[tauri::command]
pub async fn loader_versions(
    state: State<'_, AppState>,
    loader: ModLoader,
    mc: String,
) -> CmdResult<Vec<LoaderVersion>> {
    loaders::versions(&state.http, loader, &mc)
        .await
        .map_err(err)
}

// ---------------------------------------------------------------------------
// Content

#[tauri::command]
pub async fn search(
    state: State<'_, AppState>,
    source: SourceFilter,
    query: SearchQuery,
) -> CmdResult<SearchPage> {
    state.content().search(source, &query).await.map_err(err)
}

#[tauri::command]
pub async fn project(
    state: State<'_, AppState>,
    source: Source,
    id: String,
) -> CmdResult<ProjectDetails> {
    state.content().project(source, &id).await.map_err(err)
}

#[tauri::command]
pub async fn project_versions(
    state: State<'_, AppState>,
    source: Source,
    id: String,
    kind: ContentKind,
    instance_id: Option<String>,
) -> CmdResult<Vec<ProjectVersion>> {
    let content = state.content();
    match instance_id {
        Some(iid) => {
            let inst = state.instances.get(&iid).map_err(err)?;
            content
                .versions(source, &id, kind, &inst)
                .await
                .map_err(err)
        }
        None => match source {
            Source::Modrinth => content
                .modrinth
                .versions(&id, kind, None, None)
                .await
                .map_err(err),
            Source::CurseForge => content
                .curseforge
                .as_ref()
                .ok_or("Add a CurseForge API key in Settings")?
                .versions(&id, kind, None, None)
                .await
                .map_err(err),
        },
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct InstallRequest {
    pub instance_id: String,
    pub source: Source,
    pub project_id: String,
    pub version_id: Option<String>,
    pub kind: ContentKind,
    pub world: Option<String>,
    pub title: String,
}

#[tauri::command]
pub fn install_content(state: State<'_, AppState>, request: InstallRequest) -> CmdResult<u64> {
    let inst = state.instances.get(&request.instance_id).map_err(err)?;
    let content = state.content();
    let (id, progress, job) = state.jobs.start(format!("Installing {}", request.title));
    tauri::async_runtime::spawn(async move {
        let r = content
            .install(
                &inst,
                request.source,
                &request.project_id,
                request.version_id.as_deref(),
                request.kind,
                request.world.as_deref(),
                progress,
            )
            .await;
        jobs::finish(&job, r);
    });
    Ok(id)
}

#[derive(Debug, serde::Deserialize)]
pub struct PackRequest {
    pub source: Source,
    pub project_id: String,
    pub version_id: String,
    pub title: String,
}

#[tauri::command]
pub fn install_pack(state: State<'_, AppState>, request: PackRequest) -> CmdResult<u64> {
    let content = state.content();
    let instances = state.instances.clone();
    let cache = state.paths.meta_dir().join("packs");
    let concurrency = lock(&state.settings).concurrent_downloads.clamp(1, 64);
    let http = state.http.clone();
    let (id, progress, job) = state.jobs.start(format!("Installing {}", request.title));
    tauri::async_runtime::spawn(async move {
        let r = async {
            match request.source {
                Source::Modrinth => {
                    content.install_modrinth_pack(&instances, &cache, &request.project_id, &request.version_id, concurrency, progress).await
                }
                Source::CurseForge => {
                    let cf = content.curseforge.as_ref().ok_or_else(|| elysium_core::Error::Invalid("Add a CurseForge API key in Settings".into()))?;
                    let file_id: u64 = request.version_id.parse().map_err(|_| elysium_core::Error::Invalid("bad file id".into()))?;
                    let file = cf.files_by_id(&[file_id]).await?.into_iter().next().ok_or_else(|| elysium_core::Error::Invalid("pack file not found".into()))?;
                    let url = file.download_url.clone().ok_or_else(|| {
                        elysium_core::Error::Invalid(format!(
                            "The author of {} doesn't allow downloads outside CurseForge. Download the pack zip from its page and import it with “Import pack”.",
                            request.title
                        ))
                    })?;
                    let dest = cache.join(&file.file_name);
                    http.download_all(vec![elysium_core::http::DownloadTask::new(url, &dest).sha1(file.sha1().map(str::to_owned))], 1, Verify::Hash, Arc::clone(&progress)).await?;
                    let origin = PackOrigin { source: "curseforge".into(), project_id: Some(request.project_id.clone()), version_id: Some(request.version_id.clone()), name: request.title.clone() };
                    let r = content.import_curseforge_pack(&instances, &dest, Some(origin), concurrency, progress).await;
                    let _ = std::fs::remove_file(&dest);
                    r
                }
            }
        }
        .await;
        jobs::finish(&job, r);
    });
    Ok(id)
}

#[tauri::command]
pub fn import_pack_file(state: State<'_, AppState>, path: PathBuf) -> CmdResult<u64> {
    let content = state.content();
    let instances = state.instances.clone();
    let concurrency = lock(&state.settings).concurrent_downloads.clamp(1, 64);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (id, progress, job) = state.jobs.start(format!("Importing {name}"));
    tauri::async_runtime::spawn(async move {
        let r = if name.to_lowercase().ends_with(".mrpack") {
            content
                .import_mrpack(&instances, &path, None, concurrency, progress)
                .await
        } else {
            content
                .import_curseforge_pack(&instances, &path, None, concurrency, progress)
                .await
        };
        jobs::finish(&job, r);
    });
    Ok(id)
}

#[tauri::command]
pub fn export_pack(state: State<'_, AppState>, id: String, path: PathBuf) -> CmdResult<()> {
    let inst = state.instances.get(&id).map_err(err)?;
    modpack::export_mrpack(&inst, &path).map_err(err)
}

#[tauri::command]
pub fn content_list(state: State<'_, AppState>, id: String) -> CmdResult<Vec<InstalledContent>> {
    content_install::list(&state.instances.get(&id).map_err(err)?).map_err(err)
}

#[tauri::command]
pub fn content_set_enabled(
    state: State<'_, AppState>,
    id: String,
    file: String,
    enabled: bool,
) -> CmdResult<()> {
    content_install::set_enabled(&state.instances.get(&id).map_err(err)?, &file, enabled)
        .map_err(err)
}

#[tauri::command]
pub fn content_remove(state: State<'_, AppState>, id: String, file: String) -> CmdResult<()> {
    content_install::remove(&state.instances.get(&id).map_err(err)?, &file).map_err(err)
}

#[tauri::command]
pub async fn content_updates(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<Vec<content_install::ContentUpdate>> {
    let inst = state.instances.get(&id).map_err(err)?;
    state.content().check_updates(&inst).await.map_err(err)
}

#[tauri::command]
pub fn worlds(state: State<'_, AppState>, id: String) -> CmdResult<Vec<String>> {
    let saves = state
        .instances
        .get(&id)
        .map_err(err)?
        .game_dir()
        .join("saves");
    let mut out: Vec<String> = std::fs::read_dir(&saves)
        .map(|r| {
            r.flatten()
                .filter(|e| e.path().join("level.dat").exists())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    Ok(out)
}

#[derive(Debug, Serialize)]
pub struct ManualCheck {
    pub found: Vec<String>,
    pub downloads_dir: Option<PathBuf>,
}

/// Looks in the Downloads folder for files the player was asked to download by hand.
#[tauri::command]
pub fn manual_check(
    state: State<'_, AppState>,
    id: String,
    items: Vec<ManualDownload>,
) -> CmdResult<ManualCheck> {
    let inst = state.instances.get(&id).map_err(err)?;
    let Some(dir) = dirs::download_dir() else {
        return Ok(ManualCheck {
            found: Vec::new(),
            downloads_dir: None,
        });
    };
    let found = content_install::collect_manual(&inst, &items, &dir).map_err(err)?;
    Ok(ManualCheck {
        found,
        downloads_dir: Some(dir),
    })
}

#[tauri::command]
pub fn open_url(url: String) -> CmdResult<()> {
    let ok = url::Url::parse(&url).is_ok_and(|u| u.scheme() == "https");
    if !ok {
        return Err("Only https links can be opened".into());
    }
    open::that_detached(&url).map_err(|e| format!("Couldn't open the browser: {e}"))
}

#[tauri::command]
pub fn jobs(state: State<'_, AppState>) -> Vec<JobView> {
    state.jobs.views()
}

#[tauri::command]
pub fn job_cancel(state: State<'_, AppState>, id: u64) {
    state.jobs.cancel(id);
}

#[tauri::command]
pub fn job_dismiss(state: State<'_, AppState>, id: u64) {
    state.jobs.dismiss(id);
}

#[tauri::command]
pub fn set_curseforge_key(state: State<'_, AppState>, key: String) -> CmdResult<bool> {
    let key = key.trim();
    if key.is_empty() {
        state.secrets.delete(CURSEFORGE_KEY);
        return Ok(false);
    }
    let persisted = state.secrets.set(CURSEFORGE_KEY, key);
    if !persisted {
        return Err("Saved for this session only: no system credential store is available".into());
    }
    Ok(true)
}

// ---------------------------------------------------------------------------
// Microsoft accounts

#[derive(Debug, Serialize)]
pub struct MsaStart {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
}

#[tauri::command]
pub async fn msa_start(app: AppHandle, state: State<'_, AppState>) -> CmdResult<MsaStart> {
    let auth = state.msa()?;
    let code = auth.start().await.map_err(err)?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut m = lock(&state.msa);
        m.cancel.store(true, Ordering::Relaxed);
        *m = MsaLogin {
            code: Some(code.clone()),
            cancel: Arc::clone(&cancel),
            result: None,
        };
    }
    let reply = MsaStart {
        user_code: code.user_code.clone(),
        verification_uri: code.verification_uri.clone(),
        expires_in: code.expires_in,
    };
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let result = auth.wait(&code, || cancel.load(Ordering::Relaxed)).await;
        let result = match result {
            Ok((account, tokens)) => state.store_microsoft(account, tokens).map(|_| ()),
            Err(elysium_core::Error::Cancelled) => return,
            Err(e) => Err(e.to_string()),
        };
        lock(&state.msa).result = Some(result);
        let _ = app.emit("msa-finished", ());
    });
    Ok(reply)
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MsaStatus {
    Idle,
    Waiting,
    Done { accounts: AccountStore },
    Failed { message: String },
}

#[tauri::command]
pub fn msa_status(state: State<'_, AppState>) -> MsaStatus {
    let mut m = lock(&state.msa);
    match m.result.take() {
        Some(Ok(())) => {
            m.code = None;
            MsaStatus::Done {
                accounts: lock(&state.accounts).clone(),
            }
        }
        Some(Err(message)) => {
            m.code = None;
            MsaStatus::Failed { message }
        }
        None if m.code.is_some() => MsaStatus::Waiting,
        None => MsaStatus::Idle,
    }
}

#[tauri::command]
pub fn msa_cancel(state: State<'_, AppState>) {
    let mut m = lock(&state.msa);
    m.cancel.store(true, Ordering::Relaxed);
    m.code = None;
}

#[tauri::command]
pub async fn upload_skin(
    state: State<'_, AppState>,
    path: PathBuf,
    slim: bool,
) -> CmdResult<AccountStore> {
    let account = lock(&state.accounts)
        .active()
        .cloned()
        .ok_or("No account selected")?;
    if account.kind != AccountKind::Microsoft {
        return Err("Skins need a Microsoft account".into());
    }
    let png = std::fs::read(&path).map_err(|e| format!("Couldn't read {}: {e}", path.display()))?;
    let session = state.session_for(&account).await?;
    let updated = state
        .msa()?
        .upload_skin(&session.access_token, png, slim, account.owns_game)
        .await
        .map_err(err)?;
    let mut store = lock(&state.accounts);
    store
        .accounts
        .iter_mut()
        .filter(|a| a.uuid == updated.uuid)
        .for_each(|a| a.skin = updated.skin.clone());
    store.save(&state.paths).map_err(err)?;
    Ok(store.clone())
}

// ---------------------------------------------------------------------------
// Horizon

#[tauri::command]
pub async fn horizon_status(state: State<'_, AppState>) -> CmdResult<horizon::UpdateStatus> {
    horizon::check(&state.paths, &state.http).await.map_err(err)
}

#[tauri::command]
pub fn horizon_update(state: State<'_, AppState>) -> CmdResult<u64> {
    let (paths, http) = (state.paths.clone(), state.http.clone());
    let (id, progress, job) = state.jobs.start("Updating Horizon");
    tauri::async_runtime::spawn(async move {
        let r = horizon::install_latest(&paths, &http, progress).await;
        jobs::finish(&job, r);
    });
    Ok(id)
}

/// The Minecraft version the installed (or next) Horizon build runs.
#[tauri::command]
pub async fn horizon_minecraft(state: State<'_, AppState>) -> CmdResult<String> {
    match horizon::build_info(&state.paths).await {
        Ok(b) => Ok(b.minecraft),
        Err(_) => Ok("26.3".into()),
    }
}

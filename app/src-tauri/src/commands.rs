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
use elysium_core::{Paths, Progress, horizon, java, official, versions};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::jobs::{self, JobView, Jobs};
use crate::sessions::{self, LogBuffer, LogEntry, Phase, Session, SessionView, Sessions};

const CURSEFORGE_KEY: &str = "curseforge-api-key";

/// CurseForge API key baked in at build time (the release workflow passes the repository
/// secret), like Prism Launcher ships its own. A key the user enters overrides it.
const BUILTIN_CURSEFORGE_KEY: Option<&str> = option_env!("ELYSIUM_CURSEFORGE_KEY");

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
    pub presence: crate::presence::Presence,
}

impl AppState {
    fn curseforge_key(&self) -> String {
        self.secrets
            .get(CURSEFORGE_KEY)
            .filter(|k| !k.trim().is_empty())
            .or_else(|| BUILTIN_CURSEFORGE_KEY.map(str::to_owned))
            .unwrap_or_default()
    }

    fn content(&self) -> ContentService {
        ContentService::new(self.http.clone(), &self.curseforge_key())
    }

    fn msa(&self) -> CmdResult<MicrosoftAuth> {
        MicrosoftAuth::new(self.http.clone(), lock(&self.settings).msa_client_id()).map_err(err)
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
    pub linked_dir: Option<PathBuf>,
    /// The launcher a shared instance is also deleted from.
    pub linked_launcher: Option<String>,
    /// The instance's picture, served to the UI over the asset protocol.
    pub icon: Option<PathBuf>,
    /// Changes whenever the picture does, so the UI can bust its cache.
    pub icon_version: u64,
    /// Friends can join this instance with a share code.
    pub shared: bool,
}

impl From<Instance> for InstanceView {
    fn from(i: Instance) -> Self {
        let icon = i.icon_path();
        let shared = elysium_core::sync::is_shared(&i);
        let linked_launcher = elysium_core::instance::linked_home(&i.config).map(|h| h.launcher);
        let icon_version = icon
            .as_ref()
            .and_then(|p| std::fs::metadata(p).ok()?.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as u64);
        Self {
            icon,
            icon_version,
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
            linked_dir: i.config.linked_dir,
            linked_launcher,
            shared,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Bootstrap {
    pub settings: Settings,
    pub curseforge_configured: bool,
    pub curseforge_builtin: bool,
    pub horizon_supported: bool,
    pub accounts: AccountStore,
    pub instances: Vec<InstanceView>,
    pub data_dir: PathBuf,
    pub version: String,
}

#[tauri::command]
pub fn bootstrap(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Bootstrap> {
    Ok(Bootstrap {
        settings: lock(&state.settings).clone(),
        curseforge_configured: !state.curseforge_key().is_empty(),
        curseforge_builtin: BUILTIN_CURSEFORGE_KEY.is_some_and(|k| !k.is_empty()),
        horizon_supported: horizon::platform_archive().is_some(),
        accounts: lock(&state.accounts).clone(),
        instances: list(&state)?,
        data_dir: state.paths.root().to_owned(),
        version: app.package_info().version.to_string(),
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
    seed_options(&state.paths, &state.instances, &inst.id);
    Ok(inst.into())
}

/// Gives a new instance the global game options (keybinds and settings) once.
fn seed_options(paths: &Paths, instances: &InstanceManager, id: &str) {
    let r = elysium_core::gameopts::load(paths).and_then(|opts| {
        if opts.is_empty() {
            return Ok(());
        }
        elysium_core::gameopts::apply(&instances.get(id)?, &opts)
    });
    if let Err(e) = r {
        tracing::warn!("couldn't apply the global game options to {id}: {e}");
    }
}

/// What the launcher shows, for Discord while no game runs. Nothing when presence is off.
#[tauri::command]
pub fn set_presence(state: State<'_, AppState>, details: Option<String>, line: Option<String>) {
    let on = lock(&state.settings).discord_presence;
    state
        .presence
        .browsing(details.filter(|_| on).map(|d| (d, line)));
}

#[tauri::command]
pub fn game_options(state: State<'_, AppState>) -> CmdResult<elysium_core::gameopts::GameOptions> {
    elysium_core::gameopts::load(&state.paths).map_err(err)
}

#[tauri::command]
pub fn save_game_options(
    state: State<'_, AppState>,
    options: elysium_core::gameopts::GameOptions,
) -> CmdResult<()> {
    elysium_core::gameopts::save(&state.paths, &options).map_err(err)
}

/// An instance's current options, to start the global ones from.
#[tauri::command(async)]
pub fn game_options_from(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<elysium_core::gameopts::GameOptions> {
    elysium_core::gameopts::from_instance(&state.instances.get(&id).map_err(err)?).map_err(err)
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
pub fn set_instance_icon(
    state: State<'_, AppState>,
    id: String,
    path: PathBuf,
) -> CmdResult<InstanceView> {
    let bytes = std::fs::read(&path).map_err(|e| format!("Couldn't read that image: {e}"))?;
    state.instances.set_icon(&id, &bytes).map_err(err)?;
    state
        .instances
        .update(&id, |c| c.icon_url = None)
        .map(Into::into)
        .map_err(err)
}

pub fn backgrounds_dir(paths: &elysium_core::paths::Paths) -> PathBuf {
    paths.root().join("backgrounds")
}

/// Copies a picture into the data folder for the custom theme's background and returns
/// the copy's path. Older copies go, except the one the saved settings still use (the
/// theme editor may be closed without applying).
#[tauri::command(async)]
pub fn import_background(state: State<'_, AppState>, path: PathBuf) -> CmdResult<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|e| ["png", "jpg", "jpeg", "gif", "webp"].contains(&e.as_str()))
        .ok_or("Choose a PNG, JPEG, GIF or WebP picture.")?;
    let bytes = std::fs::read(&path).map_err(|e| format!("Couldn't read that picture: {e}"))?;
    let dir = backgrounds_dir(&state.paths);
    std::fs::create_dir_all(&dir).map_err(err)?;
    let keep = PathBuf::from(&lock(&state.settings).appearance.background_image);
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            if e.path() != keep {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    // A new name each time, so the window never shows a cached older picture.
    let dest = dir.join(format!("background-{stamp}.{ext}"));
    elysium_core::fsutil::write_atomic(&dest, &bytes).map_err(err)?;
    Ok(dest.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn clear_instance_icon(state: State<'_, AppState>, id: String) -> CmdResult<InstanceView> {
    state.instances.clear_icon(&id).map_err(err)?;
    state
        .instances
        .update(&id, |c| c.icon_url = None)
        .map(Into::into)
        .map_err(err)
}

/// Gives a new instance the picture its source had; a missing picture isn't an error.
async fn adopt_icon(http: &Http, instances: &InstanceManager, id: &str, src: &str) {
    let r = async {
        let bytes = elysium_core::import::icon_bytes(http, src).await?;
        instances.set_icon(id, &bytes)?;
        let url = src.starts_with("https://").then(|| src.to_owned());
        instances.update(id, |c| c.icon_url = url)
    }
    .await;
    if let Err(e) = r {
        tracing::warn!("couldn't bring over the picture for {id}: {e}");
    }
}

#[tauri::command(async)]
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
pub async fn instance_overview(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<elysium_core::overview::Overview> {
    let inst = state.instances.get(&id).map_err(err)?;
    // A linked instance's screenshots live in the other launcher's folder; let the window
    // show that one folder, not anything around it.
    let shots = inst.game_dir().join("screenshots");
    if shots.is_dir() {
        app.asset_protocol_scope()
            .allow_directory(&shots, false)
            .map_err(err)?;
    }
    tokio::task::spawn_blocking(move || elysium_core::overview::overview(&inst))
        .await
        .map_err(err)
}

#[tauri::command(async)]
pub fn startup_report(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<Option<elysium_core::startup::StartupReport>> {
    let inst = state.instances.get(&id).map_err(err)?;
    Ok(elysium_core::startup::history(&inst).pop())
}

#[tauri::command(async)]
pub fn snapshot_take(
    state: State<'_, AppState>,
    id: String,
    label: String,
) -> CmdResult<elysium_core::snapshot::Snapshot> {
    let inst = state.instances.get(&id).map_err(err)?;
    elysium_core::snapshot::take(&inst, &label).map_err(err)
}

#[tauri::command(async)]
pub fn snapshot_pending(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<Option<elysium_core::snapshot::Snapshot>> {
    let inst = state.instances.get(&id).map_err(err)?;
    Ok(elysium_core::snapshot::pending(&inst))
}

#[tauri::command(async)]
pub fn snapshot_restore(state: State<'_, AppState>, id: String, snapshot: String) -> CmdResult<()> {
    if lock(&state.sessions)
        .get(&id)
        .is_some_and(|s| s.phase().is_active())
    {
        return Err("Stop the game first".into());
    }
    let inst = state.instances.get(&id).map_err(err)?;
    elysium_core::snapshot::restore(&inst, &snapshot).map_err(err)
}

#[tauri::command]
pub fn open_screenshot(state: State<'_, AppState>, id: String, name: String) -> CmdResult<()> {
    if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
        return Err("Not a screenshot".into());
    }
    let path = state
        .instances
        .get(&id)
        .map_err(err)?
        .game_dir()
        .join("screenshots")
        .join(&name);
    if !path.is_file() {
        return Err("That screenshot is gone".into());
    }
    open::that_detached(&path).map_err(|e| format!("Couldn't open {name}: {e}"))
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
///
/// With no account selected (or when the player prefers it), the game is handed to the
/// official Minecraft Launcher, which signs the player in.
#[tauri::command]
pub async fn launch(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    repair: bool,
) -> CmdResult<()> {
    let instance = state.instances.get(&id).map_err(err)?;
    let settings = lock(&state.settings).clone();
    let account = lock(&state.accounts).active().cloned();
    let horizon_instance = instance.config.loader == ModLoader::Horizon;
    let official = !horizon_instance && (account.is_none() || settings.always_official_launcher);
    if horizon_instance
        && !account
            .as_ref()
            .is_some_and(|a| a.kind == AccountKind::Microsoft && a.owns_game)
    {
        return Err("Horizon needs a Microsoft account that owns Minecraft: Java Edition. Sign in under Accounts.".into());
    }
    let launch_session = match (&account, official) {
        (Some(a), false) => Some(state.session_for(a).await?),
        _ => None,
    };

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

    tauri::async_runtime::spawn(async move {
        let presence = &app.state::<AppState>().presence;
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
            let loader = match instance.config.loader {
                ModLoader::Vanilla => String::new(),
                l => format!(" with {}", l.name()),
            };
            format!("Minecraft {}{loader}", instance.config.game_version)
        };
        push(LogLine::launcher(
            LogLevel::Info,
            format!("Preparing {what}"),
        ));

        // A friend's pack: pick up the owner's changes first. If the owner's PC can't
        // be reached, play the version already here.
        if elysium_core::sync::is_synced(&instance) {
            let concurrency = settings.concurrent_downloads.clamp(1, 64);
            match elysium_core::sync::sync(
                &http,
                &instances,
                &instance.id,
                concurrency,
                Arc::clone(&progress),
            )
            .await
            {
                Ok(r) if r.updated + r.removed > 0 => push(LogLine::launcher(
                    LogLevel::Info,
                    format!(
                        "Synced the pack: {} file(s) updated, {} removed",
                        r.updated, r.removed
                    ),
                )),
                Ok(_) => {}
                Err(elysium_core::Error::Cancelled) => {
                    push(LogLine::launcher(LogLevel::Warn, "Launch cancelled"));
                    set_phase(Phase::Stopped);
                    notify(&app);
                    return;
                }
                Err(e) => push(LogLine::launcher(
                    LogLevel::Warn,
                    format!("Couldn't sync the pack, playing the version you have: {e}"),
                )),
            }
        }
        // The sync may have changed the game or loader version.
        let instance = instances
            .get(&instance.id)
            .unwrap_or_else(|_| instance.clone());

        if settings.auto_backup {
            progress.set_stage("Backing up worlds", 0);
            let inst = instance.clone();
            let keep = settings.backups_kept;
            let p = Arc::clone(&progress);
            match tokio::task::spawn_blocking(move || {
                elysium_core::backup::auto_backup(&inst, keep, &p)
            })
            .await
            {
                Ok(Ok(made)) if !made.is_empty() => push(LogLine::launcher(
                    LogLevel::Info,
                    format!("Backed up {} world(s)", made.len()),
                )),
                Ok(Err(elysium_core::Error::Cancelled)) => {
                    push(LogLine::launcher(LogLevel::Warn, "Launch cancelled"));
                    set_phase(Phase::Stopped);
                    notify(&app);
                    return;
                }
                Ok(Err(e)) => push(LogLine::launcher(
                    LogLevel::Warn,
                    format!("Couldn't back up worlds: {e}"),
                )),
                _ => {}
            }
        }

        if official {
            let result = async {
                let prepared =
                    install::prepare(&paths, &http, &instance, &options, Arc::clone(&progress))
                        .await?;
                let launcher =
                    official::ensure_launcher(&paths, &http, Arc::clone(&progress)).await?;
                official::write_profile(&paths, &instance, &prepared.version.id, &effective)?;
                official::open(&launcher, &paths)
            }
            .await;
            match result {
                Ok(()) => {
                    push(LogLine::launcher(
                        LogLevel::Info,
                        format!(
                            "Opened the Minecraft Launcher. Choose “{}” there and press Play.",
                            instance.config.name
                        ),
                    ));
                    if let Err(e) = instances.record_session(&instance.id, Utc::now(), 0) {
                        tracing::warn!("could not save last played: {e}");
                    }
                    set_phase(Phase::HandedOff);
                }
                Err(elysium_core::Error::Cancelled) => {
                    push(LogLine::launcher(LogLevel::Warn, "Launch cancelled"));
                    set_phase(Phase::Stopped);
                }
                Err(e) => {
                    push(LogLine::launcher(
                        LogLevel::Error,
                        format!("Launch failed: {e}"),
                    ));
                    set_phase(Phase::Failed {
                        message: e.to_string(),
                    });
                }
            }
            notify(&app);
            return;
        }
        let Some(launch_session) = launch_session else {
            return;
        };

        let command = async {
            if horizon_instance {
                let installed = horizon::state(&paths)?.current.is_some();
                if settings.horizon_auto_update || !installed {
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
        if settings.discord_presence {
            let mods = std::fs::read_dir(instance.game_dir().join("mods"))
                .map(|r| {
                    r.flatten()
                        .filter(|e| e.file_name().to_string_lossy().ends_with(".jar"))
                        .count()
                })
                .unwrap_or(0);
            let state_line = match mods {
                0 => what.clone(),
                1 => format!("{what}, 1 mod"),
                n => format!("{what}, {n} mods"),
            };
            presence.playing(
                &instance.config.name,
                &state_line,
                instance.config.icon_url.as_deref(),
                started.timestamp(),
            );
        }
        let window = app.get_webview_window("main");
        if settings.hide_on_launch
            && let Some(w) = &window
        {
            let _ = w.minimize();
        }
        // Time each game line until the main menu, for the startup profiler.
        let marks: Arc<Mutex<Option<Vec<elysium_core::startup::Mark>>>> =
            Arc::new(Mutex::new(Some(Vec::new())));
        let ready = Arc::new(AtomicBool::new(false));
        let on_log: Arc<dyn Fn(LogLine) + Send + Sync> = {
            let (push, marks, ready) = (push.clone(), Arc::clone(&marks), Arc::clone(&ready));
            let t0 = std::time::Instant::now();
            Arc::new(move |line: LogLine| {
                if !ready.load(Ordering::Relaxed)
                    && line.source == elysium_core::logs::LogSource::Game
                    && let Some(list) = lock(&marks).as_mut()
                {
                    if elysium_core::startup::is_ready(&line.text) {
                        ready.store(true, Ordering::Relaxed);
                    }
                    if list.len() < 100_000 {
                        list.push(elysium_core::startup::Mark {
                            ms: t0.elapsed().as_millis() as u64,
                            logger: line.logger.clone(),
                            text: line.text.clone(),
                        });
                    }
                }
                push(line)
            })
        };
        let outcome = launch::run(&command, on_log, kill_rx).await;
        let timed = lock(&marks).take();
        if ready.load(Ordering::Relaxed)
            && let Some(list) = timed
        {
            let inst = instance.clone();
            let _ = tokio::task::spawn_blocking(move || {
                // It reached the menu: whatever changed since the last snapshot works.
                if let Err(e) = elysium_core::snapshot::verify(&inst) {
                    tracing::warn!("couldn't update snapshots: {e}");
                }
                let mods = elysium_core::content::local::scan_mods(&inst);
                if let Some(r) = elysium_core::startup::analyze(&list, &mods)
                    && let Err(e) = elysium_core::startup::record(&inst, r)
                {
                    tracing::warn!("couldn't save startup times: {e}");
                }
            })
            .await;
        }
        if settings.hide_on_launch
            && let Some(w) = &window
        {
            let _ = w.unminimize();
        }
        if settings.discord_presence {
            presence.clear();
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
                    let tail = lock(&logs).tail(4000);
                    let inst = instance.clone();
                    let diagnosis = tokio::task::spawn_blocking(move || {
                        elysium_core::crash::explain(&inst, tail)
                    })
                    .await
                    .ok()
                    .flatten();
                    Phase::Crashed {
                        code: exit.code,
                        diagnosis,
                    }
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
    /// The project's icon, given to the new instance as its picture.
    #[serde(default)]
    pub icon_url: Option<String>,
}

#[tauri::command]
pub fn install_pack(state: State<'_, AppState>, request: PackRequest) -> CmdResult<u64> {
    let content = state.content();
    let instances = state.instances.clone();
    let paths = state.paths.clone();
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
        if let (Ok(pack), Some(url)) = (
            &r,
            request
                .icon_url
                .as_deref()
                .filter(|u| u.starts_with("https://")),
        ) {
            adopt_icon(&http, &instances, &pack.instance_id, url).await;
        }
        if let Ok(pack) = &r {
            seed_options(&paths, &instances, &pack.instance_id);
        }
        jobs::finish(&job, r);
    });
    Ok(id)
}

#[tauri::command]
pub fn import_pack_file(state: State<'_, AppState>, path: PathBuf) -> CmdResult<u64> {
    let content = state.content();
    let instances = state.instances.clone();
    let paths = state.paths.clone();
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
        if let Ok(pack) = &r {
            seed_options(&paths, &instances, &pack.instance_id);
        }
        jobs::finish(&job, r);
    });
    Ok(id)
}

static SERVING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Starts the pack sync server once, the first time anything is shared.
pub async fn ensure_sync_server(instances: InstanceManager) -> CmdResult<()> {
    use std::sync::atomic::Ordering;
    if SERVING.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    elysium_core::sync::serve(instances, elysium_core::sync::PORT)
        .await
        .map(|_| ())
        .map_err(|e| {
            SERVING.store(false, Ordering::SeqCst);
            err(e)
        })
}

#[derive(Debug, Serialize)]
pub struct ShareView {
    pub code: String,
    /// Whether the router opened the port, so friends outside your network can join.
    pub opened: bool,
    pub port: u16,
}

#[tauri::command]
pub async fn share_start(state: State<'_, AppState>, id: String) -> CmdResult<ShareView> {
    let inst = state.instances.get(&id).map_err(err)?;
    let token = elysium_core::sync::start_sharing(&inst).map_err(err)?;
    ensure_sync_server(state.instances.clone()).await?;
    let reach = elysium_core::sync::reach(&state.http).await;
    let code = elysium_core::sync::ShareCode {
        addrs: reach.addrs,
        token,
        name: inst.config.name.clone(),
    };
    Ok(ShareView {
        code: code.encode(),
        opened: reach.opened,
        port: elysium_core::sync::PORT,
    })
}

#[tauri::command]
pub fn share_stop(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    elysium_core::sync::stop_sharing(&state.instances.get(&id).map_err(err)?).map_err(err)
}

/// Joins a friend's shared pack as a new instance.
#[tauri::command]
pub fn sync_join(state: State<'_, AppState>, code: String) -> CmdResult<u64> {
    elysium_core::sync::ShareCode::decode(&code).map_err(err)?;
    let (http, instances, paths) = (
        state.http.clone(),
        state.instances.clone(),
        state.paths.clone(),
    );
    let concurrency = lock(&state.settings).concurrent_downloads.clamp(1, 64);
    let (id, progress, job) = state.jobs.start("Joining a shared pack".to_owned());
    tauri::async_runtime::spawn(async move {
        let r = elysium_core::sync::join(&http, &instances, &code, concurrency, progress)
            .await
            .map(|instance_id| {
                seed_options(&paths, &instances, &instance_id);
                modpack::PackImport {
                    instance_id,
                    manual: Vec::new(),
                }
            });
        jobs::finish(&job, r);
    });
    Ok(id)
}

/// Pulls the owner's latest changes into a synced instance.
#[tauri::command]
pub fn sync_now(state: State<'_, AppState>, id: String) -> CmdResult<u64> {
    let (http, instances) = (state.http.clone(), state.instances.clone());
    let concurrency = lock(&state.settings).concurrent_downloads.clamp(1, 64);
    let (job_id, progress, job) = state.jobs.start("Syncing pack".to_owned());
    tauri::async_runtime::spawn(async move {
        let r = elysium_core::sync::sync(&http, &instances, &id, concurrency, progress).await;
        jobs::finish(&job, r);
    });
    Ok(job_id)
}

#[tauri::command(async)]
pub fn export_pack(state: State<'_, AppState>, id: String, path: PathBuf) -> CmdResult<()> {
    let inst = state.instances.get(&id).map_err(err)?;
    modpack::export_mrpack(&inst, &path).map_err(err)
}

#[tauri::command(async)]
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

#[tauri::command(async)]
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

// ---------------------------------------------------------------------------
// Import, backups, performance mods, system

#[tauri::command(async)]
pub fn import_scan() -> Vec<elysium_core::import::Foreign> {
    elysium_core::import::scan()
}

#[tauri::command(async)]
pub fn import_scan_folder(path: PathBuf) -> Vec<elysium_core::import::Foreign> {
    // A picked folder may be any launcher's; try each format.
    use elysium_core::import::{ForeignSource, scan_root};
    for source in [
        ForeignSource::Prism,
        ForeignSource::CurseForge,
        ForeignSource::Official,
    ] {
        let found = scan_root(source, &path);
        if !found.is_empty() {
            return found;
        }
    }
    Vec::new()
}

#[tauri::command]
pub fn import_run(
    state: State<'_, AppState>,
    items: Vec<elysium_core::import::Foreign>,
    linked: bool,
) -> CmdResult<u64> {
    let instances = state.instances.clone();
    let paths = state.paths.clone();
    let http = state.http.clone();
    let (id, progress, job) = state.jobs.start(format!(
        "Importing {} instance{}",
        items.len(),
        if items.len() == 1 { "" } else { "s" }
    ));
    tauri::async_runtime::spawn(async move {
        let mut ok = Vec::new();
        let mut failed = Vec::new();
        let total = items.len();
        for (n, f) in items.into_iter().enumerate() {
            if progress.is_cancelled() {
                break;
            }
            progress.set_stage(
                format!(
                    "{} {} ({} of {total})",
                    if linked { "Linking" } else { "Copying" },
                    f.name,
                    n + 1,
                ),
                0,
            );
            let (mgr, foreign) = (instances.clone(), f.clone());
            let r = tokio::task::spawn_blocking(move || {
                elysium_core::import::import(&mgr, &foreign, linked)
            })
            .await
            .map_err(|e| elysium_core::Error::Invalid(e.to_string()))
            .and_then(|r| r);
            match r {
                Ok(inst) => {
                    if let Some(src) = &f.icon {
                        adopt_icon(&http, &instances, &inst.id, src).await;
                    }
                    // A shared instance's options belong to the other launcher too.
                    if !linked {
                        seed_options(&paths, &instances, &inst.id);
                    }
                    ok.push(inst.id);
                }
                Err(e) => failed.push(format!("{}: {e}", f.name)),
            }
        }
        jobs::finish(
            &job,
            Ok::<_, elysium_core::Error>(serde_json::json!({ "imported": ok, "failed": failed })),
        );
    });
    Ok(id)
}

/// Shared storage: links identical mods and packs across instances that aren't running.
#[tauri::command]
pub async fn share_files(
    state: State<'_, AppState>,
) -> CmdResult<elysium_core::share::ShareReport> {
    let dirs: Vec<PathBuf> = state
        .instances
        .list()
        .map_err(err)?
        .into_iter()
        .filter(|i| !state_is_running(&state, &i.id))
        .map(|i| i.game_dir())
        .collect();
    tokio::task::spawn_blocking(move || {
        elysium_core::share::share(&dirs, &elysium_core::Progress::new())
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command(async)]
pub fn backups(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<Vec<elysium_core::backup::Backup>> {
    elysium_core::backup::list(&state.instances.get(&id).map_err(err)?).map_err(err)
}

#[tauri::command]
pub async fn backup_now(state: State<'_, AppState>, id: String) -> CmdResult<usize> {
    let inst = state.instances.get(&id).map_err(err)?;
    if state_is_running(&state, &id) {
        return Err("Close the game before backing up, so the worlds are saved".into());
    }
    let keep = lock(&state.settings).backups_kept;
    tokio::task::spawn_blocking(move || {
        let worlds = elysium_core::backup::worlds(&inst);
        for w in &worlds {
            elysium_core::backup::backup_world(&inst, w, keep, &elysium_core::Progress::new())?;
        }
        Ok::<_, elysium_core::Error>(worlds.len())
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
pub async fn backup_restore(
    state: State<'_, AppState>,
    id: String,
    world: String,
    file: String,
) -> CmdResult<()> {
    let inst = state.instances.get(&id).map_err(err)?;
    if state_is_running(&state, &id) {
        return Err("Close the game before restoring a world".into());
    }
    let keep = lock(&state.settings).backups_kept;
    tokio::task::spawn_blocking(move || elysium_core::backup::restore(&inst, &world, &file, keep))
        .await
        .map_err(err)?
        .map_err(err)
}

fn state_is_running(state: &AppState, id: &str) -> bool {
    lock(&state.sessions)
        .get(id)
        .is_some_and(|s| s.phase().is_active())
}

/// Well-known performance mods for Fabric and Quilt, by Modrinth slug.
const PERFORMANCE_MODS: &[&str] = &[
    "sodium",
    "lithium",
    "ferrite-core",
    "entityculling",
    "immediatelyfast",
];

#[tauri::command]
pub fn install_performance_mods(state: State<'_, AppState>, id: String) -> CmdResult<u64> {
    let inst = state.instances.get(&id).map_err(err)?;
    if !matches!(inst.config.loader, ModLoader::Fabric | ModLoader::Quilt) {
        return Err("Performance mods are available for Fabric and Quilt instances".into());
    }
    let content = state.content();
    let (job_id, progress, job) = state.jobs.start("Adding performance mods");
    tauri::async_runtime::spawn(async move {
        let mut installed = Vec::new();
        for slug in PERFORMANCE_MODS {
            if progress.is_cancelled() {
                break;
            }
            // Mods without a build for this version are skipped, not fatal.
            if let Ok(r) = content
                .install(
                    &inst,
                    Source::Modrinth,
                    slug,
                    None,
                    ContentKind::Mod,
                    None,
                    Arc::clone(&progress),
                )
                .await
            {
                installed.extend(r.installed.into_iter().map(|e| e.title));
            }
        }
        jobs::finish(&job, Ok::<_, elysium_core::Error>(installed));
    });
    Ok(job_id)
}

#[derive(Debug, Serialize)]
pub struct SystemInfo {
    pub total_memory_mb: u64,
    pub recommended_max_mb: u32,
    pub discord_available: bool,
}

#[tauri::command]
pub fn system_info() -> SystemInfo {
    let total = elysium_core::system::total_memory_mb();
    SystemInfo {
        total_memory_mb: total,
        recommended_max_mb: elysium_core::settings::recommended_memory(total).1,
        discord_available: crate::presence::Presence::available(),
    }
}

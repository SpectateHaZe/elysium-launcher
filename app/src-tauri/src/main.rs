//! Elysium Launcher desktop app (Tauri shell around `elysium-core`).

// Don't open a console window next to the app on Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod commands;
mod jobs;
mod presence;
mod sessions;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use elysium_core::Paths;
use elysium_core::account::AccountStore;
use elysium_core::http::Http;
use elysium_core::instance::InstanceManager;
use elysium_core::settings::Settings;
use tauri::{Emitter, Manager};

use crate::commands::AppState;

/// Set once the user confirmed quitting while a game is running.
static QUIT_CONFIRMED: AtomicBool = AtomicBool::new(false);

#[tauri::command]
fn quit(app: tauri::AppHandle) {
    QUIT_CONFIRMED.store(true, Ordering::SeqCst);
    app.exit(0);
}

fn init_state() -> Result<AppState, String> {
    let paths = Paths::from_env().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(paths.root())
        .map_err(|e| format!("Couldn't create {}: {e}", paths.root().display()))?;
    let settings = Settings::load(&paths).unwrap_or_else(|e| {
        tracing::warn!("settings unreadable, using defaults: {e}");
        Settings::default()
    });
    let accounts = AccountStore::load(&paths).unwrap_or_else(|e| {
        tracing::warn!("accounts unreadable: {e}");
        AccountStore::default()
    });
    Ok(AppState {
        http: Http::new().map_err(|e| e.to_string())?,
        instances: InstanceManager::new(paths.instances_dir()),
        settings: Mutex::new(settings),
        accounts: Mutex::new(accounts),
        sessions: Mutex::default(),
        secrets: elysium_core::secrets::SecretStore::new(),
        tokens: Mutex::default(),
        jobs: crate::jobs::Jobs::default(),
        msa: Mutex::default(),
        presence: crate::presence::Presence::default(),
        paths,
    })
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,elysium_core=info,elysium_launcher=info".into()),
        )
        .init();

    let state = match init_state() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Elysium Launcher couldn't start: {e}");
            std::process::exit(1);
        }
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // Instance pictures are the only local files the window may load.
            let dir = app.state::<AppState>().paths.instances_dir();
            app.asset_protocol_scope().allow_directory(&dir, true)?;
            Ok(())
        })
        .manage(state)
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                if !QUIT_CONFIRMED.load(Ordering::SeqCst) && commands::any_active(&state) {
                    api.prevent_close();
                    let _ = window.emit("confirm-quit", ());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            quit,
            commands::bootstrap,
            commands::instances,
            commands::versions,
            commands::create_instance,
            commands::update_instance,
            commands::set_instance_icon,
            commands::instance_overview,
            commands::open_screenshot,
            commands::startup_report,
            commands::snapshot_take,
            commands::snapshot_pending,
            commands::snapshot_restore,
            commands::clear_instance_icon,
            commands::delete_instance,
            commands::save_settings,
            commands::add_offline_account,
            commands::remove_account,
            commands::set_active_account,
            commands::probe_java,
            commands::open_folder,
            commands::sessions,
            commands::logs,
            commands::clear_logs,
            commands::stop,
            commands::launch,
            commands::loader_versions,
            commands::search,
            commands::project,
            commands::project_versions,
            commands::install_content,
            commands::install_pack,
            commands::import_pack_file,
            commands::export_pack,
            commands::content_list,
            commands::content_set_enabled,
            commands::content_remove,
            commands::content_updates,
            commands::worlds,
            commands::manual_check,
            commands::open_url,
            commands::jobs,
            commands::job_cancel,
            commands::job_dismiss,
            commands::set_curseforge_key,
            commands::msa_start,
            commands::msa_status,
            commands::msa_cancel,
            commands::upload_skin,
            commands::horizon_status,
            commands::horizon_update,
            commands::horizon_minecraft,
            commands::import_scan,
            commands::import_scan_folder,
            commands::import_run,
            commands::share_files,
            commands::set_presence,
            commands::game_options,
            commands::save_game_options,
            commands::game_options_from,
            commands::backups,
            commands::backup_now,
            commands::backup_restore,
            commands::install_performance_mods,
            commands::system_info,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            eprintln!("Elysium Launcher stopped unexpectedly: {e}");
            std::process::exit(1);
        });
}

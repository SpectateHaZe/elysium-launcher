//! Elysium Launcher desktop app.

// Don't open a console window next to the app on Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod backend;
mod icons;
mod theme;
mod views;
mod widgets;

use eframe::egui;

fn main() -> eframe::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,elysium_core=info,elysium_launcher=info".into()),
        )
        .init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Elysium Launcher")
            .with_app_id("elysium-launcher")
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([920.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Elysium Launcher",
        options,
        Box::new(|cc| Ok(Box::new(app::ElysiumApp::new(cc)?))),
    )
}

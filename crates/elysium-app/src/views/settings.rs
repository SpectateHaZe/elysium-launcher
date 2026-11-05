//! Global settings, grouped like a system settings app.

use std::path::PathBuf;

use eframe::egui::{self, RichText};
use elysium_core::settings::{Resolution, Theme};

use crate::app::{ElysiumApp, apply_theme};
use crate::icons;
use crate::theme::{self, Palette};
use crate::views::page_header;
use crate::widgets::{self, ButtonKind};

pub fn show(app: &mut ElysiumApp, ui: &mut egui::Ui) {
    let p = Palette::of(ui.ctx());
    page_header(
        ui,
        "Settings",
        "Defaults for every instance. Instances can override them.",
    );
    ui.add_space(20.0);
    let before = app.settings.clone();

    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.set_max_width(820.0);

        widgets::section_label(ui, "Appearance");
        widgets::card_frame(&p).show(ui, |ui| {
            ui.set_width(ui.available_width());
            widgets::setting_row(ui, "Theme", "Follow your system, or pick light or dark", |ui| {
                if widgets::segmented(ui, "theme", &mut app.settings.theme, &[(Theme::System, "System"), (Theme::Light, "Light"), (Theme::Dark, "Dark")]) {
                    apply_theme(ui.ctx(), app.settings.theme);
                }
            });
        });
        ui.add_space(14.0);

        widgets::section_label(ui, "Java");
        widgets::card_frame(&p).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut auto = app.settings.java_path.is_none();
            widgets::setting_row(ui, "Automatic Java", "Download the exact Java version each Minecraft version needs", |ui| {
                if widgets::toggle(ui, &mut auto).changed() {
                    app.settings.java_path = if auto { None } else { Some(PathBuf::new()) };
                }
            });
            if let Some(mut path) = app.settings.java_path.clone() {
                java_picker(app, ui, &mut path);
                app.settings.java_path = Some(path);
            }
        });
        ui.add_space(14.0);

        widgets::section_label(ui, "Memory");
        widgets::card_frame(&p).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("How much RAM Minecraft may use. 4–8 GB suits most modpacks; vanilla needs less.").font(theme::regular(12.5)).color(p.text_secondary));
            ui.add_space(4.0);
            let (mut min, mut max) = (app.settings.min_memory_mb, app.settings.max_memory_mb);
            memory_sliders(ui, &mut min, &mut max);
            (app.settings.min_memory_mb, app.settings.max_memory_mb) = (min, max);
        });
        ui.add_space(14.0);

        widgets::section_label(ui, "Game");
        widgets::card_frame(&p).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut custom = app.settings.resolution.is_some();
            widgets::setting_row(ui, "Custom window size", "Start the game window at a fixed size", |ui| {
                if widgets::toggle(ui, &mut custom).changed() {
                    app.settings.resolution = custom.then_some(Resolution { width: 1280, height: 720 });
                }
            });
            if let Some(r) = &mut app.settings.resolution {
                resolution_editor(ui, r);
            }
            widgets::divider(ui);
            ui.label(RichText::new("JVM arguments").font(theme::semibold(14.0)).color(p.text));
            ui.label(RichText::new("Extra arguments passed to Java for every instance. Use quotes for values with spaces.").font(theme::regular(12.5)).color(p.text_secondary));
            widgets::text_field(ui, &mut app.settings.jvm_args, "-XX:+UseG1GC -XX:MaxGCPauseMillis=50", ui.available_width());
            widgets::divider(ui);
            widgets::setting_row(ui, "Minimize while playing", "Get the launcher out of the way once the game starts", |ui| {
                widgets::toggle(ui, &mut app.settings.hide_on_launch);
            });
        });
        ui.add_space(14.0);

        widgets::section_label(ui, "Downloads & Storage");
        widgets::card_frame(&p).show(ui, |ui| {
            ui.set_width(ui.available_width());
            widgets::setting_row(ui, "Parallel downloads", "Lower this on slow or unstable connections", |ui| {
                ui.add(egui::Slider::new(&mut app.settings.concurrent_downloads, 1..=64).integer());
            });
            widgets::divider(ui);
            let root = app.backend.paths.root().to_owned();
            widgets::setting_row(ui, "Data folder", &root.display().to_string(), |ui| {
                if widgets::pill_button(ui, ButtonKind::Secondary, Some(icons::FOLDER_OPEN), "Open").clicked() {
                    app.open_path(&root);
                }
            });
        });
        ui.add_space(20.0);
        ui.label(RichText::new(format!("Elysium Launcher {}", env!("CARGO_PKG_VERSION"))).font(theme::regular(12.0)).color(p.text_tertiary));
    });

    if app.settings != before {
        app.save_settings();
    }
}

/// Min/max memory sliders (values in MB; max is kept ≥ min).
pub fn memory_sliders(ui: &mut egui::Ui, min: &mut u32, max: &mut u32) {
    let fmt = |v: f64, _: std::ops::RangeInclusive<usize>| {
        if v >= 1024.0 {
            format!("{:.1} GB", v / 1024.0)
        } else {
            format!("{v:.0} MB")
        }
    };
    widgets::setting_row(ui, "Maximum", "", |ui| {
        ui.add(
            egui::Slider::new(max, 512..=32768)
                .step_by(256.0)
                .custom_formatter(fmt),
        );
    });
    widgets::setting_row(ui, "Minimum", "", |ui| {
        ui.add(
            egui::Slider::new(min, 256..=16384)
                .step_by(256.0)
                .custom_formatter(fmt),
        );
    });
    if *min > *max {
        *min = *max;
    }
}

/// Path field + Browse + Test for a Java executable.
pub fn java_picker(app: &mut ElysiumApp, ui: &mut egui::Ui, path: &mut PathBuf) {
    let p = Palette::of(ui.ctx());
    ui.horizontal(|ui| {
        let mut text = path.to_string_lossy().into_owned();
        let width = (ui.available_width() - 210.0).max(160.0);
        if widgets::text_field(ui, &mut text, "Path to java or javaw.exe", width).changed() {
            *path = PathBuf::from(text);
        }
        if widgets::pill_button(ui, ButtonKind::Secondary, None, "Browse").clicked()
            && let Some(picked) = rfd::FileDialog::new()
                .set_title("Select a Java executable")
                .pick_file()
        {
            *path = picked;
        }
        let can_test = !path.as_os_str().is_empty();
        if ui
            .add_enabled_ui(can_test, |ui| {
                widgets::pill_button(ui, ButtonKind::Ghost, None, "Test")
            })
            .inner
            .clicked()
        {
            app.java_probe = None;
            app.backend.probe_java(path.clone());
        }
    });
    if let Some((probed, result)) = &app.java_probe
        && probed == path
    {
        let (icon, color, text) = match result {
            Ok(info) => (
                icons::CHECK_CIRCLE,
                p.success,
                format!("Java {} ({})", info.version, info.arch),
            ),
            Err(e) => (icons::X_CIRCLE, p.danger, e.clone()),
        };
        ui.label(
            RichText::new(format!("{icon}  {text}"))
                .font(theme::regular(12.5))
                .color(color),
        );
    }
}

pub fn resolution_editor(ui: &mut egui::Ui, r: &mut Resolution) {
    ui.horizontal(|ui| {
        ui.add(
            egui::DragValue::new(&mut r.width)
                .range(320..=7680)
                .suffix(" px"),
        );
        ui.label("×");
        ui.add(
            egui::DragValue::new(&mut r.height)
                .range(240..=4320)
                .suffix(" px"),
        );
    });
}

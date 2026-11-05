//! Instance detail page: hero, overview, per-instance settings and the live game log.

use eframe::egui::{self, Align2, Color32, CornerRadius, Margin, Modal, RichText, Sense};
use elysium_core::instance::{Instance, InstanceOverrides};
use elysium_core::logs::{LogLevel, LogSource};
use elysium_core::settings::Resolution;

use crate::app::{ElysiumApp, InstanceTab, Page, ToastKind};
use crate::backend::Phase;
use crate::icons;
use crate::theme::{self, Palette};
use crate::widgets::{self, ButtonKind};

pub fn show(app: &mut ElysiumApp, ui: &mut egui::Ui, id: &str) {
    let Some(inst) = app.instance(id).cloned() else {
        app.page = Page::Library;
        return;
    };
    let p = Palette::of(ui.ctx());

    if ui
        .add(
            egui::Button::new(
                RichText::new(format!("{}  Library", icons::CARET_LEFT))
                    .font(theme::semibold(14.0))
                    .color(p.accent),
            )
            .frame(false),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        app.page = Page::Library;
    }
    ui.add_space(8.0);
    hero(app, ui, &inst, &p);
    ui.add_space(20.0);

    widgets::segmented(
        ui,
        "instance_tab",
        &mut app.instance_tab,
        &[
            (InstanceTab::Overview, "Overview"),
            (InstanceTab::Settings, "Settings"),
            (InstanceTab::Logs, "Logs"),
        ],
    );
    ui.add_space(16.0);

    match app.instance_tab {
        InstanceTab::Overview => {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| overview(app, ui, &inst, &p));
        }
        InstanceTab::Settings => {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| settings(app, ui, &inst, &p));
        }
        InstanceTab::Logs => logs(app, ui, &inst, &p),
    }
}

fn hero(app: &mut ElysiumApp, ui: &mut egui::Ui, inst: &Instance, p: &Palette) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 184.0), Sense::hover());
    let (a, b) = widgets::art_colors(&inst.config.name);
    widgets::paint_gradient(ui.painter(), rect, 22.0, a, b);
    // Scrim so white text stays legible on light gradients.
    widgets::paint_gradient(
        ui.painter(),
        rect,
        22.0,
        Color32::from_black_alpha(70),
        Color32::TRANSPARENT,
    );
    // Decorative large glyph.
    ui.painter().text(
        rect.right_center() - egui::vec2(180.0, -10.0),
        Align2::CENTER_CENTER,
        icons::CUBE,
        theme::regular(150.0),
        Color32::from_white_alpha(38),
    );

    let inner = rect.shrink2(egui::vec2(32.0, 28.0));
    let mut ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Max)),
    );
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 10.0;
        ui.add_space(inner.height() - 92.0);
        ui.label(
            RichText::new(&inst.config.name)
                .font(theme::bold(34.0))
                .color(Color32::WHITE),
        );
        ui.horizontal(|ui| {
            let chip_bg = Color32::from_white_alpha(48);
            widgets::chip(
                ui,
                &format!("Minecraft {}", inst.config.game_version),
                Color32::WHITE,
                chip_bg,
            );
            widgets::chip(ui, "Vanilla", Color32::WHITE, chip_bg);
        });
    });

    let session = app.backend.sessions.get(&inst.id);
    let phase = session.map(|s| s.phase());
    let progress = session.map(|s| (s.progress.stage(), s.progress.fraction()));
    ui.with_layout(
        egui::Layout::right_to_left(egui::Align::Max),
        |ui| match phase {
            Some(Phase::Preparing) => {
                if hero_button(ui, icons::X, "Cancel", p.danger).clicked() {
                    app.backend.stop(&inst.id);
                }
                if let Some((stage, frac)) = progress {
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.add_space(4.0);
                        let label = match frac {
                            Some(f) => format!("{stage}  {:.0}%", f * 100.0),
                            None => stage,
                        };
                        ui.label(
                            RichText::new(label)
                                .font(theme::semibold(13.0))
                                .color(Color32::WHITE),
                        );
                        widgets::progress_bar(ui, frac, 200.0);
                    });
                }
            }
            Some(Phase::Running { .. }) => {
                if hero_button(ui, icons::STOP, "Stop", p.danger).clicked() {
                    app.backend.stop(&inst.id);
                }
            }
            _ => {
                if hero_button(ui, icons::PLAY, "Play", Color32::from_rgb(0x1D, 0x1D, 0x1F))
                    .clicked()
                {
                    app.play(&inst.id, false);
                }
            }
        },
    );
}

/// White pill button sitting on the coloured hero.
fn hero_button(ui: &mut egui::Ui, icon: &str, text: &str, color: Color32) -> egui::Response {
    let icon_g =
        ui.painter()
            .layout_no_wrap(icon.to_owned(), widgets::icon_font(icon, 19.0), color);
    let text_g = ui
        .painter()
        .layout_no_wrap(text.to_owned(), theme::bold(17.0), color);
    let gap = 8.0;
    let content_w = icon_g.size().x + gap + text_g.size().x;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(content_w + 60.0, 52.0), Sense::click());
    let hover = ui.ctx().animate_bool_responsive(resp.id, resp.hovered());
    let rect = rect.expand(hover * 2.0);
    let painter = ui.painter();
    painter.add(
        egui::Shadow {
            offset: [0, 6],
            blur: 18,
            spread: 0,
            color: Color32::from_black_alpha(50),
        }
        .as_shape(rect, CornerRadius::same(255)),
    );
    painter.rect_filled(rect, CornerRadius::same(255), Color32::WHITE);
    let x = rect.center().x - content_w / 2.0;
    let icon_w = icon_g.size().x;
    painter.galley(
        egui::pos2(x, rect.center().y - icon_g.size().y / 2.0),
        icon_g,
        color,
    );
    painter.galley(
        egui::pos2(x + icon_w + gap, rect.center().y - text_g.size().y / 2.0),
        text_g,
        color,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn stat_card(ui: &mut egui::Ui, p: &Palette, width: f32, icon: &str, label: &str, value: &str) {
    widgets::card_frame(p).show(ui, |ui| {
        ui.set_width(width - 40.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.label(
                RichText::new(icon)
                    .font(theme::regular(20.0))
                    .color(p.accent),
            );
            ui.label(
                RichText::new(label)
                    .font(theme::regular(12.5))
                    .color(p.text_secondary),
            );
            ui.add(
                egui::Label::new(RichText::new(value).font(theme::bold(19.0)).color(p.text))
                    .truncate(),
            );
        });
    });
}

fn overview(app: &mut ElysiumApp, ui: &mut egui::Ui, inst: &Instance, p: &Palette) {
    let gap = 16.0;
    let cols = if ui.available_width() > 760.0 { 4 } else { 2 };
    let w = (ui.available_width() - 8.0 - gap * (cols - 1) as f32) / cols as f32;
    let stats = [
        (
            icons::CLOCK,
            "Time played",
            widgets::format_duration(inst.config.playtime_secs),
        ),
        (
            icons::CALENDAR_BLANK,
            "Last played",
            inst.config
                .last_played
                .map_or("Never".to_owned(), widgets::format_relative),
        ),
        (icons::CUBE, "Version", inst.config.game_version.clone()),
        (
            icons::SPARKLE,
            "Created",
            inst.config
                .created
                .with_timezone(&chrono::Local)
                .format("%b %-d, %Y")
                .to_string(),
        ),
    ];
    for row in stats.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.add_space(2.0);
            for (icon, label, value) in row {
                stat_card(ui, p, w, icon, label, value);
            }
        });
        ui.add_space(gap - 10.0);
    }
    ui.add_space(8.0);

    widgets::section_label(ui, "Manage");
    widgets::card_frame(p).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let running = app.backend.is_running(&inst.id);
        widgets::setting_row(
            ui,
            "Game folder",
            "Worlds, screenshots, resource packs and options",
            |ui| {
                if widgets::pill_button(ui, ButtonKind::Secondary, Some(icons::FOLDER_OPEN), "Open")
                    .clicked()
                {
                    app.open_path(&inst.game_dir());
                }
            },
        );
        widgets::divider(ui);
        widgets::setting_row(
            ui,
            "Repair",
            "Re-check every game file and re-download anything damaged",
            |ui| {
                ui.add_enabled_ui(!running, |ui| {
                    if widgets::pill_button(
                        ui,
                        ButtonKind::Secondary,
                        Some(icons::WRENCH),
                        "Repair & Play",
                    )
                    .clicked()
                    {
                        app.play(&inst.id, true);
                    }
                });
            },
        );
        widgets::divider(ui);
        widgets::setting_row(
            ui,
            "Delete instance",
            "Permanently removes this instance and its worlds",
            |ui| {
                ui.add_enabled_ui(!running, |ui| {
                    if widgets::pill_button(ui, ButtonKind::Danger, Some(icons::TRASH), "Delete")
                        .clicked()
                    {
                        app.confirm_delete = Some(inst.id.clone());
                    }
                });
            },
        );
    });
}

fn settings(app: &mut ElysiumApp, ui: &mut egui::Ui, inst: &Instance, p: &Palette) {
    let mut name = inst.config.name.clone();
    let mut o: InstanceOverrides = inst.config.overrides.clone();
    let global = app.settings.clone();

    widgets::section_label(ui, "General");
    widgets::card_frame(p).show(ui, |ui| {
        ui.set_width(ui.available_width());
        widgets::setting_row(ui, "Name", "", |ui| {
            let id = ui.scope_id().with(("rename", &inst.id));
            let mut draft = ui
                .data_mut(|d| d.get_temp::<String>(id))
                .unwrap_or_else(|| name.clone());
            let resp = widgets::text_field(ui, &mut draft, "Instance name", 260.0);
            if resp.lost_focus() && !draft.trim().is_empty() {
                name = draft.trim().to_owned();
                ui.data_mut(|d| d.remove::<String>(id));
            } else if resp.has_focus() || resp.changed() {
                ui.data_mut(|d| d.insert_temp(id, draft));
            }
        });
    });
    ui.add_space(12.0);

    widgets::section_label(ui, "Overrides");
    ui.label(
        RichText::new("Turn on an option to override the global setting for this instance only.")
            .font(theme::regular(12.5))
            .color(p.text_secondary),
    );
    ui.add_space(4.0);
    widgets::card_frame(p).show(ui, |ui| {
        ui.set_width(ui.available_width());

        let mut memory = o.max_memory_mb.is_some() || o.min_memory_mb.is_some();
        widgets::setting_row(
            ui,
            "Memory",
            "Maximum and minimum RAM given to the game",
            |ui| {
                if widgets::toggle(ui, &mut memory).changed() {
                    if memory {
                        o.max_memory_mb = Some(global.max_memory_mb);
                        o.min_memory_mb = Some(global.min_memory_mb);
                    } else {
                        o.max_memory_mb = None;
                        o.min_memory_mb = None;
                    }
                }
            },
        );
        if memory {
            let mut max = o.max_memory_mb.unwrap_or(global.max_memory_mb);
            let mut min = o.min_memory_mb.unwrap_or(global.min_memory_mb);
            crate::views::settings::memory_sliders(ui, &mut min, &mut max);
            o.max_memory_mb = Some(max);
            o.min_memory_mb = Some(min);
        }
        widgets::divider(ui);

        let mut custom_java = o.java_path.is_some();
        widgets::setting_row(
            ui,
            "Java",
            "Use a specific Java installation for this instance",
            |ui| {
                if widgets::toggle(ui, &mut custom_java).changed() {
                    o.java_path = custom_java.then(|| global.java_path.clone().unwrap_or_default());
                }
            },
        );
        if let Some(path) = &mut o.java_path {
            crate::views::settings::java_picker(app, ui, path);
        }
        widgets::divider(ui);

        let mut custom_args = o.jvm_args.is_some();
        widgets::setting_row(
            ui,
            "JVM arguments",
            "Extra arguments passed to Java",
            |ui| {
                if widgets::toggle(ui, &mut custom_args).changed() {
                    o.jvm_args = custom_args.then(|| global.jvm_args.clone());
                }
            },
        );
        if let Some(args) = &mut o.jvm_args {
            widgets::text_field(ui, args, "-XX:+UseG1GC", ui.available_width());
        }
        widgets::divider(ui);

        let mut custom_res = o.resolution.is_some();
        widgets::setting_row(ui, "Window size", "Initial size of the game window", |ui| {
            if widgets::toggle(ui, &mut custom_res).changed() {
                o.resolution = custom_res.then(|| {
                    global.resolution.unwrap_or(Resolution {
                        width: 1280,
                        height: 720,
                    })
                });
            }
        });
        if let Some(r) = &mut o.resolution {
            crate::views::settings::resolution_editor(ui, r);
        }
    });

    if name != inst.config.name || o != inst.config.overrides {
        match app.backend.instances.update(&inst.id, |c| {
            c.name = name.clone();
            c.overrides = o.clone();
        }) {
            Ok(updated) => {
                if let Some(slot) = app.instances.iter_mut().find(|i| i.id == inst.id) {
                    *slot = updated;
                }
            }
            Err(e) => app.toast(ToastKind::Error, format!("Could not save instance: {e}")),
        }
    }
}

fn logs(app: &mut ElysiumApp, ui: &mut egui::Ui, inst: &Instance, p: &Palette) {
    let session = app.backend.sessions.get(&inst.id);
    let lines: Vec<(LogLevel, LogSource, String)> = session
        .map(|s| {
            s.logs
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .map(|l| (l.level, l.source, l.text.clone()))
                .collect()
        })
        .unwrap_or_default();

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{} lines", lines.len()))
                .font(theme::regular(13.0))
                .color(p.text_secondary),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, icons::BROOM, "Clear").clicked() {
                app.backend.clear_logs(&inst.id);
            }
            if widgets::icon_button(ui, icons::COPY, "Copy all").clicked() {
                let text = lines
                    .iter()
                    .map(|(_, _, t)| t.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                ui.ctx().copy_text(text);
                app.toast(ToastKind::Success, "Log copied");
            }
            if widgets::icon_button(ui, icons::FOLDER_OPEN, "Open logs folder").clicked() {
                app.open_path(&inst.game_dir().join("logs"));
            }
            ui.add_space(8.0);
            widgets::toggle(ui, &mut app.log_autoscroll);
            ui.label(
                RichText::new("Follow")
                    .font(theme::regular(13.0))
                    .color(p.text_secondary),
            );
        });
    });
    ui.add_space(6.0);

    egui::Frame::new()
        .fill(p.console_bg)
        .corner_radius(16)
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_height(ui.available_height());
            if lines.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{}  Press Play to see the game's output here",
                            icons::TERMINAL_WINDOW
                        ))
                        .color(Color32::from_gray(140)),
                    );
                });
                return;
            }
            let row_height = ui.text_style_height(&egui::TextStyle::Monospace) + 2.0;
            egui::ScrollArea::both()
                .auto_shrink(false)
                .stick_to_bottom(app.log_autoscroll)
                .show_rows(ui, row_height, lines.len(), |ui, range| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for (level, source, text) in &lines[range] {
                        let color = match (level, source) {
                            (LogLevel::Error, _) => Color32::from_rgb(0xFF, 0x6B, 0x6B),
                            (LogLevel::Warn, _) => Color32::from_rgb(0xFF, 0xC1, 0x4D),
                            (_, LogSource::Launcher) => Color32::from_rgb(0x7F, 0xB2, 0xFF),
                            (LogLevel::Debug, _) => Color32::from_gray(120),
                            (LogLevel::Info, _) => Color32::from_gray(215),
                        };
                        ui.add(
                            egui::Label::new(RichText::new(text).monospace().color(color)).extend(),
                        );
                    }
                });
        });
}

pub fn delete_dialog(app: &mut ElysiumApp, ctx: &egui::Context) {
    let Some(id) = app.confirm_delete.clone() else {
        return;
    };
    let Some(inst) = app.instance(&id).cloned() else {
        app.confirm_delete = None;
        return;
    };
    let p = Palette::of(ctx);
    let mut close = false;
    let resp = Modal::new(egui::Id::unique("confirm_delete")).frame(widgets::card_frame(&p).inner_margin(Margin::same(28))).show(ctx, |ui| {
        ui.set_width(400.0);
        ui.label(RichText::new(format!("Delete “{}”?", inst.config.name)).font(theme::bold(20.0)).color(p.text));
        ui.add_space(4.0);
        ui.label(RichText::new("This permanently deletes the instance folder, including all worlds, screenshots and settings. This can't be undone.").color(p.text_secondary));
        ui.add_space(16.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::pill_button(ui, ButtonKind::Danger, None, "Delete").clicked() {
                match app.backend.instances.delete(&id) {
                    Ok(()) => {
                        app.toast(ToastKind::Success, format!("Deleted {}", inst.config.name));
                        app.page = Page::Library;
                        app.backend.sessions.remove(&id);
                        app.reload_instances();
                    }
                    Err(e) => app.toast(ToastKind::Error, format!("Could not delete: {e}")),
                }
                close = true;
            }
            if widgets::pill_button(ui, ButtonKind::Secondary, None, "Cancel").clicked() {
                close = true;
            }
        });
    });
    if close || resp.should_close() {
        app.confirm_delete = None;
    }
}

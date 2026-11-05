//! "New Instance" sheet: pick a name and a Minecraft version.

use eframe::egui::{self, CornerRadius, Margin, Modal, RichText, Sense};
use elysium_core::meta::{ManifestEntry, VersionType};

use crate::app::{ElysiumApp, Page, ToastKind};
use crate::theme::{self, Palette};
use crate::widgets::{self, ButtonKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Filter {
    Release,
    Snapshot,
    Beta,
    Alpha,
}

impl Filter {
    fn matches(self, kind: VersionType) -> bool {
        match self {
            Self::Release => kind == VersionType::Release,
            Self::Snapshot => kind == VersionType::Snapshot,
            Self::Beta => kind == VersionType::OldBeta,
            Self::Alpha => kind == VersionType::OldAlpha,
        }
    }
}

pub struct NewInstanceDialog {
    name: String,
    search: String,
    filter: Filter,
    selected: Option<String>,
}

impl Default for NewInstanceDialog {
    fn default() -> Self {
        Self {
            name: String::new(),
            search: String::new(),
            filter: Filter::Release,
            selected: None,
        }
    }
}

const ROW_HEIGHT: f32 = 44.0;

pub fn show(app: &mut ElysiumApp, ctx: &egui::Context) {
    let Some(mut dialog) = app.new_instance.take() else {
        return;
    };
    let p = Palette::of(ctx);
    if dialog.selected.is_none() {
        dialog.selected = app.manifest.as_ref().map(|m| m.latest.release.clone());
    }

    let mut close = false;
    let mut create = false;
    let response = Modal::new(egui::Id::unique("new_instance"))
        .frame(widgets::card_frame(&p).inner_margin(Margin::same(28)))
        .show(ctx, |ui| {
            ui.set_width(560.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("New Instance")
                        .font(theme::bold(24.0))
                        .color(p.text),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, crate::icons::X, "Close").clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(14.0);

            widgets::section_label(ui, "Name");
            let placeholder = dialog
                .selected
                .clone()
                .map_or_else(|| "My instance".to_owned(), |v| format!("Minecraft {v}"));
            let name_resp =
                widgets::text_field(ui, &mut dialog.name, &placeholder, ui.available_width());
            ui.add_space(14.0);

            widgets::section_label(ui, "Version");
            ui.horizontal(|ui| {
                widgets::segmented(
                    ui,
                    "version_filter",
                    &mut dialog.filter,
                    &[
                        (Filter::Release, "Releases"),
                        (Filter::Snapshot, "Snapshots"),
                        (Filter::Beta, "Beta"),
                        (Filter::Alpha, "Alpha"),
                    ],
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    widgets::search_field(
                        ui,
                        &mut dialog.search,
                        "Filter",
                        ui.available_width().min(170.0),
                    );
                });
            });
            ui.add_space(6.0);
            version_list(app, ui, &mut dialog, &p);
            ui.add_space(18.0);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ready = dialog.selected.is_some();
                if ui
                    .add_enabled_ui(ready, |ui| {
                        widgets::pill_button(ui, ButtonKind::Primary, None, "Create")
                    })
                    .inner
                    .clicked()
                {
                    create = true;
                }
                if widgets::pill_button(ui, ButtonKind::Secondary, None, "Cancel").clicked() {
                    close = true;
                }
            });
            if name_resp.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                && dialog.selected.is_some()
            {
                create = true;
            }
        });
    if response.should_close() {
        close = true;
    }

    if create && let Some(version) = dialog.selected.clone() {
        let name = if dialog.name.trim().is_empty() {
            format!("Minecraft {version}")
        } else {
            dialog.name.trim().to_owned()
        };
        match app.backend.instances.create(&name, &version) {
            Ok(inst) => {
                app.toast(ToastKind::Success, format!("Created {}", inst.config.name));
                app.reload_instances();
                app.page = Page::Instance(inst.id);
                app.instance_tab = crate::app::InstanceTab::Overview;
                return;
            }
            Err(e) => app.toast(ToastKind::Error, format!("Could not create instance: {e}")),
        }
    }
    if !close {
        app.new_instance = Some(dialog);
    }
}

fn version_list(app: &ElysiumApp, ui: &mut egui::Ui, dialog: &mut NewInstanceDialog, p: &Palette) {
    let height = 300.0;
    egui::Frame::new()
        .fill(p.surface_alt)
        .corner_radius(14)
        .inner_margin(Margin::same(6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_height(height);
            let Some(manifest) = &app.manifest else {
                ui.centered_and_justified(|ui| match &app.manifest_error {
                    Some(e) => ui.label(
                        RichText::new(format!("Couldn't load versions: {e}")).color(p.danger),
                    ),
                    None => ui.add(egui::Spinner::new().size(22.0)),
                });
                return;
            };
            let query = dialog.search.trim().to_lowercase();
            let rows: Vec<&ManifestEntry> = manifest
                .versions
                .iter()
                .filter(|v| {
                    dialog.filter.matches(v.kind)
                        && (query.is_empty() || v.id.to_lowercase().contains(&query))
                })
                .collect();
            if rows.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new("No matching versions").color(p.text_secondary))
                });
                return;
            }
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .id_salt(("versions", dialog.filter as u8))
                .show_rows(ui, ROW_HEIGHT, rows.len(), |ui, range| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for v in &rows[range] {
                        let selected = dialog.selected.as_deref() == Some(v.id.as_str());
                        let (rect, resp) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), ROW_HEIGHT),
                            Sense::click(),
                        );
                        let hover = ui.ctx().animate_bool_responsive(resp.id, resp.hovered());
                        let fill = if selected {
                            p.accent_soft
                        } else {
                            widgets::lerp_color(egui::Color32::TRANSPARENT, p.hover, hover)
                        };
                        ui.painter().rect_filled(
                            rect.shrink2(egui::vec2(0.0, 2.0)),
                            CornerRadius::same(10),
                            fill,
                        );
                        let color = if selected { p.accent } else { p.text };
                        ui.painter().text(
                            rect.left_center() + egui::vec2(14.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            &v.id,
                            theme::semibold(14.0),
                            color,
                        );
                        let mut right = v
                            .release_time
                            .with_timezone(&chrono::Local)
                            .format("%b %-d, %Y")
                            .to_string();
                        if manifest.latest.release == v.id || manifest.latest.snapshot == v.id {
                            right = format!("Latest  ·  {right}");
                        }
                        ui.painter().text(
                            rect.right_center() - egui::vec2(14.0, 0.0),
                            egui::Align2::RIGHT_CENTER,
                            right,
                            theme::regular(12.5),
                            p.text_secondary,
                        );
                        if resp
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            dialog.selected = Some(v.id.clone());
                        }
                    }
                });
        });
}

pub mod accounts;
pub mod instance;
pub mod library;
pub mod new_instance;
pub mod settings;

use eframe::egui::{self, RichText};

use crate::theme::{self, Palette};

/// Large page title with an optional subtitle.
pub fn page_header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    let p = Palette::of(ui.ctx());
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.label(RichText::new(title).font(theme::bold(32.0)).color(p.text));
        if !subtitle.is_empty() {
            ui.label(
                RichText::new(subtitle)
                    .font(theme::regular(15.0))
                    .color(p.text_secondary),
            );
        }
    });
}

//! Account management.

use eframe::egui::{self, RichText};
use elysium_core::account::Account;

use crate::app::{ElysiumApp, ToastKind};
use crate::icons;
use crate::theme::{self, Palette};
use crate::views::page_header;
use crate::widgets::{self, ButtonKind};

pub fn show(app: &mut ElysiumApp, ui: &mut egui::Ui) {
    let p = Palette::of(ui.ctx());
    page_header(ui, "Accounts", "Choose who you play as.");
    ui.add_space(20.0);

    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.set_max_width(820.0);
        if !app.accounts.accounts.is_empty() {
            widgets::section_label(ui, "Your accounts");
            let mut remove = None;
            let mut activate = None;
            widgets::card_frame(&p).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let active = app.accounts.active;
                let count = app.accounts.accounts.len();
                for (i, acc) in app.accounts.accounts.iter().enumerate() {
                    ui.horizontal(|ui| {
                        widgets::avatar(ui, &acc.username, 40.0);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.label(RichText::new(&acc.username).font(theme::semibold(15.0)).color(p.text));
                            ui.label(RichText::new("Offline account · singleplayer and offline servers").font(theme::regular(12.5)).color(p.text_secondary));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::icon_button(ui, icons::TRASH, "Remove account").clicked() {
                                remove = Some(acc.uuid);
                            }
                            if active == Some(acc.uuid) {
                                widgets::chip(ui, &format!("{}  Active", icons::CHECK_CIRCLE), p.accent, p.accent_soft);
                            } else if widgets::pill_button(ui, ButtonKind::Secondary, None, "Use").clicked() {
                                activate = Some(acc.uuid);
                            }
                        });
                    });
                    if i + 1 < count {
                        widgets::divider(ui);
                    }
                }
            });
            if let Some(uuid) = activate {
                app.accounts.active = Some(uuid);
                app.save_accounts();
            }
            if let Some(uuid) = remove {
                app.accounts.remove(uuid);
                app.save_accounts();
            }
            ui.add_space(14.0);
        }

        widgets::section_label(ui, "Add an offline account");
        widgets::card_frame(&p).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Offline accounts work in singleplayer and on servers in offline mode. Skins and online servers need a Microsoft account.").font(theme::regular(12.5)).color(p.text_secondary));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let resp = widgets::text_field(ui, &mut app.account_input, "Username", 260.0);
                let submit = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if widgets::pill_button(ui, ButtonKind::Primary, Some(icons::USER_PLUS), "Add").clicked() || submit {
                    match Account::offline(&app.account_input) {
                        Ok(acc) => {
                            app.toast(ToastKind::Success, format!("Playing as {}", acc.username));
                            app.accounts.add(acc);
                            app.save_accounts();
                            app.account_input.clear();
                            app.account_error = None;
                        }
                        Err(e) => app.account_error = Some(e.to_string()),
                    }
                }
            });
            if let Some(err) = &app.account_error {
                ui.label(RichText::new(err).font(theme::regular(12.5)).color(p.danger));
            }
        });
        ui.add_space(14.0);

        widgets::section_label(ui, "Microsoft");
        widgets::card_frame(&p).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(icons::INFO).font(theme::regular(20.0)).color(p.text_secondary));
                ui.label(RichText::new("Microsoft sign-in is coming in a future update.").color(p.text_secondary));
            });
        });
    });
}

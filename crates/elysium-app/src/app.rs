//! Application state, navigation and frame loop.

use std::path::PathBuf;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, Frame, Margin, Modal, RichText, ThemePreference,
};
use elysium_core::account::AccountStore;
use elysium_core::instance::Instance;
use elysium_core::java::JavaInfo;
use elysium_core::meta::VersionManifest;
use elysium_core::settings::{Settings, Theme};

use crate::backend::{Backend, Event};
use crate::icons;
use crate::theme::{self, Palette};
use crate::views;
use crate::widgets::{self, ButtonKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    Library,
    Instance(String),
    Settings,
    Accounts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceTab {
    Overview,
    Settings,
    Logs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub created: Option<f64>,
}

pub struct ElysiumApp {
    pub backend: Backend,
    pub settings: Settings,
    pub accounts: AccountStore,
    pub instances: Vec<Instance>,
    pub manifest: Option<VersionManifest>,
    pub manifest_error: Option<String>,
    pub page: Page,
    pub instance_tab: InstanceTab,
    pub search: String,
    pub new_instance: Option<views::new_instance::NewInstanceDialog>,
    pub confirm_delete: Option<String>,
    pub java_probe: Option<(PathBuf, Result<JavaInfo, String>)>,
    pub account_input: String,
    pub account_error: Option<String>,
    pub log_autoscroll: bool,
    toasts: Vec<Toast>,
    confirm_quit: bool,
    quit_confirmed: bool,
}

impl ElysiumApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, elysium_core::Error> {
        theme::install(&cc.egui_ctx);
        let backend = Backend::new(cc.egui_ctx.clone())?;
        let mut toasts = Vec::new();
        let settings = Settings::load(&backend.paths).unwrap_or_else(|e| {
            toasts.push(Toast {
                text: format!("Settings were reset: {e}"),
                kind: ToastKind::Error,
                created: None,
            });
            Settings::default()
        });
        let accounts = AccountStore::load(&backend.paths).unwrap_or_else(|e| {
            toasts.push(Toast {
                text: format!("Could not read accounts: {e}"),
                kind: ToastKind::Error,
                created: None,
            });
            AccountStore::default()
        });
        apply_theme(&cc.egui_ctx, settings.theme);
        backend.load_manifest();
        let mut app = Self {
            backend,
            settings,
            accounts,
            instances: Vec::new(),
            manifest: None,
            manifest_error: None,
            page: Page::Library,
            instance_tab: InstanceTab::Overview,
            search: String::new(),
            new_instance: None,
            confirm_delete: None,
            java_probe: None,
            account_input: String::new(),
            account_error: None,
            log_autoscroll: true,
            toasts,
            confirm_quit: false,
            quit_confirmed: false,
        };
        app.reload_instances();
        Ok(app)
    }

    pub fn reload_instances(&mut self) {
        match self.backend.instances.list() {
            Ok(list) => self.instances = list,
            Err(e) => self.toast(ToastKind::Error, format!("Could not read instances: {e}")),
        }
    }

    pub fn instance(&self, id: &str) -> Option<&Instance> {
        self.instances.iter().find(|i| i.id == id)
    }

    pub fn toast(&mut self, kind: ToastKind, text: impl Into<String>) {
        self.toasts.push(Toast {
            text: text.into(),
            kind,
            created: None,
        });
    }

    pub fn save_settings(&mut self) {
        if let Err(e) = self.settings.save(&self.backend.paths) {
            self.toast(ToastKind::Error, format!("Could not save settings: {e}"));
        }
    }

    pub fn save_accounts(&mut self) {
        if let Err(e) = self.accounts.save(&self.backend.paths) {
            self.toast(ToastKind::Error, format!("Could not save accounts: {e}"));
        }
    }

    /// Starts (or repairs and starts) an instance with the active account.
    pub fn play(&mut self, instance_id: &str, repair: bool) {
        let Some(account) = self.accounts.active().cloned() else {
            self.toast(ToastKind::Info, "Add an account to start playing");
            self.page = Page::Accounts;
            return;
        };
        let Some(instance) = self.instance(instance_id).cloned() else {
            return;
        };
        self.backend
            .launch(&instance, &account, &self.settings, repair);
    }

    pub fn open_path(&mut self, path: &std::path::Path) {
        if let Err(e) = std::fs::create_dir_all(path).and_then(|()| open::that_detached(path)) {
            self.toast(
                ToastKind::Error,
                format!("Could not open {}: {e}", path.display()),
            );
        }
    }

    fn handle_events(&mut self, ctx: &egui::Context) {
        for event in self.backend.poll() {
            match event {
                Event::Manifest(Ok(m)) => {
                    self.manifest = Some(m);
                    self.manifest_error = None;
                }
                Event::Manifest(Err(e)) => self.manifest_error = Some(e),
                Event::GameStarted => {
                    if self.settings.hide_on_launch {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                }
                Event::SessionEnded {
                    instance_id,
                    outcome,
                } => {
                    self.reload_instances();
                    let name = self
                        .instance(&instance_id)
                        .map_or(instance_id.clone(), |i| i.config.name.clone());
                    match outcome {
                        Ok(exit) if exit.success() => {}
                        Ok(_) => self.toast(
                            ToastKind::Error,
                            format!("{name} crashed. Check the logs for details."),
                        ),
                        Err(e) if e.contains("cancelled") => {
                            self.toast(ToastKind::Info, format!("{name}: launch cancelled"))
                        }
                        Err(e) => self.toast(ToastKind::Error, format!("{name}: {e}")),
                    }
                    if self.settings.hide_on_launch {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    }
                }
                Event::JavaProbed { path, result } => self.java_probe = Some((path, result)),
            }
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let p = Palette::of(ui.ctx());
        egui::Panel::left("sidebar")
            .exact_size(236.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(p.sidebar).inner_margin(Margin {
                left: 16,
                right: 16,
                top: 22,
                bottom: 16,
            }))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::hover());
                    widgets::paint_gradient(
                        ui.painter(),
                        rect,
                        10.0,
                        Color32::from_rgb(0x6A, 0x8D, 0xFF),
                        Color32::from_rgb(0x8E, 0x5B, 0xF6),
                    );
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        icons::SPARKLE,
                        theme::icon_fill(19.0),
                        Color32::WHITE,
                    );
                    ui.add_space(2.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(
                            RichText::new("Elysium")
                                .font(theme::bold(17.0))
                                .color(p.text),
                        );
                        ui.label(
                            RichText::new("Launcher")
                                .font(theme::regular(12.0))
                                .color(p.text_secondary),
                        );
                    });
                });
                ui.add_space(26.0);
                ui.spacing_mut().item_spacing.y = 4.0;

                let in_library = matches!(self.page, Page::Library | Page::Instance(_));
                if widgets::nav_item(ui, icons::SQUARES_FOUR, "Library", in_library).clicked() {
                    self.page = Page::Library;
                }
                if widgets::nav_item(
                    ui,
                    icons::USER_CIRCLE,
                    "Accounts",
                    self.page == Page::Accounts,
                )
                .clicked()
                {
                    self.page = Page::Accounts;
                }
                if widgets::nav_item(ui, icons::GEAR_SIX, "Settings", self.page == Page::Settings)
                    .clicked()
                {
                    self.page = Page::Settings;
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    self.account_chip(ui, &p);
                    let running: Vec<String> = self
                        .instances
                        .iter()
                        .filter(|i| self.backend.is_running(&i.id))
                        .map(|i| i.config.name.clone())
                        .collect();
                    if !running.is_empty() {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            let (r, _) =
                                ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                            ui.painter().circle_filled(r.center(), 4.0, p.success);
                            ui.label(
                                RichText::new(format!("Playing {}", running.join(", ")))
                                    .font(theme::regular(12.5))
                                    .color(p.text_secondary),
                            );
                        });
                    }
                });
            });
    }

    fn account_chip(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 52.0), egui::Sense::click());
        let hover = ui.ctx().animate_bool_responsive(resp.id, resp.hovered());
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(14),
            widgets::lerp_color(
                p.surface.gamma_multiply(if p.dark { 0.6 } else { 0.7 }),
                p.hover,
                hover,
            ),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(egui::vec2(10.0, 8.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        match self.accounts.active() {
            Some(acc) => {
                widgets::avatar(&mut child, &acc.username, 34.0);
                child.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(
                        RichText::new(&acc.username)
                            .font(theme::semibold(13.5))
                            .color(p.text),
                    );
                    ui.label(
                        RichText::new("Offline account")
                            .font(theme::regular(11.5))
                            .color(p.text_secondary),
                    );
                });
            }
            None => {
                child.label(
                    RichText::new(icons::USER_PLUS)
                        .font(theme::regular(20.0))
                        .color(p.accent),
                );
                child.label(
                    RichText::new("Add account")
                        .font(theme::semibold(13.5))
                        .color(p.accent),
                );
            }
        }
        if resp
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            self.page = Page::Accounts;
        }
    }

    fn toasts(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        for t in &mut self.toasts {
            t.created.get_or_insert(now);
        }
        self.toasts
            .retain(|t| t.created.is_some_and(|c| now - c < 5.0));
        if self.toasts.is_empty() {
            return;
        }
        let p = Palette::of(ctx);
        egui::Area::new(egui::Id::unique("toasts"))
            .anchor(Align2::RIGHT_BOTTOM, egui::vec2(-24.0, -24.0))
            .order(egui::Order::Tooltip)
            .interactable(false)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                for t in &self.toasts {
                    let age = (now - t.created.unwrap_or(now)) as f32;
                    let alpha = (age * 6.0).min(1.0).min((5.0 - age) * 3.0).clamp(0.0, 1.0);
                    let (icon, color) = match t.kind {
                        ToastKind::Info => (icons::INFO, p.accent),
                        ToastKind::Success => (icons::CHECK_CIRCLE, p.success),
                        ToastKind::Error => (icons::WARNING_CIRCLE, p.danger),
                    };
                    Frame::new()
                        .fill(if p.dark { p.surface_alt } else { p.console_bg })
                        .corner_radius(14)
                        .inner_margin(Margin::symmetric(16, 12))
                        .shadow(egui::Shadow {
                            offset: [0, 8],
                            blur: 24,
                            spread: 0,
                            color: p.shadow,
                        })
                        .multiply_with_opacity(alpha)
                        .show(ui, |ui| {
                            ui.set_max_width(380.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(icon)
                                        .font(theme::icon_fill(18.0))
                                        .color(color.gamma_multiply(alpha)),
                                );
                                ui.label(
                                    RichText::new(&t.text)
                                        .font(theme::regular(13.5))
                                        .color(Color32::WHITE.gamma_multiply(alpha)),
                                );
                            });
                        });
                }
            });
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }

    fn quit_dialog(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested())
            && !self.quit_confirmed
            && self.backend.any_running()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_quit = true;
        }
        if !self.confirm_quit {
            return;
        }
        let p = Palette::of(ctx);
        let modal = Modal::new(egui::Id::unique("confirm_quit")).frame(widgets::card_frame(&p).inner_margin(Margin::same(28))).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(RichText::new("Minecraft is still running").font(theme::bold(20.0)).color(p.text));
            ui.add_space(4.0);
            ui.label(RichText::new("If you quit, the game keeps running, but this session's playtime won't be recorded and installs in progress are cancelled.").color(p.text_secondary));
            ui.add_space(16.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::pill_button(ui, ButtonKind::Primary, None, "Quit Launcher").clicked() {
                    self.quit_confirmed = true;
                    self.confirm_quit = false;
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if widgets::pill_button(ui, ButtonKind::Secondary, None, "Keep Open").clicked() {
                    self.confirm_quit = false;
                }
            });
        });
        if modal.should_close() {
            self.confirm_quit = false;
        }
    }
}

pub fn apply_theme(ctx: &egui::Context, theme: Theme) {
    ctx.set_theme(match theme {
        Theme::System => ThemePreference::System,
        Theme::Light => ThemePreference::Light,
        Theme::Dark => ThemePreference::Dark,
    });
}

impl eframe::App for ElysiumApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_events(&ctx);
        if self.backend.any_running() {
            // Keep progress bars and the "running" state fresh.
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }

        self.sidebar(ui);
        let p = Palette::of(&ctx);
        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg).inner_margin(Margin {
                left: 40,
                right: 40,
                top: 32,
                bottom: 24,
            }))
            .show(ui, |ui| match self.page.clone() {
                Page::Library => views::library::show(self, ui),
                Page::Instance(id) => views::instance::show(self, ui, &id),
                Page::Settings => views::settings::show(self, ui),
                Page::Accounts => views::accounts::show(self, ui),
            });

        views::new_instance::show(self, &ctx);
        views::instance::delete_dialog(self, &ctx);
        self.quit_dialog(&ctx);
        self.toasts(&ctx);
    }
}

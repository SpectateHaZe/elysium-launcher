//! The library: a grid of instance cards.

use eframe::egui::{self, Align2, Color32, CornerRadius, Rect, RichText, Sense, Shadow, Stroke};
use elysium_core::instance::Instance;

use crate::app::{ElysiumApp, Page};
use crate::backend::Phase;
use crate::icons;
use crate::theme::{self, Palette, RADIUS_CARD};
use crate::views::page_header;
use crate::widgets::{self, ButtonKind};

const CARD_MIN_WIDTH: f32 = 220.0;
const CARD_HEIGHT: f32 = 236.0;
const ART_HEIGHT: f32 = 140.0;
const GAP: f32 = 20.0;

enum CardAction {
    Open,
    Play,
    Stop,
}

pub fn show(app: &mut ElysiumApp, ui: &mut egui::Ui) {
    let p = Palette::of(ui.ctx());
    ui.horizontal(|ui| {
        let count = app.instances.len();
        let subtitle = match count {
            0 => "Your Minecraft instances will appear here".to_owned(),
            1 => "1 instance".to_owned(),
            n => format!("{n} instances"),
        };
        page_header(ui, "Library", &subtitle);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::pill_button(ui, ButtonKind::Primary, Some(icons::PLUS), "New Instance")
                .clicked()
            {
                app.new_instance = Some(Default::default());
            }
            if count > 0 {
                widgets::search_field(ui, &mut app.search, "Search", 220.0);
            }
        });
    });
    ui.add_space(24.0);

    if app.instances.is_empty() {
        empty_state(app, ui, &p);
        return;
    }

    let query = app.search.trim().to_lowercase();
    let visible: Vec<Instance> = app
        .instances
        .iter()
        .filter(|i| {
            query.is_empty()
                || i.config.name.to_lowercase().contains(&query)
                || i.config.game_version.to_lowercase().contains(&query)
        })
        .cloned()
        .collect();
    if visible.is_empty() {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(format!("No instances match “{}”", app.search.trim()))
                    .font(theme::regular(15.0))
                    .color(p.text_secondary),
            );
        });
        return;
    }

    let mut action = None;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            // Leave room for card shadows on the edges.
            let width = ui.available_width() - 8.0;
            let cols = (((width + GAP) / (CARD_MIN_WIDTH + GAP)).floor() as usize).max(1);
            let card_w = (width - GAP * (cols - 1) as f32) / cols as f32;
            for row in visible.chunks(cols) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = GAP;
                    ui.add_space(4.0);
                    for inst in row {
                        if let Some(a) = card(app, ui, inst, card_w, &p) {
                            action = Some((inst.id.clone(), a));
                        }
                    }
                });
                ui.add_space(GAP - ui.spacing().item_spacing.y + 4.0);
            }
        });

    match action {
        Some((id, CardAction::Open)) => {
            app.page = Page::Instance(id);
        }
        Some((id, CardAction::Play)) => app.play(&id, false),
        Some((id, CardAction::Stop)) => app.backend.stop(&id),
        None => {}
    }
}

fn card(
    app: &ElysiumApp,
    ui: &mut egui::Ui,
    inst: &Instance,
    width: f32,
    p: &Palette,
) -> Option<CardAction> {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, CARD_HEIGHT), Sense::click());
    if !ui.is_rect_visible(rect) {
        return None;
    }
    let ctx = ui.ctx().clone();
    let hover = ctx.animate_bool_responsive(resp.id, resp.hovered());
    let lift = hover * 3.0;
    let rect = rect.translate(egui::vec2(0.0, -lift));
    let painter = ui.painter();

    painter.add(
        Shadow {
            offset: [0, (4.0 + hover * 8.0) as i8],
            blur: (16.0 + hover * 18.0) as u8,
            spread: 0,
            color: p
                .shadow
                .gamma_multiply(if p.dark { 0.6 } else { 0.8 + hover * 0.6 }),
        }
        .as_shape(rect, CornerRadius::same(RADIUS_CARD)),
    );
    painter.rect_filled(rect, CornerRadius::same(RADIUS_CARD), p.surface);
    if p.dark {
        painter.rect_stroke(
            rect,
            CornerRadius::same(RADIUS_CARD),
            Stroke::new(1.0, p.border),
            egui::StrokeKind::Inside,
        );
    }

    // Artwork.
    let art = Rect::from_min_size(
        rect.min + egui::vec2(8.0, 8.0),
        egui::vec2(rect.width() - 16.0, ART_HEIGHT),
    );
    let (a, b) = widgets::art_colors(&inst.config.name);
    widgets::paint_gradient(painter, art, 12.0, a, b);
    painter.text(
        art.center(),
        Align2::CENTER_CENTER,
        icons::CUBE,
        theme::regular(54.0),
        Color32::from_white_alpha(200),
    );

    // Text.
    let text_left = rect.left() + 18.0;
    let name_y = art.bottom() + 16.0;
    let name = truncate(
        ui,
        &inst.config.name,
        rect.width() - 36.0,
        theme::semibold(16.0),
    );
    painter.text(
        egui::pos2(text_left, name_y),
        Align2::LEFT_TOP,
        name,
        theme::semibold(16.0),
        p.text,
    );
    painter.text(
        egui::pos2(text_left, name_y + 24.0),
        Align2::LEFT_TOP,
        format!("Minecraft {}", inst.config.game_version),
        theme::regular(13.0),
        p.text_secondary,
    );

    let session = app.backend.sessions.get(&inst.id);
    let phase = session.map(|s| s.phase());
    let status_pos = egui::pos2(text_left, rect.bottom() - 18.0);
    match &phase {
        Some(Phase::Preparing) => {
            let s = session.expect("phase implies session");
            let bar = Rect::from_min_size(
                egui::pos2(text_left, rect.bottom() - 22.0),
                egui::vec2(rect.width() - 36.0, 5.0),
            );
            painter.rect_filled(bar, CornerRadius::same(255), p.surface_alt);
            let frac = s.progress.fraction().unwrap_or(0.0);
            if frac > 0.0 {
                let fill = Rect::from_min_size(
                    bar.min,
                    egui::vec2((bar.width() * frac).max(5.0), bar.height()),
                );
                painter.rect_filled(fill, CornerRadius::same(255), p.accent);
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        Some(Phase::Running { .. }) => {
            painter.circle_filled(status_pos + egui::vec2(4.0, 0.0), 4.0, p.success);
            painter.text(
                status_pos + egui::vec2(14.0, 0.0),
                Align2::LEFT_CENTER,
                "Playing",
                theme::semibold(12.5),
                p.success,
            );
        }
        _ => {
            let text = match inst.config.last_played {
                Some(t) => format!(
                    "{}  ·  {}",
                    widgets::format_relative(t),
                    widgets::format_duration(inst.config.playtime_secs)
                ),
                None => "Never played".to_owned(),
            };
            painter.text(
                status_pos,
                Align2::LEFT_CENTER,
                text,
                theme::regular(12.5),
                p.text_tertiary,
            );
        }
    }

    // Floating play/stop button on the artwork.
    let active = matches!(phase, Some(Phase::Preparing | Phase::Running { .. }));
    let show_button = hover.max(if active { 1.0 } else { 0.0 });
    let mut action = None;
    if show_button > 0.01 {
        let center = egui::pos2(art.right() - 30.0, art.bottom() - 30.0);
        let btn = Rect::from_center_size(center, egui::vec2(44.0, 44.0));
        let btn_resp = ui
            .interact(btn, resp.id.with("play"), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let pressed = ctx.animate_bool_responsive(btn_resp.id, btn_resp.hovered());
        let fill = if active { p.danger } else { p.accent };
        let painter = ui.painter();
        painter.add(
            Shadow {
                offset: [0, 4],
                blur: 12,
                spread: 0,
                color: Color32::from_black_alpha((70.0 * show_button) as u8),
            }
            .as_shape(btn, CornerRadius::same(255)),
        );
        painter.circle_filled(
            center,
            22.0 + pressed * 2.0,
            fill.gamma_multiply(show_button),
        );
        let icon = if active { icons::STOP } else { icons::PLAY };
        let offset = if active { 0.0 } else { 1.5 };
        painter.text(
            center + egui::vec2(offset, 0.0),
            Align2::CENTER_CENTER,
            icon,
            theme::icon_fill(19.0),
            Color32::WHITE.gamma_multiply(show_button),
        );
        if btn_resp.clicked() {
            action = Some(if active {
                CardAction::Stop
            } else {
                CardAction::Play
            });
        }
    }
    if action.is_none()
        && resp
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
    {
        action = Some(CardAction::Open);
    }
    action
}

/// Truncates `text` with an ellipsis to fit `max_width`.
fn truncate(ui: &egui::Ui, text: &str, max_width: f32, font: egui::FontId) -> String {
    let fits = |s: &str| {
        ui.painter()
            .layout_no_wrap(s.to_owned(), font.clone(), Color32::WHITE)
            .size()
            .x
            <= max_width
    };
    if fits(text) {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut len = chars.len();
    while len > 0 {
        let candidate: String = chars[..len].iter().collect::<String>() + "…";
        if fits(&candidate) {
            return candidate;
        }
        len -= 1;
    }
    "…".to_owned()
}

fn empty_state(app: &mut ElysiumApp, ui: &mut egui::Ui, p: &Palette) {
    ui.add_space(60.0);
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(96.0, 96.0), Sense::hover());
        widgets::paint_gradient(ui.painter(), rect, 28.0, Color32::from_rgb(0x6A, 0x8D, 0xFF), Color32::from_rgb(0x8E, 0x5B, 0xF6));
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, icons::CUBE, theme::regular(48.0), Color32::WHITE);
        ui.add_space(20.0);
        ui.label(RichText::new("Create your first instance").font(theme::bold(24.0)).color(p.text));
        ui.label(RichText::new("Pick any Minecraft version, from the latest snapshot back to the very first alpha.").font(theme::regular(15.0)).color(p.text_secondary));
        ui.add_space(18.0);
        if widgets::pill_button_sized(ui, ButtonKind::Primary, Some(icons::PLUS), "New Instance", 46.0).clicked() {
            app.new_instance = Some(Default::default());
        }
    });
}

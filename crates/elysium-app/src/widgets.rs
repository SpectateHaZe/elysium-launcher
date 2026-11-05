//! Custom widgets in the app's design language.

use std::f32::consts::{FRAC_PI_2, PI};
use std::hash::{Hash, Hasher};

use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, Frame, Margin, Mesh, Pos2, Rect, Response,
    Role, Sense, Shadow, Shape, Stroke, TextEdit, Ui, Vec2, WidgetInfo,
};

use crate::icons;
use crate::theme::{self, Palette, RADIUS_CARD, RADIUS_CONTROL};

/// White card with a soft shadow (light) or a hairline border (dark).
pub fn card_frame(p: &Palette) -> Frame {
    let frame = Frame::new()
        .fill(p.surface)
        .corner_radius(RADIUS_CARD)
        .inner_margin(Margin::same(20));
    if p.dark {
        frame.stroke(Stroke::new(1.0, p.border))
    } else {
        frame.shadow(Shadow {
            offset: [0, 2],
            blur: 14,
            spread: 0,
            color: p.shadow,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Danger,
    Ghost,
}

/// Pill-shaped button with an optional leading icon.
pub fn pill_button(ui: &mut Ui, kind: ButtonKind, icon: Option<&str>, text: &str) -> Response {
    pill_button_sized(ui, kind, icon, text, 36.0)
}

pub fn pill_button_sized(
    ui: &mut Ui,
    kind: ButtonKind,
    icon: Option<&str>,
    text: &str,
    height: f32,
) -> Response {
    let p = Palette::of(ui.ctx());
    let font_size = if height >= 44.0 { 16.0 } else { 14.0 };
    let (fg, bg, bg_hover) = match kind {
        ButtonKind::Primary => (p.on_accent, p.accent, p.accent_hover),
        ButtonKind::Secondary => (p.text, p.surface_alt, p.hover),
        ButtonKind::Danger => (p.on_accent, p.danger, p.danger.gamma_multiply(0.85)),
        ButtonKind::Ghost => (p.accent, Color32::TRANSPARENT, p.accent_soft),
    };
    let text_galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), theme::semibold(font_size), fg);
    let icon_galley = icon.map(|i| {
        ui.painter()
            .layout_no_wrap(i.to_owned(), icon_font(i, font_size + 2.0), fg)
    });
    let gap = 8.0;
    let content_w = text_galley.size().x + icon_galley.as_ref().map_or(0.0, |g| g.size().x + gap);
    let pad = if height >= 44.0 { 28.0 } else { 18.0 };
    let size = egui::vec2(content_w + pad * 2.0, height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let enabled = ui.is_enabled();
    response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, text));

    if ui.is_rect_visible(rect) {
        let t = ui
            .ctx()
            .animate_bool_responsive(response.id, response.hovered());
        let mut fill = lerp_color(bg, bg_hover, t);
        if response.is_pointer_button_down_on() {
            fill = fill.gamma_multiply(0.92);
        }
        let painter = ui.painter();
        if kind == ButtonKind::Primary && !p.dark {
            let glow = rect
                .translate(egui::vec2(0.0, 3.0))
                .shrink2(egui::vec2(6.0, 2.0));
            painter.add(
                Shadow {
                    offset: [0, 0],
                    blur: 12,
                    spread: 0,
                    color: p.accent.gamma_multiply(0.22),
                }
                .as_shape(glow, CornerRadius::same(255)),
            );
        }
        painter.rect_filled(rect, CornerRadius::same(255), fill);
        let mut x = rect.center().x - content_w / 2.0;
        if let Some(g) = icon_galley {
            let w = g.size().x;
            painter.galley(Pos2::new(x, rect.center().y - g.size().y / 2.0), g, fg);
            x += w + gap;
        }
        painter.galley(
            Pos2::new(x, rect.center().y - text_galley.size().y / 2.0),
            text_galley,
            fg,
        );
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Play/stop and status icons use the filled style; everything else is outlined.
pub fn icon_font(icon: &str, size: f32) -> egui::FontId {
    match icon {
        icons::PLAY
        | icons::STOP
        | icons::CHECK_CIRCLE
        | icons::WARNING_CIRCLE
        | icons::X_CIRCLE => theme::icon_fill(size),
        _ => theme::regular(size),
    }
}

/// Round icon-only button.
pub fn icon_button(ui: &mut Ui, icon: &str, tooltip: &str) -> Response {
    let p = Palette::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), Sense::click());
    let enabled = ui.is_enabled();
    response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, tooltip));
    if ui.is_rect_visible(rect) {
        let t = ui
            .ctx()
            .animate_bool_responsive(response.id, response.hovered());
        let fill = lerp_color(Color32::TRANSPARENT, p.hover, t);
        ui.painter()
            .rect_filled(rect, CornerRadius::same(255), fill);
        let color = if enabled {
            p.text_secondary
        } else {
            p.text_tertiary
        };
        let color = lerp_color(color, p.text, t);
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            icon,
            theme::regular(18.0),
            color,
        );
    }
    response
        .on_hover_text(tooltip)
        .on_hover_cursor(CursorIcon::PointingHand)
}

/// Sidebar navigation item with a pill highlight.
pub fn nav_item(ui: &mut Ui, icon: &str, label: &str, selected: bool) -> Response {
    let p = Palette::of(ui.ctx());
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), Sense::click());
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, selected, label));
    if ui.is_rect_visible(rect) {
        let hover = ui
            .ctx()
            .animate_bool_responsive(response.id, response.hovered());
        let sel = ui
            .ctx()
            .animate_bool_responsive(response.id.with("sel"), selected);
        let fill = lerp_color(
            lerp_color(Color32::TRANSPARENT, p.hover, hover * 0.7),
            p.accent_soft,
            sel,
        );
        ui.painter().rect_filled(rect, CornerRadius::same(12), fill);
        let fg = lerp_color(p.text_secondary, p.accent, sel);
        let fg = lerp_color(fg, p.text, hover * (1.0 - sel));
        ui.painter().text(
            rect.left_center() + egui::vec2(14.0, 0.0),
            Align2::LEFT_CENTER,
            icon,
            if selected {
                theme::icon_fill(18.0)
            } else {
                theme::regular(18.0)
            },
            fg,
        );
        let font = if selected {
            theme::semibold(14.0)
        } else {
            theme::regular(14.0)
        };
        ui.painter().text(
            rect.left_center() + egui::vec2(44.0, 0.0),
            Align2::LEFT_CENTER,
            label,
            font,
            fg,
        );
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// iOS-style segmented control. Returns true if the selection changed.
pub fn segmented<T: PartialEq + Copy>(
    ui: &mut Ui,
    id: &str,
    value: &mut T,
    options: &[(T, &str)],
) -> bool {
    let p = Palette::of(ui.ctx());
    let font = theme::semibold(13.0);
    let seg_w = options
        .iter()
        .map(|(_, l)| {
            ui.painter()
                .layout_no_wrap((*l).to_owned(), font.clone(), p.text)
                .size()
                .x
                + 28.0
        })
        .fold(72.0f32, f32::max);
    let size = egui::vec2(seg_w * options.len() as f32 + 4.0, 34.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter().clone();
    painter.rect_filled(rect, CornerRadius::same(RADIUS_CONTROL), p.surface_alt);

    let index = options.iter().position(|(v, _)| v == value).unwrap_or(0);
    let anim_id = ui.scope_id().with(id).with("segmented");
    let x = ui
        .ctx()
        .animate_value_with_time(anim_id, index as f32, 0.18);
    let thumb = Rect::from_min_size(
        rect.min + egui::vec2(2.0 + x * seg_w, 2.0),
        egui::vec2(seg_w, rect.height() - 4.0),
    );
    let thumb_fill = if p.dark { p.hover } else { p.surface };
    painter.add(
        Shadow {
            offset: [0, 1],
            blur: 4,
            spread: 0,
            color: p.shadow,
        }
        .as_shape(thumb, CornerRadius::same(8)),
    );
    painter.rect_filled(thumb, CornerRadius::same(8), thumb_fill);

    let mut changed = false;
    for (i, (option, label)) in options.iter().enumerate() {
        let seg = Rect::from_min_size(
            rect.min + egui::vec2(2.0 + i as f32 * seg_w, 2.0),
            egui::vec2(seg_w, rect.height() - 4.0),
        );
        let resp = ui
            .interact(seg, anim_id.with(i), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        let selected = option == value;
        resp.widget_info(|| WidgetInfo::selected(Role::RadioButton, true, selected, *label));
        if resp.clicked() && !selected {
            *value = *option;
            changed = true;
        }
        let color = if selected { p.text } else { p.text_secondary };
        painter.text(
            seg.center(),
            Align2::CENTER_CENTER,
            *label,
            font.clone(),
            color,
        );
    }
    changed
}

/// iOS-style switch. Returns the response (changed() when toggled).
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let p = Palette::of(ui.ctx());
    let size = egui::vec2(46.0, 28.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let value = *on;
    response.widget_info(|| WidgetInfo::selected(Role::Switch, true, value, ""));
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_responsive(response.id, *on);
        let track = lerp_color(
            if p.dark {
                p.hover
            } else {
                Color32::from_rgb(0xD9, 0xD9, 0xDE)
            },
            p.accent,
            t,
        );
        ui.painter()
            .rect_filled(rect, CornerRadius::same(255), track);
        let r = rect.height() / 2.0 - 3.0;
        let cx = egui::lerp((rect.left() + 3.0 + r)..=(rect.right() - 3.0 - r), t);
        let c = Pos2::new(cx, rect.center().y);
        ui.painter()
            .circle_filled(c + egui::vec2(0.0, 1.0), r, Color32::from_black_alpha(40));
        ui.painter().circle_filled(c, r, Color32::WHITE);
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Rounded single-line text input.
pub fn text_field(ui: &mut Ui, text: &mut String, hint: &str, width: f32) -> Response {
    let p = Palette::of(ui.ctx());
    ui.add(
        TextEdit::singleline(text)
            .hint_text(hint.to_owned())
            .desired_width(width)
            .frame(input_frame(&p).inner_margin(Margin::symmetric(12, 9))),
    )
}

/// Rounded search input with a magnifying glass.
pub fn search_field(ui: &mut Ui, text: &mut String, hint: &str, width: f32) -> Response {
    let p = Palette::of(ui.ctx());
    ui.add(
        TextEdit::singleline(text)
            .hint_text(hint.to_owned())
            .prefix(format!("{} ", icons::MAGNIFYING_GLASS))
            .desired_width(width)
            .frame(
                input_frame(&p)
                    .corner_radius(255)
                    .inner_margin(Margin::symmetric(14, 9)),
            ),
    )
}

fn input_frame(p: &Palette) -> Frame {
    Frame::new()
        .fill(p.surface_alt)
        .corner_radius(RADIUS_CONTROL)
        .stroke(Stroke::new(
            1.0,
            p.border.gamma_multiply(if p.dark { 1.0 } else { 0.6 }),
        ))
}

/// Small rounded label.
pub fn chip(ui: &mut Ui, text: &str, fg: Color32, bg: Color32) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), theme::semibold(12.0), fg);
    let size = galley.size() + egui::vec2(20.0, 8.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(255), bg);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, fg);
    response
}

/// Thin rounded progress bar; `None` shows an indeterminate shimmer.
pub fn progress_bar(ui: &mut Ui, fraction: Option<f32>, width: f32) {
    let p = Palette::of(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 6.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(255), p.surface_alt);
    match fraction {
        Some(f) => {
            let w = rect.width() * f.clamp(0.0, 1.0);
            if w > 0.5 {
                let fill =
                    Rect::from_min_size(rect.min, egui::vec2(w.max(rect.height()), rect.height()));
                painter.rect_filled(fill, CornerRadius::same(255), p.accent);
            }
        }
        None => {
            let t = ui.input(|i| i.time) as f32;
            let seg = rect.width() * 0.3;
            let x = (t * 0.8).fract() * (rect.width() + seg) - seg;
            let bar = Rect::from_min_max(
                Pos2::new((rect.left() + x).max(rect.left()), rect.top()),
                Pos2::new((rect.left() + x + seg).min(rect.right()), rect.bottom()),
            );
            if bar.width() > 0.0 {
                painter.rect_filled(bar, CornerRadius::same(255), p.accent);
            }
            ui.ctx().request_repaint();
        }
    }
}

/// Circular avatar with the first letter of `name`.
pub fn avatar(ui: &mut Ui, name: &str, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let (a, b) = art_colors(name);
    paint_gradient(ui.painter(), rect, size / 2.0, a, b);
    let letter = name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        letter,
        theme::bold(size * 0.45),
        Color32::WHITE,
    );
    response
}

/// Deterministic pleasant gradient pair for a name.
pub fn art_colors(name: &str) -> (Color32, Color32) {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut h);
    let hash = h.finish();
    let hue = (hash % 360) as f32 / 360.0;
    let shift = 0.08 + ((hash >> 16) % 10) as f32 / 100.0;
    (hsv(hue, 0.58, 0.88), hsv((hue + shift) % 1.0, 0.78, 0.64))
}

fn hsv(h: f32, s: f32, v: f32) -> Color32 {
    egui::ecolor::Hsva::new(h, s, v, 1.0).into()
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    a.lerp_to_gamma(b, t.clamp(0.0, 1.0))
}

/// Fills a rounded rectangle with a diagonal gradient from `a` (top-left) to `b`
/// (bottom-right).
pub fn paint_gradient(painter: &egui::Painter, rect: Rect, radius: f32, a: Color32, b: Color32) {
    let points = rounded_rect_points(rect, radius);
    let mut mesh = Mesh::default();
    let diag = rect.size().length_sq().max(1.0);
    let color_at = |pos: Pos2| {
        let t = (pos - rect.min).dot(rect.size()) / diag;
        lerp_color(a, b, t)
    };
    mesh.colored_vertex(rect.center(), color_at(rect.center()));
    for &pt in &points {
        mesh.colored_vertex(pt, color_at(pt));
    }
    let n = points.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    painter.add(Shape::mesh(mesh));
    // Hairline outline for anti-aliased edges.
    painter.add(Shape::closed_line(
        points,
        Stroke::new(1.0, lerp_color(a, b, 0.5).gamma_multiply(0.6)),
    ));
}

fn rounded_rect_points(rect: Rect, radius: f32) -> Vec<Pos2> {
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let corners = [
        (Pos2::new(rect.right() - r, rect.top() + r), -FRAC_PI_2),
        (Pos2::new(rect.right() - r, rect.bottom() - r), 0.0),
        (Pos2::new(rect.left() + r, rect.bottom() - r), FRAC_PI_2),
        (Pos2::new(rect.left() + r, rect.top() + r), PI),
    ];
    let steps = 10;
    let mut pts = Vec::with_capacity(4 * (steps + 1));
    for (center, start) in corners {
        for s in 0..=steps {
            let angle = start + FRAC_PI_2 * s as f32 / steps as f32;
            pts.push(center + r * egui::vec2(angle.cos(), angle.sin()));
        }
    }
    pts
}

/// Section label (small caps style) used above grouped settings.
pub fn section_label(ui: &mut Ui, text: &str) {
    let p = Palette::of(ui.ctx());
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .font(theme::semibold(11.5))
            .color(p.text_tertiary),
    );
}

/// One row inside a settings card: title + description on the left, control on the right.
pub fn setting_row(ui: &mut Ui, title: &str, description: &str, control: impl FnOnce(&mut Ui)) {
    let p = Palette::of(ui.ctx());
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(
                egui::RichText::new(title)
                    .font(theme::semibold(14.0))
                    .color(p.text),
            );
            if !description.is_empty() {
                ui.label(
                    egui::RichText::new(description)
                        .font(theme::regular(12.5))
                        .color(p.text_secondary),
                );
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
    });
}

/// Hairline divider spanning the available width.
pub fn divider(ui: &mut Ui) {
    let p = Palette::of(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter()
        .hline(rect.x_range(), rect.center().y, Stroke::new(1.0, p.border));
}

/// Formats seconds as "3h 12m" / "12m" / "45s".
pub fn format_duration(secs: u64) -> String {
    let (h, m) = (secs / 3600, (secs % 3600) / 60);
    match (h, m) {
        (0, 0) => format!("{secs}s"),
        (0, m) => format!("{m}m"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// "Today", "Yesterday", "3 days ago", or a date.
pub fn format_relative(when: chrono::DateTime<chrono::Utc>) -> String {
    let local = when.with_timezone(&chrono::Local);
    let days = (chrono::Local::now().date_naive() - local.date_naive()).num_days();
    match days {
        ..=0 => format!("Today, {}", local.format("%H:%M")),
        1 => "Yesterday".to_owned(),
        2..=6 => format!("{days} days ago"),
        _ => local.format("%b %-d, %Y").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(format_duration(45), "45s");
        assert_eq!(format_duration(125), "2m");
        assert_eq!(format_duration(3 * 3600 + 12 * 60), "3h 12m");
    }

    #[test]
    fn art_colors_are_deterministic() {
        assert_eq!(art_colors("Survival"), art_colors("Survival"));
    }
}

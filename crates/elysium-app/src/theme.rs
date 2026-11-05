//! Visual design: palette, typography and egui style setup.
//!
//! The look borrows from Apple's and Google's product sites: generous whitespace,
//! large confident type, soft neutral surfaces, big corner radii and a single vivid
//! accent colour. Every colour used by the app comes from [`Palette`].

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow,
    Stroke, Style, TextStyle, Theme, Visuals,
};

pub const SEMIBOLD: &str = "semibold";
pub const BOLD: &str = "bold";
pub const ICON_FILL: &str = "icons-fill";

/// Corner radius for cards and sheets.
pub const RADIUS_CARD: u8 = 18;
/// Corner radius for controls (buttons are pills; inputs use this).
pub const RADIUS_CONTROL: u8 = 10;

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    /// Window background.
    pub bg: Color32,
    /// Sidebar background.
    pub sidebar: Color32,
    /// Cards and sheets.
    pub surface: Color32,
    /// Controls on top of surfaces (inputs, secondary buttons).
    pub surface_alt: Color32,
    pub hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_secondary: Color32,
    pub text_tertiary: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_soft: Color32,
    pub on_accent: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub console_bg: Color32,
    pub shadow: Color32,
}

impl Palette {
    pub const LIGHT: Self = Self {
        dark: false,
        bg: Color32::from_rgb(0xF5, 0xF5, 0xF7),
        sidebar: Color32::from_rgb(0xEC, 0xEC, 0xEF),
        surface: Color32::WHITE,
        surface_alt: Color32::from_rgb(0xF0, 0xF0, 0xF3),
        hover: Color32::from_rgb(0xE3, 0xE3, 0xE8),
        border: Color32::from_rgb(0xDE, 0xDE, 0xE3),
        text: Color32::from_rgb(0x1D, 0x1D, 0x1F),
        text_secondary: Color32::from_rgb(0x6E, 0x6E, 0x73),
        text_tertiary: Color32::from_rgb(0x9A, 0x9A, 0xA0),
        accent: Color32::from_rgb(0x1A, 0x73, 0xE8),
        accent_hover: Color32::from_rgb(0x17, 0x5F, 0xC7),
        accent_soft: Color32::from_rgb(0xE5, 0xEF, 0xFD),
        on_accent: Color32::WHITE,
        success: Color32::from_rgb(0x1E, 0x8E, 0x3E),
        warning: Color32::from_rgb(0xC2, 0x6A, 0x00),
        danger: Color32::from_rgb(0xD9, 0x30, 0x25),
        console_bg: Color32::from_rgb(0x1C, 0x1C, 0x1E),
        shadow: Color32::from_black_alpha(22),
    };

    pub const DARK: Self = Self {
        dark: true,
        bg: Color32::from_rgb(0x0E, 0x0E, 0x10),
        sidebar: Color32::from_rgb(0x16, 0x16, 0x19),
        surface: Color32::from_rgb(0x1C, 0x1C, 0x1F),
        surface_alt: Color32::from_rgb(0x27, 0x27, 0x2B),
        hover: Color32::from_rgb(0x30, 0x30, 0x35),
        border: Color32::from_rgb(0x2E, 0x2E, 0x33),
        text: Color32::from_rgb(0xF5, 0xF5, 0xF7),
        text_secondary: Color32::from_rgb(0xA1, 0xA1, 0xA6),
        text_tertiary: Color32::from_rgb(0x6E, 0x6E, 0x73),
        accent: Color32::from_rgb(0x4C, 0x8D, 0xF6),
        accent_hover: Color32::from_rgb(0x6A, 0xA1, 0xF8),
        accent_soft: Color32::from_rgb(0x1B, 0x2A, 0x45),
        on_accent: Color32::WHITE,
        success: Color32::from_rgb(0x34, 0xC7, 0x59),
        warning: Color32::from_rgb(0xFF, 0x9F, 0x0A),
        danger: Color32::from_rgb(0xFF, 0x45, 0x3A),
        console_bg: Color32::from_rgb(0x0A, 0x0A, 0x0C),
        shadow: Color32::from_black_alpha(90),
    };

    pub fn of(ctx: &egui::Context) -> Self {
        Self::for_theme(ctx.theme())
    }

    pub fn for_theme(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self::DARK,
            Theme::Light => Self::LIGHT,
        }
    }
}

/// Font helpers.
pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}
pub fn icon_fill(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(ICON_FILL.into()))
}

pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);
    for theme in [Theme::Light, Theme::Dark] {
        ctx.set_style_of(theme, style_for(Palette::for_theme(theme)));
    }
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, data: &'static [u8]| {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(data)));
    };
    add(
        "inter-regular",
        include_bytes!("../assets/fonts/Inter-Regular.otf"),
    );
    add(
        "inter-semibold",
        include_bytes!("../assets/fonts/Inter-SemiBold.otf"),
    );
    add(
        "inter-bold",
        include_bytes!("../assets/fonts/Inter-Bold.otf"),
    );
    add("phosphor", include_bytes!("../assets/fonts/Phosphor.ttf"));
    add(
        "phosphor-fill",
        include_bytes!("../assets/fonts/Phosphor-Fill.ttf"),
    );

    // egui's bundled fonts stay as fallbacks for symbols Inter lacks.
    let fallbacks = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let family = |primary: &str| {
        let mut v = vec![primary.to_owned(), "phosphor".to_owned()];
        v.extend(fallbacks.iter().cloned());
        v
    };
    fonts
        .families
        .insert(FontFamily::Proportional, family("inter-regular"));
    fonts
        .families
        .insert(FontFamily::Name(SEMIBOLD.into()), family("inter-semibold"));
    fonts
        .families
        .insert(FontFamily::Name(BOLD.into()), family("inter-bold"));
    fonts.families.insert(
        FontFamily::Name(ICON_FILL.into()),
        vec!["phosphor-fill".to_owned()],
    );
    if let Some(mono) = fonts.families.get_mut(&FontFamily::Monospace) {
        mono.push("phosphor".to_owned());
    }
    ctx.set_fonts(fonts);
}

fn style_for(p: Palette) -> Style {
    let mut style = Style {
        text_styles: [
            (TextStyle::Small, regular(11.5)),
            (TextStyle::Body, regular(14.0)),
            (
                TextStyle::Button,
                FontId::new(14.0, FontFamily::Name(SEMIBOLD.into())),
            ),
            (TextStyle::Heading, bold(28.0)),
            (TextStyle::Monospace, FontId::monospace(12.5)),
        ]
        .into(),
        ..Style::default()
    };

    let s = &mut style.spacing;
    s.item_spacing = egui::vec2(10.0, 10.0);
    s.button_padding = egui::vec2(16.0, 8.0);
    s.interact_size = egui::vec2(40.0, 34.0);
    s.combo_height = 320.0;
    s.slider_width = 260.0;
    s.text_edit_width = 280.0;
    s.window_margin = Margin::same(24);
    s.menu_margin = Margin::same(8);
    s.scroll.bar_width = 6.0;
    s.scroll.floating = true;

    let mut v = if p.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.faint_bg_color = p.surface_alt;
    v.extreme_bg_color = p.surface_alt;
    v.text_edit_bg_color = Some(p.surface_alt);
    v.code_bg_color = p.surface_alt;
    v.override_text_color = None;
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.danger;
    v.window_corner_radius = CornerRadius::same(RADIUS_CARD);
    v.menu_corner_radius = CornerRadius::same(12);
    v.window_stroke = Stroke::new(1.0, p.border);
    let shadow = Shadow {
        offset: [0, 12],
        blur: 40,
        spread: 0,
        color: p.shadow,
    };
    v.window_shadow = shadow;
    v.popup_shadow = Shadow {
        offset: [0, 6],
        blur: 20,
        spread: 0,
        color: p.shadow,
    };
    v.selection.bg_fill = p.accent.gamma_multiply(if p.dark { 0.45 } else { 0.25 });
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    v.striped = false;

    let radius = CornerRadius::same(RADIUS_CONTROL);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = radius;

    w.inactive.bg_fill = p.surface_alt;
    w.inactive.weak_bg_fill = p.surface_alt;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = radius;

    w.hovered.bg_fill = p.hover;
    w.hovered.weak_bg_fill = p.hover;
    w.hovered.bg_stroke = Stroke::NONE;
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.hover;
    w.active.weak_bg_fill = p.hover;
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(1.5, p.text);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;

    w.open = w.active;
    w.open.bg_stroke = Stroke::NONE;
    style.visuals = v;
    style
}

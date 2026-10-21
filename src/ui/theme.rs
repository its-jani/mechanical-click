//! Palette, typography and egui style. One place to keep the Komi Store look
//! consistent across every view.

use std::collections::BTreeMap;
use std::sync::Arc;

use egui::{
    Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke,
    Style, TextStyle, Vec2, Visuals,
};

use crate::config::Theme;

pub const MEDIUM: &str = "geist-medium";
pub const SEMIBOLD: &str = "geist-semibold";

pub const RADIUS_CARD: u8 = 14;
pub const RADIUS_CTRL: u8 = 10;

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub sidebar: Color32,
    pub surface: Color32,
    pub surface_hi: Color32,
    pub surface_low: Color32,
    pub input: Color32,
    pub border: Color32,
    pub border_soft: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    pub accent: Color32,
    pub accent_hi: Color32,
    pub on_accent: Color32,
    pub success: Color32,
    pub warn: Color32,
    pub danger: Color32,
    pub shadow: Color32,
}

pub const fn rgb(rgb: u32) -> Color32 {
    Color32::from_rgb(
        ((rgb >> 16) & 0xFF) as u8,
        ((rgb >> 8) & 0xFF) as u8,
        (rgb & 0xFF) as u8,
    )
}

pub fn alpha(color: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        color.r(),
        color.g(),
        color.b(),
        (a.clamp(0.0, 1.0) * 255.0) as u8,
    )
}

/// Linear RGB mix preserving opacity.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(
        f(a.r(), b.r()),
        f(a.g(), b.g()),
        f(a.b(), b.b()),
        f(a.a(), b.a()),
    )
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            dark: true,
            bg: rgb(0x0D1117),
            sidebar: rgb(0x0A0E14),
            surface: rgb(0x161B22),
            surface_hi: rgb(0x1D242D),
            surface_low: rgb(0x10151C),
            input: rgb(0x0B0F15),
            border: rgb(0x30363D),
            border_soft: Color32::from_rgba_unmultiplied(0x30, 0x36, 0x3D, 130),
            text: rgb(0xE6EDF3),
            text_muted: rgb(0x9BA7B4),
            text_faint: rgb(0x6E7681),
            accent: rgb(0x2F81F7),
            accent_hi: rgb(0x38BDF8),
            on_accent: rgb(0xFFFFFF),
            success: rgb(0x3FB950),
            warn: rgb(0xD29922),
            danger: rgb(0xF85149),
            shadow: Color32::from_black_alpha(110),
        }
    }

    pub fn light() -> Self {
        Self {
            dark: false,
            bg: rgb(0xF6F8FA),
            sidebar: rgb(0xFFFFFF),
            surface: rgb(0xFFFFFF),
            surface_hi: rgb(0xF2F5F9),
            surface_low: rgb(0xEDF1F6),
            input: rgb(0xFFFFFF),
            border: rgb(0xD0D7DE),
            border_soft: Color32::from_rgba_unmultiplied(0x8C, 0x95, 0x9F, 90),
            text: rgb(0x1F2328),
            text_muted: rgb(0x59636E),
            text_faint: rgb(0x818B98),
            accent: rgb(0x0969DA),
            accent_hi: rgb(0x4F46E5),
            on_accent: rgb(0xFFFFFF),
            success: rgb(0x1A7F37),
            warn: rgb(0x9A6700),
            danger: rgb(0xCF222E),
            shadow: Color32::from_black_alpha(24),
        }
    }

    pub fn for_theme(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self::dark(),
            Theme::Light => Self::light(),
        }
    }

    pub fn kind_color(&self, kind: crate::config::SwitchKind) -> Color32 {
        let (r, g, b) = kind.rgb();
        let base = Color32::from_rgb(r, g, b);
        if self.dark {
            base
        } else {
            // Slightly deepen the accents for contrast on white.
            mix(base, rgb(0x1F2328), 0.18)
        }
    }
}

// ---------------------------------------------------------------------------
// Type
// ---------------------------------------------------------------------------

pub fn family(name: &'static str) -> FontFamily {
    FontFamily::Name(name.into())
}

pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn medium(size: f32) -> FontId {
    FontId::new(size, family(MEDIUM))
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, family(SEMIBOLD))
}

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

pub fn install_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    let regular = include_bytes!("../../assets/fonts/Geist-Regular.ttf");
    let medium_face = include_bytes!("../../assets/fonts/Geist-Medium.ttf");
    let semibold_face = include_bytes!("../../assets/fonts/Geist-SemiBold.ttf");
    let mono_face = include_bytes!("../../assets/fonts/GeistMono-Regular.ttf");

    fonts
        .font_data
        .insert("geist".into(), Arc::new(FontData::from_static(regular)));
    fonts.font_data.insert(
        "geist-medium".into(),
        Arc::new(FontData::from_static(medium_face)),
    );
    fonts.font_data.insert(
        "geist-semibold".into(),
        Arc::new(FontData::from_static(semibold_face)),
    );
    fonts.font_data.insert(
        "geist-mono".into(),
        Arc::new(FontData::from_static(mono_face)),
    );

    fonts.families.insert(
        FontFamily::Proportional,
        vec!["geist".into(), "geist-medium".into()],
    );
    fonts
        .families
        .insert(FontFamily::Monospace, vec!["geist-mono".into()]);
    fonts
        .families
        .insert(family(MEDIUM), vec!["geist-medium".into(), "geist".into()]);
    fonts.families.insert(
        family(SEMIBOLD),
        vec![
            "geist-semibold".into(),
            "geist-medium".into(),
            "geist".into(),
        ],
    );

    ctx.set_fonts(fonts);
}

// ---------------------------------------------------------------------------
// Style
// ---------------------------------------------------------------------------

pub fn apply(ctx: &Context, pal: &Palette) {
    let pref = if pal.dark {
        egui::ThemePreference::Dark
    } else {
        egui::ThemePreference::Light
    };
    ctx.set_theme(pref);

    let mut visuals = if pal.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    visuals.panel_fill = pal.bg;
    visuals.window_fill = pal.surface;
    visuals.extreme_bg_color = pal.input;
    visuals.faint_bg_color = pal.surface_low;
    visuals.override_text_color = Some(pal.text);
    visuals.window_stroke = Stroke::new(1.0, pal.border);
    visuals.window_corner_radius = CornerRadius::same(12);
    visuals.menu_corner_radius = CornerRadius::same(10);
    visuals.selection.bg_fill = alpha(pal.accent, 0.35);
    visuals.selection.stroke = Stroke::new(1.0, pal.accent);
    visuals.hyperlink_color = pal.accent;
    visuals.slider_trailing_fill = true;

    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = pal.surface;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, pal.border_soft);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0, pal.text);

    widgets.inactive.bg_fill = pal.surface_hi;
    widgets.inactive.weak_bg_fill = pal.surface_hi;
    widgets.inactive.bg_stroke = Stroke::new(1.0, pal.border);
    widgets.inactive.fg_stroke = Stroke::new(1.0, pal.text);
    widgets.inactive.corner_radius = CornerRadius::same(RADIUS_CTRL);

    widgets.hovered.bg_fill = mix(pal.surface_hi, pal.accent, 0.10);
    widgets.hovered.weak_bg_fill = mix(pal.surface_hi, pal.accent, 0.10);
    widgets.hovered.bg_stroke = Stroke::new(1.0, alpha(pal.accent, 0.55));
    widgets.hovered.fg_stroke = Stroke::new(1.0, pal.text);
    widgets.hovered.corner_radius = CornerRadius::same(RADIUS_CTRL);

    widgets.active.bg_fill = alpha(pal.accent, 0.22);
    widgets.active.weak_bg_fill = alpha(pal.accent, 0.22);
    widgets.active.bg_stroke = Stroke::new(1.0, alpha(pal.accent, 0.8));
    widgets.active.fg_stroke = Stroke::new(1.0, pal.text);
    widgets.active.corner_radius = CornerRadius::same(RADIUS_CTRL);

    widgets.open.bg_fill = pal.surface_hi;
    widgets.open.bg_stroke = Stroke::new(1.0, pal.border);
    widgets.open.corner_radius = CornerRadius::same(RADIUS_CTRL);

    ctx.set_visuals_of(
        if pal.dark {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        },
        visuals,
    );

    ctx.all_styles_mut(|style| tune(style, pal));
}

fn tune(style: &mut Style, _pal: &Palette) {
    style.animation_time = 0.13;
    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(12.0, 7.0);
    style.spacing.menu_margin = Margin::same(8);
    style.spacing.interact_size.y = 28.0;
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.floating = true;

    let mut text = BTreeMap::new();
    text.insert(TextStyle::Heading, semibold(22.0));
    text.insert(TextStyle::Body, font(13.5));
    text.insert(TextStyle::Button, medium(13.0));
    text.insert(TextStyle::Small, font(11.5));
    text.insert(TextStyle::Monospace, mono(11.5));
    style.text_styles = text;
}

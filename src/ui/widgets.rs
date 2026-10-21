//! Hand-drawn widgets: cards, pills, sliders, toggles, waveforms, keycaps and a
//! small procedural icon set. Everything is painted with the palette so both
//! themes stay pixel-consistent.

use egui::{
    Align2, Color32, CornerRadius, Layout, Mesh, Pos2, Rect, Response, Sense, Stroke, StrokeKind,
    Ui, UiBuilder, Vec2, pos2, vec2,
};

use super::theme::{self, Palette, RADIUS_CARD, RADIUS_CTRL, alpha, mix};

// ---------------------------------------------------------------------------
// Painting helpers
// ---------------------------------------------------------------------------

pub fn gradient_rect(
    painter: &egui::Painter,
    rect: Rect,
    radius: f32,
    top: Color32,
    bottom: Color32,
) {
    let r = radius
        .min(rect.width() * 0.5)
        .min(rect.height() * 0.5)
        .max(0.0);
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.center(), mix(top, bottom, 0.5));

    let corners = [
        (rect.right_bottom() - vec2(r, r), 0.0f32),
        (rect.left_bottom() + vec2(r, -r), 0.5 * std::f32::consts::PI),
        (rect.left_top() + vec2(r, r), std::f32::consts::PI),
        (rect.right_top() + vec2(-r, r), 1.5 * std::f32::consts::PI),
    ];
    let segs = 7;
    for (center, start) in corners {
        for i in 0..=segs {
            let angle = start + i as f32 / segs as f32 * (0.5 * std::f32::consts::PI);
            let point = pos2(center.x + r * angle.cos(), center.y + r * angle.sin());
            let t = ((point.y - rect.top()) / rect.height().max(1.0)).clamp(0.0, 1.0);
            mesh.colored_vertex(point, mix(top, bottom, t));
        }
    }
    let last = mesh.vertices.len() as u32 - 1;
    for i in 1..=last {
        let next = if i == last { 1 } else { i + 1 };
        mesh.add_triangle(0, i, next);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// A soft radial light. Cheap: one triangle fan, no blur pass.
pub fn glow(painter: &egui::Painter, center: Pos2, radius: f32, color: Color32) {
    if radius <= 1.0 {
        return;
    }
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, color);
    let segs = 44;
    for i in 0..=segs {
        let a = i as f32 / segs as f32 * std::f32::consts::TAU;
        mesh.colored_vertex(
            pos2(center.x + radius * a.cos(), center.y + radius * a.sin()),
            Color32::TRANSPARENT,
        );
    }
    for i in 1..=segs as u32 {
        let next = if i == segs as u32 { 1 } else { i + 1 };
        mesh.add_triangle(0, i, next);
    }
    painter.add(egui::Shape::mesh(mesh));
}

pub fn hairline(painter: &egui::Painter, rect: Rect, pal: &Palette) {
    painter.line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0, pal.border_soft),
    );
}

pub fn divider(ui: &mut Ui, pal: &Palette) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
    hairline(ui.painter(), rect, pal);
}

// ---------------------------------------------------------------------------
// Cards
// ---------------------------------------------------------------------------

pub struct CardStyle {
    pub selected: bool,
    pub interactive: bool,
    pub spotlight: Option<Color32>,
}

impl Default for CardStyle {
    fn default() -> Self {
        Self {
            selected: false,
            interactive: true,
            spotlight: None,
        }
    }
}

/// Allocates a rounded card and returns the space to draw its content into.
pub fn card<R>(
    ui: &mut Ui,
    pal: &Palette,
    size: Vec2,
    style: CardStyle,
    add: impl FnOnce(&mut Ui) -> R,
) -> (Response, R) {
    let sense = if style.interactive {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let hover_t = ui
        .ctx()
        .animate_bool_responsive(response.id, response.hovered() && style.interactive);

    let fill = if style.selected {
        mix(pal.surface, pal.accent, 0.07)
    } else if hover_t > 0.0 {
        mix(pal.surface, pal.surface_hi, hover_t.max(0.35))
    } else {
        pal.surface
    };
    let border = if style.selected {
        alpha(pal.accent, 0.65)
    } else if hover_t > 0.0 {
        mix(pal.border, alpha(pal.accent, 0.8), hover_t * 0.55)
    } else {
        pal.border
    };

    let painter = ui.painter();
    // A blurred `epaint::Shadow` costs hundreds of tessellated quads per card,
    // which dominates frame time on software rasterizers. Two offset plates
    // give the same lifted look for the price of two rects.
    painter.rect_filled(
        Rect::from_min_max(rect.min + vec2(0.0, 6.0), rect.max + vec2(0.0, 6.0)).expand(2.0),
        CornerRadius::same(RADIUS_CARD + 2),
        alpha(pal.shadow, 0.35),
    );
    painter.rect_filled(
        Rect::from_min_max(rect.min + vec2(0.0, 3.0), rect.max + vec2(0.0, 3.0)).expand(1.0),
        CornerRadius::same(RADIUS_CARD + 1),
        alpha(pal.shadow, 0.45),
    );
    painter.rect_filled(rect, CornerRadius::same(RADIUS_CARD), fill);
    if let Some(color) = style.spotlight {
        painter.rect_filled(
            Rect::from_min_size(rect.min, vec2(rect.width(), 2.0)),
            CornerRadius {
                nw: RADIUS_CARD,
                ne: RADIUS_CARD,
                sw: 0,
                se: 0,
            },
            alpha(color, 0.9),
        );
    }
    painter.rect_stroke(
        rect,
        CornerRadius::same(RADIUS_CARD),
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );

    let inner = rect.shrink2(vec2(16.0, 14.0));
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(inner.intersect(ui.clip_rect()));
    let inside = add(&mut child);
    (response, inside)
}

pub fn mount_card<R>(ui: &mut Ui, pal: &Palette, height: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let width = ui.available_width();
    let (_, inside) = card(ui, pal, vec2(width, height), CardStyle::default(), add);
    inside
}

// ---------------------------------------------------------------------------
// Small pieces
// ---------------------------------------------------------------------------

pub fn badge(ui: &mut Ui, pal: &Palette, label: &str, color: Color32) -> Response {
    let font = theme::medium(10.5);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_uppercase(), font, color);
    let size = vec2(galley.size().x + 16.0, 19.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(6), alpha(color, 0.14));
    painter.rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0, alpha(color, 0.28)),
        StrokeKind::Inside,
    );
    painter.galley(rect.center() - galley.size() * 0.5, galley, pal.text);
    response
}

pub fn dot(painter: &egui::Painter, center: Pos2, radius: f32, color: Color32) {
    painter.circle_filled(center, radius, color);
}

pub fn active_dot(ui: &mut Ui, pal: &Palette, active: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(9.0, 9.0), Sense::hover());
    let color = if active { pal.success } else { pal.text_faint };
    if active {
        glow(ui.painter(), rect.center(), 9.0, alpha(pal.success, 0.25));
    }
    dot(ui.painter(), rect.center(), 3.5, color);
    response
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

pub fn button(ui: &mut Ui, pal: &Palette, label: &str, kind: ButtonKind) -> Response {
    let font = theme::medium(12.5);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font, Color32::WHITE);
    let size = vec2(galley.size().x + 26.0, 30.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let hover = ui
        .ctx()
        .animate_bool_responsive(response.id, response.hovered());

    let (fill, stroke, text) = match kind {
        ButtonKind::Primary => (
            mix(pal.accent, pal.accent_hi, hover * 0.55),
            mix(pal.accent, pal.accent_hi, hover),
            pal.on_accent,
        ),
        ButtonKind::Secondary => (
            mix(pal.surface_hi, pal.surface, 1.0 - hover),
            mix(pal.border, alpha(pal.accent, 0.7), hover),
            pal.text,
        ),
        ButtonKind::Ghost => (
            alpha(pal.surface_hi, hover * 0.9),
            Color32::TRANSPARENT,
            pal.text,
        ),
        ButtonKind::Danger => (
            alpha(pal.danger, 0.10 + hover * 0.10),
            alpha(pal.danger, 0.35 + hover * 0.3),
            pal.danger,
        ),
    };

    let painter = ui.painter();
    if kind != ButtonKind::Ghost {
        painter.rect_filled(rect, CornerRadius::same(RADIUS_CTRL), fill);
        painter.rect_stroke(
            rect,
            CornerRadius::same(RADIUS_CTRL),
            Stroke::new(1.0, stroke),
            StrokeKind::Inside,
        );
    } else if hover > 0.01 {
        painter.rect_filled(rect, CornerRadius::same(RADIUS_CTRL), fill);
    }
    painter.galley(
        rect.center() - galley.size() * 0.5,
        ui.painter()
            .layout_no_wrap(label.to_string(), theme::medium(12.5), text),
        text,
    );
    response
}

pub fn icon_button(ui: &mut Ui, pal: &Palette, icon: Icon, active: bool) -> Response {
    let size = vec2(30.0, 30.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let hover = ui
        .ctx()
        .animate_bool_responsive(response.id, response.hovered());
    let fill = if active {
        alpha(pal.accent, 0.16)
    } else if hover > 0.01 {
        alpha(pal.surface_hi, hover)
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, CornerRadius::same(9), fill);
    }
    let color = if active {
        pal.accent
    } else if response.hovered() {
        pal.text
    } else {
        pal.text_muted
    };
    draw_icon(ui.painter(), rect.shrink(8.0), icon, color, 1.6);
    response
}

// ---------------------------------------------------------------------------
// Slider / toggle / progress
// ---------------------------------------------------------------------------

pub fn slider(
    ui: &mut Ui,
    pal: &Palette,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    width: f32,
) -> Response {
    let height = 22.0;
    let (rect, mut response) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
    let hover = ui
        .ctx()
        .animate_bool_responsive(response.id, response.hovered());

    let (min, max) = (*range.start(), *range.end());
    if let Some(pos) = response.interact_pointer_pos() {
        let t = ((pos.x - rect.left() - 7.0) / (rect.width() - 14.0)).clamp(0.0, 1.0);
        let next = min + (max - min) * t;
        if (next - *value).abs() > f32::EPSILON {
            *value = next;
            response.mark_changed();
        }
    }

    let t = ((*value - min) / (max - min)).clamp(0.0, 1.0);
    let track = Rect::from_center_size(
        pos2(rect.center().x, rect.center().y),
        vec2(rect.width() - 14.0, 5.0),
    );
    let painter = ui.painter();
    painter.rect_filled(track, CornerRadius::same(3), pal.surface_low);
    painter.rect_stroke(
        track,
        CornerRadius::same(3),
        Stroke::new(1.0, pal.border_soft),
        StrokeKind::Inside,
    );
    let filled = Rect::from_min_size(track.min, vec2(track.width() * t, track.height()));
    if filled.width() > 0.5 {
        gradient_rect(
            painter,
            filled,
            3.0,
            pal.accent,
            mix(pal.accent, pal.accent_hi, 0.7),
        );
    }
    let knob = pos2(track.left() + track.width() * t, rect.center().y);
    if hover > 0.0 {
        glow(painter, knob, 13.0, alpha(pal.accent, 0.20 * hover));
    }
    painter.circle_filled(knob, 6.5 + hover * 0.8, pal.surface);
    painter.circle_stroke(knob, 6.5 + hover * 0.8, Stroke::new(2.0, pal.accent));
    painter.circle_filled(knob, 2.0, mix(pal.accent, pal.accent_hi, 0.4));
    response
}

pub fn toggle(ui: &mut Ui, pal: &Palette, on: &mut bool) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(vec2(42.0, 24.0), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(response.id, *on);
    let track = Rect::from_center_size(rect.center(), vec2(42.0, 24.0));
    let painter = ui.painter();
    let fill = mix(pal.surface_low, pal.accent, t);
    painter.rect_filled(track, CornerRadius::same(12), fill);
    if t > 0.5 {
        glow(painter, track.center(), 18.0, alpha(pal.accent, 0.10));
    }
    painter.rect_stroke(
        track,
        CornerRadius::same(12),
        Stroke::new(1.0, mix(pal.border, alpha(pal.accent, 0.8), t)),
        StrokeKind::Inside,
    );
    let x = track.left() + 12.0 + t * (track.width() - 24.0);
    painter.circle_filled(
        pos2(x, track.center().y),
        9.0,
        mix(pal.text_faint, pal.on_accent, t),
    );
    response
}

pub fn progress(ui: &mut Ui, pal: &Palette, fraction: f32, width: f32, height: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        CornerRadius::same(height as u8 / 2 + 1),
        pal.surface_low,
    );
    let w = rect.width() * fraction.clamp(0.0, 1.0);
    if w > 1.0 {
        let filled = Rect::from_min_size(rect.min, vec2(w, rect.height()));
        gradient_rect(
            painter,
            filled,
            height / 2.0,
            mix(pal.accent, pal.accent_hi, 0.15),
            pal.accent_hi,
        );
    }
    response
}

// ---------------------------------------------------------------------------
// Waveform + keycap illustration
// ---------------------------------------------------------------------------

pub fn waveform(
    ui: &mut Ui,
    pal: &Palette,
    peaks: &[f32],
    width: f32,
    height: f32,
    color: Color32,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    draw_waveform(ui.painter(), rect, peaks, color, pal);
    response
}

pub fn draw_waveform(
    painter: &egui::Painter,
    rect: Rect,
    peaks: &[f32],
    color: Color32,
    pal: &Palette,
) {
    if peaks.is_empty() {
        return;
    }
    let n = peaks.len();
    let gap = 2.0;
    let bar = ((rect.width() - gap * (n as f32 - 1.0)) / n as f32).max(1.0);
    let mid = rect.center().y;
    for (i, peak) in peaks.iter().enumerate() {
        let x = rect.left() + i as f32 * (bar + gap);
        let h = (peak.clamp(0.0, 1.0) * rect.height() * 0.92).max(2.0);
        let fade = 0.55 + 0.45 * (1.0 - i as f32 / n as f32);
        let bar_rect = Rect::from_min_max(pos2(x, mid - h * 0.5), pos2(x + bar, mid + h * 0.5));
        painter.rect_filled(
            bar_rect,
            CornerRadius::same((bar * 0.5) as u8),
            alpha(color, 0.85 * fade),
        );
    }
    let _ = pal;
}

/// A keycap drawn in perspective-ish 2D, with a press offset and ripple.
pub fn keycap(ui: &mut Ui, pal: &Palette, label: &str, size: Vec2, press: f32, accent: Color32) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    let lift = press.clamp(0.0, 1.0);
    let body = Rect::from_min_size(
        rect.min + vec2(0.0, 8.0),
        vec2(rect.width(), rect.height() - 8.0),
    );
    let face_h = body.height() * 0.80;
    let face = Rect::from_min_size(
        pos2(body.left(), body.top() + lift * 5.0),
        vec2(body.width(), face_h),
    );
    let radius = (rect.width() * 0.16) as u8;

    // Drop shadow.
    painter.rect_filled(
        body.translate(vec2(0.0, 8.0)),
        CornerRadius::same(radius),
        Color32::from_black_alpha(if pal.dark { 90 } else { 28 }),
    );
    // Cap sides.
    painter.rect_filled(
        body,
        CornerRadius::same(radius),
        mix(pal.surface_hi, pal.accent, 0.18),
    );
    painter.rect_stroke(
        body,
        CornerRadius::same(radius),
        Stroke::new(1.0, alpha(pal.accent, 0.25)),
        StrokeKind::Inside,
    );
    // Top face.
    gradient_rect(
        painter,
        face,
        radius as f32,
        mix(
            pal.surface,
            Color32::WHITE,
            if pal.dark { 0.12 } else { 0.65 },
        ),
        mix(pal.surface_hi, pal.accent, 0.10),
    );
    painter.rect_stroke(
        face,
        CornerRadius::same(radius),
        Stroke::new(1.0, alpha(accent, 0.45)),
        StrokeKind::Inside,
    );
    // Top highlight.
    painter.line_segment(
        [
            pos2(face.left() + 10.0, face.top() + 1.5),
            pos2(face.right() - 10.0, face.top() + 1.5),
        ],
        Stroke::new(
            1.0,
            alpha(Color32::WHITE, if pal.dark { 0.16 } else { 0.9 }),
        ),
    );

    let font = if label.len() > 3 {
        theme::semibold(13.0)
    } else {
        theme::semibold(20.0)
    };
    painter.text(
        face.center(),
        Align2::CENTER_CENTER,
        label,
        font,
        mix(pal.text, accent, 0.2),
    );

    if press > 0.01 {
        for i in 0..3 {
            let spread = 6.0 + (i as f32 + 1.0) * 10.0 * press;
            painter.circle_stroke(
                face.center(),
                spread + face.width() * 0.18,
                Stroke::new(1.2, alpha(accent, (1.0 - press) * 0.30 / (i as f32 + 1.0))),
            );
        }
    }
}

pub fn stat_chip(ui: &mut Ui, pal: &Palette, label: &str, value: &str, accent: Color32) {
    let font = theme::mono(11.0);
    let value_galley = ui.painter().layout_no_wrap(value.to_string(), font, accent);
    let label_galley =
        ui.painter()
            .layout_no_wrap(label.to_string(), theme::font(11.0), pal.text_faint);
    let width = 12.0 + value_galley.size().x + 6.0 + label_galley.size().x + 12.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, 24.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(7), alpha(pal.surface_low, 0.75));
    painter.rect_stroke(
        rect,
        CornerRadius::same(7),
        Stroke::new(1.0, pal.border_soft),
        StrokeKind::Inside,
    );
    let x = rect.left() + 12.0;
    let y = rect.center().y;
    painter.galley(
        pos2(x, y - value_galley.size().y * 0.5),
        value_galley.clone(),
        accent,
    );
    painter.galley(
        pos2(
            x + value_galley.size().x + 6.0,
            y - label_galley.size().y * 0.5,
        ),
        label_galley,
        pal.text_faint,
    );
}

// ---------------------------------------------------------------------------
// Icons
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Home,
    Store,
    Library,
    Settings,
    Play,
    Trash,
    Theme,
}

pub fn draw_icon(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32, width: f32) {
    let s = rect.width().min(rect.height()) / 24.0;
    let p = |x: f32, y: f32| pos2(rect.min.x + x * s, rect.min.y + y * s);
    let stroke = Stroke::new(width, color);
    let line = |a: (f32, f32), b: (f32, f32)| {
        painter.line_segment([p(a.0, a.1), p(b.0, b.1)], stroke);
    };
    let arc = |cx: f32, cy: f32, r: f32, from: f32, to: f32, segs: usize| {
        let mut pts = Vec::with_capacity(segs + 1);
        for i in 0..=segs {
            let t = from + (to - from) * i as f32 / segs as f32;
            pts.push(p(cx + r * t.cos(), cy + r * t.sin()));
        }
        painter.add(egui::Shape::line(pts, stroke));
    };

    match icon {
        Icon::Home => {
            line((3.0, 10.5), (12.0, 3.0));
            line((12.0, 3.0), (21.0, 10.5));
            line((5.5, 9.5), (5.5, 20.0));
            line((18.5, 9.5), (18.5, 20.0));
            line((5.5, 20.0), (18.5, 20.0));
            line((10.0, 20.0), (10.0, 14.0));
            line((10.0, 14.0), (14.0, 14.0));
            line((14.0, 14.0), (14.0, 20.0));
        }
        Icon::Store => {
            painter.rect_stroke(
                Rect::from_min_max(p(4.0, 8.0), p(20.0, 21.0)),
                CornerRadius::same(3),
                stroke,
                StrokeKind::Middle,
            );
            arc(
                12.0,
                8.0,
                4.5,
                std::f32::consts::PI,
                std::f32::consts::TAU,
                10,
            );
        }
        Icon::Library => {
            line((5.0, 4.0), (5.0, 20.0));
            line((9.5, 4.0), (9.5, 20.0));
            painter.line_segment([p(14.0, 5.5), p(17.5, 20.0)], stroke);
            line((19.5, 4.5), (21.0, 19.5));
        }
        Icon::Settings => {
            line((4.0, 7.0), (20.0, 7.0));
            line((4.0, 12.0), (15.0, 12.0));
            line((4.0, 17.0), (20.0, 17.0));
            painter.circle_filled(p(15.5, 7.0), 2.4 * s.max(0.5), color);
            painter.circle_filled(p(9.0, 12.0), 2.4 * s.max(0.5), color);
            painter.circle_filled(p(8.0, 17.0), 2.4 * s.max(0.5), color);
        }
        Icon::Play => {
            painter.add(egui::Shape::convex_polygon(
                vec![p(8.0, 5.0), p(19.0, 12.0), p(8.0, 19.0)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Trash => {
            line((4.5, 7.0), (19.5, 7.0));
            line((9.5, 7.0), (9.5, 4.0));
            line((9.5, 4.0), (14.5, 4.0));
            line((14.5, 4.0), (14.5, 7.0));
            line((6.5, 7.0), (7.5, 20.0));
            line((17.5, 7.0), (16.5, 20.0));
            line((7.5, 20.0), (16.5, 20.0));
        }
        Icon::Theme => {
            painter.circle_stroke(p(12.0, 12.0), 8.0 * s, stroke);
            let mut pts = Vec::new();
            for i in 0..=12 {
                let a = -0.5 * std::f32::consts::PI + i as f32 / 12.0 * std::f32::consts::PI;
                pts.push(p(12.0 + 8.0 * a.cos(), 12.0 + 8.0 * a.sin()));
            }
            pts.push(p(12.0, 4.0));
            painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        }
    }
}

// ---------------------------------------------------------------------------
// Text helpers
// ---------------------------------------------------------------------------

pub fn label(ui: &mut Ui, text: &str, font: egui::FontId, color: Color32) -> Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, color);
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::hover());
    ui.painter().galley(rect.min, galley, color);
    response
}

pub fn section_label(ui: &mut Ui, pal: &Palette, text: &str) {
    let _ = label(
        ui,
        &text.to_uppercase(),
        theme::medium(10.5),
        pal.text_faint,
    );
}

/// Left-aligned title + right-aligned control row used across settings.
pub fn setting_row(
    ui: &mut Ui,
    pal: &Palette,
    title: &str,
    description: &str,
    control: impl FnOnce(&mut Ui),
) {
    let width = ui.available_width();
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            vec2(width - 168.0, 0.0),
            Layout::top_down(egui::Align::Min),
            |ui| {
                label(ui, title, theme::medium(13.0), pal.text);
                if !description.is_empty() {
                    ui.add_space(2.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(description)
                                .font(theme::font(11.5))
                                .color(pal.text_muted),
                        )
                        .wrap(),
                    );
                }
            },
        );
        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
            control(ui);
        });
    });
}

/// Row with a state dot and a label, used by the sidebar engine panel.
pub fn status_pill(ui: &mut Ui, _pal: &Palette, text: &str, color: Color32) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), theme::medium(11.0), color);
    let width = galley.size().x + 26.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(11), alpha(color, 0.10));
    painter.rect_stroke(
        rect,
        CornerRadius::same(11),
        Stroke::new(1.0, alpha(color, 0.22)),
        StrokeKind::Inside,
    );
    dot(
        painter,
        pos2(rect.left() + 11.0, rect.center().y),
        3.0,
        color,
    );
    painter.galley(
        pos2(rect.left() + 19.0, rect.center().y - galley.size().y * 0.5),
        galley,
        color,
    );
    response
}

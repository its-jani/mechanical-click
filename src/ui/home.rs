//! Home: live hero for the active switch, volume controls and the quick-switch
//! grid.

use egui::{
    Align, CornerRadius, Label, Layout, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2,
};

use super::page_header;
use super::theme::{self, Palette, alpha, mix};
use super::widgets::{self, CardStyle, Icon};
use super::{Action, App, Nav};
use crate::config::Profile;

impl App {
    pub(super) fn view_home(&mut self, ui: &mut Ui, act: &mut Option<Action>) {
        let pal = self.pal;
        let active = self.active().cloned();
        let active_id = self.cfg.profile_id.clone();

        page_header(
            ui,
            &pal,
            "Sound",
            "Pick the switch that follows your keyboard.",
            |ui| {
                let cpu = format!("{:.1}%", self.perf.cpu());
                let ram = format!("{:.0} MB", self.perf.ram_mb());
                widgets::stat_chip(ui, &pal, "CPU", &cpu, pal.success);
                widgets::stat_chip(ui, &pal, "RAM", &ram, pal.accent);
                if widgets::icon_button(ui, &pal, Icon::Theme, false)
                    .on_hover_text("Toggle dark / light")
                    .clicked()
                {
                    *act = Some(Action::ToggleTheme);
                }
            },
        );

        self.hero(ui, &pal, active.as_ref());

        ui.add_space(14.0);
        self.volume_card(ui, &pal, &active_id, act);

        if !self.cfg.dismissed_hint {
            ui.add_space(12.0);
            widgets::mount_card(ui, &pal, 76.0, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.add_space(2.0);
                        widgets::label(
                            ui,
                            "Want more switches?",
                            theme::medium(13.0),
                            pal.text,
                        );
                        ui.add_space(1.0);
                        widgets::label(
                            ui,
                            "Grab community packs from the store, or drop your own .wav / .ogg samples in the library.",
                            theme::font(11.5),
                            pal.text_muted,
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::button(ui, &pal, "Dismiss", widgets::ButtonKind::Ghost)
                            .clicked()
                        {
                            *act = Some(Action::DismissHint);
                        }
                        if widgets::button(ui, &pal, "Sound Store", widgets::ButtonKind::Primary)
                            .clicked()
                        {
                            *act = Some(Action::Nav(Nav::Store));
                        }
                    });
                });
            });
        }

        ui.add_space(22.0);
        ui.horizontal(|ui| {
            widgets::section_label(ui, &pal, "Switch library");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                widgets::label(
                    ui,
                    &format!("{} profiles", self.profiles.len()),
                    theme::mono(10.5),
                    pal.text_faint,
                );
            });
        });
        ui.add_space(8.0);

        let list = self.profiles.clone();
        let mut cards: Vec<(Profile, Vec<f32>)> = Vec::with_capacity(list.len());
        for profile in list {
            let peaks = self.engine.waveform(&profile.press, 18);
            cards.push((profile, peaks));
        }

        let gap = 12.0;
        let cols = 3;
        let card_w = ((ui.available_width() - gap * (cols - 1) as f32) / cols as f32).max(170.0);
        let mut index = 0;
        while index < cards.len() {
            ui.horizontal(|ui| {
                for _ in 0..cols {
                    if index >= cards.len() {
                        break;
                    }
                    let (profile, peaks) = &cards[index];
                    index += 1;
                    profile_card(
                        ui,
                        &pal,
                        profile,
                        peaks,
                        card_w,
                        profile.id == active_id,
                        act,
                    );
                    ui.add_space(gap);
                }
            });
            ui.add_space(gap);
        }
    }

    fn hero(&self, ui: &mut Ui, pal: &Palette, active: Option<&Profile>) {
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(vec2(width, 236.0), Sense::hover());
        let kind_color = active.map(|p| pal.kind_color(p.kind)).unwrap_or(pal.accent);
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));

        painter.rect_filled(rect, CornerRadius::same(16), pal.surface);
        widgets::gradient_rect(
            &painter,
            rect.shrink(1.0),
            15.0,
            mix(pal.surface, pal.accent, 0.22),
            mix(pal.surface, pal.accent, 0.02),
        );
        widgets::glow(
            &painter,
            pos2(rect.right() - 110.0, rect.top() + 26.0),
            250.0,
            alpha(pal.accent, 0.16),
        );
        widgets::glow(
            &painter,
            pos2(rect.left() + 60.0, rect.bottom()),
            220.0,
            alpha(kind_color, 0.10),
        );
        painter.rect_stroke(
            rect,
            CornerRadius::same(16),
            Stroke::new(1.0, mix(pal.border, kind_color, 0.30)),
            egui::StrokeKind::Inside,
        );

        let inner = rect.shrink2(vec2(22.0, 18.0));
        let mut left = ui.new_child(
            UiBuilder::new()
                .max_rect(Rect::from_min_size(
                    inner.min,
                    vec2(inner.width() - 190.0, 150.0),
                ))
                .layout(Layout::top_down(Align::Min)),
        );
        widgets::section_label(&mut left, pal, "Active profile");
        left.add_space(3.0);
        match active {
            Some(profile) => {
                widgets::label(&mut left, &profile.name, theme::semibold(27.0), pal.text);
                left.add_space(2.0);
                left.add(
                    Label::new(
                        RichText::new(&profile.description)
                            .font(theme::font(12.0))
                            .color(pal.text_muted),
                    )
                    .truncate(),
                );
                left.add_space(8.0);
                left.horizontal(|ui| {
                    widgets::badge(ui, pal, profile.kind.label(), kind_color);
                    widgets::badge(
                        ui,
                        pal,
                        if profile.builtin {
                            "Bundled"
                        } else {
                            "Installed"
                        },
                        if profile.builtin {
                            pal.accent
                        } else {
                            pal.success
                        },
                    );
                    if profile.release.is_some() {
                        widgets::badge(ui, pal, "Release", pal.text_faint);
                    }
                    if !self.cfg.enabled {
                        widgets::badge(ui, pal, "Muted", pal.danger);
                    }
                });
            }
            None => {
                widgets::label(&mut left, "No profile yet", theme::semibold(25.0), pal.text);
                left.add_space(4.0);
                widgets::label(
                    &mut left,
                    "Open the library to add a switch sound.",
                    theme::font(12.0),
                    pal.text_muted,
                );
            }
        }

        // Keycap with the live press animation.
        let kc_rect = Rect::from_min_size(
            pos2(inner.right() - 118.0, inner.top() + 8.0),
            vec2(110.0, 116.0),
        );
        let mut kc = ui.new_child(UiBuilder::new().max_rect(kc_rect));
        let press = if self.key_down {
            self.pulse().max(0.55)
        } else {
            self.pulse()
        };
        widgets::keycap(
            &mut kc,
            pal,
            &self.key_label,
            vec2(110.0, 116.0),
            press,
            kind_color,
        );
        painter.text(
            pos2(kc_rect.center().x, kc_rect.bottom() + 6.0),
            egui::Align2::CENTER_TOP,
            format!("{} keys", self.key_count),
            theme::mono(10.0),
            pal.text_faint,
        );

        // Bottom row: stat chips + waveform.
        let bar_top = rect.bottom() - 62.0;
        painter.line_segment(
            [
                pos2(inner.left(), bar_top - 10.0),
                pos2(inner.right(), bar_top - 10.0),
            ],
            Stroke::new(1.0, pal.border_soft),
        );
        let mut chips = ui.new_child(
            UiBuilder::new()
                .max_rect(Rect::from_min_size(
                    pos2(inner.left(), bar_top),
                    vec2(300.0, 32.0),
                ))
                .layout(Layout::left_to_right(Align::Center)),
        );
        let latency = self
            .engine
            .status
            .latency_ms()
            .max(self.engine.status.requested_ms as f32);
        widgets::stat_chip(
            &mut chips,
            pal,
            "buffer",
            &format!("{latency:.0} ms"),
            pal.accent,
        );
        widgets::stat_chip(
            &mut chips,
            pal,
            "press",
            &format!("{:.0} ms", self.press_ms),
            pal.text_muted,
        );
        widgets::stat_chip(
            &mut chips,
            pal,
            "keys",
            &self.key_count.to_string(),
            pal.success,
        );

        let wave_rect = Rect::from_min_max(
            pos2(inner.left() + 316.0, bar_top),
            pos2(inner.right(), bar_top + 34.0),
        );
        widgets::draw_waveform(&painter, wave_rect, &self.waveform, kind_color, pal);
    }

    fn volume_card(
        &mut self,
        ui: &mut Ui,
        pal: &Palette,
        active_id: &str,
        act: &mut Option<Action>,
    ) {
        let mut volume = self.cfg.volume;
        let mut enabled = self.cfg.enabled;
        let ready = self.engine.status.ready();

        widgets::mount_card(ui, pal, 64.0, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    widgets::label(ui, "Output volume", theme::medium(12.5), pal.text);
                    widgets::label(
                        ui,
                        &format!("{:.0}%", volume * 100.0),
                        theme::mono(10.5),
                        pal.text_faint,
                    );
                });
                ui.add_space(12.0);
                let slider_w = (ui.available_width() - 250.0).max(110.0);
                widgets::slider(ui, pal, &mut volume, 0.0..=2.0, slider_w);
                ui.add_space(8.0);
                if widgets::button(ui, pal, "Preview", widgets::ButtonKind::Secondary).clicked() {
                    *act = Some(Action::PreviewProfile(active_id.to_string()));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::toggle(ui, pal, &mut enabled)
                        .on_hover_text("Enable or mute keyboard sounds")
                        .changed()
                    {
                        *act = Some(Action::ToggleEnabled);
                    }
                    if !ready {
                        widgets::status_pill(ui, pal, "Audio offline", pal.danger);
                    }
                });
            });
        });

        if (volume - self.cfg.volume).abs() > 0.001 {
            self.cfg.volume = volume;
        }
    }
}

fn profile_card(
    ui: &mut Ui,
    pal: &Palette,
    profile: &Profile,
    peaks: &[f32],
    width: f32,
    active: bool,
    act: &mut Option<Action>,
) {
    let kind_color = pal.kind_color(profile.kind);
    let style = CardStyle {
        selected: active,
        interactive: true,
        spotlight: Some(kind_color),
    };
    let (response, _) = widgets::card(ui, pal, vec2(width, 144.0), style, |ui| {
        ui.spacing_mut().item_spacing.y = 3.0;
        ui.horizontal(|ui| {
            let name_w = (ui.available_width() - 62.0).max(60.0);
            ui.allocate_ui_with_layout(
                vec2(name_w, 22.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    widgets::label(ui, &profile.name, theme::medium(13.5), pal.text);
                },
            );
            if widgets::icon_button(ui, pal, Icon::Play, false)
                .on_hover_text("Preview")
                .clicked()
            {
                *act = Some(Action::PreviewProfile(profile.id.clone()));
            }
            widgets::active_dot(ui, pal, active);
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            widgets::badge(ui, pal, profile.kind.label(), kind_color);
            widgets::badge(
                ui,
                pal,
                if profile.builtin { "Bundled" } else { "Custom" },
                pal.text_faint,
            );
        });
        ui.add_space(2.0);
        ui.add(
            Label::new(
                RichText::new(&profile.description)
                    .font(theme::font(11.0))
                    .color(pal.text_muted),
            )
            .truncate(),
        );
        ui.add_space(1.0);
        widgets::waveform(ui, pal, peaks, ui.available_width(), 24.0, kind_color);
    });
    if response.clicked() {
        *act = Some(Action::Select(profile.id.clone()));
    }
}

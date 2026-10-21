//! Library: every installed profile with preview, use and delete controls.

use egui::{Align, Label, Layout, RichText, Ui, vec2};

use super::page_header;
use super::theme;
use super::widgets::{self, ButtonKind, CardStyle, Icon};
use super::{Action, App, Nav};
use crate::config::{self, Profile};

impl App {
    pub(super) fn view_library(&mut self, ui: &mut Ui, act: &mut Option<Action>) {
        let pal = self.pal;
        page_header(ui, &pal, "Library", Nav::Library.subtitle(), |ui| {
            if widgets::button(ui, &pal, "Import sample", ButtonKind::Primary).clicked() {
                *act = Some(Action::Import);
            }
            if widgets::button(ui, &pal, "Rescan", ButtonKind::Ghost).clicked() {
                *act = Some(Action::Rescan);
            }
            if widgets::button(ui, &pal, "Open folder", ButtonKind::Ghost).clicked() {
                *act = Some(Action::OpenFolder);
            }
        });

        let list = self.profiles.clone();
        let active_id = self.cfg.profile_id.clone();
        let mut rows: Vec<(Profile, Vec<f32>, f32, f32)> = Vec::with_capacity(list.len());
        for profile in list {
            let peaks = self.engine.waveform(&profile.press, 48);
            let press = self.engine.sample_duration_ms(&profile.press);
            let release = profile
                .release
                .as_ref()
                .map(|src| self.engine.sample_duration_ms(src))
                .unwrap_or(0.0);
            rows.push((profile, peaks, press, release));
        }

        ui.horizontal(|ui| {
            widgets::section_label(ui, &pal, "Installed profiles");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                widgets::label(
                    ui,
                    &format!(
                        "{} built-in / {} custom",
                        self.builtin_count(),
                        rows.len() - self.builtin_count()
                    ),
                    theme::mono(10.5),
                    pal.text_faint,
                );
            });
        });
        ui.add_space(8.0);

        let width = ui.available_width();
        for (profile, peaks, press, release) in &rows {
            let active = profile.id == active_id;
            let confirming = self.confirm_delete.as_deref() == Some(profile.id.as_str());
            let kind_color = pal.kind_color(profile.kind);
            let style = CardStyle {
                selected: active,
                interactive: true,
                spotlight: Some(kind_color),
            };
            let (response, _) = widgets::card(ui, &pal, vec2(width, 76.0), style, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        vec2(226.0, 44.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.horizontal(|ui| {
                                widgets::label(ui, &profile.name, theme::medium(13.0), pal.text);
                                if !profile.builtin {
                                    widgets::badge(ui, &pal, "Custom", pal.text_faint);
                                }
                            });
                            ui.add(
                                Label::new(
                                    RichText::new(format!(
                                        "{} / {} / press {:.0} ms / up {:.0} ms",
                                        profile.kind.label(),
                                        profile.author,
                                        press,
                                        release
                                    ))
                                    .font(theme::font(10.5))
                                    .color(pal.text_faint),
                                )
                                .truncate(),
                            );
                        },
                    );
                    let wave_w = (ui.available_width() - 260.0).max(80.0);
                    widgets::waveform(ui, &pal, peaks, wave_w, 34.0, kind_color);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if confirming {
                            if widgets::button(ui, &pal, "Remove", ButtonKind::Danger).clicked() {
                                *act = Some(Action::ConfirmDelete);
                            }
                            if widgets::button(ui, &pal, "Keep", ButtonKind::Ghost).clicked() {
                                *act = Some(Action::CancelDelete);
                            }
                        } else {
                            if !profile.builtin
                                && widgets::icon_button(ui, &pal, Icon::Trash, false)
                                    .on_hover_text("Delete this pack")
                                    .clicked()
                            {
                                *act = Some(Action::Delete(profile.id.clone()));
                            }
                            if active {
                                widgets::status_pill(ui, &pal, "In use", pal.success);
                            } else if widgets::button(ui, &pal, "Use", ButtonKind::Secondary)
                                .clicked()
                            {
                                *act = Some(Action::Select(profile.id.clone()));
                            }
                            if widgets::icon_button(ui, &pal, Icon::Play, false)
                                .on_hover_text("Preview")
                                .clicked()
                            {
                                *act = Some(Action::PreviewProfile(profile.id.clone()));
                            }
                        }
                    });
                });
            });
            if response.clicked() && !active {
                *act = Some(Action::Select(profile.id.clone()));
            }
            ui.add_space(8.0);
        }

        ui.add_space(6.0);
        widgets::mount_card(ui, &pal, 96.0, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    widgets::label(ui, "Where do packs live?", theme::medium(12.5), pal.text);
                    ui.add_space(2.0);
                    ui.add(
                        Label::new(
                            RichText::new(
                                "Anything you import or install is written to your sound library. \
                                 Import two files at once to pair a key-down with a key-up sample.",
                            )
                            .font(theme::font(11.0))
                            .color(pal.text_muted),
                        )
                        .wrap(),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::button(ui, &pal, "Open library", ButtonKind::Secondary).clicked() {
                        *act = Some(Action::OpenFolder);
                    }
                });
            });
        });
        ui.add_space(6.0);
        widgets::label(
            ui,
            &config::sounds_dir().display().to_string(),
            theme::mono(10.0),
            pal.text_faint,
        );
    }

    fn builtin_count(&self) -> usize {
        self.profiles.iter().filter(|p| p.builtin).count()
    }
}

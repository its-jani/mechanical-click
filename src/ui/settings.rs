//! Settings: audio engine, behaviour flags, Windows integration and about.

use egui::{Align, Label, Layout, RichText, Ui};

use super::page_header;
use super::theme::{self, Palette};
use super::widgets::{self, ButtonKind};
use super::{Action, App, Nav};
use crate::config::{self, Theme};
use crate::platform;

impl App {
    pub(super) fn view_settings(&mut self, ui: &mut Ui, act: &mut Option<Action>) {
        let pal = self.pal;
        let dark = matches!(self.cfg.theme, Theme::Dark);
        let ready = self.engine.status.ready();
        let device = self.engine.status.device.clone();
        let host = self.engine.status.host.clone();
        let error = self.engine.status.error.clone();
        let sample_rate = self.engine.status.sample_rate;
        let channels = self.engine.status.channels;
        let buffer = self.engine.status.buffer_frames;
        let latency = self.engine.status.latency_ms();

        page_header(ui, &pal, "Settings", Nav::Settings.subtitle(), |ui| {
            if widgets::button(
                ui,
                &pal,
                if dark { "Light mode" } else { "Dark mode" },
                ButtonKind::Secondary,
            )
            .clicked()
            {
                *act = Some(Action::ToggleTheme);
            }
        });

        // -- Audio ----------------------------------------------------------
        section(ui, &pal, "Audio engine");
        widgets::mount_card(ui, &pal, 190.0, |ui| {
            ui.spacing_mut().item_spacing.y = 7.0;
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    widgets::label(ui, "Output device", theme::medium(12.5), pal.text);
                    widgets::label(
                        ui,
                        &format!("{device}  /  {host}"),
                        theme::font(11.0),
                        pal.text_muted,
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::button(ui, &pal, "Reopen", ButtonKind::Secondary).clicked() {
                        *act = Some(Action::ReopenAudio);
                    }
                    if ready {
                        widgets::status_pill(ui, &pal, "Ready", pal.success);
                    } else {
                        widgets::status_pill(ui, &pal, "Offline", pal.danger);
                    }
                });
            });
            widgets::divider(ui, &pal);
            ui.horizontal(|ui| {
                widgets::stat_chip(
                    ui,
                    &pal,
                    "rate",
                    &format!("{} kHz", sample_rate / 1000),
                    pal.accent,
                );
                widgets::stat_chip(ui, &pal, "channels", &channels.to_string(), pal.text_muted);
                widgets::stat_chip(
                    ui,
                    &pal,
                    "buffer",
                    &format!("{buffer} frames"),
                    pal.text_muted,
                );
                widgets::stat_chip(
                    ui,
                    &pal,
                    "latency",
                    &format!("{latency:.1} ms"),
                    pal.success,
                );
            });
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    widgets::label(ui, "Buffer size", theme::medium(12.5), pal.text);
                    widgets::label(
                        ui,
                        "Lower is snappier. The driver may round up.",
                        theme::font(10.5),
                        pal.text_muted,
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    widgets::label(
                        ui,
                        &format!("{} ms", self.cfg.latency_ms),
                        theme::mono(11.0),
                        pal.accent,
                    );
                    let mut value = self.cfg.latency_ms as f32;
                    let response = widgets::slider(ui, &pal, &mut value, 4.0..=40.0, 150.0);
                    let rounded = value.round() as u32;
                    if rounded != self.cfg.latency_ms && !response.dragged() {
                        self.cfg.latency_ms = rounded;
                        self.engine.set_latency(rounded);
                        self.touch();
                    }
                });
            });
            if let Some(err) = &error {
                ui.add(
                    Label::new(RichText::new(err).font(theme::font(10.5)).color(pal.danger))
                        .truncate(),
                );
            }
        });

        // -- Behaviour ------------------------------------------------------
        ui.add_space(16.0);
        section(ui, &pal, "Behaviour");
        widgets::mount_card(ui, &pal, 232.0, |ui| {
            widgets::setting_row(
                ui,
                &pal,
                "Keyboard sounds",
                "Master switch. When off the hook stays installed but silent.",
                |ui| {
                    if widgets::toggle(ui, &pal, &mut self.cfg.enabled).changed() {
                        self.touch();
                    }
                },
            );
            widgets::divider(ui, &pal);
            widgets::setting_row(
                ui,
                &pal,
                "Key-up sample",
                "Play a second, softer sample when a key is released.",
                |ui| {
                    if widgets::toggle(ui, &pal, &mut self.cfg.key_release_sound).changed() {
                        self.touch();
                    }
                },
            );
            widgets::divider(ui, &pal);
            widgets::setting_row(
                ui,
                &pal,
                "Repeat while held",
                "Off by default: holding a key clicks once, not on every auto-repeat.",
                |ui| {
                    if widgets::toggle(ui, &pal, &mut self.cfg.allow_repeat).changed() {
                        self.touch();
                    }
                },
            );
            widgets::divider(ui, &pal);
            widgets::setting_row(
                ui,
                &pal,
                "Include synthetic keys",
                "Also play for injected input from macro tools and remote desktop.",
                |ui| {
                    if widgets::toggle(ui, &pal, &mut self.cfg.capture_injected).changed() {
                        self.touch();
                    }
                },
            );
        });

        // -- Windows --------------------------------------------------------
        ui.add_space(16.0);
        section(ui, &pal, "Windows");
        widgets::mount_card(ui, &pal, 148.0, |ui| {
            widgets::setting_row(
                ui,
                &pal,
                "Start with Windows",
                "Adds a HKCU Run entry that launches the app in the background at sign-in.",
                |ui| {
                    let mut on = self.cfg.start_with_windows;
                    if widgets::toggle(ui, &pal, &mut on).changed() {
                        match platform::set_autostart(on) {
                            Ok(()) => {
                                self.cfg.start_with_windows = on;
                                self.touch();
                                self.toast(
                                    if on {
                                        "Start-up enabled"
                                    } else {
                                        "Start-up disabled"
                                    },
                                    Some("Registered under HKCU\\...\\CurrentVersion\\Run".into()),
                                    self.pal.accent,
                                );
                            }
                            Err(err) => {
                                self.toast("Cannot update start-up", Some(err), self.pal.danger)
                            }
                        }
                    }
                },
            );
            widgets::divider(ui, &pal);
            widgets::setting_row(
                ui,
                &pal,
                "Close to tray",
                "Closing the window keeps the hook running. Use the tray menu to exit.",
                |ui| {
                    if widgets::toggle(ui, &pal, &mut self.cfg.close_to_tray).changed() {
                        self.touch();
                    }
                },
            );
        });

        // -- Storage --------------------------------------------------------
        ui.add_space(16.0);
        section(ui, &pal, "Storage");
        widgets::mount_card(ui, &pal, 132.0, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    widgets::label(ui, "Sound library", theme::medium(12.5), pal.text);
                    widgets::label(
                        ui,
                        &config::sounds_dir().display().to_string(),
                        theme::mono(10.0),
                        pal.text_muted,
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::button(ui, &pal, "Open", ButtonKind::Secondary).clicked() {
                        *act = Some(Action::OpenFolder);
                    }
                    if widgets::button(ui, &pal, "Import", ButtonKind::Ghost).clicked() {
                        *act = Some(Action::Import);
                    }
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                widgets::label(ui, "Settings", theme::font(11.0), pal.text_faint);
                widgets::label(
                    ui,
                    &config::config_path().display().to_string(),
                    theme::mono(10.0),
                    pal.text_faint,
                );
            });
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                widgets::label(ui, "Manifest", theme::font(11.0), pal.text_faint);
                widgets::label(
                    ui,
                    if self.cfg.manifest_url.is_empty() {
                        "not configured"
                    } else {
                        &self.cfg.manifest_url
                    },
                    theme::mono(10.0),
                    pal.text_faint,
                );
            });
        });

        // -- About ----------------------------------------------------------
        ui.add_space(16.0);
        section(ui, &pal, "About");
        widgets::mount_card(ui, &pal, 118.0, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    widgets::label(
                        ui,
                        &format!("Mechanical Click v{}", config::VERSION),
                        theme::medium(12.5),
                        pal.text,
                    );
                    widgets::label(
                        ui,
                        "Rust, egui and cpal. The keyboard hook lives on its own thread.",
                        theme::font(10.5),
                        pal.text_muted,
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::button(ui, &pal, "Exit", ButtonKind::Danger).clicked() {
                        *act = Some(Action::Exit);
                    }
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let (text, color) = if self.hook.is_some() {
                    ("Hook: listening system-wide", pal.success)
                } else {
                    ("Hook: not installed", pal.danger)
                };
                widgets::label(ui, text, theme::mono(10.0), color);
                widgets::label(
                    ui,
                    &format!("Sample cache: {} profiles", self.profiles.len()),
                    theme::mono(10.0),
                    pal.text_faint,
                );
            });
        });
    }
}

fn section(ui: &mut Ui, pal: &Palette, title: &str) {
    widgets::section_label(ui, pal, title);
    ui.add_space(6.0);
}

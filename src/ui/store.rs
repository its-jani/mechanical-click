//! Sound store: manifest entry point, featured pack and the pack grid.

use egui::{
    Align, CornerRadius, Label, Layout, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2,
};

use super::page_header;
use super::theme::{self, Palette, alpha, mix};
use super::widgets::{self, CardStyle, Icon};
use super::{Action, App, Nav};
use crate::store::StorePack;

impl App {
    pub(super) fn view_store(&mut self, ui: &mut Ui, act: &mut Option<Action>) {
        let pal = self.pal;
        let installed = self.installed.clone();
        let downloads = self.downloads.clone();
        let installing = self.installing.clone();

        page_header(ui, &pal, "Sound Store", Nav::Store.subtitle(), |ui| {
            if widgets::button(ui, &pal, "Library", widgets::ButtonKind::Secondary).clicked() {
                *act = Some(Action::Nav(Nav::Library));
            }
            if self.store.is_fetching() {
                widgets::status_pill(ui, &pal, "Fetching", pal.warn);
            }
        });

        widgets::mount_card(ui, &pal, 84.0, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    widgets::section_label(ui, &pal, "Manifest URL");
                    ui.add_space(3.0);
                    let width = (ui.available_width() - 176.0).max(180.0);
                    let field = egui::TextEdit::singleline(&mut self.url_input)
                        .hint_text("https://host/packs/manifest.json  or  C:\\packs\\manifest.json")
                        .desired_width(width)
                        .font(theme::font(12.0));
                    ui.add(field);
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::button(ui, &pal, "Load", widgets::ButtonKind::Primary).clicked() {
                        *act = Some(Action::SaveManifest);
                    }
                    if widgets::button(ui, &pal, "Refresh", widgets::ButtonKind::Ghost).clicked() {
                        *act = Some(Action::FetchManifest);
                    }
                });
            });
        });

        if let Some((note, is_error)) = &self.store_note {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                widgets::dot(
                    ui.painter(),
                    pos2(ui.cursor().min.x + 4.0, ui.cursor().min.y + 8.0),
                    3.0,
                    if *is_error { pal.danger } else { pal.success },
                );
                ui.add_space(12.0);
                ui.add(
                    Label::new(
                        RichText::new(note)
                            .font(theme::font(11.5))
                            .color(if *is_error {
                                pal.danger
                            } else {
                                pal.text_muted
                            }),
                    )
                    .truncate(),
                );
            });
        }

        ui.add_space(16.0);
        let featured = self
            .packs
            .iter()
            .find(|p| p.featured)
            .or_else(|| self.packs.first())
            .cloned();
        if let Some(pack) = featured {
            let progress = downloads.get(&pack.id).copied();
            featured_card(
                ui,
                &pal,
                &pack,
                installed.contains(&pack.id),
                progress,
                installing.contains(&pack.id),
                act,
            );
        }

        ui.add_space(20.0);
        ui.horizontal(|ui| {
            widgets::section_label(ui, &pal, "All packs");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                widgets::label(
                    ui,
                    &format!("{} available", self.packs.len()),
                    theme::mono(10.5),
                    pal.text_faint,
                );
            });
        });
        ui.add_space(8.0);

        let packs = self.packs.clone();
        let gap = 12.0;
        let cols = 3;
        let card_w = ((ui.available_width() - gap * (cols - 1) as f32) / cols as f32).max(170.0);
        let mut index = 0;
        while index < packs.len() {
            ui.horizontal(|ui| {
                for _ in 0..cols {
                    if index >= packs.len() {
                        break;
                    }
                    let pack = &packs[index];
                    index += 1;
                    pack_card(
                        ui,
                        &pal,
                        pack,
                        card_w,
                        installed.contains(&pack.id),
                        downloads.get(&pack.id).copied(),
                        installing.contains(&pack.id),
                        act,
                    );
                    ui.add_space(gap);
                }
            });
            ui.add_space(gap);
        }

        ui.add_space(8.0);
        widgets::mount_card(ui, &pal, 112.0, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    widgets::label(ui, "Publishing a pack", theme::medium(12.5), pal.text);
                    ui.add_space(2.0);
                    ui.add(
                        Label::new(
                            RichText::new(
                                "Host a manifest.json that lists each zip, then point the field above at it. \
                                 Zips may contain any number of .wav / .ogg files; names containing \"up\" or \"release\" become the key-up sample.",
                            )
                            .font(theme::font(11.0))
                            .color(pal.text_muted),
                        )
                        .wrap(),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::button(ui, &pal, "Copy example", widgets::ButtonKind::Secondary)
                        .clicked()
                    {
                        *act = Some(Action::CopyExample);
                    }
                });
            });
        });
    }
}

fn featured_card(
    ui: &mut Ui,
    pal: &Palette,
    pack: &StorePack,
    installed: bool,
    progress: Option<(u64, u64)>,
    installing: bool,
    act: &mut Option<Action>,
) {
    let kind_color = pal.kind_color(pack.kind);
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 176.0), Sense::hover());
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect_filled(rect, CornerRadius::same(15), pal.surface);
    widgets::gradient_rect(
        &painter,
        rect.shrink(1.0),
        14.0,
        mix(pal.surface, kind_color, 0.20),
        pal.surface,
    );
    widgets::glow(
        &painter,
        pos2(rect.right() - 90.0, rect.top() + 10.0),
        240.0,
        alpha(kind_color, 0.18),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(15),
        Stroke::new(1.0, mix(pal.border, kind_color, 0.40)),
        egui::StrokeKind::Inside,
    );

    let inner = rect.shrink2(vec2(20.0, 16.0));
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::top_down(Align::Min)),
    );
    child.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width((inner.width() - 220.0).max(220.0));
            ui.horizontal(|ui| {
                widgets::badge(ui, pal, "Featured", pal.warn);
                widgets::badge(ui, pal, pack.kind.label(), kind_color);
                if installed {
                    widgets::status_pill(ui, pal, "Installed", pal.success);
                }
            });
            ui.add_space(6.0);
            widgets::label(ui, &pack.name, theme::semibold(24.0), pal.text);
            ui.add_space(3.0);
            ui.add(
                Label::new(
                    RichText::new(&pack.description)
                        .font(theme::font(12.0))
                        .color(pal.text_muted),
                )
                .truncate(),
            );
            ui.add_space(6.0);
            widgets::label(
                ui,
                &format!(
                    "{}  /  v{}  /  {}",
                    pack.author,
                    if pack.version.is_empty() {
                        "1.0.0"
                    } else {
                        &pack.version
                    },
                    pack.size_label()
                ),
                theme::mono(10.5),
                pal.text_faint,
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            pack_actions(ui, pal, pack, installed, progress, installing, act);
        });
    });
}

fn pack_card(
    ui: &mut Ui,
    pal: &Palette,
    pack: &StorePack,
    width: f32,
    installed: bool,
    progress: Option<(u64, u64)>,
    installing: bool,
    act: &mut Option<Action>,
) {
    let kind_color = pal.kind_color(pack.kind);
    let style = CardStyle {
        selected: installed,
        interactive: false,
        spotlight: Some(kind_color),
    };
    widgets::card(ui, pal, vec2(width, 158.0), style, |ui| {
        ui.spacing_mut().item_spacing.y = 3.0;
        ui.horizontal(|ui| {
            let name_w = (ui.available_width() - 30.0).max(60.0);
            ui.allocate_ui_with_layout(
                vec2(name_w, 22.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    widgets::label(ui, &pack.name, theme::medium(13.0), pal.text);
                },
            );
            if installed {
                widgets::active_dot(ui, pal, true);
            }
        });
        ui.horizontal(|ui| {
            widgets::badge(ui, pal, pack.kind.label(), kind_color);
            widgets::label(ui, &pack.author, theme::font(10.5), pal.text_faint);
        });
        ui.add(
            Label::new(
                RichText::new(&pack.description)
                    .font(theme::font(11.0))
                    .color(pal.text_muted),
            )
            .truncate(),
        );
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            widgets::label(ui, &pack.size_label(), theme::mono(10.0), pal.text_faint);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                pack_actions(ui, pal, pack, installed, progress, installing, act);
            });
        });
    });
}

/// Install / preview controls shared by the featured card and the grid.
fn pack_actions(
    ui: &mut Ui,
    pal: &Palette,
    pack: &StorePack,
    installed: bool,
    progress: Option<(u64, u64)>,
    installing: bool,
    act: &mut Option<Action>,
) {
    if installed {
        widgets::status_pill(ui, pal, "Installed", pal.success);
        return;
    }
    if let Some((done, total)) = progress {
        let fraction = if total > 0 {
            done as f32 / total as f32
        } else {
            0.0
        };
        widgets::label(
            ui,
            &format!("{:.0}%", fraction * 100.0),
            theme::mono(10.5),
            pal.text_muted,
        );
        widgets::progress(ui, pal, fraction, 96.0, 6.0);
        return;
    }
    if installing {
        widgets::label(ui, "Installing...", theme::mono(10.5), pal.text_muted);
        return;
    }
    if widgets::button(ui, pal, "Install", widgets::ButtonKind::Primary).clicked() {
        *act = Some(Action::InstallPack(pack.id.clone()));
    }
    if matches!(pack.source, crate::store::PackSource::Bundled { .. })
        && widgets::icon_button(ui, pal, Icon::Play, false)
            .on_hover_text("Preview")
            .clicked()
    {
        *act = Some(Action::PreviewPack(pack.id.clone()));
    }
}

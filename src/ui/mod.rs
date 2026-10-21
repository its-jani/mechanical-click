//! Application shell: state, the sidebar + content layout, and the event pumps
//! that glue the hook, audio engine, store and tray together.
//!
//! Views never mutate state while painting. They collect an [`Action`] and the
//! shell applies it after the frame, which keeps borrows simple and makes every
//! state change go through one place.

mod home;
mod library;
mod settings;
mod store;
pub mod theme;
pub mod widgets;

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use egui::{
    Align, Align2, Color32, CornerRadius, Id, Layout, Margin, Rect, RichText, Sense, Stroke, Ui,
    UiBuilder, pos2, vec2,
};

use crate::audio::AudioEngine;
use crate::config::{self, Config, Profile, SampleSource, Theme};
use crate::hook::{self, HookHandle};
use crate::perf::Perf;
use crate::platform::SingleInstance;
use crate::store::{PackSource, Store, StoreEvent, StorePack};
use crate::tray::{Tray, TrayCmd};

use theme::{Palette, alpha, mix};
use widgets::Icon;

const SIDEBAR_WIDTH: f32 = 232.0;
const CONTENT_MAX: f32 = 920.0;
/// How long the keycap stays lit after a press, in seconds.
const PULSE: f32 = 0.28;

// ---------------------------------------------------------------------------
// Navigation & actions
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Home,
    Store,
    Library,
    Settings,
}

impl Nav {
    const ALL: [Nav; 4] = [Nav::Home, Nav::Store, Nav::Library, Nav::Settings];

    fn label(self) -> &'static str {
        match self {
            Nav::Home => "Home",
            Nav::Store => "Sound Store",
            Nav::Library => "Library",
            Nav::Settings => "Settings",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Nav::Home => Icon::Home,
            Nav::Store => Icon::Store,
            Nav::Library => Icon::Library,
            Nav::Settings => Icon::Settings,
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Nav::Home => "Pick the switch that follows your keyboard.",
            Nav::Store => "Download community packs straight into your library.",
            Nav::Library => "Every profile installed on this machine.",
            Nav::Settings => "Audio engine, behaviour and Windows integration.",
        }
    }
}

/// Everything a view can ask the shell to do.
pub enum Action {
    Nav(Nav),
    Select(String),
    PreviewProfile(String),
    PreviewPack(String),
    InstallPack(String),
    Delete(String),
    ConfirmDelete,
    CancelDelete,
    Import,
    Rescan,
    OpenFolder,
    ReopenAudio,
    FetchManifest,
    SaveManifest,
    ToggleTheme,
    ToggleEnabled,
    DismissHint,
    CopyExample,
    Exit,
}

pub struct Toast {
    pub title: String,
    pub detail: Option<String>,
    pub color: Color32,
    pub at: Instant,
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

pub struct App {
    cfg: Config,
    pal: Palette,
    engine: AudioEngine,
    hook: Option<HookHandle>,
    hook_error: Option<String>,
    profiles: Vec<Profile>,
    installed: HashSet<String>,
    nav: Nav,
    store: Store,
    packs: Vec<StorePack>,
    downloads: HashMap<String, (u64, u64)>,
    installing: HashSet<String>,
    waveform: Vec<f32>,
    press_ms: f32,
    release_ms: f32,
    key_label: String,
    key_down: bool,
    last_press: Instant,
    key_count: u32,
    rate: Vec<Instant>,
    toasts: Vec<Toast>,
    url_input: String,
    store_note: Option<(String, bool)>,
    perf: Perf,
    tray: Option<Tray>,
    tray_error: Option<String>,
    confirm_delete: Option<String>,
    dirty: bool,
    last_edit: Instant,
    quitting: bool,
    hidden: bool,
    hide_sent: bool,
    icon: Option<egui::TextureHandle>,
    _instance: Option<SingleInstance>,
    last_volume: f32,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        cfg: Config,
        start_hidden: bool,
        instance: Option<SingleInstance>,
    ) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let mut cfg = cfg;
        let pal = Palette::for_theme(cfg.theme);
        theme::apply(&cc.egui_ctx, &pal);
        let _ = config::ensure_dirs();

        let mut engine = AudioEngine::new(cfg.latency_ms);
        engine.set_gain(cfg.volume);

        let mut profiles = config::builtin_profiles();
        profiles.extend(config::scan_library());
        if !profiles.iter().any(|p| p.id == cfg.profile_id) {
            if let Some(first) = profiles.first() {
                cfg.profile_id = first.id.clone();
            }
        }
        cfg.start_with_windows = crate::platform::autostart_enabled();
        let installed: HashSet<String> = profiles
            .iter()
            .filter(|p| !p.builtin)
            .map(|p| p.id.clone())
            .collect();

        let mut waveform = vec![0.0; 64];
        let mut press_ms = 0.0;
        let mut release_ms = 0.0;
        if let Some(active) = profiles.iter().find(|p| p.id == cfg.profile_id) {
            if engine.apply_profile(active).is_ok() {
                waveform = engine.waveform(&active.press, 64);
                press_ms = engine.sample_duration_ms(&active.press);
                release_ms = active
                    .release
                    .as_ref()
                    .map(|src| engine.sample_duration_ms(src))
                    .unwrap_or(0.0);
            }
        }

        let (hook, hook_error) = match hook::spawn(engine.sender()) {
            Ok(handle) => {
                handle.set_enabled(cfg.enabled);
                handle.set_release_sound(cfg.key_release_sound);
                handle.set_allow_repeat(cfg.allow_repeat);
                handle.set_capture_injected(cfg.capture_injected);
                let ctx = cc.egui_ctx.clone();
                handle.set_wake(move || ctx.request_repaint());
                (Some(handle), None)
            }
            Err(err) => (None, Some(err)),
        };

        let icon = cc.egui_ctx.load_texture(
            "mc-app-icon",
            egui::ColorImage::from_rgba_unmultiplied(
                [64, 64],
                include_bytes!("../../assets/icons/app64.rgba"),
            ),
            egui::TextureOptions::LINEAR,
        );

        let mut store = Store::new();
        if !cfg.manifest_url.trim().is_empty() {
            store.fetch_manifest(cfg.manifest_url.clone());
        }

        let (tray, tray_error) = match Tray::new(&cfg, &profiles, {
            let ctx = cc.egui_ctx.clone();
            move || ctx.request_repaint()
        }) {
            Ok(tray) => (Some(tray), None),
            Err(err) => (None, Some(err)),
        };

        let mut app = Self {
            url_input: cfg.manifest_url.clone(),
            cfg,
            pal,
            engine,
            hook,
            hook_error,
            profiles,
            installed,
            nav: Nav::Home,
            store,
            packs: crate::store::bundled_catalog(),
            downloads: HashMap::new(),
            installing: HashSet::new(),
            waveform,
            press_ms,
            release_ms,
            key_label: "A".into(),
            key_down: false,
            last_press: Instant::now() - Duration::from_secs(5),
            key_count: 0,
            rate: Vec::new(),
            toasts: Vec::new(),
            store_note: None,
            perf: Perf::new(),
            tray,
            tray_error,
            confirm_delete: None,
            dirty: false,
            last_edit: Instant::now(),
            quitting: false,
            hidden: start_hidden,
            hide_sent: false,
            icon: Some(icon),
            _instance: instance,
            last_volume: 0.0,
        };
        app.last_volume = app.cfg.volume;
        if let Some(err) = app.hook_error.clone() {
            app.toast("Keyboard hook unavailable", Some(err), app.pal.danger);
        }
        if let Some(err) = app.tray_error.clone() {
            app.toast("Tray icon unavailable", Some(err), app.pal.warn);
        }
        if !app.engine.status.ready() {
            let detail = app.engine.status.error.clone();
            app.toast("Audio device problem", detail, app.pal.danger);
        }
        app
    }

    // -- state helpers ------------------------------------------------------

    pub(super) fn active(&self) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == self.cfg.profile_id)
    }

    pub(super) fn toast(
        &mut self,
        title: impl Into<String>,
        detail: Option<String>,
        color: Color32,
    ) {
        self.toasts.push(Toast {
            title: title.into(),
            detail,
            color,
            at: Instant::now(),
        });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    fn touch(&mut self) {
        self.dirty = true;
        self.last_edit = Instant::now();
    }

    fn pulse(&self) -> f32 {
        let t = self.last_press.elapsed().as_secs_f32();
        (1.0 - t / PULSE).clamp(0.0, 1.0)
    }

    fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    fn select_profile(&mut self, id: &str) {
        let Some(profile) = self.profile(id).cloned() else {
            return;
        };
        match self.engine.apply_profile(&profile) {
            Ok(()) => {
                self.cfg.profile_id = profile.id.clone();
                self.waveform = self.engine.waveform(&profile.press, 64);
                self.press_ms = self.engine.sample_duration_ms(&profile.press);
                self.release_ms = profile
                    .release
                    .as_ref()
                    .map(|src| self.engine.sample_duration_ms(src))
                    .unwrap_or(0.0);
                self.key_count = 0;
                self.toast(
                    format!("{} is active", profile.name),
                    Some("Every key press now uses this sample.".into()),
                    self.pal.accent,
                );
                self.touch();
            }
            Err(err) => {
                self.toast("Cannot load that sample", Some(err), self.pal.danger);
            }
        }
    }

    fn refresh_library(&mut self) {
        let mut profiles = config::builtin_profiles();
        profiles.extend(config::scan_library());
        if !profiles.iter().any(|p| p.id == self.cfg.profile_id) {
            if let Some(first) = profiles.first() {
                self.cfg.profile_id = first.id.clone();
            }
        }
        self.profiles = profiles;
        self.installed = self
            .profiles
            .iter()
            .filter(|p| !p.builtin)
            .map(|p| p.id.clone())
            .collect();
    }

    fn import_files(&mut self) {
        let Some(files) = rfd::FileDialog::new()
            .set_title("Add switch samples")
            .add_filter("Audio", &["wav", "ogg"])
            .set_directory(config::sounds_dir())
            .pick_files()
        else {
            return;
        };
        let mut chosen = files.into_iter();
        let Some(press_path) = chosen.next() else {
            return;
        };
        let release_path = chosen.next();

        let read = |path: &std::path::Path| std::fs::read(path).map_err(|e| e.to_string());
        let press = match read(&press_path) {
            Ok(bytes) => bytes,
            Err(err) => {
                self.toast("Cannot read file", Some(err), self.pal.danger);
                return;
            }
        };
        let press_ext = press_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("wav")
            .to_string();

        let stem = press_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "custom-switch".into());
        let name = config::pretty_name(&stem);
        let mut id = config::slug(&stem);
        if config::is_installed(&id) {
            let mut n = 2;
            loop {
                let candidate = format!("{}-{n}", config::slug(&stem));
                if !config::is_installed(&candidate) {
                    id = candidate;
                    break;
                }
                n += 1;
            }
        }

        let release: Option<(Vec<u8>, String)> = match release_path {
            Some(path) => match std::fs::read(&path) {
                Ok(bytes) => {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("wav")
                        .to_string();
                    Some((bytes, ext))
                }
                Err(err) => {
                    self.toast(
                        "Cannot read release file",
                        Some(err.to_string()),
                        self.pal.warn,
                    );
                    None
                }
            },
            None => None,
        };

        let release_ref = release
            .as_ref()
            .map(|(bytes, ext)| (bytes.as_slice(), ext.as_str()));
        match config::write_pack_ext(
            &id,
            &name,
            config::SwitchKind::Custom,
            "Local file",
            "Imported from this PC.",
            (&press, &press_ext),
            release_ref,
        ) {
            Ok(_) => {
                self.refresh_library();
                self.select_profile(&id);
                self.toast(
                    format!("Imported {name}"),
                    Some("Stored in your sound library.".into()),
                    self.pal.success,
                );
            }
            Err(err) => self.toast("Import failed", Some(err), self.pal.danger),
        }
    }

    // -- event pumps --------------------------------------------------------

    fn pump_hook(&mut self, ctx: &egui::Context) {
        let mut events = Vec::new();
        if let Some(hook) = &self.hook {
            while let Ok(event) = hook.events.try_recv() {
                events.push(event);
            }
        }
        if events.is_empty() {
            return;
        }
        let now = Instant::now();
        for event in events {
            if event.down {
                self.key_label = hook::key_label(event.vk);
                self.key_down = true;
                self.last_press = now;
                self.key_count = self.key_count.wrapping_add(1);
                self.rate.push(now);
            } else {
                self.key_down = false;
            }
        }
        self.rate
            .retain(|at| now.duration_since(*at) < Duration::from_secs(1));
        // Only wake the renderer when someone can actually see the keycap pulse;
        // a global hook fires on every keystroke of every app, and repainting
        // for each one while unfocused burns CPU for nothing.
        if ctx.input(|i| i.viewport().focused.unwrap_or(false)) {
            ctx.request_repaint();
        }
    }

    fn pump_tray(&mut self, ctx: &egui::Context) {
        let Some(tray) = &self.tray else {
            return;
        };
        let commands = tray.poll();
        for command in commands {
            match command {
                TrayCmd::Toggle => {
                    self.cfg.enabled = !self.cfg.enabled;
                    self.touch();
                }
                TrayCmd::Select(id) => self.select_profile(&id),
                TrayCmd::Volume(value) => {
                    self.cfg.volume = value.clamp(0.0, 2.0);
                    self.touch();
                }
                TrayCmd::Show => self.hidden = false,
                TrayCmd::Exit => {
                    self.quitting = true;
                    let _ = self.cfg.save();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            ctx.request_repaint();
        }
    }

    fn pump_store(&mut self, ctx: &egui::Context) {
        if let Some(result) = self.store.poll_manifest() {
            match result {
                Ok(found) => {
                    let mut merged = crate::store::bundled_catalog();
                    for pack in found {
                        if !merged.iter().any(|p| p.id == pack.id) {
                            merged.push(pack);
                        }
                    }
                    self.packs = merged;
                    self.store_note =
                        Some((format!("{} packs available", self.packs.len()), false));
                }
                Err(err) => self.store_note = Some((err, true)),
            }
        }
        for event in self.store.drain() {
            match event {
                StoreEvent::Progress { id, done, total } => {
                    self.downloads.insert(id, (done, total));
                }
                StoreEvent::Installed { id, name } => {
                    self.downloads.remove(&id);
                    self.installing.remove(&id);
                    self.refresh_library();
                    self.toast(
                        format!("{name} installed"),
                        Some("It is waiting in your library.".into()),
                        self.pal.success,
                    );
                }
                StoreEvent::Failed { id, error } => {
                    self.downloads.remove(&id);
                    self.installing.remove(&id);
                    self.toast("Install failed", Some(error), self.pal.danger);
                }
            }
            ctx.request_repaint();
        }
    }

    /// Applies cheap runtime state (gain, hook flags, theme) and debounce-saves.
    fn sync_runtime(&mut self, ctx: &egui::Context, focused: bool) {
        if (self.cfg.volume - self.last_volume).abs() > 0.0005 {
            self.engine.set_gain(self.cfg.volume);
            self.last_volume = self.cfg.volume;
            self.touch();
        }
        if let Some(hook) = &self.hook {
            hook.set_enabled(self.cfg.enabled);
            hook.set_release_sound(self.cfg.key_release_sound);
            hook.set_allow_repeat(self.cfg.allow_repeat);
            hook.set_capture_injected(self.cfg.capture_injected);
            hook.set_focused(focused);
        }
        let want_dark = matches!(self.cfg.theme, Theme::Dark);
        if want_dark != self.pal.dark {
            self.pal = Palette::for_theme(self.cfg.theme);
            theme::apply(ctx, &self.pal);
        }
        if self.dirty && self.last_edit.elapsed() > Duration::from_millis(500) {
            if self.cfg.save().is_ok() {
                self.dirty = false;
            }
        }
    }

    fn sync_window(&mut self, ctx: &egui::Context) {
        if self.hidden != self.hide_sent {
            if self.hidden {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            self.hide_sent = self.hidden;
        }
    }

    // -- actions ------------------------------------------------------------

    fn apply(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::Nav(nav) => {
                self.nav = nav;
                self.confirm_delete = None;
            }
            Action::Select(id) => self.select_profile(&id),
            Action::PreviewProfile(id) => {
                if let Some(source) = self.profile(&id).map(|p| p.press.clone()) {
                    if let Err(err) = self.engine.preview(&source) {
                        self.toast("Cannot preview", Some(err), self.pal.danger);
                    }
                }
            }
            Action::PreviewPack(id) => {
                let press = self.packs.iter().find(|p| p.id == id).and_then(|p| {
                    if let PackSource::Bundled { press, .. } = &p.source {
                        Some(*press)
                    } else {
                        None
                    }
                });
                if let Some(bytes) = press {
                    let source = SampleSource::Embedded(bytes, "wav");
                    if let Err(err) = self.engine.preview(&source) {
                        self.toast("Cannot preview", Some(err), self.pal.danger);
                    }
                }
            }
            Action::InstallPack(id) => {
                if let Some(pack) = self.packs.iter().find(|p| p.id == id).cloned() {
                    self.installing.insert(pack.id.clone());
                    self.downloads.remove(&pack.id);
                    self.store.install(pack);
                }
            }
            Action::Delete(id) => self.confirm_delete = Some(id),
            Action::CancelDelete => self.confirm_delete = None,
            Action::ConfirmDelete => {
                if let Some(id) = self.confirm_delete.take() {
                    if let Some(profile) = self.profile(&id).cloned() {
                        let was_active = self.cfg.profile_id == profile.id;
                        match config::delete_pack(&profile) {
                            Ok(()) => {
                                if was_active {
                                    self.cfg.profile_id.clear();
                                }
                                self.refresh_library();
                                if was_active {
                                    let next = self.cfg.profile_id.clone();
                                    self.select_profile(&next);
                                }
                                self.toast(
                                    format!("{} removed", profile.name),
                                    Some("The files were deleted from your library.".into()),
                                    self.pal.warn,
                                );
                                self.touch();
                            }
                            Err(err) => {
                                self.toast("Cannot delete pack", Some(err), self.pal.danger)
                            }
                        }
                    }
                }
            }
            Action::Import => self.import_files(),
            Action::Rescan => {
                self.refresh_library();
                let count = self.profiles.len();
                self.toast(
                    "Library refreshed",
                    Some(format!("{count} profiles available")),
                    self.pal.accent,
                );
            }
            Action::OpenFolder => {
                let dir = config::sounds_dir();
                let _ = std::process::Command::new("explorer").arg(&dir).spawn();
            }
            Action::ReopenAudio => {
                self.engine.rebuild();
                if self.engine.status.ready() {
                    let name = self.engine.status.device.clone();
                    self.toast("Audio device reopened", Some(name), self.pal.success);
                } else {
                    let err = self.engine.status.error.clone();
                    self.toast("Still no audio device", err, self.pal.danger);
                }
            }
            Action::FetchManifest => {
                let url = self.cfg.manifest_url.trim().to_string();
                if url.is_empty() {
                    self.store_note = Some(("Add a manifest URL first.".into(), true));
                } else {
                    self.store.fetch_manifest(url);
                    self.store_note = Some(("Fetching manifest...".into(), false));
                }
            }
            Action::SaveManifest => {
                self.cfg.manifest_url = self.url_input.trim().to_string();
                self.cfg.store_url_editing = false;
                self.touch();
                self.apply(Action::FetchManifest, ctx);
            }
            Action::ToggleTheme => {
                self.cfg.theme = self.cfg.theme.toggled();
                self.touch();
            }
            Action::ToggleEnabled => {
                self.cfg.enabled = !self.cfg.enabled;
                self.touch();
            }
            Action::DismissHint => {
                self.cfg.dismissed_hint = true;
                self.touch();
            }
            Action::CopyExample => {
                ctx.copy_text(crate::store::EXAMPLE_MANIFEST.to_string());
                self.toast(
                    "Manifest example copied",
                    Some("Paste it next to your zip files.".into()),
                    self.pal.accent,
                );
            }
            Action::Exit => {
                self.quitting = true;
                let _ = self.cfg.save();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    // -- chrome -------------------------------------------------------------

    fn sidebar(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let active = self.active().cloned();
        let mut action: Option<Action> = None;
        let ctx = ui.ctx().clone();

        egui::Panel::left("mc-sidebar")
            .exact_size(SIDEBAR_WIDTH)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(pal.sidebar).inner_margin(Margin {
                left: 14,
                right: 14,
                top: 16,
                bottom: 14,
            }))
            .show(ui, |ui| {
                let full = ui.max_rect();
                ui.painter().line_segment(
                    [
                        pos2(full.right() + 13.0, full.top()),
                        pos2(full.right() + 13.0, full.bottom()),
                    ],
                    Stroke::new(1.0, pal.border_soft),
                );

                ui.horizontal(|ui| {
                    if let Some(icon) = &self.icon {
                        ui.add(
                            egui::Image::new(egui::load::SizedTexture::new(
                                icon.id(),
                                vec2(30.0, 30.0),
                            ))
                            .corner_radius(CornerRadius::same(9)),
                        );
                        ui.add_space(4.0);
                    }
                    ui.vertical(|ui| {
                        widgets::label(ui, "Mechanical Click", theme::semibold(13.0), pal.text);
                        widgets::label(
                            ui,
                            &format!("v{}", config::VERSION),
                            theme::mono(9.5),
                            pal.text_faint,
                        );
                    });
                });

                ui.add_space(20.0);
                widgets::section_label(ui, &pal, "Navigation");
                ui.add_space(6.0);
                for nav in Nav::ALL {
                    if nav_button(ui, &pal, nav.icon(), nav.label(), self.nav == nav).clicked() {
                        action = Some(Action::Nav(nav));
                    }
                    ui.add_space(2.0);
                }

                let footer = 206.0;
                ui.add_space((ui.available_height() - footer).max(10.0));
                self.sidebar_footer(ui, &pal, active, &mut action);
            });

        if let Some(action) = action {
            self.apply(action, &ctx);
        }
    }

    fn sidebar_footer(
        &self,
        ui: &mut Ui,
        pal: &Palette,
        active: Option<Profile>,
        action: &mut Option<Action>,
    ) {
        let kind_color = active
            .as_ref()
            .map(|p| pal.kind_color(p.kind))
            .unwrap_or(pal.accent);
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(vec2(width, 96.0), Sense::click());
        let hover = ui
            .ctx()
            .animate_bool_responsive(response.id, response.hovered());
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(11), alpha(pal.surface_hi, 0.55));
        painter.rect_stroke(
            rect,
            CornerRadius::same(11),
            Stroke::new(
                1.0,
                mix(pal.border_soft, alpha(pal.accent, 0.5), hover * 0.6),
            ),
            egui::StrokeKind::Inside,
        );
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect.shrink2(vec2(11.0, 9.0))));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        widgets::section_label(&mut child, pal, "Now playing");
        child.add_space(2.0);
        match &active {
            Some(profile) => {
                widgets::label(&mut child, &profile.name, theme::medium(12.5), pal.text);
                child.add_space(6.0);
                widgets::draw_waveform(
                    child.painter(),
                    Rect::from_min_size(child.cursor().min, vec2(child.available_width(), 40.0)),
                    &self.waveform,
                    kind_color,
                    pal,
                );
            }
            None => {
                widgets::label(
                    &mut child,
                    "No profile",
                    theme::medium(12.5),
                    pal.text_muted,
                );
            }
        }
        if response.clicked() {
            *action = Some(Action::Nav(Nav::Home));
        }

        ui.add_space(10.0);
        let (label, color) = if self.engine.status.ready() {
            if self.cfg.enabled {
                ("Listening", pal.success)
            } else {
                ("Muted", pal.text_muted)
            }
        } else {
            ("Audio offline", pal.danger)
        };
        widgets::status_pill(ui, pal, label, color);

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            widgets::label(ui, "Theme", theme::font(11.5), pal.text_muted);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::icon_button(ui, pal, Icon::Theme, false)
                    .on_hover_text("Toggle dark / light")
                    .clicked()
                {
                    *action = Some(Action::ToggleTheme);
                }
            });
        });
    }

    fn content(&mut self, ui: &mut Ui) {
        let full = ui.max_rect();
        let width = full.width().min(CONTENT_MAX);
        let rect = Rect::from_min_size(
            pos2(full.center().x - width * 0.5, full.top()),
            vec2(width, full.height()),
        );
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::top_down(Align::Min)),
        );
        let nav = self.nav;
        let mut action: Option<Action> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                ui.add_space(18.0);
                ui.horizontal(|ui| {
                    ui.add_space(22.0);
                    ui.vertical(|ui| {
                        ui.set_width((width - 44.0).max(240.0));
                        match nav {
                            Nav::Home => self.view_home(ui, &mut action),
                            Nav::Store => self.view_store(ui, &mut action),
                            Nav::Library => self.view_library(ui, &mut action),
                            Nav::Settings => self.view_settings(ui, &mut action),
                        }
                    });
                });
                ui.add_space(28.0);
            });
        if let Some(action) = action {
            self.apply(action, ui.ctx());
        }
    }

    fn draw_toasts(&mut self, ctx: &egui::Context) {
        self.toasts
            .retain(|toast| toast.at.elapsed() < Duration::from_secs(4));
        if self.toasts.is_empty() {
            return;
        }
        let pal = self.pal;
        let toasts = &self.toasts;
        egui::Area::new(Id::unique("mc-toasts"))
            .anchor(Align2::RIGHT_BOTTOM, vec2(-18.0, -18.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_width(296.0);
                ui.with_layout(Layout::bottom_up(Align::Max), |ui| {
                    for toast in toasts {
                        let age = toast.at.elapsed().as_secs_f32();
                        let fade = ((4.0 - age) / 0.5).clamp(0.0, 1.0);
                        egui::Frame::new()
                            .fill(mix(pal.surface, pal.surface_hi, 0.45))
                            .corner_radius(CornerRadius::same(11))
                            .stroke(Stroke::new(1.0, mix(pal.border, toast.color, 0.35)))
                            .inner_margin(Margin {
                                left: 10,
                                right: 12,
                                top: 9,
                                bottom: 9,
                            })
                            .show(ui, |ui| {
                                ui.set_width(270.0);
                                ui.horizontal(|ui| {
                                    let (bar, _) =
                                        ui.allocate_exact_size(vec2(3.0, 30.0), Sense::hover());
                                    ui.painter().rect_filled(
                                        bar,
                                        CornerRadius::same(2),
                                        alpha(toast.color, fade),
                                    );
                                    ui.add_space(7.0);
                                    ui.vertical(|ui| {
                                        ui.add(egui::Label::new(
                                            RichText::new(&toast.title)
                                                .font(theme::medium(12.0))
                                                .color(alpha(pal.text, fade)),
                                        ));
                                        if let Some(detail) = &toast.detail {
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(detail)
                                                        .font(theme::font(11.0))
                                                        .color(alpha(pal.text_muted, fade)),
                                                )
                                                .truncate(),
                                            );
                                        }
                                    });
                                });
                            });
                        ui.add_space(7.0);
                    }
                });
            });
    }
}

impl eframe::App for App {
    /// Runs even while the window is hidden, so the tray menu and the
    /// hook/audio pipeline stay responsive without an egui pass.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(false));
        self.perf.tick();
        self.pump_hook(ctx);
        self.pump_tray(ctx);
        self.pump_store(ctx);
        self.sync_runtime(ctx, focused);

        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting && self.cfg.close_to_tray
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hidden = true;
        }
        self.sync_window(ctx);

        if let Some(tray) = &mut self.tray {
            tray.sync(&self.cfg, &self.profiles);
        }

        // A hidden window gets no egui pass, so keep waking `logic` often enough
        // to service the tray menu. Tray clicks and focused key presses wake us
        // directly, so the periodic tick is only a safety net: it is slow enough
        // to disappear in the noise while idle, and quick only when there is
        // something pending (an unsaved setting, a running download).
        let busy = focused && Instant::now() < self.last_press + Duration::from_secs_f32(PULSE);
        let transferring = !self.downloads.is_empty() || self.store.is_fetching();
        let cadence = if self.hidden {
            if self.dirty {
                600
            } else if transferring {
                300
            } else {
                5_000
            }
        } else if busy {
            33
        } else if transferring || !self.toasts.is_empty() {
            90
        } else if focused {
            500
        } else {
            1500
        };
        ctx.request_repaint_after(Duration::from_millis(cadence));
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.sidebar(ui);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(self.pal.bg))
            .show(ui, |ui| self.content(ui));
        self.draw_toasts(&ctx);
    }
}

// ---------------------------------------------------------------------------
// Shared chrome
// ---------------------------------------------------------------------------

fn nav_button(ui: &mut Ui, pal: &Palette, icon: Icon, label: &str, active: bool) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 34.0), Sense::click());
    let hover = ui
        .ctx()
        .animate_bool_responsive(response.id, response.hovered());
    let painter = ui.painter();
    if active {
        painter.rect_filled(rect, CornerRadius::same(9), alpha(pal.accent, 0.13));
        painter.rect_filled(
            Rect::from_min_size(pos2(rect.left(), rect.center().y - 8.0), vec2(3.0, 16.0)),
            CornerRadius::same(2),
            pal.accent,
        );
    } else if hover > 0.01 {
        painter.rect_filled(
            rect,
            CornerRadius::same(9),
            alpha(pal.surface_hi, hover * 0.75),
        );
    }
    let text_color = if active {
        pal.text
    } else if response.hovered() {
        pal.text
    } else {
        pal.text_muted
    };
    widgets::draw_icon(
        painter,
        Rect::from_center_size(pos2(rect.left() + 20.0, rect.center().y), vec2(15.0, 15.0)),
        icon,
        if active { pal.accent } else { text_color },
        1.6,
    );
    painter.text(
        pos2(rect.left() + 36.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        theme::medium(12.5),
        text_color,
    );
    response
}

pub(super) fn page_header(
    ui: &mut Ui,
    pal: &Palette,
    title: &str,
    subtitle: &str,
    right: impl FnOnce(&mut Ui),
) {
    ui.horizontal(|ui| {
        let text_width = (ui.available_width() - 280.0).max(180.0);
        ui.allocate_ui_with_layout(vec2(text_width, 44.0), Layout::top_down(Align::Min), |ui| {
            widgets::label(ui, title, theme::semibold(23.0), pal.text);
            ui.add_space(1.0);
            widgets::label(ui, subtitle, theme::font(12.5), pal.text_muted);
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| right(ui));
    });
    ui.add_space(18.0);
}

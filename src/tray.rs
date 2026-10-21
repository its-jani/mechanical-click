//! Windows system tray: menu construction, cheap state sync and click routing.
//!
//! `muda` delivers clicks either through its global channel or through a
//! handler, never both. We install a handler so a click can wake the UI thread
//! immediately and forward the event into our own channel.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crossbeam_channel::{Receiver, unbounded};
use muda::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::config::{Config, Profile};

/// Volume shortcuts offered in the tray menu, in percent.
const VOLUME_STEPS: [u32; 6] = [25, 50, 75, 100, 150, 200];

pub enum TrayCmd {
    Toggle,
    Select(String),
    Volume(f32),
    Show,
    Exit,
}

pub struct Tray {
    icon: TrayIcon,
    fingerprint: u64,
    events: Receiver<MenuEvent>,
}

impl Tray {
    pub fn new(
        cfg: &Config,
        profiles: &[Profile],
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, String> {
        let icon = Icon::from_rgba(
            include_bytes!("../assets/icons/app64.rgba").to_vec(),
            64,
            64,
        )
        .map_err(|e| format!("tray icon: {e}"))?;
        let tray = TrayIconBuilder::new()
            .with_icon(icon)
            .with_menu(Box::new(build_menu(cfg, profiles)))
            .with_menu_on_left_click(false)
            .with_tooltip(tooltip(cfg, profiles))
            .build()
            .map_err(|e| format!("cannot create tray icon: {e}"))?;
        let (tx, events) = unbounded();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let _ = tx.send(event);
            wake();
        }));
        Ok(Self {
            icon: tray,
            fingerprint: fingerprint(cfg, profiles),
            events,
        })
    }

    /// Rebuilds the menu only when the state it displays actually changed.
    pub fn sync(&mut self, cfg: &Config, profiles: &[Profile]) {
        let new = fingerprint(cfg, profiles);
        if new == self.fingerprint {
            return;
        }
        self.fingerprint = new;
        self.icon
            .set_menu(Some(Box::new(build_menu(cfg, profiles))));
        let _ = self.icon.set_tooltip(Some(tooltip(cfg, profiles)));
    }

    /// Drains pending menu and icon events into simple commands.
    pub fn poll(&self) -> Vec<TrayCmd> {
        let mut commands = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            let id = event.id.0.as_str();
            if id == "toggle" {
                commands.push(TrayCmd::Toggle);
            } else if id == "show" {
                commands.push(TrayCmd::Show);
            } else if id == "quit" {
                commands.push(TrayCmd::Exit);
            } else if let Some(rest) = id.strip_prefix("profile:") {
                commands.push(TrayCmd::Select(rest.to_string()));
            } else if let Some(rest) = id.strip_prefix("vol:") {
                if let Ok(pct) = rest.parse::<f32>() {
                    commands.push(TrayCmd::Volume(pct / 100.0));
                }
            }
        }
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if matches!(event, TrayIconEvent::DoubleClick { .. }) {
                commands.push(TrayCmd::Show);
            }
        }
        commands
    }
}

fn tooltip(cfg: &Config, profiles: &[Profile]) -> String {
    let name = profiles
        .iter()
        .find(|p| p.id == cfg.profile_id)
        .map(|p| p.name.as_str())
        .unwrap_or("No profile");
    if cfg.enabled {
        format!("Mechanical Click - {name}")
    } else {
        "Mechanical Click - muted".to_string()
    }
}

fn fingerprint(cfg: &Config, profiles: &[Profile]) -> u64 {
    let mut hasher = DefaultHasher::new();
    cfg.enabled.hash(&mut hasher);
    cfg.profile_id.hash(&mut hasher);
    ((cfg.volume * 100.0).round() as u32).hash(&mut hasher);
    for profile in profiles {
        profile.id.hash(&mut hasher);
        profile.name.hash(&mut hasher);
    }
    hasher.finish()
}

fn build_menu(cfg: &Config, profiles: &[Profile]) -> Menu {
    let menu = Menu::new();
    let toggle = CheckMenuItem::with_id("toggle", "Keyboard sounds", true, cfg.enabled, None);
    let _ = menu.append(&toggle);
    let _ = menu.append(&PredefinedMenuItem::separator());

    let profile_menu = Submenu::with_id("profiles", "Switch profile", true);
    for profile in profiles.iter().take(24) {
        let item = CheckMenuItem::with_id(
            format!("profile:{}", profile.id),
            &profile.name,
            true,
            profile.id == cfg.profile_id,
            None,
        );
        let _ = profile_menu.append(&item);
    }
    let _ = menu.append(&profile_menu);

    let volume_menu = Submenu::with_id("volume", "Volume", true);
    let current = (cfg.volume * 100.0).round() as u32;
    for pct in VOLUME_STEPS {
        let item = CheckMenuItem::with_id(
            format!("vol:{pct}"),
            format!("{pct}% (currently {current}%)"),
            true,
            pct.abs_diff(current) < 3,
            None,
        );
        let _ = volume_menu.append(&item);
    }
    let _ = menu.append(&volume_menu);

    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::with_id(
        "show",
        "Open Mechanical Click",
        true,
        None,
    ));
    let _ = menu.append(&MenuItem::with_id("quit", "Exit", true, None));
    menu
}

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! `mechanical-click` - global mechanical keyboard sounds for Windows.
//!
//! Threads: (1) the egui/winit UI thread, (2) the `WH_KEYBOARD_LL` hook thread
//! with its own message pump, (3) the cpal output callback. The hook talks to
//! the audio engine over a bounded channel so a key press never waits for the
//! UI to repaint.

mod audio;
mod config;
mod hook;
mod perf;
mod platform;
mod smoke;
mod store;
mod tray;
mod ui;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let start_hidden = args
        .iter()
        .any(|arg| arg == "--background" || arg == "--tray" || arg == "--hidden");

    // Headless subsystem boot check: runs before the instance guard so CI can
    // verify a build while the app itself is running.
    if args.iter().any(|arg| arg == "--smoke-test") {
        std::process::exit(smoke::run());
    }

    // Second copy: leave the running instance alone and exit quietly.
    let Some(instance) = platform::SingleInstance::acquire() else {
        return Ok(());
    };

    let mut cfg = config::Config::load();
    cfg.start_with_windows = platform::autostart_enabled();
    let _ = config::ensure_dirs();

    let icon = egui::IconData {
        rgba: include_bytes!("../assets/icons/app64.rgba").to_vec(),
        width: 64,
        height: 64,
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(config::APP_TITLE)
            .with_inner_size([1080.0, 720.0])
            .with_min_inner_size([900.0, 600.0])
            .with_icon(icon),
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "mechanical-click",
        options,
        Box::new(move |cc| {
            let app = ui::App::new(cc, cfg, start_hidden, Some(instance));
            Ok(Box::new(app))
        }),
    )
}

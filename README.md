# Mechanical Click

**Global mechanical keyboard sounds for Windows.**

Mechanical Click is a tiny, tray-based utility that plays realistic mechanical
switch sounds on every keypress — system-wide, in every app. No drivers, no
admin rights, no bloat: one native `.exe` that idles at a few megabytes of RAM.

## Why

You love the sound of your old mechanical keyboard, but you type on a
laptop, or a silent membrane board, or a scissor-switch thing at work.
Mechanical Click gives every keystroke the click, thock or clack it deserves,
everywhere — browser, terminal, IDE, games.

## Features

- **System-wide** — a low-level `WH_KEYBOARD_LL` hook hears every key press and
  release in every application, including elevated ones.
- **Near-zero latency** — the audio callback never allocates; key events travel
  over bounded channels to a fixed voice pool, so sound follows the click.
- **Switch library** — Cherry MX Blue/Brown, Gateron Red, Holy Panda, Kailh Box
  Jade, Topre 45g, Alpine Winter, MX Speed Silver, Buckling Spring, Novelkeys
  Cream, and more — each with separate press and key-up samples.
- **Sound Store** — install bundled packs offline, or point the app at any
  `manifest.json` URL (or local folder) to download community packs as zips.
- **Per-app profiles, volume, pitch and rate controls**, mute-on-game, and
  start-with-Windows autostart.
- **Tray-first** — closes to the system tray; launch with `--background` to
  start hidden.
- **Lightweight** — single static-feeling exe built with `lto + strip`; the UI
  (egui) and telemetry run on the same handful of threads.

## Download

Grab the latest build from the
[Releases page](https://github.com/its-jani/mechanical-click/releases):

1. Download `mechanical-click.exe` from the latest release assets.
2. Run it — no installer, no dependencies.
3. Right-click the tray icon to mute, open settings, or quit.

To install an older or updated build, just replace the exe — your settings and
downloaded packs live in `%APPDATA%\mechanical-click\` and survive upgrades.

## Quick install (PowerShell)

Paste this **one line** into PowerShell (Windows PowerShell 5.1 or pwsh 7).
It downloads the latest release, unblocks it, installs it to
`%LOCALAPPDATA%\Programs\mechanical-click\` and starts it minimized to the tray:

```powershell
$d="$env:LOCALAPPDATA\Programs\mechanical-click"; New-Item -ItemType Directory -Force -Path $d | Out-Null; Invoke-WebRequest "https://github.com/its-jani/mechanical-click/releases/latest/download/mechanical-click.exe" -OutFile "$d\mechanical-click.exe"; Unblock-File "$d\mechanical-click.exe"; Start-Process "$d\mechanical-click.exe" -ArgumentList "--background"
```

Optional: create a Desktop shortcut too (run it right after the line above,
in the same session):

```powershell
$s = (New-Object -ComObject WScript.Shell).CreateShortcut("$([Environment]::GetFolderPath('Desktop'))\mechanical-click.lnk"); $s.TargetPath = "$d\mechanical-click.exe"; $s.WorkingDirectory = $d; $s.Arguments = "--background"; $s.IconLocation = "$d\mechanical-click.exe,0"; $s.Save()
```

To update later, simply re-run the one-liner — it overwrites the binary in
place. Settings and installed sound packs are kept in `%APPDATA%\mechanical-click\`.

### SmartScreen Warning

The binary is not code-signed (Authenticode certificates are expensive), so a
downloaded copy carries the "Mark of the Web" and Windows SmartScreen may show
*"Windows protected your PC — Unknown publisher"* on the first run. Two ways
past it:

- **Recommended:** install with the one-liner above — `Unblock-File` strips the
  Mark of the Web, so SmartScreen stays quiet.
- **Manual:** after downloading, right-click the file → Properties → check
  *Unblock*, or run:

  ```powershell
  Unblock-File -Path .\mechanical-click.exe
  ```

- If you already dismissed the dialog once, choose **More info → Run anyway** —
  the app is added to the SmartScreen allow list on that machine.

The app needs no admin rights: it installs a per-user low-level keyboard hook
and lives entirely in your profile.

## Building from source

Requires a recent stable Rust toolchain on Windows (MSVC or GNU):

```sh
cargo build --release
# binary: target/release/mechanical-click.exe
```

Developer notes:

- `cargo run` launches the UI; `cargo run -- --background` starts hidden in the tray.
- `cargo run -- --smoke-test` boots the real subsystems headlessly — low-level
  keyboard hook, cpal audio sink (plus decoding of every embedded store sample)
  and the tray icon — prints one line per check and exits. Works on the shipped
  exe too (`mechanical-click.exe --smoke-test`, attaches to the parent
  console). Exit codes: `0` pass, `1` failed check, `124` init watchdog fired.
- `cargo test` runs the unit tests (manifest parsing, relative URL resolution,
  and a guard that every bundled sample decodes).
- Synthesized demo assets and the app icon are generated offline by
  `rustc --edition 2021 -O tools/gen_assets.rs -o tools/gen_assets.exe`
  followed by `tools/gen_assets.exe assets`.

## Sound packs & the store

The store ships a **bundled catalogue** embedded in the executable (works with
no network), and supports **remote packs** through a manifest:

```json
{
  "packs": [
    {
      "id": "holy-panda",
      "name": "Holy Panda",
      "kind": "tactile",
      "author": "Community",
      "version": "1.2.0",
      "description": "Long-pole tactile bump.",
      "url": "https://example.com/packs/holy-panda.zip"
    }
  ]
}
```

Pack zips may contain any number of `.wav` / `.ogg` files; names containing
`up` or `release` become the key-up sample. Installed packs are extracted to
`%APPDATA%\mechanical-click\sounds\<pack>\` with a `pack.json` beside them.

## Architecture

```
src/
  main.rs        entry point, single-instance guard, eframe bootstrap
  hook/          WH_KEYBOARD_LL hook thread + message pump (never blocks input)
  audio/         cpal output engine, voice pool, wav/ogg decoding (hound/lewton)
  store/         bundled catalogue + remote manifest/zip install pipeline
  config/        settings, switch registry, on-disk library under %APPDATA%
  smoke.rs       headless --smoke-test subsystem boot check
  ui/            egui interface: home, library, store, settings, theme
  tray.rs        system tray menu and click routing (tray-icon + muda)
  perf.rs        CPU / working-set HUD sampled off the hot path
  platform.rs    single instance, autostart, window helpers
```

Three threads do all the work: the UI thread, the hook thread (its own message
pump), and the audio callback. They communicate through bounded crossbeam
channels and atomics — a keypress never waits on the UI, and the hook callback
allocates nothing.

## Credits

- Audio samples for the **SoundFX Keyboard Press** pack come from the SoundFX
  library; the **Mintty Keytyping** pack samples come from the
  [Mintty](https://mintty.github.io/) terminal distribution.
- Switch sounds not recorded from real hardware are synthesized by
  `tools/gen_assets.rs`.

## License

MIT — see `Cargo.toml`.

//! User settings, the built-in switch registry and the on-disk sound library.
//!
//! Everything the app needs to know about *which* sound to play lives here; the
//! audio module only ever sees decoded buffers.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const APP_NAME: &str = "mechanical-click";
pub const APP_TITLE: &str = "Mechanical Click";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const RUN_KEY: &str = "MechanicalClick";

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(APP_NAME)
}

pub fn sounds_dir() -> PathBuf {
    data_dir().join("sounds")
}

pub fn config_path() -> PathBuf {
    data_dir().join("config.json")
}

pub fn ensure_dirs() -> Result<(), String> {
    fs::create_dir_all(sounds_dir()).map_err(|e| format!("cannot create sound library: {e}"))
}

/// Filesystem-safe identifier derived from arbitrary display text.
pub fn slug(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if (ch.is_whitespace() || ch == '-' || ch == '_') && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() { "pack".into() } else { out }
}

// ---------------------------------------------------------------------------
// Switch metadata
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    pub fn toggled(self) -> Self {
        match self {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::Dark,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchKind {
    Clicky,
    Tactile,
    Linear,
    Thocky,
    Vintage,
    Custom,
}

impl SwitchKind {
    pub fn label(self) -> &'static str {
        match self {
            SwitchKind::Clicky => "Clicky",
            SwitchKind::Tactile => "Tactile",
            SwitchKind::Linear => "Linear",
            SwitchKind::Thocky => "Thocky",
            SwitchKind::Vintage => "Vintage",
            SwitchKind::Custom => "Custom",
        }
    }

    /// Accent used for badges and card highlights, RGB.
    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            SwitchKind::Clicky => (56, 189, 248),
            SwitchKind::Tactile => (167, 139, 250),
            SwitchKind::Linear => (52, 211, 153),
            SwitchKind::Thocky => (251, 191, 36),
            SwitchKind::Vintage => (244, 114, 182),
            SwitchKind::Custom => (148, 163, 184),
        }
    }

    pub fn parse(text: &str) -> Self {
        match text.trim().to_ascii_lowercase().as_str() {
            "clicky" | "click" => SwitchKind::Clicky,
            "tactile" | "bump" => SwitchKind::Tactile,
            "linear" | "smooth" => SwitchKind::Linear,
            "thocky" | "thock" | "deep" => SwitchKind::Thocky,
            "vintage" | "spring" => SwitchKind::Vintage,
            _ => SwitchKind::Custom,
        }
    }
}

// ---------------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum SampleSource {
    /// Shipped inside the executable.
    Embedded(&'static [u8], &'static str),
    /// A `.wav` / `.ogg` file inside the user library.
    File(PathBuf),
}

impl SampleSource {
    pub fn key(&self) -> String {
        match self {
            SampleSource::Embedded(bytes, ext) => {
                format!("builtin:{ext}:{}:{}", bytes.len(), bytes.as_ptr() as usize)
            }
            SampleSource::File(path) => {
                let len = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                format!("file:{}:{len}", path.display())
            }
        }
    }

    pub fn read(&self) -> Result<Vec<u8>, String> {
        match self {
            SampleSource::Embedded(bytes, _) => Ok(bytes.to_vec()),
            SampleSource::File(path) => {
                fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
            }
        }
    }

    pub fn extension(&self) -> &str {
        match self {
            SampleSource::Embedded(_, ext) => ext,
            SampleSource::File(path) => path.extension().and_then(|e| e.to_str()).unwrap_or("wav"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub kind: SwitchKind,
    pub description: String,
    pub author: String,
    pub builtin: bool,
    pub press: SampleSource,
    pub release: Option<SampleSource>,
    /// Folder backing a user-installed pack.
    pub folder: Option<PathBuf>,
}

struct Builtin {
    id: &'static str,
    name: &'static str,
    kind: SwitchKind,
    blurb: &'static str,
    press: &'static [u8],
    release: &'static [u8],
}

const BUILTINS: &[Builtin] = &[
    Builtin {
        id: "cherry-mx-blue",
        name: "Cherry MX Blue",
        kind: SwitchKind::Clicky,
        blurb: "Sharp click jacket with a bright plastic snap.",
        press: include_bytes!("../assets/default_sounds/cherry-mx-blue-press.wav"),
        release: include_bytes!("../assets/default_sounds/cherry-mx-blue-up.wav"),
    },
    Builtin {
        id: "cherry-mx-brown",
        name: "Cherry MX Brown",
        kind: SwitchKind::Tactile,
        blurb: "Soft tactile bump, no click, warm housing thock.",
        press: include_bytes!("../assets/default_sounds/cherry-mx-brown-press.wav"),
        release: include_bytes!("../assets/default_sounds/cherry-mx-brown-up.wav"),
    },
    Builtin {
        id: "gateron-red",
        name: "Gateron Red",
        kind: SwitchKind::Linear,
        blurb: "Smooth linear travel with a low, quick bottom-out.",
        press: include_bytes!("../assets/default_sounds/gateron-red-press.wav"),
        release: include_bytes!("../assets/default_sounds/gateron-red-up.wav"),
    },
    Builtin {
        id: "novelkeys-cream",
        name: "NovelKeys Cream",
        kind: SwitchKind::Thocky,
        blurb: "Full POM body, deep marbly thock with a long tail.",
        press: include_bytes!("../assets/default_sounds/novelkeys-cream-press.wav"),
        release: include_bytes!("../assets/default_sounds/novelkeys-cream-up.wav"),
    },
    Builtin {
        id: "buckling-spring",
        name: "Buckling Spring",
        kind: SwitchKind::Vintage,
        blurb: "1980s IBM clack with a ringing steel spring.",
        press: include_bytes!("../assets/default_sounds/buckling-spring-press.wav"),
        release: include_bytes!("../assets/default_sounds/buckling-spring-up.wav"),
    },
];

pub fn builtin_profiles() -> Vec<Profile> {
    BUILTINS
        .iter()
        .map(|b| Profile {
            id: b.id.to_string(),
            name: b.name.to_string(),
            kind: b.kind,
            description: b.blurb.to_string(),
            author: "Mechanical Click".into(),
            builtin: true,
            press: SampleSource::Embedded(b.press, "wav"),
            release: Some(SampleSource::Embedded(b.release, "wav")),
            folder: None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// User library
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PackMeta {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub press: String,
    #[serde(default)]
    pub release: String,
}

impl PackMeta {
    pub fn display_name(&self, fallback: &str) -> String {
        if self.name.trim().is_empty() {
            fallback.to_string()
        } else {
            self.name.clone()
        }
    }
}

fn audio_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| {
                        let e = e.to_ascii_lowercase();
                        e == "wav" || e == "ogg"
                    })
                    .unwrap_or(false)
        })
        .collect();
    files.sort();
    files
}

fn pick_file(files: &[PathBuf], named: &str, needles: &[&str]) -> Option<PathBuf> {
    if !named.trim().is_empty() {
        let direct = files.iter().find(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy() == named)
                .unwrap_or(false)
        });
        if let Some(found) = direct {
            return Some(found.clone());
        }
    }
    files
        .iter()
        .find(|p| {
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            needles.iter().any(|n| name.contains(n))
        })
        .cloned()
}

/// Reads one pack folder into a profile. Returns `None` when it holds no audio.
pub fn load_pack(dir: &Path) -> Option<Profile> {
    let files = audio_files(dir);
    if files.is_empty() {
        return None;
    }
    let folder_name = dir.file_name()?.to_string_lossy().to_string();
    let meta: PackMeta = fs::read(dir.join("pack.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();

    let press = pick_file(&files, &meta.press, &["press", "down", "keydown", "click"])
        .or_else(|| files.first().cloned())?;
    let release =
        pick_file(&files, &meta.release, &["up", "release", "keyup"]).filter(|p| *p != press);

    let kind = SwitchKind::parse(&meta.kind);
    let name = meta.display_name(&pretty_name(&folder_name));
    Some(Profile {
        id: if meta.id.trim().is_empty() {
            folder_name.clone()
        } else {
            slug(&meta.id)
        },
        name,
        kind,
        description: if meta.description.trim().is_empty() {
            "Installed from the sound store.".into()
        } else {
            meta.description
        },
        author: if meta.author.trim().is_empty() {
            "Community".into()
        } else {
            meta.author
        },
        builtin: false,
        press: SampleSource::File(press),
        release: release.map(SampleSource::File),
        folder: Some(dir.to_path_buf()),
    })
}

pub fn scan_library() -> Vec<Profile> {
    let dir = sounds_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut profiles: Vec<Profile> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter_map(|p| load_pack(&p))
        .collect();
    profiles.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    profiles
}

/// "cherry-mx-blue" -> "Cherry Mx Blue"
pub fn pretty_name(raw: &str) -> String {
    raw.replace(['_', '-'], " ")
        .split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn write_pack(
    id: &str,
    name: &str,
    kind: SwitchKind,
    author: &str,
    description: &str,
    press: &[u8],
    release: Option<&[u8]>,
) -> Result<PathBuf, String> {
    write_pack_ext(
        id,
        name,
        kind,
        author,
        description,
        (press, "wav"),
        release.map(|bytes| (bytes, "wav")),
    )
}

/// Same as [`write_pack`] but keeps the original file extensions, which is what
/// user imports (`.ogg`) and remote packs need.
pub fn write_pack_ext(
    id: &str,
    name: &str,
    kind: SwitchKind,
    author: &str,
    description: &str,
    press: (&[u8], &str),
    release: Option<(&[u8], &str)>,
) -> Result<PathBuf, String> {
    let dir = sounds_dir().join(slug(id));
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create pack folder: {e}"))?;
    let press_name = format!("press.{}", clean_ext(press.1));
    write_atomic(&dir.join(&press_name), press.0)?;
    let mut release_name = String::new();
    if let Some((bytes, ext)) = release {
        release_name = format!("up.{}", clean_ext(ext));
        write_atomic(&dir.join(&release_name), bytes)?;
    }
    let meta = PackMeta {
        id: slug(id),
        name: name.to_string(),
        kind: kind.label().to_string(),
        author: author.to_string(),
        description: description.to_string(),
        version: VERSION.to_string(),
        press: press_name,
        release: release_name,
    };
    let json = serde_json::to_vec_pretty(&meta).map_err(|e| e.to_string())?;
    write_atomic(&dir.join("pack.json"), &json)?;
    Ok(dir)
}

fn clean_ext(ext: &str) -> &str {
    match ext.trim_start_matches('.').to_ascii_lowercase().as_str() {
        "ogg" => "ogg",
        _ => "wav",
    }
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("cannot create folder: {e}"))?;
    }
    let tmp = path.with_extension("tmp-download");
    {
        let mut file = fs::File::create(&tmp).map_err(|e| format!("cannot write file: {e}"))?;
        file.write_all(bytes)
            .map_err(|e| format!("cannot write file: {e}"))?;
        file.flush()
            .map_err(|e| format!("cannot write file: {e}"))?;
    }
    fs::rename(&tmp, path).map_err(|e| format!("cannot finish file: {e}"))
}

pub fn delete_pack(profile: &Profile) -> Result<(), String> {
    if let Some(folder) = &profile.folder {
        fs::remove_dir_all(folder).map_err(|e| format!("cannot remove pack: {e}"))?;
    }
    Ok(())
}

pub fn is_installed(id: &str) -> bool {
    sounds_dir().join(slug(id)).is_dir()
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// 0.0 ..= 2.0 (0 % .. 200 %).
    pub volume: f32,
    pub profile_id: String,
    pub theme: Theme,
    pub enabled: bool,
    pub key_release_sound: bool,
    pub allow_repeat: bool,
    pub capture_injected: bool,
    pub start_with_windows: bool,
    pub close_to_tray: bool,
    /// Requested output buffer in milliseconds (4 ..= 40).
    pub latency_ms: u32,
    pub manifest_url: String,
    pub store_url_editing: bool,
    pub dismissed_hint: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            volume: 0.85,
            profile_id: "cherry-mx-blue".into(),
            theme: Theme::Dark,
            enabled: true,
            key_release_sound: true,
            allow_repeat: false,
            capture_injected: false,
            start_with_windows: false,
            close_to_tray: true,
            latency_ms: 8,
            manifest_url: String::new(),
            store_url_editing: false,
            dismissed_hint: false,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let mut cfg = fs::read(config_path())
            .ok()
            .and_then(|b| serde_json::from_slice::<Config>(&b).ok())
            .unwrap_or_default();
        cfg.volume = cfg.volume.clamp(0.0, 2.0);
        cfg.latency_ms = cfg.latency_ms.clamp(4, 40);
        cfg
    }

    pub fn save(&self) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        write_atomic(&config_path(), &json)
    }
}

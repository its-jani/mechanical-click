//! Sound store: a bundled catalogue that always works offline plus a remote
//! manifest/zip pipeline (`manifest.json` -> download -> extract into
//! `%APPDATA%/mechanical-click/sounds/<pack>/`).

use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::config::{self, SwitchKind};

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub enum PackSource {
    /// Shipped with the executable; installing just writes the files out.
    Bundled {
        press: &'static [u8],
        release: &'static [u8],
    },
    /// Fetched from a manifest URL (http(s), `file://` or a plain path).
    Remote { url: String },
}

#[derive(Clone)]
pub struct StorePack {
    pub id: String,
    pub name: String,
    pub kind: SwitchKind,
    pub author: String,
    pub description: String,
    pub version: String,
    pub size: u64,
    pub featured: bool,
    pub source: PackSource,
}

impl StorePack {
    pub fn size_label(&self) -> String {
        if self.size >= 1024 * 1024 {
            format!("{:.1} MB", self.size as f64 / (1024.0 * 1024.0))
        } else if self.size == 0 {
            "remote".into()
        } else {
            format!("{} KB", (self.size as f64 / 1024.0).round().max(1.0) as u64)
        }
    }
}

// ---------------------------------------------------------------------------
// Bundled catalogue
// ---------------------------------------------------------------------------

struct Bundled {
    id: &'static str,
    name: &'static str,
    kind: SwitchKind,
    author: &'static str,
    blurb: &'static str,
    version: &'static str,
    featured: bool,
    press: &'static [u8],
    release: &'static [u8],
}

const BUNDLED: &[Bundled] = &[
    Bundled {
        id: "holy-panda",
        name: "Holy Panda",
        kind: SwitchKind::Tactile,
        author: "Community",
        blurb: "Long-pole tactile bump with a deep, rounded thock.",
        version: "1.2.0",
        featured: true,
        press: include_bytes!("../../assets/default_sounds/holy-panda-press.wav"),
        release: include_bytes!("../../assets/default_sounds/holy-panda-up.wav"),
    },
    Bundled {
        id: "kailh-box-jade",
        name: "Kailh Box Jade",
        kind: SwitchKind::Clicky,
        author: "Kailh",
        blurb: "Glass-shattering click bar, the loudest switch in the box.",
        version: "1.1.0",
        featured: false,
        press: include_bytes!("../../assets/default_sounds/kailh-box-jade-press.wav"),
        release: include_bytes!("../../assets/default_sounds/kailh-box-jade-up.wav"),
    },
    Bundled {
        id: "topre-45g",
        name: "Topre 45g",
        kind: SwitchKind::Thocky,
        author: "Realforce",
        blurb: "Cup rubber dome thock: soft, low and impossibly smooth.",
        version: "1.0.3",
        featured: false,
        press: include_bytes!("../../assets/default_sounds/topre-45g-press.wav"),
        release: include_bytes!("../../assets/default_sounds/topre-45g-up.wav"),
    },
    Bundled {
        id: "alpine-winter",
        name: "Alpine Winter",
        kind: SwitchKind::Linear,
        author: "Community",
        blurb: "Cold, crisp linear clack with a tight, fast tail.",
        version: "2.0.0",
        featured: false,
        press: include_bytes!("../../assets/default_sounds/alpine-winter-press.wav"),
        release: include_bytes!("../../assets/default_sounds/alpine-winter-up.wav"),
    },
    Bundled {
        id: "mx-speed-silver",
        name: "MX Speed Silver",
        kind: SwitchKind::Linear,
        author: "Cherry",
        blurb: "Short-throw linear with a snappy bottom-out for fast typists.",
        version: "1.0.0",
        featured: false,
        press: include_bytes!("../../assets/default_sounds/mx-speed-silver-press.wav"),
        release: include_bytes!("../../assets/default_sounds/mx-speed-silver-up.wav"),
    },
];

pub fn bundled_catalog() -> Vec<StorePack> {
    BUNDLED
        .iter()
        .map(|b| StorePack {
            id: b.id.to_string(),
            name: b.name.to_string(),
            kind: b.kind,
            author: b.author.to_string(),
            description: b.blurb.to_string(),
            version: b.version.to_string(),
            size: (b.press.len() + b.release.len()) as u64,
            featured: b.featured,
            source: PackSource::Bundled {
                press: b.press,
                release: b.release,
            },
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    packs: Vec<ManifestPack>,
}

#[derive(Deserialize)]
struct ManifestPack {
    id: String,
    name: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    featured: bool,
    /// Zip archive, absolute or relative to the manifest.
    url: String,
}

pub const EXAMPLE_MANIFEST: &str = r#"{
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
}"#;

fn is_remote(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

fn local_path(url: &str) -> &str {
    url.strip_prefix("file://").unwrap_or(url)
}

fn resolve(base: &str, url: &str) -> String {
    if is_remote(url) || url.starts_with("file://") || Path::new(url).is_absolute() {
        return url.to_string();
    }
    // Relative to the manifest location (works for both http and disk).
    match base.rfind('/') {
        Some(idx) => format!("{}/{}", &base[..idx], url.trim_start_matches("./")),
        None => url.to_string(),
    }
}

pub fn parse_manifest(text: &str, base_url: &str) -> Result<Vec<StorePack>, String> {
    let manifest: Manifest =
        serde_json::from_str(text).map_err(|e| format!("invalid manifest: {e}"))?;
    if manifest.packs.is_empty() {
        return Err("manifest has no packs".into());
    }
    Ok(manifest
        .packs
        .into_iter()
        .filter(|p| !p.id.trim().is_empty() && !p.name.trim().is_empty())
        .map(|p| StorePack {
            id: config::slug(&p.id),
            name: p.name,
            kind: SwitchKind::parse(&p.kind),
            author: if p.author.trim().is_empty() {
                "Community".into()
            } else {
                p.author
            },
            description: if p.description.trim().is_empty() {
                "A community switch sample pack.".into()
            } else {
                p.description
            },
            version: if p.version.trim().is_empty() {
                "1.0.0".into()
            } else {
                p.version
            },
            size: p.size,
            featured: p.featured,
            source: PackSource::Remote {
                url: resolve(base_url, &p.url),
            },
        })
        .collect())
}

pub fn load_manifest(url: &str) -> Result<Vec<StorePack>, String> {
    let text = if is_remote(url) {
        let mut response = ureq::get(url)
            .call()
            .map_err(|e| format!("request failed: {e}"))?;
        response
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("cannot read manifest: {e}"))?
    } else {
        fs::read_to_string(local_path(url)).map_err(|e| format!("cannot read manifest: {e}"))?
    };
    parse_manifest(&text, url)
}

// ---------------------------------------------------------------------------
// Downloads
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum StoreEvent {
    Progress { id: String, done: u64, total: u64 },
    Installed { id: String, name: String },
    Failed { id: String, error: String },
}

pub struct Store {
    events_tx: Sender<StoreEvent>,
    events_rx: Receiver<StoreEvent>,
    manifest_rx: Option<Receiver<Result<Vec<StorePack>, String>>>,
}

impl Store {
    pub fn new() -> Self {
        let (events_tx, events_rx) = mpsc::channel();
        Self {
            events_tx,
            events_rx,
            manifest_rx: None,
        }
    }

    pub fn fetch_manifest(&mut self, url: String) {
        let (tx, rx) = mpsc::channel();
        self.manifest_rx = Some(rx);
        std::thread::Builder::new()
            .name("mc-manifest".into())
            .spawn(move || {
                let result = load_manifest(&url);
                let _ = tx.send(result);
            })
            .ok();
    }

    pub fn poll_manifest(&mut self) -> Option<Result<Vec<StorePack>, String>> {
        let rx = self.manifest_rx.as_ref()?;
        match rx.try_recv() {
            Ok(result) => {
                self.manifest_rx = None;
                Some(result)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.manifest_rx = None;
                Some(Err("manifest request stopped unexpectedly".into()))
            }
        }
    }

    pub fn is_fetching(&self) -> bool {
        self.manifest_rx.is_some()
    }

    pub fn install(&self, pack: StorePack) {
        let tx = self.events_tx.clone();
        std::thread::Builder::new()
            .name("mc-install".into())
            .spawn(move || {
                let id = pack.id.clone();
                let result = match &pack.source {
                    PackSource::Bundled { press, release } => config::write_pack(
                        &pack.id,
                        &pack.name,
                        pack.kind,
                        &pack.author,
                        &pack.description,
                        press,
                        Some(release),
                    )
                    .map(|_| ()),
                    PackSource::Remote { url } => download_and_extract(&pack, url, &tx),
                };
                match result {
                    Ok(()) => {
                        let _ = tx.send(StoreEvent::Installed {
                            id,
                            name: pack.name.clone(),
                        });
                    }
                    Err(error) => {
                        let _ = tx.send(StoreEvent::Failed { id, error });
                    }
                }
            })
            .ok();
    }

    pub fn drain(&self) -> Vec<StoreEvent> {
        let mut out = Vec::new();
        while let Ok(event) = self.events_rx.try_recv() {
            out.push(event);
        }
        out
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

fn download_and_extract(
    pack: &StorePack,
    url: &str,
    tx: &Sender<StoreEvent>,
) -> Result<(), String> {
    let bytes = if is_remote(url) {
        let mut response = ureq::get(url)
            .call()
            .map_err(|e| format!("download failed: {e}"))?;
        let total = response
            .body()
            .content_length()
            .filter(|n| *n > 0)
            .unwrap_or(pack.size);
        let mut reader = response.body_mut().as_reader();
        stream_to_vec(&mut reader, total, &pack.id, tx)?
    } else {
        let path = local_path(url);
        let total = fs::metadata(path).map(|m| m.len()).unwrap_or(pack.size);
        let mut file = fs::File::open(path).map_err(|e| format!("cannot open zip: {e}"))?;
        stream_to_vec(&mut file, total, &pack.id, tx)?
    };
    extract_zip(&bytes, pack, tx)
}

fn stream_to_vec(
    reader: &mut impl Read,
    total: u64,
    id: &str,
    tx: &Sender<StoreEvent>,
) -> Result<Vec<u8>, String> {
    let mut data: Vec<u8> = Vec::with_capacity(total.min(8 * 1024 * 1024) as usize);
    let mut chunk = [0u8; 32 * 1024];
    let mut last = Instant::now();
    let mut last_sent = 0u64;
    loop {
        let read = reader
            .read(&mut chunk)
            .map_err(|e| format!("download failed: {e}"))?;
        if read == 0 {
            break;
        }
        data.extend_from_slice(&chunk[..read]);
        if data.len() > 64 * 1024 * 1024 {
            return Err("archive is too large".into());
        }
        if last.elapsed() >= Duration::from_millis(70) && data.len() as u64 != last_sent {
            last_sent = data.len() as u64;
            let _ = tx.send(StoreEvent::Progress {
                id: id.to_string(),
                done: last_sent,
                total,
            });
            last = Instant::now();
        }
    }
    let _ = tx.send(StoreEvent::Progress {
        id: id.to_string(),
        done: data.len() as u64,
        total: total.max(data.len() as u64),
    });
    Ok(data)
}

fn extract_zip(data: &[u8], pack: &StorePack, tx: &Sender<StoreEvent>) -> Result<(), String> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(data)).map_err(|e| format!("invalid zip archive: {e}"))?;
    if archive.len() > 512 {
        return Err("archive contains too many files".into());
    }
    let dir = config::sounds_dir().join(config::slug(&pack.id));
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create pack folder: {e}"))?;

    let mut written: Vec<(String, bool)> = Vec::new(); // (file name, looks like release)
    let total = archive.len() as u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("cannot read archive entry: {e}"))?;
        if !entry.is_file() {
            continue;
        }
        let Some(path) = entry.enclosed_name() else {
            continue;
        };
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if ext != "wav" && ext != "ogg" {
            continue;
        }
        if entry.size() > 24 * 1024 * 1024 {
            continue;
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| format!("cannot extract {name}: {e}"))?;
        let safe = config::slug(&name.replace('.', "-")) + "." + &ext;
        let is_release = name.contains("up") || name.contains("release");
        config::write_atomic(&dir.join(&safe), &bytes)?;
        written.push((safe, is_release));
        let _ = tx.send(StoreEvent::Progress {
            id: pack.id.clone(),
            done: index as u64 + 1,
            total,
        });
    }

    if written.is_empty() {
        return Err("archive contains no .wav or .ogg samples".into());
    }
    let press = written
        .iter()
        .find(|(_, release)| !*release)
        .or_else(|| written.first())
        .map(|(name, _)| name.clone())
        .unwrap_or_default();
    let release = written
        .iter()
        .find(|(name, is_release)| *is_release && *name != press)
        .map(|(name, _)| name.clone());

    let meta = config::PackMeta {
        id: config::slug(&pack.id),
        name: pack.name.clone(),
        kind: pack.kind.label().to_string(),
        author: pack.author.clone(),
        description: pack.description.clone(),
        version: pack.version.clone(),
        press,
        release: release.unwrap_or_default(),
    };
    let json = serde_json::to_vec_pretty(&meta).map_err(|e| e.to_string())?;
    config::write_atomic(&dir.join("pack.json"), &json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_urls_resolve() {
        assert_eq!(
            resolve("https://host/packs/manifest.json", "a.zip"),
            "https://host/packs/a.zip"
        );
        assert_eq!(
            resolve("https://host/manifest.json", "https://cdn/x.zip"),
            "https://cdn/x.zip"
        );
    }

    #[test]
    fn manifest_parses() {
        let packs = parse_manifest(EXAMPLE_MANIFEST, "https://host/manifest.json").unwrap();
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].kind, SwitchKind::Tactile);
    }
}

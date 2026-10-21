//! Offline asset generator for mechanical-click.
//!
//! Synthesizes the bundled mechanical-switch samples and renders the application
//! icon. No external crates on purpose so it can be built with plain `rustc`:
//!
//! ```text
//! rustc --edition 2021 -O tools/gen_assets.rs -o tools/gen_assets.exe
//! tools/gen_assets.exe assets
//! ```

use std::f32::consts::TAU;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

const SR: f32 = 44_100.0;

struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Self {
        Rng(seed.wrapping_mul(2_654_435_761) | 1)
    }
    fn noise(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

fn envelope(t: f32, attack: f32, decay: f32) -> f32 {
    if t < attack {
        t / attack.max(1e-6)
    } else {
        (-(t - attack) / decay.max(1e-6)).exp()
    }
}

/// Damped sine partial with a short anti-click attack.
fn partial(buf: &mut [f32], start: f32, freq: f32, amp: f32, decay: f32) {
    let i0 = (start * SR) as usize;
    let len = ((decay * 9.0) * SR) as usize + 1;
    for i in 0..len {
        let idx = i0 + i;
        if idx >= buf.len() {
            break;
        }
        let t = i as f32 / SR;
        buf[idx] += amp * envelope(t, 0.0006, decay) * (TAU * freq * t).sin();
    }
}

/// Sine with an exponential pitch sweep - the "case thump" of a key.
fn thump(buf: &mut [f32], start: f32, f0: f32, f1: f32, amp: f32, decay: f32) {
    let i0 = (start * SR) as usize;
    let len = ((decay * 9.0) * SR) as usize + 1;
    let mut phase = 0.0f32;
    for i in 0..len {
        let idx = i0 + i;
        if idx >= buf.len() {
            break;
        }
        let t = i as f32 / SR;
        let k = (-t / (decay * 0.8)).exp();
        let freq = f1 + (f0 - f1) * k;
        phase += TAU * freq / SR;
        buf[idx] += amp * envelope(t, 0.0008, decay) * phase.sin();
    }
}

/// Band-passed noise burst - plastic clack, housing rattle, click leaf.
fn clack(
    buf: &mut [f32],
    start: f32,
    center: f32,
    q: f32,
    amp: f32,
    decay: f32,
    rng: &mut Rng,
) {
    let w0 = TAU * center / SR;
    let alpha = w0.sin() / (2.0 * q.max(0.1));
    let a0 = 1.0 + alpha;
    let (b0, b2) = (alpha / a0, -alpha / a0);
    let (a1, a2) = (-2.0 * w0.cos() / a0, (1.0 - alpha) / a0);
    let (mut y1, mut y2) = (0.0f32, 0.0f32);
    let (mut x1, mut x2) = (0.0f32, 0.0f32);
    let i0 = (start * SR) as usize;
    let len = ((decay * 9.0) * SR) as usize + 1;
    for i in 0..len {
        let idx = i0 + i;
        if idx >= buf.len() {
            break;
        }
        let x = rng.noise();
        let y = b0 * x + b2 * x2 - a1 * y1 - a2 * y2;
        x2 = x1;
        x1 = x;
        y2 = y1;
        y1 = y;
        let t = i as f32 / SR;
        buf[idx] += amp * envelope(t, 0.00015, decay) * (y * 3.2);
    }
}

struct Spec {
    id: &'static str,
    file: &'static str,
    dur: f32,
    seed: u32,
    /// Broadband snap level + center of the snap band + decay.
    snap: f32,
    snap_tone: f32,
    snap_decay: f32,
    /// High frequency ticks: (freq, amp, decay).
    tick: &'static [(f32, f32, f32)],
    /// Low body partials: (freq, amp, decay).
    body: &'static [(f32, f32, f32)],
    /// Case thump: (f0, f1, amp, decay).
    thump: (f32, f32, f32, f32),
    /// Resonant noise: (center, q, amp, decay).
    clack: &'static [(f32, f32, f32, f32)],
    /// Inharmonic ring: (freq, amp, decay).
    metal: &'static [(f32, f32, f32)],
}

const SPECS: &[Spec] = &[
    Spec {
        id: "mx-blue",
        file: "cherry-mx-blue",
        dur: 0.17,
        seed: 11,
        snap: 0.85,
        snap_tone: 5200.0,
        snap_decay: 0.0055,
        tick: &[(3100.0, 0.30, 0.013), (4300.0, 0.18, 0.009), (6100.0, 0.10, 0.006)],
        body: &[(190.0, 0.26, 0.038), (420.0, 0.13, 0.022), (780.0, 0.07, 0.013)],
        thump: (170.0, 120.0, 0.48, 0.045),
        clack: &[(1500.0, 2.0, 0.30, 0.014), (2700.0, 2.4, 0.20, 0.009)],
        metal: &[(3800.0, 0.045, 0.05)],
    },
    Spec {
        id: "mx-brown",
        file: "cherry-mx-brown",
        dur: 0.15,
        seed: 23,
        snap: 0.16,
        snap_tone: 3000.0,
        snap_decay: 0.004,
        tick: &[(2200.0, 0.06, 0.01)],
        body: &[(210.0, 0.40, 0.052), (430.0, 0.17, 0.030), (900.0, 0.09, 0.020)],
        thump: (205.0, 140.0, 0.50, 0.042),
        clack: &[(1200.0, 1.6, 0.34, 0.020)],
        metal: &[],
    },
    Spec {
        id: "gat-red",
        file: "gateron-red",
        dur: 0.17,
        seed: 37,
        snap: 0.07,
        snap_tone: 2600.0,
        snap_decay: 0.004,
        tick: &[],
        body: &[(160.0, 0.46, 0.070), (320.0, 0.21, 0.045), (640.0, 0.11, 0.030)],
        thump: (150.0, 92.0, 0.55, 0.058),
        clack: &[(900.0, 1.4, 0.22, 0.030)],
        metal: &[],
    },
    Spec {
        id: "nk-cream",
        file: "novelkeys-cream",
        dur: 0.21,
        seed: 41,
        snap: 0.05,
        snap_tone: 2200.0,
        snap_decay: 0.0035,
        tick: &[],
        body: &[
            (120.0, 0.50, 0.100),
            (238.0, 0.34, 0.072),
            (476.0, 0.17, 0.048),
            (952.0, 0.07, 0.028),
        ],
        thump: (132.0, 78.0, 0.66, 0.090),
        clack: &[(700.0, 1.5, 0.24, 0.045)],
        metal: &[],
    },
    Spec {
        id: "buckling-spring",
        file: "buckling-spring",
        dur: 0.30,
        seed: 53,
        snap: 0.80,
        snap_tone: 4200.0,
        snap_decay: 0.005,
        tick: &[(2400.0, 0.26, 0.020), (3600.0, 0.18, 0.014)],
        body: &[(220.0, 0.24, 0.050)],
        thump: (210.0, 150.0, 0.44, 0.050),
        clack: &[(1800.0, 2.2, 0.22, 0.012)],
        metal: &[
            (1850.0, 0.17, 0.140),
            (2650.0, 0.12, 0.105),
            (3920.0, 0.07, 0.072),
            (5210.0, 0.04, 0.050),
        ],
    },
    Spec {
        id: "box-jade",
        file: "kailh-box-jade",
        dur: 0.15,
        seed: 67,
        snap: 0.95,
        snap_tone: 6000.0,
        snap_decay: 0.0045,
        tick: &[(3600.0, 0.34, 0.011), (5200.0, 0.22, 0.008)],
        body: &[(200.0, 0.28, 0.030), (450.0, 0.12, 0.018)],
        thump: (172.0, 128.0, 0.40, 0.035),
        clack: &[(2000.0, 2.2, 0.26, 0.011)],
        metal: &[(6000.0, 0.05, 0.03)],
    },
    Spec {
        id: "holy-panda",
        file: "holy-panda",
        dur: 0.18,
        seed: 71,
        snap: 0.20,
        snap_tone: 2900.0,
        snap_decay: 0.004,
        tick: &[(2000.0, 0.07, 0.010)],
        body: &[(170.0, 0.45, 0.062), (340.0, 0.24, 0.040), (700.0, 0.10, 0.024)],
        thump: (180.0, 118.0, 0.56, 0.052),
        clack: &[(1000.0, 1.4, 0.30, 0.030)],
        metal: &[],
    },
    Spec {
        id: "topre-45g",
        file: "topre-45g",
        dur: 0.20,
        seed: 83,
        snap: 0.05,
        snap_tone: 1800.0,
        snap_decay: 0.0035,
        tick: &[],
        body: &[(95.0, 0.55, 0.090), (190.0, 0.28, 0.062), (380.0, 0.14, 0.040)],
        thump: (112.0, 68.0, 0.60, 0.080),
        clack: &[(600.0, 1.0, 0.20, 0.042)],
        metal: &[],
    },
    Spec {
        id: "alpine-winter",
        file: "alpine-winter",
        dur: 0.16,
        seed: 97,
        snap: 0.13,
        snap_tone: 3400.0,
        snap_decay: 0.0038,
        tick: &[(2600.0, 0.09, 0.009)],
        body: &[(230.0, 0.40, 0.058), (520.0, 0.19, 0.038), (1040.0, 0.11, 0.026)],
        thump: (200.0, 140.0, 0.45, 0.050),
        clack: &[(1600.0, 1.4, 0.24, 0.025)],
        metal: &[],
    },
    Spec {
        id: "mx-speed-silver",
        file: "mx-speed-silver",
        dur: 0.13,
        seed: 103,
        snap: 0.11,
        snap_tone: 3800.0,
        snap_decay: 0.0035,
        tick: &[(2900.0, 0.08, 0.008)],
        body: &[(260.0, 0.40, 0.040), (520.0, 0.19, 0.026), (1100.0, 0.08, 0.016)],
        thump: (220.0, 160.0, 0.44, 0.036),
        clack: &[(1800.0, 1.5, 0.20, 0.018)],
        metal: &[],
    },
];

fn synth(spec: &Spec, release: bool) -> Vec<f32> {
    let dur = if release { spec.dur * 0.80 } else { spec.dur };
    let mut buf = vec![0.0f32; (dur * SR) as usize + 1];
    let mut rng = Rng::new(spec.seed + if release { 1000 } else { 0 });

    // Release notes are a softer, slightly lower version of the press.
    let (pitch, gain, snap_gain) = if release {
        (0.94, 0.62, 0.45)
    } else {
        (1.0, 1.0, 1.0)
    };

    if spec.snap > 0.0 {
        clack(
            &mut buf,
            0.0,
            spec.snap_tone,
            0.9,
            spec.snap * 0.55 * snap_gain,
            spec.snap_decay,
            &mut rng,
        );
        partial(
            &mut buf,
            0.0,
            spec.snap_tone * 0.55 * pitch,
            spec.snap * 0.18 * snap_gain,
            0.004,
        );
    }
    for &(f, a, d) in spec.tick {
        partial(&mut buf, 0.0004, f * pitch, a * gain * 0.9, d);
    }
    for &(f, a, d) in spec.body {
        partial(&mut buf, 0.0012, f * pitch, a * gain, d);
    }
    let (f0, f1, a, d) = spec.thump;
    thump(&mut buf, 0.0, f0 * pitch, f1 * pitch, a * gain, d);
    for &(c, q, a, d) in spec.clack {
        clack(&mut buf, 0.0008, c * pitch, q, a * gain * 0.8, d, &mut rng);
    }
    for &(f, a, d) in spec.metal {
        partial(&mut buf, 0.0006, f * pitch, a * gain * 0.85, d);
    }

    // DC block + gentle saturation, then normalize.
    let mut prev_in = 0.0f32;
    let mut prev_out = 0.0f32;
    let r = 0.995;
    for s in buf.iter_mut() {
        let x = *s;
        let y = r * prev_out + x - prev_in;
        prev_in = x;
        prev_out = y;
        *s = (y * 1.4).tanh();
    }
    let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-6);
    let norm = 0.86 / peak;
    let fade = (0.006 * SR) as usize;
    let n = buf.len();
    for (i, s) in buf.iter_mut().enumerate() {
        let mut v = *s * norm;
        if n - i < fade {
            v *= (n - i) as f32 / fade as f32;
        }
        *s = v;
    }
    buf
}

fn write_wav(path: &Path, samples: &[f32]) {
    let data_len = samples.len() * 2;
    let mut out = Vec::with_capacity(44 + data_len);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&(SR as u32).to_le_bytes());
    out.extend_from_slice(&(SR as u32 * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    let mut f = File::create(path).expect("create wav");
    f.write_all(&out).expect("write wav");
}

// ---------------------------------------------------------------------------
// Icon rendering
// ---------------------------------------------------------------------------

type Rgba = [u8; 4];

fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    [
        (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t) as u8,
        (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t) as u8,
        (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t) as u8,
        (a[3] as f32 + (b[3] as f32 - a[3] as f32) * t) as u8,
    ]
}

fn over(dst: Rgba, src: Rgba) -> Rgba {
    let sa = src[3] as f32 / 255.0;
    let da = dst[3] as f32 / 255.0;
    let oa = sa + da * (1.0 - sa);
    if oa <= 0.0 {
        return [0, 0, 0, 0];
    }
    let ch = |i: usize| ((src[i] as f32 * sa + dst[i] as f32 * da * (1.0 - sa)) / oa).round() as u8;
    [ch(0), ch(1), ch(2), (oa * 255.0).round() as u8]
}

fn in_round_rect(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> bool {
    if x < x0 || x > x1 || y < y0 || y > y1 {
        return false;
    }
    let cx = x.clamp(x0 + r, x1 - r);
    let cy = y.clamp(y0 + r, y1 - r);
    let dx = x - cx;
    let dy = y - cy;
    dx * dx + dy * dy <= r * r
}

fn render_icon(size: usize) -> Vec<Rgba> {
    let ss = 4usize;
    let n = size * ss;
    let mut acc = vec![[0f32; 4]; size * size];
    let f = n as f32;

    for py in 0..n {
        for px in 0..n {
            let x = px as f32 + 0.5;
            let y = py as f32 + 0.5;
            let mut c: Rgba = [0, 0, 0, 0];

            // Rounded square plate with a vertical blue/cyan gradient.
            let plate_r = f * 0.235;
            if in_round_rect(x, y, 0.0, 0.0, f, f, plate_r) {
                let t = y / f;
                let top = [0x4f, 0xcb, 0xff, 255];
                let bottom = [0x1c, 0x63, 0xe8, 255];
                c = mix(top, bottom, t.powf(1.15));
                // Soft diagonal sheen.
                let sheen = ((1.0 - (x / f) * 0.65 - (y / f) * 0.55) * 0.9).clamp(0.0, 1.0);
                c = over(c, [255, 255, 255, (sheen * 26.0) as u8]);
            }

            // Keycap: outer bevel then top face.
            let k0 = f * 0.215;
            let k1 = f * 0.785;
            let kr = f * 0.145;
            if in_round_rect(x, y, k0, k0 + f * 0.012, k1, k1, kr) {
                c = over(c, [10, 34, 74, 46]);
            }
            if in_round_rect(x, y, k0, k0, k1, k1 - f * 0.018, kr) {
                let t = (y - k0) / (k1 - k0);
                c = mix([242, 251, 255, 255], [186, 226, 255, 255], t);
                if in_round_rect(x, y, k0 + 2.5, k0 + 2.5, k1 - 2.5, k1 - 2.5, kr * 0.86) {
                    c = mix([252, 254, 255, 255], [206, 236, 255, 255], t);
                }
            }

            // Stem: a small rounded dot, the universal "switch" mark.
            let cx = f * 0.5;
            let cy = f * 0.5;
            let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
            if d < f * 0.115 {
                c = over(c, [0x1d, 0x5f, 0xc9, 255]);
                if d < f * 0.072 {
                    c = over(c, [0x2f, 0x81, 0xf7, 255]);
                }
                if d < f * 0.030 {
                    c = over(c, [255, 255, 255, 235]);
                }
            }

            let ax = px / ss;
            let ay = py / ss;
            let a = &mut acc[ay * size + ax];
            a[0] += c[0] as f32;
            a[1] += c[1] as f32;
            a[2] += c[2] as f32;
            a[3] += c[3] as f32;
        }
    }

    let inv = 1.0 / (ss * ss) as f32;
    acc.iter()
        .map(|a| {
            let al = a[3] * inv;
            let ch = |i: usize| {
                if al <= 0.01 {
                    0
                } else {
                    (a[i] * inv).min(255.0).round() as u8
                }
            };
            [ch(0), ch(1), ch(2), al.round() as u8]
        })
        .collect()
}

fn ico_from_images(images: &[(usize, Vec<Rgba>)], path: &Path) {
    let count = images.len() as u16;
    let mut blobs: Vec<Vec<u8>> = Vec::new();
    for (size, px) in images {
        let w = *size;
        let h = *size;
        let mask_row = ((w + 31) / 32) * 4;
        let mut bmp = Vec::with_capacity(40 + w * h * 4 + mask_row * h);
        bmp.extend_from_slice(&40u32.to_le_bytes());
        bmp.extend_from_slice(&(w as i32).to_le_bytes());
        bmp.extend_from_slice(&((h * 2) as i32).to_le_bytes());
        bmp.extend_from_slice(&1u16.to_le_bytes());
        bmp.extend_from_slice(&32u16.to_le_bytes());
        bmp.extend_from_slice(&0u32.to_le_bytes());
        bmp.extend_from_slice(&((w * h * 4) as u32).to_le_bytes());
        bmp.extend_from_slice(&[0u8; 16]);
        for y in (0..h).rev() {
            for x in 0..w {
                let p = px[y * w + x];
                bmp.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
            }
        }
        bmp.extend(std::iter::repeat(0u8).take(mask_row * h));
        blobs.push(bmp);
    }

    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    let mut offset = 6 + 16 * images.len();
    for (i, (size, _)) in images.iter().enumerate() {
        out.push(if *size >= 256 { 0 } else { *size as u8 });
        out.push(if *size >= 256 { 0 } else { *size as u8 });
        out.push(0);
        out.push(0);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(blobs[i].len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += blobs[i].len();
    }
    for b in blobs {
        out.extend_from_slice(&b);
    }
    let mut f = File::create(path).expect("create ico");
    f.write_all(&out).expect("write ico");
}

fn main() {
    let root = std::env::args().nth(1).unwrap_or_else(|| "assets".into());
    let root = Path::new(&root);
    let sounds = root.join("default_sounds");
    let icons = root.join("icons");
    fs::create_dir_all(&sounds).expect("sounds dir");
    fs::create_dir_all(&icons).expect("icons dir");

    for spec in SPECS {
        let press = synth(spec, false);
        let release = synth(spec, true);
        write_wav(&sounds.join(format!("{}-press.wav", spec.file)), &press);
        write_wav(&sounds.join(format!("{}-up.wav", spec.file)), &release);
        println!(
            "sounds: {:<22} press {:>5.1} ms  release {:>5.1} ms",
            spec.id,
            press.len() as f32 * 1000.0 / SR,
            release.len() as f32 * 1000.0 / SR
        );
    }

    let sizes = [16usize, 24, 32, 48, 64, 128, 256];
    let images: Vec<(usize, Vec<Rgba>)> = sizes.iter().map(|s| (*s, render_icon(*s))).collect();
    ico_from_images(&images, &icons.join("app.ico"));

    // Raw RGBA for egui / tray at runtime (straight alpha, top-down).
    for s in [64usize, 256] {
        let px = &images.iter().find(|(sz, _)| *sz == s).unwrap().1;
        let mut raw = Vec::with_capacity(s * s * 4);
        for p in px {
            raw.extend_from_slice(&[p[0], p[1], p[2], p[3]]);
        }
        fs::write(icons.join(format!("app{s}.rgba")), raw).expect("write rgba");
    }
    println!("icon: app.ico ({} sizes)", sizes.len());
}

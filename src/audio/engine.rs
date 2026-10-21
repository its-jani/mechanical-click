//! The audio engine: one long-lived cpal output stream, a lock-free-ish command
//! channel and a small fixed voice pool. The callback never allocates.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, FromSample, SampleFormat, SizedSample, Stream, StreamConfig};
use crossbeam_channel::{Receiver, Sender, bounded};

use super::decode::{self, SoundBuffer};
use crate::config::{Profile, SampleSource};

const MAX_VOICES: usize = 24;
const CHANNEL_CAPACITY: usize = 128;
const CACHE_LIMIT: usize = 96;

pub enum AudioCmd {
    /// Key press with the active profile.
    Play,
    /// Key release with the active profile (only sent when enabled).
    Release,
    /// One-off playback of an arbitrary buffer (store / library preview).
    Preview(Arc<SoundBuffer>),
    SetPress(Arc<SoundBuffer>),
    SetRelease(Option<Arc<SoundBuffer>>),
}

struct Voice {
    buffer: Arc<SoundBuffer>,
    /// Position in source frames (fractional).
    pos: f64,
    /// Source frames consumed per output frame.
    step: f64,
}

pub struct Mixer {
    rx: Receiver<AudioCmd>,
    press: Option<Arc<SoundBuffer>>,
    release: Option<Arc<SoundBuffer>>,
    voices: [Option<Voice>; MAX_VOICES],
    next: usize,
    gain: Arc<AtomicU32>,
    rate: u32,
    channels: usize,
}

impl Mixer {
    fn new(rx: Receiver<AudioCmd>, gain: Arc<AtomicU32>) -> Self {
        Self {
            rx,
            press: None,
            release: None,
            voices: std::array::from_fn(|_| None),
            next: 0,
            gain,
            rate: 48_000,
            channels: 2,
        }
    }

    fn configure(&mut self, rate: u32, channels: u16) {
        self.rate = rate.max(8_000);
        self.channels = channels.max(1) as usize;
        for voice in self.voices.iter_mut().flatten() {
            voice.step = voice.buffer.rate as f64 / self.rate as f64;
        }
    }

    fn start(&mut self, buffer: Arc<SoundBuffer>) {
        let step = buffer.rate as f64 / self.rate as f64;
        let voice = Voice {
            buffer,
            pos: 0.0,
            step,
        };
        // Round-robin voice stealing: with a handful of concurrent keys the
        // oldest slot is always either free or long finished.
        self.voices[self.next] = Some(voice);
        self.next = (self.next + 1) % MAX_VOICES;
    }

    fn apply(&mut self, cmd: AudioCmd) {
        match cmd {
            AudioCmd::Play => {
                if let Some(buf) = self.press.clone() {
                    self.start(buf);
                }
            }
            AudioCmd::Release => {
                if let Some(buf) = self.release.clone() {
                    self.start(buf);
                }
            }
            AudioCmd::Preview(buf) => self.start(buf),
            AudioCmd::SetPress(buf) => self.press = Some(buf),
            AudioCmd::SetRelease(buf) => self.release = buf,
        }
    }

    /// Renders one interleaved output block.
    fn fill(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        while let Ok(cmd) = self.rx.try_recv() {
            self.apply(cmd);
        }

        let channels = self.channels;
        let frames = out.len() / channels;
        let mut active = false;
        for slot in self.voices.iter_mut() {
            let Some(voice) = slot.as_mut() else {
                continue;
            };
            active = true;
            let data = &voice.buffer.frames;
            let len = data.len();
            let mut pos = voice.pos;
            for f in 0..frames {
                let i = pos as usize;
                if i + 1 >= len {
                    pos = len as f64;
                    break;
                }
                let frac = (pos - i as f64) as f32;
                let sample = data[i] + (data[i + 1] - data[i]) * frac;
                let base = f * channels;
                for c in 0..channels {
                    out[base + c] += sample;
                }
                pos += voice.step;
            }
            voice.pos = pos;
            if pos >= len as f64 {
                *slot = None;
            }
        }

        // Silence is the common case: skip gain and ceiling work entirely.
        if !active {
            return;
        }
        let gain = f32::from_bits(self.gain.load(Ordering::Relaxed));
        if (gain - 1.0).abs() > 0.001 {
            for s in out.iter_mut() {
                *s *= gain;
            }
        }
        // Gentle ceiling instead of hard clipping at 200 % volume.
        for s in out.iter_mut() {
            if s.abs() > 0.8 {
                *s = s.tanh();
            }
        }
    }
}

#[derive(Clone)]
pub struct EngineStatus {
    pub device: String,
    pub host: String,
    pub error: Option<String>,
    pub sample_rate: u32,
    pub channels: u16,
    pub buffer_frames: u32,
    pub requested_ms: u32,
}

impl EngineStatus {
    pub fn empty() -> Self {
        Self {
            device: "No output device".into(),
            host: String::new(),
            error: Some("no audio device".into()),
            sample_rate: 0,
            channels: 0,
            buffer_frames: 0,
            requested_ms: 0,
        }
    }

    pub fn ready(&self) -> bool {
        self.error.is_none() && self.buffer_frames > 0
    }

    pub fn latency_ms(&self) -> f32 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.buffer_frames as f32 * 1000.0 / self.sample_rate as f32
        }
    }
}

pub struct AudioEngine {
    tx: Sender<AudioCmd>,
    mixer: Arc<Mutex<Mixer>>,
    gain: Arc<AtomicU32>,
    stream: Option<Stream>,
    pub status: EngineStatus,
    cache: HashMap<String, Arc<SoundBuffer>>,
    previews: HashMap<String, Vec<f32>>,
    latency_ms: u32,
}

impl AudioEngine {
    pub fn new(latency_ms: u32) -> Self {
        let (tx, rx) = bounded(CHANNEL_CAPACITY);
        let gain = Arc::new(AtomicU32::new(1.0f32.to_bits()));
        let mixer = Arc::new(Mutex::new(Mixer::new(rx, gain.clone())));
        let mut engine = Self {
            tx,
            mixer,
            gain,
            stream: None,
            status: EngineStatus::empty(),
            cache: HashMap::new(),
            previews: HashMap::new(),
            latency_ms,
        };
        engine.rebuild();
        engine
    }

    /// Re-opens the output device; used at start-up and from "retry audio".
    pub fn rebuild(&mut self) {
        self.stream = None;
        let (stream, status) = open_stream(self.mixer.clone(), self.latency_ms);
        self.stream = stream;
        self.status = status;
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.configure(
                self.status.sample_rate.max(48_000),
                self.status.channels.max(2),
            );
        }
        if let Some(stream) = &self.stream {
            let _ = stream.play();
        }
    }

    pub fn set_latency(&mut self, latency_ms: u32) {
        self.latency_ms = latency_ms.clamp(4, 40);
        self.rebuild();
    }

    pub fn sender(&self) -> Sender<AudioCmd> {
        self.tx.clone()
    }

    pub fn set_gain(&self, gain: f32) {
        self.gain
            .store(gain.clamp(0.0, 2.0).to_bits(), Ordering::Relaxed);
    }

    pub fn load(&mut self, source: &SampleSource) -> Result<Arc<SoundBuffer>, String> {
        self.load_key(&source.key(), || {
            (source.read(), source.extension().to_string())
        })
    }

    fn load_key(
        &mut self,
        key: &str,
        read: impl FnOnce() -> (Result<Vec<u8>, String>, String),
    ) -> Result<Arc<SoundBuffer>, String> {
        if let Some(hit) = self.cache.get(key) {
            return Ok(hit.clone());
        }
        let (bytes, ext) = read();
        let buffer = Arc::new(decode::decode(&bytes?, &ext)?);
        if self.cache.len() >= CACHE_LIMIT {
            self.cache.clear();
            self.previews.clear();
        }
        self.cache.insert(key.to_string(), buffer.clone());
        Ok(buffer)
    }

    /// Decoded waveform for the UI, cached alongside the sample.
    pub fn waveform(&mut self, source: &SampleSource, buckets: usize) -> Vec<f32> {
        let key = format!("{}#{buckets}", source.key());
        if let Some(hit) = self.previews.get(&key) {
            return hit.clone();
        }
        let peaks = match self.load(source) {
            Ok(buffer) => decode::peaks(&buffer, buckets),
            Err(_) => vec![0.0; buckets],
        };
        self.previews.insert(key, peaks.clone());
        peaks
    }

    /// Makes `profile` the sound used for real key events.
    pub fn apply_profile(&mut self, profile: &Profile) -> Result<(), String> {
        let press = self.load(&profile.press)?;
        let release = match &profile.release {
            Some(src) => match self.load(src) {
                Ok(buf) => Some(buf),
                Err(_) => None,
            },
            None => None,
        };
        let _ = self.tx.try_send(AudioCmd::SetPress(press));
        let _ = self.tx.try_send(AudioCmd::SetRelease(release));
        Ok(())
    }

    pub fn preview(&mut self, source: &SampleSource) -> Result<(), String> {
        let buffer = self.load(source)?;
        let _ = self.tx.try_send(AudioCmd::Preview(buffer));
        Ok(())
    }

    pub fn sample_duration_ms(&mut self, source: &SampleSource) -> f32 {
        self.load(source).map(|b| b.duration_ms()).unwrap_or(0.0)
    }
}

/// Opens the default output device and starts the mixer callback.
fn open_stream(mixer: Arc<Mutex<Mixer>>, latency_ms: u32) -> (Option<Stream>, EngineStatus) {
    let host = cpal::default_host();
    let host_name = format!("{:?}", host.id().name());
    let mut status = EngineStatus {
        device: "Default output".into(),
        host: host_name,
        error: None,
        sample_rate: 0,
        channels: 0,
        buffer_frames: 0,
        requested_ms: latency_ms,
    };

    let Some(device) = host.default_output_device() else {
        status.error = Some("No audio output device found".into());
        return (None, status);
    };
    if let Ok(description) = device.description() {
        status.device = description.name().to_string();
    }

    let supported = match device.default_output_config() {
        Ok(cfg) => cfg,
        Err(err) => {
            status.error = Some(format!("No output format: {err}"));
            return (None, status);
        }
    };

    let sample_rate = supported.sample_rate();
    let channels = supported.channels();
    let format = supported.sample_format();

    // Ask for a small buffer; fall back to the driver default if refused.
    let target_frames = (sample_rate as u64 * latency_ms as u64 / 1000).max(64) as u32;
    let frames = match supported.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } => target_frames.clamp(*min, *max),
        cpal::SupportedBufferSize::Unknown => target_frames,
    };

    let base: StreamConfig = supported.config();
    let fixed = StreamConfig {
        buffer_size: BufferSize::Fixed(frames),
        ..base
    };
    // If the driver refuses a small buffer, retry with its own default.
    let fallback = StreamConfig {
        buffer_size: BufferSize::Default,
        ..base
    };

    let mut last_error = String::new();
    for config in [fixed, fallback] {
        let error_slot: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let err_cb = {
            let slot = error_slot.clone();
            move |err: cpal::Error| {
                if let Ok(mut guard) = slot.lock() {
                    *guard = Some(err.to_string());
                }
            }
        };
        let built = match format {
            SampleFormat::I16 => build_typed::<i16>(&device, &config, mixer.clone(), err_cb),
            SampleFormat::U16 => build_typed::<u16>(&device, &config, mixer.clone(), err_cb),
            _ => build_typed::<f32>(&device, &config, mixer.clone(), err_cb),
        };

        match built {
            Ok(stream) => {
                if let Ok(mut m) = mixer.lock() {
                    m.configure(sample_rate, channels);
                }
                status.sample_rate = sample_rate;
                status.channels = channels;
                status.buffer_frames = match config.buffer_size {
                    BufferSize::Fixed(n) => n,
                    BufferSize::Default => 0,
                };
                if let Ok(mut guard) = error_slot.lock() {
                    if let Some(err) = guard.take() {
                        status.error = Some(err);
                    }
                }
                return (Some(stream), status);
            }
            Err(err) => last_error = err.to_string(),
        }
    }

    status.error = Some(format!("Cannot open output stream: {last_error}"));
    (None, status)
}

fn build_typed<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    mixer: Arc<Mutex<Mixer>>,
    error_cb: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let mut scratch: Vec<f32> = Vec::new();
    device.build_output_stream::<T, _, _>(
        *config,
        move |data: &mut [T], _info| {
            if scratch.len() < data.len() {
                scratch.resize(data.len(), 0.0);
            }
            let block = &mut scratch[..data.len()];
            match mixer.try_lock() {
                Ok(mut m) => m.fill(block),
                Err(_) => block.fill(0.0),
            }
            for (out, sample) in data.iter_mut().zip(block.iter()) {
                *out = T::from_sample(*sample);
            }
        },
        error_cb,
        None,
    )
}

//! Sample decoding: `.wav` (hound) and `.ogg` (lewton) into mono `f32` buffers,
//! trimmed and cached in RAM so playback never touches the disk.

use std::io::Cursor;

/// A fully decoded, mono, disk-free sound ready for the mixer.
pub struct SoundBuffer {
    pub frames: Vec<f32>,
    pub rate: u32,
}

impl SoundBuffer {
    pub fn duration_ms(&self) -> f32 {
        self.frames.len() as f32 * 1000.0 / self.rate.max(1) as f32
    }
}

pub fn decode(bytes: &[u8], ext: &str) -> Result<SoundBuffer, String> {
    if bytes.len() < 12 {
        return Err("sample is empty".into());
    }
    let ogg_magic = &bytes[0..4] == b"OggS";
    let wav_magic = &bytes[0..4] == b"RIFF";
    let (frames, rate) = if wav_magic {
        decode_wav(bytes)?
    } else if ogg_magic {
        decode_ogg(bytes)?
    } else if ext.eq_ignore_ascii_case("ogg") {
        decode_ogg(bytes)?
    } else {
        return Err("unsupported audio format (use .wav or .ogg)".into());
    };
    if frames.is_empty() {
        return Err("sample contains no audio".into());
    }
    Ok(finish(frames, rate))
}

fn decode_wav(bytes: &[u8]) -> Result<(Vec<f32>, u32), String> {
    let mut reader =
        hound::WavReader::new(Cursor::new(bytes)).map_err(|e| format!("invalid wav: {e}"))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let rate = spec.sample_rate.clamp(8_000, 384_000);

    let interleaved: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, 32) => {
            reader.samples::<f32>().filter_map(|s| s.ok()).collect()
        }
        (hound::SampleFormat::Int, 8) => reader
            .samples::<i8>()
            .filter_map(|s| s.ok())
            .map(|s| s as f32 / 128.0)
            .collect(),
        (hound::SampleFormat::Int, 16) => reader
            .samples::<i16>()
            .filter_map(|s| s.ok())
            .map(|s| s as f32 / 32_768.0)
            .collect(),
        (hound::SampleFormat::Int, bits) if bits <= 32 => {
            let scale = 1.0 / (1i64 << (bits.min(32) - 1)) as f32;
            reader
                .samples::<i32>()
                .filter_map(|s| s.ok())
                .map(|s| s as f32 * scale)
                .collect()
        }
        _ => {
            return Err(format!(
                "unsupported wav encoding ({:?})",
                spec.sample_format
            ));
        }
    };
    Ok((downmix(interleaved, channels), rate))
}

fn decode_ogg(bytes: &[u8]) -> Result<(Vec<f32>, u32), String> {
    let mut reader = lewton::inside_ogg::OggStreamReader::new(Cursor::new(bytes))
        .map_err(|e| format!("invalid ogg: {e}"))?;
    let channels = (reader.ident_hdr.audio_channels as usize).max(1);
    let rate = reader.ident_hdr.audio_sample_rate.clamp(8_000, 384_000);
    let mut interleaved: Vec<f32> = Vec::new();
    loop {
        match reader.read_dec_packet_itl() {
            Ok(Some(packet)) => {
                interleaved.extend(packet.iter().map(|s| *s as f32 / 32_768.0));
                if interleaved.len() > rate as usize * channels * 8 {
                    // Nothing useful past eight seconds; stop pathological files.
                    break;
                }
            }
            Ok(None) => break,
            Err(e) => return Err(format!("ogg decode failed: {e}")),
        }
    }
    Ok((downmix(interleaved, channels), rate))
}

fn downmix(interleaved: Vec<f32>, channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved;
    }
    let mut out = Vec::with_capacity(interleaved.len() / channels);
    let mut acc = 0.0;
    let mut n = 0usize;
    for sample in interleaved {
        acc += sample;
        n += 1;
        if n == channels {
            out.push(acc / channels as f32);
            acc = 0.0;
            n = 0;
        }
    }
    out
}

/// Trims silence (that would otherwise be audible latency), caps the tail and
/// normalises the peak for the UI waveform.
fn finish(mut frames: Vec<f32>, rate: u32) -> SoundBuffer {
    let threshold = 0.0008; // ~ -62 dBFS
    let first = frames.iter().position(|s| s.abs() > threshold).unwrap_or(0);
    let last = frames
        .iter()
        .rposition(|s| s.abs() > threshold)
        .map(|i| i + 1)
        .unwrap_or(frames.len());
    if last > first {
        let pre_roll = (rate as usize / 4_000).min(first); // 0.25 ms of attack
        frames.drain(last..);
        frames.drain(..first - pre_roll);
    }
    frames.truncate(rate as usize * 2);

    // Kill a click if the sample was cut mid-waveform.
    let fade = (rate as usize / 250).min(frames.len());
    let total = frames.len();
    for (i, s) in frames.iter_mut().enumerate().skip(total - fade) {
        let t = (total - i) as f32 / fade as f32;
        *s *= t;
    }

    SoundBuffer { frames, rate }
}

/// `n` normalised peak buckets for waveform previews.
pub fn peaks(buffer: &SoundBuffer, n: usize) -> Vec<f32> {
    if buffer.frames.is_empty() || n == 0 {
        return vec![0.0; n];
    }
    let mut out = Vec::with_capacity(n);
    let per = (buffer.frames.len() as f32 / n as f32).max(1.0);
    for i in 0..n {
        let start = (i as f32 * per) as usize;
        let end = (((i + 1) as f32 * per) as usize)
            .min(buffer.frames.len())
            .max(start + 1);
        let peak = buffer.frames[start..end]
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()));
        out.push(peak);
    }
    let max = out.iter().fold(0.0f32, |m, s| m.max(*s)).max(1e-6);
    out.iter().map(|v| (v / max).clamp(0.0, 1.0)).collect()
}

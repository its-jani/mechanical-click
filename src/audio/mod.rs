//! Audio: decoding (`wav`/`ogg`) and the low-latency cpal mixer.

pub mod decode;
pub mod engine;

pub use engine::{AudioCmd, AudioEngine};

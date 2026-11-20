//! Audio playback built on `kira`.
//!
//! Phase 0 only needs a synthetic beep to prove that sound works.

use std::f32::consts::TAU;
use std::sync::Arc;

use anyhow::{Context, Result};
use kira::sound::static_sound::StaticSoundData;
use kira::{AudioManager, AudioManagerSettings, DefaultBackend, Frame};

/// Sample rate used for sounds generated in code.
const SYNTH_SAMPLE_RATE: u32 = 44_100;

/// Owns the connection to the system audio device.
pub struct Audio {
    manager: AudioManager<DefaultBackend>,
}

impl Audio {
    /// Opens the default audio output device.
    pub fn new() -> Result<Self> {
        let manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
            .map_err(|e| anyhow::anyhow!("{e}"))
            .context("could not open the audio output device")?;
        Ok(Self { manager })
    }

    /// Plays a short sine beep at the given frequency (Hz).
    pub fn beep(&mut self, frequency: f32) -> Result<()> {
        let sound = sine_beep(frequency, 0.25);
        self.manager
            .play(sound)
            .map_err(|e| anyhow::anyhow!("{e}"))
            .context("could not play the beep")?;
        Ok(())
    }
}

/// Generates a sine tone with short fade-in/out so it does not click.
fn sine_beep(frequency: f32, seconds: f32) -> StaticSoundData {
    let total = (SYNTH_SAMPLE_RATE as f32 * seconds) as usize;
    let fade = (SYNTH_SAMPLE_RATE as f32 * 0.01) as usize; // 10 ms
    let frames: Vec<Frame> = (0..total)
        .map(|i| {
            let t = i as f32 / SYNTH_SAMPLE_RATE as f32;
            let envelope = (i.min(total - i) as f32 / fade as f32).min(1.0);
            Frame::from_mono((TAU * frequency * t).sin() * 0.3 * envelope)
        })
        .collect();

    StaticSoundData {
        sample_rate: SYNTH_SAMPLE_RATE,
        frames: Arc::from(frames),
        settings: Default::default(),
        slice: None,
    }
}

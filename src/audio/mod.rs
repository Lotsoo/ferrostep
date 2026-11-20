//! Audio playback built on `kira`: music, song previews and UI sounds.

pub mod clock;

use std::f32::consts::TAU;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use kira::sound::static_sound::StaticSoundData;
use kira::sound::streaming::{StreamingSoundData, StreamingSoundHandle};
use kira::sound::{FromFileError, PlaybackState};
use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Frame, Tween};

pub use clock::AudioClock;

/// Sample rate used for sounds generated in code.
const SYNTH_SAMPLE_RATE: u32 = 44_100;

/// Converts a 0..=1 slider value to decibels (with a perceptual curve).
pub fn volume_to_db(volume: f32) -> Decibels {
    if volume <= 0.001 {
        Decibels::SILENCE
    } else {
        Decibels(20.0 * volume.clamp(0.0, 1.0).powf(1.5).log10())
    }
}

fn tween_ms(ms: u64) -> Tween {
    Tween {
        duration: Duration::from_millis(ms),
        ..Default::default()
    }
}

/// A playing song (or preview).
pub struct Music {
    handle: StreamingSoundHandle<FromFileError>,
}

impl Music {
    /// Current playback position in seconds (updated per audio buffer).
    pub fn position(&self) -> f64 {
        self.handle.position()
    }

    /// True once the sound finished or was stopped.
    pub fn is_stopped(&self) -> bool {
        self.handle.state() == PlaybackState::Stopped
    }

    pub fn pause(&mut self) {
        self.handle.pause(tween_ms(5));
    }

    pub fn resume(&mut self) {
        self.handle.resume(tween_ms(5));
    }

    /// Fades out over `ms` milliseconds, then stops.
    pub fn fade_out(&mut self, ms: u64) {
        self.handle.stop(tween_ms(ms));
    }
}

impl Drop for Music {
    fn drop(&mut self) {
        self.handle.stop(tween_ms(30));
    }
}

/// Owns the connection to the system audio device.
pub struct Audio {
    manager: AudioManager<DefaultBackend>,
    click: StaticSoundData,
    click_accent: StaticSoundData,
}

impl Audio {
    /// Opens the default audio output device.
    pub fn new() -> Result<Self> {
        let manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
            .map_err(|e| anyhow!("{e}"))
            .context("could not open the audio output device")?;
        Ok(Self {
            manager,
            click: sine_burst(1320.0, 0.04, 0.5),
            click_accent: sine_burst(1760.0, 0.04, 0.7),
        })
    }

    /// Sets the master volume (0..=1).
    pub fn set_volume(&mut self, volume: f32) {
        self.manager
            .main_track()
            .set_volume(volume_to_db(volume), tween_ms(50));
    }

    /// Starts streaming a music file from `start` seconds, with optional fade-in.
    pub fn play_music(&mut self, path: &Path, start: f64, fade_in_ms: u64) -> Result<Music> {
        let mut data = StreamingSoundData::from_file(path)
            .with_context(|| format!("could not open audio file {}", path.display()))?
            .start_position(start.max(0.0));
        if fade_in_ms > 0 {
            data = data.fade_in_tween(tween_ms(fade_in_ms));
        }
        let handle = self.manager.play(data).map_err(|e| anyhow!("{e}"))?;
        Ok(Music { handle })
    }

    /// Plays a metronome click (used by the calibration wizard).
    pub fn click(&mut self, accent: bool) {
        let sound = if accent {
            self.click_accent.clone()
        } else {
            self.click.clone()
        };
        if let Err(e) = self.manager.play(sound) {
            log::warn!("click failed: {e}");
        }
    }

    /// Plays a short sine beep at the given frequency (Hz).
    pub fn beep(&mut self, frequency: f32) -> Result<()> {
        self.manager
            .play(sine_burst(frequency, 0.25, 0.3))
            .map_err(|e| anyhow!("{e}"))
            .context("could not play the beep")?;
        Ok(())
    }
}

/// Generates a sine tone with short fade-in/out so it does not click.
fn sine_burst(frequency: f32, seconds: f32, amp: f32) -> StaticSoundData {
    let total = (SYNTH_SAMPLE_RATE as f32 * seconds) as usize;
    let fade = (SYNTH_SAMPLE_RATE as f32 * 0.004) as usize;
    let frames: Vec<Frame> = (0..total)
        .map(|i| {
            let t = i as f32 / SYNTH_SAMPLE_RATE as f32;
            let envelope = (i.min(total - i) as f32 / fade as f32).min(1.0);
            Frame::from_mono((TAU * frequency * t).sin() * amp * envelope)
        })
        .collect();
    StaticSoundData {
        sample_rate: SYNTH_SAMPLE_RATE,
        frames: Arc::from(frames),
        settings: Default::default(),
        slice: None,
    }
}

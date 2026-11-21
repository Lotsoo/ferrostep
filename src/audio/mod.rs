//! Audio playback built on `kira`: music, song previews and UI sounds.

pub mod clock;
pub mod sfx;

use std::f32::consts::TAU;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use kira::sound::static_sound::StaticSoundData;
use kira::sound::streaming::{StreamingSoundData, StreamingSoundHandle};
use kira::sound::{FromFileError, PlaybackState};
use kira::track::{TrackBuilder, TrackHandle};
use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Frame, Tween};

pub use clock::AudioClock;
pub use sfx::Sfx;

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

/// Menu-sound slider: gentler curve than music, so 70% is still loud.
fn sfx_volume_to_db(volume: f32) -> Decibels {
    if volume <= 0.001 {
        Decibels::SILENCE
    } else {
        Decibels(20.0 * volume.clamp(0.0, 1.0).sqrt().log10())
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
    /// A fade-out was requested; dropping must not cut it short.
    fading: bool,
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
        self.fading = true;
    }
}

impl Drop for Music {
    fn drop(&mut self) {
        if !self.fading {
            self.handle.stop(tween_ms(30));
        }
    }
}

/// Owns the connection to the system audio device.
pub struct Audio {
    manager: AudioManager<DefaultBackend>,
    click: StaticSoundData,
    click_accent: StaticSoundData,
    /// Menu sounds, indexed by [`Sfx::index`].
    sfx: Vec<StaticSoundData>,
    /// Separate mixer track so menu sounds have their own volume.
    sfx_track: Option<TrackHandle>,
    /// Current menu sound slider value.
    sfx_volume: f32,
}

impl Audio {
    /// Opens the default audio output device.
    pub fn new() -> Result<Self> {
        let mut manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
            .map_err(|e| anyhow!("{e}"))
            .context("could not open the audio output device")?;
        let sfx_track = manager
            .add_sub_track(TrackBuilder::new())
            .map_err(|e| log::warn!("no separate track for menu sounds: {e}"))
            .ok();
        Ok(Self {
            manager,
            click: sine_burst(1320.0, 0.04, 0.5),
            click_accent: sine_burst(1760.0, 0.04, 0.7),
            sfx: Sfx::ALL
                .iter()
                .map(|s| stereo_sound(sfx::render(*s, SYNTH_SAMPLE_RATE)))
                .collect(),
            sfx_track,
            sfx_volume: 1.0,
        })
    }

    /// Sets the master volume (0..=1).
    pub fn set_volume(&mut self, volume: f32) {
        self.manager
            .main_track()
            .set_volume(volume_to_db(volume), tween_ms(50));
    }

    /// Sets the menu sound volume (0..=1), relative to the master volume
    /// (one master slider controls everything; sounds are pre-balanced
    /// against typical music loudness).
    pub fn set_sfx_volume(&mut self, volume: f32) {
        self.sfx_volume = volume;
        self.apply_sfx_volume();
    }

    fn apply_sfx_volume(&mut self) {
        let db = sfx_volume_to_db(self.sfx_volume);
        if let Some(track) = &mut self.sfx_track {
            track.set_volume(db, tween_ms(50));
        }
    }

    /// Plays a menu sound effect.
    pub fn play_sfx(&mut self, sfx: Sfx) {
        let Some(sound) = self.sfx.get(sfx.index()).cloned() else {
            return;
        };
        let result = match &mut self.sfx_track {
            Some(track) => track.play(sound).map(|_| ()),
            None => self.manager.play(sound).map(|_| ()),
        };
        if let Err(e) = result {
            log::debug!("menu sound failed: {e}");
        }
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
        Ok(Music {
            handle,
            fading: false,
        })
    }

    /// Streams audio embedded in the binary (e.g. Ogg data from `include_bytes!`).
    /// With `looped`, it repeats until stopped.
    pub fn play_bytes(
        &mut self,
        bytes: &'static [u8],
        looped: bool,
        fade_in_ms: u64,
    ) -> Result<Music> {
        let mut data = StreamingSoundData::from_cursor(std::io::Cursor::new(bytes))
            .context("could not decode embedded audio")?;
        if looped {
            data = data.loop_region(..);
        }
        if fade_in_ms > 0 {
            data = data.fade_in_tween(tween_ms(fade_in_ms));
        }
        let handle = self.manager.play(data).map_err(|e| anyhow!("{e}"))?;
        Ok(Music {
            handle,
            fading: false,
        })
    }

    /// Plays a soft plucked tone (tutorial melody): a few harmonics with an
    /// exponential decay, about `length` seconds long.
    pub fn play_tone(&mut self, frequency: f32, length: f32) {
        let rate = SYNTH_SAMPLE_RATE as f32;
        let total = ((length + 0.3) * rate) as usize;
        let decay = 3.2 / length.max(0.2);
        let samples: Vec<f32> = (0..total)
            .map(|i| {
                let t = i as f32 / rate;
                let attack = (t / 0.006).min(1.0);
                let tail = ((total - i) as f32 / (0.03 * rate)).min(1.0);
                let w = TAU * frequency * t;
                let tone = w.sin() * (-t * decay).exp()
                    + 0.35 * (2.0 * w).sin() * (-t * decay * 1.8).exp()
                    + 0.12 * (3.0 * w).sin() * (-t * decay * 3.0).exp();
                tone * 0.32 * attack * tail
            })
            .collect();
        if let Err(e) = self.manager.play(mono_sound(samples)) {
            log::debug!("tone failed: {e}");
        }
    }

    /// Plays a metronome as one audio track: `lead` seconds of silence, then
    /// `beats` clicks at `bpm` (every 4th accented). Its position is a precise
    /// clock for calibration: the clicks are exactly where the track says.
    pub fn play_metronome(&mut self, bpm: f64, lead: f64, beats: usize) -> Result<Music> {
        let rate = SYNTH_SAMPLE_RATE as f64;
        let period = 60.0 / bpm;
        let total = ((lead + period * beats as f64 + 0.5) * rate) as usize;
        let mut samples = vec![0.0f32; total];
        for k in 0..beats {
            let (freq, amp) = if k % 4 == 0 {
                (1760.0, 0.7)
            } else {
                (1320.0, 0.5)
            };
            let start = ((lead + k as f64 * period) * rate) as usize;
            let len = (0.045 * rate) as usize;
            for i in 0..len {
                let t = i as f32 / SYNTH_SAMPLE_RATE as f32;
                // Instant attack (the click is sharp), quick exponential decay.
                let env = (-t * 90.0).exp() * (i as f32 / 8.0).min(1.0);
                if let Some(s) = samples.get_mut(start + i) {
                    *s = (TAU * freq * t).sin() * amp * env;
                }
            }
        }
        let mut wav = Vec::new();
        {
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: SYNTH_SAMPLE_RATE,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut writer = hound::WavWriter::new(std::io::Cursor::new(&mut wav), spec)
                .context("could not build the metronome")?;
            for s in samples {
                writer
                    .write_sample((s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16)
                    .context("could not build the metronome")?;
            }
            writer.finalize().context("could not build the metronome")?;
        }
        let data = StreamingSoundData::from_cursor(std::io::Cursor::new(wav))
            .context("could not decode the metronome")?;
        let handle = self.manager.play(data).map_err(|e| anyhow!("{e}"))?;
        Ok(Music {
            handle,
            fading: false,
        })
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
    let samples: Vec<f32> = (0..total)
        .map(|i| {
            let t = i as f32 / SYNTH_SAMPLE_RATE as f32;
            let envelope = (i.min(total - i) as f32 / fade as f32).min(1.0);
            (TAU * frequency * t).sin() * amp * envelope
        })
        .collect();
    mono_sound(samples)
}

/// Wraps mono samples (at the synth sample rate) as a playable sound.
fn mono_sound(samples: Vec<f32>) -> StaticSoundData {
    frames_sound(samples.into_iter().map(Frame::from_mono).collect())
}

/// Wraps stereo samples `[left, right]` (at the synth sample rate).
fn stereo_sound(samples: Vec<[f32; 2]>) -> StaticSoundData {
    frames_sound(
        samples
            .into_iter()
            .map(|[left, right]| Frame { left, right })
            .collect(),
    )
}

fn frames_sound(frames: Vec<Frame>) -> StaticSoundData {
    StaticSoundData {
        sample_rate: SYNTH_SAMPLE_RATE,
        frames: Arc::from(frames),
        settings: Default::default(),
        slice: None,
    }
}

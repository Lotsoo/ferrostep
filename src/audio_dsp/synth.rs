//! Synthetic audio for tests, the metronome test song and the calibration wizard.
//!
//! Everything here is generated in code, so no copyrighted audio is needed.

use std::f32::consts::TAU;

/// A tiny deterministic random generator (xorshift) for noise.
pub struct Noise(u64);

impl Noise {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    /// Uniform value in -1.0..1.0.
    pub fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 23) as f32 - 1.0
    }
}

/// Adds a decaying sine "click" starting at `time` seconds.
pub fn add_click(buf: &mut [f32], sample_rate: u32, time: f64, freq: f32, amp: f32) {
    let sr = sample_rate as f32;
    let start = (time * f64::from(sample_rate)).round();
    if start < 0.0 {
        return;
    }
    let start = start as usize;
    let len = (0.04 * sr) as usize; // 40 ms
    for i in 0..len {
        let Some(s) = buf.get_mut(start + i) else {
            break;
        };
        let t = i as f32 / sr;
        *s += (TAU * freq * t).sin() * amp * (-t * 90.0).exp();
    }
}

/// Adds a synthetic kick drum (pitch-swept sine) at `time`.
pub fn add_kick(buf: &mut [f32], sample_rate: u32, time: f64, amp: f32) {
    let sr = sample_rate as f32;
    let start = (time * f64::from(sample_rate)).round().max(0.0) as usize;
    let len = (0.25 * sr) as usize;
    let mut phase = 0.0f32;
    for i in 0..len {
        let Some(s) = buf.get_mut(start + i) else {
            break;
        };
        let t = i as f32 / sr;
        let freq = 50.0 + 120.0 * (-t * 30.0).exp();
        phase += TAU * freq / sr;
        *s += phase.sin() * amp * (-t * 12.0).exp();
    }
}

/// Adds a synthetic hi-hat (filtered noise burst) at `time`.
pub fn add_hihat(buf: &mut [f32], sample_rate: u32, time: f64, amp: f32, noise: &mut Noise) {
    let sr = sample_rate as f32;
    let start = (time * f64::from(sample_rate)).round().max(0.0) as usize;
    let len = (0.05 * sr) as usize;
    let mut prev = 0.0f32;
    for i in 0..len {
        let Some(s) = buf.get_mut(start + i) else {
            break;
        };
        let t = i as f32 / sr;
        let n = noise.next_f32();
        let high = n - prev; // crude high-pass
        prev = n;
        *s += high * amp * (-t * 80.0).exp();
    }
}

/// A click track: one click per beat, accented every `accent_every` beats.
///
/// `first_beat` is the time of the first click in seconds.
pub fn click_track(
    sample_rate: u32,
    duration: f64,
    bpm: f64,
    first_beat: f64,
    accent_every: u32,
) -> Vec<f32> {
    let mut buf = vec![0.0; (duration * f64::from(sample_rate)) as usize];
    let spb = 60.0 / bpm;
    let mut beat = 0u32;
    loop {
        let t = first_beat + f64::from(beat) * spb;
        if t >= duration {
            break;
        }
        let accent = accent_every > 0 && beat.is_multiple_of(accent_every);
        let (freq, amp) = if accent {
            (1760.0, 0.8)
        } else {
            (1320.0, 0.55)
        };
        add_click(&mut buf, sample_rate, t, freq, amp);
        beat += 1;
    }
    buf
}

/// A drum loop: kick on every beat, hi-hat on every off-beat.
pub fn drum_pattern(
    sample_rate: u32,
    duration: f64,
    bpm: f64,
    first_beat: f64,
    seed: u64,
) -> Vec<f32> {
    let mut buf = vec![0.0; (duration * f64::from(sample_rate)) as usize];
    let mut noise = Noise::new(seed);
    let spb = 60.0 / bpm;
    let mut i = 0u32;
    loop {
        let t = first_beat + f64::from(i) * spb;
        if t >= duration {
            break;
        }
        add_kick(&mut buf, sample_rate, t, 0.8);
        add_hihat(&mut buf, sample_rate, t + spb / 2.0, 0.35, &mut noise);
        i += 1;
    }
    buf
}

/// Mixes white noise into `buf` at amplitude `amp`.
pub fn add_noise(buf: &mut [f32], amp: f32, seed: u64) {
    let mut noise = Noise::new(seed);
    for s in buf {
        *s += noise.next_f32() * amp;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_track_has_energy_at_beats() {
        let sr = 44_100;
        let buf = click_track(sr, 2.0, 120.0, 0.25, 4);
        let at = |t: f64| buf[(t * f64::from(sr)) as usize + 20].abs();
        assert!(at(0.25) > 0.1);
        assert!(at(0.75) > 0.1);
        assert!(at(0.5) < 1e-6, "silence between beats");
    }

    #[test]
    fn noise_is_deterministic_and_bounded() {
        let mut a = Noise::new(7);
        let mut b = Noise::new(7);
        for _ in 0..1000 {
            let x = a.next_f32();
            assert_eq!(x, b.next_f32());
            assert!((-1.0..1.0).contains(&x));
        }
    }
}

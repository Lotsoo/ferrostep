//! STFT, spectral-flux onset detection function and peak picking.

use std::f32::consts::PI;

use rustfft::FftPlanner;
use rustfft::num_complex::Complex;

/// FFT window size (samples).
pub const WINDOW: usize = 2048;
/// Hop between frames (samples).
pub const HOP: usize = 512;
/// Log compression factor: `ln(1 + GAMMA * |X|)`.
const GAMMA: f32 = 10.0;
/// Frequency splitting the "low" (kick) and "high" (snare/hi-hat) bands.
const LOW_BAND_HZ: f32 = 200.0;
const HIGH_BAND_HZ: f32 = 2000.0;
/// Minimum distance between two onsets (seconds).
const MIN_ONSET_GAP: f64 = 0.05;

/// Where the flux of a sharp attack peaks inside a frame, in samples.
///
/// With a Hann window of size N and hop H, the positive difference between
/// consecutive frames is largest when the attack sits `3N/4 − H/2` samples
/// into the frame (derived from sin²a − sin²b = sin(a+b)·sin(a−b)).
pub const FLUX_LATENCY: usize = 3 * WINDOW / 4 - HOP / 2;

/// Onset detection functions, one value per frame.
#[derive(Debug, Clone)]
pub struct OnsetEnvelope {
    /// Full-band spectral flux.
    pub flux: Vec<f32>,
    /// Flux of the low band (kick).
    pub low: Vec<f32>,
    /// Flux of the high band (snare, hi-hat).
    pub high: Vec<f32>,
    /// RMS energy per frame (for sustain / preview detection).
    pub energy: Vec<f32>,
    pub sample_rate: u32,
}

impl OnsetEnvelope {
    /// Time (seconds) of the attack that peaks in frame `i` (fractional frames allowed).
    pub fn frame_time(&self, i: f64) -> f64 {
        (i * HOP as f64 + FLUX_LATENCY as f64) / f64::from(self.sample_rate)
    }

    /// Frames per second.
    pub fn frame_rate(&self) -> f64 {
        f64::from(self.sample_rate) / HOP as f64
    }

    /// Duration covered by the envelope (seconds).
    pub fn duration(&self) -> f64 {
        self.flux.len() as f64 / self.frame_rate()
    }
}

/// Which part of the spectrum dominates an onset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    Low,
    High,
    Mixed,
}

/// A detected onset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Onset {
    /// Seconds from the start of the audio.
    pub time: f64,
    /// Normalized strength 0..=1.
    pub strength: f32,
    pub band: Band,
    /// Low-band (kick) strength, normalized to the loudest low-band frame (0..=1).
    pub low: f32,
    /// High-band (snare/hi-hat) strength, normalized likewise (0..=1).
    pub high: f32,
    /// How long the sound stays loud after the onset (seconds).
    pub sustain: f64,
}

/// Computes the onset envelope. `progress` receives values 0..=1.
pub fn onset_envelope(
    samples: &[f32],
    sample_rate: u32,
    progress: &mut dyn FnMut(f32),
) -> OnsetEnvelope {
    let frames = if samples.len() >= WINDOW {
        (samples.len() - WINDOW) / HOP + 1
    } else {
        0
    };
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(WINDOW);
    let window: Vec<f32> = (0..WINDOW)
        .map(|i| (PI * i as f32 / WINDOW as f32).sin().powi(2))
        .collect();
    let bins = WINDOW / 2 + 1;
    let hz_per_bin = sample_rate as f32 / WINDOW as f32;
    let low_end = ((LOW_BAND_HZ / hz_per_bin) as usize).max(1).min(bins);
    let high_start = ((HIGH_BAND_HZ / hz_per_bin) as usize).min(bins);

    let mut buf = vec![Complex::new(0.0f32, 0.0); WINDOW];
    let mut scratch = vec![Complex::new(0.0f32, 0.0); fft.get_inplace_scratch_len()];
    let mut prev = vec![0.0f32; bins];
    let mut mag = vec![0.0f32; bins];
    let mut env = OnsetEnvelope {
        flux: Vec::with_capacity(frames),
        low: Vec::with_capacity(frames),
        high: Vec::with_capacity(frames),
        energy: Vec::with_capacity(frames),
        sample_rate,
    };

    for f in 0..frames {
        let chunk = &samples[f * HOP..f * HOP + WINDOW];
        let mut sum_sq = 0.0f32;
        for ((b, &s), &w) in buf.iter_mut().zip(chunk).zip(&window) {
            *b = Complex::new(s * w, 0.0);
            sum_sq += s * s;
        }
        fft.process_with_scratch(&mut buf, &mut scratch);
        let (mut total, mut low, mut high) = (0.0f32, 0.0f32, 0.0f32);
        for k in 0..bins {
            mag[k] = (1.0 + GAMMA * buf[k].norm()).ln();
            let diff = (mag[k] - prev[k]).max(0.0);
            total += diff;
            if k < low_end {
                low += diff;
            } else if k >= high_start {
                high += diff;
            }
        }
        std::mem::swap(&mut prev, &mut mag);
        // The first frame compares against silence; ignore it.
        let (total, low, high) = if f == 0 {
            (0.0, 0.0, 0.0)
        } else {
            (total, low, high)
        };
        env.flux.push(total);
        env.low.push(low);
        env.high.push(high);
        env.energy.push((sum_sq / WINDOW as f32).sqrt());
        if f % 256 == 0 {
            progress(f as f32 / frames.max(1) as f32);
        }
    }
    progress(1.0);
    env
}

/// Median of a small slice (copies and sorts).
fn median(values: &[f32]) -> f32 {
    let mut v = values.to_vec();
    v.sort_by(f32::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(0.0)
}

/// Detection function where the low band (kicks) counts as much as the full band.
///
/// Broadband sounds (hi-hats, noise) produce far more spectral flux than a kick,
/// which lives in a few low bins; without this balance kicks get lost.
pub fn balanced_odf(env: &OnsetEnvelope) -> Vec<f32> {
    let max_flux = env.flux.iter().copied().fold(0.0f32, f32::max).max(1e-9);
    let max_low = env.low.iter().copied().fold(0.0f32, f32::max).max(1e-9);
    let raw: Vec<f32> = env
        .flux
        .iter()
        .zip(&env.low)
        .map(|(f, l)| 0.5 * f / max_flux + 0.5 * l / max_low)
        .collect();
    let max = raw.iter().copied().fold(0.0f32, f32::max).max(1e-9);
    raw.into_iter().map(|v| v / max).collect()
}

/// Picks onsets from the envelope with an adaptive threshold (moving median + delta).
pub fn pick_onsets(env: &OnsetEnvelope) -> Vec<Onset> {
    let n = env.flux.len();
    if n < 3 {
        return Vec::new();
    }
    let odf = balanced_odf(env);
    let mean = odf.iter().sum::<f32>() / n as f32;
    let half = (0.1 * env.frame_rate()).round() as usize; // ±100 ms window
    let delta = 0.05 + 0.5 * mean;
    let min_gap = (MIN_ONSET_GAP * env.frame_rate()).ceil() as usize;
    let max_low = env.low.iter().copied().fold(0.0f32, f32::max).max(1e-9);
    let max_high = env.high.iter().copied().fold(0.0f32, f32::max).max(1e-9);

    let mut onsets: Vec<Onset> = Vec::new();
    let mut last_frame: Option<usize> = None;
    for i in 1..n - 1 {
        let v = odf[i];
        if v < odf[i - 1] || v < odf[i + 1] {
            continue;
        }
        let lo = i.saturating_sub(half);
        let hi = (i + half + 1).min(n);
        if v < median(&odf[lo..hi]) + delta {
            continue;
        }
        if odf[lo..hi].iter().any(|&x| x > v) {
            continue; // not the largest peak in its neighbourhood
        }
        // Parabolic interpolation for sub-frame accuracy.
        let (a, b, c) = (odf[i - 1], v, odf[i + 1]);
        let denom = a - 2.0 * b + c;
        let shift = if denom.abs() > 1e-9 {
            (0.5 * (a - c) / denom).clamp(-0.5, 0.5)
        } else {
            0.0
        };
        // Band energy can peak a few frames after the full-band flux (e.g. a kick's
        // pitch drops into the low band), so look slightly ahead.
        let around = |v: &[f32]| {
            v[i.saturating_sub(1)..(i + 4).min(n)]
                .iter()
                .copied()
                .fold(0.0f32, f32::max)
        };
        let low = around(&env.low) / max_low;
        let high = around(&env.high) / max_high;
        let onset = Onset {
            time: env.frame_time(i as f64 + f64::from(shift)),
            strength: v,
            band: band_of(low, high),
            low,
            high,
            sustain: sustain_of(env, i),
        };
        match last_frame {
            Some(prev) if i - prev < min_gap => {
                if let Some(last) = onsets.last_mut()
                    && onset.strength > last.strength
                {
                    *last = onset;
                    last_frame = Some(i);
                }
            }
            _ => {
                onsets.push(onset);
                last_frame = Some(i);
            }
        }
    }
    onsets
}

/// Classifies an onset from its normalized band strengths.
fn band_of(low: f32, high: f32) -> Band {
    if low > high * 1.5 {
        Band::Low
    } else if high > low * 1.5 {
        Band::High
    } else {
        Band::Mixed
    }
}

/// Seconds the energy stays above half of its peak just after frame `i` (max 4 s).
fn sustain_of(env: &OnsetEnvelope, i: usize) -> f64 {
    let n = env.energy.len();
    let peak_end = (i + 4).min(n);
    let peak = env.energy[i..peak_end]
        .iter()
        .copied()
        .fold(0.0f32, f32::max);
    if peak <= 1e-6 {
        return 0.0;
    }
    let max_frames = (4.0 * env.frame_rate()) as usize;
    let mut j = i;
    while j < n && j - i < max_frames && env.energy[j] > peak * 0.5 {
        j += 1;
    }
    (j - i) as f64 / env.frame_rate()
}

/// Start time of the most energetic `length`-second window (for song previews).
pub fn most_energetic_window(env: &OnsetEnvelope, length: f64) -> f64 {
    let win = (length * env.frame_rate()) as usize;
    let n = env.energy.len();
    if n <= win || win == 0 {
        return 0.0;
    }
    let mut sum: f32 = env.energy[..win].iter().sum();
    let (mut best, mut best_i) = (sum, 0usize);
    for i in 1..=n - win {
        sum += env.energy[i + win - 1] - env.energy[i - 1];
        if sum > best {
            best = sum;
            best_i = i;
        }
    }
    best_i as f64 / env.frame_rate()
}

/// Peak amplitude per bin, for drawing a waveform.
pub fn waveform_peaks(samples: &[f32], bins: usize) -> Vec<f32> {
    if samples.is_empty() || bins == 0 {
        return Vec::new();
    }
    let per = samples.len().div_ceil(bins);
    samples
        .chunks(per)
        .map(|c| c.iter().fold(0.0f32, |m, s| m.max(s.abs())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_dsp::synth;

    #[test]
    fn detects_clicks_at_the_right_time() {
        let sr = 44_100;
        let buf = synth::click_track(sr, 6.0, 120.0, 0.5, 4);
        let env = onset_envelope(&buf, sr, &mut |_| {});
        let onsets = pick_onsets(&env);
        // clicks at 0.5, 1.0, ..., 5.5
        assert_eq!(onsets.len(), 11, "{onsets:?}");
        for (k, o) in onsets.iter().enumerate() {
            let expected = 0.5 + k as f64 * 0.5;
            assert!(
                (o.time - expected).abs() < 0.01,
                "onset {k}: {} vs {expected}",
                o.time
            );
        }
    }

    #[test]
    fn energetic_window_finds_loud_part() {
        let sr = 22_050;
        let mut buf = vec![0.0f32; sr as usize * 20];
        synth::add_noise(&mut buf[sr as usize * 12..sr as usize * 16], 0.8, 3);
        let env = onset_envelope(&buf, sr, &mut |_| {});
        let start = most_energetic_window(&env, 4.0);
        assert!((start - 12.0).abs() < 0.2, "{start}");
    }
}

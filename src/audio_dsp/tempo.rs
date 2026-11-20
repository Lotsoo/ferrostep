//! Tempo (BPM) and beat phase (offset) estimation. Assumes a constant BPM.

use std::f64::consts::TAU;

use super::onset::{Onset, OnsetEnvelope};

pub const MIN_BPM: f64 = 70.0;
pub const MAX_BPM: f64 = 200.0;
/// Center of the tempo preference (reduces double/half-tempo mistakes).
const PREFERRED_BPM: f64 = 120.0;
/// Width of the preference in octaves.
const PREFERENCE_WIDTH: f64 = 0.9;

/// Result of tempo detection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoResult {
    pub bpm: f64,
    /// Time (seconds) of the first beat in the audio, in `0..period`.
    pub first_beat: f64,
    /// StepMania `#OFFSET` (= −`first_beat`).
    pub offset: f64,
    /// 0..=1, how clearly the beat stands out.
    pub confidence: f32,
}

fn preference(bpm: f64) -> f64 {
    let octaves = (bpm / PREFERRED_BPM).log2() / PREFERENCE_WIDTH;
    (-0.5 * octaves * octaves).exp()
}

/// Mean-removed, half-wave rectified, normalized onset function.
fn prepared_odf(env: &OnsetEnvelope) -> Vec<f64> {
    let n = env.flux.len();
    let half = (env.frame_rate() * 0.25) as usize;
    let mut prefix = vec![0.0f64; n + 1];
    for (i, v) in env.flux.iter().enumerate() {
        prefix[i + 1] = prefix[i] + f64::from(*v);
    }
    let out: Vec<f64> = (0..n)
        .map(|i| {
            let lo = i.saturating_sub(half);
            let hi = (i + half + 1).min(n);
            let local = (prefix[hi] - prefix[lo]) / (hi - lo) as f64;
            (f64::from(env.flux[i]) - local).max(0.0)
        })
        .collect();
    let max = out.iter().copied().fold(0.0, f64::max).max(1e-12);
    out.into_iter().map(|v| v / max).collect()
}

/// Coarse BPM from the weighted autocorrelation of the onset function.
fn coarse_bpm(odf: &[f64], frame_rate: f64) -> Option<(f64, f32)> {
    let n = odf.len();
    let lag_min = (60.0 * frame_rate / MAX_BPM).floor() as usize;
    let lag_max = (60.0 * frame_rate / MIN_BPM).ceil() as usize + 1;
    if n <= lag_max * 2 {
        return None;
    }
    let ac = |lag: usize| -> f64 {
        odf[..n - lag]
            .iter()
            .zip(&odf[lag..])
            .map(|(a, b)| a * b)
            .sum::<f64>()
            / (n - lag) as f64
    };
    let raw: Vec<f64> = (lag_min - 1..=lag_max + 1).map(ac).collect();
    let weighted: Vec<f64> = raw
        .iter()
        .enumerate()
        .map(|(k, r)| {
            let lag = (lag_min - 1 + k) as f64;
            r * preference(60.0 * frame_rate / lag)
        })
        .collect();
    let (best_k, _) = weighted[1..weighted.len() - 1]
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(k, v)| (k + 1, *v))?;
    // parabolic interpolation on the raw autocorrelation
    let (a, b, c) = (raw[best_k - 1], raw[best_k], raw[best_k + 1]);
    let denom = a - 2.0 * b + c;
    let shift = if denom.abs() > 1e-12 {
        (0.5 * (a - c) / denom).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    let lag = (lag_min - 1 + best_k) as f64 + shift;
    let mean = raw.iter().sum::<f64>() / raw.len() as f64;
    let confidence = if b > 0.0 {
        ((b - mean) / b).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    Some((60.0 * frame_rate / lag, confidence))
}

/// Strength of beat alignment for period `period` (phase coherence of the onset function).
fn coherence(times: &[(f64, f64)], period: f64) -> (f64, f64) {
    let (mut re, mut im) = (0.0, 0.0);
    for &(t, w) in times {
        let a = TAU * t / period;
        re += w * a.cos();
        im += w * a.sin();
    }
    ((re * re + im * im).sqrt(), im.atan2(re))
}

/// Refines the BPM around `bpm0` by maximizing phase coherence.
fn refine_bpm(times: &[(f64, f64)], bpm0: f64) -> f64 {
    let sweep = |center: f64, span: f64, step: f64| -> f64 {
        let steps = (span / step).round() as i64;
        (-steps..=steps)
            .map(|k| center + k as f64 * step)
            .filter(|b| *b > 0.0)
            .max_by(|a, b| {
                coherence(times, 60.0 / *a)
                    .0
                    .total_cmp(&coherence(times, 60.0 / *b).0)
            })
            .unwrap_or(center)
    };
    let coarse = sweep(bpm0, bpm0 * 0.02, 0.02);
    let fine = sweep(coarse, 0.03, 0.002);
    sweep(fine, 0.003, 0.0002)
}

/// Finds the beat phase (seconds in `0..period`) with 1 ms resolution,
/// maximizing the onset strength that lands on the grid.
///
/// Kick-like (low band) onsets weigh more: in most music the kick marks the
/// beat, while hi-hats often sit on the off-beat.
pub fn best_phase(onsets: &[Onset], period: f64) -> f64 {
    const SIGMA: f64 = 0.010;
    let steps = (period * 1000.0).ceil() as usize;
    (0..steps)
        .map(|ms| ms as f64 / 1000.0)
        .max_by(|a, b| {
            phase_score(onsets, period, *a, SIGMA)
                .total_cmp(&phase_score(onsets, period, *b, SIGMA))
        })
        .unwrap_or(0.0)
}

/// How much an onset counts when locating beats.
fn beat_weight(o: &Onset) -> f64 {
    f64::from(o.low) + 0.3 * f64::from(o.strength)
}

fn phase_score(onsets: &[Onset], period: f64, phase: f64, sigma: f64) -> f64 {
    onsets
        .iter()
        .map(|o| {
            let d = (o.time - phase).rem_euclid(period);
            let d = d.min(period - d);
            beat_weight(o) * (-0.5 * (d / sigma).powi(2)).exp()
        })
        .sum()
}

/// Estimates BPM and first beat. Returns `None` for audio that is too short or silent.
pub fn estimate_tempo(env: &OnsetEnvelope, onsets: &[Onset]) -> Option<TempoResult> {
    let odf = prepared_odf(env);
    let (bpm0, confidence) = coarse_bpm(&odf, env.frame_rate())?;
    // Only frames with some activity matter for coherence; keeps the sweep fast.
    let times: Vec<(f64, f64)> = odf
        .iter()
        .enumerate()
        .filter(|(_, v)| **v > 0.02)
        .map(|(i, v)| (env.frame_time(i as f64), *v))
        .collect();
    let mut bpm = refine_bpm(&times, bpm0);
    if (bpm - bpm.round()).abs() < 0.05 {
        bpm = bpm.round();
    } else {
        bpm = (bpm * 100.0).round() / 100.0;
    }
    let period = 60.0 / bpm;
    let first_beat = best_phase(onsets, period);
    Some(TempoResult {
        bpm,
        first_beat,
        offset: -first_beat,
        confidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preference_peaks_at_120() {
        assert!(preference(120.0) > preference(60.0));
        assert!(preference(120.0) > preference(240.0));
    }

    #[test]
    fn phase_search_finds_grid() {
        let onsets: Vec<Onset> = (0..20)
            .map(|k| Onset {
                time: 0.237 + k as f64 * 0.5,
                strength: 1.0,
                band: super::super::onset::Band::Low,
                low: 1.0,
                high: 0.0,
                sustain: 0.0,
            })
            .collect();
        let phase = best_phase(&onsets, 0.5);
        assert!((phase - 0.237).abs() < 0.0015, "{phase}");
    }
}

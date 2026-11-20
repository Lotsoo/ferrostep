//! Audio analysis: decoding, onsets, tempo and offset detection.
//!
//! Analysis functions take `&[f32]` samples and a sample rate and do no I/O
//! (except [`decode`], which reads files).

pub mod decode;
pub mod onset;
pub mod synth;
pub mod tempo;

use onset::{Onset, OnsetEnvelope, most_energetic_window, onset_envelope, pick_onsets};
use tempo::{TempoResult, estimate_tempo};

/// Everything the chart generator needs from the audio.
#[derive(Debug, Clone)]
pub struct Analysis {
    pub onsets: Vec<Onset>,
    pub tempo: TempoResult,
    pub duration: f64,
    /// Good start for the song-select preview (seconds).
    pub sample_start: f64,
}

/// Errors from analysis.
#[derive(Debug, thiserror::Error)]
pub enum AnalysisError {
    #[error("lagu terlalu pendek untuk dianalisis (minimal 10 detik)")]
    TooShort,
    #[error("tidak ada ketukan yang jelas di lagu ini")]
    NoBeat,
}

/// Runs the whole analysis. `progress` receives 0..=1.
pub fn analyze(
    samples: &[f32],
    sample_rate: u32,
    progress: &mut dyn FnMut(f32),
) -> Result<Analysis, AnalysisError> {
    let duration = samples.len() as f64 / f64::from(sample_rate.max(1));
    if duration < 10.0 {
        return Err(AnalysisError::TooShort);
    }
    let env: OnsetEnvelope = onset_envelope(samples, sample_rate, &mut |p| progress(p * 0.8));
    let onsets = pick_onsets(&env);
    progress(0.85);
    if onsets.len() < 8 {
        return Err(AnalysisError::NoBeat);
    }
    let tempo = estimate_tempo(&env, &onsets).ok_or(AnalysisError::NoBeat)?;
    progress(0.97);
    let sample_start = most_energetic_window(&env, 15.0);
    progress(1.0);
    Ok(Analysis {
        onsets,
        tempo,
        duration,
        sample_start,
    })
}

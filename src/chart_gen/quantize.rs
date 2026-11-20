//! Snaps onsets to the beat grid and scores the candidate positions.

use std::collections::BTreeMap;

use crate::audio_dsp::onset::Onset;

/// Rows per beat on the finest grid (192nd notes), used as a map key.
pub const ROWS_PER_BEAT: f64 = 48.0;

/// A possible note position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    /// Quantized beat.
    pub beat: f64,
    /// Time in seconds of the quantized beat.
    pub time: f64,
    /// Selection priority (higher = more likely to become a note).
    pub score: f32,
    pub strength: f32,
    pub low: f32,
    pub sustain: f64,
    /// Subdivision of the position: 1 = on the beat, 2 = 8th, 4 = 16th.
    pub subdivision: u32,
}

/// Grid parameters for one difficulty.
#[derive(Debug, Clone, Copy)]
pub struct Grid {
    /// Steps per beat (1 = quarter notes, 2 = eighths, 4 = sixteenths).
    pub per_beat: u32,
    /// Onsets at least this strong may use the 16th grid even if `per_beat` is 2.
    pub strong_sixteenth: Option<f32>,
}

/// Which subdivision a beat position falls on.
pub fn subdivision_of(beat: f64) -> u32 {
    let rows = (beat * ROWS_PER_BEAT).round() as i64;
    let in_beat = rows.rem_euclid(48);
    if in_beat == 0 {
        1
    } else if in_beat % 24 == 0 {
        2
    } else if in_beat % 12 == 0 {
        4
    } else {
        8
    }
}

fn position_weight(sub: u32) -> f32 {
    match sub {
        1 => 1.0,
        2 => 0.8,
        4 => 0.6,
        _ => 0.4,
    }
}

/// Builds candidates: onsets snapped to the grid, merged when they share a slot.
///
/// `beat0` is the time (s) of beat 0; onsets before `earliest` (s) are skipped.
pub fn candidates(
    onsets: &[Onset],
    bpm: f64,
    beat0: f64,
    earliest: f64,
    latest: f64,
    grid: Grid,
) -> Vec<Candidate> {
    let spb = 60.0 / bpm;
    let mut slots: BTreeMap<i64, Candidate> = BTreeMap::new();
    for o in onsets {
        if o.time < earliest || o.time > latest {
            continue;
        }
        let beat = (o.time - beat0) / spb;
        let per_beat = match grid.strong_sixteenth {
            Some(min) if o.strength >= min => 4,
            _ => grid.per_beat,
        };
        let step = 1.0 / f64::from(per_beat);
        let q = (beat / step).round() * step;
        let q_time = beat0 + q * spb;
        if q < 0.0 || q_time < earliest || q_time > latest {
            continue;
        }
        let err = ((beat - q).abs() / (0.5 * step)) as f32; // 0 = exactly on grid
        let sub = subdivision_of(q);
        let score = o.strength.max(o.low) * position_weight(sub) * (1.0 - 0.5 * err.min(1.0));
        let cand = Candidate {
            beat: q,
            time: q_time,
            score,
            strength: o.strength,
            low: o.low,
            sustain: o.sustain,
            subdivision: sub,
        };
        let key = (q * ROWS_PER_BEAT).round() as i64;
        slots
            .entry(key)
            .and_modify(|c| {
                if cand.score > c.score {
                    *c = cand;
                }
            })
            .or_insert(cand);
    }
    slots.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_dsp::onset::Band;

    fn onset(time: f64, strength: f32) -> Onset {
        Onset {
            time,
            strength,
            band: Band::Mixed,
            low: 0.0,
            high: 0.0,
            sustain: 0.0,
        }
    }

    #[test]
    fn snaps_and_merges() {
        let grid = Grid {
            per_beat: 2,
            strong_sixteenth: None,
        };
        // 120 BPM, beat 0 at 1.0 s: 2.01 s ≈ beat 2, 2.26 s ≈ beat 2.5
        let c = candidates(
            &[onset(2.01, 0.5), onset(1.99, 0.9), onset(2.26, 0.4)],
            120.0,
            1.0,
            1.0,
            100.0,
            grid,
        );
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].beat, 2.0);
        assert_eq!(c[0].strength, 0.9, "strongest onset wins the slot");
        assert_eq!(c[1].beat, 2.5);
        assert_eq!(c[1].subdivision, 2);
    }

    #[test]
    fn subdivisions() {
        assert_eq!(subdivision_of(3.0), 1);
        assert_eq!(subdivision_of(3.5), 2);
        assert_eq!(subdivision_of(3.25), 4);
    }
}

//! Automatic 4-panel charts from onsets + tempo, for Beginner, Easy, Medium and Hard.

pub mod patterns;
pub mod quantize;

use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::audio_dsp::onset::Onset;
use crate::parser::writer::DEFAULT_RADAR;
use crate::parser::{Chart, Difficulty, Note, NoteKind};
use patterns::{FootState, Rules};
use quantize::{Candidate, Grid, candidates};

/// No notes before this many seconds of audio.
pub const LEAD_SECONDS: f64 = 1.0;

/// Tunable settings for one difficulty.
#[derive(Debug, Clone, Copy)]
pub struct Profile {
    pub difficulty: Difficulty,
    pub grid: Grid,
    /// Maximum average notes per second.
    pub max_nps: f64,
    /// Minimum time between two notes (seconds).
    pub min_gap: f64,
    /// Chance of a jump on very strong on-beat onsets (0 = never).
    pub jump_chance: f64,
    /// Onset strength needed for a jump.
    pub jump_strength: f32,
    pub holds: bool,
    pub max_jack: usize,
    /// 0..=1, how much the feet like to move.
    pub movement: f64,
}

impl Profile {
    pub fn rules(&self) -> Rules {
        Rules {
            max_jack: self.max_jack,
            allow_jumps: self.jump_chance > 0.0,
            allow_holds: self.holds,
        }
    }
}

/// Default profiles (CLAUDE.md §5.5). Change the numbers here to tune difficulty.
pub const PROFILES: [Profile; 4] = [
    Profile {
        difficulty: Difficulty::Beginner,
        grid: Grid {
            per_beat: 1,
            strong_sixteenth: None,
        },
        max_nps: 1.5,
        min_gap: 0.4,
        jump_chance: 0.0,
        jump_strength: 1.1,
        holds: false,
        max_jack: 2,
        movement: 0.3,
    },
    Profile {
        difficulty: Difficulty::Easy,
        grid: Grid {
            per_beat: 2,
            strong_sixteenth: None,
        },
        max_nps: 2.5,
        min_gap: 0.22,
        jump_chance: 0.12,
        jump_strength: 0.85,
        holds: false,
        max_jack: 2,
        movement: 0.5,
    },
    Profile {
        difficulty: Difficulty::Medium,
        grid: Grid {
            per_beat: 2,
            strong_sixteenth: Some(0.75),
        },
        max_nps: 4.5,
        min_gap: 0.14,
        jump_chance: 0.3,
        jump_strength: 0.75,
        holds: true,
        max_jack: 2,
        movement: 0.65,
    },
    Profile {
        difficulty: Difficulty::Hard,
        grid: Grid {
            per_beat: 4,
            strong_sixteenth: None,
        },
        max_nps: 7.0,
        min_gap: 0.1,
        jump_chance: 0.45,
        jump_strength: 0.65,
        holds: true,
        max_jack: 3,
        movement: 0.8,
    },
];

/// What the generator needs to know about the song.
#[derive(Debug, Clone, Copy)]
pub struct SongInfo<'a> {
    pub onsets: &'a [Onset],
    pub bpm: f64,
    /// StepMania `#OFFSET` (beat 0 is at `-offset` seconds).
    pub offset: f64,
    pub duration: f64,
    /// Seed from the audio hash: same song → same chart.
    pub seed: u64,
}

impl SongInfo<'_> {
    fn spb(&self) -> f64 {
        60.0 / self.bpm
    }

    fn beat0(&self) -> f64 {
        -self.offset
    }

    pub fn beat_to_time(&self, beat: f64) -> f64 {
        self.beat0() + beat * self.spb()
    }
}

/// Generates all four charts.
pub fn generate_all(song: &SongInfo) -> Vec<Chart> {
    PROFILES.iter().map(|p| generate(song, p)).collect()
}

/// Keeps the best-scoring candidates within the NPS budget and spacing rules.
fn select(cands: &[Candidate], profile: &Profile) -> Vec<Candidate> {
    let (Some(first), Some(last)) = (cands.first(), cands.last()) else {
        return Vec::new();
    };
    let active = (last.time - first.time).max(1.0);
    let budget = ((profile.max_nps * 0.92) * active).floor() as usize;
    let mut order: Vec<&Candidate> = cands.iter().collect();
    order.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.time.total_cmp(&b.time)));
    let mut chosen: Vec<Candidate> = Vec::with_capacity(budget);
    for c in order {
        if chosen.len() >= budget {
            break;
        }
        let too_close = chosen
            .iter()
            .any(|x| (x.time - c.time).abs() < profile.min_gap);
        if !too_close {
            chosen.push(*c);
        }
    }
    chosen.sort_by(|a, b| a.beat.total_cmp(&b.beat));
    chosen
}

/// Generates one chart.
pub fn generate(song: &SongInfo, profile: &Profile) -> Chart {
    let seed = song.seed ^ (profile.difficulty as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let earliest = LEAD_SECONDS.max(song.beat0());
    let cands = candidates(
        song.onsets,
        song.bpm,
        song.beat0(),
        earliest,
        song.duration - 0.5,
        profile.grid,
    );
    let picked = select(&cands, profile);
    let max_beat = (song.duration - 0.3 - song.beat0()) / song.spb();

    let mut feet = FootState::default();
    let mut notes: Vec<Note> = Vec::new();
    let mut last_beat = f64::NEG_INFINITY;
    for (k, c) in picked.iter().enumerate() {
        feet.release_holds(c.beat);
        let gap = c.beat - last_beat;
        let can_jump = profile.jump_chance > 0.0
            && !feet.holding()
            && c.subdivision == 1
            && c.strength.max(c.low) >= profile.jump_strength
            && gap >= 1.0;
        if can_jump && rng.random_bool(profile.jump_chance) {
            let (a, b) = feet.jump(&mut rng);
            notes.push(Note {
                beat: c.beat,
                column: a,
                kind: NoteKind::Tap,
            });
            notes.push(Note {
                beat: c.beat,
                column: b,
                kind: NoteKind::Tap,
            });
        } else {
            let (foot, col) = feet.step(&mut rng, profile.movement);
            let next_beat = picked.get(k + 1).map_or(f64::INFINITY, |n| n.beat);
            // Only one hold at a time: the other foot must stay free.
            let kind = if feet.holding() {
                NoteKind::Tap
            } else {
                hold_kind(song, profile, c, next_beat, max_beat, &mut rng)
            };
            if let NoteKind::HoldHead { end_beat } = kind {
                feet.hold = Some((foot, col, end_beat));
            }
            notes.push(Note {
                beat: c.beat,
                column: col,
                kind,
            });
        }
        last_beat = c.beat;
    }

    let mut chart = Chart {
        steps_type: "dance-single".into(),
        description: "Ferrostep".into(),
        difficulty: profile.difficulty,
        meter: 1,
        radar: DEFAULT_RADAR.into(),
        columns: 4,
        notes,
        timing_override: None,
    };
    chart.meter = estimate_meter(&chart, song);
    chart
}

/// Decides whether a note becomes a hold, and for how long.
fn hold_kind(
    song: &SongInfo,
    profile: &Profile,
    c: &Candidate,
    next_beat: f64,
    max_beat: f64,
    rng: &mut ChaCha8Rng,
) -> NoteKind {
    if !profile.holds {
        return NoteKind::Tap;
    }
    let sustain_beats = c.sustain / song.spb();
    // Holds need a long, stable sound and room before the next note for the other foot.
    if sustain_beats < 1.0 || c.strength < 0.35 || next_beat - c.beat < 0.5 || !rng.random_bool(0.5)
    {
        return NoteKind::Tap;
    }
    let length = ((sustain_beats * 2.0).round() / 2.0).clamp(1.0, 4.0);
    let end_beat = (c.beat + length).min(max_beat);
    if end_beat - c.beat < 1.0 {
        return NoteKind::Tap;
    }
    NoteKind::HoldHead { end_beat }
}

/// Difficulty number 1..=10 from average and peak notes per second.
pub fn estimate_meter(chart: &Chart, song: &SongInfo) -> u32 {
    let times: Vec<f64> = chart
        .notes
        .iter()
        .map(|n| song.beat_to_time(n.beat))
        .collect();
    if times.len() < 2 {
        return 1;
    }
    let rows: Vec<f64> = {
        let mut r = times.clone();
        r.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        r
    };
    let span = (rows[rows.len() - 1] - rows[0]).max(1.0);
    let avg = rows.len() as f64 / span;
    let window = 4.0;
    let mut peak = 0usize;
    let mut j = 0;
    for i in 0..rows.len() {
        while rows[i] - rows[j] > window {
            j += 1;
        }
        peak = peak.max(i - j + 1);
    }
    let peak_nps = peak as f64 / window;
    (0.4 + 0.9 * avg + 0.35 * peak_nps).round().clamp(1.0, 10.0) as u32
}

/// Average notes (rows) per second, used by tests and the import summary.
pub fn average_nps(chart: &Chart, song: &SongInfo) -> f64 {
    let mut times: Vec<f64> = chart
        .notes
        .iter()
        .map(|n| song.beat_to_time(n.beat))
        .collect();
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    if times.len() < 2 {
        return 0.0;
    }
    times.len() as f64 / (times[times.len() - 1] - times[0]).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_dsp::onset::Band;
    use crate::audio_dsp::synth::Noise;
    use patterns::Foot;

    /// Random onsets with random strengths and sustains.
    fn random_onsets(seed: u64, count: usize, duration: f64) -> Vec<Onset> {
        let mut n = Noise::new(seed);
        let mut v: Vec<Onset> = (0..count)
            .map(|_| {
                let time = (f64::from(n.next_f32()) * 0.5 + 0.5) * duration;
                let strength = n.next_f32() * 0.5 + 0.5;
                Onset {
                    time,
                    strength,
                    band: Band::Mixed,
                    low: (n.next_f32() * 0.5 + 0.5),
                    high: 0.5,
                    sustain: f64::from(n.next_f32() * 0.5 + 0.5) * 2.0,
                }
            })
            .collect();
        v.sort_by(|a, b| a.time.total_cmp(&b.time));
        v
    }

    #[test]
    fn charts_follow_ergonomic_rules() {
        for seed in 1..=25u64 {
            let onsets = random_onsets(seed, 900, 120.0);
            let bpm = 90.0 + (seed as f64 * 7.3) % 100.0;
            let song = SongInfo {
                onsets: &onsets,
                bpm,
                offset: -0.37,
                duration: 120.0,
                seed,
            };
            for (profile, chart) in PROFILES.iter().zip(generate_all(&song)) {
                let v = patterns::violations(&chart, profile.rules(), |b| song.beat_to_time(b));
                assert!(v.is_empty(), "seed {seed} {:?}: {v:?}", profile.difficulty);
                let nps = average_nps(&chart, &song);
                assert!(
                    nps <= profile.max_nps + 1e-9,
                    "{:?} nps {nps}",
                    profile.difficulty
                );
                assert!(
                    chart.notes.windows(2).all(|w| w[0].beat <= w[1].beat),
                    "sorted"
                );
                assert!(!chart.notes.is_empty());
            }
        }
    }

    #[test]
    fn no_crossovers_feet_model() {
        let mut rng = ChaCha8Rng::seed_from_u64(5);
        let mut feet = FootState::default();
        for _ in 0..2000 {
            let (foot, col) = feet.step(&mut rng, 0.9);
            match foot {
                Foot::Left => assert_ne!(col, patterns::RIGHT),
                Foot::Right => assert_ne!(col, patterns::LEFT),
            }
        }
    }

    #[test]
    fn deterministic_for_same_seed() {
        let onsets = random_onsets(3, 500, 90.0);
        let song = SongInfo {
            onsets: &onsets,
            bpm: 128.0,
            offset: 0.0,
            duration: 90.0,
            seed: 42,
        };
        assert_eq!(generate_all(&song), generate_all(&song));
        let other = SongInfo { seed: 43, ..song };
        assert_ne!(generate_all(&song)[3].notes, generate_all(&other)[3].notes);
    }

    #[test]
    fn difficulty_increases() {
        let onsets = random_onsets(9, 1200, 120.0);
        let song = SongInfo {
            onsets: &onsets,
            bpm: 140.0,
            offset: 0.0,
            duration: 120.0,
            seed: 1,
        };
        let charts = generate_all(&song);
        let counts: Vec<usize> = charts.iter().map(|c| c.notes.len()).collect();
        assert!(counts.windows(2).all(|w| w[0] < w[1]), "{counts:?}");
        let meters: Vec<u32> = charts.iter().map(|c| c.meter).collect();
        assert!(meters.windows(2).all(|w| w[0] <= w[1]), "{meters:?}");
        assert!(
            charts[0].notes.iter().all(|n| n.beat.fract() == 0.0),
            "beginner only on beats"
        );
    }
}

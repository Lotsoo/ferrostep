//! Generates the synthetic metronome test song (`--make-test-song`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::audio_dsp::synth;
use crate::parser::writer::{DEFAULT_RADAR, write_sm};
use crate::parser::{Chart, Difficulty, Metadata, Note, NoteKind, Simfile, TimingData};

const SAMPLE_RATE: u32 = 44_100;
const BPM: f64 = 120.0;
/// Time of beat 0 in the audio (seconds).
const FIRST_BEAT: f64 = 1.0;
const BEATS: u32 = 72;

/// Writes `Songs/_Test/Metronome/` (WAV + `.sm`) and returns the folder.
pub fn make_test_song(songs_dir: &Path) -> Result<PathBuf> {
    let dir = songs_dir.join("_Test").join("Metronome");
    std::fs::create_dir_all(&dir).with_context(|| format!("could not create {}", dir.display()))?;

    let duration = FIRST_BEAT + f64::from(BEATS) * 60.0 / BPM + 2.0;
    let samples = synth::click_track(SAMPLE_RATE, duration, BPM, FIRST_BEAT, 4);
    write_wav(&dir.join("metronome.wav"), &samples, SAMPLE_RATE)?;

    let sm = write_sm(&test_simfile());
    std::fs::write(dir.join("metronome.sm"), sm).context("could not write metronome.sm")?;
    Ok(dir)
}

/// Writes mono 16-bit PCM WAV.
pub fn write_wav(path: &Path, samples: &[f32], sample_rate: u32) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .with_context(|| format!("could not create {}", path.display()))?;
    for &s in samples {
        writer.write_sample((s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16)?;
    }
    writer.finalize()?;
    Ok(())
}

fn tap(beat: f64, column: u8) -> Note {
    Note {
        beat,
        column,
        kind: NoteKind::Tap,
    }
}

fn chart(difficulty: Difficulty, meter: u32, notes: Vec<Note>) -> Chart {
    Chart {
        steps_type: "dance-single".into(),
        description: "Metronome".into(),
        difficulty,
        meter,
        radar: DEFAULT_RADAR.into(),
        columns: 4,
        notes,
        timing_override: None,
    }
}

fn test_simfile() -> Simfile {
    let pattern = [0u8, 1, 2, 3, 2, 1];
    let beginner: Vec<Note> = (4..BEATS - 2)
        .map(|b| tap(f64::from(b), pattern[b as usize % pattern.len()]))
        .collect();

    let easy: Vec<Note> = (8..(BEATS - 2) * 2)
        .filter(|h| h % 2 == 0 || (*h >= 64 && h % 4 == 3))
        .map(|h| tap(f64::from(h) / 2.0, pattern[h as usize % pattern.len()]))
        .filter(|n| n.beat >= 4.0)
        .collect();

    let mut medium = Vec::new();
    for b in (4..BEATS - 4).step_by(4) {
        let b = f64::from(b);
        if (b as u32 / 4).is_multiple_of(3) {
            medium.push(Note {
                beat: b,
                column: 0,
                kind: NoteKind::HoldHead { end_beat: b + 2.0 },
            });
            medium.push(tap(b + 1.0, 3));
            medium.push(tap(b + 2.0, 2));
            medium.push(Note {
                beat: b + 3.0,
                column: 1,
                kind: NoteKind::Mine,
            });
            medium.push(tap(b + 3.5, 3));
        } else {
            for (i, col) in [0u8, 1, 2, 3, 3, 2, 1, 0].into_iter().enumerate() {
                medium.push(tap(b + i as f64 / 2.0, col));
            }
        }
    }
    medium.sort_by(|a, b| a.beat.total_cmp(&b.beat).then(a.column.cmp(&b.column)));

    Simfile {
        meta: Metadata {
            title: "Metronome".into(),
            subtitle: "Lagu uji sinkronisasi".into(),
            artist: "Ferrostep".into(),
            credit: "Ferrostep Test".into(),
            music: "metronome.wav".into(),
            sample_start: 2.0,
            sample_length: 12.0,
            ..Default::default()
        },
        timing: TimingData {
            offset: -FIRST_BEAT,
            bpms: vec![(0.0, BPM)],
            stops: vec![],
        },
        charts: vec![
            chart(Difficulty::Beginner, 1, beginner),
            chart(Difficulty::Easy, 3, easy),
            chart(Difficulty::Medium, 5, medium),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::sm;

    #[test]
    fn test_simfile_survives_round_trip() {
        let sim = test_simfile();
        let parsed = sm::parse(&write_sm(&sim)).unwrap();
        assert_eq!(parsed.charts, sim.charts);
        assert_eq!(parsed.timing, sim.timing);
    }
}

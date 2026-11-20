//! Writes a [`Simfile`] as a standard `.sm` file readable by StepMania 5.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::{Chart, NoteKind, Simfile};

/// Radar values written for generated charts (StepMania recomputes them).
pub const DEFAULT_RADAR: &str = "0.000,0.000,0.000,0.000,0.000";

/// Finest grid: 192 rows per measure = 48 rows per beat.
const ROWS_PER_MEASURE: i64 = 192;
const ROWS_PER_BEAT: f64 = 48.0;
/// Allowed row counts per measure, smallest first.
const RESOLUTIONS: [i64; 10] = [4, 8, 12, 16, 24, 32, 48, 64, 96, 192];

/// Produces the full `.sm` text.
pub fn write_sm(sim: &Simfile) -> String {
    let mut out = String::new();
    let m = &sim.meta;
    let tag = |out: &mut String, name: &str, value: &str| {
        let _ = writeln!(out, "#{name}:{};", escape(value));
    };
    tag(&mut out, "TITLE", &m.title);
    tag(&mut out, "SUBTITLE", &m.subtitle);
    tag(&mut out, "ARTIST", &m.artist);
    tag(&mut out, "CREDIT", &m.credit);
    tag(&mut out, "BANNER", &m.banner);
    tag(&mut out, "BACKGROUND", &m.background);
    tag(&mut out, "MUSIC", &m.music);
    tag(&mut out, "OFFSET", &format_num(sim.timing.offset));
    tag(&mut out, "SAMPLESTART", &format_num(m.sample_start));
    tag(&mut out, "SAMPLELENGTH", &format_num(m.sample_length));
    tag(&mut out, "BPMS", &format_pairs(&sim.timing.bpms));
    tag(&mut out, "STOPS", &format_pairs(&sim.timing.stops));
    for (name, value) in &m.extra {
        tag(&mut out, name, value);
    }

    for chart in &sim.charts {
        write_chart(&mut out, chart);
    }
    out
}

/// `#`, `:` and `;` would break the tag syntax, so they are dropped from text values.
fn escape(value: &str) -> String {
    value
        .chars()
        .filter(|c| !matches!(c, '#' | ';' | ':'))
        .collect()
}

fn format_num(v: f64) -> String {
    format!("{v:.6}")
}

fn format_pairs(pairs: &[(f64, f64)]) -> String {
    pairs
        .iter()
        .map(|(b, v)| format!("{}={}", format_num(*b), format_num(*v)))
        .collect::<Vec<_>>()
        .join(",")
}

fn write_chart(out: &mut String, chart: &Chart) {
    let _ = writeln!(
        out,
        "\n//---------------{} - {}----------------",
        chart.steps_type,
        escape(&chart.description)
    );
    let _ = writeln!(out, "#NOTES:");
    let _ = writeln!(out, "     {}:", chart.steps_type);
    let _ = writeln!(out, "     {}:", escape(&chart.description));
    let _ = writeln!(out, "     {}:", chart.difficulty.as_sm_str());
    let _ = writeln!(out, "     {}:", chart.meter);
    let _ = writeln!(out, "     {}:", chart.radar);
    out.push_str(&note_data(chart));
    out.push_str(";\n");
}

/// Converts a beat to a row on the 192-per-measure grid.
fn beat_to_row(beat: f64) -> i64 {
    (beat * ROWS_PER_BEAT).round() as i64
}

/// Builds the measures block (`,`-separated) with the smallest fitting resolution.
fn note_data(chart: &Chart) -> String {
    let cols = usize::from(chart.columns);
    // row -> characters for that row
    let mut cells: BTreeMap<i64, Vec<u8>> = BTreeMap::new();
    let mut put = |row: i64, col: u8, ch: u8| {
        if row < 0 {
            log::warn!("dropping note before beat 0");
            return;
        }
        let line = cells.entry(row).or_insert_with(|| vec![b'0'; cols]);
        if let Some(cell) = line.get_mut(usize::from(col)) {
            *cell = ch;
        }
    };
    for note in &chart.notes {
        let row = beat_to_row(note.beat);
        match note.kind {
            NoteKind::Tap => put(row, note.column, b'1'),
            NoteKind::Mine => put(row, note.column, b'M'),
            NoteKind::Lift => put(row, note.column, b'L'),
            NoteKind::Fake => put(row, note.column, b'F'),
            NoteKind::HoldHead { end_beat } | NoteKind::RollHead { end_beat } => {
                let head = if matches!(note.kind, NoteKind::HoldHead { .. }) {
                    b'2'
                } else {
                    b'4'
                };
                put(row, note.column, head);
                put(beat_to_row(end_beat).max(row + 1), note.column, b'3');
            }
        }
    }

    let last_row = cells.keys().next_back().copied().unwrap_or(0);
    let measures = last_row / ROWS_PER_MEASURE + 1;
    let empty = "0".repeat(cols);
    let mut out = String::new();
    for measure in 0..measures {
        if measure > 0 {
            out.push_str(",\n");
        }
        let start = measure * ROWS_PER_MEASURE;
        let used: Vec<i64> = cells
            .range(start..start + ROWS_PER_MEASURE)
            .map(|(r, _)| r - start)
            .collect();
        let rows = RESOLUTIONS
            .into_iter()
            .find(|n| used.iter().all(|r| r % (ROWS_PER_MEASURE / n) == 0))
            .unwrap_or(ROWS_PER_MEASURE);
        let step = ROWS_PER_MEASURE / rows;
        for i in 0..rows {
            match cells.get(&(start + i * step)) {
                Some(line) => out.push_str(&String::from_utf8_lossy(line)),
                None => out.push_str(&empty),
            }
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{Difficulty, Note};

    fn chart(notes: Vec<Note>) -> Chart {
        Chart {
            steps_type: "dance-single".into(),
            description: String::new(),
            difficulty: Difficulty::Easy,
            meter: 2,
            radar: String::new(),
            columns: 4,
            notes,
            timing_override: None,
        }
    }

    #[test]
    fn uses_smallest_resolution() {
        let data = note_data(&chart(vec![
            Note {
                beat: 0.0,
                column: 0,
                kind: NoteKind::Tap,
            },
            Note {
                beat: 1.5,
                column: 1,
                kind: NoteKind::Tap,
            },
        ]));
        // 8 rows: beat 0 and beat 1.5 (row 3)
        assert_eq!(data, "1000\n0000\n0000\n0100\n0000\n0000\n0000\n0000\n");
    }

    #[test]
    fn triplets_use_12_rows() {
        let data = note_data(&chart(vec![Note {
            beat: 1.0 / 3.0,
            column: 2,
            kind: NoteKind::Tap,
        }]));
        assert_eq!(data.lines().count(), 12);
        assert_eq!(data.lines().nth(1), Some("0010"));
    }
}

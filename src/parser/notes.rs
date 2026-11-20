//! Parsing of the measure-based note data block.

use super::{Note, NoteKind};

/// Number of columns for a StepMania steps type. Unknown types default to 4.
pub fn columns_for_steps_type(steps_type: &str) -> u8 {
    match steps_type.trim().to_ascii_lowercase().as_str() {
        "dance-single" => 4,
        "dance-solo" => 6,
        "dance-double" | "dance-couple" => 8,
        "pump-single" => 5,
        "pump-halfdouble" => 6,
        "pump-double" => 10,
        _ => 4,
    }
}

/// Beat of row `row` (of `rows` rows) inside measure `measure`.
///
/// Computed as one division of exact integers, so the same position written
/// with a different row count gives a bit-identical `f64` (needed for round-trips).
pub(crate) fn row_beat(measure: usize, row: usize, rows: usize) -> f64 {
    ((measure * rows + row) * 4) as f64 / rows as f64
}

/// Parses note data (`,`-separated measures of rows) into sorted notes.
///
/// Hold/roll heads without a tail are turned into taps (with a warning).
pub fn parse_note_data(data: &str, columns: u8) -> Vec<Note> {
    let cols = usize::from(columns);
    let mut notes: Vec<Note> = Vec::new();
    // Index into `notes` of the hold/roll currently open in each column.
    let mut open: Vec<Option<usize>> = vec![None; cols];

    for (measure, block) in data.split(',').enumerate() {
        let rows: Vec<&str> = block
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        if rows.is_empty() {
            continue;
        }
        for (r, line) in rows.iter().enumerate() {
            let beat = row_beat(measure, r, rows.len());
            for (c, ch) in line.chars().take(cols).enumerate() {
                let column = c as u8;
                let kind = match ch {
                    '1' => NoteKind::Tap,
                    '2' => NoteKind::HoldHead { end_beat: beat },
                    '4' => NoteKind::RollHead { end_beat: beat },
                    'M' | 'm' => NoteKind::Mine,
                    'L' | 'l' => NoteKind::Lift,
                    'F' | 'f' => NoteKind::Fake,
                    '3' => {
                        close_hold(&mut notes, &mut open[c], beat);
                        continue;
                    }
                    _ => continue,
                };
                if let Some(idx) = open[c].take() {
                    log::warn!(
                        "hold at beat {} has no tail; treating as tap",
                        notes[idx].beat
                    );
                    notes[idx].kind = NoteKind::Tap;
                }
                if matches!(kind, NoteKind::HoldHead { .. } | NoteKind::RollHead { .. }) {
                    open[c] = Some(notes.len());
                }
                notes.push(Note { beat, column, kind });
            }
        }
    }

    for idx in open.into_iter().flatten() {
        log::warn!(
            "hold at beat {} never ends; treating as tap",
            notes[idx].beat
        );
        notes[idx].kind = NoteKind::Tap;
    }
    notes.sort_by(|a, b| a.beat.total_cmp(&b.beat).then(a.column.cmp(&b.column)));
    notes
}

fn close_hold(notes: &mut [Note], open: &mut Option<usize>, beat: f64) {
    if let Some(idx) = open.take() {
        match &mut notes[idx].kind {
            NoteKind::HoldHead { end_beat } | NoteKind::RollHead { end_beat } => *end_beat = beat,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beat_of_rows() {
        assert_eq!(row_beat(0, 0, 4), 0.0);
        assert_eq!(row_beat(0, 1, 4), 1.0);
        assert_eq!(row_beat(2, 3, 8), 9.5);
        assert_eq!(row_beat(0, 1, 12), row_beat(0, 16, 192));
    }

    #[test]
    fn parses_taps_holds_and_mines() {
        let data = "1000\n0200\n0000\n0300\n,\nM001\n0000\n0000\n0000";
        let notes = parse_note_data(data, 4);
        assert_eq!(notes.len(), 4);
        assert_eq!(
            notes[0],
            Note {
                beat: 0.0,
                column: 0,
                kind: NoteKind::Tap
            }
        );
        assert_eq!(
            notes[1],
            Note {
                beat: 1.0,
                column: 1,
                kind: NoteKind::HoldHead { end_beat: 3.0 }
            }
        );
        assert_eq!(notes[2].kind, NoteKind::Mine);
        assert_eq!(notes[2].beat, 4.0);
        assert_eq!(notes[3].column, 3);
    }

    #[test]
    fn unterminated_hold_becomes_tap() {
        let notes = parse_note_data("2000\n0000\n0000\n0000", 4);
        assert_eq!(notes[0].kind, NoteKind::Tap);
    }
}

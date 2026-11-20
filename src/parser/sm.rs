//! Reader for StepMania `.sm` files.

use super::tags::split_tags;
use super::{
    Chart, Difficulty, ParseError, Simfile, columns_for_steps_type, parse_beat_pairs,
    parse_f64_or_zero, parse_note_data,
};

/// Parses the text of a `.sm` file.
pub fn parse(text: &str) -> Result<Simfile, ParseError> {
    let mut sim = Simfile::default();
    for (name, value) in split_tags(text) {
        if name == "NOTES" {
            if let Some(chart) = parse_notes_tag(&value) {
                sim.charts.push(chart);
            }
        } else {
            apply_song_tag(&mut sim, &name, &value);
        }
    }
    finish(sim)
}

/// Stores a song-level tag shared by `.sm` and `.ssc`. Unknown tags go to `meta.extra`.
pub(crate) fn apply_song_tag(sim: &mut Simfile, name: &str, value: &str) {
    let v = value.trim().to_string();
    let m = &mut sim.meta;
    match name {
        "TITLE" => m.title = v,
        "SUBTITLE" => m.subtitle = v,
        "ARTIST" => m.artist = v,
        "CREDIT" => m.credit = v,
        "BANNER" => m.banner = v,
        "BACKGROUND" => m.background = v,
        "MUSIC" => m.music = v,
        "SAMPLESTART" => m.sample_start = parse_f64_or_zero(&v),
        "SAMPLELENGTH" => m.sample_length = parse_f64_or_zero(&v),
        "OFFSET" => sim.timing.offset = parse_f64_or_zero(&v),
        "BPMS" => sim.timing.bpms = parse_beat_pairs(&v),
        "STOPS" | "FREEZES" => sim.timing.stops = parse_beat_pairs(&v),
        _ => m.extra.push((name.to_string(), v)),
    }
}

/// Final validation shared by both readers.
pub(crate) fn finish(sim: Simfile) -> Result<Simfile, ParseError> {
    let has_bpm = !sim.timing.bpms.is_empty()
        || sim.charts.iter().any(|c| {
            c.timing_override
                .as_ref()
                .is_some_and(|t| !t.bpms.is_empty())
        });
    if !has_bpm {
        return Err(ParseError::MissingBpm);
    }
    Ok(sim)
}

/// Parses the 6 `:`-separated fields of a `#NOTES` tag.
fn parse_notes_tag(value: &str) -> Option<Chart> {
    let fields: Vec<&str> = value.splitn(6, ':').collect();
    if fields.len() < 6 {
        log::warn!("skipping #NOTES with {} fields (expected 6)", fields.len());
        return None;
    }
    let steps_type = fields[0].trim().to_string();
    let columns = columns_for_steps_type(&steps_type);
    Some(Chart {
        columns,
        description: fields[1].trim().to_string(),
        difficulty: Difficulty::parse(fields[2]),
        meter: fields[3].trim().parse().unwrap_or(1),
        radar: fields[4].trim().to_string(),
        notes: parse_note_data(fields[5], columns),
        steps_type,
        timing_override: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::NoteKind;

    const SAMPLE: &str = "#TITLE:Test Song;
#ARTIST:Someone;
#MUSIC:song.ogg;
#OFFSET:-0.250;
#BPMS:0.000=120.000,
16.000=150.000;
#STOPS:8.000=0.500;
#CUSTOMTAG:keep me;
#NOTES:
     dance-single:
     :
     Easy:
     3:
     0,0,0,0,0:
1000
0100
0010
0001
,
2000
0000
3000
0000
;
";

    #[test]
    fn parses_header_and_chart() {
        let sim = parse(SAMPLE).expect("valid file");
        assert_eq!(sim.meta.title, "Test Song");
        assert_eq!(sim.meta.music, "song.ogg");
        assert_eq!(sim.timing.offset, -0.25);
        assert_eq!(sim.timing.bpms, vec![(0.0, 120.0), (16.0, 150.0)]);
        assert_eq!(sim.timing.stops, vec![(8.0, 0.5)]);
        assert_eq!(sim.meta.extra, vec![("CUSTOMTAG".into(), "keep me".into())]);

        let chart = &sim.charts[0];
        assert!(chart.is_dance_single());
        assert_eq!(chart.difficulty, Difficulty::Easy);
        assert_eq!(chart.meter, 3);
        assert_eq!(chart.notes.len(), 5);
        assert_eq!(chart.notes[4].kind, NoteKind::HoldHead { end_beat: 6.0 });
    }

    #[test]
    fn missing_bpm_is_an_error() {
        assert!(matches!(parse("#TITLE:x;"), Err(ParseError::MissingBpm)));
    }
}

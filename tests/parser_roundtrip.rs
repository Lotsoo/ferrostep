//! Parser tests on hand-made fixtures, plus parse -> write -> parse round-trips.

use std::path::Path;

use ferrostep::parser::{Difficulty, NoteKind, Simfile, load_simfile, sm, writer::write_sm};
use ferrostep::timing::{TimingMap, time_notes};

fn fixture(name: &str) -> Simfile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    load_simfile(&path).expect("fixture should parse")
}

#[test]
fn weird_sm_parses() {
    let sim = fixture("weird.sm");
    assert_eq!(sim.meta.title, "Weird Fixture");
    assert_eq!(sim.meta.music, "fixture.ogg");
    assert_eq!(sim.timing.offset, -0.123);
    assert_eq!(sim.timing.bpms, vec![(0.0, 128.0), (32.0, 64.0)]);
    assert_eq!(sim.timing.stops, vec![(8.0, 0.25)]);
    assert_eq!(sim.meta.sample_start, 10.5);
    assert!(
        sim.meta
            .extra
            .iter()
            .any(|(k, v)| k == "UNKNOWNTAG" && v == "some value")
    );
    assert_eq!(sim.charts.len(), 2);

    let single = &sim.charts[0];
    assert!(single.is_dance_single());
    assert_eq!(single.difficulty, Difficulty::Medium);
    // 192-row measure: row 16 is beat 4 + 16/48
    assert!(
        single
            .notes
            .iter()
            .any(|n| n.column == 1 && (n.beat - (4.0 + 16.0 / 48.0)).abs() < 1e-12)
    );
    assert!(single.notes.iter().any(|n| n.kind == NoteKind::Mine));
    assert!(
        single
            .notes
            .iter()
            .any(|n| matches!(n.kind, NoteKind::RollHead { end_beat } if end_beat == 10.0))
    );
    assert!(
        single
            .notes
            .iter()
            .any(|n| matches!(n.kind, NoteKind::HoldHead { end_beat } if end_beat == 14.0))
    );

    let double = &sim.charts[1];
    assert_eq!(double.columns, 8);
    assert!(!double.is_dance_single());
    assert_eq!(double.notes[1].column, 7);
}

#[test]
fn ssc_prefers_chart_timing() {
    let sim = fixture("chart_timing.ssc");
    assert_eq!(sim.charts.len(), 2);
    assert_eq!(sim.charts[0].description, "Main");
    assert_eq!(sim.charts[0].difficulty, Difficulty::Challenge);
    let easy = &sim.charts[1];
    let map = TimingMap::new(sim.timing_for(easy));
    let notes = time_notes(easy, &map);
    // 75 BPM = 0.8 s per beat, offset 0: second note at beat 2 = 1.6 s
    assert!((notes[1].time - 1.6).abs() < 1e-9);
}

#[test]
fn round_trip_is_identical() {
    for name in ["weird.sm", "chart_timing.ssc"] {
        let original = fixture(name);
        // .ssc per-chart timing cannot be stored in .sm, so compare only what .sm supports.
        let mut expected = original.clone();
        for chart in &mut expected.charts {
            chart.timing_override = None;
        }
        let written = write_sm(&original);
        let reparsed = sm::parse(&written).expect("written file should parse");
        assert_eq!(reparsed.meta, expected.meta, "{name}: metadata");
        assert_eq!(reparsed.timing, expected.timing, "{name}: timing");
        assert_eq!(reparsed.charts, expected.charts, "{name}: charts");

        // Writing again must give exactly the same text.
        assert_eq!(write_sm(&reparsed), written, "{name}: second write");
    }
}

#[test]
fn timed_notes_respect_stop_and_offset() {
    let sim = fixture("weird.sm");
    let chart = &sim.charts[0];
    let map = TimingMap::new(&sim.timing);
    let notes = time_notes(chart, &map);
    let spb = 60.0 / 128.0;
    // first note: beat 0 at -offset = 0.123 s
    assert!((notes[0].time - 0.123).abs() < 1e-9);
    // a note after the 0.25 s stop at beat 8 (roll head at beat 8 sits ON the stop)
    let roll = notes
        .iter()
        .find(|n| matches!(n.kind, NoteKind::RollHead { .. }))
        .unwrap();
    assert!((roll.time - (0.123 + 8.0 * spb)).abs() < 1e-9);
    assert!((roll.end_time - (0.123 + 10.0 * spb + 0.25)).abs() < 1e-9);
}

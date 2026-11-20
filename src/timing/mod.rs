//! Beat <-> seconds conversion with BPM changes and stops.
//!
//! The segment table is built once; lookups use binary search.

use crate::parser::{Chart, NoteKind, TimingData};

/// Two beats closer than this are treated as the same position.
const BEAT_EPSILON: f64 = 1e-6;

/// One stretch of constant BPM starting at `beat`.
#[derive(Debug, Clone, Copy)]
struct Segment {
    beat: f64,
    /// Time when scrolling *leaves* `beat` (after any stop at `beat`).
    time: f64,
    seconds_per_beat: f64,
    /// Length of the stop at `beat` (0 if none).
    stop: f64,
}

impl Segment {
    /// Time when `beat` is first reached (before its stop).
    fn arrive_time(&self) -> f64 {
        self.time - self.stop
    }
}

/// Precomputed timing for fast beat/second conversion.
#[derive(Debug, Clone)]
pub struct TimingMap {
    segments: Vec<Segment>,
}

impl TimingMap {
    /// Builds the segment table. Non-positive BPMs (warps) are ignored with a warning.
    pub fn new(timing: &TimingData) -> Self {
        let bpms: Vec<(f64, f64)> = timing
            .bpms
            .iter()
            .copied()
            .filter(|&(beat, bpm)| {
                let ok = bpm > 0.0;
                if !ok {
                    log::warn!("ignoring non-positive BPM {bpm} at beat {beat}");
                }
                ok
            })
            .collect();
        let first_bpm = bpms.first().map_or(120.0, |p| p.1);

        // All beats where something changes, starting at beat 0.
        let mut events: Vec<f64> = std::iter::once(0.0)
            .chain(bpms.iter().map(|p| p.0))
            .chain(timing.stops.iter().map(|p| p.0))
            .filter(|b| *b >= 0.0)
            .collect();
        events.sort_by(f64::total_cmp);
        events.dedup_by(|a, b| (*a - *b).abs() < BEAT_EPSILON);

        let mut segments = Vec::with_capacity(events.len());
        let mut beat = 0.0;
        let mut time = -timing.offset;
        let mut spb = 60.0 / first_bpm;
        for event in events {
            time += (event - beat) * spb;
            beat = event;
            if let Some(&(_, bpm)) = bpms.iter().rev().find(|p| p.0 <= event + BEAT_EPSILON) {
                spb = 60.0 / bpm;
            }
            let stop: f64 = timing
                .stops
                .iter()
                .filter(|s| (s.0 - event).abs() < BEAT_EPSILON && s.1 > 0.0)
                .map(|s| s.1)
                .sum();
            time += stop;
            segments.push(Segment {
                beat,
                time,
                seconds_per_beat: spb,
                stop,
            });
        }
        Self { segments }
    }

    /// Time in seconds (relative to the start of the audio) of `beat`.
    ///
    /// A note exactly on a stop happens at the *start* of the stop, as in StepMania.
    pub fn beat_to_seconds(&self, beat: f64) -> f64 {
        let idx = self
            .segments
            .partition_point(|s| s.beat <= beat + BEAT_EPSILON);
        let seg = &self.segments[idx.saturating_sub(1)];
        if (beat - seg.beat).abs() < BEAT_EPSILON {
            seg.arrive_time()
        } else {
            seg.time + (beat - seg.beat) * seg.seconds_per_beat
        }
    }

    /// Beat at `seconds`. During a stop the beat stays constant.
    pub fn seconds_to_beat(&self, seconds: f64) -> f64 {
        let idx = self
            .segments
            .partition_point(|s| s.arrive_time() <= seconds);
        let seg = &self.segments[idx.saturating_sub(1)];
        if seconds < seg.time && seconds >= seg.arrive_time() {
            seg.beat
        } else {
            seg.beat + (seconds - seg.time) / seg.seconds_per_beat
        }
    }

    /// BPM in effect at `beat`.
    pub fn bpm_at(&self, beat: f64) -> f64 {
        let idx = self
            .segments
            .partition_point(|s| s.beat <= beat + BEAT_EPSILON);
        60.0 / self.segments[idx.saturating_sub(1)].seconds_per_beat
    }
}

/// A note with its time already computed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimedNote {
    /// Seconds from the start of the audio.
    pub time: f64,
    pub beat: f64,
    pub column: u8,
    pub kind: NoteKind,
    /// End time for holds/rolls (equal to `time` for other notes).
    pub end_time: f64,
    pub end_beat: f64,
}

/// Converts all notes of a chart to times, sorted by time then column.
pub fn time_notes(chart: &Chart, map: &TimingMap) -> Vec<TimedNote> {
    let mut out: Vec<TimedNote> = chart
        .notes
        .iter()
        .map(|n| {
            let end_beat = match n.kind {
                NoteKind::HoldHead { end_beat } | NoteKind::RollHead { end_beat } => end_beat,
                _ => n.beat,
            };
            TimedNote {
                time: map.beat_to_seconds(n.beat),
                beat: n.beat,
                column: n.column,
                kind: n.kind,
                end_time: map.beat_to_seconds(end_beat),
                end_beat,
            }
        })
        .collect();
    out.sort_by(|a, b| a.time.total_cmp(&b.time).then(a.column.cmp(&b.column)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(offset: f64, bpms: &[(f64, f64)], stops: &[(f64, f64)]) -> TimingMap {
        TimingMap::new(&TimingData {
            offset,
            bpms: bpms.to_vec(),
            stops: stops.to_vec(),
        })
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    #[test]
    fn constant_bpm() {
        let m = map(0.0, &[(0.0, 120.0)], &[]);
        close(m.beat_to_seconds(0.0), 0.0);
        close(m.beat_to_seconds(4.0), 2.0);
        close(m.seconds_to_beat(1.25), 2.5);
    }

    #[test]
    fn offset_sign_convention() {
        // #OFFSET:-0.5 means beat 0 happens 0.5 s INTO the audio.
        let m = map(-0.5, &[(0.0, 60.0)], &[]);
        close(m.beat_to_seconds(0.0), 0.5);
        close(m.beat_to_seconds(1.0), 1.5);
        // positive offset: beat 0 is before the audio starts
        let m = map(0.25, &[(0.0, 60.0)], &[]);
        close(m.beat_to_seconds(0.0), -0.25);
        close(m.seconds_to_beat(0.0), 0.25);
    }

    #[test]
    fn bpm_change() {
        // 4 beats at 120 (2 s), then 150 BPM (0.4 s per beat)
        let m = map(0.0, &[(0.0, 120.0), (4.0, 150.0)], &[]);
        close(m.beat_to_seconds(4.0), 2.0);
        close(m.beat_to_seconds(6.0), 2.8);
        close(m.seconds_to_beat(2.8), 6.0);
        close(m.bpm_at(5.0), 150.0);
    }

    #[test]
    fn stop() {
        // stop of 1 s at beat 2 (120 BPM)
        let m = map(0.0, &[(0.0, 120.0)], &[(2.0, 1.0)]);
        close(m.beat_to_seconds(2.0), 1.0); // note on the stop: start of stop
        close(m.beat_to_seconds(3.0), 2.5); // 1.0 + 1.0 stop + 0.5
        close(m.seconds_to_beat(1.5), 2.0); // frozen during the stop
        close(m.seconds_to_beat(2.5), 3.0);
    }

    #[test]
    fn negative_beats_extrapolate() {
        let m = map(0.0, &[(0.0, 120.0)], &[]);
        close(m.beat_to_seconds(-2.0), -1.0);
        close(m.seconds_to_beat(-1.0), -2.0);
    }
}

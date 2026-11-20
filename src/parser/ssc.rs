//! Reader for StepMania 5 `.ssc` files.

use super::sm::{apply_song_tag, finish};
use super::tags::split_tags;
use super::{
    Chart, Difficulty, ParseError, Simfile, TimingData, columns_for_steps_type, parse_beat_pairs,
    parse_f64_or_zero, parse_note_data,
};

/// Chart fields collected while reading tags after `#NOTEDATA`.
#[derive(Default)]
struct PendingChart {
    steps_type: String,
    description: String,
    difficulty: String,
    meter: String,
    radar: String,
    notes: String,
    offset: Option<f64>,
    bpms: Option<Vec<(f64, f64)>>,
    stops: Option<Vec<(f64, f64)>>,
}

impl PendingChart {
    fn build(self, song_timing: &TimingData) -> Chart {
        let columns = columns_for_steps_type(&self.steps_type);
        let has_override = self.offset.is_some() || self.bpms.is_some() || self.stops.is_some();
        let timing_override = has_override.then(|| TimingData {
            offset: self.offset.unwrap_or(song_timing.offset),
            bpms: self.bpms.unwrap_or_else(|| song_timing.bpms.clone()),
            stops: self.stops.unwrap_or_else(|| song_timing.stops.clone()),
        });
        Chart {
            columns,
            description: self.description,
            difficulty: Difficulty::parse(&self.difficulty),
            meter: self.meter.trim().parse().unwrap_or(1),
            radar: self.radar,
            notes: parse_note_data(&self.notes, columns),
            steps_type: self.steps_type,
            timing_override,
        }
    }
}

/// Parses the text of a `.ssc` file.
pub fn parse(text: &str) -> Result<Simfile, ParseError> {
    let mut sim = Simfile::default();
    let mut pending: Option<PendingChart> = None;

    for (name, value) in split_tags(text) {
        if name == "NOTEDATA" {
            if let Some(chart) = pending.take() {
                sim.charts.push(chart.build(&sim.timing));
            }
            pending = Some(PendingChart::default());
            continue;
        }
        let Some(chart) = pending.as_mut() else {
            apply_song_tag(&mut sim, &name, &value);
            continue;
        };
        let v = value.trim().to_string();
        match name.as_str() {
            "STEPSTYPE" => chart.steps_type = v,
            "DESCRIPTION" | "CHARTNAME" if chart.description.is_empty() => chart.description = v,
            "DIFFICULTY" => chart.difficulty = v,
            "METER" => chart.meter = v,
            "RADARVALUES" => chart.radar = v,
            "NOTES" | "NOTES2" => chart.notes = v,
            "OFFSET" => chart.offset = Some(parse_f64_or_zero(&v)),
            "BPMS" => chart.bpms = Some(parse_beat_pairs(&v)),
            "STOPS" => chart.stops = Some(parse_beat_pairs(&v)),
            _ => {} // other per-chart tags are not needed yet
        }
    }
    if let Some(chart) = pending.take() {
        sim.charts.push(chart.build(&sim.timing));
    }
    finish(sim)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "#VERSION:0.83;
#TITLE:SSC Song;
#OFFSET:0.100;
#BPMS:0=140;
#NOTEDATA:;
#STEPSTYPE:dance-single;
#DIFFICULTY:Hard;
#METER:8;
#NOTES:
1000
0000
0000
0000
;
#NOTEDATA:;
#STEPSTYPE:dance-single;
#DIFFICULTY:Beginner;
#METER:1;
#BPMS:0=70;
#NOTES:
0001
0000
0000
0000
;
";

    #[test]
    fn parses_charts_and_per_chart_timing() {
        let sim = parse(SAMPLE).expect("valid file");
        assert_eq!(sim.meta.title, "SSC Song");
        assert_eq!(sim.charts.len(), 2);
        assert_eq!(sim.charts[0].difficulty, Difficulty::Hard);
        assert!(sim.charts[0].timing_override.is_none());

        let beginner = &sim.charts[1];
        let timing = beginner.timing_override.as_ref().expect("override");
        assert_eq!(timing.bpms, vec![(0.0, 70.0)]);
        assert_eq!(timing.offset, 0.1, "offset inherited from song");
        assert_eq!(sim.timing_for(beginner).bpms[0].1, 70.0);
    }
}

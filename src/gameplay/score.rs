//! Score, combo, life and timing statistics.

use serde::{Deserialize, Serialize};

use super::judge::Judgment;

/// Dance-point weights per judgment (StepMania style).
const DP_WEIGHTS: [i64; 6] = [3, 2, 1, 0, 0, 0];
const DP_HOLD_OK: i64 = 3;
const DP_MINE: i64 = -2;
const LIFE_DELTA: [f32; 6] = [0.008, 0.008, 0.004, 0.0, -0.04, -0.08];

/// Running score for one play.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Score {
    /// Count per judgment, indexed by [`Judgment::index`].
    pub counts: [u32; 6],
    pub holds_ok: u32,
    pub holds_ng: u32,
    pub mines_hit: u32,
    pub combo: u32,
    pub max_combo: u32,
    dp: i64,
    max_dp: i64,
    /// 0.0..=1.0
    pub life: f32,
    pub failed: bool,
    /// `(note time, offset in ms)`; positive = pressed late.
    pub offsets: Vec<(f64, f64)>,
}

impl Score {
    /// New score for a chart with `taps` judgeable notes and `holds` holds/rolls.
    pub fn new(taps: u32, holds: u32) -> Self {
        Self {
            max_dp: i64::from(taps) * DP_WEIGHTS[0] + i64::from(holds) * DP_HOLD_OK,
            life: 0.5,
            ..Default::default()
        }
    }

    /// Records a tap judgment. `offset` is press time − note time (seconds), `None` for misses.
    pub fn add_judgment(&mut self, judgment: Judgment, note_time: f64, offset: Option<f64>) {
        self.counts[judgment.index()] += 1;
        self.dp += DP_WEIGHTS[judgment.index()];
        if judgment.keeps_combo() {
            self.combo += 1;
            self.max_combo = self.max_combo.max(self.combo);
        } else {
            self.combo = 0;
        }
        if let Some(o) = offset {
            self.offsets.push((note_time, o * 1000.0));
        }
        self.change_life(LIFE_DELTA[judgment.index()]);
    }

    pub fn add_hold(&mut self, ok: bool) {
        if ok {
            self.holds_ok += 1;
            self.dp += DP_HOLD_OK;
        } else {
            self.holds_ng += 1;
            self.change_life(-0.08);
        }
    }

    pub fn add_mine(&mut self) {
        self.mines_hit += 1;
        self.dp += DP_MINE;
        self.change_life(-0.04);
    }

    fn change_life(&mut self, delta: f32) {
        self.life = (self.life + delta).clamp(0.0, 1.0);
        if self.life <= 0.0 {
            self.failed = true;
        }
    }

    /// Score percentage 0..=100.
    pub fn percent(&self) -> f64 {
        if self.max_dp == 0 {
            return 0.0;
        }
        (self.dp.max(0) as f64 / self.max_dp as f64 * 100.0).clamp(0.0, 100.0)
    }

    /// Letter grade from the percentage.
    pub fn grade(&self, no_fail: bool) -> &'static str {
        if self.failed && !no_fail {
            return "F";
        }
        let p = self.percent();
        match p {
            _ if p >= 99.0 => "SS",
            _ if p >= 95.0 => "S",
            _ if p >= 90.0 => "A",
            _ if p >= 80.0 => "B",
            _ if p >= 70.0 => "C",
            _ => "D",
        }
    }

    /// Mean and standard deviation of offsets (ms). `None` without data.
    pub fn offset_stats(&self) -> Option<(f64, f64)> {
        offset_stats(self.offsets.iter().map(|o| o.1))
    }
}

/// Mean and standard deviation of a list of values.
pub fn offset_stats(values: impl Iterator<Item = f64> + Clone) -> Option<(f64, f64)> {
    let n = values.clone().count();
    if n == 0 {
        return None;
    }
    let mean = values.clone().sum::<f64>() / n as f64;
    let var = values.map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64;
    Some((mean, var.sqrt()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combo_and_percent() {
        let mut s = Score::new(4, 0);
        s.add_judgment(Judgment::Marvelous, 0.0, Some(0.0));
        s.add_judgment(Judgment::Great, 1.0, Some(0.05));
        assert_eq!(s.combo, 2);
        s.add_judgment(Judgment::Good, 2.0, Some(0.1));
        assert_eq!(s.combo, 0);
        assert_eq!(s.max_combo, 2);
        s.add_judgment(Judgment::Marvelous, 3.0, Some(0.0));
        // (3 + 1 + 0 + 3) / 12
        assert!((s.percent() - 7.0 / 12.0 * 100.0).abs() < 1e-9);
    }

    #[test]
    fn stats() {
        let (mean, sd) = offset_stats([10.0, 20.0, 30.0].into_iter()).unwrap();
        assert!((mean - 20.0).abs() < 1e-9);
        assert!((sd - (200.0f64 / 3.0).sqrt()).abs() < 1e-9);
    }
}

//! Foot model that turns note times into columns, plus an ergonomics checker.
//!
//! Left foot may use Left/Down/Up, right foot Down/Up/Right (no crossovers).
//! Feet alternate, and a foot never steps where the other foot stands, so
//! consecutive single notes never share a column (no jacks).

use rand::RngExt;
use rand_chacha::ChaCha8Rng;

use crate::parser::{Chart, Note, NoteKind};

/// Columns: Left, Down, Up, Right.
pub const LEFT: u8 = 0;
pub const DOWN: u8 = 1;
pub const UP: u8 = 2;
pub const RIGHT: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Foot {
    Left,
    Right,
}

impl Foot {
    fn other(self) -> Self {
        match self {
            Foot::Left => Foot::Right,
            Foot::Right => Foot::Left,
        }
    }

    fn allowed(self) -> [u8; 3] {
        match self {
            Foot::Left => [LEFT, DOWN, UP],
            Foot::Right => [DOWN, UP, RIGHT],
        }
    }
}

/// Where each foot stands and whether one is holding a note.
pub struct FootState {
    pub left: u8,
    pub right: u8,
    pub next: Foot,
    /// Holding foot, column and end beat.
    pub hold: Option<(Foot, u8, f64)>,
    /// How many rows in a row each column has been used (jack length).
    pub run: [u8; 4],
}

/// The model never creates jacks longer than this.
const MAX_RUN: u8 = 2;

impl Default for FootState {
    fn default() -> Self {
        Self {
            left: LEFT,
            right: RIGHT,
            next: Foot::Left,
            hold: None,
            run: [0; 4],
        }
    }
}

impl FootState {
    fn pos(&self, foot: Foot) -> u8 {
        match foot {
            Foot::Left => self.left,
            Foot::Right => self.right,
        }
    }

    fn set(&mut self, foot: Foot, col: u8) {
        match foot {
            Foot::Left => self.left = col,
            Foot::Right => self.right = col,
        }
    }

    /// Records which columns the latest row used.
    fn commit_row(&mut self, cols: &[u8]) {
        for c in 0..4u8 {
            let r = &mut self.run[usize::from(c)];
            *r = if cols.contains(&c) {
                r.saturating_add(1)
            } else {
                0
            };
        }
    }

    fn tired(&self, col: u8) -> bool {
        self.run[usize::from(col)] >= MAX_RUN
    }

    /// Ends a hold that is over at `beat`.
    pub fn release_holds(&mut self, beat: f64) {
        if let Some((_, _, end)) = self.hold
            && beat > end + 1e-6
        {
            self.hold = None;
        }
    }

    pub fn holding(&self) -> bool {
        self.hold.is_some()
    }

    /// Chooses a column for a single step and moves the foot.
    ///
    /// `movement` (0..=1) is how much the chart likes moving away from the current panel.
    pub fn step(&mut self, rng: &mut ChaCha8Rng, movement: f64) -> (Foot, u8) {
        let foot = match self.hold {
            Some((holding, _, _)) => holding.other(),
            None => self.next,
        };
        let other = self.pos(foot.other());
        let hold_col = self.hold.map(|(_, c, _)| c);
        let here = self.pos(foot);
        // During a hold the same foot steps repeatedly: forbid its current panel (no jacks).
        let options: Vec<u8> = foot
            .allowed()
            .into_iter()
            .filter(|&c| {
                c != other
                    && Some(c) != hold_col
                    && !(self.hold.is_some() && c == here)
                    && !self.tired(c)
            })
            .collect();
        let col = if options.is_empty() {
            here
        } else {
            let weights: Vec<f64> = options
                .iter()
                .map(|&c| {
                    let side = matches!((foot, c), (Foot::Left, LEFT) | (Foot::Right, RIGHT));
                    let base = if side { 1.4 } else { 1.0 };
                    if c == here {
                        base * (1.0 - movement).max(0.15)
                    } else {
                        base * (0.5 + movement)
                    }
                })
                .collect();
            let total: f64 = weights.iter().sum();
            let mut r = rng.random::<f64>() * total;
            let mut pick = options[0];
            for (c, w) in options.iter().zip(&weights) {
                if r < *w {
                    pick = *c;
                    break;
                }
                r -= w;
            }
            pick
        };
        self.set(foot, col);
        self.commit_row(&[col]);
        if self.hold.is_none() {
            self.next = foot.other();
        }
        (foot, col)
    }

    /// Chooses columns for a jump and moves both feet.
    pub fn jump(&mut self, rng: &mut ChaCha8Rng) -> (u8, u8) {
        const JUMPS: [(u8, u8, f64); 5] = [
            (LEFT, RIGHT, 4.0),
            (LEFT, UP, 1.0),
            (DOWN, RIGHT, 1.0),
            (LEFT, DOWN, 0.6),
            (UP, RIGHT, 0.6),
        ];
        // Avoid repeating exactly where the feet already are.
        let mut options: Vec<&(u8, u8, f64)> = JUMPS
            .iter()
            .filter(|j| {
                !(j.0 == self.left && j.1 == self.right) && !self.tired(j.0) && !self.tired(j.1)
            })
            .collect();
        if options.is_empty() {
            options = JUMPS
                .iter()
                .filter(|j| !self.tired(j.0) && !self.tired(j.1))
                .collect();
        }
        let total: f64 = options.iter().map(|j| j.2).sum();
        let mut r = rng.random::<f64>() * total;
        let mut chosen = (LEFT, RIGHT);
        for j in options {
            if r < j.2 {
                chosen = (j.0, j.1);
                break;
            }
            r -= j.2;
        }
        self.left = chosen.0;
        self.right = chosen.1;
        self.commit_row(&[chosen.0, chosen.1]);
        self.next = if rng.random_bool(0.5) {
            Foot::Left
        } else {
            Foot::Right
        };
        chosen
    }
}

/// Limits the checker enforces.
#[derive(Debug, Clone, Copy)]
pub struct Rules {
    pub max_jack: usize,
    pub allow_jumps: bool,
    pub allow_holds: bool,
}

/// Returns human-readable rule violations (empty = chart is fine).
///
/// `times` maps beats to seconds. Notes must be sorted.
pub fn violations(chart: &Chart, rules: Rules, beat_to_time: impl Fn(f64) -> f64) -> Vec<String> {
    let mut out = Vec::new();
    // Group into rows.
    let mut rows: Vec<(f64, Vec<&Note>)> = Vec::new();
    for n in &chart.notes {
        match rows.last_mut() {
            Some((b, list)) if (*b - n.beat).abs() < 1e-9 => list.push(n),
            _ => rows.push((n.beat, vec![n])),
        }
    }
    let mut run = [0usize; 4];
    let mut holds: Vec<(u8, f64, f64)> = Vec::new(); // column, start, end
    for (beat, notes) in &rows {
        let t = beat_to_time(*beat);
        if t < 1.0 - 1e-6 {
            out.push(format!("note at {t:.2}s is inside the first second"));
        }
        if notes.len() > 2 {
            out.push(format!("more than 2 notes at beat {beat}"));
        }
        if notes.len() == 2 && !rules.allow_jumps {
            out.push(format!("jump at beat {beat} not allowed"));
        }
        let cols: Vec<u8> = notes.iter().map(|n| n.column).collect();
        for c in 0..4u8 {
            let i = usize::from(c);
            run[i] = if cols.contains(&c) { run[i] + 1 } else { 0 };
            if run[i] > rules.max_jack {
                out.push(format!("jack of {} in column {c} at beat {beat}", run[i]));
            }
        }
        for n in notes {
            if let Some((_, s, e)) = holds
                .iter()
                .find(|h| h.0 == n.column && *beat > h.1 + 1e-9 && *beat <= h.2 + 1e-9)
            {
                out.push(format!(
                    "note in held column {} at beat {beat} (hold {s}–{e})",
                    n.column
                ));
            }
            if let NoteKind::HoldHead { end_beat } | NoteKind::RollHead { end_beat } = n.kind {
                if !rules.allow_holds {
                    out.push(format!("hold at beat {beat} not allowed"));
                }
                holds.push((n.column, *beat, end_beat));
            }
        }
    }
    out
}

//! Game state for one play: judging presses, holds, mines, misses and score.
//!
//! Pure logic: it receives times in seconds and never reads the clock itself.

pub mod judge;
pub mod score;

use crate::parser::NoteKind;
use crate::timing::TimedNote;

use judge::{HOLD_LET_GO, Judgment, ROLL_LET_GO, judge, miss_window};
use score::Score;

/// Progress of a hold or roll.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HoldState {
    /// Not a hold, or head not reached yet.
    Inactive,
    /// Head was hit; `released_at` is set while the key is up.
    Holding {
        released_at: Option<f64>,
        last_tap: f64,
    },
    Ok,
    Ng,
}

/// A note plus its result.
#[derive(Debug, Clone)]
pub struct NoteState {
    pub note: TimedNote,
    pub judgment: Option<Judgment>,
    pub hold: HoldState,
    /// Mines: true once exploded.
    pub mine_hit: bool,
}

impl NoteState {
    fn is_hold(&self) -> bool {
        matches!(
            self.note.kind,
            NoteKind::HoldHead { .. } | NoteKind::RollHead { .. }
        )
    }

    fn is_judgeable(&self) -> bool {
        !matches!(self.note.kind, NoteKind::Mine | NoteKind::Fake)
    }
}

/// Something the UI may want to animate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GameEvent {
    Hit {
        column: u8,
        judgment: Judgment,
        offset: f64,
    },
    Miss {
        column: u8,
    },
    HoldOk {
        column: u8,
    },
    HoldNg {
        column: u8,
    },
    MineHit {
        column: u8,
    },
}

/// State of one play.
pub struct GameSession {
    pub notes: Vec<NoteState>,
    columns: usize,
    /// Per column: indices of judgeable notes, sorted by time.
    by_column: Vec<Vec<usize>>,
    /// Per column: first index in `by_column` that may still be unjudged.
    next: Vec<usize>,
    mines: Vec<usize>,
    next_mine: usize,
    pressed: Vec<bool>,
    judge_scale: f64,
    last_time: f64,
    pub score: Score,
    /// Events produced since the UI last drained them.
    pub events: Vec<GameEvent>,
    /// Time after which the song is over.
    pub end_time: f64,
}

impl GameSession {
    /// Creates a session for already-timed notes.
    pub fn new(notes: Vec<TimedNote>, columns: u8, judge_scale: f64) -> Self {
        let columns = usize::from(columns);
        let notes: Vec<NoteState> = notes
            .into_iter()
            .filter(|n| usize::from(n.column) < columns)
            .map(|note| NoteState {
                note,
                judgment: None,
                hold: HoldState::Inactive,
                mine_hit: false,
            })
            .collect();

        let mut by_column = vec![Vec::new(); columns];
        let mut mines = Vec::new();
        let (mut taps, mut holds) = (0u32, 0u32);
        for (i, n) in notes.iter().enumerate() {
            if n.note.kind == NoteKind::Mine {
                mines.push(i);
            } else if n.is_judgeable() {
                by_column[usize::from(n.note.column)].push(i);
                taps += 1;
                if n.is_hold() {
                    holds += 1;
                }
            }
        }
        let end_time = notes.iter().map(|n| n.note.end_time).fold(0.0, f64::max);
        Self {
            notes,
            columns,
            by_column,
            next: vec![0; columns],
            mines,
            next_mine: 0,
            pressed: vec![false; columns],
            judge_scale,
            last_time: f64::NEG_INFINITY,
            score: Score::new(taps, holds),
            events: Vec::new(),
            end_time,
        }
    }

    /// Whether a column is currently held down.
    pub fn is_pressed(&self, column: u8) -> bool {
        self.pressed
            .get(usize::from(column))
            .copied()
            .unwrap_or(false)
    }

    /// A column was pressed at song time `t`.
    pub fn press(&mut self, column: u8, t: f64) {
        let col = usize::from(column);
        if col >= self.columns {
            return;
        }
        self.pressed[col] = true;
        self.refresh_holds(col, t, true);

        let window = miss_window(self.judge_scale);
        let mut best: Option<(usize, f64)> = None;
        for &idx in &self.by_column[col][self.next[col]..] {
            let n = &self.notes[idx];
            if n.note.time > t + window {
                break;
            }
            if n.judgment.is_some() {
                continue;
            }
            let diff = t - n.note.time;
            if diff.abs() <= window && best.is_none_or(|(_, d)| diff.abs() < d.abs()) {
                best = Some((idx, diff));
            }
        }
        let Some((idx, diff)) = best else { return };
        let Some(judgment) = judge(diff, self.judge_scale) else {
            return;
        };

        let state = &mut self.notes[idx];
        state.judgment = Some(judgment);
        if state.is_hold() {
            state.hold = HoldState::Holding {
                released_at: None,
                last_tap: t,
            };
        }
        let note_time = state.note.time;
        self.score.add_judgment(judgment, note_time, Some(diff));
        self.events.push(GameEvent::Hit {
            column,
            judgment,
            offset: diff,
        });
        self.advance(col);
    }

    /// A column was released at song time `t`.
    pub fn release(&mut self, column: u8, t: f64) {
        let col = usize::from(column);
        if col >= self.columns {
            return;
        }
        self.pressed[col] = false;
        self.refresh_holds(col, t, false);
    }

    /// Updates holds on press/release in a column.
    fn refresh_holds(&mut self, col: usize, t: f64, pressed: bool) {
        for &idx in &self.by_column[col] {
            if let HoldState::Holding {
                released_at,
                last_tap,
            } = &mut self.notes[idx].hold
            {
                if pressed {
                    *released_at = None;
                    *last_tap = t;
                } else {
                    *released_at = Some(t);
                }
            }
        }
    }

    /// Moves `next[col]` past notes that are fully judged.
    fn advance(&mut self, col: usize) {
        let list = &self.by_column[col];
        while let Some(&idx) = list.get(self.next[col]) {
            if self.notes[idx].judgment.is_none() {
                break;
            }
            self.next[col] += 1;
        }
    }

    /// Advances the game to song time `t`: misses, hold results and mines.
    pub fn update(&mut self, t: f64) {
        let window = miss_window(self.judge_scale);
        for col in 0..self.columns {
            // Misses
            for i in self.next[col]..self.by_column[col].len() {
                let idx = self.by_column[col][i];
                let n = &mut self.notes[idx];
                if n.note.time + window >= t {
                    break;
                }
                if n.judgment.is_none() {
                    n.judgment = Some(Judgment::Miss);
                    let was_hold = n.is_hold();
                    if was_hold {
                        n.hold = HoldState::Ng;
                    }
                    let time = n.note.time;
                    self.score.add_judgment(Judgment::Miss, time, None);
                    self.events.push(GameEvent::Miss { column: col as u8 });
                    if was_hold {
                        self.score.add_hold(false);
                    }
                }
            }
            self.advance(col);
        }
        self.update_holds(t);
        self.update_mines(t);
        self.last_time = t;
    }

    fn update_holds(&mut self, t: f64) {
        for i in 0..self.notes.len() {
            let n = &mut self.notes[i];
            let HoldState::Holding {
                released_at,
                last_tap,
            } = n.hold
            else {
                continue;
            };
            let is_roll = matches!(n.note.kind, NoteKind::RollHead { .. });
            let column = n.note.column;
            let failed = if is_roll {
                t - last_tap > ROLL_LET_GO && t < n.note.end_time
            } else {
                released_at.is_some_and(|r| t - r > HOLD_LET_GO)
            };
            if failed {
                n.hold = HoldState::Ng;
                self.score.add_hold(false);
                self.events.push(GameEvent::HoldNg { column });
            } else if t >= n.note.end_time {
                n.hold = HoldState::Ok;
                self.score.add_hold(true);
                self.events.push(GameEvent::HoldOk { column });
            }
        }
    }

    fn update_mines(&mut self, t: f64) {
        while let Some(&idx) = self.mines.get(self.next_mine) {
            let n = &mut self.notes[idx];
            if n.note.time > t {
                break;
            }
            if n.note.time > self.last_time && self.pressed[usize::from(n.note.column)] {
                n.mine_hit = true;
                self.score.add_mine();
                self.events.push(GameEvent::MineHit {
                    column: n.note.column,
                });
            }
            self.next_mine += 1;
        }
    }

    /// True when every note is resolved and the last note is over.
    pub fn is_finished(&self, t: f64) -> bool {
        t > self.end_time + miss_window(self.judge_scale) + 0.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(time: f64, column: u8, kind: NoteKind) -> TimedNote {
        let end_time = match kind {
            NoteKind::HoldHead { end_beat } | NoteKind::RollHead { end_beat } => end_beat,
            _ => time,
        };
        TimedNote {
            time,
            beat: time,
            column,
            kind,
            end_time,
            end_beat: end_time,
        }
    }

    #[test]
    fn press_judges_nearest_note_only() {
        let mut g = GameSession::new(
            vec![note(1.0, 0, NoteKind::Tap), note(1.1, 0, NoteKind::Tap)],
            4,
            1.0,
        );
        g.press(0, 1.09);
        assert_eq!(g.notes[0].judgment, None);
        assert_eq!(g.notes[1].judgment, Some(Judgment::Marvelous));
        g.update(1.5);
        assert_eq!(g.notes[0].judgment, Some(Judgment::Miss));
        assert_eq!(g.score.combo, 0);
    }

    #[test]
    fn press_far_from_notes_does_nothing() {
        let mut g = GameSession::new(vec![note(1.0, 1, NoteKind::Tap)], 4, 1.0);
        g.press(1, 0.5);
        assert_eq!(g.notes[0].judgment, None);
        g.press(2, 1.0); // wrong column
        assert_eq!(g.notes[0].judgment, None);
    }

    #[test]
    fn hold_ok_and_ng() {
        let hold = NoteKind::HoldHead { end_beat: 2.0 };
        let mut g = GameSession::new(vec![note(1.0, 0, hold), note(1.0, 1, hold)], 4, 1.0);
        g.press(0, 1.0);
        g.press(1, 1.0);
        g.release(1, 1.2);
        g.update(1.3); // released 0.1 s: still fine
        assert!(matches!(g.notes[1].hold, HoldState::Holding { .. }));
        g.update(1.5); // released 0.3 s: NG
        assert_eq!(g.notes[1].hold, HoldState::Ng);
        g.update(2.0);
        assert_eq!(g.notes[0].hold, HoldState::Ok);
        assert_eq!((g.score.holds_ok, g.score.holds_ng), (1, 1));
    }

    #[test]
    fn mine_explodes_only_when_held() {
        let mut g = GameSession::new(
            vec![note(1.0, 0, NoteKind::Mine), note(2.0, 1, NoteKind::Mine)],
            4,
            1.0,
        );
        g.update(0.9);
        g.press(0, 0.95);
        g.update(1.05);
        g.update(2.05); // column 1 never pressed
        assert!(g.notes[0].mine_hit);
        assert!(!g.notes[1].mine_hit);
        assert_eq!(g.score.mines_hit, 1);
    }

    #[test]
    fn all_marvelous_is_100_percent() {
        let notes: Vec<TimedNote> = (0..8)
            .map(|i| note(f64::from(i) * 0.5, (i % 4) as u8, NoteKind::Tap))
            .collect();
        let mut g = GameSession::new(notes, 4, 1.0);
        for i in 0..8 {
            let t = f64::from(i) * 0.5;
            g.update(t);
            g.press((i % 4) as u8, t + 0.01);
            g.release((i % 4) as u8, t + 0.05);
        }
        g.update(10.0);
        assert_eq!(g.score.percent(), 100.0);
        assert_eq!(g.score.max_combo, 8);
    }
}

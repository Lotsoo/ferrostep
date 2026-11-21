//! Gameplay stage and HUD: a calm, cinematic stage that comes alive as the
//! player keeps a combo, plus glass "islands" that are readable at a glance.
//!
//! Everything here only draws; state lives in [`HudAnim`] and the game session.

use std::collections::VecDeque;
use std::f32::consts::TAU;
use std::time::Instant;

use eframe::egui::{
    Align2, Color32, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, epaint::PathShape, pos2, vec2,
};

use super::notefield::{Layout, Scroll, draw_glow};
use super::theme::{self, ACCENT_A, ACCENT_B, column_color, font, font_bold, judgment_color};
use super::{anim, widgets};
use crate::gameplay::judge::{Judgment, WINDOWS};
use crate::gameplay::score::Score;
use crate::timing::TimingMap;

mod countdown;
mod islands;
mod stage;

pub use countdown::*;
pub use islands::*;
pub use stage::*;

/// How long a press stays on the hit-error meter (seconds).
const METER_MEMORY: f32 = 3.0;
/// Most recent presses kept for the hit-error meter.
const METER_HITS: usize = 24;
/// Combo milestones (every this many notes) get a celebration.
pub const MILESTONE: u32 = 50;

/// Animated values for the HUD. Springs keep every change smooth and interruptible.
pub struct HudAnim {
    life: anim::Spring,
    accuracy: anim::Spring,
    /// 0..1, grows with a long combo, drops when it breaks.
    energy: anim::Spring,
    /// Key beam brightness per column, 0..1.
    beams: [f32; 4],
    /// Recent presses: offset in ms (positive = late), color, when.
    hits: VecDeque<(f32, Color32, Instant)>,
    milestone: Option<(u32, Instant)>,
    /// Current live grade and when it last changed (for the pop).
    grade: (&'static str, Option<Instant>),
    reduce: bool,
    ready: bool,
}

impl Default for HudAnim {
    fn default() -> Self {
        Self {
            life: anim::Spring::new(0.5, anim::Spring::SMOOTH),
            accuracy: anim::Spring::new(100.0, anim::Spring::SNAPPY),
            energy: anim::Spring::new(0.0, (1.0, 0.9)),
            beams: [0.0; 4],
            hits: VecDeque::new(),
            milestone: None,
            grade: ("SS", None),
            reduce: false,
            ready: false,
        }
    }
}

impl HudAnim {
    /// Advances all animations by one frame.
    /// `failed` = the play is lost (never true with No Fail).
    pub fn update(&mut self, score: &Score, failed: bool, pressed: &[bool], dt: f32, reduce: bool) {
        self.reduce = reduce;
        let grade = live_grade(score.accuracy_percent(), failed);
        if grade != self.grade.0 {
            self.grade = (grade, Some(Instant::now()));
        }
        self.life.set_target(score.life.clamp(0.0, 1.0));
        self.accuracy.set_target(score.accuracy_percent() as f32);
        let energy = if score.combo >= 10 {
            ((score.combo - 10) as f32 / 90.0).min(1.0)
        } else {
            0.0
        };
        self.energy.set_target(energy);
        for spring in [&mut self.life, &mut self.accuracy, &mut self.energy] {
            if reduce || !self.ready {
                spring.snap(spring.target);
            } else {
                spring.step(dt);
            }
        }
        self.ready = true;
        for (c, beam) in self.beams.iter_mut().enumerate() {
            if pressed.get(c).copied().unwrap_or(false) {
                *beam = 1.0;
            } else {
                *beam *= (-dt * 12.0).exp();
            }
        }
    }

    /// Records a judged press for the hit-error meter.
    pub fn record_hit(&mut self, judgment: Judgment, offset_seconds: f64, now: Instant) {
        self.hits.push_back((
            offset_seconds as f32 * 1000.0,
            judgment_color(judgment),
            now,
        ));
        while self.hits.len() > METER_HITS {
            self.hits.pop_front();
        }
    }

    /// Call when the combo changes, to trigger milestone celebrations.
    pub fn combo_changed(&mut self, combo: u32, now: Instant) {
        if combo > 0 && combo.is_multiple_of(MILESTONE) {
            self.milestone = Some((combo, now));
        }
    }

    /// 1 right after the grade changed, fading to 0 within half a second.
    fn grade_pop(&self) -> f32 {
        match self.grade.1 {
            Some(at) if !self.reduce => {
                1.0 - anim::ease_out_cubic((at.elapsed().as_secs_f32() / 0.5).min(1.0))
            }
            _ => 0.0,
        }
    }

    pub fn energy(&self) -> f32 {
        self.energy.value.clamp(0.0, 1.0)
    }

    /// The milestone currently being celebrated and its age in seconds.
    pub fn milestone(&self, now: Instant) -> Option<(u32, f32)> {
        self.milestone
            .map(|(c, at)| (c, anim::since(at, now)))
            .filter(|(_, age)| *age < 1.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grade_follows_accuracy() {
        assert_eq!(live_grade(100.0, false), "SS");
        assert_eq!(live_grade(96.0, false), "S");
        assert_eq!(live_grade(50.0, false), "D");
        assert_eq!(live_grade(100.0, true), "F");
    }

    #[test]
    fn milestones_and_hit_memory() {
        let mut hud = HudAnim::default();
        let now = Instant::now();
        hud.combo_changed(49, now);
        assert!(hud.milestone(now).is_none());
        hud.combo_changed(50, now);
        assert_eq!(hud.milestone(now).map(|m| m.0), Some(50));
        for _ in 0..100 {
            hud.record_hit(Judgment::Great, 0.05, now);
        }
        assert_eq!(hud.hits.len(), METER_HITS);
        assert_eq!(milestone_word(100), "Hebat sekali!");
    }
}

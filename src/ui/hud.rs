//! Gameplay stage and HUD: a calm, cinematic stage that comes alive as the
//! player keeps a combo, plus glass "islands" that are readable at a glance.
//!
//! Everything here only draws; state lives in [`HudAnim`] and the game session.

use std::collections::VecDeque;
use std::f32::consts::{FRAC_PI_2, TAU};
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
            ready: false,
        }
    }
}

impl HudAnim {
    /// Advances all animations by one frame.
    pub fn update(&mut self, score: &Score, pressed: &[bool], dt: f32, reduce: bool) {
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

// ── Stage ───────────────────────────────────────────────────────────────

/// Everything needed to paint the stage behind the notes.
pub struct Stage<'a> {
    pub rect: Rect,
    pub layout: &'a Layout,
    pub columns: u8,
    pub tint: Color32,
    /// Background brightness setting, 0..1.
    pub brightness: f32,
    pub energy: f32,
    /// 1 on the beat, falling to 0 before the next one.
    pub pulse: f32,
    /// Seconds, for slow ambient motion.
    pub time: f32,
    pub reduce: bool,
}

/// Background, ambient light and drifting particles.
pub fn draw_stage(painter: &Painter, s: &Stage) {
    let rect = s.rect;
    widgets::vertical_gradient(
        painter,
        rect,
        Color32::from_rgb(7, 7, 14),
        Color32::from_rgb(12, 11, 26),
    );
    let light = 0.35 + s.brightness * 0.65;

    // Two large tinted lights at the sides, drifting very slowly.
    let drift = if s.reduce { 0.0 } else { s.time * 0.05 };
    let w = rect.width();
    for (fx, fy, color, r) in [
        (0.12 + 0.03 * drift.sin(), 0.3, s.tint, 0.42),
        (0.88 - 0.03 * drift.cos(), 0.72, ACCENT_A, 0.38),
    ] {
        soft_light(
            painter,
            pos2(rect.left() + w * fx, rect.top() + rect.height() * fy),
            w * r,
            color,
            (0.16 + s.energy * 0.14) * light,
        );
    }

    // Light behind the receptors: breathes with the beat, grows with the combo.
    let field_w = s.layout.width(s.columns);
    let center = pos2(rect.center().x, s.layout.receptor_y);
    soft_light(
        painter,
        center,
        field_w * (0.9 + s.energy * 0.3),
        s.tint.lerp_to_gamma(ACCENT_B, 0.4),
        (0.10 + s.energy * 0.18 + s.pulse * 0.06) * light,
    );

    if !s.reduce {
        draw_particles(painter, s);
    }

    // Cinematic vignette: darker edges pull the eye to the notefield.
    let band = w * 0.18;
    widgets::gradient_rect(
        painter,
        Rect::from_min_size(rect.min, vec2(band, rect.height())),
        Color32::from_black_alpha(150),
        Color32::TRANSPARENT,
    );
    widgets::gradient_rect(
        painter,
        Rect::from_min_size(
            pos2(rect.right() - band, rect.top()),
            vec2(band, rect.height()),
        ),
        Color32::TRANSPARENT,
        Color32::from_black_alpha(150),
    );
}

/// A big, very soft radial light (see [`notefield::radial_light`]).
fn soft_light(painter: &Painter, center: Pos2, radius: f32, color: Color32, strength: f32) {
    super::notefield::radial_light(painter, center, radius, color, strength);
}

/// A rounded tile with a vertical gradient (top → bottom).
pub fn gradient_tile(painter: &Painter, rect: Rect, radius: u8, top: Color32, bottom: Color32) {
    let r = f32::from(radius).min(rect.height() / 2.0);
    let top_band = Rect::from_min_max(rect.min, pos2(rect.right(), rect.top() + r));
    let bottom_band = Rect::from_min_max(pos2(rect.left(), rect.bottom() - r), rect.max);
    painter
        .with_clip_rect(top_band)
        .rect_filled(rect, radius, top);
    painter
        .with_clip_rect(bottom_band)
        .rect_filled(rect, radius, bottom);
    widgets::vertical_gradient(
        painter,
        Rect::from_min_max(top_band.left_bottom(), bottom_band.right_top()),
        top,
        bottom,
    );
}

/// Dust of light rising slowly; brighter and faster with a long combo.
fn draw_particles(painter: &Painter, s: &Stage) {
    const COUNT: u32 = 44;
    let rect = s.rect;
    let h = rect.height() + 40.0;
    for i in 0..COUNT {
        let hash = i.wrapping_mul(2_654_435_761) ^ 0x9E37_79B9;
        let r = |shift: u32| ((hash >> shift) & 0xFF) as f32 / 255.0;
        let speed = 10.0 + r(0) * 22.0 + s.energy * 18.0;
        let x = rect.left() + rect.width() * r(8);
        let y = rect.bottom() + 20.0 - (s.time * speed + r(16) * h).rem_euclid(h);
        let twinkle = 0.6 + 0.4 * (s.time * (0.6 + r(24)) + r(4) * TAU).sin();
        let alpha = (0.05 + 0.12 * r(12)) * (0.6 + s.energy) * twinkle;
        let color = s.tint.lerp_to_gamma(Color32::WHITE, 0.5);
        painter.circle_filled(pos2(x, y), 1.0 + r(20) * 1.6, color.gamma_multiply(alpha));
    }
}

/// The glass lane under the notes: soft panel, column separators, scrolling
/// beat lines and key beams for pressed columns.
pub fn draw_lane(
    painter: &Painter,
    s: &Stage,
    scroll: &Scroll,
    timing: &TimingMap,
    beams: &HudAnim,
) {
    let layout = s.layout;
    let rect = s.rect;
    let field = Rect::from_x_y_ranges(
        layout.left - 12.0..=layout.left + layout.width(s.columns) + 12.0,
        rect.y_range(),
    );
    // Darker near the receptors (where reading matters), lighter far away.
    let (near, far) = (
        Color32::from_black_alpha(150),
        Color32::from_black_alpha(70),
    );
    let (top, bottom) = if layout.dir > 0.0 {
        (near, far)
    } else {
        (far, near)
    };
    widgets::vertical_gradient(painter, field, top, bottom);

    // Side edges light up with the combo energy.
    let edge = Color32::WHITE
        .gamma_multiply(0.06)
        .lerp_to_gamma(ACCENT_B.gamma_multiply(0.5), s.energy);
    for x in [field.left(), field.right()] {
        widgets::vertical_gradient(
            painter,
            Rect::from_min_max(pos2(x - 0.5, field.top()), pos2(x + 0.5, field.bottom())),
            if layout.dir > 0.0 {
                edge
            } else {
                Color32::TRANSPARENT
            },
            if layout.dir > 0.0 {
                Color32::TRANSPARENT
            } else {
                edge
            },
        );
    }
    for c in 1..s.columns {
        let x = layout.left + layout.lane * f32::from(c);
        painter.line_segment(
            [pos2(x, field.top()), pos2(x, field.bottom())],
            Stroke::new(1.0, Color32::from_white_alpha(5)),
        );
    }

    // Beat lines travel with the notes: a quiet rhythm guide.
    let span = rect.height() + layout.arrow;
    let first = scroll.now_beat.floor() as i64;
    for b in first..first + 96 {
        let beat = b as f64;
        let d = scroll.distance(timing.beat_to_seconds(beat), beat);
        if d < -layout.arrow * 0.5 {
            continue;
        }
        if d > span {
            break;
        }
        let y = layout.receptor_y + d * layout.dir;
        let fade = (1.0 - d / span).clamp(0.0, 1.0);
        let measure = b.rem_euclid(4) == 0;
        let alpha = if measure { 0.10 } else { 0.04 } * fade;
        painter.line_segment(
            [pos2(field.left() + 2.0, y), pos2(field.right() - 2.0, y)],
            Stroke::new(
                if measure { 1.5 } else { 1.0 },
                Color32::WHITE.gamma_multiply(alpha),
            ),
        );
    }

    // Key beams: light rising from each pressed receptor along its lane.
    for c in 0..s.columns.min(4) {
        let strength = beams.beams[usize::from(c)];
        if strength < 0.02 {
            continue;
        }
        let x = layout.column_x(c);
        let len = layout.arrow * 3.4;
        let w = layout.lane * 0.86;
        let (y0, y1) = if layout.dir > 0.0 {
            (layout.receptor_y, layout.receptor_y + len)
        } else {
            (layout.receptor_y - len, layout.receptor_y)
        };
        let color = column_color(c).gamma_multiply(0.30 * strength);
        let r = Rect::from_min_max(pos2(x - w / 2.0, y0), pos2(x + w / 2.0, y1));
        if layout.dir > 0.0 {
            widgets::vertical_gradient(painter, r, color, Color32::TRANSPARENT);
        } else {
            widgets::vertical_gradient(painter, r, Color32::TRANSPARENT, color);
        }
    }

    // Judgment line: a soft band of light behind the receptors.
    let band = Rect::from_center_size(
        pos2(field.center().x, layout.receptor_y),
        vec2(field.width(), layout.arrow * 1.25),
    );
    painter.rect_filled(
        band,
        theme::RADIUS_CARD,
        Color32::WHITE.gamma_multiply(0.025 + 0.03 * s.pulse),
    );
}

// ── Glass islands ───────────────────────────────────────────────────────

/// A floating translucent surface: shadow, tinted glass, bright top edge.
pub fn glass(painter: &Painter, rect: Rect) {
    widgets::elevation(
        painter,
        rect,
        theme::RADIUS_CARD,
        widgets::Elevation::Medium,
    );
    painter.rect_filled(
        rect,
        theme::RADIUS_CARD,
        Color32::from_rgba_unmultiplied(22, 20, 40, 190),
    );
    painter.rect_stroke(
        rect,
        theme::RADIUS_CARD,
        Stroke::new(1.0, Color32::from_white_alpha(18)),
        StrokeKind::Inside,
    );
    widgets::top_highlight(painter, rect, theme::RADIUS_CARD, 0.14);
}

/// Now-playing island (top left): swatch, title, artist/difficulty, time left.
pub struct SongInfo<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    pub difficulty_color: Color32,
    pub tint: Color32,
    pub time: f64,
    pub length: f64,
}

pub fn draw_song_island(painter: &Painter, screen: Rect, info: &SongInfo) {
    let rect = Rect::from_min_size(screen.left_top() + vec2(24.0, 20.0), vec2(330.0, 82.0));
    glass(painter, rect);
    let swatch = Rect::from_min_size(rect.min + vec2(14.0, 15.0), vec2(52.0, 52.0));
    gradient_tile(
        painter,
        swatch,
        12,
        info.tint,
        info.tint.gamma_multiply(0.45),
    );
    // Equalizer-like bars in the swatch: shows it is playing.
    let t = painter.ctx().input(|i| i.time) as f32;
    let playing = info.time > 0.0 && info.time < info.length;
    for i in 0..4 {
        let h = if playing {
            8.0 + 14.0 * (0.5 + 0.5 * (t * (3.0 + i as f32 * 1.3) + i as f32).sin())
        } else {
            6.0
        };
        let x = swatch.left() + 13.0 + i as f32 * 7.5;
        painter.rect_filled(
            Rect::from_min_max(
                pos2(x, swatch.bottom() - 12.0 - h),
                pos2(x + 4.0, swatch.bottom() - 12.0),
            ),
            2,
            Color32::WHITE.gamma_multiply(0.85),
        );
    }

    let x = swatch.right() + 16.0;
    painter.text(
        pos2(x, rect.top() + 24.0),
        Align2::LEFT_CENTER,
        crate::ui::screens::song_select::truncate(info.title, 24),
        font_bold(theme::TEXT_M - 1.0),
        Color32::WHITE.gamma_multiply(0.95),
    );
    painter.circle_filled(pos2(x + 4.0, rect.top() + 45.0), 4.0, info.difficulty_color);
    painter.text(
        pos2(x + 14.0, rect.top() + 45.0),
        Align2::LEFT_CENTER,
        crate::ui::screens::song_select::truncate(info.subtitle, 30),
        font(theme::TEXT_XS),
        Color32::WHITE.gamma_multiply(0.6),
    );

    // Progress with a glowing head and the time left.
    let bar = Rect::from_min_max(
        pos2(x, rect.top() + 63.0),
        pos2(rect.right() - 62.0, rect.top() + 67.0),
    );
    let p = (info.time / info.length.max(1.0)).clamp(0.0, 1.0) as f32;
    painter.rect_filled(bar, theme::RADIUS_PILL, Color32::from_white_alpha(18));
    if p > 0.0 {
        let fill = Rect::from_min_size(bar.min, vec2(bar.width() * p, bar.height()));
        widgets::gradient_pill(painter, fill, ACCENT_A, ACCENT_B);
        draw_glow(painter, fill.right_center(), 7.0, ACCENT_B, 1.2);
    }
    let left = (info.length - info.time.max(0.0)).max(0.0);
    widgets::tabular_text(
        painter,
        pos2(rect.right() - 16.0, bar.center().y),
        Align2::RIGHT_CENTER,
        &format!("-{}:{:02}", (left / 60.0) as u32, (left % 60.0) as u32),
        font(theme::TEXT_XS),
        Color32::WHITE.gamma_multiply(0.6),
    );
}

/// Live grade from the accuracy so far.
fn live_grade(accuracy: f64, failed: bool) -> (&'static str, Color32, Color32) {
    let gold = Color32::from_rgb(0xFF, 0xD8, 0x6B);
    match accuracy {
        _ if failed => ("F", theme::DANGER, Color32::from_rgb(0x8A, 0x2A, 0x3A)),
        a if a >= 99.0 => ("SS", Color32::from_rgb(0xE0, 0xF7, 0xFF), ACCENT_B),
        a if a >= 95.0 => ("S", gold, Color32::from_rgb(0xFF, 0x9F, 0x5C)),
        a if a >= 90.0 => ("A", theme::SUCCESS, Color32::from_rgb(0x2A, 0xB0, 0x8A)),
        a if a >= 80.0 => ("B", ACCENT_B, ACCENT_A),
        a if a >= 70.0 => ("C", Color32::from_rgb(0xB0, 0x7C, 0xFF), ACCENT_A),
        _ => ("D", Color32::from_gray(170), Color32::from_gray(90)),
    }
}

/// Score island (top right): live grade, accuracy, score bar, judgment counts.
pub fn draw_score_island(
    painter: &Painter,
    screen: Rect,
    score: &Score,
    hud: &HudAnim,
    failed: bool,
) {
    let rect = Rect::from_min_size(
        pos2(screen.right() - 24.0 - 320.0, screen.top() + 20.0),
        vec2(320.0, 128.0),
    );
    glass(painter, rect);

    let (grade, light, deep) = live_grade(score.accuracy_percent(), failed);
    let badge = Rect::from_min_size(rect.min + vec2(14.0, 14.0), vec2(58.0, 58.0));
    gradient_tile(
        painter,
        badge,
        14,
        light.gamma_multiply(0.85),
        deep.gamma_multiply(0.85),
    );
    painter.rect_stroke(
        badge,
        14,
        Stroke::new(1.0, Color32::WHITE.gamma_multiply(0.25)),
        StrokeKind::Inside,
    );
    widgets::text_tracked(
        painter,
        badge.center(),
        Align2::CENTER_CENTER,
        grade,
        font_bold(if grade.len() > 1 { 24.0 } else { 30.0 }),
        Color32::WHITE,
    );

    widgets::text_tracked(
        painter,
        pos2(rect.right() - 16.0, rect.top() + 22.0),
        Align2::RIGHT_CENTER,
        "AKURASI",
        font_bold(10.0),
        Color32::WHITE.gamma_multiply(0.45),
    );
    widgets::tabular_text(
        painter,
        pos2(rect.right() - 16.0, rect.top() + 50.0),
        Align2::RIGHT_CENTER,
        &format!("{:.2}%", hud.accuracy.value.clamp(0.0, 100.0)),
        font_bold(30.0),
        Color32::WHITE,
    );

    // Score bar: bright = earned, soft = still reachable, dots = grade lines.
    let bar = Rect::from_min_max(
        pos2(rect.left() + 16.0, rect.top() + 84.0),
        pos2(rect.right() - 16.0, rect.top() + 90.0),
    );
    let earned = (score.percent() / 100.0) as f32;
    let possible = (score.max_possible_percent() / 100.0) as f32;
    let part =
        |v: f32| Rect::from_min_size(bar.min, vec2(bar.width() * v.clamp(0.0, 1.0), bar.height()));
    painter.rect_filled(bar, theme::RADIUS_PILL, Color32::from_white_alpha(14));
    painter.rect_filled(
        part(possible),
        theme::RADIUS_PILL,
        ACCENT_B.gamma_multiply(0.20),
    );
    if earned > 0.0 {
        widgets::gradient_pill(painter, part(earned), ACCENT_A, ACCENT_B);
    }
    for (pct, label) in [(0.8, "B"), (0.9, "A"), (0.95, "S"), (0.99, "SS")] {
        let x = bar.left() + bar.width() * pct;
        let reached = earned >= pct;
        let reachable = possible >= pct;
        let color = if reached {
            Color32::WHITE
        } else {
            Color32::WHITE.gamma_multiply(if reachable { 0.45 } else { 0.15 })
        };
        painter.circle_filled(pos2(x, bar.center().y), 2.0, color);
        // S and SS sit close together: SS goes above the bar.
        let (pos, align) = if label == "SS" {
            (pos2(x, bar.top() - 3.0), Align2::CENTER_BOTTOM)
        } else {
            (pos2(x, bar.bottom() + 3.0), Align2::CENTER_TOP)
        };
        painter.text(pos, align, label, font(9.0), color);
    }

    // Judgment counts.
    let row_y = rect.bottom() - 16.0;
    let cell = (rect.width() - 32.0) / 6.0;
    for (i, j) in Judgment::ALL.iter().enumerate() {
        let x = rect.left() + 16.0 + cell * i as f32;
        painter.circle_filled(pos2(x + 4.0, row_y), 3.5, judgment_color(*j));
        widgets::tabular_text(
            painter,
            pos2(x + 12.0, row_y),
            Align2::LEFT_CENTER,
            &score.counts[j.index()].to_string(),
            font(theme::TEXT_XS - 1.0),
            Color32::WHITE.gamma_multiply(0.7),
        );
    }
}

// ── Life gauge ──────────────────────────────────────────────────────────

/// Vertical life tube beside the notefield, with a heart underneath.
pub fn draw_life_gauge(painter: &Painter, s: &Stage, hud: &HudAnim, no_fail: bool, failed: bool) {
    let layout = s.layout;
    let height = (s.rect.height() * 0.5).clamp(160.0, 420.0);
    let x = layout.left - 12.0 - 30.0;
    let tube = Rect::from_center_size(pos2(x, s.rect.center().y), vec2(10.0, height));
    let life = hud.life.value.clamp(0.0, 1.0);
    let danger = life < 0.25;
    // Calm warning pulse (~1.5 Hz), never a strobe.
    let pulse = if danger && !s.reduce {
        0.5 + 0.5 * (s.time * TAU * 1.5).sin()
    } else {
        0.0
    };

    painter.rect_filled(
        tube.expand(3.0),
        theme::RADIUS_PILL,
        Color32::from_black_alpha(140),
    );
    painter.rect_stroke(
        tube.expand(3.0),
        theme::RADIUS_PILL,
        Stroke::new(
            1.0,
            Color32::from_white_alpha(22).lerp_to_gamma(theme::DANGER, pulse * 0.8),
        ),
        StrokeKind::Inside,
    );
    let yellow = Color32::from_rgb(0xFF, 0xE4, 0x5C);
    let color = if life < 0.5 {
        theme::DANGER.lerp_to_gamma(yellow, life / 0.5)
    } else {
        yellow.lerp_to_gamma(theme::SUCCESS, (life - 0.5) / 0.5)
    };
    if life > 0.0 {
        let fill = Rect::from_min_max(
            pos2(tube.left(), tube.bottom() - tube.height() * life),
            tube.max,
        );
        widgets::vertical_gradient(
            painter,
            fill.shrink2(vec2(0.0, 2.0)),
            color.lerp_to_gamma(Color32::WHITE, 0.25),
            color.gamma_multiply(0.7),
        );
        painter.circle_filled(
            fill.center_top() + vec2(0.0, 5.0),
            5.0,
            color.lerp_to_gamma(Color32::WHITE, 0.3),
        );
        painter.circle_filled(
            fill.center_bottom() - vec2(0.0, 5.0),
            5.0,
            color.gamma_multiply(0.7),
        );
        draw_glow(
            painter,
            fill.center_top() + vec2(0.0, 5.0),
            10.0,
            color,
            1.0 + pulse,
        );
    }

    let heart_c = tube.center_bottom() + vec2(0.0, 24.0);
    draw_heart(
        painter,
        heart_c,
        9.0,
        if danger {
            theme::DANGER.gamma_multiply(0.6 + 0.4 * pulse)
        } else {
            color
        },
    );
    if no_fail {
        painter.text(
            heart_c + vec2(0.0, 18.0),
            Align2::CENTER_TOP,
            if failed { "No Fail\nhabis" } else { "No Fail" },
            font(10.0),
            Color32::WHITE.gamma_multiply(0.4),
        );
    }
}

/// A small vector heart.
fn draw_heart(painter: &Painter, center: Pos2, size: f32, color: Color32) {
    let r = size * 0.55;
    painter.circle_filled(center + vec2(-r * 0.9, -r * 0.35), r, color);
    painter.circle_filled(center + vec2(r * 0.9, -r * 0.35), r, color);
    painter.add(Shape::convex_polygon(
        vec![
            center + vec2(-r * 1.86, -r * 0.1),
            center + vec2(r * 1.86, -r * 0.1),
            center + vec2(0.0, size * 1.25),
        ],
        color,
        Stroke::NONE,
    ));
}

// ── Center feedback ─────────────────────────────────────────────────────

/// Hit-error meter: where recent presses landed relative to the beat.
/// Center = perfect, left = early, right = late. Helps players self-calibrate.
pub fn draw_hit_meter(
    painter: &Painter,
    center: Pos2,
    hud: &HudAnim,
    judge_scale: f64,
    now: Instant,
) {
    let half = 120.0;
    let max_ms = (WINDOWS[4] * judge_scale * 1000.0) as f32;
    let visible = hud
        .hits
        .iter()
        .any(|(_, _, at)| anim::since(*at, now) < METER_MEMORY);
    if !visible {
        return;
    }
    // Window zones, widest first.
    for (i, w) in WINDOWS.iter().enumerate().rev() {
        let ww = (*w * judge_scale * 1000.0) as f32 / max_ms * half;
        let color = judgment_color(Judgment::ALL[i]).gamma_multiply(0.16);
        painter.rect_filled(
            Rect::from_center_size(center, vec2(ww * 2.0, 4.0)),
            theme::RADIUS_PILL,
            color,
        );
    }
    painter.line_segment(
        [center - vec2(0.0, 9.0), center + vec2(0.0, 9.0)],
        Stroke::new(1.5, Color32::WHITE.gamma_multiply(0.6)),
    );
    let mut sum = 0.0;
    let mut n = 0.0;
    for (ms, color, at) in &hud.hits {
        let age = anim::since(*at, now);
        if age > METER_MEMORY {
            continue;
        }
        let fade = 1.0 - age / METER_MEMORY;
        let x = center.x + (ms / max_ms).clamp(-1.0, 1.0) * half;
        painter.rect_filled(
            Rect::from_center_size(pos2(x, center.y), vec2(2.5, 14.0)),
            1,
            color.gamma_multiply(fade),
        );
        sum += ms;
        n += 1.0;
    }
    if n > 0.0 {
        let avg = sum / n;
        let x = center.x + (avg / max_ms).clamp(-1.0, 1.0) * half;
        painter.add(Shape::convex_polygon(
            vec![
                pos2(x, center.y - 11.0),
                pos2(x - 5.0, center.y - 18.0),
                pos2(x + 5.0, center.y - 18.0),
            ],
            Color32::WHITE.gamma_multiply(0.85),
            Stroke::NONE,
        ));
    }
    painter.text(
        center + vec2(-half, 14.0),
        Align2::LEFT_TOP,
        "cepat",
        font(10.0),
        Color32::WHITE.gamma_multiply(0.35),
    );
    painter.text(
        center + vec2(half, 14.0),
        Align2::RIGHT_TOP,
        "lambat",
        font(10.0),
        Color32::WHITE.gamma_multiply(0.35),
    );
}

/// Words of encouragement at combo milestones.
pub fn milestone_word(combo: u32) -> &'static str {
    match combo {
        c if c >= 500 => "Legendaris!",
        c if c >= 300 => "Tak terhentikan!",
        c if c >= 200 => "Luar biasa!",
        c if c >= 100 => "Hebat sekali!",
        _ => "Mantap!",
    }
}

/// Draws a circular arc (angles in radians, 0 = right, clockwise on screen).
pub fn draw_arc(
    painter: &Painter,
    center: Pos2,
    radius: f32,
    start: f32,
    sweep: f32,
    stroke: Stroke,
) {
    if sweep.abs() < 0.001 {
        return;
    }
    let steps = ((sweep.abs() / TAU) * 96.0).ceil().max(2.0) as usize;
    let points: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let a = start + sweep * i as f32 / steps as f32;
            center + vec2(a.cos(), a.sin()) * radius
        })
        .collect();
    painter.add(PathShape::line(points, stroke));
}

/// Lead-in countdown: a ring that drains each second around a popping number.
pub fn draw_countdown(painter: &Painter, center: Pos2, seconds_left: f64, reduce: bool) {
    let n = seconds_left.ceil() as i32;
    if n < 1 {
        return;
    }
    let frac = seconds_left.fract() as f32; // 1 → 0 within each second
    let frac = if frac == 0.0 { 1.0 } else { frac };
    let pop = if reduce {
        1.0
    } else {
        1.0 + 0.25 * anim::ease_out_cubic(((frac - 0.8) / 0.2).clamp(0.0, 1.0))
    };
    let r = 64.0;
    painter.circle_filled(center, r + 8.0, Color32::from_black_alpha(90));
    painter.circle_stroke(center, r, Stroke::new(4.0, Color32::from_white_alpha(20)));
    draw_arc(
        painter,
        center,
        r,
        -FRAC_PI_2,
        TAU * frac,
        Stroke::new(4.0, ACCENT_B.lerp_to_gamma(ACCENT_A, 1.0 - frac)),
    );
    painter.text(
        center,
        Align2::CENTER_CENTER,
        n.to_string(),
        font_bold(56.0 * pop),
        Color32::WHITE.gamma_multiply(0.95),
    );
    widgets::text_tracked(
        painter,
        center + vec2(0.0, r + 30.0),
        Align2::CENTER_CENTER,
        "BERSIAP",
        font_bold(theme::TEXT_XS),
        Color32::WHITE.gamma_multiply(0.55),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grade_follows_accuracy() {
        assert_eq!(live_grade(100.0, false).0, "SS");
        assert_eq!(live_grade(96.0, false).0, "S");
        assert_eq!(live_grade(50.0, false).0, "D");
        assert_eq!(live_grade(100.0, true).0, "F");
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

//! Draws receptors, arrows, holds, mines and hit effects with vector shapes.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use eframe::egui::{
    Color32, Mesh, Painter, Pos2, Rect, Shape, Stroke, Vec2, epaint::PathShape, pos2, vec2,
};

use super::theme::column_color;
use crate::config::{ScrollMode, Settings};
use crate::gameplay::{HoldState, NoteState};
use crate::parser::NoteKind;
use crate::timing::TimingMap;

/// Rotation of the arrow for each column (the base arrow points up).
fn column_rotation(column: u8) -> f32 {
    match column % 4 {
        0 => -FRAC_PI_2, // left
        1 => PI,         // down
        2 => 0.0,        // up
        _ => FRAC_PI_2,  // right
    }
}

/// Arrow outline (pointing up) in unit coordinates, centered on (0, 0).
const ARROW: [(f32, f32); 7] = [
    (0.0, -0.46),
    (0.44, -0.02),
    (0.17, -0.02),
    (0.17, 0.44),
    (-0.17, 0.44),
    (-0.17, -0.02),
    (-0.44, -0.02),
];

fn arrow_points(center: Pos2, size: f32, rotation: f32) -> Vec<Pos2> {
    let (s, c) = rotation.sin_cos();
    ARROW
        .iter()
        .map(|&(x, y)| center + vec2(x * c - y * s, x * s + y * c) * size)
        .collect()
}

/// Draws a filled arrow. The concave outline is filled as head triangle + shaft quad.
pub fn draw_arrow(
    painter: &Painter,
    center: Pos2,
    size: f32,
    column: u8,
    fill: Color32,
    stroke: Stroke,
) {
    let p = arrow_points(center, size, column_rotation(column));
    if fill.a() > 0 {
        painter.add(Shape::convex_polygon(
            vec![p[0], p[1], p[6]],
            fill,
            Stroke::NONE,
        ));
        painter.add(Shape::convex_polygon(
            vec![p[2], p[3], p[4], p[5]],
            fill,
            Stroke::NONE,
        ));
    }
    if stroke.width > 0.0 {
        painter.add(PathShape::closed_line(p, stroke));
    }
}

/// Soft glow around a point (`strength` roughly 0..2).
pub fn draw_glow(painter: &Painter, center: Pos2, radius: f32, color: Color32, strength: f32) {
    radial_light(painter, center, radius * 1.4, color, strength * 0.32);
}

/// Smooth radial light: one mesh with a Gaussian-like falloff, no visible rings.
pub fn radial_light(painter: &Painter, center: Pos2, radius: f32, color: Color32, strength: f32) {
    const SEGMENTS: u32 = 64;
    // (radius fraction, intensity): close to a Gaussian falloff.
    const RINGS: [(f32, f32); 4] = [(0.25, 0.8), (0.5, 0.45), (0.75, 0.15), (1.0, 0.0)];
    let strength = strength.clamp(0.0, 1.0);
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, color.gamma_multiply(strength));
    for (f, k) in RINGS {
        for i in 0..SEGMENTS {
            let a = i as f32 / SEGMENTS as f32 * TAU;
            mesh.colored_vertex(
                center + vec2(a.cos(), a.sin()) * radius * f,
                color.gamma_multiply(strength * k),
            );
        }
    }
    let ring = |r: u32, i: u32| 1 + r * SEGMENTS + i % SEGMENTS;
    for i in 0..SEGMENTS {
        mesh.add_triangle(0, ring(0, i), ring(0, i + 1));
        for r in 0..RINGS.len() as u32 - 1 {
            mesh.add_triangle(ring(r, i), ring(r + 1, i), ring(r + 1, i + 1));
            mesh.add_triangle(ring(r, i), ring(r + 1, i + 1), ring(r, i + 1));
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// Geometry of the notefield for the current frame.
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub arrow: f32,
    pub lane: f32,
    pub left: f32,
    pub receptor_y: f32,
    /// Arrow size before the lane-size setting: scroll speed is based on it,
    /// so a bigger lane does not shorten the time to read the notes.
    pub base_arrow: f32,
    /// +1 when notes travel upwards, -1 in reverse mode.
    pub dir: f32,
    pub rect: Rect,
}

impl Layout {
    /// Lane centered in `rect`; `scale` is the lane-size setting
    /// ([`crate::config::LaneSize::scale`]), limited so the lane always fits.
    pub fn new(rect: Rect, columns: u8, reverse: bool, scale: f32) -> Self {
        let base_arrow = (rect.height() / 8.5).clamp(44.0, 112.0);
        let fit_width = rect.width() * 0.9 / (1.12 * f32::from(columns.max(1)));
        let arrow = (base_arrow * scale.clamp(0.5, 2.0))
            .min(fit_width)
            .min(rect.height() / 5.5)
            .max(base_arrow.min(fit_width));
        let lane = arrow * 1.12;
        let width = lane * f32::from(columns);
        let receptor_y = if reverse {
            rect.bottom() - arrow * 1.3
        } else {
            rect.top() + arrow * 1.3
        };
        Self {
            arrow,
            lane,
            left: rect.center().x - width / 2.0,
            receptor_y,
            base_arrow,
            dir: if reverse { -1.0 } else { 1.0 },
            rect,
        }
    }

    pub fn column_x(&self, column: u8) -> f32 {
        self.left + self.lane * (f32::from(column) + 0.5)
    }

    pub fn width(&self, columns: u8) -> f32 {
        self.lane * f32::from(columns)
    }

    /// Shrinks a scaled-up lane to at most `max_width` (e.g. to keep clear
    /// of HUD cards in a small window), never below the normal size.
    pub fn limit_width(mut self, columns: u8, max_width: f32) -> Self {
        let cols = f32::from(columns.max(1));
        let arrow = (max_width / (1.12 * cols))
            .max(self.base_arrow)
            .min(self.arrow);
        let center = self.left + self.lane * cols / 2.0;
        let edge = self.receptor_y - self.dir * self.arrow * 1.3;
        self.arrow = arrow;
        self.lane = arrow * 1.12;
        self.left = center - self.lane * cols / 2.0;
        self.receptor_y = edge + self.dir * arrow * 1.3;
        self
    }

    /// How much bigger than normal the lane is drawn (1.0 = normal).
    pub fn scale(&self) -> f32 {
        self.arrow / self.base_arrow
    }
}

/// Converts note positions to screen distance from the receptor.
pub struct Scroll<'a> {
    pub mode: ScrollMode,
    /// Pixels per second (CMod) or pixels per beat (XMod).
    pub speed: f32,
    pub timing: &'a TimingMap,
    pub now_time: f64,
    pub now_beat: f64,
}

impl<'a> Scroll<'a> {
    pub fn new(settings: &Settings, layout: &Layout, timing: &'a TimingMap, now_time: f64) -> Self {
        let speed = match settings.scroll_mode {
            ScrollMode::CMod => settings.cmod / 60.0 * layout.base_arrow,
            ScrollMode::XMod => settings.xmod * layout.base_arrow,
        };
        Self {
            mode: settings.scroll_mode,
            speed,
            timing,
            now_time,
            now_beat: timing.seconds_to_beat(now_time),
        }
    }

    /// Distance in pixels from the receptor for something at `time`/`beat`.
    pub fn distance(&self, time: f64, beat: f64) -> f32 {
        match self.mode {
            ScrollMode::CMod => (time - self.now_time) as f32 * self.speed,
            ScrollMode::XMod => (beat - self.now_beat) as f32 * self.speed,
        }
    }
}

/// Draws the four receptors. `pressed` lights them; `beat_pulse` 0..1 makes them breathe.
pub fn draw_receptors(
    painter: &Painter,
    layout: &Layout,
    columns: u8,
    pressed: &[bool],
    beat_pulse: f32,
    light: bool,
) {
    let base = if light {
        Color32::from_gray(60)
    } else {
        Color32::from_gray(200)
    };
    for c in 0..columns {
        let center = pos2(layout.column_x(c), layout.receptor_y);
        let color = column_color(c);
        let is_pressed = pressed.get(usize::from(c)).copied().unwrap_or(false);
        let size = layout.arrow * if is_pressed { 0.9 } else { 1.0 };
        draw_glow(
            painter,
            center,
            layout.arrow * 0.55,
            color,
            0.35 + beat_pulse * 0.5 + if is_pressed { 0.8 } else { 0.0 },
        );
        let fill = if is_pressed {
            color.gamma_multiply(0.45)
        } else {
            base.gamma_multiply(0.06)
        };
        let stroke = Stroke::new(
            2.5 + beat_pulse * 1.5,
            if is_pressed {
                color
            } else {
                base.gamma_multiply(0.5 + beat_pulse * 0.4)
            },
        );
        draw_arrow(painter, center, size, c, fill, stroke);
    }
}

/// Draws all visible notes.
pub fn draw_notes(painter: &Painter, layout: &Layout, scroll: &Scroll, notes: &[NoteState]) {
    let clip = layout.rect.expand(layout.arrow);
    let visible = |y: f32| y >= clip.top() && y <= clip.bottom();
    let y_at = |d: f32| layout.receptor_y + d * layout.dir;

    for state in notes {
        let n = &state.note;
        let head_d = scroll.distance(n.time, n.beat);
        let tail_d = scroll.distance(n.end_time, n.end_beat);
        // Fully past the top (or bottom in reverse) and finished: skip.
        if tail_d < -layout.arrow * 2.0 && !matches!(state.hold, HoldState::Holding { .. }) {
            continue;
        }
        if head_d > clip.height() + layout.arrow * 2.0 {
            // Notes are sorted by time, but XMod with BPM changes can reorder distances slightly.
            if scroll.mode == ScrollMode::CMod {
                break;
            }
            continue;
        }
        let x = layout.column_x(n.column);
        let color = column_color(n.column);

        match n.kind {
            NoteKind::Mine => {
                if state.mine_hit || head_d < -layout.arrow {
                    continue;
                }
                let y = y_at(head_d);
                if visible(y) {
                    draw_mine(painter, pos2(x, y), layout.arrow, scroll.now_time as f32);
                }
            }
            NoteKind::HoldHead { .. } | NoteKind::RollHead { .. } => {
                let is_roll = matches!(n.kind, NoteKind::RollHead { .. });
                let holding = matches!(state.hold, HoldState::Holding { .. });
                if state.hold == HoldState::Ok {
                    continue;
                }
                let head_y = if holding {
                    layout.receptor_y
                } else {
                    y_at(head_d)
                };
                let tail_y = y_at(tail_d.max(if holding { 0.0 } else { head_d }));
                let dimmed = state.hold == HoldState::Ng;
                draw_hold_body(
                    painter,
                    x,
                    head_y,
                    tail_y,
                    layout.arrow,
                    color,
                    is_roll,
                    dimmed,
                );
                if !holding && state.judgment.is_none() && visible(head_y) {
                    draw_note(painter, pos2(x, head_y), layout.arrow, n.column, color);
                } else if holding {
                    draw_note(
                        painter,
                        pos2(x, head_y),
                        layout.arrow * 0.92,
                        n.column,
                        color,
                    );
                }
            }
            NoteKind::Fake | NoteKind::Tap | NoteKind::Lift => {
                if state
                    .judgment
                    .is_some_and(|j| j != crate::gameplay::judge::Judgment::Miss)
                {
                    continue;
                }
                let y = y_at(head_d);
                if visible(y) {
                    let c = if n.kind == NoteKind::Fake {
                        color.gamma_multiply(0.4)
                    } else {
                        color
                    };
                    draw_note(painter, pos2(x, y), layout.arrow, n.column, c);
                    if n.kind == NoteKind::Lift {
                        painter.circle_stroke(
                            pos2(x, y),
                            layout.arrow * 0.5,
                            Stroke::new(2.0, Color32::WHITE),
                        );
                    }
                }
            }
        }
    }
}

fn draw_note(painter: &Painter, center: Pos2, size: f32, column: u8, color: Color32) {
    draw_glow(painter, center, size * 0.45, color, 0.6);
    draw_arrow(
        painter,
        center,
        size * 0.94,
        column,
        color,
        Stroke::new(2.0, Color32::WHITE.gamma_multiply(0.85)),
    );
    // inner highlight
    draw_arrow(
        painter,
        center + vec2(0.0, 0.0),
        size * 0.42,
        column,
        Color32::WHITE.gamma_multiply(0.35),
        Stroke::NONE,
    );
}

#[allow(clippy::too_many_arguments)] // drawing helper; grouping into a struct adds no clarity
fn draw_hold_body(
    painter: &Painter,
    x: f32,
    head_y: f32,
    tail_y: f32,
    arrow: f32,
    color: Color32,
    roll: bool,
    dimmed: bool,
) {
    let (top, bottom) = if head_y < tail_y {
        (head_y, tail_y)
    } else {
        (tail_y, head_y)
    };
    if bottom - top < 1.0 {
        return;
    }
    let w = arrow * 0.5;
    let alpha = if dimmed { 0.18 } else { 0.55 };
    let rect = Rect::from_min_max(pos2(x - w / 2.0, top), pos2(x + w / 2.0, bottom));
    painter.rect_filled(rect, (w / 2.0) as u8, color.gamma_multiply(alpha));
    if roll {
        let step = arrow * 0.35;
        let mut y = top;
        while y < bottom {
            let y2 = (y + step * 0.5).min(bottom);
            painter.rect_filled(
                Rect::from_min_max(pos2(rect.left(), y), pos2(rect.right(), y2)),
                0,
                Color32::WHITE.gamma_multiply(if dimmed { 0.05 } else { 0.18 }),
            );
            y += step;
        }
    }
    // tail cap
    let cap = if head_y < tail_y { bottom } else { top };
    painter.circle_filled(pos2(x, cap), w / 2.0, color.gamma_multiply(alpha + 0.2));
}

fn draw_mine(painter: &Painter, center: Pos2, size: f32, t: f32) {
    let r = size * 0.3;
    let red = Color32::from_rgb(0xFF, 0x4D, 0x6A);
    draw_glow(painter, center, r * 1.4, red, 0.7);
    painter.circle_filled(center, r, Color32::from_rgb(20, 16, 28));
    painter.circle_stroke(center, r, Stroke::new(3.0, red));
    for i in 0..6 {
        let a = t * 2.0 + i as f32 / 6.0 * TAU;
        let d = Vec2::angled(a);
        painter.line_segment(
            [center + d * r * 0.35, center + d * r * 0.8],
            Stroke::new(2.0, red),
        );
    }
}

/// A short-lived burst of light where a note was hit.
#[derive(Debug, Clone, Copy)]
pub struct HitEffect {
    pub column: u8,
    pub color: Color32,
    /// Seconds since the hit.
    pub age: f32,
    /// Particle seed so each burst looks a bit different.
    pub seed: u32,
}

pub const HIT_EFFECT_LEN: f32 = 0.35;

/// Draws expanding ring + sparks for a hit.
pub fn draw_hit_effect(painter: &Painter, layout: &Layout, fx: &HitEffect) {
    let t = (fx.age / HIT_EFFECT_LEN).clamp(0.0, 1.0);
    let e = super::anim::ease_out_cubic(t);
    let center = pos2(layout.column_x(fx.column), layout.receptor_y);
    let fade = 1.0 - t;
    draw_glow(
        painter,
        center,
        layout.arrow * (0.5 + e * 0.4),
        fx.color,
        fade * 2.0,
    );
    painter.circle_stroke(
        center,
        layout.arrow * (0.45 + e * 0.45),
        Stroke::new(4.0 * fade, fx.color.gamma_multiply(fade)),
    );
    for i in 0..8u32 {
        let h = fx.seed.wrapping_mul(2_654_435_761).wrapping_add(i * 40_503);
        let jitter = (h % 1000) as f32 / 1000.0;
        let a = i as f32 / 8.0 * TAU + jitter * 0.6;
        let dist = layout.arrow * (0.4 + e * (0.5 + jitter * 0.5));
        let p = center + Vec2::angled(a) * dist;
        painter.circle_filled(p, 2.5 * fade + 0.5, fx.color.gamma_multiply(fade));
    }
}

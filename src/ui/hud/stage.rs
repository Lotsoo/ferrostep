//! The stage behind the notes: side lights, lane, particles and the milestone surge.

use super::*;

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
    /// Song beat (negative during the lead-in).
    pub beat: f64,
}

/// Background, ambient light and drifting particles.
/// Neon highway and equalizers beside the notefield (never behind it, so
/// arrows stay easy to read). They grow livelier with the combo.
pub(super) fn draw_side_stage(painter: &Painter, s: &Stage, light: f32) {
    let beat = crate::ui::beatfx::Beat::at(s.beat, 120.0, s.beat >= 0.0);
    let field = Rect::from_min_max(
        pos2(s.layout.left - 24.0, s.rect.top()),
        pos2(
            s.layout.left + s.layout.width(s.columns) + 24.0,
            s.rect.bottom(),
        ),
    );
    let tint = s.tint.gamma_multiply(light * (0.55 + 0.45 * s.energy));
    for side in [
        Rect::from_min_max(s.rect.min, pos2(field.left(), s.rect.bottom())),
        Rect::from_min_max(pos2(field.right(), s.rect.top()), s.rect.max),
    ] {
        if side.width() < 40.0 {
            continue;
        }
        crate::ui::beatfx::draw_highway(&painter.with_clip_rect(side), s.rect, tint, &beat);
        let eq = Rect::from_min_max(
            pos2(side.left() + 24.0, side.bottom() - 40.0 - 50.0 * s.energy),
            pos2(side.right() - 24.0, side.bottom() - 12.0),
        );
        crate::ui::beatfx::draw_equalizer(
            painter,
            eq,
            (
                s.tint.gamma_multiply(0.5 * light),
                ACCENT_B.gamma_multiply(0.5 * light),
            ),
            &beat,
        );
    }
}

/// Combo milestone celebration: a band of light in the four arrow colors
/// sweeps along the lane away from the receptors, and sparks burst from both
/// lane edges. Drawn under the notes so arrows stay readable.
pub fn draw_milestone_surge(painter: &Painter, s: &Stage, age: f32) {
    if age > 1.0 {
        return;
    }
    let width = s.layout.width(s.columns);
    let left = s.layout.left;
    let lane = Rect::from_min_max(
        pos2(left, s.rect.top()),
        pos2(left + width, s.rect.bottom()),
    );
    let lane_painter = painter.with_clip_rect(lane);
    // Sweep: from the receptor line toward the far end of the lane.
    let e = anim::ease_out_cubic((age / 0.7).min(1.0));
    let far = if s.layout.dir > 0.0 {
        s.rect.bottom()
    } else {
        s.rect.top()
    };
    let y = s.layout.receptor_y + (far - s.layout.receptor_y) * e;
    let fade = 1.0 - (age / 1.0);
    let band = 70.0;
    for c in 0..s.columns.min(4) {
        let col = Rect::from_min_max(
            pos2(left + s.layout.lane * f32::from(c), y - band),
            pos2(left + s.layout.lane * f32::from(c + 1), y + band),
        );
        let color = column_color(c);
        widgets::vertical_gradient(
            &lane_painter,
            Rect::from_min_max(col.min, pos2(col.max.x, y)),
            Color32::TRANSPARENT,
            color.gamma_multiply(0.35 * fade),
        );
        widgets::vertical_gradient(
            &lane_painter,
            Rect::from_min_max(pos2(col.min.x, y), col.max),
            color.gamma_multiply(0.35 * fade),
            Color32::TRANSPARENT,
        );
    }
    lane_painter.line_segment(
        [pos2(lane.left(), y), pos2(lane.right(), y)],
        Stroke::new(2.0, Color32::WHITE.gamma_multiply(0.7 * fade)),
    );
    // Sparks from both edges of the lane, at the receptors.
    let base_y = s.layout.receptor_y;
    for side in [-1.0f32, 1.0] {
        let x = if side < 0.0 {
            lane.left()
        } else {
            lane.right()
        };
        for i in 0..10 {
            let a = (i as f32 / 9.0 - 0.5) * 1.4; // spread angle
            let dir = vec2(side * a.cos(), -(a.sin() * s.layout.dir.signum()));
            let d = 20.0 + e * (90.0 + (i % 3) as f32 * 40.0);
            let color = column_color((i % 4) as u8);
            painter.circle_filled(
                pos2(x, base_y) + dir * d,
                3.0 * (1.0 - e) + 0.5,
                color.gamma_multiply(fade),
            );
        }
    }
}

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
        draw_side_stage(painter, s, light);
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
pub(super) fn soft_light(
    painter: &Painter,
    center: Pos2,
    radius: f32,
    color: Color32,
    strength: f32,
) {
    crate::ui::notefield::radial_light(painter, center, radius, color, strength);
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
pub(super) fn draw_particles(painter: &Painter, s: &Stage) {
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

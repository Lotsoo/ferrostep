//! Glass HUD islands: now playing, score and grade, life gauge and the hit-error meter.

use super::*;

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
        crate::ui::widgets::truncate(info.title, 24),
        font_bold(theme::TEXT_M - 1.0),
        Color32::WHITE.gamma_multiply(0.95),
    );
    painter.circle_filled(pos2(x + 4.0, rect.top() + 45.0), 4.0, info.difficulty_color);
    painter.text(
        pos2(x + 14.0, rect.top() + 45.0),
        Align2::LEFT_CENTER,
        crate::ui::widgets::truncate(info.subtitle, 30),
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

/// Live grade from the accuracy so far (same thresholds as the results).
pub(super) fn live_grade(accuracy: f64, failed: bool) -> &'static str {
    match accuracy {
        _ if failed => "F",
        a if a >= 99.0 => "SS",
        a if a >= 95.0 => "S",
        a if a >= 90.0 => "A",
        a if a >= 80.0 => "B",
        a if a >= 70.0 => "C",
        _ => "D",
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

    let grade = live_grade(score.accuracy_percent(), failed);
    let badge = Rect::from_min_size(rect.min + vec2(14.0, 14.0), vec2(58.0, 58.0));
    let time = painter.ctx().input(|i| i.time) as f32;
    draw_grade_badge(painter, badge, grade, hud.grade_pop(), time, hud.reduce);

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

/// Grade badge: a bright gradient in the grade's own color (gold SS, orange
/// S, green A, blue B, violet C, grey D, red F), a soft glow, a light sweep,
/// and a letter in whichever ink reads best on it. `pop` (0..1) briefly
/// enlarges it when the grade changes.
pub fn draw_grade_badge(
    painter: &Painter,
    badge: Rect,
    grade: &str,
    pop: f32,
    time: f32,
    reduce: bool,
) {
    let style = theme::grade_style(grade);
    let badge = Rect::from_center_size(badge.center(), badge.size() * (1.0 + 0.14 * pop));
    let radius = (badge.width() * 0.24) as u8;
    crate::ui::notefield::radial_light(
        painter,
        badge.center(),
        badge.width() * (1.15 + 0.3 * pop),
        style.glow,
        0.45 + 0.4 * pop,
    );
    gradient_tile(painter, badge, radius, style.top, style.bottom);
    // Glossy top half and a bright rim.
    let gloss = Rect::from_min_max(badge.min, pos2(badge.right(), badge.center().y));
    painter.with_clip_rect(gloss).rect_filled(
        badge.shrink(2.0),
        radius,
        Color32::WHITE.gamma_multiply(0.16),
    );
    painter.rect_stroke(
        badge,
        radius,
        Stroke::new(1.5, Color32::WHITE.gamma_multiply(0.55)),
        StrokeKind::Inside,
    );
    // A diagonal light sweep every few seconds.
    if !reduce {
        let cycle = (time / 3.2).fract();
        if cycle < 0.3 {
            let x = badge.left() - badge.width() * 0.4 + badge.width() * 1.8 * (cycle / 0.3);
            let w = badge.width() * 0.18;
            let h = badge.height();
            painter
                .with_clip_rect(badge.shrink(2.0))
                .add(Shape::convex_polygon(
                    vec![
                        pos2(x, badge.bottom()),
                        pos2(x + w, badge.bottom()),
                        pos2(x + w + h * 0.5, badge.top()),
                        pos2(x + h * 0.5, badge.top()),
                    ],
                    Color32::WHITE.gamma_multiply(0.35),
                    Stroke::NONE,
                ));
        }
    }
    let size = badge.height() * if grade.len() > 1 { 0.46 } else { 0.58 };
    widgets::text_tracked(
        painter,
        badge.center() + vec2(0.0, size * 0.03),
        Align2::CENTER_CENTER,
        grade,
        font_bold(size),
        style.text,
    );
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
pub(super) fn draw_heart(painter: &Painter, center: Pos2, size: f32, color: Color32) {
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

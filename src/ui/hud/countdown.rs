//! Lead-in before the music: the "get ready" card and the 3-2-1 count.

use super::*;

/// Total lead-in before the music starts (seconds): a 2 s "get ready" card,
/// then 3, 2, 1.
pub const LEAD_IN: f64 = 5.0;

/// What the "get ready" card shows.
pub struct CountdownInfo<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    pub tint: Color32,
}

/// Number color: each count feels different (cyan, violet, pink), "GO" is gold.
pub(super) fn count_color(n: i32) -> Color32 {
    match n {
        3 => ACCENT_B,
        2 => ACCENT_A,
        1 => Color32::from_rgb(0xFF, 0x5C, 0x8A),
        _ => Color32::from_rgb(0xFF, 0xD8, 0x4D),
    }
}

/// Text with a dark outline so it reads over anything, drawn at `scale` ×
/// `size` (scaled as shapes, see `widgets::scale_shape`).
pub(super) fn outlined(
    painter: &Painter,
    pos: Pos2,
    text: &str,
    size: f32,
    scale: f32,
    color: Color32,
    alpha: f32,
) {
    let outline = Color32::from_black_alpha((170.0 * alpha) as u8);
    let o = (size * scale * 0.035).max(1.5);
    for d in [
        vec2(o, 0.0),
        vec2(-o, 0.0),
        vec2(0.0, o),
        vec2(0.0, -o),
        vec2(0.0, o * 2.0),
    ] {
        widgets::text_tracked_scaled(
            painter,
            pos + d,
            Align2::CENTER_CENTER,
            text,
            font_bold(size),
            outline,
            scale,
        );
    }
    widgets::text_tracked_scaled(
        painter,
        pos,
        Align2::CENTER_CENTER,
        text,
        font_bold(size),
        color.gamma_multiply(alpha),
        scale,
    );
}

/// The countdown. `song_time` runs from −[`LEAD_IN`] to about +0.9 s:
/// get-ready card → 3, 2, 1 (each slams in with a shockwave and a flash)
/// → "MULAI!".
pub fn draw_countdown(
    painter: &Painter,
    rect: Rect,
    song_time: f64,
    info: &CountdownInfo,
    reduce: bool,
) {
    let center = rect.center();
    if song_time < -3.0 {
        draw_get_ready(painter, rect, song_time, info, reduce);
        return;
    }
    let (n, age) = if song_time < 0.0 {
        let n = (-song_time).ceil() as i32;
        (n, (song_time + f64::from(n)) as f32) // 0 → 1 within this second
    } else {
        (0, song_time as f32)
    };
    let life = if n == 0 { 0.9 } else { 1.0 };
    if age > life {
        return;
    }
    let color = count_color(n);
    let fade = if n == 0 {
        (1.0 - (age - 0.5).max(0.0) / 0.4).clamp(0.0, 1.0)
    } else {
        (1.0 - (age - 0.75).max(0.0) / 0.25).clamp(0.0, 1.0)
    };

    if !reduce {
        // Flash: the whole screen lights up for an instant in the number's color.
        let flash = (1.0 - age / 0.22).max(0.0);
        if flash > 0.0 {
            painter.rect_filled(rect, 0, color.gamma_multiply(0.22 * flash));
        }
        // Shockwave rings.
        let e = anim::ease_out_cubic((age / 0.6).min(1.0));
        for (k, delay) in [(0.0f32, 0.0f32), (1.0, 0.08)] {
            let e2 = anim::ease_out_cubic(((age - delay) / 0.6).clamp(0.0, 1.0));
            if e2 <= 0.0 {
                continue;
            }
            painter.circle_stroke(
                center,
                70.0 + e2 * rect.height() * 0.55,
                Stroke::new(
                    (8.0 - k * 3.0) * (1.0 - e2),
                    color.gamma_multiply((1.0 - e2) * 0.9),
                ),
            );
        }
        // Burst of sparks.
        for i in 0..18 {
            let a = i as f32 / 18.0 * TAU + n as f32;
            let d = 80.0 + e * (180.0 + (i % 3) as f32 * 60.0);
            painter.circle_filled(
                center + vec2(a.cos(), a.sin()) * d,
                3.5 * (1.0 - e) + 0.5,
                color
                    .lerp_to_gamma(Color32::WHITE, 0.4)
                    .gamma_multiply(1.0 - e),
            );
        }
    }
    // Dark halo so the number is the brightest thing on screen.
    crate::ui::notefield::radial_light(painter, center, 260.0, Color32::BLACK, 0.55 * fade);
    crate::ui::notefield::radial_light(painter, center, 200.0, color, 0.45 * fade);

    // Slam: starts big, lands with a small overshoot.
    let slam = if reduce {
        1.0
    } else {
        let land = anim::ease_out_back((age / 0.22).min(1.0));
        2.2 - 1.2 * land - 0.06 * (age - 0.22).max(0.0)
    };
    let (text, size) = if n == 0 {
        ("MULAI!", 104.0)
    } else {
        (["", "1", "2", "3"][n as usize], 168.0)
    };
    outlined(
        painter,
        center,
        text,
        size,
        slam,
        color.lerp_to_gamma(Color32::WHITE, 0.25),
        fade,
    );
    if n > 0 {
        widgets::text_tracked(
            painter,
            center + vec2(0.0, 120.0),
            Align2::CENTER_CENTER,
            "BERSIAP",
            font_bold(theme::TEXT_S),
            Color32::WHITE.gamma_multiply(0.7 * fade),
        );
    }
}

/// First two seconds: song card, controls reminder and a filling bar.
pub(super) fn draw_get_ready(
    painter: &Painter,
    rect: Rect,
    song_time: f64,
    info: &CountdownInfo,
    reduce: bool,
) {
    let elapsed = (song_time + LEAD_IN) as f32; // 0 → 2
    let appear = if reduce {
        1.0
    } else {
        anim::ease_out_cubic((elapsed / 0.35).min(1.0))
    };
    let leave = ((2.0 - elapsed) / 0.2).clamp(0.0, 1.0);
    let alpha = appear * leave;
    let card = Rect::from_center_size(
        rect.center() + vec2(0.0, (1.0 - appear) * 30.0),
        vec2(460.0, 250.0),
    );
    painter.rect_filled(rect, 0, Color32::from_black_alpha((90.0 * alpha) as u8));
    crate::ui::notefield::radial_light(painter, card.center(), 380.0, info.tint, 0.35 * alpha);
    glass(painter, card);
    painter.rect_filled(
        card,
        theme::RADIUS_CARD,
        Color32::from_rgba_unmultiplied(22, 20, 40, (60.0 * (1.0 - alpha)) as u8),
    );
    widgets::text_tracked(
        painter,
        card.center_top() + vec2(0.0, 34.0),
        Align2::CENTER_CENTER,
        "BERSIAP",
        font_bold(theme::TEXT_S),
        ACCENT_B.gamma_multiply(alpha),
    );
    widgets::text_tracked(
        painter,
        card.center_top() + vec2(0.0, 76.0),
        Align2::CENTER_CENTER,
        &crate::ui::widgets::truncate(info.title, 26),
        font_bold(theme::TEXT_XL),
        Color32::WHITE.gamma_multiply(alpha),
    );
    painter.text(
        card.center_top() + vec2(0.0, 110.0),
        Align2::CENTER_CENTER,
        info.subtitle,
        font(theme::TEXT_S),
        Color32::WHITE.gamma_multiply(0.7 * alpha),
    );
    // Works for keyboard and dance mat alike: the four arrows, and what to do.
    for c in 0..4u8 {
        let center = card.center_top() + vec2((f32::from(c) - 1.5) * 44.0, 146.0);
        crate::ui::notefield::draw_arrow(
            painter,
            center,
            30.0,
            c,
            column_color(c).gamma_multiply(alpha),
            Stroke::new(1.5, Color32::WHITE.gamma_multiply(0.8 * alpha)),
        );
    }
    painter.text(
        card.center_top() + vec2(0.0, 176.0),
        Align2::CENTER_CENTER,
        "Tekan atau injak saat panah menyentuh sasaran",
        font_bold(theme::TEXT_S),
        Color32::WHITE.gamma_multiply(0.9 * alpha),
    );
    // Filling bar: how long until the count starts.
    let bar = Rect::from_center_size(
        card.center_bottom() - vec2(0.0, 30.0),
        vec2(card.width() - 80.0, 6.0),
    );
    painter.rect_filled(
        bar,
        theme::RADIUS_PILL,
        Color32::from_white_alpha((20.0 * alpha) as u8),
    );
    let p = (elapsed / 2.0).clamp(0.0, 1.0);
    if p > 0.0 {
        widgets::gradient_pill(
            painter,
            Rect::from_min_size(bar.min, vec2(bar.width() * p, bar.height())),
            ACCENT_A.gamma_multiply(alpha),
            ACCENT_B.gamma_multiply(alpha),
        );
    }
}

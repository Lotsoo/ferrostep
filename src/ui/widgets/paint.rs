//! Painting helpers: gradients, backgrounds, cards, elevation, progress bar and page dots.

use super::*;

/// Horizontal gradient rectangle (no rounding).
pub fn gradient_rect(painter: &Painter, rect: Rect, left: Color32, right: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), left);
    mesh.colored_vertex(rect.right_top(), right);
    mesh.colored_vertex(rect.right_bottom(), right);
    mesh.colored_vertex(rect.left_bottom(), left);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

/// Vertical gradient rectangle (no rounding).
pub fn vertical_gradient(painter: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

/// Pill-shaped horizontal gradient (rounded ends).
pub fn gradient_pill(painter: &Painter, rect: Rect, left: Color32, right: Color32) {
    let r = rect.height() / 2.0;
    if rect.width() <= rect.height() {
        painter.circle_filled(rect.center(), r.min(rect.width() / 2.0), left);
        return;
    }
    painter.circle_filled(pos2(rect.left() + r, rect.center().y), r, left);
    painter.circle_filled(pos2(rect.right() - r, rect.center().y), r, right);
    gradient_rect(
        painter,
        Rect::from_min_max(
            pos2(rect.left() + r, rect.top()),
            pos2(rect.right() - r, rect.bottom()),
        ),
        left,
        right,
    );
}

/// App background: soft vertical gradient plus two slow color blobs.
pub fn background(
    painter: &Painter,
    rect: Rect,
    palette: &Palette,
    time: f32,
    tint: Option<Color32>,
) {
    vertical_gradient(painter, rect, palette.bg_top, palette.bg_bottom);
    let a = tint.unwrap_or(ACCENT_A);
    let strength = if palette.dark { 0.10 } else { 0.07 };
    let blob = |center: Pos2, radius: f32, color: Color32| {
        for i in 0..6 {
            let f = 1.0 - i as f32 / 6.0;
            painter.circle_filled(
                center,
                radius * f,
                color.gamma_multiply(strength * (1.0 - f) * 0.9),
            );
        }
    };
    let w = rect.width();
    let h = rect.height();
    blob(
        pos2(
            rect.left() + w * (0.2 + 0.05 * (time * 0.13).sin()),
            rect.top() + h * 0.25,
        ),
        w * 0.45,
        a,
    );
    blob(
        pos2(
            rect.left() + w * (0.8 + 0.05 * (time * 0.11).cos()),
            rect.top() + h * 0.8,
        ),
        w * 0.4,
        ACCENT_B,
    );
}

/// Rounded card background.
pub fn card(painter: &Painter, rect: Rect, palette: &Palette) {
    // Bigger surfaces read as thicker material: deeper shadow.
    let level = if rect.area() > 200_000.0 {
        Elevation::High
    } else {
        Elevation::Medium
    };
    elevation(painter, rect, RADIUS_CARD, level);
    painter.rect_filled(rect, RADIUS_CARD, palette.surface);
    painter.rect_stroke(rect, RADIUS_CARD, palette.card_stroke(), StrokeKind::Inside);
    top_highlight(
        painter,
        rect,
        RADIUS_CARD,
        if palette.dark { 0.10 } else { 0.5 },
    );
}

/// How far a surface floats above the background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Elevation {
    Low,
    Medium,
    High,
}

/// Soft drop shadow under a surface (depth conveys hierarchy).
pub fn elevation(painter: &Painter, rect: Rect, radius: u8, level: Elevation) {
    let (offset, blur, alpha) = match level {
        Elevation::Low => (2, 8, 50),
        Elevation::Medium => (6, 20, 60),
        Elevation::High => (12, 40, 80),
    };
    let shadow = egui::epaint::Shadow {
        offset: [0, offset],
        blur,
        spread: 0,
        color: Color32::from_black_alpha(alpha),
    };
    painter.add(shadow.as_shape(rect, radius));
}

/// A thin bright line along the top edge: light catching the material.
pub fn top_highlight(painter: &Painter, rect: Rect, radius: u8, strength: f32) {
    let band = Rect::from_min_max(
        rect.min,
        pos2(rect.right(), rect.top() + f32::from(radius) + 2.0),
    );
    painter.with_clip_rect(band).rect_stroke(
        rect,
        radius,
        Stroke::new(1.0, Color32::WHITE.gamma_multiply(strength)),
        StrokeKind::Inside,
    );
}

/// A frame that looks like a card, for egui layouts.
pub fn card_frame(palette: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(palette.surface)
        .stroke(palette.card_stroke())
        .corner_radius(CornerRadius::same(RADIUS_CARD))
        .inner_margin(egui::Margin::same(20))
}

/// Gradient progress bar, `value` 0..=1.
pub fn progress_bar(painter: &Painter, rect: Rect, value: f32, palette: &Palette) {
    let track = if palette.dark {
        Color32::from_white_alpha(18)
    } else {
        Color32::from_black_alpha(20)
    };
    painter.rect_filled(rect, theme::RADIUS_PILL, track);
    let w = rect.width() * value.clamp(0.0, 1.0);
    if w > 0.5 {
        let fill = Rect::from_min_size(rect.min, vec2(w, rect.height()));
        gradient_pill(
            painter,
            fill,
            ACCENT_A,
            ACCENT_A.lerp_to_gamma(ACCENT_B, value.clamp(0.0, 1.0)),
        );
    }
}

/// Deterministic pleasant color from a string (used for song backgrounds).
pub fn color_from_hash(text: &str) -> Color32 {
    let mut h: u32 = 2_166_136_261;
    for b in text.bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(16_777_619);
    }
    let hue = (h % 360) as f32 / 360.0;
    let hsva = egui::ecolor::Hsva::new(hue, 0.65, 0.95, 1.0);
    Color32::from(hsva)
}

/// Page indicator: evenly spaced dots, the current one stretched into a pill.
/// The stretch moves from `previous` to `current` as `progress` goes 0 → 1,
/// and the row keeps its spacing, so dots never overlap. `anchor` is the
/// vertical center and the left, center or right end of the row (`align`).
#[allow(clippy::too_many_arguments)] // plain values from two callers; a struct would only rename them
pub fn page_dots(
    painter: &Painter,
    anchor: Pos2,
    align: egui::Align,
    count: usize,
    current: usize,
    previous: Option<usize>,
    progress: f32,
    color: impl Fn(usize) -> Color32,
) {
    const DOT: f32 = 8.0;
    const PILL: f32 = 24.0;
    const GAP: f32 = 8.0;
    let p = progress.clamp(0.0, 1.0);
    let stretch = |i: usize| {
        if i == current {
            p
        } else if Some(i) == previous {
            1.0 - p
        } else {
            0.0
        }
    };
    let widths: Vec<f32> = (0..count)
        .map(|i| DOT + (PILL - DOT) * stretch(i))
        .collect();
    let total = widths.iter().sum::<f32>() + GAP * count.saturating_sub(1) as f32;
    let mut x = match align {
        egui::Align::Min => anchor.x,
        egui::Align::Center => anchor.x - total / 2.0,
        egui::Align::Max => anchor.x - total,
    };
    for (i, w) in widths.iter().enumerate() {
        let r = Rect::from_min_size(pos2(x, anchor.y - DOT / 2.0), vec2(*w, DOT));
        let c = color(i);
        painter.rect_filled(r, DOT / 2.0, c.gamma_multiply(0.6 + 0.4 * stretch(i)));
        x += w + GAP;
    }
}

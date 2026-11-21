//! Text helpers: tracked and tabular text, animated scaling, eliding and truncating.

use super::*;

/// Text with size-aware tracking (letter spacing), Apple style: large text is
/// tightened, small all-caps labels are opened up. Returns the drawn rect.
pub fn text_tracked(
    painter: &Painter,
    pos: Pos2,
    anchor: Align2,
    text: &str,
    font_id: egui::FontId,
    color: Color32,
) -> Rect {
    text_tracked_scaled(painter, pos, anchor, text, font_id, color, 1.0)
}

/// Scales shapes by `scale` around `center`.
///
/// Animated text must be scaled like this instead of changing the font size
/// every frame: each new font size adds glyphs to egui's font atlas, which
/// then fills up and is rebuilt, and that rebuild stalls a frame for
/// 50–300 ms (visible as stutter in gameplay).
pub fn scale_shape(shape: impl Into<egui::Shape>, center: Pos2, scale: f32) -> egui::Shape {
    let mut shape = shape.into();
    if (scale - 1.0).abs() > 1e-4 {
        shape.transform(egui::emath::TSTransform::new(
            center.to_vec2() * (1.0 - scale),
            scale,
        ));
    }
    shape
}

/// [`text_tracked`] drawn at `scale` × its size (for pop animations).
pub fn text_tracked_scaled(
    painter: &Painter,
    pos: Pos2,
    anchor: Align2,
    text: &str,
    font_id: egui::FontId,
    color: Color32,
    scale: f32,
) -> Rect {
    let size = font_id.size;
    let is_caps_label = size <= 13.0 && text.chars().all(|c| !c.is_lowercase());
    let tracking_em = if is_caps_label {
        0.08
    } else if size >= 40.0 {
        -0.025
    } else if size >= 28.0 {
        -0.02
    } else if size >= 20.0 {
        -0.01
    } else {
        0.0
    };
    let mut job = egui::text::LayoutJob::default();
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id,
            color,
            extra_letter_spacing: tracking_em * size,
            ..Default::default()
        },
    );
    let galley = painter.layout_job(job);
    let rect = anchor.anchor_size(pos, galley.size());
    painter.add(scale_shape(
        egui::Shape::galley(rect.min, galley, color),
        pos,
        scale,
    ));
    scale_rect(rect, pos, scale)
}

pub(super) fn scale_rect(rect: Rect, center: Pos2, scale: f32) -> Rect {
    Rect::from_min_max(
        center + (rect.min - center) * scale,
        center + (rect.max - center) * scale,
    )
}

/// Draws a number with fixed-width digits so it does not wobble while counting.
pub fn tabular_text(
    painter: &Painter,
    pos: Pos2,
    anchor: Align2,
    text: &str,
    font_id: egui::FontId,
    color: Color32,
) -> Rect {
    tabular_text_scaled(painter, pos, anchor, text, font_id, color, 1.0)
}

/// [`tabular_text`] drawn at `scale` × its size (for pop animations).
pub fn tabular_text_scaled(
    painter: &Painter,
    pos: Pos2,
    anchor: Align2,
    text: &str,
    font_id: egui::FontId,
    color: Color32,
    scale: f32,
) -> Rect {
    let digit_w = painter
        .layout_no_wrap("0".into(), font_id.clone(), color)
        .size()
        .x;
    let widths: Vec<f32> = text
        .chars()
        .map(|c| {
            if c.is_ascii_digit() {
                digit_w
            } else {
                painter
                    .layout_no_wrap(c.to_string(), font_id.clone(), color)
                    .size()
                    .x
            }
        })
        .collect();
    let total: f32 = widths.iter().sum();
    let height = font_id.size * 1.2;
    let rect = anchor.anchor_size(pos, vec2(total, height));
    let mut x = rect.left();
    for (c, w) in text.chars().zip(widths) {
        let galley = painter.layout_no_wrap(c.to_string(), font_id.clone(), color);
        let at =
            Align2::CENTER_CENTER.anchor_size(pos2(x + w / 2.0, rect.center().y), galley.size());
        painter.add(scale_shape(
            egui::Shape::galley(at.min, galley, color),
            pos,
            scale,
        ));
        x += w;
    }
    scale_rect(rect, pos, scale)
}

/// Shortens `text` with "…" so it fits in `max_width` pixels in `font_id`.
pub fn elide(painter: &Painter, text: &str, font_id: egui::FontId, max_width: f32) -> String {
    let width = |t: &str| {
        painter
            .layout_no_wrap(t.to_string(), font_id.clone(), Color32::WHITE)
            .size()
            .x
    };
    if width(text) <= max_width {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    // Binary search for the longest prefix that fits with the ellipsis.
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let candidate: String = chars[..mid].iter().collect::<String>() + "…";
        if width(&candidate) <= max_width {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    chars[..lo]
        .iter()
        .collect::<String>()
        .trim_end()
        .to_string()
        + "…"
}

/// Shortens text to `max` characters with an ellipsis.
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut s: String = text.chars().take(max.saturating_sub(1)).collect();
        s.push('…');
        s
    }
}

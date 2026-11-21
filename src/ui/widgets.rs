//! Custom widgets: buttons, cards, chips, progress bars, background.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, Mesh, Painter, Pos2, Rect, Response, Sense, Stroke,
    StrokeKind, Ui, pos2, vec2,
};

use super::theme::{
    self, ACCENT_A, ACCENT_B, Palette, RADIUS_BUTTON, RADIUS_CARD, font, font_bold,
};

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

/// Large gradient call-to-action button.
pub fn primary_button(ui: &mut Ui, text: &str, size: egui::Vec2, enabled: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let hover = ui.ctx().animate_bool(
        response.id,
        enabled && (response.hovered() || response.has_focus()),
    );
    let pressed = enabled && response.is_pointer_button_down_on();
    let painter = ui.painter();
    elevation(painter, rect, RADIUS_BUTTON, Elevation::Low);
    // Feedback on press, not on release: shrink 3 % instantly.
    let rect = press_rect(rect.expand(hover * 2.0), pressed);
    let (a, b) = if enabled {
        (ACCENT_A, ACCENT_B)
    } else {
        (Color32::from_gray(70), Color32::from_gray(90))
    };
    let fill = a.lerp_to_gamma(b, 0.25 + hover * 0.5);
    let fill = if pressed {
        fill.lerp_to_gamma(Color32::BLACK, 0.12)
    } else {
        fill
    };
    painter.rect_filled(rect, RADIUS_BUTTON, fill);
    top_highlight(painter, rect, RADIUS_BUTTON, 0.22);
    if hover > 0.0 {
        painter.rect_stroke(
            rect,
            RADIUS_BUTTON,
            Stroke::new(2.0, Color32::WHITE.gamma_multiply(0.4 * hover)),
            StrokeKind::Outside,
        );
    }
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        font_bold(theme::TEXT_M),
        Color32::WHITE,
    );
    response
}

/// Secondary (outlined) button.
pub fn secondary_button(ui: &mut Ui, palette: &Palette, text: &str, size: egui::Vec2) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let hover = ui
        .ctx()
        .animate_bool(response.id, response.hovered() || response.has_focus());
    let pressed = response.is_pointer_button_down_on();
    let painter = ui.painter();
    let rect = press_rect(rect, pressed);
    let fill = palette.surface.lerp_to_gamma(palette.surface_hover, hover);
    painter.rect_filled(
        rect,
        RADIUS_BUTTON,
        if pressed {
            fill.lerp_to_gamma(Color32::BLACK, 0.15)
        } else {
            fill
        },
    );
    top_highlight(painter, rect, RADIUS_BUTTON, 0.08);
    painter.rect_stroke(
        rect,
        RADIUS_BUTTON,
        Stroke::new(1.0, palette.border.lerp_to_gamma(ACCENT_B, hover * 0.7)),
        StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        font(theme::TEXT_S + 1.0),
        palette.text,
    );
    response
}

/// Big menu tile with title + subtitle, used on the title screen.
pub fn menu_tile(
    ui: &mut Ui,
    palette: &Palette,
    title: &str,
    subtitle: &str,
    size: egui::Vec2,
    selected: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let active = response.hovered() || selected;
    let t = ui.ctx().animate_bool(response.id, active);
    let pressed = response.is_pointer_button_down_on();
    let painter = ui.painter();
    let rect = press_rect(rect.translate(vec2(t * 6.0, 0.0)), pressed);
    elevation(painter, rect, RADIUS_CARD, Elevation::Medium);
    painter.rect_filled(
        rect,
        RADIUS_CARD,
        palette.surface.lerp_to_gamma(palette.surface_hover, t),
    );
    top_highlight(painter, rect, RADIUS_CARD, 0.10);
    painter.rect_stroke(rect, RADIUS_CARD, palette.card_stroke(), StrokeKind::Inside);
    if t > 0.0 {
        let bar = Rect::from_min_size(
            rect.left_top() + vec2(0.0, 12.0),
            vec2(4.0, rect.height() - 24.0),
        );
        painter.rect_filled(bar, 2, ACCENT_B.gamma_multiply(t));
    }
    text_tracked(
        painter,
        rect.left_center() + vec2(24.0, -11.0),
        Align2::LEFT_CENTER,
        title,
        font_bold(theme::TEXT_L),
        palette.text,
    );
    painter.text(
        rect.left_center() + vec2(24.0, 16.0),
        Align2::LEFT_CENTER,
        subtitle,
        font(theme::TEXT_S),
        palette.text_dim,
    );
    text_tracked(
        painter,
        rect.right_center() - vec2(24.0 - t * 4.0, 0.0),
        Align2::RIGHT_CENTER,
        "›",
        font_bold(theme::TEXT_XL),
        palette.text_dim.lerp_to_gamma(palette.text, t),
    );
    response
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

/// Pressed controls shrink by 3 % around their center — immediately.
fn press_rect(rect: Rect, pressed: bool) -> Rect {
    if pressed {
        Rect::from_center_size(rect.center(), rect.size() * 0.97)
    } else {
        rect
    }
}

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

fn scale_rect(rect: Rect, center: Pos2, scale: f32) -> Rect {
    Rect::from_min_max(
        center + (rect.min - center) * scale,
        center + (rect.max - center) * scale,
    )
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

/// Small rounded label, e.g. a difficulty chip or "Auto" badge.
pub fn chip(ui: &mut Ui, text: &str, color: Color32, filled: bool) -> Response {
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        font_bold(theme::TEXT_XS + 1.0),
        Color32::WHITE,
    );
    let size = galley.size() + vec2(20.0, 10.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let hover = ui.ctx().animate_bool(response.id, response.hovered());
    let painter = ui.painter();
    if filled {
        painter.rect_filled(rect, theme::RADIUS_PILL, color);
    } else {
        painter.rect_filled(
            rect,
            theme::RADIUS_PILL,
            color.gamma_multiply(0.14 + hover * 0.1),
        );
        painter.rect_stroke(
            rect,
            theme::RADIUS_PILL,
            Stroke::new(1.0, color.gamma_multiply(0.6)),
            StrokeKind::Inside,
        );
    }
    let text_color = if filled {
        Color32::from_rgb(12, 12, 20)
    } else {
        color
    };
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        font_bold(theme::TEXT_XS + 1.0),
        text_color,
    );
    response
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

/// iOS-style on/off switch. Returns true when it was toggled.
pub fn toggle(ui: &mut Ui, on: &mut bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(48.0, 28.0), Sense::click());
    if response.clicked() {
        *on = !*on;
    }
    let t = ui.ctx().animate_bool_with_time(response.id, *on, 0.15);
    let painter = ui.painter();
    let off_color = Color32::from_gray(90);
    painter.rect_filled(
        rect,
        theme::RADIUS_PILL,
        off_color.lerp_to_gamma(ACCENT_A, t),
    );
    let knob = pos2(rect.left() + 14.0 + t * 20.0, rect.center().y);
    painter.circle_filled(
        knob,
        10.0 + if response.hovered() { 1.0 } else { 0.0 },
        Color32::WHITE,
    );
    response.clicked()
}

/// Row of mutually exclusive options. Returns true when the selection changed.
pub fn segmented(ui: &mut Ui, palette: &Palette, options: &[&str], selected: &mut usize) -> bool {
    let mut changed = false;
    let galleys: Vec<_> = options
        .iter()
        .map(|o| {
            ui.painter()
                .layout_no_wrap((*o).to_string(), font(theme::TEXT_S), palette.text)
        })
        .collect();
    let widths: Vec<f32> = galleys.iter().map(|g| g.size().x + 28.0).collect();
    let total: f32 = widths.iter().sum::<f32>() + 8.0;
    let (rect, group) = ui.allocate_exact_size(vec2(total, 36.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, RADIUS_BUTTON, palette.surface_hover);
    let mut x = rect.left() + 4.0;
    for (i, (opt, w)) in options.iter().zip(&widths).enumerate() {
        let r = Rect::from_min_size(pos2(x, rect.top() + 4.0), vec2(*w, 28.0));
        // IDs come from this control's own allocation, so two segmented
        // controls in different rows never clash.
        let resp = ui.interact(r, group.id.with(("segmented", i)), Sense::click());
        if resp.clicked() && *selected != i {
            *selected = i;
            changed = true;
        }
        let active = *selected == i;
        if active {
            gradient_rect(ui.painter(), r, ACCENT_A, ACCENT_B);
        } else if resp.hovered() {
            ui.painter()
                .rect_filled(r, RADIUS_BUTTON - 4, palette.surface);
        }
        ui.painter().text(
            r.center(),
            Align2::CENTER_CENTER,
            *opt,
            if active {
                font_bold(theme::TEXT_S)
            } else {
                font(theme::TEXT_S)
            },
            if active {
                Color32::WHITE
            } else {
                palette.text_dim
            },
        );
        x += w;
    }
    changed
}

/// Control hints at the bottom of a screen: each key drawn as a keycap with a
/// bold label, on a glass strip. `items` are `(key, what it does)`.
pub fn key_hints(
    painter: &Painter,
    anchor: Pos2,
    align: Align2,
    items: &[(&str, &str)],
    palette: &Palette,
) -> Rect {
    const H: f32 = 40.0;
    const PAD: f32 = 14.0;
    const GAP_KEY: f32 = 8.0;
    const GAP_ITEM: f32 = 20.0;
    let key_font = font_bold(theme::TEXT_S);
    let label_font = font_bold(theme::TEXT_S + 1.0);
    let measure = |text: &str, f: &egui::FontId| {
        painter
            .layout_no_wrap(text.to_string(), f.clone(), Color32::WHITE)
            .size()
            .x
    };
    let widths: Vec<(f32, f32)> = items
        .iter()
        .map(|(k, l)| {
            (
                (measure(k, &key_font) + 16.0).max(30.0),
                measure(l, &label_font),
            )
        })
        .collect();
    let total = PAD * 2.0
        + widths.iter().map(|(k, l)| k + GAP_KEY + l).sum::<f32>()
        + GAP_ITEM * (items.len().saturating_sub(1)) as f32;
    let strip = align.anchor_size(anchor, vec2(total, H));

    painter.rect_filled(
        strip,
        theme::RADIUS_PILL,
        palette.surface.gamma_multiply(0.92),
    );
    painter.rect_stroke(
        strip,
        theme::RADIUS_PILL,
        palette.card_stroke(),
        StrokeKind::Inside,
    );
    let mut x = strip.left() + PAD;
    let cy = strip.center().y;
    for ((key, label), (kw, lw)) in items.iter().zip(widths) {
        let cap = Rect::from_min_size(pos2(x, cy - 13.0), vec2(kw, 24.0));
        // Keycap: darker base below gives a pressable, 3D look.
        painter.rect_filled(
            cap.translate(vec2(0.0, 2.0)),
            7,
            Color32::from_black_alpha(110),
        );
        painter.rect_filled(
            cap,
            7,
            palette.surface_hover.lerp_to_gamma(Color32::WHITE, 0.10),
        );
        painter.rect_stroke(
            cap,
            7,
            Stroke::new(1.0, Color32::from_white_alpha(60)),
            StrokeKind::Inside,
        );
        painter.text(
            cap.center(),
            Align2::CENTER_CENTER,
            *key,
            key_font.clone(),
            palette.text,
        );
        painter.text(
            pos2(cap.right() + GAP_KEY, cy),
            Align2::LEFT_CENTER,
            *label,
            label_font.clone(),
            palette.text,
        );
        x += kw + GAP_KEY + lw + GAP_ITEM;
    }
    strip
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

/// Readable slider: a bold value box, a thick track with a gradient fill and a
/// big white knob. Click anywhere on the track or drag. `label` formats the
/// value shown in the box. Returns a response whose `changed()` is set when
/// the value moved (and `drag_stopped()` when a drag ends).
pub fn slider(
    ui: &mut Ui,
    palette: &Palette,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    step: f64,
    label: impl Fn(f64) -> String,
) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(300.0, 36.0), Sense::click_and_drag());
    let (lo, hi) = (*range.start(), *range.end());
    let value_box = Rect::from_min_size(rect.left_top(), vec2(86.0, rect.height()));
    let track = Rect::from_min_max(
        pos2(value_box.right() + 20.0, rect.center().y - 4.0),
        pos2(rect.right() - 12.0, rect.center().y + 4.0),
    );
    if let Some(p) = resp.interact_pointer_pos()
        && (resp.dragged() || resp.clicked() || resp.drag_started())
        && p.x >= value_box.right()
    {
        let t = f64::from(((p.x - track.left()) / track.width()).clamp(0.0, 1.0));
        let mut v = lo + t * (hi - lo);
        if step > 0.0 {
            v = (v / step).round() * step;
        }
        let v = v.clamp(lo, hi);
        if (v - *value).abs() > f64::EPSILON {
            *value = v;
            resp.mark_changed();
        }
    }
    let t = (((*value - lo) / (hi - lo)).clamp(0.0, 1.0)) as f32;
    let hot = ui
        .ctx()
        .animate_bool(resp.id, resp.hovered() || resp.dragged());
    let painter = ui.painter();

    // Value box.
    painter.rect_filled(value_box, RADIUS_BUTTON, palette.surface_hover);
    painter.rect_stroke(
        value_box,
        RADIUS_BUTTON,
        Stroke::new(1.0, palette.border.lerp_to_gamma(ACCENT_B, hot)),
        StrokeKind::Inside,
    );
    painter.text(
        value_box.center(),
        Align2::CENTER_CENTER,
        label(*value),
        font_bold(theme::TEXT_S + 1.0),
        palette.text,
    );
    // Track: clearly visible empty part, gradient filled part.
    let empty = if palette.dark {
        Color32::from_white_alpha(38)
    } else {
        Color32::from_black_alpha(30)
    };
    painter.rect_filled(track, theme::RADIUS_PILL, empty);
    let filled = Rect::from_min_size(
        track.min,
        vec2((track.width() * t).max(track.height()), track.height()),
    );
    gradient_pill(painter, filled, ACCENT_A, ACCENT_B);
    // Knob.
    let knob = pos2(track.left() + track.width() * t, track.center().y);
    let r = 10.0 + 2.0 * hot;
    painter.circle_filled(
        knob + vec2(0.0, 2.0),
        r + 1.0,
        Color32::from_black_alpha(70),
    );
    painter.circle_filled(knob, r, Color32::WHITE);
    painter.circle_stroke(
        knob,
        r,
        Stroke::new(2.0, ACCENT_B.gamma_multiply(0.6 + 0.4 * hot)),
    );
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

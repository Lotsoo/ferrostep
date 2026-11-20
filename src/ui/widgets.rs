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
    let painter = ui.painter();
    let rect = rect.expand(hover * 2.0);
    let (a, b) = if enabled {
        (ACCENT_A, ACCENT_B)
    } else {
        (Color32::from_gray(70), Color32::from_gray(90))
    };
    painter.rect_filled(rect, RADIUS_BUTTON, a.lerp_to_gamma(b, 0.25 + hover * 0.5));
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
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        RADIUS_BUTTON,
        palette.surface.lerp_to_gamma(palette.surface_hover, hover),
    );
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
    let painter = ui.painter();
    let rect = rect.translate(vec2(t * 6.0, 0.0));
    painter.rect_filled(
        rect,
        RADIUS_CARD,
        palette.surface.lerp_to_gamma(palette.surface_hover, t),
    );
    painter.rect_stroke(rect, RADIUS_CARD, palette.card_stroke(), StrokeKind::Inside);
    if t > 0.0 {
        let bar = Rect::from_min_size(
            rect.left_top() + vec2(0.0, 12.0),
            vec2(4.0, rect.height() - 24.0),
        );
        painter.rect_filled(bar, 2, ACCENT_B.gamma_multiply(t));
    }
    painter.text(
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
    painter.text(
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
    painter.rect_filled(rect, RADIUS_CARD, palette.surface);
    painter.rect_stroke(rect, RADIUS_CARD, palette.card_stroke(), StrokeKind::Inside);
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
        painter.text(
            pos2(x + w / 2.0, rect.center().y),
            Align2::CENTER_CENTER,
            c,
            font_id.clone(),
            color,
        );
        x += w;
    }
    rect
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

//! Interactive widgets: buttons, menu tiles, chips, toggle, segmented control, slider and key hints.

use super::*;

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

/// Pressed controls shrink by 3 % around their center — immediately.
pub(super) fn press_rect(rect: Rect, pressed: bool) -> Rect {
    if pressed {
        Rect::from_center_size(rect.center(), rect.size() * 0.97)
    } else {
        rect
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

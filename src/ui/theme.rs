//! Design tokens (CLAUDE.md §6.1): colors, sizes, radii.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke};

use crate::config::ThemeChoice;
use crate::gameplay::judge::Judgment;

/// Spacing unit (everything is a multiple of 8).
pub const SPACE: f32 = 8.0;
pub const RADIUS_CARD: u8 = 16;
pub const RADIUS_BUTTON: u8 = 12;
pub const RADIUS_PILL: u8 = 255;

/// Type scale.
pub const TEXT_XL: f32 = 32.0;
pub const TEXT_L: f32 = 24.0;
pub const TEXT_M: f32 = 18.0;
pub const TEXT_S: f32 = 14.0;
pub const TEXT_XS: f32 = 12.0;

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Column colors: Left, Down, Up, Right.
pub const COLUMN_COLORS: [Color32; 4] =
    [rgb(0xFF5C8A), rgb(0x3EC8FF), rgb(0x5CFFB0), rgb(0xFFC85C)];
pub const ACCENT_A: Color32 = rgb(0x7C5CFF);
pub const ACCENT_B: Color32 = rgb(0x3EC8FF);
pub const DANGER: Color32 = rgb(0xFF5C5C);
pub const SUCCESS: Color32 = rgb(0x5CFFB0);

/// Color of a judgment.
pub fn judgment_color(j: Judgment) -> Color32 {
    match j {
        Judgment::Marvelous => rgb(0xE0F7FF),
        Judgment::Perfect => rgb(0xFFE45C),
        Judgment::Great => rgb(0x5CFFB0),
        Judgment::Good => rgb(0x3EC8FF),
        Judgment::Boo => rgb(0xB07CFF),
        Judgment::Miss => rgb(0xFF5C5C),
    }
}

/// Color for column `c` (wraps for >4 columns).
pub fn column_color(c: u8) -> Color32 {
    COLUMN_COLORS[usize::from(c) % 4]
}

/// Colors that change between dark and light theme.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub bg_top: Color32,
    pub bg_bottom: Color32,
    pub surface: Color32,
    pub surface_hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_dim: Color32,
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            dark: true,
            bg_top: rgb(0x0B0B12),
            bg_bottom: rgb(0x141428),
            surface: Color32::from_rgba_unmultiplied(0x1A, 0x1A, 0x2E, 217),
            surface_hover: Color32::from_rgba_unmultiplied(0x24, 0x24, 0x3E, 230),
            border: Color32::from_rgba_unmultiplied(255, 255, 255, 0x14),
            text: rgb(0xF2F2F7),
            text_dim: rgb(0xA0A0B8),
        }
    }

    pub fn light() -> Self {
        Self {
            dark: false,
            bg_top: rgb(0xF4F4FA),
            bg_bottom: rgb(0xE6E6F4),
            surface: Color32::from_rgba_unmultiplied(255, 255, 255, 230),
            surface_hover: Color32::from_rgba_unmultiplied(246, 244, 255, 240),
            border: Color32::from_rgba_unmultiplied(20, 20, 60, 0x1C),
            text: rgb(0x16162A),
            text_dim: rgb(0x5A5A74),
        }
    }

    pub fn for_choice(choice: ThemeChoice) -> Self {
        match choice {
            ThemeChoice::Dark => Self::dark(),
            ThemeChoice::Light => Self::light(),
        }
    }

    pub fn card_stroke(&self) -> Stroke {
        Stroke::new(1.0, self.border)
    }
}

/// Proportional font of a given size.
pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

/// Bold font (registered in `ui::fonts`).
pub fn font_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(super::fonts::BOLD.into()))
}

/// Applies the palette to egui's built-in widgets.
pub fn apply(ctx: &egui::Context, palette: &Palette, ui_scale: f32) {
    let theme = if palette.dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    ctx.set_theme(theme);
    ctx.set_zoom_factor(ui_scale.clamp(0.9, 1.5));
    let p = *palette;
    ctx.all_styles_mut(move |style| {
        let v = &mut style.visuals;
        v.panel_fill = p.bg_top;
        v.window_fill = p.bg_bottom;
        v.extreme_bg_color = if p.dark { rgb(0x10101C) } else { rgb(0xFFFFFF) };
        v.override_text_color = Some(p.text);
        v.selection.bg_fill = ACCENT_A.gamma_multiply(0.6);
        v.hyperlink_color = ACCENT_B;
        let radius = CornerRadius::same(RADIUS_BUTTON);
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = radius;
        }
        v.widgets.inactive.weak_bg_fill = p.surface;
        v.widgets.inactive.bg_fill = p.surface;
        v.widgets.hovered.weak_bg_fill = p.surface_hover;
        v.widgets.hovered.bg_fill = p.surface_hover;
        v.widgets.inactive.bg_stroke = p.card_stroke();
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_B.gamma_multiply(0.6));
        v.widgets.active.bg_fill = ACCENT_A;
        v.widgets.active.weak_bg_fill = ACCENT_A;
        style.spacing.item_spacing = egui::vec2(SPACE, SPACE);
        style.spacing.button_padding = egui::vec2(2.0 * SPACE, SPACE);
        style.spacing.interact_size.y = 32.0;
        style
            .text_styles
            .insert(egui::TextStyle::Body, font(TEXT_S + 1.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, font(TEXT_S + 1.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, font_bold(TEXT_L));
        style
            .text_styles
            .insert(egui::TextStyle::Small, font(TEXT_XS));
    });
}

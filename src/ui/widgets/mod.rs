//! Custom widgets: buttons, cards, chips, progress bars, background.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, Mesh, Painter, Pos2, Rect, Response, Sense, Stroke,
    StrokeKind, Ui, pos2, vec2,
};

use super::theme::{
    self, ACCENT_A, ACCENT_B, Palette, RADIUS_BUTTON, RADIUS_CARD, font, font_bold,
};

mod controls;
mod paint;
mod text;

pub use controls::*;
pub use paint::*;
pub use text::*;

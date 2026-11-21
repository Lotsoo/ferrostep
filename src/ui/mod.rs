//! Everything visual. Screens live in [`screens`].

pub mod anim;
pub mod beatfx;
pub mod fonts;
pub mod gift;
pub mod hud;
pub mod notefield;
pub mod screens;
pub mod theme;
pub mod widgets;

use std::time::Instant;

use eframe::egui::{self, Color32, Rect, StrokeKind, pos2, vec2};

use theme::{DANGER, Palette, SUCCESS, font};

/// Kind of toast notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
}

struct Toast {
    text: String,
    kind: ToastKind,
    created: Instant,
}

/// Small notifications in the bottom-right corner ("Dance mat terhubung", ...).
#[derive(Default)]
pub struct Toasts {
    items: Vec<Toast>,
}

const TOAST_LIFETIME: f32 = 4.0;

impl Toasts {
    pub fn push(&mut self, kind: ToastKind, text: impl Into<String>) {
        self.items.push(Toast {
            text: text.into(),
            kind,
            created: Instant::now(),
        });
        if self.items.len() > 4 {
            self.items.remove(0);
        }
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.push(ToastKind::Info, text);
    }

    pub fn success(&mut self, text: impl Into<String>) {
        self.push(ToastKind::Success, text);
    }

    pub fn warning(&mut self, text: impl Into<String>) {
        self.push(ToastKind::Warning, text);
    }

    /// Draws toasts on top of everything.
    pub fn show(&mut self, ctx: &egui::Context, palette: &Palette, now: Instant) {
        self.items
            .retain(|t| anim::since(t.created, now) < TOAST_LIFETIME);
        if self.items.is_empty() {
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::unique("toasts"),
        ));
        let screen = ctx.content_rect();
        let mut y = screen.bottom() - 24.0;
        for toast in self.items.iter().rev() {
            let age = anim::since(toast.created, now);
            let appear = anim::ease_out_cubic(age / 0.25);
            let fade = ((TOAST_LIFETIME - age) / 0.4).clamp(0.0, 1.0);
            let alpha = appear.min(fade);
            let galley =
                painter.layout(toast.text.clone(), font(theme::TEXT_S), palette.text, 360.0);
            let size = galley.size() + vec2(48.0, 24.0);
            let x = screen.right() - 24.0 - size.x + (1.0 - appear) * 40.0;
            let rect = Rect::from_min_size(pos2(x, y - size.y), size);
            let accent = match toast.kind {
                ToastKind::Info => theme::ACCENT_B,
                ToastKind::Success => SUCCESS,
                ToastKind::Warning => DANGER,
            };
            let bg = if palette.dark {
                Color32::from_rgb(0x1E, 0x1E, 0x34)
            } else {
                Color32::WHITE
            };
            painter.rect_filled(rect, theme::RADIUS_BUTTON, bg.gamma_multiply(alpha));
            painter.rect_stroke(
                rect,
                theme::RADIUS_BUTTON,
                egui::Stroke::new(1.0, accent.gamma_multiply(0.6 * alpha)),
                StrokeKind::Inside,
            );
            painter.circle_filled(
                pos2(rect.left() + 18.0, rect.center().y),
                5.0,
                accent.gamma_multiply(alpha),
            );
            painter.galley_with_override_text_color(
                pos2(rect.left() + 34.0, rect.top() + 12.0),
                galley,
                palette.text.gamma_multiply(alpha),
            );
            y -= size.y + 8.0;
        }
        ctx.request_repaint();
    }
}

/// Opens a folder in the system file manager (Dolphin, Nautilus, Explorer, ...).
pub fn open_in_file_manager(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(windows)]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let program = "xdg-open";
    std::process::Command::new(program)
        .arg(path)
        .spawn()
        .map(|_| ())
}

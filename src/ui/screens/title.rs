//! Title screen: animated logo and the three main actions.

use std::time::Instant;

use eframe::egui::{self, Align2, Color32, Rect, Stroke, Ui, pos2, vec2};

use super::Nav;
use crate::app::Shared;
use crate::input::{FrameInput, MenuAction};
use crate::ui::theme::{self, ACCENT_A, ACCENT_B, font, font_bold};
use crate::ui::{anim, widgets};

const ITEMS: [(&str, &str); 3] = [
    ("Main", "Pilih lagu dan mulai bermain"),
    ("Impor Lagu", "Ubah MP3/MP4 menjadi chart otomatis"),
    ("Pengaturan", "Kontrol, dance mat, audio, tampilan"),
];

/// Visual tempo of the title animation.
const TITLE_BPM: f32 = 110.0;

pub struct TitleScreen {
    selected: usize,
    opened: Instant,
}

impl Default for TitleScreen {
    fn default() -> Self {
        Self {
            selected: 0,
            opened: Instant::now(),
        }
    }
}

impl TitleScreen {
    fn choose(&self, i: usize) -> Nav {
        match i {
            0 => Nav::SongSelect,
            1 => Nav::Import(Vec::new()),
            _ => Nav::Settings,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, shared: &mut Shared, input: &FrameInput) -> Option<Nav> {
        let reduce = shared.settings.reduce_motion;
        if !reduce {
            ui.ctx().request_repaint();
        }
        for a in &input.menu {
            match a {
                MenuAction::Up => self.selected = (self.selected + ITEMS.len() - 1) % ITEMS.len(),
                MenuAction::Down => self.selected = (self.selected + 1) % ITEMS.len(),
                MenuAction::Confirm | MenuAction::Right => return Some(self.choose(self.selected)),
                MenuAction::Back => return Some(Nav::Quit),
                MenuAction::Left => {}
            }
        }

        let rect = ui.max_rect();
        let painter = ui.painter().clone();
        let palette = shared.palette;
        let t = if reduce {
            0.0
        } else {
            anim::since(shared.start, input.now)
        };

        // Pulsing rings around the logo, on the beat.
        let beat = t * TITLE_BPM / 60.0;
        let logo_center = pos2(rect.left() + rect.width() * 0.32, rect.center().y - 20.0);
        if !reduce {
            for k in 0..3 {
                let phase = (beat + k as f32 / 3.0).fract();
                let r = 80.0 + phase * 260.0;
                let alpha = (1.0 - phase).powi(2) * 0.25;
                painter.circle_stroke(
                    logo_center,
                    r,
                    Stroke::new(
                        2.0,
                        ACCENT_A
                            .lerp_to_gamma(ACCENT_B, phase)
                            .gamma_multiply(alpha),
                    ),
                );
            }
        }
        let pulse = if reduce {
            0.0
        } else {
            (1.0 - beat.fract()).powi(4)
        };
        crate::ui::notefield::draw_glow(
            &painter,
            logo_center,
            160.0 + pulse * 20.0,
            ACCENT_A,
            0.6 + pulse * 0.3,
        );

        draw_gradient_logo(&painter, logo_center, "Ferrostep", 96.0, t);
        painter.text(
            logo_center + vec2(0.0, 72.0),
            Align2::CENTER_CENTER,
            "Injak ketukannya.",
            font(theme::TEXT_M),
            palette.text_dim,
        );

        // Menu tiles on the right.
        let appear = anim::ease_out_cubic(anim::progress(self.opened, input.now, 0.4, reduce));
        let tile = vec2(420.0, 92.0);
        let x = rect.left() + rect.width() * 0.62;
        let mut nav = None;
        for (i, (title, sub)) in ITEMS.iter().enumerate() {
            let y = rect.center().y - 150.0
                + i as f32 * (tile.y + 16.0)
                + (1.0 - appear) * 30.0 * (i as f32 + 1.0);
            let r = Rect::from_min_size(pos2(x, y), tile);
            let resp = ui.put(r, |ui: &mut Ui| {
                widgets::menu_tile(ui, &palette, title, sub, tile, self.selected == i)
            });
            if resp.hovered() {
                self.selected = i;
            }
            if resp.clicked() {
                nav = Some(self.choose(i));
            }
        }

        let hint = if shared.gamepad_name.is_some() {
            "Injak ↑ ↓ untuk memilih · → untuk masuk"
        } else {
            "↑ ↓ untuk memilih · Enter untuk masuk · Esc untuk keluar"
        };
        painter.text(
            rect.center_bottom() - vec2(0.0, 28.0),
            Align2::CENTER_CENTER,
            hint,
            font(theme::TEXT_XS),
            palette.text_dim,
        );
        if let Some(name) = &shared.gamepad_name {
            painter.text(
                rect.right_top() + vec2(-24.0, 24.0),
                Align2::RIGHT_TOP,
                format!("🎮 {name}"),
                font(theme::TEXT_XS),
                theme::SUCCESS,
            );
        }
        nav
    }
}

/// Draws text where each letter gets a color from a slowly moving gradient.
fn draw_gradient_logo(painter: &egui::Painter, center: egui::Pos2, text: &str, size: f32, t: f32) {
    let font_id = font_bold(size);
    let widths: Vec<f32> = text
        .chars()
        .map(|c| {
            painter
                .layout_no_wrap(c.to_string(), font_id.clone(), Color32::WHITE)
                .size()
                .x
        })
        .collect();
    let total: f32 = widths.iter().sum();
    let mut x = center.x - total / 2.0;
    let n = text.chars().count().max(1) as f32;
    for (i, (c, w)) in text.chars().zip(widths).enumerate() {
        let phase = ((i as f32 / n) + t * 0.15).sin() * 0.5 + 0.5;
        let color = ACCENT_A.lerp_to_gamma(ACCENT_B, phase);
        painter.text(
            pos2(x + w / 2.0, center.y),
            Align2::CENTER_CENTER,
            c,
            font_id.clone(),
            color,
        );
        x += w;
    }
}

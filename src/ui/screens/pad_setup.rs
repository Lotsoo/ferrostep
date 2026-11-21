//! Dance mat mapping wizard: "step on LEFT… DOWN… UP… RIGHT", then a live test.

use std::time::Instant;

use eframe::egui::{Align2, Color32, Rect, Stroke, Ui, pos2, vec2};

use super::Nav;
use crate::app::Shared;
use crate::audio::Sfx;
use crate::input::gamepad::PadEvent;
use crate::input::profiles::{DeviceProfile, RawInput};
use crate::input::{Device, FrameInput, MenuAction};
use crate::ui::notefield::{draw_arrow, draw_glow};
use crate::ui::theme::{self, column_color, font, font_bold};
use crate::ui::{anim, beatfx, widgets};

const STEPS: [&str; 6] = ["KIRI", "BAWAH", "ATAS", "KANAN", "START", "SELECT / BACK"];
/// Ignore presses this soon after a capture (mats can chatter).
const CAPTURE_GAP: f32 = 0.3;

pub struct PadSetupScreen {
    key: String,
    name: String,
    step: usize,
    profile: DeviceProfile,
    last_capture: Instant,
    opened: Instant,
    pressed: [bool; 4],
    /// Last time each panel was stepped on / captured (for the hit burst).
    hit_at: [Option<Instant>; 4],
    back_to_settings: bool,
}

impl PadSetupScreen {
    pub fn new(key: String, name: String, back_to_settings: bool) -> Self {
        Self {
            profile: DeviceProfile {
                name: name.clone(),
                ..Default::default()
            },
            key,
            name,
            step: 0,
            last_capture: Instant::now(),
            opened: Instant::now(),
            pressed: [false; 4],
            hit_at: [None; 4],
            back_to_settings,
        }
    }

    fn done(&self) -> bool {
        self.step >= STEPS.len()
    }

    fn exit(&self) -> Nav {
        if self.back_to_settings {
            Nav::Settings
        } else {
            Nav::Title
        }
    }

    fn already_used(&self, raw: RawInput) -> bool {
        self.profile.columns.iter().any(|c| c.contains(&raw))
            || self.profile.start.contains(&raw)
            || self.profile.back.contains(&raw)
    }

    fn capture(&mut self, raw: RawInput, shared: &mut Shared, now: Instant) {
        if anim::since(self.last_capture, now) < CAPTURE_GAP || self.already_used(raw) {
            return;
        }
        match self.step {
            0..=3 => {
                self.profile.columns[self.step].push(raw);
                self.hit_at[self.step] = Some(now);
            }
            4 => self.profile.start.push(raw),
            5 => self.profile.back.push(raw),
            _ => return,
        }
        self.last_capture = now;
        shared.sfx(Sfx::Confirm);
        self.advance(shared);
    }

    fn advance(&mut self, shared: &mut Shared) {
        self.step += 1;
        if self.done() {
            shared.sfx(Sfx::Success);
            shared.save_pad_profile = Some((self.key.clone(), self.profile.clone()));
            shared
                .toasts
                .success(format!("Pengaturan {} disimpan", self.name));
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, shared: &mut Shared, input: &FrameInput) -> Option<Nav> {
        let now = input.now;
        ui.ctx().request_repaint();
        for ev in &input.pad_events {
            if let PadEvent::Raw { key, raw, .. } = ev
                && *key == self.key
                && !self.done()
            {
                self.capture(*raw, shared, now);
            }
        }
        for ev in &input.columns {
            if ev.device == Device::Gamepad && usize::from(ev.column) < 4 {
                let c = usize::from(ev.column);
                self.pressed[c] = ev.pressed;
                if ev.pressed && self.done() {
                    self.hit_at[c] = Some(ev.at);
                }
            }
        }
        // Only the keyboard Esc leaves the wizard (mat buttons are being mapped).
        if input.last_key == Some(eframe::egui::Key::Escape) && input.has(MenuAction::Back) {
            return Some(self.exit());
        }

        let rect = ui.max_rect();
        let palette = shared.palette;
        let painter = ui.painter().clone();
        let reduce = shared.settings.reduce_motion;
        let t = anim::since(self.opened, now);
        if palette.dark {
            // Floor glows in the color of the arrow being asked for.
            let tint = column_color(self.step.min(3) as u8);
            let beat = beatfx::Beat::idle(t);
            beatfx::draw_highway(
                &painter,
                rect,
                tint,
                &if reduce { beat.calm() } else { beat },
            );
        }
        let card = Rect::from_center_size(rect.center(), vec2(720.0, 520.0));
        widgets::card(&painter, card, &palette);
        widgets::text_tracked(
            &painter,
            card.center_top() + vec2(0.0, 44.0),
            Align2::CENTER_CENTER,
            "Atur Dance Mat",
            font_bold(theme::TEXT_L),
            palette.text,
        );
        painter.text(
            card.center_top() + vec2(0.0, 76.0),
            Align2::CENTER_CENTER,
            &self.name,
            font(theme::TEXT_S),
            palette.text_dim,
        );

        // Four arrows; the current one pulses.
        let size = 96.0;
        let row_y = card.top() + 200.0;
        let pulse = 0.5 + 0.5 * (anim::since(self.opened, now) * 5.0).sin();
        for c in 0..4u8 {
            let x = card.center().x + (f32::from(c) - 1.5) * (size + 28.0);
            let center = pos2(x, row_y);
            let color = column_color(c);
            let ci = usize::from(c);
            let mapped = !self.profile.columns[ci].is_empty();
            let active = self.step == ci;
            let lit = self.done() && self.pressed[ci];
            if active || lit {
                draw_glow(
                    &painter,
                    center,
                    size * 0.6,
                    color,
                    if lit { 1.6 } else { 0.6 + pulse },
                );
            }
            let fill = if mapped || lit {
                color.gamma_multiply(if lit { 1.0 } else { 0.55 })
            } else {
                Color32::TRANSPARENT
            };
            let stroke_color = if active {
                color
            } else if mapped {
                color.gamma_multiply(0.8)
            } else {
                palette.text_dim.gamma_multiply(0.5)
            };
            draw_arrow(
                &painter,
                center,
                size,
                c,
                fill,
                eframe::egui::Stroke::new(3.0, stroke_color),
            );
            // A note falling onto the arrow being asked for: "step here".
            if active && !reduce {
                let p = (t * 0.9).rem_euclid(1.0);
                let e = anim::ease_out_cubic(p);
                let y = center.y - 64.0 * (1.0 - e);
                draw_arrow(
                    &painter,
                    pos2(center.x, y),
                    size * 0.8,
                    c,
                    color.gamma_multiply(0.6 * (1.0 - p * 0.5)),
                    Stroke::new(1.5, Color32::WHITE.gamma_multiply(0.5 * (1.0 - p))),
                );
            }
            // Hit burst when captured or stepped on.
            if let Some(at) = self.hit_at[ci] {
                let age = anim::since(at, now);
                if age < 0.5 && !reduce {
                    let e = anim::ease_out_cubic(age / 0.5);
                    painter.circle_stroke(
                        center,
                        size * (0.5 + 0.6 * e),
                        Stroke::new(5.0 * (1.0 - e), color.gamma_multiply(1.0 - e)),
                    );
                    crate::ui::notefield::radial_light(
                        &painter,
                        center,
                        size,
                        color,
                        0.8 * (1.0 - e),
                    );
                }
            }
        }

        let (title, hint) = if self.done() {
            (
                "Selesai! Coba injak panelnya.".to_string(),
                "Panah menyala saat diinjak. Kalau ada yang salah, pilih Ulangi.".to_string(),
            )
        } else if self.step < 4 {
            (
                format!("Injak panah {}", STEPS[self.step]),
                "Injak sekali, lalu lepaskan.".to_string(),
            )
        } else {
            (
                format!("Tekan tombol {}", STEPS[self.step]),
                "Tombol ini untuk menu. Lewati kalau mat-mu tidak punya.".to_string(),
            )
        };
        widgets::text_tracked(
            &painter,
            card.center_top() + vec2(0.0, 300.0),
            Align2::CENTER_CENTER,
            &title,
            font_bold(theme::TEXT_L),
            palette.text,
        );
        painter.text(
            card.center_top() + vec2(0.0, 336.0),
            Align2::CENTER_CENTER,
            hint,
            font(theme::TEXT_S),
            palette.text_dim,
        );

        // progress dots
        for i in 0..STEPS.len() {
            let x = card.center().x + (i as f32 - 2.5) * 22.0;
            let color = if i < self.step {
                theme::SUCCESS
            } else if i == self.step {
                theme::ACCENT_B
            } else {
                palette.border
            };
            painter.circle_filled(pos2(x, card.top() + 372.0), 5.0, color);
        }

        let mut nav = None;
        let b = vec2(200.0, 48.0);
        let left = Rect::from_min_size(pos2(card.center().x - 310.0, card.bottom() - 80.0), b);
        let mid = Rect::from_min_size(pos2(card.center().x - 100.0, card.bottom() - 80.0), b);
        let right = Rect::from_min_size(pos2(card.center().x + 110.0, card.bottom() - 80.0), b);
        if ui
            .put(left, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, "Ulangi", b)
            })
            .clicked()
        {
            *self = Self::new(self.key.clone(), self.name.clone(), self.back_to_settings);
        }
        if self.done() {
            if ui
                .put(right, |ui: &mut Ui| {
                    widgets::primary_button(ui, "Selesai", b, true)
                })
                .clicked()
            {
                nav = Some(self.exit());
            }
        } else {
            if self.step >= 4
                && ui
                    .put(mid, |ui: &mut Ui| {
                        widgets::secondary_button(ui, &palette, "Lewati langkah", b)
                    })
                    .clicked()
            {
                self.advance(shared);
            }
            if ui
                .put(right, |ui: &mut Ui| {
                    widgets::secondary_button(ui, &palette, "Nanti saja", b)
                })
                .clicked()
            {
                shared.toasts.info(
                    "Memakai pengaturan standar (D-pad). Bisa diatur di Pengaturan → Kontrol.",
                );
                nav = Some(self.exit());
            }
        }
        painter.text(
            card.center_bottom() + vec2(0.0, 28.0),
            Align2::CENTER_CENTER,
            "Esc di keyboard untuk keluar",
            font(theme::TEXT_XS),
            palette.text_dim,
        );
        nav
    }
}

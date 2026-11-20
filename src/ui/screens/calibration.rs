//! Offset calibration: the player taps along with metronome clicks; the average
//! lateness becomes the global offset. Used by Settings and onboarding.

use std::time::Instant;

use eframe::egui::{Align2, Rect, Stroke, Ui, pos2, vec2};

use crate::app::Shared;
use crate::input::FrameInput;
use crate::ui::theme::{self, ACCENT_A, ACCENT_B, SUCCESS, font, font_bold};
use crate::ui::{anim, widgets};

const BPM: f64 = 100.0;
/// Clicks before taps start counting.
const COUNT_IN: usize = 4;
/// Taps needed for a result.
pub const TAPS_NEEDED: usize = 16;
/// Taps further than this from a click are ignored (seconds).
const MAX_ERROR: f64 = 0.25;

/// Pure math, kept separate so it can be tested without audio.
pub mod math {
    /// Signed distance (seconds) from `tap` to the nearest click in `clicks`.
    pub fn nearest_offset(clicks: &[f64], tap: f64) -> Option<f64> {
        clicks
            .iter()
            .map(|c| tap - c)
            .min_by(|a, b| a.abs().total_cmp(&b.abs()))
    }

    /// Mean and standard deviation, after dropping the worst 10 % as outliers.
    pub fn robust_stats(values: &[f64]) -> Option<(f64, f64)> {
        if values.is_empty() {
            return None;
        }
        let mean0 = values.iter().sum::<f64>() / values.len() as f64;
        let mut sorted: Vec<f64> = values.to_vec();
        sorted.sort_by(|a, b| (a - mean0).abs().total_cmp(&(b - mean0).abs()));
        let keep = ((values.len() as f64) * 0.9).ceil() as usize;
        let kept = &sorted[..keep.max(1)];
        let mean = kept.iter().sum::<f64>() / kept.len() as f64;
        let var = kept.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / kept.len() as f64;
        Some((mean, var.sqrt()))
    }
}

/// What the calibration widget reports.
pub enum CalibrationEvent {
    /// The player accepted this offset (ms).
    Accepted(f64),
    Cancelled,
}

pub struct Calibration {
    started: Instant,
    /// Monotonic times (s since `started`) when clicks were triggered.
    clicks: Vec<f64>,
    taps: Vec<f64>,
    result: Option<(f64, f64)>,
    last_flash: Option<Instant>,
}

impl Default for Calibration {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            clicks: Vec::new(),
            taps: Vec::new(),
            result: None,
            last_flash: None,
        }
    }
}

impl Calibration {
    fn t(&self, at: Instant) -> f64 {
        at.saturating_duration_since(self.started).as_secs_f64()
    }

    fn restart(&mut self) {
        *self = Self::default();
    }

    /// Draws the widget in `rect` and handles input.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        shared: &mut Shared,
        input: &FrameInput,
    ) -> Option<CalibrationEvent> {
        let now = input.now;
        let palette = shared.palette;
        let painter = ui.painter().clone();
        let period = 60.0 / BPM;

        if self.result.is_none() {
            ui.ctx().request_repaint();
            // Trigger clicks on schedule (start after a short pause).
            let next = 0.8 + self.clicks.len() as f64 * period;
            if self.t(now) >= next {
                let accent = self.clicks.len().is_multiple_of(4);
                if let Some(audio) = &mut shared.audio {
                    audio.click(accent);
                }
                self.clicks.push(self.t(now));
            }
            // Taps: Space, any column key, or a dance mat panel.
            let mut taps: Vec<Instant> = input
                .columns
                .iter()
                .filter(|c| c.pressed)
                .map(|c| c.at)
                .collect();
            if ui.input(|i| i.key_pressed(eframe::egui::Key::Space)) {
                taps.push(now);
            }
            for at in taps {
                self.last_flash = Some(at);
                if self.clicks.len() <= COUNT_IN {
                    continue;
                }
                if let Some(d) = math::nearest_offset(&self.clicks, self.t(at))
                    && d.abs() <= MAX_ERROR
                {
                    self.taps.push(d);
                }
            }
            if self.taps.len() >= TAPS_NEEDED {
                self.result = math::robust_stats(&self.taps);
            }
        }

        // Pulsing circle on the beat.
        let center = pos2(rect.center().x, rect.top() + rect.height() * 0.36);
        let phase =
            self.clicks
                .last()
                .map_or(1.0, |c| ((self.t(now) - c) / period).clamp(0.0, 1.0)) as f32;
        let pulse = if shared.settings.reduce_motion {
            0.0
        } else {
            (1.0 - phase).powi(3)
        };
        painter.circle_filled(
            center,
            54.0 + 14.0 * pulse,
            ACCENT_A.gamma_multiply(0.15 + 0.35 * pulse),
        );
        painter.circle_stroke(center, 54.0, Stroke::new(3.0, ACCENT_B));
        let flash = self
            .last_flash
            .map_or(0.0, |t| 1.0 - anim::progress(t, now, 0.2, false));
        if flash > 0.0 {
            painter.circle_filled(center, 30.0, SUCCESS.gamma_multiply(flash));
        }

        if let Some((mean, sd)) = self.result {
            let ms = mean * 1000.0;
            widgets::text_tracked(
                &painter,
                center,
                Align2::CENTER_CENTER,
                "✓",
                font_bold(theme::TEXT_XL),
                SUCCESS,
            );
            let direction = if ms > 0.5 {
                format!("Kamu menekan rata-rata {ms:.0} ms setelah bunyi klik.")
            } else if ms < -0.5 {
                format!("Kamu menekan rata-rata {:.0} ms sebelum bunyi klik.", -ms)
            } else {
                "Ketukanmu sudah pas dengan bunyi klik.".to_string()
            };
            painter.text(
                center + vec2(0.0, 90.0),
                Align2::CENTER_CENTER,
                direction,
                font_bold(theme::TEXT_M),
                palette.text,
            );
            let steadiness = if sd * 1000.0 < 25.0 {
                "stabil"
            } else if sd * 1000.0 < 45.0 {
                "cukup stabil"
            } else {
                "kurang stabil — coba ulangi"
            };
            painter.text(
                center + vec2(0.0, 118.0),
                Align2::CENTER_CENTER,
                format!(
                    "Usulan global offset: {ms:+.0} ms  •  sebaran ±{:.0} ms ({steadiness})",
                    sd * 1000.0
                ),
                font(theme::TEXT_S),
                palette.text_dim,
            );
            let y = rect.bottom() - 56.0;
            let accept = Rect::from_min_size(pos2(rect.center().x - 250.0, y), vec2(220.0, 48.0));
            if ui
                .put(accept, |ui: &mut Ui| {
                    widgets::primary_button(ui, "Pakai offset ini", accept.size(), true)
                })
                .clicked()
            {
                return Some(CalibrationEvent::Accepted(ms.round()));
            }
            let again =
                Rect::from_min_size(pos2(accept.right() + 12.0, y + 2.0), vec2(130.0, 44.0));
            if ui
                .put(again, |ui: &mut Ui| {
                    widgets::secondary_button(ui, &palette, "Ulangi", again.size())
                })
                .clicked()
            {
                self.restart();
            }
            let cancel =
                Rect::from_min_size(pos2(again.right() + 12.0, y + 2.0), vec2(110.0, 44.0));
            if ui
                .put(cancel, |ui: &mut Ui| {
                    widgets::secondary_button(ui, &palette, "Batal", cancel.size())
                })
                .clicked()
            {
                return Some(CalibrationEvent::Cancelled);
            }
            return None;
        }

        let counting_in = self.clicks.len() <= COUNT_IN;
        let label = if counting_in {
            format!(
                "Dengarkan… {}",
                (COUNT_IN + 1).saturating_sub(self.clicks.len()).max(1)
            )
        } else {
            format!("{} / {TAPS_NEEDED}", self.taps.len())
        };
        painter.text(
            center,
            Align2::CENTER_CENTER,
            label,
            font_bold(if counting_in {
                theme::TEXT_M
            } else {
                theme::TEXT_L
            }),
            palette.text,
        );
        painter.text(
            center + vec2(0.0, 90.0),
            Align2::CENTER_CENTER,
            "Tekan Spasi (atau injak panel mana saja) tepat saat mendengar klik",
            font_bold(theme::TEXT_M),
            palette.text,
        );
        painter.text(
            center + vec2(0.0, 118.0),
            Align2::CENTER_CENTER,
            "Dengarkan bunyinya, jangan ikuti lingkaran. Headset Bluetooth biasanya butuh offset besar.",
            font(theme::TEXT_S),
            palette.text_dim,
        );
        if shared.audio.is_none() {
            painter.text(
                center + vec2(0.0, 146.0),
                Align2::CENTER_CENTER,
                "Perangkat audio tidak ditemukan, kalibrasi butuh suara.",
                font(theme::TEXT_S),
                theme::DANGER,
            );
        }
        let bar = Rect::from_min_size(
            pos2(rect.center().x - 160.0, rect.bottom() - 100.0),
            vec2(320.0, 8.0),
        );
        widgets::progress_bar(
            &painter,
            bar,
            self.taps.len() as f32 / TAPS_NEEDED as f32,
            &palette,
        );
        let cancel = Rect::from_center_size(
            pos2(rect.center().x, rect.bottom() - 40.0),
            vec2(130.0, 44.0),
        );
        if ui
            .put(cancel, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, "Batal", cancel.size())
            })
            .clicked()
        {
            return Some(CalibrationEvent::Cancelled);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::math::*;

    #[test]
    fn nearest_click() {
        let clicks = [0.0, 0.6, 1.2];
        assert!((nearest_offset(&clicks, 0.65).unwrap() - 0.05).abs() < 1e-9);
        assert!((nearest_offset(&clicks, 1.15).unwrap() + 0.05).abs() < 1e-9);
    }

    #[test]
    fn stats_drop_outliers() {
        let mut v = vec![0.020; 15];
        v.push(0.200);
        let (mean, sd) = robust_stats(&v).unwrap();
        assert!((mean - 0.020).abs() < 1e-9, "{mean}");
        assert!(sd < 1e-9);
    }
}

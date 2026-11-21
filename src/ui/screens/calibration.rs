//! Offset calibration: the player taps along with a metronome; the average
//! lateness becomes the global offset. Used by Settings and onboarding.
//!
//! The metronome is one audio track and its playback position is the clock
//! (the same smoothed audio clock gameplay uses), so the measured lateness is
//! exactly what gameplay needs to correct. Flow: short explanation → 4
//! count-in clicks with big numbers → 16 taps on the sound (no visual beat,
//! only feedback after each tap) → result with "Simpan".

use std::time::Instant;

use eframe::egui::{Align2, Color32, Painter, Pos2, Rect, Stroke, Ui, pos2, vec2};

use crate::app::Shared;
use crate::audio::{AudioClock, Music};
use crate::input::{FrameInput, MenuAction};
use crate::ui::theme::{self, ACCENT_A, ACCENT_B, SUCCESS, column_color, font, font_bold};
use crate::ui::{anim, widgets};

const BPM: f64 = 100.0;
/// Silence before the first click (seconds).
const LEAD: f64 = 1.0;
/// Clicks before taps start counting.
const COUNT_IN: usize = 4;
/// Taps needed for a result.
pub const TAPS_NEEDED: usize = 16;
/// Clicks in the track: count-in, the taps, and room for some misses.
const TOTAL_BEATS: usize = COUNT_IN + TAPS_NEEDED + 10;
/// Taps further than this from a click are ignored (seconds).
const MAX_ERROR: f64 = 0.25;
/// Fewer valid taps than this when the track ends: ask to try again.
const MIN_TAPS: usize = 8;

fn period() -> f64 {
    60.0 / BPM
}

/// Track time of click `k`.
fn beat_time(k: usize) -> f64 {
    LEAD + k as f64 * period()
}

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

enum Phase {
    /// What to do, and a "Mulai" button.
    Intro,
    /// Metronome playing; taps are collected.
    Running,
    /// Result: Some((mean, sd)) in seconds, None when there were too few
    /// taps; and when it was shown.
    Done(Option<(f64, f64)>, Instant),
}

pub struct Calibration {
    phase: Phase,
    /// When the metronome run started, its sound and its clock.
    started: Instant,
    music: Option<Music>,
    clock: Option<AudioClock>,
    /// Signed tap errors (seconds, + = late).
    taps: Vec<f64>,
    last_flash: Option<Instant>,
}

impl Default for Calibration {
    fn default() -> Self {
        Self {
            phase: Phase::Intro,
            started: Instant::now(),
            music: None,
            clock: None,
            taps: Vec::new(),
            last_flash: None,
        }
    }
}

impl Calibration {
    /// Starts right away (the caller already explained what to do).
    pub fn without_intro() -> Self {
        Self {
            phase: Phase::Running,
            ..Self::default()
        }
    }

    fn start(&mut self, now: Instant) {
        *self = Self {
            phase: Phase::Running,
            started: now,
            ..Self::default()
        };
    }

    /// Draws the widget in `rect` and handles input.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        shared: &mut Shared,
        input: &FrameInput,
    ) -> Option<CalibrationEvent> {
        ui.ctx().request_repaint();
        match self.phase {
            Phase::Intro => self.intro_ui(ui, rect, shared, input),
            Phase::Running => {
                self.running_ui(ui, rect, shared, input);
                None
            }
            Phase::Done(result, at) => {
                // A late tap (a mat panel is also "OK") must not press Simpan.
                let settled = anim::since(at, input.now) > 0.8;
                self.done_ui(ui, rect, shared, input, result, settled)
            }
        }
    }

    fn intro_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        shared: &mut Shared,
        input: &FrameInput,
    ) -> Option<CalibrationEvent> {
        let palette = shared.palette;
        let painter = ui.painter().clone();
        let top = rect.top() + 8.0;
        painter.text(
            pos2(rect.center().x, top),
            Align2::CENTER_TOP,
            "Setiap speaker/headset punya jeda. Kalibrasi membuat panah pas dengan lagu.",
            font(theme::TEXT_M),
            palette.text_dim,
        );
        let steps = [
            (
                "1",
                "Dengarkan",
                "Kamu akan mendengar bunyi \"tuk… tuk… tuk…\" yang teratur.",
            ),
            (
                "2",
                "Ikuti iramanya",
                "Hitungan 4-3-2-1 dulu, lalu tekan Spasi (atau injak panah) bareng bunyinya.",
            ),
            (
                "3",
                "Selesai",
                "Setelah 16 ketukan, offset dihitung otomatis. Tinggal tekan Simpan.",
            ),
        ];
        let card_w = (rect.width() - 48.0).min(640.0);
        for (i, (num, title, text)) in steps.iter().enumerate() {
            let r = Rect::from_min_size(
                pos2(rect.center().x - card_w / 2.0, top + 48.0 + i as f32 * 84.0),
                vec2(card_w, 72.0),
            );
            widgets::card(&painter, r, &palette);
            let color = column_color(i as u8 + 1);
            let badge = pos2(r.left() + 40.0, r.center().y);
            painter.circle_filled(badge, 20.0, color.gamma_multiply(0.25));
            painter.circle_stroke(badge, 20.0, Stroke::new(2.0, color));
            painter.text(
                badge,
                Align2::CENTER_CENTER,
                num,
                font_bold(theme::TEXT_L),
                color,
            );
            painter.text(
                pos2(r.left() + 76.0, r.top() + 14.0),
                Align2::LEFT_TOP,
                title,
                font_bold(theme::TEXT_M),
                palette.text,
            );
            painter.text(
                pos2(r.left() + 76.0, r.top() + 40.0),
                Align2::LEFT_TOP,
                widgets::elide(&painter, text, font(theme::TEXT_S), r.width() - 92.0),
                font(theme::TEXT_S),
                palette.text_dim,
            );
        }
        let hint_y = top + 48.0 + 3.0 * 84.0 + 8.0;
        painter.text(
            pos2(rect.center().x, hint_y),
            Align2::CENTER_TOP,
            "Tips: pakai headset kabel. Bluetooth punya jeda besar, jadi wajib dikalibrasi.",
            font(theme::TEXT_S),
            palette.text_dim,
        );
        if shared.audio.is_none() {
            painter.text(
                pos2(rect.center().x, hint_y + 24.0),
                Align2::CENTER_TOP,
                "Perangkat audio tidak ditemukan, kalibrasi butuh suara.",
                font(theme::TEXT_S),
                theme::DANGER,
            );
        }
        let y = (hint_y + 64.0).min(rect.bottom() - 52.0);
        let start = Rect::from_min_size(pos2(rect.center().x - 230.0, y), vec2(220.0, 52.0));
        let space = ui.input(|i| i.key_pressed(eframe::egui::Key::Space));
        if ui
            .put(start, |ui: &mut Ui| {
                widgets::primary_button(ui, "Mulai  (Enter)", start.size(), shared.audio.is_some())
            })
            .clicked()
            || (shared.audio.is_some() && (input.has(MenuAction::Confirm) || space))
        {
            self.start(input.now);
            return None;
        }
        let cancel = Rect::from_min_size(pos2(start.right() + 20.0, y + 4.0), vec2(200.0, 44.0));
        if ui
            .put(cancel, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, "Batal  (Esc)", cancel.size())
            })
            .clicked()
        {
            return Some(CalibrationEvent::Cancelled);
        }
        None
    }

    fn running_ui(&mut self, ui: &mut Ui, rect: Rect, shared: &mut Shared, input: &FrameInput) {
        let now = input.now;
        let palette = shared.palette;
        let painter = ui.painter().clone();
        let (started, music, clock) = (self.started, &mut self.music, &mut self.clock);
        // Start the metronome on the first frame.
        if music.is_none()
            && clock.is_none()
            && let Some(audio) = &mut shared.audio
        {
            match audio.play_metronome(BPM, LEAD, TOTAL_BEATS) {
                Ok(m) => {
                    *clock = Some(AudioClock::new(m.position(), now));
                    *music = Some(m);
                }
                Err(e) => log::warn!("calibration metronome: {e:#}"),
            }
        }
        // Track time now (falls back to the system timer without audio).
        let t = match (music.as_ref(), clock.as_mut()) {
            (Some(m), Some(c)) => c.update(m.position(), now),
            _ => anim::since(started, now) as f64,
        };
        let stopped = music.as_ref().is_some_and(Music::is_stopped);

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
        let first_counted = beat_time(COUNT_IN) - period() / 2.0;
        for at in taps {
            let tap = t - now.saturating_duration_since(at).as_secs_f64();
            if tap < first_counted {
                continue;
            }
            self.last_flash = Some(at);
            let clicks: Vec<f64> = (COUNT_IN..TOTAL_BEATS).map(beat_time).collect();
            if let Some(d) = math::nearest_offset(&clicks, tap)
                && d.abs() <= MAX_ERROR
            {
                self.taps.push(d);
            }
        }

        let center = pos2(rect.center().x, rect.top() + rect.height() * 0.34);
        let beats_done = ((t - LEAD) / period()).floor();
        if beats_done < COUNT_IN as f64 {
            // Count-in: big numbers on the clicks (they set the rhythm).
            let (text, pop) = if beats_done < 0.0 {
                ("Siap…".to_string(), 0.0)
            } else {
                let k = beats_done as usize;
                let age = (t - beat_time(k)) as f32;
                (format!("{}", COUNT_IN - k), (1.0 - age / 0.25).max(0.0))
            };
            painter.circle_filled(center, 64.0, ACCENT_A.gamma_multiply(0.12 + 0.3 * pop));
            painter.circle_stroke(center, 64.0, Stroke::new(3.0, ACCENT_B));
            widgets::text_tracked_scaled(
                &painter,
                center,
                Align2::CENTER_CENTER,
                &text,
                font_bold(48.0),
                palette.text,
                1.0 + 0.2 * pop,
            );
            painter.text(
                center + vec2(0.0, 110.0),
                Align2::CENTER_CENTER,
                "Dengarkan iramanya… mulai tekan setelah hitungan 1",
                font_bold(theme::TEXT_M),
                palette.text,
            );
        } else {
            // Tapping: no beat on screen (it would bias the result), only
            // a flash when the player taps and the tap count.
            let flash = self
                .last_flash
                .map_or(0.0, |f| 1.0 - anim::progress(f, now, 0.2, false));
            painter.circle_filled(center, 64.0, SUCCESS.gamma_multiply(0.1 + 0.4 * flash));
            painter.circle_stroke(center, 64.0, Stroke::new(3.0, SUCCESS));
            widgets::text_tracked_scaled(
                &painter,
                center,
                Align2::CENTER_CENTER,
                &format!("{}", self.taps.len()),
                font_bold(48.0),
                palette.text,
                1.0 + 0.15 * flash,
            );
            painter.text(
                center + vec2(0.0, 40.0),
                Align2::CENTER_TOP,
                format!("dari {TAPS_NEEDED}"),
                font(theme::TEXT_S),
                palette.text_dim,
            );
            painter.text(
                center + vec2(0.0, 110.0),
                Align2::CENTER_CENTER,
                "Tekan Spasi (atau injak panah) bareng bunyi \"tuk\"",
                font_bold(theme::TEXT_M),
                palette.text,
            );
            painter.text(
                center + vec2(0.0, 138.0),
                Align2::CENTER_CENTER,
                "Pejamkan mata kalau perlu: dengarkan saja suaranya.",
                font(theme::TEXT_S),
                palette.text_dim,
            );
        }
        // 16 slots filling up.
        let slot_w = 22.0;
        let row_left = rect.center().x - slot_w * TAPS_NEEDED as f32 / 2.0;
        for i in 0..TAPS_NEEDED {
            let p = pos2(row_left + slot_w * (i as f32 + 0.5), center.y + 182.0);
            match self.taps.get(i) {
                Some(d) => {
                    let color = crate::gameplay::judge::judge(*d, 1.0)
                        .map_or(Color32::GRAY, theme::judgment_color);
                    painter.circle_filled(p, 7.0, color);
                }
                None => {
                    painter.circle_stroke(
                        p,
                        6.0,
                        Stroke::new(1.5, palette.text_dim.gamma_multiply(0.5)),
                    );
                }
            }
        }
        draw_tap_strip(
            &painter,
            pos2(rect.center().x, center.y + 236.0),
            &self.taps,
            self.last_flash.map(|f| anim::since(f, now)),
            &palette,
        );

        if self.taps.len() >= TAPS_NEEDED || stopped || t > beat_time(TOTAL_BEATS) + 0.3 {
            let result = (self.taps.len() >= MIN_TAPS)
                .then(|| math::robust_stats(&self.taps))
                .flatten();
            if let Some(m) = &mut self.music {
                m.fade_out(150);
            }
            self.phase = Phase::Done(result, now);
        }
    }

    fn done_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        shared: &mut Shared,
        input: &FrameInput,
        result: Option<(f64, f64)>,
        settled: bool,
    ) -> Option<CalibrationEvent> {
        let confirm = settled && input.has(MenuAction::Confirm);
        let palette = shared.palette;
        let painter = ui.painter().clone();
        let center = pos2(rect.center().x, rect.top() + rect.height() * 0.3);
        let y = center.y + 170.0;
        let Some((mean, sd)) = result else {
            painter.circle_stroke(center, 64.0, Stroke::new(3.0, theme::DANGER));
            painter.text(
                center,
                Align2::CENTER_CENTER,
                "!",
                font_bold(48.0),
                theme::DANGER,
            );
            painter.text(
                center + vec2(0.0, 100.0),
                Align2::CENTER_CENTER,
                "Ketukannya belum cukup terbaca.",
                font_bold(theme::TEXT_M),
                palette.text,
            );
            painter.text(
                center + vec2(0.0, 128.0),
                Align2::CENTER_CENTER,
                "Coba lagi: tekan bareng setiap bunyi \"tuk\", mulai setelah hitungan 1.",
                font(theme::TEXT_S),
                palette.text_dim,
            );
            let again = Rect::from_min_size(pos2(rect.center().x - 230.0, y), vec2(220.0, 52.0));
            if ui
                .put(again, |ui: &mut Ui| {
                    widgets::primary_button(ui, "Coba lagi  (Enter)", again.size(), true)
                })
                .clicked()
                || confirm
            {
                self.start(input.now);
            }
            let cancel =
                Rect::from_min_size(pos2(again.right() + 20.0, y + 4.0), vec2(200.0, 44.0));
            if ui
                .put(cancel, |ui: &mut Ui| {
                    widgets::secondary_button(ui, &palette, "Batal  (Esc)", cancel.size())
                })
                .clicked()
            {
                return Some(CalibrationEvent::Cancelled);
            }
            return None;
        };

        let ms = (mean * 1000.0).round();
        painter.circle_filled(center, 64.0, SUCCESS.gamma_multiply(0.15));
        painter.circle_stroke(center, 64.0, Stroke::new(3.0, SUCCESS));
        widgets::text_tracked(
            &painter,
            center,
            Align2::CENTER_CENTER,
            &format!("{ms:+.0} ms"),
            font_bold(theme::TEXT_L),
            palette.text,
        );
        let direction = if ms > 0.5 {
            format!("Kamu menekan rata-rata {ms:.0} ms setelah bunyi. Game akan menyesuaikannya.")
        } else if ms < -0.5 {
            format!(
                "Kamu menekan rata-rata {:.0} ms sebelum bunyi. Game akan menyesuaikannya.",
                -ms
            )
        } else {
            "Ketukanmu sudah pas dengan bunyinya.".to_string()
        };
        painter.text(
            center + vec2(0.0, 100.0),
            Align2::CENTER_CENTER,
            direction,
            font_bold(theme::TEXT_M),
            palette.text,
        );
        let steady = sd * 1000.0 < 45.0;
        painter.text(
            center + vec2(0.0, 128.0),
            Align2::CENTER_CENTER,
            if steady {
                "Ketukanmu stabil, hasilnya bisa dipercaya."
            } else {
                "Ketukanmu masih kurang rata. Disarankan ulangi sekali lagi."
            },
            font(theme::TEXT_S),
            if steady { SUCCESS } else { palette.text_dim },
        );
        let save = Rect::from_min_size(pos2(rect.center().x - 310.0, y), vec2(220.0, 52.0));
        if ui
            .put(save, |ui: &mut Ui| {
                widgets::primary_button(ui, "Simpan  (Enter)", save.size(), true)
            })
            .clicked()
            || confirm
        {
            return Some(CalibrationEvent::Accepted(ms));
        }
        let again = Rect::from_min_size(pos2(save.right() + 16.0, y + 4.0), vec2(180.0, 44.0));
        if ui
            .put(again, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, "Ulangi", again.size())
            })
            .clicked()
        {
            self.start(input.now);
        }
        let cancel = Rect::from_min_size(pos2(again.right() + 16.0, y + 4.0), vec2(180.0, 44.0));
        if ui
            .put(cancel, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, "Batal  (Esc)", cancel.size())
            })
            .clicked()
        {
            return Some(CalibrationEvent::Cancelled);
        }
        None
    }
}

/// Half-width of the strip in seconds (taps beyond are pinned to the edge).
const STRIP_RANGE: f64 = 0.15;

/// "early ← → late" strip: one dot per tap (newest brightest) and the latest
/// tap's judgment word, like in gameplay.
fn draw_tap_strip(
    painter: &Painter,
    center: Pos2,
    taps: &[f64],
    since_last: Option<f32>,
    palette: &theme::Palette,
) {
    use crate::gameplay::judge::{Judgment, judge};
    let half = 210.0;
    let track = Rect::from_center_size(center, vec2(half * 2.0, 6.0));
    // Judgment zones, widest first, so the middle shows the tightest color.
    for (i, w) in crate::gameplay::judge::WINDOWS.iter().enumerate().rev() {
        let x = (w / STRIP_RANGE).min(1.0) as f32 * half;
        let j = Judgment::ALL[i];
        painter.rect_filled(
            Rect::from_center_size(center, vec2(x * 2.0, 6.0)),
            3,
            theme::judgment_color(j).gamma_multiply(0.25),
        );
    }
    painter.line_segment(
        [center - vec2(0.0, 12.0), center + vec2(0.0, 12.0)],
        Stroke::new(2.0, palette.text),
    );
    painter.text(
        track.left_bottom() + vec2(0.0, 14.0),
        Align2::LEFT_TOP,
        "cepat",
        font(theme::TEXT_XS),
        palette.text_dim,
    );
    painter.text(
        track.right_bottom() + vec2(0.0, 14.0),
        Align2::RIGHT_TOP,
        "lambat",
        font(theme::TEXT_XS),
        palette.text_dim,
    );
    let n = taps.len();
    for (k, d) in taps.iter().enumerate() {
        let x = (d / STRIP_RANGE).clamp(-1.0, 1.0) as f32 * half;
        let age = (n - 1 - k) as f32;
        let color = judge(*d, 1.0).map_or(Color32::GRAY, theme::judgment_color);
        let newest = k + 1 == n;
        let pop = if newest {
            since_last.map_or(0.0, |t| (1.0 - t / 0.25).max(0.0))
        } else {
            0.0
        };
        painter.circle_filled(
            center + vec2(x, 0.0),
            4.0 + 3.0 * pop + if newest { 1.5 } else { 0.0 },
            color.gamma_multiply((1.0 - age * 0.06).max(0.25)),
        );
    }
    // Latest judgment word pops above the strip.
    if let (Some(d), Some(t)) = (taps.last(), since_last)
        && t < 0.8
    {
        let alpha = (1.0 - (t - 0.5).max(0.0) / 0.3).clamp(0.0, 1.0);
        let scale = 1.0 + 0.25 * (1.0 - (t / 0.15).min(1.0));
        let (word, color) = judge(*d, 1.0).map_or(("Meleset", theme::DANGER), |j| {
            (j.label(), theme::judgment_color(j))
        });
        widgets::text_tracked_scaled(
            painter,
            center - vec2(0.0, 30.0),
            Align2::CENTER_CENTER,
            word,
            font_bold(theme::TEXT_M),
            color.gamma_multiply(alpha),
            scale,
        );
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

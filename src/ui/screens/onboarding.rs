//! First-run onboarding (≤ 1 minute, skippable): controls → calibration → first song.

use std::time::Instant;

use eframe::egui::{Align2, Rect, Stroke, Ui, pos2, vec2};

use super::Nav;
use super::calibration::{Calibration, CalibrationEvent};
use crate::app::Shared;
use crate::input::{FrameInput, MenuAction};
use crate::ui::theme::{self, ACCENT_B, COLUMN_COLORS, SUCCESS, font, font_bold};
use crate::ui::{anim, beatfx, notefield, widgets};

const STEP_TITLES: [&str; 3] = ["Kontrol", "Sinkronisasi", "Mulai main"];

pub struct OnboardingScreen {
    step: usize,
    calibration: Option<Calibration>,
    lit: [Option<Instant>; 4],
    step_started: Instant,
}

impl Default for OnboardingScreen {
    fn default() -> Self {
        Self {
            step: 0,
            calibration: None,
            lit: [None; 4],
            step_started: Instant::now(),
        }
    }
}

impl OnboardingScreen {
    /// True while the offset calibration is running (it needs silence).
    pub fn is_calibrating(&self) -> bool {
        self.calibration.is_some()
    }

    fn next(&mut self) {
        self.step += 1;
        self.calibration = None;
        self.step_started = Instant::now();
    }

    pub fn ui(&mut self, ui: &mut Ui, shared: &mut Shared, input: &FrameInput) -> Option<Nav> {
        let now = input.now;
        let palette = shared.palette;
        let rect = ui.max_rect();
        for c in input.columns.iter().filter(|c| c.pressed) {
            self.lit[usize::from(c.column.min(3))] = Some(c.at);
        }

        let reduce = shared.settings.reduce_motion;
        let mut beat = beatfx::Beat::idle(anim::since(shared.start, now));
        if reduce {
            beat = beat.calm();
        } else {
            ui.ctx().request_repaint();
        }
        if palette.dark {
            beatfx::draw_highway(ui.painter(), rect, COLUMN_COLORS[self.step.min(3)], &beat);
        }

        // Steps drawn as notes on a track: arrows joined by a line.
        let top = rect.top() + 48.0;
        let xs: Vec<f32> = (0..STEP_TITLES.len())
            .map(|i| rect.center().x + (i as f32 - 1.0) * 160.0)
            .collect();
        for w in xs.windows(2).enumerate() {
            let (i, pair) = w;
            let done = i < self.step;
            ui.painter().line_segment(
                [pos2(pair[0] + 18.0, top), pos2(pair[1] - 18.0, top)],
                Stroke::new(
                    2.0,
                    if done {
                        SUCCESS
                    } else {
                        palette.text_dim.gamma_multiply(0.3)
                    },
                ),
            );
        }
        for (i, title) in STEP_TITLES.iter().enumerate() {
            let x = xs[i];
            let active = i == self.step;
            let done = i < self.step;
            let col = COLUMN_COLORS[i];
            let center = pos2(x, top);
            if active {
                notefield::draw_glow(ui.painter(), center, 22.0, col, 0.5 + 0.5 * beat.pulse);
            }
            notefield::draw_arrow(
                ui.painter(),
                center,
                if active { 30.0 } else { 24.0 },
                // Right-pointing arrow: "forward".
                3,
                if done || active {
                    col
                } else {
                    col.gamma_multiply(0.15)
                },
                Stroke::new(
                    1.2,
                    if done || active {
                        col
                    } else {
                        col.gamma_multiply(0.5)
                    },
                ),
            );
            ui.painter().text(
                pos2(x, top + 22.0),
                Align2::CENTER_TOP,
                *title,
                font(theme::TEXT_XS + 1.0),
                if active || done {
                    palette.text
                } else {
                    palette.text_dim
                },
            );
        }
        let skip = Rect::from_min_size(
            pos2(rect.right() - 172.0, rect.top() + 28.0),
            vec2(140.0, 40.0),
        );
        if ui
            .put(skip, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, "Lewati semua", skip.size())
            })
            .clicked()
        {
            return Some(Nav::Title);
        }

        let area = Rect::from_center_size(
            pos2(rect.center().x, rect.center().y + 30.0),
            vec2(
                (rect.width() - 96.0).min(900.0),
                (rect.height() - 180.0).min(540.0),
            ),
        );
        let appear = anim::ease_out_cubic(anim::progress(
            self.step_started,
            now,
            0.3,
            shared.settings.reduce_motion,
        ));
        let area = area.translate(vec2(0.0, (1.0 - appear) * 12.0));

        match self.step {
            0 => self.controls_step(ui, shared, input, area),
            1 => self.calibration_step(ui, shared, input, area),
            _ => start_step(ui, shared, input, area),
        }
    }

    fn controls_step(
        &mut self,
        ui: &mut Ui,
        shared: &mut Shared,
        input: &FrameInput,
        area: Rect,
    ) -> Option<Nav> {
        let palette = shared.palette;
        let painter = ui.painter().clone();
        widgets::text_tracked(
            &painter,
            area.center_top(),
            Align2::CENTER_TOP,
            "Selamat datang di Ferrostep!",
            font_bold(theme::TEXT_XL),
            palette.text,
        );
        painter.text(
            area.center_top() + vec2(0.0, 48.0),
            Align2::CENTER_TOP,
            "Injak (atau tekan) panah tepat saat panah menyentuh garis di atas.",
            font(theme::TEXT_M),
            palette.text_dim,
        );

        // Live arrows
        let row_y = area.top() + 170.0;
        for col in 0..4 {
            let c = pos2(area.center().x + (col as f32 - 1.5) * 110.0, row_y);
            let glow =
                self.lit[col].map_or(0.0, |t| 1.0 - anim::progress(t, input.now, 0.4, false));
            if glow > 0.0 {
                ui.ctx().request_repaint();
                notefield::draw_glow(&painter, c, 48.0, COLUMN_COLORS[col], glow);
            }
            notefield::draw_arrow(
                &painter,
                c,
                36.0,
                col as u8,
                COLUMN_COLORS[col].gamma_multiply(0.25 + 0.75 * glow),
                Stroke::new(2.0, COLUMN_COLORS[col]),
            );
            let keys = shared.settings.keybinds[col]
                .first()
                .cloned()
                .unwrap_or_default();
            painter.text(
                c + vec2(0.0, 56.0),
                Align2::CENTER_CENTER,
                keys,
                font_bold(theme::TEXT_M),
                palette.text,
            );
        }
        painter.text(
            pos2(area.center().x, row_y + 96.0),
            Align2::CENTER_CENTER,
            "Keyboard: D F J K atau tombol panah. Coba tekan sekarang!",
            font(theme::TEXT_S),
            palette.text_dim,
        );

        // Dance mat status
        let card = Rect::from_center_size(pos2(area.center().x, row_y + 180.0), vec2(560.0, 84.0));
        widgets::card(&painter, card, &palette);
        let (title, sub, color) = match &shared.gamepad_name {
            Some(name) => (
                format!("Dance mat terhubung: {name}"),
                "Injak panel-panelnya: panah di atas ikut menyala.".to_string(),
                SUCCESS,
            ),
            None => (
                "Punya dance mat USB?".to_string(),
                "Colokkan kapan saja: Ferrostep akan mengenalinya dan memandu pengaturannya."
                    .to_string(),
                ACCENT_B,
            ),
        };
        painter.circle_filled(card.left_center() + vec2(32.0, 0.0), 6.0, color);
        painter.text(
            card.left_center() + vec2(52.0, -12.0),
            Align2::LEFT_CENTER,
            title,
            font_bold(theme::TEXT_S + 1.0),
            palette.text,
        );
        painter.text(
            card.left_center() + vec2(52.0, 14.0),
            Align2::LEFT_CENTER,
            sub,
            font(theme::TEXT_XS + 1.0),
            palette.text_dim,
        );

        let next = Rect::from_center_size(
            pos2(area.center().x, area.bottom() - 26.0),
            vec2(220.0, 52.0),
        );
        if ui
            .put(next, |ui: &mut Ui| {
                widgets::primary_button(ui, "Lanjut  ›", next.size(), true)
            })
            .clicked()
            || input.has(MenuAction::Confirm)
        {
            self.next();
        }
        None
    }

    fn calibration_step(
        &mut self,
        ui: &mut Ui,
        shared: &mut Shared,
        input: &FrameInput,
        area: Rect,
    ) -> Option<Nav> {
        let palette = shared.palette;
        let painter = ui.painter().clone();
        widgets::text_tracked(
            &painter,
            area.center_top(),
            Align2::CENTER_TOP,
            "Samakan waktu suara dan layar",
            font_bold(theme::TEXT_XL),
            palette.text,
        );
        let Some(cal) = &mut self.calibration else {
            painter.text(
                area.center_top() + vec2(0.0, 56.0),
                Align2::CENTER_TOP,
                "Setiap speaker/headset punya jeda berbeda. Ketuk mengikuti klik 16 kali (±20 detik)",
                font(theme::TEXT_M),
                palette.text_dim,
            );
            painter.text(
                area.center_top() + vec2(0.0, 84.0),
                Align2::CENTER_TOP,
                "agar penilaian terasa adil. Bisa diulang kapan saja di Pengaturan → Audio.",
                font(theme::TEXT_M),
                palette.text_dim,
            );
            let start =
                Rect::from_center_size(area.center() + vec2(-90.0, 40.0), vec2(220.0, 52.0));
            if ui
                .put(start, |ui: &mut Ui| {
                    widgets::primary_button(ui, "Mulai kalibrasi", start.size(), true)
                })
                .clicked()
                || input.has(MenuAction::Confirm)
            {
                self.calibration = Some(Calibration::without_intro());
            }
            let skip = Rect::from_center_size(area.center() + vec2(130.0, 40.0), vec2(180.0, 44.0));
            if ui
                .put(skip, |ui: &mut Ui| {
                    widgets::secondary_button(ui, &palette, "Lewati langkah ini", skip.size())
                })
                .clicked()
            {
                self.next();
            }
            return None;
        };
        match cal.ui(ui, area.with_min_y(area.top() + 50.0), shared, input) {
            Some(CalibrationEvent::Accepted(ms)) => {
                shared.settings.global_offset_ms = ms;
                shared
                    .toasts
                    .success(format!("Global offset diatur ke {ms:+.0} ms"));
                self.next();
            }
            Some(CalibrationEvent::Cancelled) => self.calibration = None,
            None => {}
        }
        None
    }
}

fn start_step(ui: &mut Ui, shared: &mut Shared, input: &FrameInput, area: Rect) -> Option<Nav> {
    let palette = shared.palette;
    let painter = ui.painter().clone();
    widgets::text_tracked(
        &painter,
        area.center_top(),
        Align2::CENTER_TOP,
        "Siap main!",
        font_bold(theme::TEXT_XL),
        palette.text,
    );
    painter.text(
        area.center_top() + vec2(0.0, 48.0),
        Align2::CENTER_TOP,
        "Coba lagu contoh dulu, atau langsung ubah lagu favoritmu jadi chart.",
        font(theme::TEXT_M),
        palette.text_dim,
    );
    // Four arrows lighting up in turn: a taste of the game.
    let beat = beatfx::Beat::idle(anim::since(shared.start, input.now));
    let current = beat.beat.rem_euclid(4.0).floor() as u8;
    for c in 0..4u8 {
        let center = area.center_top() + vec2((f32::from(c) - 1.5) * 64.0, 120.0);
        let color = COLUMN_COLORS[usize::from(c)];
        let lit = c == current && !shared.settings.reduce_motion;
        if lit {
            notefield::radial_light(&painter, center, 54.0, color, 0.7 * beat.pulse.max(0.3));
        }
        notefield::draw_arrow(
            &painter,
            center,
            44.0,
            c,
            if lit {
                color
            } else {
                color.gamma_multiply(0.2)
            },
            Stroke::new(1.5, color),
        );
    }
    let tile = vec2(400.0, 96.0);
    let a = Rect::from_center_size(area.center() + vec2(0.0, 30.0), tile);
    let b = a.translate(vec2(0.0, tile.y + 16.0));
    let mut nav = None;
    if ui
        .put(a, |ui: &mut Ui| {
            widgets::menu_tile(
                ui,
                &palette,
                "Main lagu contoh",
                "Metronom 120 BPM untuk pemanasan",
                tile,
                false,
            )
        })
        .clicked()
        || input.has(MenuAction::Confirm)
    {
        nav = Some(Nav::SongSelect);
    }
    let import_sub = if super::import::drop_supported() {
        "Seret atau pilih MP3/MP4 milikmu"
    } else {
        "Pilih MP3/MP4 milikmu"
    };
    if ui
        .put(b, |ui: &mut Ui| {
            widgets::menu_tile(
                ui,
                &palette,
                "Impor lagu pertamamu",
                import_sub,
                tile,
                false,
            )
        })
        .clicked()
    {
        nav = Some(Nav::Import(Vec::new()));
    }
    nav
}

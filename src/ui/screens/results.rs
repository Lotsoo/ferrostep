//! Results screen: grade, percentage, judgment bars, timing graph, calibration hint.

use std::time::Instant;

use eframe::egui::{self, Align2, Pos2, Rect, Stroke, Ui, pos2, vec2};

use super::{Nav, ResultData, difficulty_color, difficulty_label};
use crate::app::Shared;
use crate::gameplay::judge::{Judgment, WINDOWS};
use crate::input::{FrameInput, MenuAction};
use crate::ui::theme::{self, font, font_bold, judgment_color};
use crate::ui::{anim, widgets};

pub struct ResultsScreen {
    data: ResultData,
    opened: Instant,
    offset_applied: bool,
    selected: usize,
}

impl ResultsScreen {
    pub fn new(data: ResultData) -> Self {
        Self {
            data,
            opened: Instant::now(),
            offset_applied: false,
            selected: 1,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, shared: &mut Shared, input: &FrameInput) -> Option<Nav> {
        let now = input.now;
        let reduce = shared.settings.reduce_motion;
        let palette = shared.palette;
        let t = anim::since(self.opened, now);
        if t < 2.0 {
            ui.ctx().request_repaint();
        }

        for a in &input.menu {
            match a {
                MenuAction::Left | MenuAction::Up => self.selected = 0,
                MenuAction::Right | MenuAction::Down => self.selected = 1,
                MenuAction::Back => return Some(Nav::SongSelect),
                MenuAction::Confirm => {
                    return Some(if self.selected == 0 {
                        Nav::Play(self.data.request.clone())
                    } else {
                        Nav::SongSelect
                    });
                }
            }
        }

        let rect = ui.max_rect().shrink(32.0);
        let painter = ui.painter().clone();
        let score = &self.data.score;

        // ── Left: grade + percentage ─────────────────────────────
        let left = Rect::from_min_size(rect.min, vec2(rect.width() * 0.38, rect.height() - 220.0));
        widgets::card(&painter, left, &palette);
        let grade = score.grade(self.data.no_fail);
        let appear = anim::progress(self.opened, now, 0.5, reduce);
        let scale = anim::ease_out_back(appear);
        let grade_pos = left.center_top() + vec2(0.0, left.height() * 0.32);
        let size = 150.0 * scale.max(0.01);
        crate::ui::hud::draw_grade_badge(
            &painter,
            Rect::from_center_size(grade_pos, vec2(size, size)),
            grade,
            (1.0 - appear) * 0.5,
            ui.input(|i| i.time) as f32,
            reduce,
        );
        if !reduce {
            ui.ctx().request_repaint();
        }

        let count_up = anim::ease_out_cubic(anim::progress(self.opened, now, 1.2, reduce));
        let shown = score.percent() * f64::from(count_up);
        widgets::tabular_text(
            &painter,
            grade_pos + vec2(0.0, 118.0),
            Align2::CENTER_CENTER,
            &format!("{shown:.2}%"),
            font_bold(theme::TEXT_XL + 8.0),
            palette.text,
        );
        if let Some(prev) = self.data.previous_best {
            let (text, color) = if score.percent() > prev {
                (
                    format!("Rekor baru! (sebelumnya {prev:.2}%)"),
                    theme::SUCCESS,
                )
            } else {
                (format!("Terbaik: {prev:.2}%"), palette.text_dim)
            };
            painter.text(
                grade_pos + vec2(0.0, 156.0),
                Align2::CENTER_CENTER,
                text,
                font(theme::TEXT_S),
                color,
            );
        }
        painter.text(
            left.center_bottom() - vec2(0.0, 64.0),
            Align2::CENTER_CENTER,
            &self.data.title,
            font_bold(theme::TEXT_M),
            palette.text,
        );
        painter.text(
            left.center_bottom() - vec2(0.0, 38.0),
            Align2::CENTER_CENTER,
            format!(
                "{} · {} {}",
                self.data.artist,
                difficulty_label(self.data.difficulty),
                self.data.meter
            ),
            font(theme::TEXT_S),
            difficulty_color(self.data.difficulty),
        );

        // ── Right: judgment bars ─────────────────────────────────
        let right = Rect::from_min_max(
            pos2(left.right() + 24.0, rect.top()),
            pos2(rect.right(), left.bottom()),
        );
        widgets::card(&painter, right, &palette);
        let total: u32 = score.counts.iter().sum::<u32>().max(1);
        let bar_left = right.left() + 140.0;
        let bar_w = right.width() - 140.0 - 90.0;
        for (i, j) in Judgment::ALL.iter().enumerate() {
            let y = right.top() + 40.0 + i as f32 * 40.0;
            let count = score.counts[j.index()];
            painter.text(
                pos2(right.left() + 28.0, y),
                Align2::LEFT_CENTER,
                j.label(),
                font_bold(theme::TEXT_S),
                judgment_color(*j),
            );
            let grow = anim::ease_out_cubic(anim::progress(
                self.opened,
                now,
                0.6 + i as f32 * 0.08,
                reduce,
            ));
            let frac = count as f32 / total as f32 * grow;
            let track = Rect::from_min_size(pos2(bar_left, y - 6.0), vec2(bar_w, 12.0));
            painter.rect_filled(track, 6, palette.border);
            if frac > 0.0 {
                painter.rect_filled(
                    Rect::from_min_size(track.min, vec2((bar_w * frac).max(12.0), 12.0)),
                    6,
                    judgment_color(*j).gamma_multiply(0.85),
                );
            }
            widgets::tabular_text(
                &painter,
                pos2(right.right() - 28.0, y),
                Align2::RIGHT_CENTER,
                &count.to_string(),
                font_bold(theme::TEXT_S),
                palette.text,
            );
        }
        let stats_y = right.top() + 40.0 + 6.0 * 40.0 + 16.0;
        let (mean, sd) = score.offset_stats().unwrap_or((0.0, 0.0));
        let stats = [
            ("Combo maks", score.max_combo.to_string()),
            (
                "Hold OK/NG",
                format!("{}/{}", score.holds_ok, score.holds_ng),
            ),
            ("Ranjau kena", score.mines_hit.to_string()),
            ("Rata-rata", format!("{mean:+.1} ms")),
            ("Simpangan", format!("{sd:.1} ms")),
        ];
        let col_w = (right.width() - 56.0) / stats.len() as f32;
        for (i, (label, value)) in stats.iter().enumerate() {
            let x = right.left() + 28.0 + col_w * (i as f32 + 0.5);
            painter.text(
                pos2(x, stats_y),
                Align2::CENTER_TOP,
                label,
                font(theme::TEXT_XS),
                palette.text_dim,
            );
            painter.text(
                pos2(x, stats_y + 18.0),
                Align2::CENTER_TOP,
                value,
                font_bold(theme::TEXT_M),
                palette.text,
            );
        }

        // ── Bottom: timing graph ─────────────────────────────────
        let graph = Rect::from_min_max(
            pos2(rect.left(), left.bottom() + 20.0),
            pos2(rect.right(), rect.bottom() - 80.0),
        );
        widgets::card(&painter, graph, &palette);
        self.draw_timing_graph(&painter, graph.shrink(16.0), &palette, now, reduce);

        // ── Calibration hint + buttons ───────────────────────────
        let bottom = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 60.0), rect.max);
        let mut nav = None;
        if score.offsets.len() >= 16 && mean.abs() > 10.0 && !self.offset_applied {
            let word = if mean > 0.0 {
                "terlambat"
            } else {
                "terlalu cepat"
            };
            painter.text(
                bottom.left_center(),
                Align2::LEFT_CENTER,
                format!(
                    "Kamu cenderung menekan {:.0} ms {word}. Sesuaikan offset?",
                    mean.abs()
                ),
                font(theme::TEXT_S),
                palette.text,
            );
            let r =
                Rect::from_min_size(bottom.left_center() + vec2(470.0, -22.0), vec2(170.0, 44.0));
            if ui
                .put(r, |ui: &mut Ui| {
                    widgets::secondary_button(ui, &palette, "Sesuaikan offset", r.size())
                })
                .clicked()
            {
                shared.settings.global_offset_ms += mean;
                self.offset_applied = true;
                shared.toasts.success(format!(
                    "Offset global sekarang {:+.0} ms",
                    shared.settings.global_offset_ms
                ));
            }
        } else if self.offset_applied {
            painter.text(
                bottom.left_center(),
                Align2::LEFT_CENTER,
                "Offset disesuaikan ✓",
                font(theme::TEXT_S),
                theme::SUCCESS,
            );
        }
        let again =
            Rect::from_min_size(bottom.right_center() - vec2(380.0, 24.0), vec2(170.0, 48.0));
        let next =
            Rect::from_min_size(bottom.right_center() - vec2(190.0, 24.0), vec2(190.0, 48.0));
        let r_again = ui.put(again, |ui: &mut Ui| {
            if self.selected == 0 {
                widgets::primary_button(ui, "Main lagi", again.size(), true)
            } else {
                widgets::secondary_button(ui, &palette, "Main lagi", again.size())
            }
        });
        let r_next = ui.put(next, |ui: &mut Ui| {
            if self.selected == 1 {
                widgets::primary_button(ui, "Lanjut  ›", next.size(), true)
            } else {
                widgets::secondary_button(ui, &palette, "Lanjut  ›", next.size())
            }
        });
        if r_again.clicked() {
            nav = Some(Nav::Play(self.data.request.clone()));
        }
        if r_next.clicked() {
            nav = Some(Nav::SongSelect);
        }
        nav
    }

    fn draw_timing_graph(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        palette: &theme::Palette,
        now: Instant,
        reduce: bool,
    ) {
        let max_ms = WINDOWS[4] * 1000.0;
        let y_for = |ms: f64| rect.center().y - (ms / max_ms) as f32 * rect.height() / 2.0;
        // window bands
        for (i, w) in WINDOWS.iter().enumerate().take(3) {
            let ms = w * 1000.0;
            let band = Rect::from_x_y_ranges(rect.x_range(), y_for(ms)..=y_for(-ms));
            painter.rect_filled(
                band,
                0,
                judgment_color(Judgment::ALL[i]).gamma_multiply(0.04),
            );
        }
        painter.line_segment(
            [
                pos2(rect.left(), rect.center().y),
                pos2(rect.right(), rect.center().y),
            ],
            Stroke::new(1.0, palette.text_dim.gamma_multiply(0.5)),
        );
        painter.text(
            rect.left_top(),
            Align2::LEFT_TOP,
            "terlambat",
            font(theme::TEXT_XS),
            palette.text_dim,
        );
        painter.text(
            rect.left_bottom(),
            Align2::LEFT_BOTTOM,
            "terlalu cepat",
            font(theme::TEXT_XS),
            palette.text_dim,
        );

        let len = self.data.song_length.max(1.0);
        let reveal = anim::progress(self.opened, now, 1.0, reduce);
        for &(time, ms) in &self.data.score.offsets {
            let fx = (time / len) as f32;
            if fx > reveal {
                continue;
            }
            let x = rect.left() + fx.clamp(0.0, 1.0) * rect.width();
            let j = crate::gameplay::judge::judge(ms / 1000.0, 1.0).unwrap_or(Judgment::Boo);
            painter.circle_filled(
                Pos2::new(x, y_for(ms.clamp(-max_ms, max_ms))),
                2.5,
                judgment_color(j),
            );
        }
    }
}

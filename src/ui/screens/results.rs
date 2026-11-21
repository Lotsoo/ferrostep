//! Results screen: grade, percentage, judgment bars, timing graph, calibration hint.

use std::time::Instant;

use eframe::egui::{self, Align2, Color32, Pos2, Rect, Stroke, Ui, pos2, vec2};

use super::{Nav, ResultData, difficulty_color, difficulty_label};
use crate::app::Shared;
use crate::audio::Sfx;
use crate::gameplay::judge::{Judgment, WINDOWS};
use crate::input::{FrameInput, MenuAction};
use crate::ui::theme::{self, font, font_bold, judgment_color};
use crate::ui::{anim, beatfx, widgets};

/// When the grade slams in (after the percentage finished counting).
const GRADE_AT: f32 = 1.3;
/// When the "new record" / "full combo" stamp lands.
const STAMP_AT: f32 = 1.9;

pub struct ResultsScreen {
    data: ResultData,
    opened: Instant,
    offset_applied: bool,
    selected: usize,
    /// Reveal sounds already played (grade, stamp).
    grade_sound: bool,
    stamp_sound: bool,
}

impl ResultsScreen {
    pub fn new(data: ResultData) -> Self {
        Self {
            data,
            opened: Instant::now(),
            offset_applied: false,
            selected: 1,
            grade_sound: false,
            stamp_sound: false,
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
        let grade = score.grade(self.data.no_fail);
        let style = theme::grade_style(grade);
        // Reveal timeline (skipped with reduce motion).
        let t_reveal = if reduce { 10.0 } else { t };
        let grade_t = t_reveal - GRADE_AT;
        let full_combo = score.counts[Judgment::Good.index()] == 0
            && score.counts[Judgment::Boo.index()] == 0
            && score.counts[Judgment::Miss.index()] == 0
            && score.counts.iter().sum::<u32>() > 0;
        let record = self
            .data
            .previous_best
            .is_none_or(|prev| score.percent() > prev)
            && score.percent() > 0.0;
        let stamp = if full_combo {
            Some("FULL COMBO")
        } else if record {
            Some("REKOR BARU!")
        } else {
            None
        };
        if grade_t >= 0.0 && !self.grade_sound {
            self.grade_sound = true;
            if !reduce {
                shared.sfx(if matches!(grade, "SS" | "S" | "A") {
                    Sfx::Start
                } else {
                    Sfx::Count
                });
            }
        }
        if stamp.is_some() && t_reveal >= STAMP_AT && !self.stamp_sound {
            self.stamp_sound = true;
            if !reduce {
                shared.sfx(Sfx::Success);
            }
        }
        if t < 3.0 {
            ui.ctx().request_repaint();
        }

        // Arcade stage behind everything, tinted by the grade.
        let mut beat = beatfx::Beat::idle(t);
        if reduce {
            beat = beat.calm();
        }
        if palette.dark {
            beatfx::draw_highway(&painter, ui.max_rect(), style.glow, &beat);
        }
        // Flash in the grade color when it lands.
        if !reduce && (0.0..0.35).contains(&grade_t) {
            painter.rect_filled(
                ui.max_rect(),
                0,
                style.glow.gamma_multiply(0.25 * (1.0 - grade_t / 0.35)),
            );
        }

        // ── Left: grade + percentage ─────────────────────────────
        let left = Rect::from_min_size(rect.min, vec2(rect.width() * 0.38, rect.height() - 220.0));
        widgets::card(&painter, left, &palette);
        let grade_pos = left.center_top() + vec2(0.0, left.height() * 0.32);
        if grade_t < 0.0 {
            draw_drum_roll(&painter, grade_pos, t, style.glow);
        } else {
            // Slam: starts huge, lands with a small bounce.
            let land = anim::ease_out_back((grade_t / 0.28).min(1.0));
            let slam = 2.4 - 1.4 * land;
            let size = 150.0 * slam;
            if !reduce {
                draw_impact(&painter, grade_pos, grade_t, style.glow);
            }
            crate::ui::hud::draw_grade_badge(
                &painter,
                Rect::from_center_size(grade_pos, vec2(size, size)),
                grade,
                (1.0 - (grade_t / 0.5).min(1.0)) * 0.5,
                ui.input(|i| i.time) as f32,
                reduce,
            );
            if let Some(text) = stamp {
                draw_stamp(
                    &painter,
                    grade_pos + vec2(84.0, -78.0),
                    text,
                    t_reveal - STAMP_AT,
                    if full_combo {
                        Color32::from_rgb(0xFF, 0xD8, 0x4D)
                    } else {
                        theme::SUCCESS
                    },
                );
            }
        }
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
                &((count as f32 * grow).round() as u32).to_string(),
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

/// Spinning "drum roll" while the score counts up, before the grade lands.
fn draw_drum_roll(painter: &egui::Painter, center: Pos2, t: f32, color: Color32) {
    for k in 0..12 {
        let a = k as f32 / 12.0 * std::f32::consts::TAU + t * 9.0;
        let lit = ((t * 18.0) as i32 - k).rem_euclid(12) < 3;
        painter.circle_filled(
            center + vec2(a.cos(), a.sin()) * 64.0,
            if lit { 6.0 } else { 4.0 },
            if lit {
                color
            } else {
                Color32::from_white_alpha(40)
            },
        );
    }
    crate::ui::notefield::radial_light(painter, center, 90.0, color, 0.2 + 0.1 * (t * 12.0).sin());
}

/// Shockwave rings and sparks when the grade lands.
fn draw_impact(painter: &egui::Painter, center: Pos2, age: f32, color: Color32) {
    if age > 0.9 {
        return;
    }
    for (k, delay) in [(0.0f32, 0.0f32), (1.0, 0.1)] {
        let e = anim::ease_out_cubic(((age - delay) / 0.7).clamp(0.0, 1.0));
        if e <= 0.0 || e >= 1.0 {
            continue;
        }
        painter.circle_stroke(
            center,
            80.0 + e * 320.0,
            Stroke::new((9.0 - k * 4.0) * (1.0 - e), color.gamma_multiply(1.0 - e)),
        );
    }
    let e = anim::ease_out_cubic((age / 0.8).min(1.0));
    for i in 0..20 {
        let a = i as f32 / 20.0 * std::f32::consts::TAU;
        let d = 90.0 + e * (150.0 + (i % 4) as f32 * 45.0);
        painter.circle_filled(
            center + vec2(a.cos(), a.sin()) * d,
            4.0 * (1.0 - e) + 0.5,
            color
                .lerp_to_gamma(Color32::WHITE, 0.5)
                .gamma_multiply(1.0 - e),
        );
    }
}

/// Tilted rubber-stamp label ("FULL COMBO", "REKOR BARU!") that thumps down.
fn draw_stamp(painter: &egui::Painter, center: Pos2, text: &str, age: f32, color: Color32) {
    if age < 0.0 {
        return;
    }
    let land = anim::ease_out_back((age / 0.22).min(1.0));
    let scale = 1.8 - 0.8 * land;
    let alpha = (age / 0.1).min(1.0);
    let galley = painter.layout_no_wrap(
        text.to_string(),
        font_bold(theme::TEXT_M * scale),
        theme::INK.gamma_multiply(alpha),
    );
    let size = galley.size() + vec2(24.0, 12.0) * scale;
    let rect = Rect::from_center_size(center, size);
    crate::ui::notefield::radial_light(painter, center, size.x, color, 0.35 * alpha);
    painter.rect_filled(rect, 8.0 * scale, color.gamma_multiply(alpha));
    painter.rect_stroke(
        rect.expand(3.0),
        11.0 * scale,
        Stroke::new(2.0, Color32::WHITE.gamma_multiply(0.8 * alpha)),
        egui::StrokeKind::Outside,
    );
    painter.galley(rect.center() - galley.size() / 2.0, galley, theme::INK);
}

//! Interactive tutorial: the song only moves forward when the player presses.
//!
//! Arrows rise to the targets and *stop there and wait* until the right
//! key / panel is pressed; every press plays the next note of the melody
//! ("Twinkle Twinkle Little Star", a traditional tune), so the player makes
//! the music by playing. Six short lessons: single arrows, colors and keys,
//! timing, jumps, holds, and a final mix. Used on first start (onboarding)
//! and from the main menu.

use std::time::Instant;

use eframe::egui::{
    self, Align2, Color32, Rect, Stroke, Ui, pos2,
    text::{LayoutJob, TextFormat},
    vec2,
};

use crate::app::Shared;
use crate::gameplay::judge::{Judgment, judge};
use crate::input::{FrameInput, MenuAction};
use crate::ui::notefield::{self, HIT_EFFECT_LEN, HitEffect, Layout};
use crate::ui::theme::{self, ACCENT_A, ACCENT_B, SUCCESS, column_color, font, font_bold};
use crate::ui::{anim, widgets};

use super::lessons::*;

/// What the tutorial asks its host to do when the player finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TutorialEvent {
    /// Onboarding: continue with the next step.
    Next,
    Play,
    Calibrate,
    Menu,
}

/// The tutorial widget. See module docs.
pub struct Tutorial {
    lesson: usize,
    notes: Vec<Note>,
    /// Lesson time in beats.
    t: f64,
    last_frame: Option<Instant>,
    held: [bool; 4],
    /// Press flash per column and wrong-press shake per column.
    pressed_at: [Option<Instant>; 4],
    wrong_at: [Option<Instant>; 4],
    effects: Vec<(HitEffect, Instant)>,
    hits: u32,
    feedback: Option<(&'static str, Color32, Instant)>,
    /// Melody steps already played (one tone per step, even for jumps).
    sounded: Vec<usize>,
    lesson_started: Instant,
    lesson_done: Option<Instant>,
    /// Judgment counts over the whole tutorial: perfect-ish, good, waited.
    counts: [u32; 3],
    finished: Option<Instant>,
    /// Shown inside onboarding (final button is "Lanjut").
    onboarding: bool,
    selected: usize,
}

impl Tutorial {
    pub fn new(onboarding: bool) -> Self {
        let now = Instant::now();
        let mut t = Self {
            lesson: 0,
            notes: Vec::new(),
            t: 0.0,
            last_frame: None,
            held: [false; 4],
            pressed_at: [None; 4],
            wrong_at: [None; 4],
            effects: Vec::new(),
            hits: 0,
            feedback: None,
            sounded: Vec::new(),
            lesson_started: now,
            lesson_done: None,
            counts: [0; 3],
            finished: None,
            onboarding,
            selected: 0,
        };
        t.start_lesson(0, now);
        t
    }

    fn start_lesson(&mut self, lesson: usize, now: Instant) {
        let defs = lesson_defs();
        self.lesson = lesson;
        self.notes = build_notes(&defs[lesson]);
        // Arrows start below the screen and rise in.
        self.t = -2.5;
        self.sounded.clear();
        self.lesson_started = now;
        self.lesson_done = None;
    }

    fn beats_per_second() -> f64 {
        BPM / 60.0
    }

    /// Advances the lesson clock; it stops at the first arrow that was not
    /// pressed yet, and while a hold is not being held.
    fn advance(&mut self, now: Instant) {
        let dt = self
            .last_frame
            .map_or(0.0, |l| now.saturating_duration_since(l).as_secs_f64())
            .min(0.1);
        self.last_frame = Some(now);
        let mut limit = f64::INFINITY;
        for n in &self.notes {
            if !n.hit {
                limit = limit.min(n.beat);
            } else if !n.done && !self.held[usize::from(n.column)] {
                limit = limit.min(self.t);
            }
        }
        self.t = (self.t + dt * Self::beats_per_second()).min(limit.max(self.t));
        for n in &mut self.notes {
            if !n.hit && n.arrived.is_none() && self.t >= n.beat - 1e-9 {
                n.arrived = Some(now);
            }
            if n.hit && !n.done && self.t >= n.beat + n.hold - 1e-9 {
                n.done = true;
                self.effects.push((
                    HitEffect {
                        column: n.column,
                        color: SUCCESS,
                        age: 0.0,
                        seed: self.hits.wrapping_mul(7),
                    },
                    now,
                ));
            }
        }
    }

    fn press(&mut self, column: u8, at: Instant, shared: &mut Shared) {
        let col = usize::from(column);
        self.held[col] = true;
        self.pressed_at[col] = Some(at);
        let window = EARLY_WINDOW * Self::beats_per_second();
        let candidate = self
            .notes
            .iter()
            .enumerate()
            .filter(|(_, n)| !n.hit && n.column == column && n.beat - self.t <= window)
            .min_by(|a, b| a.1.beat.total_cmp(&b.1.beat))
            .map(|(i, _)| i);
        let Some(i) = candidate else {
            self.wrong_at[col] = Some(at);
            return;
        };
        let note = &mut self.notes[i];
        note.hit = true;
        if note.hold <= 0.0 {
            note.done = true;
        }
        // Timing: early presses count from the arrival time, late ones from
        // when the arrow started waiting.
        let early = (note.beat - self.t) / Self::beats_per_second();
        let late = note
            .arrived
            .map_or(0.0, |a| at.saturating_duration_since(a).as_secs_f64());
        let error = if early > 0.0 { early } else { late };
        let (word, color, bucket) = match judge(error, 1.0) {
            Some(j @ (Judgment::Marvelous | Judgment::Perfect)) => {
                (j.label(), theme::judgment_color(j), 0)
            }
            Some(j @ (Judgment::Great | Judgment::Good)) => {
                (j.label(), theme::judgment_color(j), 1)
            }
            _ => ("Oke!", ACCENT_B, 2),
        };
        self.counts[bucket] += 1;
        self.feedback = Some((word, color, at));
        self.hits = self.hits.wrapping_add(1);
        self.effects.push((
            HitEffect {
                column,
                color,
                age: 0.0,
                seed: self.hits,
            },
            at,
        ));
        let (step, pitch, hold) = (note.step, note.pitch, note.hold);
        if !self.sounded.contains(&step) {
            self.sounded.push(step);
            if let Some(audio) = &mut shared.audio {
                let length = if hold > 0.0 {
                    (hold / Self::beats_per_second()) as f32 + 0.5
                } else {
                    0.9
                };
                audio.play_tone(pitch, length);
            }
        }
    }

    /// Draws the tutorial in `rect` and handles input.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        shared: &mut Shared,
        input: &FrameInput,
    ) -> Option<TutorialEvent> {
        let now = input.now;
        ui.ctx().request_repaint();
        if self.finished.is_some() {
            return self.finished_ui(ui, rect, shared, input);
        }

        for ev in &input.columns {
            let c = ev.column.min(3);
            if ev.pressed {
                self.press(c, ev.at, shared);
            } else {
                self.held[usize::from(c)] = false;
            }
        }
        self.advance(now);

        let lessons = lesson_defs();
        let all_done = self.notes.iter().all(|n| n.done);
        if all_done && self.lesson_done.is_none() {
            self.lesson_done = Some(now);
            shared.sfx(crate::audio::Sfx::Success);
        }
        if let Some(done) = self.lesson_done
            && anim::since(done, now) >= LESSON_BREAK
        {
            if self.lesson + 1 < lessons.len() {
                self.start_lesson(self.lesson + 1, now);
            } else {
                self.finished = Some(now);
                return None;
            }
        }

        let palette = shared.palette;
        let painter = ui.painter().clone();
        let def = &lessons[self.lesson];
        let reduce = shared.settings.reduce_motion;

        // Header: lesson counter, title and explanation.
        let slide = anim::ease_out_cubic(anim::progress(self.lesson_started, now, 0.35, reduce));
        let text_rect = Rect::from_min_size(
            pos2(rect.left() + 24.0 + (1.0 - slide) * 30.0, rect.top()),
            vec2(rect.width() - 48.0, 110.0),
        );
        painter.text(
            text_rect.left_top(),
            Align2::LEFT_TOP,
            format!("PELAJARAN {} DARI {}", self.lesson + 1, lessons.len()),
            font_bold(theme::TEXT_XS),
            column_color((self.lesson % 4) as u8).gamma_multiply(slide),
        );
        painter.text(
            text_rect.left_top() + vec2(0.0, 20.0),
            Align2::LEFT_TOP,
            def.title,
            font_bold(theme::TEXT_L + 4.0),
            palette.text.gamma_multiply(slide),
        );
        let mut job = LayoutJob::default();
        job.wrap.max_width = text_rect.width().min(760.0);
        job.append(
            def.text,
            0.0,
            TextFormat {
                font_id: font(theme::TEXT_M),
                color: palette.text_dim.gamma_multiply(slide),
                ..Default::default()
            },
        );
        let galley = painter.layout_job(job);
        let text_h = galley.size().y;
        painter.galley(text_rect.left_top() + vec2(0.0, 58.0), galley, palette.text);
        // Lesson progress dots.
        for i in 0..lessons.len() {
            let p = pos2(
                rect.right() - 24.0 - (lessons.len() - 1 - i) as f32 * 18.0,
                rect.top() + 8.0,
            );
            let color = if i <= self.lesson {
                column_color((i % 4) as u8)
            } else {
                palette.text_dim.gamma_multiply(0.35)
            };
            painter.circle_filled(p, if i == self.lesson { 5.0 } else { 3.5 }, color);
        }

        // The lane.
        let lane_top = rect.top() + 70.0 + text_h + 16.0;
        let lane_rect = Rect::from_min_max(pos2(rect.left(), lane_top), rect.max);
        let scale = shared
            .settings
            .lane_size
            .scale(shared.gamepad_name.is_some());
        let mut layout = Layout::new(lane_rect, 4, false, 1.0);
        layout.arrow = ((lane_rect.height() / 5.2).clamp(56.0, 96.0) * scale)
            .min(lane_rect.height() / 4.2)
            .min(lane_rect.width() * 0.85 / 4.8);
        layout.lane = layout.arrow * 1.2;
        layout.left = lane_rect.center().x - layout.lane * 2.0;
        layout.receptor_y = lane_rect.top() + layout.arrow * 0.75;
        let ppb = layout.arrow * 1.6;
        let lane_bg = Rect::from_min_max(
            pos2(layout.left - 12.0, lane_rect.top()),
            pos2(layout.left + layout.lane * 4.0 + 12.0, lane_rect.bottom()),
        );
        painter.rect_filled(lane_bg, 16, Color32::from_black_alpha(90));

        let pressed: Vec<bool> = self.held.to_vec();
        notefield::draw_receptors(&painter, &layout, 4, &pressed, 0.0, !palette.dark);

        // Key labels under each target.
        for c in 0..4u8 {
            let label = key_hint(shared, c);
            let shake = self.wrong_at[usize::from(c)]
                .map(|w| anim::since(w, now))
                .filter(|s| *s < 0.35)
                .map_or(0.0, |s| (s * 60.0).sin() * 5.0 * (1.0 - s / 0.35));
            painter.text(
                pos2(
                    layout.column_x(c) + shake,
                    layout.receptor_y - layout.arrow * 0.62,
                ),
                Align2::CENTER_BOTTOM,
                label,
                font_bold(theme::TEXT_S),
                column_color(c),
            );
        }

        // Notes (bottom to top so the next one is drawn last).
        let clip = painter.clone().with_clip_rect(lane_bg);
        for n in self.notes.iter().rev() {
            if n.done {
                continue;
            }
            let x = layout.column_x(n.column);
            let color = column_color(n.column);
            let head_y = if n.hit {
                layout.receptor_y
            } else {
                layout.receptor_y + (n.beat - self.t) as f32 * ppb
            };
            if n.hold > 0.0 {
                let tail_y = layout.receptor_y + (n.beat + n.hold - self.t) as f32 * ppb;
                let body = Rect::from_min_max(
                    pos2(x - layout.arrow * 0.26, head_y),
                    pos2(x + layout.arrow * 0.26, tail_y.max(head_y)),
                );
                clip.rect_filled(
                    body,
                    10,
                    color.gamma_multiply(if n.hit { 0.65 } else { 0.4 }),
                );
                clip.rect_stroke(body, 10, Stroke::new(1.5, color), egui::StrokeKind::Inside);
            }
            if head_y > lane_bg.bottom() + layout.arrow {
                continue;
            }
            let waiting = !n.hit && n.arrived.is_some();
            if waiting && !reduce {
                let pulse = 0.5 + 0.5 * (anim::since(n.arrived.unwrap_or(now), now) * 6.0).sin();
                notefield::draw_glow(
                    &clip,
                    pos2(x, head_y),
                    layout.arrow * 0.9,
                    color,
                    0.6 + 0.8 * pulse,
                );
            }
            notefield::draw_arrow(
                &clip,
                pos2(x, head_y),
                layout.arrow * 0.92,
                n.column,
                color,
                Stroke::new(2.0, Color32::WHITE.gamma_multiply(0.8)),
            );
        }

        // Hint bubble when an arrow is waiting.
        let waiting: Vec<u8> = self
            .notes
            .iter()
            .filter(|n| !n.hit && n.arrived.is_some())
            .map(|n| n.column)
            .collect();
        let holding = self.notes.iter().any(|n| n.hit && !n.done);
        let hint = if holding {
            Some("Tahan terus sampai ekornya habis…".to_string())
        } else if !waiting.is_empty() {
            let keys: Vec<String> = waiting.iter().map(|c| key_hint(shared, *c)).collect();
            Some(if keys.len() > 1 {
                format!("Tekan {} bersamaan", keys.join(" + "))
            } else {
                format!("Tekan {}", keys[0])
            })
        } else {
            None
        };
        let below = layout.receptor_y + layout.arrow * 1.05;
        if let Some(h) = hint {
            painter.text(
                pos2(lane_bg.center().x, below + 70.0),
                Align2::CENTER_CENTER,
                h,
                font_bold(theme::TEXT_M),
                palette.text,
            );
        }
        if self
            .wrong_at
            .iter()
            .flatten()
            .any(|w| anim::since(*w, now) < 1.0)
            && !holding
        {
            painter.text(
                pos2(lane_bg.center().x, below + 100.0),
                Align2::CENTER_CENTER,
                "Bukan itu — lihat warna dan arah panah yang datang",
                font(theme::TEXT_S),
                theme::DANGER,
            );
        }

        // Hit effects and the latest judgment word.
        self.effects
            .retain(|(_, at)| anim::since(*at, now) < HIT_EFFECT_LEN);
        for (fx, at) in &self.effects {
            let mut fx = *fx;
            fx.age = anim::since(*at, now);
            notefield::draw_hit_effect(&painter, &layout, &fx);
        }
        if let Some((word, color, at)) = self.feedback {
            let age = anim::since(at, now);
            if age < 0.7 {
                let pop = if reduce {
                    1.0
                } else {
                    1.0 + 0.3 * (1.0 - (age / 0.15).min(1.0))
                };
                let alpha = (1.0 - ((age - 0.45) / 0.25).max(0.0)).clamp(0.0, 1.0);
                widgets::text_tracked_scaled(
                    &painter,
                    pos2(lane_bg.center().x, below + 24.0),
                    Align2::CENTER_CENTER,
                    word,
                    font_bold(30.0),
                    color.gamma_multiply(alpha),
                    pop,
                );
            }
        }

        // Lesson complete.
        if let Some(done) = self.lesson_done {
            let a = anim::ease_out_cubic(anim::progress(done, now, 0.25, reduce));
            let card = Rect::from_center_size(lane_bg.center(), vec2(320.0, 110.0));
            painter.rect_filled(
                card,
                theme::RADIUS_CARD,
                palette.bg_top.gamma_multiply(0.95 * a),
            );
            painter.rect_stroke(
                card,
                theme::RADIUS_CARD,
                Stroke::new(2.0, SUCCESS.gamma_multiply(a)),
                egui::StrokeKind::Inside,
            );
            painter.text(
                card.center() - vec2(0.0, 14.0),
                Align2::CENTER_CENTER,
                "✓ Bagus sekali!",
                font_bold(theme::TEXT_L),
                SUCCESS.gamma_multiply(a),
            );
            painter.text(
                card.center() + vec2(0.0, 20.0),
                Align2::CENTER_CENTER,
                if self.lesson + 1 < lessons.len() {
                    "Lanjut ke pelajaran berikutnya…"
                } else {
                    "Tutorial selesai!"
                },
                font(theme::TEXT_S),
                palette.text_dim.gamma_multiply(a),
            );
        }
        None
    }

    fn finished_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        shared: &mut Shared,
        input: &FrameInput,
    ) -> Option<TutorialEvent> {
        let palette = shared.palette;
        let painter = ui.painter().clone();
        let now = input.now;
        let a = anim::ease_out_cubic(anim::progress(
            self.finished.unwrap_or(now),
            now,
            0.4,
            shared.settings.reduce_motion,
        ));
        let center = pos2(rect.center().x, rect.top() + rect.height() * 0.28);
        notefield::draw_glow(&painter, center, 120.0, ACCENT_A, 0.8 * a);
        widgets::text_tracked(
            &painter,
            center,
            Align2::CENTER_CENTER,
            "Kamu siap main!",
            font_bold(theme::TEXT_XL + 8.0),
            palette.text.gamma_multiply(a),
        );
        painter.text(
            center + vec2(0.0, 50.0),
            Align2::CENTER_CENTER,
            format!(
                "Pas: {}  •  Hampir pas: {}  •  Menunggu dulu: {}",
                self.counts[0], self.counts[1], self.counts[2]
            ),
            font(theme::TEXT_M),
            palette.text_dim.gamma_multiply(a),
        );
        painter.text(
            center + vec2(0.0, 80.0),
            Align2::CENTER_CENTER,
            "Di lagu sungguhan panah tidak menunggu, jadi tekan saat panah menutupi target.",
            font(theme::TEXT_S),
            palette.text_dim.gamma_multiply(a),
        );

        // `None` = play the tutorial again.
        let choices: Vec<(&str, Option<TutorialEvent>)> = if self.onboarding {
            vec![
                ("Lanjut", Some(TutorialEvent::Next)),
                ("Ulangi tutorial", None),
            ]
        } else {
            vec![
                ("Main sekarang", Some(TutorialEvent::Play)),
                ("Kalibrasi", Some(TutorialEvent::Calibrate)),
                ("Ulangi", None),
                ("Menu utama", Some(TutorialEvent::Menu)),
            ]
        };
        for action in &input.menu {
            match action {
                MenuAction::Left | MenuAction::Up => {
                    self.selected = (self.selected + choices.len() - 1) % choices.len();
                }
                MenuAction::Right | MenuAction::Down => {
                    self.selected = (self.selected + 1) % choices.len();
                }
                _ => {}
            }
        }
        let settled = anim::since(self.finished.unwrap_or(now), now) > 0.8;
        let w = 190.0;
        let total = w * choices.len() as f32 + 16.0 * (choices.len() as f32 - 1.0);
        let mut chosen = None;
        for (i, (label, event)) in choices.iter().enumerate() {
            let r = Rect::from_min_size(
                pos2(
                    rect.center().x - total / 2.0 + i as f32 * (w + 16.0),
                    center.y + 130.0,
                ),
                vec2(w, 50.0),
            );
            let resp = ui.put(r, |ui: &mut Ui| {
                if i == self.selected {
                    widgets::primary_button(ui, label, r.size(), true)
                } else {
                    widgets::secondary_button(ui, &palette, label, r.size())
                }
            });
            if resp.hovered() {
                self.selected = i;
            }
            if resp.clicked() || (settled && i == self.selected && input.has(MenuAction::Confirm)) {
                chosen = Some(*event);
            }
        }
        match chosen {
            Some(None) => {
                *self = Self::new(self.onboarding);
                None
            }
            Some(event) => event,
            None => None,
        }
    }
}

/// Short label for a column's key ("D / ←"), or the mat panel.
fn key_hint(shared: &Shared, column: u8) -> String {
    if shared.gamepad_name.is_some() {
        return ["Kiri", "Bawah", "Atas", "Kanan"][usize::from(column.min(3))].to_string();
    }
    let keys = &shared.settings.keybinds[usize::from(column.min(3))];
    let labels: Vec<String> = keys
        .iter()
        .take(2)
        .map(|k| match egui::Key::from_name(k) {
            Some(egui::Key::ArrowLeft) => "←".into(),
            Some(egui::Key::ArrowDown) => "↓".into(),
            Some(egui::Key::ArrowUp) => "↑".into(),
            Some(egui::Key::ArrowRight) => "→".into(),
            Some(key) => key.symbol_or_name().to_string(),
            None => k.clone(),
        })
        .collect();
    if labels.is_empty() {
        ["Kiri", "Bawah", "Atas", "Kanan"][usize::from(column.min(3))].to_string()
    } else {
        labels.join(" / ")
    }
}

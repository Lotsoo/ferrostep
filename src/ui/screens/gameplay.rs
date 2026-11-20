//! The gameplay screen: audio clock, input, judging and notefield drawing.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use eframe::egui::{self, Align2, Color32, Rect, Ui, pos2, vec2};

use super::{Nav, PlayRequest, ResultData, difficulty_label};
use crate::app::Shared;
use crate::audio::{AudioClock, Music};
use crate::config::Settings;
use crate::gameplay::{GameEvent, GameSession};
use crate::input::{FrameInput, MenuAction};
use crate::library::find_music;
use crate::timing::{TimingMap, time_notes};
use crate::ui::notefield::{self, HIT_EFFECT_LEN, HitEffect, Layout, Scroll};
use crate::ui::theme::{self, font, font_bold, judgment_color};
use crate::ui::{anim, widgets};

/// Song clock: monotonic lead-in, then the audio clock.
enum Phase {
    LeadIn { start: Instant, length: f64 },
    Playing { clock: AudioClock },
}

/// Pause menu entries.
const PAUSE_ITEMS: [&str; 3] = ["Lanjutkan", "Ulangi dari awal", "Keluar ke daftar lagu"];

struct Pause {
    selected: usize,
}

pub struct GameplayScreen {
    request: PlayRequest,
    music_path: PathBuf,
    title: String,
    subtitle: String,
    session: GameSession,
    timing: TimingMap,
    columns: u8,
    music: Option<Music>,
    phase: Phase,
    /// Last song time (seconds, offset already applied).
    song_time: f64,
    song_length: f64,
    pause: Option<Pause>,
    effects: Vec<(HitEffect, Instant)>,
    last_judgment: Option<(crate::gameplay::judge::Judgment, Instant)>,
    combo_bump: Instant,
    hit_counter: u32,
    tint: Color32,
}

impl GameplayScreen {
    /// Prepares a play. Fails with a friendly message if the chart or audio is unusable.
    pub fn new(request: PlayRequest, settings: &Settings) -> Result<Self> {
        let sim = &request.simfile;
        let chart = sim
            .charts
            .get(request.chart_index)
            .context("chart tidak ditemukan")?;
        let music_path = find_music(&request.song_dir, &sim.meta.music)
            .context("file audio lagu ini tidak ditemukan di folder lagunya")?;
        let timing = TimingMap::new(sim.timing_for(chart));
        let notes = time_notes(chart, &timing);
        if notes.is_empty() {
            bail!("chart ini tidak berisi panah");
        }
        let first = notes.first().map_or(0.0, |n| n.time);
        let lead_in = (2.5 - first).clamp(1.0, 3.0);
        let columns = chart.columns.min(4);
        let session = GameSession::new(notes, columns, settings.judge_scale);
        let song_length = session.end_time;
        Ok(Self {
            title: sim.meta.title.clone(),
            subtitle: format!(
                "{} · {} {}",
                sim.meta.artist,
                difficulty_label(chart.difficulty),
                chart.meter
            ),
            tint: widgets::color_from_hash(&sim.meta.title),
            request,
            music_path,
            session,
            timing,
            columns,
            music: None,
            phase: Phase::LeadIn {
                start: Instant::now(),
                length: lead_in,
            },
            song_time: -lead_in,
            song_length,
            pause: None,
            effects: Vec::new(),
            last_judgment: None,
            combo_bump: Instant::now(),
            hit_counter: 0,
        })
    }

    /// Current song time (seconds) at `now`, starting the music when the lead-in ends.
    fn update_clock(&mut self, shared: &mut Shared, now: Instant) -> Result<f64> {
        let offset = shared.settings.global_offset();
        match &mut self.phase {
            Phase::LeadIn { start, length } => {
                let t = now.saturating_duration_since(*start).as_secs_f64() - *length;
                if t >= 0.0 {
                    let audio = shared.audio.as_mut().context("tidak ada perangkat audio")?;
                    self.music = Some(audio.play_music(&self.music_path, 0.0, 0)?);
                    self.phase = Phase::Playing {
                        clock: AudioClock::new(0.0, now),
                    };
                    return Ok(-offset);
                }
                Ok(t - offset)
            }
            Phase::Playing { clock } => {
                let raw = self.music.as_ref().map_or(0.0, Music::position);
                Ok(clock.update(raw, now) - offset)
            }
        }
    }

    fn set_paused(&mut self, paused: bool, now: Instant) {
        if paused && self.pause.is_none() {
            self.pause = Some(Pause { selected: 0 });
            if let Some(m) = &mut self.music {
                m.pause();
            }
        } else if !paused && self.pause.is_some() {
            self.pause = None;
            if let Some(m) = &mut self.music {
                m.resume();
            }
            match &mut self.phase {
                Phase::Playing { clock } => {
                    let raw = self.music.as_ref().map_or(0.0, Music::position);
                    clock.reset(raw, now);
                }
                Phase::LeadIn { start, length } => {
                    // restart the remaining lead-in from now
                    *start = now;
                    *length = (-self.song_time).max(0.5);
                }
            }
        }
    }

    fn finish(&mut self, shared: &Shared) -> Nav {
        if let Some(m) = &mut self.music {
            m.fade_out(600);
        }
        let sim = &self.request.simfile;
        let chart = &sim.charts[self.request.chart_index];
        Nav::Results(Box::new(ResultData {
            request: self.request.clone(),
            title: sim.meta.title.clone(),
            artist: sim.meta.artist.clone(),
            difficulty: chart.difficulty,
            meter: chart.meter,
            score: self.session.score.clone(),
            no_fail: shared.settings.no_fail,
            song_length: self.song_length,
            previous_best: None,
        }))
    }

    fn handle_events(&mut self, now: Instant) {
        for event in self.session.events.drain(..) {
            match event {
                GameEvent::Hit {
                    column, judgment, ..
                } => {
                    self.hit_counter = self.hit_counter.wrapping_add(1);
                    self.effects.push((
                        HitEffect {
                            column,
                            color: judgment_color(judgment),
                            age: 0.0,
                            seed: self.hit_counter,
                        },
                        now,
                    ));
                    self.last_judgment = Some((judgment, now));
                    if judgment.keeps_combo() {
                        self.combo_bump = now;
                    }
                }
                GameEvent::Miss { .. } => {
                    self.last_judgment = Some((crate::gameplay::judge::Judgment::Miss, now));
                }
                GameEvent::HoldOk { column } => {
                    self.hit_counter = self.hit_counter.wrapping_add(1);
                    self.effects.push((
                        HitEffect {
                            column,
                            color: theme::SUCCESS,
                            age: 0.0,
                            seed: self.hit_counter,
                        },
                        now,
                    ));
                }
                GameEvent::HoldNg { .. } => {}
                GameEvent::MineHit { column } => {
                    self.effects.push((
                        HitEffect {
                            column,
                            color: theme::DANGER,
                            age: 0.0,
                            seed: self.hit_counter,
                        },
                        now,
                    ));
                }
            }
        }
        self.effects
            .retain(|(_, start)| anim::since(*start, now) < HIT_EFFECT_LEN);
    }

    pub fn ui(&mut self, ui: &mut Ui, shared: &mut Shared, input: &FrameInput) -> Option<Nav> {
        let now = input.now;
        ui.ctx().request_repaint();

        // Auto-pause when the window loses focus or a mat is unplugged.
        let focused = ui.ctx().input(|i| i.viewport().focused).unwrap_or(true);
        // (AutoPlay keeps running in the background: nobody is playing.)
        let lost_focus = !focused && !shared.autoplay;
        if (lost_focus || shared.gamepad_disconnected) && self.pause.is_none() {
            self.set_paused(true, now);
        }
        shared.gamepad_disconnected = false;

        if let Some(nav) = self.pause_input(input) {
            return Some(nav);
        }

        if self.pause.is_none() {
            match self.update_clock(shared, now) {
                Ok(t) => self.song_time = t,
                Err(e) => {
                    log::error!("{e:#}");
                    shared
                        .toasts
                        .warning(format!("Lagu tidak bisa diputar: {e}"));
                    return Some(Nav::SongSelect);
                }
            }
            for ev in &input.columns {
                // Shift each event back by how long ago it happened.
                let age = now.saturating_duration_since(ev.at).as_secs_f64();
                let t = self.song_time - age;
                if ev.column < self.columns {
                    if ev.pressed {
                        self.session.press(ev.column, t);
                    } else {
                        self.session.release(ev.column, t);
                    }
                }
            }
            if shared.autoplay {
                self.session.autoplay(self.song_time);
            }
            self.session.update(self.song_time);
            self.handle_events(now);

            let music_done = self.music.as_ref().is_some_and(Music::is_stopped);
            let failed = self.session.score.failed && !shared.settings.no_fail;
            if self.session.is_finished(self.song_time) || music_done || failed {
                return Some(self.finish(shared));
            }
        }

        self.draw(ui, shared, now);
        None
    }

    fn pause_input(&mut self, input: &FrameInput) -> Option<Nav> {
        if self.pause.is_none() {
            if input.has(MenuAction::Back) {
                self.set_paused(true, input.now);
            }
            return None;
        }
        let pause = self.pause.as_mut()?;
        for action in &input.menu {
            match action {
                MenuAction::Up => {
                    pause.selected = (pause.selected + PAUSE_ITEMS.len() - 1) % PAUSE_ITEMS.len()
                }
                MenuAction::Down => pause.selected = (pause.selected + 1) % PAUSE_ITEMS.len(),
                MenuAction::Back => {
                    self.set_paused(false, input.now);
                    return None;
                }
                MenuAction::Confirm => return self.pause_choose(input.now),
                _ => {}
            }
        }
        None
    }

    fn pause_choose(&mut self, now: Instant) -> Option<Nav> {
        let selected = self.pause.as_ref()?.selected;
        match selected {
            0 => {
                self.set_paused(false, now);
                None
            }
            1 => Some(Nav::Play(self.request.clone())),
            _ => Some(Nav::SongSelect),
        }
    }

    fn draw(&mut self, ui: &mut Ui, shared: &mut Shared, now: Instant) {
        let rect = ui.max_rect();
        let painter = ui.painter().clone();
        let palette = shared.palette;
        let reduce = shared.settings.reduce_motion;

        // Background: dark gradient tinted with the song color, dimmed by the brightness setting.
        widgets::vertical_gradient(
            &painter,
            rect,
            Color32::from_rgb(6, 6, 12),
            Color32::from_rgb(14, 14, 28),
        );
        let b = shared.settings.background_brightness.clamp(0.0, 1.0);
        widgets::background(
            &painter,
            rect,
            &theme::Palette {
                bg_top: Color32::TRANSPARENT,
                bg_bottom: Color32::TRANSPARENT,
                ..palette
            },
            if reduce {
                0.0
            } else {
                anim::since(shared.start, now)
            },
            Some(self.tint.gamma_multiply(0.3 + b)),
        );

        let layout = Layout::new(rect, self.columns, shared.settings.reverse);
        let field = Rect::from_x_y_ranges(
            layout.left - 12.0..=layout.left + layout.width(self.columns) + 12.0,
            rect.y_range(),
        );
        painter.rect_filled(field, 0, Color32::from_black_alpha(110));

        let beat = self.timing.seconds_to_beat(self.song_time);
        let pulse = if reduce {
            0.0
        } else {
            (1.0 - beat.rem_euclid(1.0) as f32).powi(4)
        };
        let pressed: Vec<bool> = (0..self.columns)
            .map(|c| self.session.is_pressed(c))
            .collect();
        notefield::draw_receptors(&painter, &layout, self.columns, &pressed, pulse, false);

        let scroll = Scroll::new(&shared.settings, &layout, &self.timing, self.song_time);
        let clipped = painter.with_clip_rect(rect);
        notefield::draw_notes(&clipped, &layout, &scroll, &self.session.notes);
        for (fx, start) in &self.effects {
            let fx = HitEffect {
                age: anim::since(*start, now),
                ..*fx
            };
            notefield::draw_hit_effect(&painter, &layout, &fx);
        }

        self.draw_judgment(&painter, &layout, rect, now, reduce);
        self.draw_hud(&painter, rect, shared);

        if self.song_time < 0.0 && self.pause.is_none() {
            let left = (-self.song_time).ceil() as i32;
            let frac = (-self.song_time).fract() as f32;
            let text = if left >= 1 {
                left.to_string()
            } else {
                String::new()
            };
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                text,
                font_bold(64.0 + frac * 20.0),
                Color32::WHITE.gamma_multiply(frac.clamp(0.2, 0.9)),
            );
        }
        if self.pause.is_some() {
            self.draw_pause(ui, shared);
        }
    }

    fn draw_judgment(
        &self,
        painter: &egui::Painter,
        layout: &Layout,
        rect: Rect,
        now: Instant,
        reduce: bool,
    ) {
        let Some((judgment, at)) = self.last_judgment else {
            return;
        };
        let age = anim::since(at, now);
        if age > 0.8 {
            return;
        }
        let pop = if reduce {
            1.0
        } else {
            1.0 + 0.3 * (1.0 - anim::ease_out_cubic(age / 0.12))
        };
        let alpha = ((0.8 - age) / 0.2).clamp(0.0, 1.0);
        let center_y = if layout.dir > 0.0 {
            rect.center().y + rect.height() * 0.05
        } else {
            rect.center().y - rect.height() * 0.1
        };
        let pos = pos2(rect.center().x, center_y);
        let color = judgment_color(judgment);
        painter.text(
            pos,
            Align2::CENTER_CENTER,
            judgment.label(),
            font_bold(34.0 * pop),
            color.gamma_multiply(alpha),
        );

        let combo = self.session.score.combo;
        if combo >= 4 {
            let bump = if reduce {
                1.0
            } else {
                1.0 + 0.12 * (1.0 - anim::ease_out_cubic(anim::since(self.combo_bump, now) / 0.15))
            };
            widgets::tabular_text(
                painter,
                pos + vec2(0.0, 48.0),
                Align2::CENTER_CENTER,
                &combo.to_string(),
                font_bold(44.0 * bump),
                Color32::WHITE.gamma_multiply(0.85),
            );
            painter.text(
                pos + vec2(0.0, 80.0),
                Align2::CENTER_CENTER,
                "COMBO",
                font(theme::TEXT_XS),
                Color32::WHITE.gamma_multiply(0.5),
            );
        }
    }

    fn draw_hud(&self, painter: &egui::Painter, rect: Rect, shared: &Shared) {
        let palette = &shared.palette;
        // song progress
        let progress = (self.song_time / self.song_length.max(1.0)).clamp(0.0, 1.0) as f32;
        let bar = Rect::from_min_size(rect.left_top() + vec2(0.0, 0.0), vec2(rect.width(), 4.0));
        widgets::progress_bar(painter, bar, progress, palette);
        self.draw_score_bar(painter, rect);

        self.draw_life_bar(painter, rect, shared.settings.no_fail);
        if shared.autoplay {
            painter.text(
                rect.center_bottom() - vec2(0.0, 24.0),
                Align2::CENTER_CENTER,
                "AUTOPLAY",
                font_bold(theme::TEXT_S),
                theme::ACCENT_B.gamma_multiply(0.8),
            );
        }
        painter.text(
            rect.left_top() + vec2(24.0, 58.0),
            Align2::LEFT_TOP,
            &self.title,
            font_bold(theme::TEXT_M),
            Color32::WHITE.gamma_multiply(0.8),
        );
        painter.text(
            rect.left_top() + vec2(24.0, 82.0),
            Align2::LEFT_TOP,
            &self.subtitle,
            font(theme::TEXT_S),
            Color32::WHITE.gamma_multiply(0.5),
        );

        widgets::tabular_text(
            painter,
            rect.right_top() + vec2(-24.0, 16.0),
            Align2::RIGHT_TOP,
            &format!("{:.2}%", self.session.score.accuracy_percent()),
            font_bold(theme::TEXT_L),
            Color32::WHITE.gamma_multiply(0.85),
        );

        if shared.settings.show_fps {
            let dt = painter.ctx().input(|i| i.unstable_dt).max(1e-4);
            painter.text(
                rect.right_bottom() - vec2(16.0, 16.0),
                Align2::RIGHT_BOTTOM,
                format!("{:.0} FPS", 1.0 / dt),
                font(theme::TEXT_XS),
                Color32::WHITE.gamma_multiply(0.6),
            );
        }
    }

    /// StepMania-style life bar in the top-left corner: segmented, red → yellow →
    /// green, pulsing red when nearly empty. Rises on good steps, drops on misses.
    fn draw_life_bar(&self, painter: &egui::Painter, rect: Rect, no_fail: bool) {
        const SEGMENTS: usize = 24;
        let score = &self.session.score;
        let life = score.life.clamp(0.0, 1.0);
        let frame = Rect::from_min_size(rect.left_top() + vec2(24.0, 16.0), vec2(300.0, 22.0));
        painter.text(
            frame.left_center() + vec2(0.0, 0.0),
            Align2::LEFT_CENTER,
            "LIFE",
            font_bold(theme::TEXT_XS),
            Color32::WHITE.gamma_multiply(0.7),
        );
        let bar = Rect::from_min_max(frame.min + vec2(40.0, 0.0), frame.max);
        let danger = life < 0.25;
        let t = painter.ctx().input(|i| i.time) as f32;
        let pulse = if danger {
            0.5 + 0.5 * (t * 8.0).sin()
        } else {
            0.0
        };
        painter.rect_filled(bar.expand(3.0), 6, Color32::from_black_alpha(120));
        painter.rect_stroke(
            bar.expand(3.0),
            6,
            egui::Stroke::new(
                1.0,
                theme::DANGER
                    .gamma_multiply(0.2 + 0.6 * pulse)
                    .lerp_to_gamma(Color32::from_white_alpha(40), 1.0 - pulse),
            ),
            egui::StrokeKind::Inside,
        );
        let yellow = Color32::from_rgb(0xFF, 0xE4, 0x5C);
        let color = if life < 0.5 {
            theme::DANGER.lerp_to_gamma(yellow, life / 0.5)
        } else {
            yellow.lerp_to_gamma(theme::SUCCESS, (life - 0.5) / 0.5)
        };
        let gap = 2.0;
        let seg_w = (bar.width() - gap * (SEGMENTS as f32 - 1.0)) / SEGMENTS as f32;
        for i in 0..SEGMENTS {
            let x = bar.left() + i as f32 * (seg_w + gap);
            let seg = Rect::from_min_size(pos2(x, bar.top()), vec2(seg_w, bar.height()));
            painter.rect_filled(seg, 2, Color32::from_white_alpha(14));
            // How much of this segment is filled (0..=1).
            let fill = (life * SEGMENTS as f32 - i as f32).clamp(0.0, 1.0);
            if fill > 0.0 {
                let r = Rect::from_min_size(seg.min, vec2(seg_w * fill, seg.height()));
                let shade = if i == SEGMENTS - 1 || life >= 1.0 {
                    color.lerp_to_gamma(Color32::WHITE, 0.25)
                } else {
                    color
                };
                painter.rect_filled(r, 2, shade.gamma_multiply(0.75 + 0.25 * pulse));
            }
        }
        if no_fail {
            painter.text(
                bar.right_center() + vec2(12.0, 0.0),
                Align2::LEFT_CENTER,
                if score.failed {
                    "No Fail · habis"
                } else {
                    "No Fail"
                },
                font(theme::TEXT_XS),
                Color32::WHITE.gamma_multiply(0.45),
            );
        }
    }

    /// Score bar under the song progress: bright = points earned, dim = the best
    /// percentage still reachable, ticks = grade thresholds.
    fn draw_score_bar(&self, painter: &egui::Painter, rect: Rect) {
        // Top-right, under the accuracy; mirrors the life bar on the left.
        let bar = Rect::from_min_size(
            pos2(rect.right() - 24.0 - 300.0, rect.top() + 62.0),
            vec2(300.0, 10.0),
        );
        let score = &self.session.score;
        let earned = (score.percent() / 100.0) as f32;
        let possible = (score.max_possible_percent() / 100.0) as f32;
        painter.text(
            bar.left_center() - vec2(10.0, 0.0),
            Align2::RIGHT_CENTER,
            "SKOR",
            font_bold(theme::TEXT_XS),
            Color32::WHITE.gamma_multiply(0.7),
        );
        painter.rect_filled(
            bar.expand(2.0),
            theme::RADIUS_PILL,
            Color32::from_black_alpha(120),
        );
        painter.rect_filled(bar, theme::RADIUS_PILL, Color32::from_white_alpha(10));
        let part = |v: f32| {
            Rect::from_min_size(bar.min, vec2(bar.width() * v.clamp(0.0, 1.0), bar.height()))
        };
        // Still reachable: a clearly visible, softer version of the accent.
        painter.rect_filled(
            part(possible),
            theme::RADIUS_PILL,
            theme::ACCENT_B.gamma_multiply(0.22),
        );
        if earned > 0.0 {
            widgets::gradient_pill(painter, part(earned), theme::ACCENT_A, theme::ACCENT_B);
        }
        // Grade thresholds; S and SS are close, so SS is labelled above the bar.
        for (pct, grade, above) in [
            (0.8, "B", false),
            (0.9, "A", false),
            (0.95, "S", false),
            (0.99, "SS", true),
        ] {
            let x = bar.left() + bar.width() * pct;
            let reachable = possible >= pct;
            let color = Color32::WHITE.gamma_multiply(if reachable { 0.6 } else { 0.18 });
            painter.line_segment(
                [pos2(x, bar.top() - 2.0), pos2(x, bar.bottom() + 2.0)],
                egui::Stroke::new(1.0, color),
            );
            let (pos, align) = if above {
                (pos2(x, bar.top() - 4.0), Align2::CENTER_BOTTOM)
            } else {
                (pos2(x, bar.bottom() + 4.0), Align2::CENTER_TOP)
            };
            painter.text(pos, align, grade, font(theme::TEXT_XS - 2.0), color);
        }
    }

    fn draw_pause(&mut self, ui: &mut Ui, shared: &mut Shared) {
        let rect = ui.max_rect();
        let painter = ui.painter().clone();
        painter.rect_filled(rect, 0, Color32::from_black_alpha(170));
        let palette = shared.palette;
        let card = Rect::from_center_size(rect.center(), vec2(380.0, 300.0));
        widgets::card(&painter, card, &palette);
        painter.text(
            card.center_top() + vec2(0.0, 40.0),
            Align2::CENTER_CENTER,
            "Dijeda",
            font_bold(theme::TEXT_L),
            palette.text,
        );

        let mut clicked = None;
        let selected = self.pause.as_ref().map_or(0, |p| p.selected);
        for (i, label) in PAUSE_ITEMS.iter().enumerate() {
            let r = Rect::from_center_size(
                card.center_top() + vec2(0.0, 100.0 + i as f32 * 60.0),
                vec2(300.0, 48.0),
            );
            let resp = ui.put(r, |ui: &mut Ui| {
                if i == selected {
                    widgets::primary_button(ui, label, r.size(), true)
                } else {
                    widgets::secondary_button(ui, &palette, label, r.size())
                }
            });
            if resp.clicked() {
                clicked = Some(i);
            }
            if resp.hovered()
                && let Some(p) = &mut self.pause
            {
                p.selected = i;
            }
        }
        painter.text(
            card.center_bottom() - vec2(0.0, 20.0),
            Align2::CENTER_CENTER,
            "Esc untuk melanjutkan",
            font(theme::TEXT_XS),
            palette.text_dim,
        );
        if let Some(i) = clicked {
            if let Some(p) = &mut self.pause {
                p.selected = i;
            }
            shared.pending_nav = self.pause_choose(Instant::now());
        }
    }
}

//! The gameplay screen: audio clock, input, judging and notefield drawing.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use eframe::egui::{self, Align2, Color32, Rect, Stroke, Ui, pos2, vec2};

use super::{Nav, PlayRequest, ResultData, difficulty_color, difficulty_label};
use crate::app::Shared;
use crate::audio::{AudioClock, Music};
use crate::config::Settings;
use crate::gameplay::judge::Judgment;
use crate::gameplay::{GameEvent, GameSession};
use crate::input::{FrameInput, MenuAction};
use crate::library::find_music;
use crate::timing::{TimingMap, time_notes};
use crate::ui::hud::{self, HudAnim, SongInfo, Stage};
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
    difficulty_color: Color32,
    hud: HudAnim,
    last_combo: u32,
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
            subtitle: {
                let level = format!("{} {}", difficulty_label(chart.difficulty), chart.meter);
                if sim.meta.artist.trim().is_empty() {
                    level
                } else {
                    format!("{} · {level}", sim.meta.artist)
                }
            },
            tint: widgets::color_from_hash(&sim.meta.title),
            difficulty_color: difficulty_color(chart.difficulty),
            hud: HudAnim::default(),
            last_combo: 0,
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
                    column,
                    judgment,
                    offset,
                } => {
                    self.hud.record_hit(judgment, offset, now);
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
                    self.last_judgment = Some((Judgment::Miss, now));
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
        let combo = self.session.score.combo;
        if combo != self.last_combo {
            self.hud.combo_changed(combo, now);
            self.last_combo = combo;
        }
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
        let reduce = shared.settings.reduce_motion;
        let dt = ui.input(|i| i.stable_dt);
        let time = anim::since(shared.start, now);

        let layout = Layout::new(rect, self.columns, shared.settings.reverse);
        let beat = self.timing.seconds_to_beat(self.song_time);
        let pulse = if reduce {
            0.0
        } else {
            (1.0 - beat.rem_euclid(1.0) as f32).powi(4)
        };
        let pressed: Vec<bool> = (0..self.columns)
            .map(|c| self.session.is_pressed(c))
            .collect();
        if self.pause.is_none() {
            self.hud.update(&self.session.score, &pressed, dt, reduce);
        }

        let stage = Stage {
            rect,
            layout: &layout,
            columns: self.columns,
            tint: self.tint,
            brightness: shared.settings.background_brightness.clamp(0.0, 1.0),
            energy: self.hud.energy(),
            pulse,
            time,
            reduce,
        };
        let scroll = Scroll::new(&shared.settings, &layout, &self.timing, self.song_time);
        hud::draw_stage(&painter, &stage);
        hud::draw_lane(&painter, &stage, &scroll, &self.timing, &self.hud);

        notefield::draw_receptors(&painter, &layout, self.columns, &pressed, pulse, false);
        let clipped = painter.with_clip_rect(rect);
        notefield::draw_notes(&clipped, &layout, &scroll, &self.session.notes);
        for (fx, start) in &self.effects {
            let fx = HitEffect {
                age: anim::since(*start, now),
                ..*fx
            };
            notefield::draw_hit_effect(&painter, &layout, &fx);
        }

        let feedback = self.feedback_anchor(&layout, rect);
        self.draw_judgment(&painter, feedback, now, reduce);
        hud::draw_hit_meter(
            &painter,
            feedback + vec2(0.0, 132.0),
            &self.hud,
            shared.settings.judge_scale,
            now,
        );
        self.draw_hud(&painter, &stage, shared);

        if self.song_time < 0.0 && self.pause.is_none() {
            hud::draw_countdown(&painter, rect.center(), -self.song_time, reduce);
        } else if self.song_time < 0.8 && self.pause.is_none() {
            let a = 1.0 - (self.song_time as f32 / 0.8);
            widgets::text_tracked(
                &painter,
                rect.center(),
                Align2::CENTER_CENTER,
                "Mulai!",
                font_bold(48.0 + (1.0 - a) * 12.0),
                Color32::WHITE.gamma_multiply(a),
            );
        }
        if self.pause.is_some() {
            self.draw_pause(ui, shared);
        }
    }

    /// Where judgment and combo appear: middle of the lane, away from the receptors.
    fn feedback_anchor(&self, layout: &Layout, rect: Rect) -> egui::Pos2 {
        let y = if layout.dir > 0.0 {
            rect.center().y + rect.height() * 0.02
        } else {
            rect.center().y - rect.height() * 0.18
        };
        pos2(layout.left + layout.width(self.columns) / 2.0, y)
    }

    fn draw_judgment(&self, painter: &egui::Painter, pos: egui::Pos2, now: Instant, reduce: bool) {
        let score = &self.session.score;
        if let Some((judgment, at)) = self.last_judgment {
            let age = anim::since(at, now);
            if age <= 0.8 {
                // Pop in with a quick overshoot, then fade.
                let pop = if reduce {
                    1.0
                } else {
                    1.0 + 0.28 * (1.0 - anim::ease_out_cubic(age / 0.14))
                };
                let alpha = ((0.8 - age) / 0.25).clamp(0.0, 1.0);
                let mut color = judgment_color(judgment);
                if judgment == Judgment::Marvelous && !reduce {
                    let shimmer = 0.5 + 0.5 * (anim::since(at, now) * 14.0).sin();
                    color = color.lerp_to_gamma(Color32::WHITE, shimmer * 0.6);
                }
                notefield::draw_glow(painter, pos, 46.0 * pop, color, alpha * 0.9);
                widgets::text_tracked(
                    painter,
                    pos,
                    Align2::CENTER_CENTER,
                    judgment.label(),
                    font_bold(36.0 * pop),
                    color.gamma_multiply(alpha),
                );
                // Early/late hint only when it matters (Great and worse).
                if let Some((_, ms)) = score.offsets.last().filter(|_| {
                    matches!(judgment, Judgment::Great | Judgment::Good | Judgment::Boo)
                }) {
                    let (word, c) = if *ms < 0.0 {
                        ("CEPAT", theme::ACCENT_B)
                    } else {
                        ("LAMBAT", Color32::from_rgb(0xFF, 0x9F, 0x5C))
                    };
                    widgets::text_tracked(
                        painter,
                        pos + vec2(0.0, -32.0),
                        Align2::CENTER_CENTER,
                        &format!("{word}  {:.0} ms", ms.abs()),
                        font_bold(11.0),
                        c.gamma_multiply(alpha),
                    );
                }
            }
        }

        let combo = score.combo;
        if combo >= 4 {
            let bump = if reduce {
                1.0
            } else {
                1.0 + 0.14 * (1.0 - anim::ease_out_cubic(anim::since(self.combo_bump, now) / 0.16))
            };
            // Full combo so far (no Good/Boo/Miss): the number turns gold.
            let full_combo = score.counts[Judgment::Good.index()]
                + score.counts[Judgment::Boo.index()]
                + score.counts[Judgment::Miss.index()]
                == 0;
            let color = if full_combo {
                Color32::from_rgb(0xFF, 0xE9, 0xA8)
            } else {
                Color32::WHITE.gamma_multiply(0.9)
            };
            let center = pos + vec2(0.0, 58.0);
            if let Some((_, age)) = self.hud.milestone(now).filter(|_| !reduce) {
                let e = anim::ease_out_cubic((age / 0.9).min(1.0));
                let fade = (1.0 - age / 0.9).max(0.0);
                painter.circle_stroke(
                    center,
                    30.0 + e * 120.0,
                    Stroke::new(3.0 * fade, theme::ACCENT_B.gamma_multiply(fade)),
                );
                notefield::draw_glow(painter, center, 60.0, theme::ACCENT_A, fade * 1.5);
            }
            widgets::tabular_text(
                painter,
                center,
                Align2::CENTER_CENTER,
                &combo.to_string(),
                font_bold(50.0 * bump),
                color,
            );
            let label_pos = center + vec2(0.0, 36.0);
            match self.hud.milestone(now) {
                Some((c, age)) => {
                    let a = (1.4 - age).clamp(0.0, 1.0);
                    widgets::text_tracked(
                        painter,
                        label_pos,
                        Align2::CENTER_CENTER,
                        hud::milestone_word(c),
                        font_bold(theme::TEXT_S),
                        theme::ACCENT_B
                            .lerp_to_gamma(Color32::WHITE, 0.3)
                            .gamma_multiply(a),
                    );
                }
                None => {
                    widgets::text_tracked(
                        painter,
                        label_pos,
                        Align2::CENTER_CENTER,
                        "COMBO",
                        font_bold(11.0),
                        color.gamma_multiply(0.55),
                    );
                }
            }
        }
    }

    fn draw_hud(&self, painter: &egui::Painter, stage: &Stage, shared: &Shared) {
        let rect = stage.rect;
        hud::draw_song_island(
            painter,
            rect,
            &SongInfo {
                title: &self.title,
                subtitle: &self.subtitle,
                difficulty_color: self.difficulty_color,
                tint: self.tint,
                time: self.song_time,
                length: self.song_length,
            },
        );
        let failed = self.session.score.failed && !shared.settings.no_fail;
        hud::draw_score_island(painter, rect, &self.session.score, &self.hud, failed);
        hud::draw_life_gauge(
            painter,
            stage,
            &self.hud,
            shared.settings.no_fail,
            self.session.score.failed,
        );
        if shared.autoplay {
            let pill = Rect::from_min_size(rect.left_top() + vec2(24.0, 112.0), vec2(96.0, 24.0));
            painter.rect_filled(
                pill,
                theme::RADIUS_PILL,
                theme::ACCENT_B.gamma_multiply(0.15),
            );
            widgets::text_tracked(
                painter,
                pill.center(),
                Align2::CENTER_CENTER,
                "AUTOPLAY",
                font_bold(11.0),
                theme::ACCENT_B,
            );
        }
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

    fn draw_pause(&mut self, ui: &mut Ui, shared: &mut Shared) {
        let rect = ui.max_rect();
        let painter = ui.painter().clone();
        painter.rect_filled(rect, 0, Color32::from_black_alpha(170));
        let palette = shared.palette;
        let card = Rect::from_center_size(rect.center(), vec2(380.0, 300.0));
        widgets::card(&painter, card, &palette);
        widgets::text_tracked(
            &painter,
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

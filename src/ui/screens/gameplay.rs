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
    /// "3, 2, 1" after unpausing: music waits until it ends (start time).
    resuming: Option<Instant>,
    /// Last resume-count number that beeped.
    resume_beeped: i32,
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
    /// Last countdown number that played its sound (4 = none yet, 0 = "go").
    count_played: i32,
    /// Last milestone that played its sound (only every 100 combo).
    milestone_sounded: u32,
}

/// Width reserved on each side for the HUD cards (24 px margin + 330 px card + gap).
const HUD_SIDE: f32 = 372.0;

impl GameplayScreen {
    /// True while the pause menu is open.
    pub fn is_paused(&self) -> bool {
        self.pause.is_some()
    }

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
        let lead_in = hud::LEAD_IN;
        let columns = chart.columns.min(4);
        let session = GameSession::new(notes, columns, settings.judge_scale);
        let song_length = session.end_time;
        Ok(Self {
            resuming: None,
            resume_beeped: 4,
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
            count_played: 4,
            milestone_sounded: 0,
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

    /// Pauses right away (e.g. when the quit dialog opens).
    pub fn pause_now(&mut self) {
        self.set_paused(true, Instant::now());
    }

    fn set_paused(&mut self, paused: bool, now: Instant) {
        if paused && self.pause.is_none() {
            // Any pause cancels a running resume countdown (it restarts later).
            self.resuming = None;
            self.pause = Some(Pause { selected: 0 });
            if let Some(m) = &mut self.music {
                m.pause();
            }
        } else if !paused && self.pause.is_some() {
            self.pause = None;
            if matches!(self.phase, Phase::Playing { .. }) && self.resuming.is_none() {
                // Give the player 3 seconds to get ready again.
                self.resuming = Some(now);
                self.resume_beeped = 4;
                return;
            }
            self.resume_now(now);
        }
    }

    /// Length of the "3, 2, 1" after unpausing.
    const RESUME_COUNT: f32 = 3.0;

    /// Actually continues the music and the clock.
    fn resume_now(&mut self, now: Instant) {
        self.resuming = None;
        {
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

        if let Some(start) = self.resuming
            && self.pause.is_none()
        {
            let elapsed = anim::since(start, now);
            let n = (Self::RESUME_COUNT - elapsed).ceil() as i32;
            if n < self.resume_beeped && n > 0 {
                self.resume_beeped = n;
                shared.sfx(crate::audio::Sfx::Count);
            }
            if elapsed >= Self::RESUME_COUNT {
                self.resume_now(now);
            } else {
                self.draw(ui, shared, now);
                return None;
            }
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
            self.countdown_sounds(shared);
            self.handle_events(now);
            let combo = self.session.score.combo;
            if combo < self.milestone_sounded {
                self.milestone_sounded = 0;
            }
            if let Some((c, _)) = self.hud.milestone(now)
                && c.is_multiple_of(100)
                && c > self.milestone_sounded
            {
                self.milestone_sounded = c;
                shared.sfx(crate::audio::Sfx::Success);
            }

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

        let scale = shared
            .settings
            .lane_size
            .scale(shared.gamepad_name.is_some());
        // Keep a big lane clear of the song card (left) and score card (right).
        let layout = Layout::new(rect, self.columns, shared.settings.reverse, scale)
            .limit_width(self.columns, rect.width() - 2.0 * HUD_SIDE);
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
            let failed = self.session.score.failed && !shared.settings.no_fail;
            self.hud
                .update(&self.session.score, failed, &pressed, dt, reduce);
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
            beat,
        };
        let scroll = Scroll::new(&shared.settings, &layout, &self.timing, self.song_time);
        hud::draw_stage(&painter, &stage);
        hud::draw_lane(&painter, &stage, &scroll, &self.timing, &self.hud);
        if let Some((_, age)) = self.hud.milestone(now).filter(|_| !reduce) {
            hud::draw_milestone_surge(&painter, &stage, age);
        }

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
        self.draw_judgment(&painter, feedback, layout.scale(), now, reduce);
        hud::draw_hit_meter(
            &painter,
            feedback + vec2(0.0, 132.0 * layout.scale()),
            &self.hud,
            shared.settings.judge_scale,
            now,
        );
        self.draw_hud(&painter, &stage, shared);
        if self.pause.is_none() && self.pause_button(ui, rect) {
            self.set_paused(true, now);
        }

        if let Some(start) = self.resuming
            && self.pause.is_none()
        {
            painter.rect_filled(rect, 0, Color32::from_black_alpha(110));
            let t = f64::from(anim::since(start, now) - Self::RESUME_COUNT);
            hud::draw_countdown(
                &painter,
                rect,
                t.min(-0.001),
                &hud::CountdownInfo {
                    title: &self.title,
                    subtitle: &self.subtitle,
                    tint: self.tint,
                },
                reduce,
            );
            ui.ctx().request_repaint();
        } else if self.song_time < 1.0 && self.pause.is_none() {
            hud::draw_countdown(
                &painter,
                rect,
                self.song_time,
                &hud::CountdownInfo {
                    title: &self.title,
                    subtitle: &self.subtitle,
                    tint: self.tint,
                },
                reduce,
            );
        }
        if self.pause.is_some() {
            self.draw_pause(ui, shared);
        }
    }

    /// Plays a beep on 3, 2, 1 and a big chord when the music starts.
    fn countdown_sounds(&mut self, shared: &mut Shared) {
        let step = if self.song_time >= 0.0 {
            0
        } else {
            (-self.song_time).ceil() as i32
        };
        if step <= 3 && step < self.count_played {
            self.count_played = step;
            shared.sfx(if step == 0 {
                crate::audio::Sfx::Go
            } else {
                crate::audio::Sfx::Count
            });
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

    /// Judgment word and combo; `k` is the lane scale (bigger lane, bigger text).
    fn draw_judgment(
        &self,
        painter: &egui::Painter,
        pos: egui::Pos2,
        k: f32,
        now: Instant,
        reduce: bool,
    ) {
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
                notefield::draw_glow(painter, pos, 46.0 * k * pop, color, alpha * 0.9);
                widgets::text_tracked_scaled(
                    painter,
                    pos,
                    Align2::CENTER_CENTER,
                    judgment.label(),
                    font_bold(36.0 * k),
                    color.gamma_multiply(alpha),
                    pop,
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
                        pos + vec2(0.0, -32.0 * k),
                        Align2::CENTER_CENTER,
                        &format!("{word}  {:.0} ms", ms.abs()),
                        font_bold(11.0 * k),
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
            let center = pos + vec2(0.0, 58.0 * k);
            if let Some((_, age)) = self.hud.milestone(now).filter(|_| !reduce) {
                let e = anim::ease_out_cubic((age / 0.9).min(1.0));
                let fade = (1.0 - age / 0.9).max(0.0);
                painter.circle_stroke(
                    center,
                    (30.0 + e * 120.0) * k,
                    Stroke::new(3.0 * fade, theme::ACCENT_B.gamma_multiply(fade)),
                );
                notefield::draw_glow(painter, center, 60.0 * k, theme::ACCENT_A, fade * 1.5);
            }
            widgets::tabular_text_scaled(
                painter,
                center,
                Align2::CENTER_CENTER,
                &combo.to_string(),
                font_bold(50.0 * k),
                color,
                bump,
            );
            let label_pos = center + vec2(0.0, 36.0 * k);
            match self.hud.milestone(now) {
                Some((c, age)) => {
                    let a = (1.4 - age).clamp(0.0, 1.0);
                    widgets::text_tracked(
                        painter,
                        label_pos,
                        Align2::CENTER_CENTER,
                        hud::milestone_word(c),
                        font_bold(theme::TEXT_S * k),
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
                        font_bold(11.0 * k),
                        color.gamma_multiply(0.55),
                    );
                }
            }
        }
    }

    /// A pause button under the song card, for mouse players (Esc works too).
    fn pause_button(&self, ui: &mut Ui, rect: Rect) -> bool {
        let r = Rect::from_min_size(rect.left_top() + vec2(24.0, 112.0), vec2(96.0, 32.0));
        let resp = ui
            .interact(r, ui.scope_id().with("pause-button"), egui::Sense::click())
            .on_hover_text("Jeda (Esc)");
        let hover = ui.ctx().animate_bool(resp.id, resp.hovered());
        let painter = ui.painter();
        let r = if resp.is_pointer_button_down_on() {
            Rect::from_center_size(r.center(), r.size() * 0.96)
        } else {
            r
        };
        hud::glass(painter, r);
        painter.rect_filled(
            r,
            theme::RADIUS_CARD,
            Color32::WHITE.gamma_multiply(0.06 * hover),
        );
        let icon = pos2(r.left() + 22.0, r.center().y);
        for dx in [-4.0, 4.0] {
            painter.rect_filled(
                Rect::from_center_size(icon + vec2(dx, 0.0), vec2(4.0, 13.0)),
                1,
                Color32::WHITE.gamma_multiply(0.85),
            );
        }
        painter.text(
            pos2(r.left() + 38.0, r.center().y),
            Align2::LEFT_CENTER,
            "Jeda",
            font_bold(theme::TEXT_S),
            Color32::WHITE.gamma_multiply(0.85 + 0.15 * hover),
        );
        resp.clicked()
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
            let pill = Rect::from_min_size(rect.left_top() + vec2(132.0, 116.0), vec2(96.0, 24.0));
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
        let card = Rect::from_center_size(rect.center(), vec2(420.0, 470.0));
        crate::ui::notefield::radial_light(&painter, card.center(), 420.0, self.tint, 0.25);
        widgets::card(&painter, card, &palette);
        // Four column colors across the top: this is a rhythm game.
        let stripe_w = (card.width() - 48.0) / 4.0;
        for c in 0..4u8 {
            let r = Rect::from_min_size(
                card.left_top() + vec2(24.0 + f32::from(c) * stripe_w, 0.0),
                vec2(stripe_w - 6.0, 4.0),
            );
            painter.rect_filled(r, 2, theme::column_color(c));
        }
        widgets::text_tracked(
            &painter,
            card.center_top() + vec2(0.0, 40.0),
            Align2::CENTER_CENTER,
            "D I J E D A",
            font_bold(theme::TEXT_L),
            palette.text,
        );
        painter.text(
            card.center_top() + vec2(0.0, 74.0),
            Align2::CENTER_CENTER,
            widgets::truncate(&self.title, 34),
            font_bold(theme::TEXT_M),
            palette.text,
        );
        painter.text(
            card.center_top() + vec2(0.0, 98.0),
            Align2::CENTER_CENTER,
            &self.subtitle,
            font(theme::TEXT_S),
            self.difficulty_color,
        );
        // Song progress + live stats.
        let progress = (self.song_time / self.song_length.max(0.001)).clamp(0.0, 1.0) as f32;
        let bar = Rect::from_min_size(
            card.left_top() + vec2(40.0, 124.0),
            vec2(card.width() - 80.0, 6.0),
        );
        painter.rect_filled(bar, 3, palette.border);
        widgets::gradient_pill(
            &painter,
            Rect::from_min_size(
                bar.min,
                vec2((bar.width() * progress).max(6.0), bar.height()),
            ),
            theme::ACCENT_A,
            theme::ACCENT_B,
        );
        let score = &self.session.score;
        let stats = [
            ("Progres", format!("{:.0}%", progress * 100.0)),
            ("Akurasi", format!("{:.2}%", score.accuracy_percent())),
            ("Combo", score.combo.to_string()),
        ];
        let col_w = bar.width() / 3.0;
        for (i, (label, value)) in stats.iter().enumerate() {
            let x = bar.left() + col_w * (i as f32 + 0.5);
            painter.text(
                pos2(x, bar.bottom() + 14.0),
                Align2::CENTER_TOP,
                *label,
                font(theme::TEXT_XS),
                palette.text_dim,
            );
            widgets::tabular_text(
                &painter,
                pos2(x, bar.bottom() + 32.0),
                Align2::CENTER_TOP,
                value,
                font_bold(theme::TEXT_M),
                palette.text,
            );
        }

        let mut clicked = None;
        let selected = self.pause.as_ref().map_or(0, |p| p.selected);
        for (i, label) in PAUSE_ITEMS.iter().enumerate() {
            let r = Rect::from_center_size(
                card.center_top() + vec2(0.0, 250.0 + i as f32 * 60.0),
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
        widgets::key_hints(
            &painter,
            card.center_bottom() + vec2(0.0, 36.0),
            Align2::CENTER_CENTER,
            &[("↑ ↓", "Pilih"), ("Enter", "OK"), ("Esc", "Lanjutkan")],
            &palette,
        );
        if let Some(i) = clicked {
            if let Some(p) = &mut self.pause {
                p.selected = i;
            }
            shared.pending_nav = self.pause_choose(Instant::now());
        }
    }
}

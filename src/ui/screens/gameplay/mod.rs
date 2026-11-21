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

mod view;

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
}

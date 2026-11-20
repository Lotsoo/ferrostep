//! Top-level application: screen state machine, shared state and transitions.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use eframe::egui::{self, Color32, UiBuilder, vec2};

use crate::audio::Audio;
use crate::config::Settings;
use crate::input::{FrameInput, InputHub};
use crate::library::scores::{BestScore, ScoreStore, chart_key};
use crate::library::{songs_dir, test_song};
use crate::parser::load_simfile;
use crate::ui::screens::gameplay::GameplayScreen;
use crate::ui::screens::results::ResultsScreen;
use crate::ui::screens::title::TitleScreen;
use crate::ui::screens::{Nav, PlayRequest, ResultData};
use crate::ui::theme::{self, Palette};
use crate::ui::{Toasts, anim, fonts, widgets};

/// State shared by all screens.
pub struct Shared {
    pub settings: Settings,
    saved_settings: Settings,
    last_save_check: Instant,
    /// `None` when no audio device could be opened; the app still runs.
    pub audio: Option<Audio>,
    pub palette: Palette,
    pub songs_dir: PathBuf,
    pub toasts: Toasts,
    pub scores: ScoreStore,
    /// When the app started (for ambient animations).
    pub start: Instant,
    /// Name of the connected gamepad / dance mat, if any.
    pub gamepad_name: Option<String>,
    /// Set for one frame when a gamepad is unplugged (gameplay pauses).
    pub gamepad_disconnected: bool,
    /// Navigation requested from inside a nested widget.
    pub pending_nav: Option<Nav>,
}

impl Shared {
    /// Saves settings shortly after they change ("no Save button").
    fn autosave(&mut self, now: Instant) {
        if anim::since(self.last_save_check, now) < 0.5 {
            return;
        }
        self.last_save_check = now;
        if self.settings != self.saved_settings {
            if let Err(e) = self.settings.save() {
                log::warn!("could not save settings: {e:#}");
            }
            if let Some(audio) = &mut self.audio {
                audio.set_volume(self.settings.volume);
            }
            self.saved_settings = self.settings.clone();
        }
    }
}

/// The current screen.
enum Screen {
    Title(TitleScreen),
    Gameplay(Box<GameplayScreen>),
    Results(Box<ResultsScreen>),
}

/// The whole app.
pub struct FerrostepApp {
    shared: Shared,
    input: InputHub,
    screen: Screen,
    transition_start: Instant,
}

impl FerrostepApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        fonts::install(&cc.egui_ctx);
        let settings = Settings::load();
        let palette = Palette::for_choice(settings.theme);
        theme::apply(&cc.egui_ctx, &palette, settings.ui_scale);

        let mut toasts = Toasts::default();
        let audio = match Audio::new() {
            Ok(mut audio) => {
                audio.set_volume(settings.volume);
                Some(audio)
            }
            Err(e) => {
                log::error!("{e:#}");
                toasts.warning("Perangkat audio tidak ditemukan. Cek speaker/headphone, lalu buka ulang aplikasi.");
                None
            }
        };
        let now = Instant::now();
        Self {
            shared: Shared {
                songs_dir: songs_dir(&settings),
                saved_settings: settings.clone(),
                settings,
                last_save_check: now,
                audio,
                palette,
                toasts,
                scores: ScoreStore::load(),
                start: now,
                gamepad_name: None,
                gamepad_disconnected: false,
                pending_nav: None,
            },
            input: InputHub::new(),
            screen: Screen::Title(TitleScreen::default()),
            transition_start: now,
        }
    }

    /// Temporary until the song list exists: play the metronome test song.
    fn test_song_request(&mut self) -> Option<PlayRequest> {
        let dir = self.shared.songs_dir.join("_Test").join("Metronome");
        if !dir.join("metronome.sm").exists()
            && let Err(e) = test_song::make_test_song(&self.shared.songs_dir)
        {
            self.shared
                .toasts
                .warning(format!("Lagu uji gagal dibuat: {e}"));
            return None;
        }
        match load_simfile(&dir.join("metronome.sm")) {
            Ok(sim) => Some(PlayRequest {
                simfile: Arc::new(sim),
                song_dir: dir,
                chart_index: 0,
            }),
            Err(e) => {
                self.shared
                    .toasts
                    .warning(format!("Lagu uji tidak bisa dibaca: {e}"));
                None
            }
        }
    }

    fn navigate(&mut self, ctx: &egui::Context, nav: Nav) {
        let next = match nav {
            Nav::Title => Screen::Title(TitleScreen::default()),
            Nav::SongSelect => match self.test_song_request() {
                Some(req) => return self.navigate(ctx, Nav::Play(req)),
                None => Screen::Title(TitleScreen::default()),
            },
            Nav::Import(_) | Nav::Settings => {
                self.shared.toasts.info("Segera hadir.");
                return;
            }
            Nav::Play(req) => match GameplayScreen::new(req, &self.shared.settings) {
                Ok(screen) => Screen::Gameplay(Box::new(screen)),
                Err(e) => {
                    self.shared
                        .toasts
                        .warning(format!("Tidak bisa memulai lagu: {e}"));
                    return;
                }
            },
            Nav::Results(data) => {
                Screen::Results(Box::new(ResultsScreen::new(self.record_score(*data))))
            }
            Nav::Quit => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        };
        self.screen = next;
        self.transition_start = Instant::now();
    }

    /// Saves a new best score and fills `previous_best`.
    fn record_score(&mut self, mut data: ResultData) -> ResultData {
        let chart = &data.request.simfile.charts[data.request.chart_index];
        let key = chart_key(&data.request.song_dir, chart);
        let failed = data.score.failed && !data.no_fail;
        if !failed {
            data.previous_best = self.shared.scores.record(
                key,
                BestScore {
                    percent: data.score.percent(),
                    grade: data.score.grade(data.no_fail).to_string(),
                    max_combo: data.score.max_combo,
                },
            );
        } else {
            data.previous_best = self.shared.scores.get(&key).map(|b| b.percent);
        }
        data
    }

    fn screen_ui(&mut self, ui: &mut egui::Ui, input: &FrameInput) -> Option<Nav> {
        let shared = &mut self.shared;
        match &mut self.screen {
            Screen::Title(s) => s.ui(ui, shared, input),
            Screen::Gameplay(s) => s.ui(ui, shared, input),
            Screen::Results(s) => s.ui(ui, shared, input),
        }
    }
}

impl eframe::App for FerrostepApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let input = self.input.poll(&ctx, &self.shared.settings);
        let now = input.now;
        let full = ui.max_rect();
        let reduce = self.shared.settings.reduce_motion;

        if !matches!(self.screen, Screen::Gameplay(_)) {
            let t = if reduce {
                0.0
            } else {
                anim::since(self.shared.start, now)
            };
            widgets::background(ui.painter(), full, &self.shared.palette, t, None);
        }

        // Transition: new screens slide up 16 px and fade in.
        let e = anim::ease_out_cubic(anim::progress(
            self.transition_start,
            now,
            anim::TRANSITION,
            reduce,
        ));
        let mut child =
            ui.new_child(UiBuilder::new().max_rect(full.translate(vec2(0.0, (1.0 - e) * 16.0))));
        let mut nav = self.screen_ui(&mut child, &input);
        if e < 1.0 {
            let base = self.shared.palette.bg_top;
            ui.painter().rect_filled(
                full,
                0,
                Color32::from_rgba_unmultiplied(
                    base.r(),
                    base.g(),
                    base.b(),
                    ((1.0 - e) * 255.0) as u8,
                ),
            );
            ctx.request_repaint();
        }

        if nav.is_none() {
            nav = self.shared.pending_nav.take();
        }
        if let Some(nav) = nav {
            self.navigate(&ctx, nav);
        }
        self.shared.toasts.show(&ctx, &self.shared.palette, now);
        self.shared.autosave(now);
    }
}

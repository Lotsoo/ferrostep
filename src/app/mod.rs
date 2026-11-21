//! Top-level application: screen state machine, shared state and transitions.

use std::time::Instant;

use eframe::egui::{self, Color32, UiBuilder, vec2};

use crate::audio::{Audio, Sfx};
use crate::config::Settings;
use crate::input::gamepad::PadEvent;
use crate::input::profiles::DeviceProfile;
use crate::input::{FrameInput, InputHub, MenuAction};
use crate::library::scores::{BestScore, ScoreStore, chart_key};
use crate::library::{Library, songs_dir, test_song};
use crate::ui::screens::gameplay::GameplayScreen;
use crate::ui::screens::import::ImportScreen;
use crate::ui::screens::onboarding::OnboardingScreen;
use crate::ui::screens::pad_setup::PadSetupScreen;
use crate::ui::screens::results::ResultsScreen;
use crate::ui::screens::settings::SettingsScreen;
use crate::ui::screens::song_select::SongSelectScreen;
use crate::ui::screens::title::TitleScreen;
use crate::ui::screens::{Nav, ResultData};
use crate::ui::theme::{self, Palette};
use crate::ui::{Toasts, anim, fonts, widgets};

mod quit;
mod shell;

use quit::QuitDialog;
use shell::{draw_transition_streaks, pointer_cursor};

/// Command-line options for developers (see `--help`).
#[derive(Debug, Clone, Default)]
pub struct LaunchOptions {
    /// Start playing this song folder right away.
    pub play: Option<std::path::PathBuf>,
    /// Chart index inside the simfile (default: the hardest playable chart).
    pub chart: Option<usize>,
    /// The game hits every note itself (scores are not saved).
    pub autoplay: bool,
    /// Open this screen first: `songs`, `import` or `settings`.
    pub open: Option<String>,
    /// Files to import right away.
    pub import: Vec<std::path::PathBuf>,
}

/// State shared by all screens.
pub struct Shared {
    pub settings: Settings,
    saved_settings: Settings,
    last_save_check: Instant,
    /// `None` when no audio device could be opened; the app still runs.
    pub audio: Option<Audio>,
    pub palette: Palette,
    pub library: Library,
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
    /// Profile finished by the mapping wizard, saved by the app.
    pub save_pad_profile: Option<(String, DeviceProfile)>,
    /// Connected gamepads / dance mats: (profile key, name).
    pub connected_pads: Vec<(String, String)>,
    /// AutoPlay mode from the command line.
    pub autoplay: bool,
    /// The welcome letter or greeting videos are showing: no title music.
    pub hold_music: bool,
    /// Theme and UI scale currently applied to egui.
    applied_look: (crate::config::ThemeChoice, f32),
}

impl Shared {
    /// Plays a menu sound (silently does nothing without an audio device).
    pub fn sfx(&mut self, sfx: Sfx) {
        if let Some(audio) = &mut self.audio {
            audio.play_sfx(sfx);
        }
    }

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
                audio.set_sfx_volume(self.settings.sfx_volume);
            }
            self.saved_settings = self.settings.clone();
        }
    }
}

/// The current screen.
enum Screen {
    Title(Box<TitleScreen>),
    SongSelect(Box<SongSelectScreen>),
    Import(Box<ImportScreen>),
    Settings(Box<SettingsScreen>),
    Onboarding(Box<OnboardingScreen>),
    Gameplay(Box<GameplayScreen>),
    Results(Box<ResultsScreen>),
    PadSetup(Box<PadSetupScreen>),
    Tutorial(Box<crate::ui::screens::tutorial::TutorialScreen>),
}

/// The whole app.
pub struct FerrostepApp {
    /// Welcome letter shown on every start (private builds only); blocks everything else.
    letter: Option<crate::ui::gift::letter::Letter>,
    /// Looping trumpet that plays while the letter is open.
    letter_music: Option<crate::audio::Music>,
    /// Greeting videos after the letter (private builds only); unskippable.
    greetings: Option<crate::ui::gift::Greetings>,
    quit_dialog: Option<QuitDialog>,
    /// Set once the player confirmed; the next close request goes through.
    allow_close: bool,
    /// Clickable widget hovered last frame (for the hover tick).
    last_hover: Option<egui::Id>,
    shared: Shared,
    input: InputHub,
    screen: Screen,
    transition_start: Instant,
    /// Screen transition progress 0 → 1 (spring, interruptible).
    transition: anim::Spring,
    /// +1 when going deeper (new screen enters from the right), −1 when going back.
    transition_dir: f32,
    /// Frames counted since `fps_since` (logged at debug level).
    fps_frames: u32,
    fps_since: Instant,
    /// CPU time spent building frames since `fps_since` (seconds).
    fps_work: f32,
}

impl Screen {
    /// How deep a screen sits in the navigation, for transition direction.
    fn depth(&self) -> u8 {
        match self {
            Screen::Title(_) | Screen::Onboarding(_) => 0,
            Screen::SongSelect(_)
            | Screen::Import(_)
            | Screen::Settings(_)
            | Screen::Tutorial(_) => 1,
            Screen::PadSetup(_) | Screen::Gameplay(_) => 2,
            Screen::Results(_) => 3,
        }
    }
}

impl FerrostepApp {
    pub fn new(cc: &eframe::CreationContext<'_>, launch: LaunchOptions) -> Self {
        fonts::install(&cc.egui_ctx);
        let settings = Settings::load();
        let palette = Palette::for_choice(settings.theme);
        theme::apply(&cc.egui_ctx, &palette, settings.ui_scale);

        let mut toasts = Toasts::default();
        let audio = match Audio::new() {
            Ok(mut audio) => {
                audio.set_volume(settings.volume);
                audio.set_sfx_volume(settings.sfx_volume);
                Some(audio)
            }
            Err(e) => {
                log::error!("{e:#}");
                toasts.warning("Perangkat audio tidak ditemukan. Cek speaker/headphone, lalu buka ulang aplikasi.");
                None
            }
        };

        let root = songs_dir(&settings);
        // First start: give new players something to play right away.
        if !root.exists()
            && let Err(e) = test_song::make_test_song(&root)
        {
            log::warn!("could not create the sample song: {e:#}");
        }
        log::info!("songs folder: {}", root.display());

        let (settings_theme, settings_scale) = (settings.theme, settings.ui_scale);
        let first_screen = if settings.onboarding_done {
            Screen::Title(Box::default())
        } else {
            Screen::Onboarding(Box::default())
        };
        let now = Instant::now();
        let mut app = Self {
            shared: Shared {
                library: Library::new(root, &cc.egui_ctx),
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
                save_pad_profile: None,
                connected_pads: Vec::new(),
                autoplay: launch.autoplay,
                hold_music: false,
                applied_look: (settings_theme, settings_scale),
            },
            input: InputHub::new(&cc.egui_ctx),
            screen: first_screen,
            transition_start: now,
            transition: anim::Spring::new(1.0, anim::Spring::SMOOTH),
            last_hover: None,
            letter: None,
            letter_music: None,
            greetings: None,
            quit_dialog: None,
            allow_close: false,
            transition_dir: 1.0,
            fps_frames: 0,
            fps_since: now,
            fps_work: 0.0,
        };
        app.letter = crate::ui::gift::letter::Letter::new(now);
        if app.letter.is_some()
            && let (Some(trumpet), Some(audio)) = (crate::ui::gift::TRUMPET, &mut app.shared.audio)
        {
            match audio.play_bytes(trumpet, true, 800) {
                Ok(music) => app.letter_music = Some(music),
                Err(e) => log::warn!("letter music: {e:#}"),
            }
        }
        app.shared.hold_music = app.letter.is_some();
        if !launch.import.is_empty() {
            app.navigate(Nav::Import(launch.import.clone()));
        }
        match launch.open.as_deref() {
            Some("songs") => app.navigate(Nav::SongSelect),
            Some("import") => app.navigate(Nav::Import(Vec::new())),
            Some("settings") => app.navigate(Nav::Settings),
            Some("calibrate") => app.navigate(Nav::Calibrate),
            Some("tutorial") => app.navigate(Nav::Tutorial),
            Some(other) => log::warn!("--open: unknown screen '{other}'"),
            None => {}
        }
        if let Some(dir) = launch.play {
            app.play_from_command_line(&dir, launch.chart);
        }
        app
    }

    /// `--play <folder>`: skip the menus and start a chart.
    fn play_from_command_line(&mut self, dir: &std::path::Path, chart: Option<usize>) {
        let Some(song) = crate::library::scanner::load_song(dir, "") else {
            log::error!("--play: no playable simfile in {}", dir.display());
            return;
        };
        let Some(&hardest) = song.playable.last() else {
            log::error!("--play: no dance-single chart in {}", dir.display());
            return;
        };
        let chart_index = chart.unwrap_or(hardest);
        let request = crate::ui::screens::PlayRequest {
            simfile: song.simfile.clone(),
            song_dir: song.dir.clone(),
            chart_index,
        };
        self.navigate(Nav::Play(request));
    }

    fn navigate(&mut self, nav: Nav) {
        // Leaving onboarding in any way (finished, skipped, mat wizard) completes it.
        if matches!(self.screen, Screen::Onboarding(_)) {
            self.shared.settings.onboarding_done = true;
        }
        let next = match nav {
            Nav::Title => Screen::Title(Box::default()),
            Nav::SongSelect => Screen::SongSelect(Box::default()),
            Nav::Import(files) => {
                if let Screen::Import(s) = &mut self.screen {
                    s.enqueue(files);
                    return;
                }
                Screen::Import(Box::new(ImportScreen::new(files, &self.shared)))
            }
            Nav::Settings => Screen::Settings(Box::new(SettingsScreen::new(None))),
            Nav::Calibrate => Screen::Settings(Box::new(SettingsScreen::calibrating())),
            Nav::Tutorial => Screen::Tutorial(Box::default()),
            Nav::PadSetup {
                key,
                name,
                from_settings,
            } => Screen::PadSetup(Box::new(PadSetupScreen::new(key, name, from_settings))),
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
                self.open_quit_dialog();
                return;
            }
        };
        // Spatial consistency: deeper screens arrive from the right, going back
        // brings the previous screen in from the left — the way it left.
        let (from, to) = (self.screen.depth(), next.depth());
        self.transition_dir = if to >= from { 1.0 } else { -1.0 };
        self.screen = next;
        self.transition_start = Instant::now();
        self.transition.snap(0.0);
        self.transition.set_target(1.0);
    }

    /// Saves a new best score and fills `previous_best`.
    fn record_score(&mut self, mut data: ResultData) -> ResultData {
        if self.shared.autoplay {
            return data; // AutoPlay results are not real scores.
        }
        let chart = &data.request.simfile.charts[data.request.chart_index];
        let key = chart_key(&data.request.song_dir, chart);
        let failed = data.score.failed && !data.no_fail;
        data.previous_best = if failed {
            self.shared.scores.get(&key).map(|b| b.percent)
        } else {
            self.shared.scores.record(
                key,
                BestScore {
                    percent: data.score.percent(),
                    grade: data.score.grade(data.no_fail).to_string(),
                    max_combo: data.score.max_combo,
                },
            )
        };
        data
    }

    /// Toasts and the mapping wizard when mats are plugged in or out.
    fn handle_pad_events(&mut self, input: &FrameInput) -> Option<Nav> {
        let mut nav = None;
        for ev in &input.pad_events {
            match ev {
                PadEvent::Connected {
                    name,
                    key,
                    has_profile,
                } => {
                    self.shared
                        .toasts
                        .success(format!("Dance mat terhubung: {name}"));
                    let in_game = matches!(self.screen, Screen::Gameplay(_) | Screen::PadSetup(_));
                    if !has_profile && !in_game {
                        nav = Some(Nav::PadSetup {
                            key: key.clone(),
                            name: name.clone(),
                            from_settings: false,
                        });
                    } else if !has_profile {
                        self.shared
                            .toasts
                            .info("Atur dance mat di Pengaturan → Kontrol setelah lagu selesai.");
                    }
                }
                PadEvent::Disconnected { name } => {
                    self.shared
                        .toasts
                        .warning(format!("{name} terputus — permainan dijeda"));
                    self.shared.gamepad_disconnected = true;
                }
                _ => {}
            }
        }
        self.shared.gamepad_name = self.input.gamepads.first_name();
        self.shared.connected_pads = self.input.gamepads.connected();
        if let Some((key, profile)) = self.shared.save_pad_profile.take() {
            self.input.gamepads.save_profile(key, profile);
        }
        nav
    }

    fn screen_ui(&mut self, ui: &mut egui::Ui, input: &FrameInput) -> Option<Nav> {
        let shared = &mut self.shared;
        match &mut self.screen {
            Screen::Title(s) => s.ui(ui, shared, input),
            Screen::SongSelect(s) => s.ui(ui, shared, input),
            Screen::Import(s) => s.ui(ui, shared, input),
            Screen::Settings(s) => s.ui(ui, shared, input),
            Screen::Onboarding(s) => s.ui(ui, shared, input),
            Screen::Gameplay(s) => s.ui(ui, shared, input),
            Screen::Results(s) => s.ui(ui, shared, input),
            Screen::PadSetup(s) => s.ui(ui, shared, input),
            Screen::Tutorial(s) => s.ui(ui, shared, input),
        }
    }
}

impl eframe::App for FerrostepApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let input = self.input.poll(&ctx, &self.shared.settings);
        // Closing the window asks first (an accidental click must not lose a play).
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            // The welcome letter must be read to the end before anything else.
            if let Some(letter) = &mut self.letter {
                letter.nudge(input.now);
            } else if let Some(greetings) = &mut self.greetings {
                greetings.nudge(input.now);
            } else {
                self.open_quit_dialog();
            }
        }
        // While the dialog is open, screens get no keys or steps.
        self.shared.hold_music = self.letter.is_some() || self.greetings.is_some();
        let screen_input = if self.quit_dialog.is_some() || self.shared.hold_music {
            let mut quiet = input.clone();
            quiet.menu.clear();
            quiet.columns.clear();
            quiet.last_key = None;
            quiet.pad_events.clear();
            quiet
        } else {
            input.clone()
        };
        let now = input.now;
        let full = ui.max_rect();
        let reduce = self.shared.settings.reduce_motion;
        self.shared.library.poll(now);
        self.apply_look(&ctx);
        let pad_nav = self.handle_pad_events(&input);

        if !matches!(self.screen, Screen::Gameplay(_)) {
            let t = if reduce {
                0.0
            } else {
                anim::since(self.shared.start, now)
            };
            widgets::background(ui.painter(), full, &self.shared.palette, t, None);
        }

        // Transition: a critically damped spring slides the new screen in from
        // the side it belongs to. Reduced motion: a short cross-fade only.
        let e = if reduce {
            anim::progress(self.transition_start, now, 0.15, false)
        } else {
            let dt = ctx.input(|i| i.stable_dt);
            self.transition.step(dt).clamp(0.0, 1.0)
        };
        let shift = if reduce {
            0.0
        } else {
            (1.0 - e) * 48.0 * self.transition_dir
        };
        let mut child = ui.new_child(UiBuilder::new().max_rect(full.translate(vec2(shift, 0.0))));
        let mut nav = self.screen_ui(&mut child, &screen_input);
        if e < 0.999 {
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
            if !reduce {
                draw_transition_streaks(ui.painter(), full, e, self.transition_dir);
            }
            ctx.request_repaint();
        }

        if let Some(letter) = &mut self.letter {
            let palette = self.shared.palette;
            let reduce = self.shared.settings.reduce_motion;
            if letter.ui(&ctx, &palette, &input, reduce) {
                self.letter = None;
                if let Some(mut music) = self.letter_music.take() {
                    music.fade_out(500);
                }
                self.greetings = crate::ui::gift::Greetings::new(now);
                if self.greetings.is_none() {
                    self.shared.sfx(Sfx::Success);
                }
            }
        } else if let Some(greetings) = &mut self.greetings {
            let palette = self.shared.palette;
            let reduce = self.shared.settings.reduce_motion;
            if greetings.ui(&ctx, &palette, self.shared.audio.as_mut(), now, reduce) {
                self.greetings = None;
                self.shared.sfx(Sfx::Success);
            }
        }
        self.quit_dialog_ui(&ctx, &input);
        pointer_cursor(&ctx);
        self.menu_sounds(&ctx, &input);
        let drop_nav = self.handle_dropped_files(ui, &ctx);
        nav = nav
            .or_else(|| self.shared.pending_nav.take())
            .or(pad_nav)
            .or(drop_nav);
        if let Some(nav) = nav {
            if matches!(nav, Nav::Play(_)) {
                self.shared.sfx(Sfx::Start);
            }
            self.navigate(nav);
        }
        self.shared.toasts.show(&ctx, &self.shared.palette, now);
        self.log_fps(now, anim::since(now, Instant::now()));
        self.shared.autosave(now);
    }
}

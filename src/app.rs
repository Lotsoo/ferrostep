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
/// "Really quit?" dialog state.
struct QuitDialog {
    /// 0 = Batal (default, safe), 1 = Keluar.
    selected: usize,
    opened: Instant,
}

pub struct FerrostepApp {
    /// Welcome letter shown on every start (private builds only); blocks everything else.
    letter: Option<crate::ui::letter::Letter>,
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
        app.letter = crate::ui::letter::Letter::new(now);
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

    /// Arcade-style feedback: a sound for every menu move, choice and click.
    /// Silent while playing (the music and your steps are the sound) and
    /// during calibration (it needs to hear only the metronome).
    fn menu_sounds(&mut self, ctx: &egui::Context, input: &FrameInput) {
        let hovered = hovered_clickable(ctx);
        let new_hover = hovered.is_some() && hovered != self.last_hover;
        self.last_hover = hovered;
        let quiet = match &self.screen {
            Screen::Gameplay(g) => !g.is_paused(),
            Screen::Settings(s) => s.is_calibrating(),
            Screen::Onboarding(o) => o.is_calibrating() || o.in_tutorial(),
            Screen::PadSetup(_) | Screen::Tutorial(_) => true,
            _ => false,
        };
        if quiet {
            return;
        }
        let sound = input
            .menu
            .last()
            .map(|a| sfx_for(*a))
            .or_else(|| {
                ctx.interaction_snapshot(|i| i.clicked)
                    .map(|_| Sfx::Confirm)
            })
            .or_else(|| new_hover.then_some(Sfx::Hover));
        if let Some(sound) = sound {
            self.shared.sfx(sound);
        }
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

    /// Logs the frame rate every 2 s (`RUST_LOG=ferrostep=debug`).
    ///
    /// `work` is the CPU time this frame took to build. The FPS itself is often
    /// capped by the compositor/monitor, so the CPU time shows the real headroom.
    fn log_fps(&mut self, now: Instant, work: f32) {
        self.fps_frames += 1;
        self.fps_work += work;
        let elapsed = anim::since(self.fps_since, now);
        if elapsed >= 2.0 {
            let avg_ms = self.fps_work / self.fps_frames as f32 * 1000.0;
            log::debug!(
                "fps: {:.0}, frame build {avg_ms:.2} ms (~{:.0} fps possible)",
                self.fps_frames as f32 / elapsed,
                1000.0 / avg_ms.max(0.01)
            );
            self.fps_frames = 0;
            self.fps_since = now;
            self.fps_work = 0.0;
        }
    }

    /// Re-applies theme and UI scale after they change in Settings.
    fn apply_look(&mut self, ctx: &egui::Context) {
        let wanted = (self.shared.settings.theme, self.shared.settings.ui_scale);
        if wanted != self.shared.applied_look {
            self.shared.palette = Palette::for_choice(wanted.0);
            theme::apply(ctx, &self.shared.palette, wanted.1);
            self.shared.applied_look = wanted;
        }
    }

    /// Files dropped on the window open the import screen (from any screen but gameplay).
    fn handle_dropped_files(&mut self, ui: &egui::Ui, ctx: &egui::Context) -> Option<Nav> {
        let (hovered, dropped) = ctx.input(|i| {
            let dropped: Vec<std::path::PathBuf> = i
                .raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect();
            (!i.raw.hovered_files.is_empty(), dropped)
        });
        let in_game = matches!(self.screen, Screen::Gameplay(_));
        if hovered && !in_game {
            let rect = ui.max_rect();
            let painter = ui.ctx().layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::unique("drop-overlay"),
            ));
            painter.rect_filled(rect, 0, Color32::from_black_alpha(170));
            painter.rect_stroke(
                rect.shrink(16.0),
                theme::RADIUS_CARD,
                egui::Stroke::new(3.0, theme::ACCENT_B),
                egui::StrokeKind::Inside,
            );
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Lepaskan untuk mengimpor",
                theme::font_bold(theme::TEXT_XL),
                Color32::WHITE,
            );
        }
        if dropped.is_empty() {
            return None;
        }
        let files: Vec<_> = dropped
            .into_iter()
            .filter(|p| crate::importer::is_importable(p))
            .collect();
        if files.is_empty() {
            self.shared
                .toasts
                .warning("Hanya file MP3, MP4, M4A, OGG, WAV, atau FLAC yang bisa diimpor.");
            None
        } else if in_game {
            self.shared
                .toasts
                .info("Selesaikan lagu dulu, lalu seret file lagi untuk mengimpor.");
            None
        } else {
            Some(Nav::Import(files))
        }
    }

    /// Asks before quitting (window close button, Alt+F4, Esc on the title).
    fn open_quit_dialog(&mut self) {
        if self.quit_dialog.is_none() {
            self.quit_dialog = Some(QuitDialog {
                selected: 0,
                opened: Instant::now(),
            });
            if let Screen::Gameplay(g) = &mut self.screen {
                g.pause_now();
            }
        }
    }

    /// Draws the quit dialog and handles its keys. Returns true when it is open.
    fn quit_dialog_ui(&mut self, ctx: &egui::Context, input: &FrameInput) -> bool {
        let Some(dialog) = &mut self.quit_dialog else {
            return false;
        };
        let mut choice = None;
        for action in &input.menu {
            match action {
                MenuAction::Left | MenuAction::Right | MenuAction::Up | MenuAction::Down => {
                    dialog.selected = 1 - dialog.selected;
                }
                MenuAction::Confirm => choice = Some(dialog.selected == 1),
                MenuAction::Back => choice = Some(false),
            }
        }
        let palette = self.shared.palette;
        let appear = anim::ease_out_cubic(anim::progress(
            dialog.opened,
            input.now,
            0.2,
            self.shared.settings.reduce_motion,
        ));
        let importing = matches!(&self.screen, Screen::Import(s) if s.is_busy());
        let selected = dialog.selected;
        let modal = egui::Modal::new(egui::Id::unique("quit-dialog"))
            .backdrop_color(Color32::from_black_alpha((160.0 * appear) as u8))
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let size = vec2(420.0, 210.0);
                let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                let rect = egui::Rect::from_center_size(
                    rect.center() + vec2(0.0, (1.0 - appear) * 16.0),
                    rect.size() * (0.96 + 0.04 * appear),
                );
                let painter = ui.painter();
                widgets::card(painter, rect, &palette);
                widgets::text_tracked(
                    painter,
                    rect.left_top() + vec2(28.0, 28.0),
                    egui::Align2::LEFT_TOP,
                    "Keluar dari Ferrostep?",
                    theme::font_bold(theme::TEXT_L),
                    palette.text,
                );
                painter.text(
                    rect.left_top() + vec2(28.0, 68.0),
                    egui::Align2::LEFT_TOP,
                    if importing {
                        "Impor yang sedang berjalan akan dibatalkan."
                    } else {
                        "Pengaturan dan skormu sudah tersimpan otomatis."
                    },
                    theme::font(theme::TEXT_S),
                    palette.text_dim,
                );
                let mut clicked = None;
                let buttons = [("Batal", 0usize), ("Keluar", 1usize)];
                for (i, (label, value)) in buttons.into_iter().enumerate() {
                    let r = egui::Rect::from_min_size(
                        rect.left_bottom() + vec2(28.0 + i as f32 * 194.0, -76.0),
                        vec2(170.0, 48.0),
                    );
                    let resp = ui.put(r, |ui: &mut egui::Ui| {
                        if selected == value {
                            widgets::primary_button(ui, label, r.size(), true)
                        } else {
                            widgets::secondary_button(ui, &palette, label, r.size())
                        }
                    });
                    if resp.clicked() {
                        clicked = Some(value == 1);
                    }
                }
                clicked
            });
        let choice = choice
            .or(modal.inner)
            .or(modal.should_close().then_some(false));
        match choice {
            Some(true) => {
                self.allow_close = true;
                if let Screen::Import(s) = &mut self.screen {
                    s.cancel_all();
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Some(false) => self.quit_dialog = None,
            None => {}
        }
        ctx.request_repaint();
        true
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

/// Shows a pointing hand over anything clickable (our custom widgets draw
/// themselves, so egui would otherwise keep the arrow). Widgets that chose
/// their own cursor (text fields, dragged lists) keep it.
fn pointer_cursor(ctx: &egui::Context) {
    if ctx.output(|o| o.cursor_icon) != egui::CursorIcon::Default {
        return;
    }
    if hovered_clickable(ctx).is_some() {
        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

/// The enabled, clickable widget under the pointer, if any.
fn hovered_clickable(ctx: &egui::Context) -> Option<egui::Id> {
    ctx.interaction_snapshot(|i| i.hovered.clone())
        .into_iter()
        .find(|id| {
            ctx.read_response(*id)
                .is_some_and(|r| r.enabled() && r.sense.senses_click())
        })
}

/// Which sound a menu action makes.
fn sfx_for(action: MenuAction) -> Sfx {
    match action {
        MenuAction::Up => Sfx::MoveUp,
        MenuAction::Down => Sfx::MoveDown,
        MenuAction::Left => Sfx::ChangeLeft,
        MenuAction::Right => Sfx::ChangeRight,
        MenuAction::Confirm => Sfx::Confirm,
        MenuAction::Back => Sfx::Back,
    }
}

/// Four light streaks in the arrow colors that fly across the screen during a
/// screen change, in the direction of travel (no extra delay: they ride the
/// same spring as the slide).
fn draw_transition_streaks(painter: &egui::Painter, full: egui::Rect, e: f32, dir: f32) {
    for c in 0..4u8 {
        let p = ((e - f32::from(c) * 0.07) / 0.75).clamp(0.0, 1.0);
        if p <= 0.0 || p >= 1.0 {
            continue;
        }
        let travel = anim::ease_out_cubic(p);
        // Enter from the side the new screen comes from.
        let x = if dir > 0.0 {
            full.right() - travel * (full.width() + 120.0)
        } else {
            full.left() + travel * (full.width() + 120.0)
        };
        let color = theme::column_color(c);
        let alpha = (1.0 - p) * 0.8;
        let streak = egui::Rect::from_min_max(
            egui::pos2(x, full.top()),
            egui::pos2(x + 120.0 * dir.signum(), full.bottom()),
        );
        let streak =
            egui::Rect::from_min_max(streak.min.min(streak.max), streak.min.max(streak.max));
        let (left, right) = if dir > 0.0 {
            (color.gamma_multiply(alpha), Color32::TRANSPARENT)
        } else {
            (Color32::TRANSPARENT, color.gamma_multiply(alpha))
        };
        widgets::gradient_rect(painter, streak, left, right);
        let edge = if dir > 0.0 {
            streak.left()
        } else {
            streak.right()
        };
        painter.line_segment(
            [
                egui::pos2(edge, full.top()),
                egui::pos2(edge, full.bottom()),
            ],
            egui::Stroke::new(2.0, Color32::WHITE.gamma_multiply(alpha)),
        );
    }
}

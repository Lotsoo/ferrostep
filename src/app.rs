//! Top-level application: screen state machine, shared state and transitions.

use std::time::Instant;

use eframe::egui::{self, Color32, UiBuilder, vec2};

use crate::audio::Audio;
use crate::config::Settings;
use crate::input::gamepad::PadEvent;
use crate::input::profiles::DeviceProfile;
use crate::input::{FrameInput, InputHub};
use crate::library::scores::{BestScore, ScoreStore, chart_key};
use crate::library::{Library, songs_dir, test_song};
use crate::ui::screens::gameplay::GameplayScreen;
use crate::ui::screens::import::ImportScreen;
use crate::ui::screens::pad_setup::PadSetupScreen;
use crate::ui::screens::results::ResultsScreen;
use crate::ui::screens::song_select::SongSelectScreen;
use crate::ui::screens::title::TitleScreen;
use crate::ui::screens::{Nav, ResultData};
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
    SongSelect(Box<SongSelectScreen>),
    Import(Box<ImportScreen>),
    Gameplay(Box<GameplayScreen>),
    Results(Box<ResultsScreen>),
    PadSetup(Box<PadSetupScreen>),
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

        let root = songs_dir(&settings);
        // First start: give new players something to play right away.
        if !root.exists()
            && let Err(e) = test_song::make_test_song(&root)
        {
            log::warn!("could not create the sample song: {e:#}");
        }
        log::info!("songs folder: {}", root.display());

        let now = Instant::now();
        Self {
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
            },
            input: InputHub::new(&cc.egui_ctx),
            screen: Screen::Title(TitleScreen::default()),
            transition_start: now,
        }
    }

    fn navigate(&mut self, ctx: &egui::Context, nav: Nav) {
        let next = match nav {
            Nav::Title => Screen::Title(TitleScreen::default()),
            Nav::SongSelect => Screen::SongSelect(Box::default()),
            Nav::Import(files) => {
                if let Screen::Import(s) = &mut self.screen {
                    s.enqueue(files);
                    return;
                }
                Screen::Import(Box::new(ImportScreen::new(files, &self.shared)))
            }
            Nav::Settings => {
                self.shared.toasts.info("Segera hadir.");
                return;
            }
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
        if let Some((key, profile)) = self.shared.save_pad_profile.take() {
            self.input.gamepads.save_profile(key, profile);
        }
        nav
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

    fn screen_ui(&mut self, ui: &mut egui::Ui, input: &FrameInput) -> Option<Nav> {
        let shared = &mut self.shared;
        match &mut self.screen {
            Screen::Title(s) => s.ui(ui, shared, input),
            Screen::SongSelect(s) => s.ui(ui, shared, input),
            Screen::Import(s) => s.ui(ui, shared, input),
            Screen::Gameplay(s) => s.ui(ui, shared, input),
            Screen::Results(s) => s.ui(ui, shared, input),
            Screen::PadSetup(s) => s.ui(ui, shared, input),
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
        self.shared.library.poll(now);
        let pad_nav = self.handle_pad_events(&input);

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

        let drop_nav = self.handle_dropped_files(ui, &ctx);
        nav = nav
            .or_else(|| self.shared.pending_nav.take())
            .or(pad_nav)
            .or(drop_nav);
        if let Some(nav) = nav {
            self.navigate(&ctx, nav);
        }
        self.shared.toasts.show(&ctx, &self.shared.palette, now);
        self.shared.autosave(now);
    }
}

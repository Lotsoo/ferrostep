//! Top-level application state.

use eframe::egui::{self, Color32, RichText};

use crate::audio::Audio;

/// Background color from the design tokens (CLAUDE.md §6.1).
const BG: Color32 = Color32::from_rgb(0x0B, 0x0B, 0x12);
const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xF2, 0xF2, 0xF7);
const TEXT_SECONDARY: Color32 = Color32::from_rgb(0xA0, 0xA0, 0xB8);
const ACCENT: Color32 = Color32::from_rgb(0x7C, 0x5C, 0xFF);

/// The whole app. Later this will hold the `Screen` state machine.
pub struct FerrostepApp {
    /// `None` when no audio device could be opened; the app still runs.
    audio: Option<Audio>,
    /// Friendly status line shown under the title.
    status: String,
}

impl FerrostepApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Dark);

        let (audio, status) = match Audio::new() {
            Ok(mut audio) => match audio.beep(880.0) {
                Ok(()) => (
                    Some(audio),
                    "Audio siap. Kamu seharusnya mendengar bunyi beep.".to_owned(),
                ),
                Err(e) => {
                    log::error!("{e:#}");
                    (
                        Some(audio),
                        "Audio terbuka, tetapi beep gagal diputar.".to_owned(),
                    )
                }
            },
            Err(e) => {
                log::error!("{e:#}");
                (
                    None,
                    "Perangkat audio tidak ditemukan. Cek speaker/headphone.".to_owned(),
                )
            }
        };

        Self { audio, status }
    }
}

impl eframe::App for FerrostepApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.painter().rect_filled(ui.max_rect(), 0.0, BG);

        ui.add_space(ui.available_height() * 0.35);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("Halo, Ferrostep!")
                    .size(48.0)
                    .strong()
                    .color(TEXT_PRIMARY),
            );
            ui.add_space(8.0);
            ui.label(RichText::new(&self.status).size(16.0).color(TEXT_SECONDARY));
            ui.add_space(24.0);

            let button = egui::Button::new(
                RichText::new("Bunyikan beep")
                    .size(18.0)
                    .color(TEXT_PRIMARY),
            )
            .fill(ACCENT)
            .corner_radius(12.0)
            .min_size(egui::vec2(200.0, 48.0));
            if ui.add_enabled(self.audio.is_some(), button).clicked()
                && let Some(audio) = &mut self.audio
                && let Err(e) = audio.beep(660.0)
            {
                log::error!("{e:#}");
                self.status = "Beep gagal diputar.".to_owned();
            }
        });
    }
}

//! Bundled Inter font (SIL OFL 1.1, see `assets/fonts/LICENSE-Inter.txt`).
//!
//! Fonts are embedded in the binary so the app works without extra files.

use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

/// Name of the bold font family.
pub const BOLD: &str = "bold";

static INTER_REGULAR: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");
static INTER_BOLD: &[u8] = include_bytes!("../../assets/fonts/Inter-Bold.ttf");

/// Installs Inter as the default font, keeping egui's fonts as fallback (emoji, symbols).
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "inter".into(),
        Arc::new(FontData::from_static(INTER_REGULAR)),
    );
    fonts.font_data.insert(
        "inter-bold".into(),
        Arc::new(FontData::from_static(INTER_BOLD)),
    );

    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
        list.insert(0, "inter".into());
    }
    let mut bold = vec!["inter-bold".to_string()];
    bold.extend(fallback);
    fonts.families.insert(FontFamily::Name(BOLD.into()), bold);
    ctx.set_fonts(fonts);
}

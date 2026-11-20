//! Screens and navigation between them.

pub mod gameplay;
pub mod pad_setup;
pub mod results;
pub mod song_select;
pub mod title;

use std::path::PathBuf;
use std::sync::Arc;

use crate::gameplay::score::Score;
use crate::parser::{Difficulty, Simfile};

/// Everything needed to start playing a chart.
#[derive(Clone)]
pub struct PlayRequest {
    pub simfile: Arc<Simfile>,
    pub song_dir: PathBuf,
    pub chart_index: usize,
}

/// Outcome of a play, shown on the results screen.
#[derive(Clone)]
pub struct ResultData {
    pub request: PlayRequest,
    pub title: String,
    pub artist: String,
    pub difficulty: Difficulty,
    pub meter: u32,
    pub score: Score,
    pub no_fail: bool,
    pub song_length: f64,
    /// Previous best percentage for this chart, if any.
    pub previous_best: Option<f64>,
}

/// Where to go next. Returned by a screen's `ui` function.
pub enum Nav {
    Title,
    SongSelect,
    /// Open the import screen, optionally with files to import right away.
    Import(Vec<PathBuf>),
    Play(PlayRequest),
    Results(Box<ResultData>),
    Settings,
    /// Mapping wizard for a dance mat.
    PadSetup {
        key: String,
        name: String,
        from_settings: bool,
    },
    Quit,
}

/// Indonesian name of a difficulty.
pub fn difficulty_label(d: Difficulty) -> &'static str {
    match d {
        Difficulty::Beginner => "Pemula",
        Difficulty::Easy => "Mudah",
        Difficulty::Medium => "Sedang",
        Difficulty::Hard => "Sulit",
        Difficulty::Challenge => "Tantangan",
        Difficulty::Edit => "Edit",
    }
}

/// Color of a difficulty chip.
pub fn difficulty_color(d: Difficulty) -> eframe::egui::Color32 {
    use eframe::egui::Color32;
    match d {
        Difficulty::Beginner => Color32::from_rgb(0x5C, 0xFF, 0xB0),
        Difficulty::Easy => Color32::from_rgb(0x3E, 0xC8, 0xFF),
        Difficulty::Medium => Color32::from_rgb(0xFF, 0xC8, 0x5C),
        Difficulty::Hard => Color32::from_rgb(0xFF, 0x5C, 0x8A),
        Difficulty::Challenge => Color32::from_rgb(0xB0, 0x7C, 0xFF),
        Difficulty::Edit => Color32::from_rgb(0xA0, 0xA0, 0xB8),
    }
}

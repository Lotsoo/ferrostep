//! Ferrostep library: all game logic lives here so it can be tested
//! without opening a window. `main.rs` only starts the app.

pub mod app;
pub mod audio;
pub mod audio_dsp;
pub mod chart_gen;
pub mod config;
pub mod gameplay;
pub mod importer;
pub mod input;
pub mod library;
pub mod parser;
pub mod timing;
pub mod ui;

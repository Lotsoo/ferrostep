//! Ferrostep — a modern 4-panel rhythm game.
//!
//! Entry point: sets up logging, handles command-line options and starts the window.

// Release builds on Windows: no console window behind the game.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use ferrostep::{app, config::Settings, library};

fn main() -> eframe::Result {
    // Logging: run with `RUST_LOG=debug cargo run` to see more output.
    // Noisy third-party crates are limited to warnings/errors by default.
    env_logger::Builder::from_env(
        env_logger::Env::default()
            .default_filter_or("info,wgpu_hal=error,wgpu_core=warn,zbus=warn,tracing=warn"),
    )
    .init();

    if std::env::args().any(|a| a == "--make-test-song") {
        let settings = Settings::load();
        let songs = library::songs_dir(&settings);
        match library::test_song::make_test_song(&songs) {
            Ok(dir) => println!("Lagu uji dibuat di: {}", dir.display()),
            Err(e) => {
                eprintln!("Gagal membuat lagu uji: {e:#}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Ferrostep")
            .with_app_id("ferrostep") // used by Wayland/KDE to match the .desktop file
            .with_inner_size([1280.0, 760.0])
            .with_min_inner_size([960.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Ferrostep",
        options,
        Box::new(|cc| Ok(Box::new(app::FerrostepApp::new(cc)))),
    )
}

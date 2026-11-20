//! Ferrostep — a modern 4-panel rhythm game.
//!
//! Entry point: sets up logging and starts the eframe window.

mod app;
mod audio;

fn main() -> eframe::Result {
    // Logging: run with `RUST_LOG=debug cargo run` to see more output.
    // Noisy third-party crates are limited to warnings/errors by default.
    env_logger::Builder::from_env(
        env_logger::Env::default()
            .default_filter_or("info,wgpu_hal=error,wgpu_core=warn,zbus=warn,tracing=warn"),
    )
    .init();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Ferrostep")
            .with_app_id("ferrostep") // used by Wayland/KDE to match the .desktop file
            .with_inner_size([1280.0, 720.0])
            .with_min_inner_size([800.0, 450.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Ferrostep",
        options,
        Box::new(|cc| Ok(Box::new(app::FerrostepApp::new(cc)))),
    )
}

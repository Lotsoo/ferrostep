//! Ferrostep — a modern 4-panel rhythm game.
//!
//! Entry point: sets up logging, handles command-line options and starts the window.

// Release builds on Windows: no console window behind the game.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use ferrostep::config::{GraphicsBackend, Settings};
use ferrostep::{app, library};

fn main() -> eframe::Result {
    // Logging: run with `RUST_LOG=debug cargo run` to see more output.
    // Noisy third-party crates are limited to warnings/errors by default.
    env_logger::Builder::from_env(
        env_logger::Env::default()
            .default_filter_or("info,wgpu_hal=error,wgpu_core=warn,zbus=warn,tracing=warn"),
    )
    .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{HELP}");
        return Ok(());
    }
    let launch = match parse_launch(&args) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{e}\n\n{HELP}");
            std::process::exit(2);
        }
    };
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

    let settings = Settings::load();
    install_crash_log();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Ferrostep")
            .with_app_id("ferrostep") // used by Wayland/KDE to match the .desktop file
            .with_inner_size([1280.0, 760.0])
            .with_min_inner_size([960.0, 600.0])
            .with_icon(window_icon()),
        wgpu_options: wgpu_options(&settings),
        ..Default::default()
    };

    let result = eframe::run_native(
        "Ferrostep",
        options,
        Box::new(|cc| Ok(Box::new(app::FerrostepApp::new(cc, launch)))),
    );
    if let Err(e) = &result {
        log::error!("could not start: {e}");
        offer_graphics_fallback(settings, &e.to_string());
    }
    result
}

const HELP: &str = "Ferrostep — game ritme 4 panah

Pemakaian: ferrostep [opsi]

  --make-test-song        Buat lagu metronom di Songs/_Test/Metronome lalu keluar
  --play <folder lagu>    Langsung mainkan lagu di folder itu (untuk pengujian)
  --chart <nomor>         Pilih chart (indeks di simfile) untuk --play
  --autoplay              Game menekan panah sendiri (skor tidak disimpan)
  --open <layar>          Buka langsung: songs, import, atau settings
  <file.mp3> <file.mp4>…  Impor file-file tersebut (bisa lebih dari satu)
  -h, --help              Tampilkan bantuan ini

Log rinci: RUST_LOG=ferrostep=debug";

/// Parses developer options.
fn parse_launch(args: &[String]) -> Result<app::LaunchOptions, String> {
    let mut launch = app::LaunchOptions::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--play" => {
                let dir = it.next().ok_or("--play butuh folder lagu")?;
                launch.play = Some(dir.into());
            }
            "--chart" => {
                let n = it.next().ok_or("--chart butuh angka")?;
                launch.chart = Some(
                    n.parse()
                        .map_err(|_| format!("--chart: '{n}' bukan angka"))?,
                );
            }
            "--autoplay" => launch.autoplay = true,
            "--open" => launch.open = Some(it.next().ok_or("--open butuh nama layar")?.clone()),
            "--make-test-song" => {}
            // Audio/video files ("Open with Ferrostep", or several at once): import them.
            other
                if !other.starts_with('-')
                    && ferrostep::importer::is_importable(other.as_ref()) =>
            {
                launch.import.push(other.into());
            }
            // Ignore unknown flags some launchers add (e.g. -psn_ on macOS).
            other => log::debug!("ignoring argument {other}"),
        }
    }
    Ok(launch)
}

/// Taskbar/window icon (Windows, X11). Wayland uses the .desktop file's icon.
fn window_icon() -> eframe::egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/icons/ferrostep-128.png"))
        .unwrap_or_default()
}

/// Graphics settings: VSync and the backend chosen in Settings.
fn wgpu_options(settings: &Settings) -> eframe::egui_wgpu::WgpuConfiguration {
    use eframe::egui_wgpu::{WgpuSetup, wgpu};
    let mut config = eframe::egui_wgpu::WgpuConfiguration::default();
    config.surface.present_mode = if settings.vsync {
        wgpu::PresentMode::AutoVsync
    } else {
        wgpu::PresentMode::AutoNoVsync
    };
    let backends = match settings.graphics_backend {
        GraphicsBackend::Auto => None, // wgpu picks; WGPU_BACKEND env var still works
        GraphicsBackend::Vulkan => Some(wgpu::Backends::VULKAN),
        GraphicsBackend::Dx12 => Some(wgpu::Backends::DX12),
        GraphicsBackend::OpenGl => Some(wgpu::Backends::GL),
    };
    if let (Some(backends), WgpuSetup::CreateNew(setup)) = (backends, &mut config.wgpu_setup) {
        setup.instance_descriptor.backends = backends;
    }
    config
}

/// The window could not open (usually a graphics driver problem). Explain it in
/// plain words and offer to try OpenGL next time — no terminal needed.
fn offer_graphics_fallback(mut settings: Settings, detail: &str) {
    let can_switch = settings.graphics_backend != GraphicsBackend::OpenGl;
    let mut text = String::from(
        "Ferrostep tidak bisa membuka jendela grafis. Biasanya ini karena driver kartu grafis.\n\n",
    );
    if can_switch {
        text.push_str("Coba mode grafis cadangan (OpenGL) saat aplikasi dibuka lagi?\n\n");
    } else {
        text.push_str("Perbarui driver kartu grafis, lalu coba lagi.\n\n");
    }
    text.push_str(&format!("Detail teknis: {detail}"));
    let dialog = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("Ferrostep")
        .set_description(text);
    if can_switch {
        let answer = dialog.set_buttons(rfd::MessageButtons::YesNo).show();
        if answer == rfd::MessageDialogResult::Yes {
            settings.graphics_backend = GraphicsBackend::OpenGl;
            if let Err(e) = settings.save() {
                log::error!("could not save settings: {e:#}");
            }
        }
    } else {
        dialog.set_buttons(rfd::MessageButtons::Ok).show();
    }
}

/// On a crash, write the details to `crash.log` in the config folder so a
/// non-technical user can send it along with a bug report.
fn install_crash_log() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_hook(info);
        let backtrace = std::backtrace::Backtrace::force_capture();
        let report = format!(
            "Ferrostep {} crashed\n{info}\n\n{backtrace}\n",
            env!("CARGO_PKG_VERSION")
        );
        log::error!("{info}");
        if let Ok(dir) = ferrostep::config::config_dir() {
            let _ = std::fs::write(dir.join("crash.log"), report);
        }
    }));
}

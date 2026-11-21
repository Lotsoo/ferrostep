//! Settings → Gameplay, Audio and Tampilan.

use super::*;

pub(super) fn gameplay_section(ui: &mut Ui, shared: &mut Shared) {
    let palette = shared.palette;
    let s = &mut shared.settings;
    section_title(ui, &palette, "Gameplay", "Berlaku mulai lagu berikutnya.");
    setting_row(
        ui,
        &palette,
        "Mode kecepatan",
        "CMod: kecepatan tetap (paling mudah dibaca). XMod: mengikuti BPM lagu.",
        |ui| {
            let mut i = usize::from(s.scroll_mode == ScrollMode::XMod);
            if widgets::segmented(ui, &palette, &["CMod", "XMod"], &mut i) {
                s.scroll_mode = if i == 0 {
                    ScrollMode::CMod
                } else {
                    ScrollMode::XMod
                };
            }
        },
    );
    match s.scroll_mode {
        ScrollMode::CMod => setting_row(
            ui,
            &palette,
            "Kecepatan panah",
            "Makin besar, panah makin cepat dan renggang. Pemula: 400–550.",
            |ui| {
                let mut v = f64::from(s.cmod);
                if widgets::slider(ui, &palette, &mut v, 150.0..=1200.0, 10.0, |v| {
                    format!("C{v:.0}")
                })
                .changed()
                {
                    s.cmod = v as f32;
                }
            },
        ),
        ScrollMode::XMod => setting_row(
            ui,
            &palette,
            "Pengali kecepatan",
            "Kecepatan = BPM lagu × pengali.",
            |ui| {
                let mut v = f64::from(s.xmod);
                if widgets::slider(ui, &palette, &mut v, 0.5..=8.0, 0.25, |v| {
                    format!("{v:.2}×")
                })
                .changed()
                {
                    s.xmod = v as f32;
                }
            },
        ),
    }
    setting_row(
        ui,
        &palette,
        "Arah terbalik (reverse)",
        "Panah turun ke receptor di bagian bawah.",
        |ui| {
            widgets::toggle(ui, &mut s.reverse);
        },
    );
    setting_row(
        ui,
        &palette,
        "Tingkat penilaian",
        "Seberapa ketat jendela waktu Marvelous…Boo.",
        |ui| {
            let mut i = JUDGE_SCALES
                .iter()
                .position(|v| (v - s.judge_scale).abs() < 1e-6)
                .unwrap_or(1);
            if widgets::segmented(ui, &palette, &["Longgar", "Normal", "Ketat"], &mut i) {
                s.judge_scale = JUDGE_SCALES[i];
            }
        },
    );
    setting_row(
        ui,
        &palette,
        "No Fail",
        "Lagu tidak berhenti walau life bar habis. Cocok untuk latihan.",
        |ui| {
            widgets::toggle(ui, &mut s.no_fail);
        },
    );
    setting_row(
        ui,
        &palette,
        "Kecerahan latar",
        "Latar lagu saat bermain. Lebih gelap = panah lebih jelas.",
        |ui| {
            let mut pct = f64::from(s.background_brightness) * 100.0;
            if widgets::slider(ui, &palette, &mut pct, 0.0..=100.0, 5.0, |v| {
                format!("{v:.0}%")
            })
            .changed()
            {
                s.background_brightness = (pct / 100.0) as f32;
            }
        },
    );
}

/// Returns true when the calibration wizard should open.
pub(super) fn audio_section(ui: &mut Ui, shared: &mut Shared) -> bool {
    let palette = shared.palette;
    let mut calibrate = false;
    let s = &mut shared.settings;
    section_title(ui, &palette, "Audio", "");
    setting_row(
        ui,
        &palette,
        "Volume",
        "Volume utama musik dan efek.",
        |ui| {
            let mut pct = f64::from(s.volume) * 100.0;
            if widgets::slider(ui, &palette, &mut pct, 0.0..=100.0, 1.0, |v| {
                format!("{v:.0}%")
            })
            .changed()
            {
                s.volume = (pct / 100.0) as f32;
            }
        },
    );
    setting_row(
        ui,
        &palette,
        "Efek suara menu",
        "Bunyi saat memilih, berpindah menu, dan memulai lagu. 0% = mati.",
        |ui| {
            let mut pct = f64::from(s.sfx_volume) * 100.0;
            if widgets::slider(ui, &palette, &mut pct, 0.0..=100.0, 1.0, |v| {
                format!("{v:.0}%")
            })
            .changed()
            {
                s.sfx_volume = (pct / 100.0) as f32;
            }
        },
    );
    setting_row(
        ui,
        &palette,
        "Global offset",
        "Positif bila suara terdengar terlambat. Headset Bluetooth biasanya +150 ms atau lebih.",
        |ui| {
            if widgets::secondary_button(ui, &palette, "+5", vec2(44.0, 32.0)).clicked() {
                s.global_offset_ms += 5.0;
            }
            widgets::slider(
                ui,
                &palette,
                &mut s.global_offset_ms,
                -300.0..=300.0,
                1.0,
                |v| format!("{v:+.0} ms"),
            );
            if widgets::secondary_button(ui, &palette, "−5", vec2(44.0, 32.0)).clicked() {
                s.global_offset_ms -= 5.0;
            }
        },
    );
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if widgets::primary_button(ui, "Kalibrasi offset", vec2(200.0, 44.0), true).clicked() {
            calibrate = true;
        }
        ui.label(
            egui::RichText::new("Suara terasa terlambat atau terlalu cepat? Ketuk mengikuti klik 16 kali (±20 detik).")
                .font(font(theme::TEXT_XS + 1.0))
                .color(palette.text_dim),
        );
    });
    if shared.audio.is_none() {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(
                "Perangkat audio tidak ditemukan. Cek speaker/headphone lalu buka ulang aplikasi.",
            )
            .color(DANGER),
        );
    }
    calibrate
}

pub(super) fn display_section(ui: &mut Ui, shared: &mut Shared) {
    let palette = shared.palette;
    let s = &mut shared.settings;
    section_title(ui, &palette, "Tampilan", "");
    setting_row(
        ui,
        &palette,
        "Tema",
        "Gelap nyaman untuk bermain; terang untuk ruangan cerah.",
        |ui| {
            let mut i = usize::from(s.theme == ThemeChoice::Light);
            if widgets::segmented(ui, &palette, &["Gelap", "Terang"], &mut i) {
                s.theme = if i == 0 {
                    ThemeChoice::Dark
                } else {
                    ThemeChoice::Light
                };
            }
        },
    );
    setting_row(
        ui,
        &palette,
        "Ukuran antarmuka",
        "Perbesar bila teks terlalu kecil di layar Anda.",
        |ui| {
            let mut pct = f64::from((s.ui_scale * 100.0).round());
            let resp = widgets::slider(ui, &palette, &mut pct, 90.0..=150.0, 10.0, |v| {
                format!("{v:.0}%")
            });
            // Apply when the drag ends so the slider does not jump under the mouse.
            if resp.drag_stopped() || (resp.changed() && !resp.dragged()) {
                s.ui_scale = (pct / 100.0) as f32;
            }
        },
    );
    setting_row(
        ui,
        &palette,
        "Ukuran jalur",
        "Besar panah, judgment, dan combo saat bermain. Otomatis = lebih besar saat dance mat terhubung (main dari jauh, mis. laptop ke TV). Kecepatan baca panah tetap sama.",
        |ui| {
            const SIZES: [LaneSize; 4] = [
                LaneSize::Auto,
                LaneSize::Normal,
                LaneSize::Large,
                LaneSize::Huge,
            ];
            let mut i = SIZES.iter().position(|v| *v == s.lane_size).unwrap_or(0);
            if widgets::segmented(
                ui,
                &palette,
                &["Otomatis", "Normal", "Besar", "Sangat besar"],
                &mut i,
            ) {
                s.lane_size = SIZES[i];
            }
        },
    );
    setting_row(
        ui,
        &palette,
        "Kurangi animasi",
        "Matikan gerakan dekoratif (transisi, partikel, denyut).",
        |ui| {
            widgets::toggle(ui, &mut s.reduce_motion);
        },
    );
    setting_row(
        ui,
        &palette,
        "Tampilkan FPS",
        "Angka frame per detik di pojok layar.",
        |ui| {
            widgets::toggle(ui, &mut s.show_fps);
        },
    );
    setting_row(
        ui,
        &palette,
        "VSync",
        "Matikan untuk latensi tampilan lebih rendah (bisa terjadi tearing). Berlaku setelah aplikasi dibuka ulang.",
        |ui| {
            widgets::toggle(ui, &mut s.vsync);
        },
    );
    setting_row(
        ui,
        &palette,
        "Backend grafis",
        "Ganti hanya bila layar hitam/berkedip. Berlaku setelah aplikasi dibuka ulang.",
        |ui| {
            let options = backend_options();
            let labels: Vec<&str> = options.iter().map(|(_, l)| *l).collect();
            let mut i = options
                .iter()
                .position(|(b, _)| *b == s.graphics_backend)
                .unwrap_or(0);
            if widgets::segmented(ui, &palette, &labels, &mut i) {
                s.graphics_backend = options[i].0;
            }
        },
    );
}

pub(super) fn backend_options() -> Vec<(GraphicsBackend, &'static str)> {
    let mut v = vec![
        (GraphicsBackend::Auto, "Otomatis"),
        (GraphicsBackend::Vulkan, "Vulkan"),
    ];
    if cfg!(windows) {
        v.push((GraphicsBackend::Dx12, "DirectX 12"));
    }
    v.push((GraphicsBackend::OpenGl, "OpenGL"));
    v
}

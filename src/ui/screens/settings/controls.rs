//! Settings → Kontrol: keyboard binds, connected dance mats and the mat help.

use super::*;

impl SettingsScreen {
    pub(super) fn controls_section(
        &mut self,
        ui: &mut Ui,
        shared: &mut Shared,
        now: Instant,
    ) -> Option<Nav> {
        let palette = shared.palette;
        let mut nav = None;
        section_title(
            ui,
            &palette,
            "Keyboard",
            "Klik kotak lalu tekan tombol baru. Esc untuk batal.",
        );
        for col in 0..4 {
            ui.horizontal(|ui| {
                ui.set_min_height(44.0);
                let (r, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
                notefield::draw_arrow(
                    ui.painter(),
                    r.center(),
                    15.0,
                    col as u8,
                    COLUMN_COLORS[col],
                    Stroke::NONE,
                );
                ui.add_sized(
                    vec2(70.0, 36.0),
                    egui::Label::new(
                        egui::RichText::new(COLUMN_NAMES[col]).font(font(theme::TEXT_S + 1.0)),
                    ),
                );
                for slot in 0..2 {
                    let name = shared.settings.keybinds[col].get(slot).cloned();
                    let capturing = self.capturing == Some((col, slot));
                    let label = if capturing {
                        "Tekan tombol…".to_string()
                    } else {
                        name.as_deref().map_or("—".to_string(), key_label)
                    };
                    let (r, resp) = ui.allocate_exact_size(vec2(150.0, 36.0), Sense::click());
                    let hover = ui.ctx().animate_bool(resp.id, resp.hovered() || capturing);
                    ui.painter()
                        .rect_filled(r, theme::RADIUS_BUTTON, palette.surface_hover);
                    ui.painter().rect_stroke(
                        r,
                        theme::RADIUS_BUTTON,
                        Stroke::new(1.0 + hover, palette.border.lerp_to_gamma(ACCENT_B, hover)),
                        StrokeKind::Inside,
                    );
                    ui.painter().text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        label,
                        font_bold(theme::TEXT_S),
                        if capturing { ACCENT_B } else { palette.text },
                    );
                    if resp.clicked() {
                        self.capturing = Some((col, slot));
                    }
                    if resp.secondary_clicked() && slot < shared.settings.keybinds[col].len() {
                        shared.settings.keybinds[col].remove(slot);
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            if widgets::secondary_button(ui, &palette, "Kembalikan default", vec2(180.0, 36.0))
                .clicked()
            {
                shared.settings.keybinds = Settings::default().keybinds;
                self.capturing = None;
            }
            ui.label(
                egui::RichText::new("Klik kanan untuk menghapus tombol.")
                    .font(font(theme::TEXT_XS))
                    .color(palette.text_dim),
            );
        });

        ui.add_space(20.0);
        section_title(
            ui,
            &palette,
            "Uji panel",
            "Injak panel atau tekan tombol. Panah menyala bila terbaca.",
        );
        let (strip, _) = ui.allocate_exact_size(vec2(4.0 * 84.0, 84.0), Sense::hover());
        for col in 0..4 {
            let c = strip.left_center() + vec2(42.0 + col as f32 * 84.0, 0.0);
            let fade = self.lit[col].map_or(0.0, |t| 1.0 - anim::progress(t, now, 0.4, false));
            let glow = if self.held[col] { 1.0 } else { fade };
            if glow > 0.0 {
                ui.ctx().request_repaint();
                notefield::draw_glow(ui.painter(), c, 40.0, COLUMN_COLORS[col], glow);
            }
            let stroke = Stroke::new(2.0, COLUMN_COLORS[col].gamma_multiply(0.8));
            notefield::draw_arrow(
                ui.painter(),
                c,
                30.0,
                col as u8,
                COLUMN_COLORS[col].gamma_multiply(0.2 + 0.8 * glow),
                stroke,
            );
        }

        ui.add_space(20.0);
        section_title(
            ui,
            &palette,
            "Dance mat",
            "Colok kapan saja: mat terdeteksi otomatis.",
        );
        if shared.connected_pads.is_empty() {
            ui.label(
                egui::RichText::new("Belum ada dance mat atau gamepad yang terhubung.")
                    .font(font(theme::TEXT_S))
                    .color(palette.text_dim),
            );
        }
        for (key, name) in shared.connected_pads.clone() {
            ui.horizontal(|ui| {
                ui.set_min_height(44.0);
                let (dot, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 5.0, SUCCESS);
                ui.label(egui::RichText::new(&name).font(font_bold(theme::TEXT_S + 1.0)));
                ui.add_space(12.0);
                if widgets::secondary_button(
                    ui,
                    &palette,
                    "Atur ulang dance mat",
                    vec2(200.0, 36.0),
                )
                .clicked()
                {
                    nav = Some(Nav::PadSetup {
                        key: key.clone(),
                        name: name.clone(),
                        from_settings: true,
                    });
                }
            });
        }
        setting_row(
            ui,
            &palette,
            "Debounce panel",
            "Abaikan sentuhan ganda dari mat murah. Naikkan bila satu injakan terbaca dua kali.",
            |ui| {
                widgets::slider(
                    ui,
                    &palette,
                    &mut shared.settings.gamepad_debounce_ms,
                    0.0..=60.0,
                    1.0,
                    |v| format!("{v:.0} ms"),
                );
            },
        );
        ui.add_space(8.0);
        let help = if self.show_mat_help {
            "▾ Dance mat tidak terbaca?"
        } else {
            "▸ Dance mat tidak terbaca?"
        };
        if ui
            .add(
                egui::Label::new(
                    egui::RichText::new(help)
                        .font(font_bold(theme::TEXT_S))
                        .color(ACCENT_B),
                )
                .sense(Sense::click()),
            )
            .clicked()
        {
            self.show_mat_help = !self.show_mat_help;
        }
        if self.show_mat_help {
            for line in MAT_HELP {
                ui.label(
                    egui::RichText::new(*line)
                        .font(font(theme::TEXT_S))
                        .color(palette.text_dim),
                );
            }
        }
        nav
    }
}

#[cfg(windows)]
pub(super) const MAT_HELP: &[&str] = &[
    "1. Pastikan mat berlabel \"USB/PC\". Mat khusus TV (colokan AV) tidak bisa dipakai.",
    "2. Colok langsung ke port USB laptop/PC (hindari hub tanpa daya).",
    "3. Buka \"Set up USB game controllers\" (ketik joy.cpl di Start). Jika mat muncul di sana, Ferrostep bisa membacanya.",
    "4. Beberapa mat punya tombol mode (PC/TV atau analog). Coba tekan Select+Start bersamaan.",
    "5. Cabut lalu colok lagi: Ferrostep mendeteksi mat tanpa perlu dibuka ulang.",
];
#[cfg(not(windows))]
pub(super) const MAT_HELP: &[&str] = &[
    "1. Pastikan mat berlabel \"USB/PC\". Mat khusus TV (colokan AV) tidak bisa dipakai.",
    "2. Colok langsung ke port USB (hindari hub tanpa daya), lalu tunggu 2–3 detik.",
    "3. Cek di terminal: evtest — mat harus muncul di daftar perangkat input.",
    "4. Jika evtest butuh sudo agar mat terbaca, mat tidak dikenali sebagai joystick oleh sistem. Laporkan nama mat (dari evtest) agar bisa dibuatkan aturan udev.",
    "5. Beberapa mat punya tombol mode (PC/TV atau analog). Coba tekan Select+Start bersamaan.",
];

/// Friendly name for a stored egui key name ("ArrowLeft" → "←").
pub(super) fn key_label(name: &str) -> String {
    match Key::from_name(name) {
        Some(Key::ArrowLeft) => "← Panah kiri".into(),
        Some(Key::ArrowDown) => "↓ Panah bawah".into(),
        Some(Key::ArrowUp) => "↑ Panah atas".into(),
        Some(Key::ArrowRight) => "→ Panah kanan".into(),
        Some(Key::Space) => "Spasi".into(),
        Some(k) => k.symbol_or_name().to_string(),
        None => name.to_string(),
    }
}

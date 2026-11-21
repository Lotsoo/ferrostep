//! Settings → Folder: songs folder and ffmpeg location.

use super::*;

impl SettingsScreen {
    pub(super) fn folders_section(&mut self, ui: &mut Ui, shared: &mut Shared) {
        let palette = shared.palette;
        let root = shared.library.root.clone();
        section_title(
            ui,
            &palette,
            "Folder lagu",
            "Tempat lagu (.sm/.ssc) dan hasil impor disimpan.",
        );
        // Long paths wrap instead of running out of the card.
        ui.add(
            egui::Label::new(
                egui::RichText::new(root.display().to_string())
                    .font(font(theme::TEXT_S))
                    .color(palette.text),
            )
            .wrap_mode(egui::TextWrapMode::Wrap),
        );
        ui.label(
            egui::RichText::new(format!("{} lagu ditemukan", shared.library.songs.len()))
                .font(font(theme::TEXT_XS))
                .color(palette.text_dim),
        );
        ui.horizontal(|ui| {
            if widgets::secondary_button(ui, &palette, "Buka folder", vec2(140.0, 36.0)).clicked() {
                let _ = std::fs::create_dir_all(&root);
                if let Err(e) = open_in_file_manager(&root) {
                    shared
                        .toasts
                        .warning(format!("Tidak bisa membuka pengelola file: {e}"));
                }
            }
            if widgets::secondary_button(ui, &palette, "Ganti folder…", vec2(150.0, 36.0)).clicked()
                && self.picker.is_none()
            {
                let (tx, rx) = crossbeam_channel::bounded(1);
                let ctx = ui.ctx().clone();
                let start = root.clone();
                let _ = std::thread::Builder::new()
                    .name("folder-dialog".into())
                    .spawn(move || {
                        let _ = tx.send(
                            rfd::FileDialog::new()
                                .set_title("Pilih folder lagu")
                                .set_directory(start)
                                .pick_folder(),
                        );
                        ctx.request_repaint();
                    });
                self.picker = Some(Picker::SongsFolder(rx));
            }
            if shared.settings.songs_dir.is_some()
                && widgets::secondary_button(ui, &palette, "Pakai default", vec2(140.0, 36.0))
                    .clicked()
            {
                shared.settings.songs_dir = None;
                reload_library(shared, ui.ctx());
            }
            if widgets::secondary_button(ui, &palette, "Pindai ulang", vec2(130.0, 36.0)).clicked()
            {
                shared.library.rescan();
            }
        });

        ui.add_space(20.0);
        section_title(
            ui,
            &palette,
            "ffmpeg (opsional)",
            "Membuat audio hasil impor dari video lebih kecil (.ogg). Tanpa ffmpeg tetap bisa impor.",
        );
        match ffmpeg::find_ffmpeg(shared.settings.ffmpeg_path.as_deref()) {
            Some(p) => ui.add(
                egui::Label::new(
                    egui::RichText::new(format!("✓ Ditemukan: {}", p.display()))
                        .font(font(theme::TEXT_S))
                        .color(SUCCESS),
                )
                .wrap_mode(egui::TextWrapMode::Wrap),
            ),
            None => ui.label(
                egui::RichText::new("ⓘ Tidak ditemukan (tidak masalah).")
                    .font(font(theme::TEXT_S))
                    .color(palette.text_dim),
            ),
        };
        ui.horizontal(|ui| {
            if widgets::secondary_button(ui, &palette, "Pilih file ffmpeg…", vec2(170.0, 36.0))
                .clicked()
                && self.picker.is_none()
            {
                let (tx, rx) = crossbeam_channel::bounded(1);
                let ctx = ui.ctx().clone();
                let _ = std::thread::Builder::new()
                    .name("ffmpeg-dialog".into())
                    .spawn(move || {
                        let _ = tx.send(
                            rfd::FileDialog::new()
                                .set_title("Pilih program ffmpeg")
                                .pick_file(),
                        );
                        ctx.request_repaint();
                    });
                self.picker = Some(Picker::Ffmpeg(rx));
            }
            if shared.settings.ffmpeg_path.is_some()
                && widgets::secondary_button(ui, &palette, "Cari otomatis", vec2(140.0, 36.0))
                    .clicked()
            {
                shared.settings.ffmpeg_path = None;
            }
        });
        ui.label(
            egui::RichText::new(FFMPEG_HELP)
                .font(font(theme::TEXT_XS + 1.0))
                .color(palette.text_dim),
        );
    }
}

#[cfg(windows)]
pub(super) const FFMPEG_HELP: &str =
    "Windows: unduh ffmpeg (build \"essentials\"), lalu taruh ffmpeg.exe di samping ferrostep.exe.";
#[cfg(not(windows))]
pub(super) const FFMPEG_HELP: &str =
    "Fedora: sudo dnf install ffmpeg-free  (atau versi lengkap dari RPM Fusion).";

/// Replaces the library after the songs folder changed.
pub(super) fn reload_library(shared: &mut Shared, ctx: &egui::Context) {
    let root = songs_dir(&shared.settings);
    if let Err(e) = std::fs::create_dir_all(&root) {
        shared
            .toasts
            .warning(format!("Folder tidak bisa dipakai: {e}"));
        return;
    }
    shared.library = Library::new(root, ctx);
    shared.toasts.success("Folder lagu diganti.");
}

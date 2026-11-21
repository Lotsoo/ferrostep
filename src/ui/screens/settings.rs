//! Settings with a category sidebar. Every change is saved automatically.

use std::path::PathBuf;
use std::time::Instant;

use crossbeam_channel::Receiver;
use eframe::egui::{self, Align2, Key, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2};

use super::Nav;
use super::calibration::{Calibration, CalibrationEvent};
use crate::app::Shared;
use crate::config::{GraphicsBackend, LaneSize, ScrollMode, Settings, ThemeChoice};
use crate::importer::ffmpeg;
use crate::input::{FrameInput, MenuAction};
use crate::library::{Library, songs_dir};
use crate::ui::notefield;
use crate::ui::theme::{self, ACCENT_B, COLUMN_COLORS, DANGER, SUCCESS, font, font_bold};
use crate::ui::{anim, open_in_file_manager, widgets};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Gameplay,
    Audio,
    Display,
    Controls,
    Folders,
}

impl Category {
    const ALL: [Category; 5] = [
        Category::Gameplay,
        Category::Audio,
        Category::Display,
        Category::Controls,
        Category::Folders,
    ];

    fn label(self) -> &'static str {
        match self {
            Category::Gameplay => "Gameplay",
            Category::Audio => "Audio",
            Category::Display => "Tampilan",
            Category::Controls => "Kontrol",
            Category::Folders => "Folder",
        }
    }

    /// Accent per category, taken from the arrow colors (plus violet).
    fn color(self) -> egui::Color32 {
        match self {
            Category::Gameplay => COLUMN_COLORS[0],
            Category::Audio => COLUMN_COLORS[1],
            Category::Display => COLUMN_COLORS[2],
            Category::Controls => COLUMN_COLORS[3],
            Category::Folders => theme::ACCENT_A,
        }
    }

    /// Arrow direction drawn as the category icon (column index).
    fn arrow(self) -> u8 {
        match self {
            Category::Gameplay => 0,
            Category::Audio => 1,
            Category::Display => 2,
            Category::Controls => 3,
            Category::Folders => 1,
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Category::Gameplay => "Kecepatan, arah, penilaian",
            Category::Audio => "Volume dan sinkronisasi",
            Category::Display => "Tema, ukuran, animasi",
            Category::Controls => "Keyboard dan dance mat",
            Category::Folders => "Lokasi lagu dan ffmpeg",
        }
    }
}

const COLUMN_NAMES: [&str; 4] = ["Kiri", "Bawah", "Atas", "Kanan"];
const JUDGE_SCALES: [f64; 3] = [1.5, 1.0, 0.75];

enum Picker {
    SongsFolder(Receiver<Option<PathBuf>>),
    Ffmpeg(Receiver<Option<PathBuf>>),
}

pub struct SettingsScreen {
    category: Category,
    /// Column and slot waiting for a key press.
    capturing: Option<(usize, usize)>,
    calibration: Option<Calibration>,
    picker: Option<Picker>,
    /// When each panel was last pressed / whether it is held (live test).
    lit: [Option<Instant>; 4],
    held: [bool; 4],
    show_mat_help: bool,
}

impl SettingsScreen {
    /// True while the offset calibration is running (it needs silence).
    pub fn is_calibrating(&self) -> bool {
        self.calibration.is_some()
    }

    pub fn new(category: Option<Category>) -> Self {
        Self {
            category: category.unwrap_or(Category::Gameplay),
            capturing: None,
            calibration: None,
            picker: None,
            lit: [None; 4],
            held: [false; 4],
            show_mat_help: false,
        }
    }

    /// Opens straight into the calibration wizard (from the results screen).
    pub fn calibrating() -> Self {
        Self {
            category: Category::Audio,
            calibration: Some(Calibration::default()),
            ..Self::new(None)
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, shared: &mut Shared, input: &FrameInput) -> Option<Nav> {
        let now = input.now;
        for c in &input.columns {
            let i = usize::from(c.column.min(3));
            self.held[i] = c.pressed;
            if c.pressed {
                self.lit[i] = Some(c.at);
            }
        }
        self.poll_picker(shared, ui.ctx());

        let rect = ui.max_rect();
        let palette = shared.palette;
        if palette.dark {
            // Calm stage: the floor moves slowly, without flashes.
            let beat = crate::ui::beatfx::Beat::idle(anim::since(shared.start, now)).calm();
            crate::ui::beatfx::draw_highway(ui.painter(), rect, self.category.color(), &beat);
            if !shared.settings.reduce_motion {
                ui.ctx().request_repaint();
            }
        }

        // Calibration takes the whole content area.
        if let Some(cal) = &mut self.calibration {
            let area = rect.shrink2(vec2(48.0, 40.0));
            widgets::text_tracked(
                ui.painter(),
                area.left_top(),
                Align2::LEFT_TOP,
                "Kalibrasi offset",
                font_bold(theme::TEXT_XL),
                palette.text,
            );
            let event = cal.ui(ui, area.with_min_y(area.top() + 60.0), shared, input);
            let esc = input.has(MenuAction::Back);
            match event {
                Some(CalibrationEvent::Accepted(ms)) => {
                    shared.settings.global_offset_ms = ms;
                    shared
                        .toasts
                        .success(format!("Global offset diatur ke {ms:+.0} ms"));
                    self.calibration = None;
                }
                Some(CalibrationEvent::Cancelled) => self.calibration = None,
                None if esc => self.calibration = None,
                None => {}
            }
            return None;
        }

        // Key capture swallows the next key press.
        if let Some((col, slot)) = self.capturing {
            ui.ctx().request_repaint();
            if let Some(key) = input.last_key {
                if key != Key::Escape {
                    let name = key.name().to_string();
                    for binds in shared.settings.keybinds.iter_mut() {
                        binds.retain(|n| n != &name);
                    }
                    let binds = &mut shared.settings.keybinds[col];
                    if slot < binds.len() {
                        binds[slot] = name;
                    } else {
                        binds.push(name);
                    }
                }
                self.capturing = None;
            }
        } else if !input.typing {
            for a in &input.menu {
                let idx = Category::ALL
                    .iter()
                    .position(|c| *c == self.category)
                    .unwrap_or(0);
                match a {
                    MenuAction::Up => self.category = Category::ALL[(idx + 4) % 5],
                    MenuAction::Down => self.category = Category::ALL[(idx + 1) % 5],
                    MenuAction::Back => return Some(Nav::Title),
                    _ => {}
                }
            }
        }

        // Header
        let back = Rect::from_min_size(rect.left_top() + vec2(32.0, 28.0), vec2(120.0, 40.0));
        if ui
            .put(back, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, "‹  Kembali", back.size())
            })
            .clicked()
        {
            return Some(Nav::Title);
        }
        widgets::text_tracked(
            ui.painter(),
            pos2(back.right() + 24.0, back.center().y),
            Align2::LEFT_CENTER,
            "Pengaturan",
            font_bold(theme::TEXT_XL),
            palette.text,
        );
        ui.painter().text(
            pos2(rect.right() - 32.0, back.center().y),
            Align2::RIGHT_CENTER,
            "Tersimpan otomatis",
            font(theme::TEXT_XS + 1.0),
            SUCCESS.gamma_multiply(0.8),
        );

        // Sidebar
        let body = Rect::from_min_max(
            pos2(rect.left() + 32.0, back.bottom() + 24.0),
            rect.max - vec2(32.0, 24.0),
        );
        let side = Rect::from_min_size(body.min, vec2(256.0, body.height()));
        widgets::card(ui.painter(), side, &palette);
        for (i, cat) in Category::ALL.iter().enumerate() {
            let r = Rect::from_min_size(
                side.min + vec2(10.0, 10.0 + i as f32 * 64.0),
                vec2(side.width() - 20.0, 58.0),
            );
            let resp = ui.interact(r, ui.scope_id().with(("cat", i)), Sense::click());
            if resp.clicked() {
                self.category = *cat;
                self.capturing = None;
            }
            let sel = self.category == *cat;
            let t = ui.ctx().animate_bool(resp.id, sel || resp.hovered());
            ui.painter().rect_filled(
                r,
                theme::RADIUS_BUTTON,
                palette.surface_hover.gamma_multiply(t),
            );
            let color = cat.color();
            if sel {
                ui.painter().rect_filled(
                    Rect::from_min_size(r.min + vec2(0.0, 12.0), vec2(3.0, r.height() - 24.0)),
                    2,
                    color,
                );
            }
            // Arrow icon in the category color (filled when selected).
            let icon = r.left_center() + vec2(26.0, 0.0);
            if sel {
                notefield::draw_glow(ui.painter(), icon, 22.0, color, 0.5);
            }
            notefield::draw_arrow(
                ui.painter(),
                icon,
                26.0,
                cat.arrow(),
                color.gamma_multiply(if sel { 1.0 } else { 0.25 + 0.3 * t }),
                Stroke::new(1.2, color),
            );
            // Text never leaves the sidebar: shorten with "…" if needed.
            let text_w = r.width() - 50.0 - 12.0;
            ui.painter().text(
                r.left_top() + vec2(50.0, 10.0),
                Align2::LEFT_TOP,
                widgets::elide(
                    ui.painter(),
                    cat.label(),
                    font_bold(theme::TEXT_S + 1.0),
                    text_w,
                ),
                font_bold(theme::TEXT_S + 1.0),
                palette.text,
            );
            ui.painter().text(
                r.left_top() + vec2(50.0, 32.0),
                Align2::LEFT_TOP,
                widgets::elide(ui.painter(), cat.hint(), font(theme::TEXT_XS), text_w),
                font(theme::TEXT_XS),
                palette.text_dim,
            );
        }

        // Content
        let content = Rect::from_min_max(pos2(side.right() + 24.0, body.top()), body.max);
        widgets::card(ui.painter(), content, &palette);
        ui.painter().rect_filled(
            Rect::from_min_size(
                content.left_top() + vec2(24.0, 0.0),
                vec2(content.width() - 48.0, 3.0),
            ),
            2,
            self.category.color(),
        );
        let mut nav = None;
        let mut child = ui.new_child(UiBuilder::new().max_rect(content.shrink(28.0)));
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                match self.category {
                    Category::Gameplay => gameplay_section(ui, shared),
                    Category::Audio => {
                        if audio_section(ui, shared) {
                            self.calibration = Some(Calibration::default());
                        }
                    }
                    Category::Display => display_section(ui, shared),
                    Category::Controls => nav = self.controls_section(ui, shared, now),
                    Category::Folders => self.folders_section(ui, shared),
                }
            });
        nav
    }

    fn poll_picker(&mut self, shared: &mut Shared, ctx: &egui::Context) {
        let Some(picker) = &self.picker else { return };
        match picker {
            Picker::SongsFolder(rx) => {
                if let Ok(choice) = rx.try_recv() {
                    self.picker = None;
                    if let Some(dir) = choice {
                        shared.settings.songs_dir = Some(dir);
                        reload_library(shared, ctx);
                    }
                }
            }
            Picker::Ffmpeg(rx) => {
                if let Ok(choice) = rx.try_recv() {
                    self.picker = None;
                    if let Some(file) = choice {
                        shared.settings.ffmpeg_path = Some(file);
                    }
                }
            }
        }
    }

    fn controls_section(&mut self, ui: &mut Ui, shared: &mut Shared, now: Instant) -> Option<Nav> {
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

    fn folders_section(&mut self, ui: &mut Ui, shared: &mut Shared) {
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
const FFMPEG_HELP: &str =
    "Windows: unduh ffmpeg (build \"essentials\"), lalu taruh ffmpeg.exe di samping ferrostep.exe.";
#[cfg(not(windows))]
const FFMPEG_HELP: &str =
    "Fedora: sudo dnf install ffmpeg-free  (atau versi lengkap dari RPM Fusion).";

#[cfg(windows)]
const MAT_HELP: &[&str] = &[
    "1. Pastikan mat berlabel \"USB/PC\". Mat khusus TV (colokan AV) tidak bisa dipakai.",
    "2. Colok langsung ke port USB laptop/PC (hindari hub tanpa daya).",
    "3. Buka \"Set up USB game controllers\" (ketik joy.cpl di Start). Jika mat muncul di sana, Ferrostep bisa membacanya.",
    "4. Beberapa mat punya tombol mode (PC/TV atau analog). Coba tekan Select+Start bersamaan.",
    "5. Cabut lalu colok lagi: Ferrostep mendeteksi mat tanpa perlu dibuka ulang.",
];
#[cfg(not(windows))]
const MAT_HELP: &[&str] = &[
    "1. Pastikan mat berlabel \"USB/PC\". Mat khusus TV (colokan AV) tidak bisa dipakai.",
    "2. Colok langsung ke port USB (hindari hub tanpa daya), lalu tunggu 2–3 detik.",
    "3. Cek di terminal: evtest — mat harus muncul di daftar perangkat input.",
    "4. Jika evtest butuh sudo agar mat terbaca, mat tidak dikenali sebagai joystick oleh sistem. Laporkan nama mat (dari evtest) agar bisa dibuatkan aturan udev.",
    "5. Beberapa mat punya tombol mode (PC/TV atau analog). Coba tekan Select+Start bersamaan.",
];

/// Replaces the library after the songs folder changed.
fn reload_library(shared: &mut Shared, ctx: &egui::Context) {
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

/// Friendly name for a stored egui key name ("ArrowLeft" → "←").
fn key_label(name: &str) -> String {
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

fn section_title(ui: &mut Ui, palette: &theme::Palette, title: &str, subtitle: &str) {
    ui.label(
        egui::RichText::new(title)
            .font(font_bold(theme::TEXT_M))
            .color(palette.text),
    );
    ui.label(
        egui::RichText::new(subtitle)
            .font(font(theme::TEXT_XS + 1.0))
            .color(palette.text_dim),
    );
    ui.add_space(6.0);
}

/// Label + description on the left, control on the right.
fn setting_row(
    ui: &mut Ui,
    palette: &theme::Palette,
    title: &str,
    description: &str,
    control: impl FnOnce(&mut Ui),
) {
    let width = ui.available_width();
    ui.horizontal(|ui| {
        ui.set_min_height(56.0);
        ui.vertical(|ui| {
            ui.set_width((width * 0.5).max(220.0));
            ui.label(
                egui::RichText::new(title)
                    .font(font_bold(theme::TEXT_S + 1.0))
                    .color(palette.text),
            );
            ui.label(
                egui::RichText::new(description)
                    .font(font(theme::TEXT_XS + 1.0))
                    .color(palette.text_dim),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
    });
    let y = ui.cursor().top();
    ui.painter().line_segment(
        [
            pos2(ui.min_rect().left(), y),
            pos2(ui.min_rect().left() + width, y),
        ],
        Stroke::new(1.0, palette.border),
    );
    ui.add_space(4.0);
}

fn gameplay_section(ui: &mut Ui, shared: &mut Shared) {
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
fn audio_section(ui: &mut Ui, shared: &mut Shared) -> bool {
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

fn display_section(ui: &mut Ui, shared: &mut Shared) {
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

fn backend_options() -> Vec<(GraphicsBackend, &'static str)> {
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

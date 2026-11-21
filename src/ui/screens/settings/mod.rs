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

mod controls;
mod folders;
mod sections;

use folders::reload_library;
use sections::{audio_section, display_section, gameplay_section};

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

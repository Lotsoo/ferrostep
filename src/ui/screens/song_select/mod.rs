//! Song select: searchable card list, details panel, difficulty chips, audio preview.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use eframe::egui::{self, Align2, Color32, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::{Nav, PlayRequest, difficulty_color, difficulty_label};
use crate::app::Shared;
use crate::audio::Music;
use crate::input::{FrameInput, MenuAction};
use crate::library::scanner::SongEntry;
use crate::library::scores::chart_key;
use crate::parser::Difficulty;
use crate::ui::theme::{self, ACCENT_B, font, font_bold};
use crate::ui::{anim, beatfx, widgets};

mod delete;
mod view;

use delete::DeleteDialog;

/// Wait this long on a song before starting its preview.
const PREVIEW_DELAY: f32 = 0.35;
const CARD_H: f32 = 76.0;
const CARD_GAP: f32 = 10.0;

struct Preview {
    music: Music,
    dir: PathBuf,
    started: Instant,
    length: f32,
    /// The song's timing, so menus can pulse on its beat.
    map: crate::timing::TimingMap,
}

pub struct SongSelectScreen {
    delete_dialog: Option<DeleteDialog>,
    /// Confirmed deletion waiting for the preview to let go of the file.
    pending_delete: Option<(PathBuf, String, Instant)>,
    query: String,
    filtered: Vec<usize>,
    selected: usize,
    selected_dir: Option<PathBuf>,
    /// Preferred difficulty, kept when moving between songs.
    difficulty: Difficulty,
    generation: u64,
    changed_at: Instant,
    preview: Option<Preview>,
    scroll_to_selected: bool,
    /// List scroll offset (px) and the sliding focus highlight (px), both springs.
    scroll: anim::Spring,
    focus: anim::Spring,
    /// Scroll value when a drag started; `None` when not dragging.
    drag_base: Option<f32>,
    /// Pointer movement since the drag started (px).
    drag_travel: f32,
    /// First frame snaps instead of animating in from 0.
    list_ready: bool,
    focus_search: bool,
    tint: Color32,
    /// Beat of the preview music (updated every frame).
    beat: beatfx::Beat,
}

impl Default for SongSelectScreen {
    fn default() -> Self {
        Self {
            delete_dialog: None,
            pending_delete: None,
            query: String::new(),
            filtered: Vec::new(),
            selected: 0,
            selected_dir: None,
            difficulty: Difficulty::Easy,
            generation: u64::MAX,
            changed_at: Instant::now(),
            preview: None,
            scroll_to_selected: true,
            scroll: anim::Spring::new(0.0, anim::Spring::SMOOTH),
            focus: anim::Spring::new(0.0, anim::Spring::SNAPPY),
            drag_base: None,
            drag_travel: 0.0,
            list_ready: false,
            focus_search: true,
            beat: beatfx::Beat::idle(0.0),
            tint: theme::ACCENT_A,
        }
    }
}

impl SongSelectScreen {
    /// Opens with a specific song selected (e.g. after importing it).
    pub fn with_selected(dir: PathBuf) -> Self {
        Self {
            selected_dir: Some(dir),
            ..Self::default()
        }
    }

    fn refilter(&mut self, songs: &[Arc<SongEntry>]) {
        let q = self.query.trim().to_lowercase();
        self.filtered = songs
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                q.is_empty() || q.split_whitespace().all(|w| s.search_text().contains(w))
            })
            .map(|(i, _)| i)
            .collect();
        self.selected = self
            .selected_dir
            .as_ref()
            .and_then(|dir| self.filtered.iter().position(|&i| &songs[i].dir == dir))
            .unwrap_or(0);
        self.scroll_to_selected = true;
    }

    fn current<'a>(&self, songs: &'a [Arc<SongEntry>]) -> Option<&'a Arc<SongEntry>> {
        self.filtered.get(self.selected).and_then(|&i| songs.get(i))
    }

    fn select(&mut self, index: usize, songs: &[Arc<SongEntry>]) {
        if self.filtered.is_empty() {
            return;
        }
        let index = index.min(self.filtered.len() - 1);
        if index != self.selected || self.selected_dir.is_none() {
            self.selected = index;
            self.changed_at = Instant::now();
            self.scroll_to_selected = true;
        }
        self.selected_dir = self.current(songs).map(|s| s.dir.clone());
    }

    /// Index into `song.playable` closest to the preferred difficulty.
    fn chart_slot(&self, song: &SongEntry) -> usize {
        let charts = &song.simfile.charts;
        song.playable
            .iter()
            .position(|&i| charts[i].difficulty == self.difficulty)
            .or_else(|| {
                song.playable
                    .iter()
                    .position(|&i| charts[i].difficulty > self.difficulty)
            })
            .unwrap_or(song.playable.len().saturating_sub(1))
    }

    fn play_request(&self, song: &SongEntry) -> PlayRequest {
        PlayRequest {
            simfile: song.simfile.clone(),
            song_dir: song.dir.clone(),
            chart_index: song.playable[self.chart_slot(song)],
        }
    }

    fn update_preview(&mut self, shared: &mut Shared, song: Option<&Arc<SongEntry>>, now: Instant) {
        let Some(song) = song else {
            if let Some(p) = &mut self.preview {
                p.music.fade_out(300);
            }
            self.preview = None;
            return;
        };
        let wanted = &song.dir;
        if self
            .pending_delete
            .as_ref()
            .is_some_and(|(dir, ..)| dir == wanted)
        {
            return;
        }
        let same = self.preview.as_ref().is_some_and(|p| &p.dir == wanted);
        if !same {
            if let Some(mut p) = self.preview.take() {
                p.music.fade_out(300);
            }
            if anim::since(self.changed_at, now) < PREVIEW_DELAY {
                return;
            }
            self.start_preview(shared, song, now);
        } else if let Some(p) = &mut self.preview
            && anim::since(p.started, now) > p.length
        {
            p.music.fade_out(800);
            if anim::since(p.started, now) > p.length + 1.2 {
                self.preview = None;
                self.start_preview(shared, song, now);
            }
        }
    }

    fn start_preview(&mut self, shared: &mut Shared, song: &SongEntry, now: Instant) {
        let (Some(audio), Some(path)) = (shared.audio.as_mut(), song.music.as_ref()) else {
            return;
        };
        let meta = &song.simfile.meta;
        let start = if meta.sample_start > 0.0 {
            meta.sample_start
        } else {
            song.length * 0.3
        };
        let length = if meta.sample_length > 1.0 {
            meta.sample_length
        } else {
            15.0
        };
        match audio.play_music(path, start, 600) {
            Ok(music) => {
                self.preview = Some(Preview {
                    music,
                    dir: song.dir.clone(),
                    started: now,
                    length: length as f32,
                    map: crate::timing::TimingMap::new(&song.simfile.timing),
                })
            }
            Err(e) => log::warn!("preview failed: {e:#}"),
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, shared: &mut Shared, input: &FrameInput) -> Option<Nav> {
        let now = input.now;
        // Before the refresh below, so the visible list is rebuilt in the
        // same frame the song disappears (stale indices would be out of range).
        self.run_pending_delete(shared, now);
        if shared.library.generation != self.generation {
            self.generation = shared.library.generation;
            if let Some((dir, at)) = &shared.library.highlight
                && anim::since(*at, now) < 5.0
            {
                self.selected_dir = Some(dir.clone());
            }
            let songs = shared.library.songs.clone();
            self.refilter(&songs);
            self.select(self.selected, &songs);
        }
        let songs = shared.library.songs.clone();

        // ── Input ───────────────────────────────────────────────
        let mut nav = None;
        // Delete key opens the confirmation (only when it is not editing the
        // search text).
        if self.delete_dialog.is_none()
            && self.query.is_empty()
            && ui.input(|i| i.key_pressed(egui::Key::Delete))
            && let Some(song) = self.current(&songs).cloned()
        {
            self.ask_delete(&song, now);
        }
        let menu: &[MenuAction] = if self.delete_dialog.is_some() {
            &[]
        } else {
            &input.menu
        };
        for action in menu {
            match action {
                MenuAction::Up => self.select(self.selected.saturating_sub(1), &songs),
                MenuAction::Down => self.select(self.selected + 1, &songs),
                MenuAction::Left | MenuAction::Right => {
                    if let Some(song) = self.current(&songs).cloned() {
                        let slot = self.chart_slot(&song);
                        let next = if *action == MenuAction::Left {
                            slot.saturating_sub(1)
                        } else {
                            (slot + 1).min(song.playable.len() - 1)
                        };
                        self.difficulty = song.simfile.charts[song.playable[next]].difficulty;
                    }
                }
                MenuAction::Confirm => {
                    nav = self
                        .current(&songs)
                        .map(|s| Nav::Play(self.play_request(s)))
                }
                MenuAction::Back => nav = Some(Nav::Title),
            }
        }
        if nav.is_some() {
            return nav;
        }

        let current = self.current(&songs).cloned();
        self.update_preview(shared, current.as_ref(), now);
        if self.preview.is_none()
            || self.tint
                != current
                    .as_ref()
                    .map_or(theme::ACCENT_A, |s| widgets::color_from_hash(&s.title()))
        {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        let target = current
            .as_ref()
            .map_or(theme::ACCENT_A, |s| widgets::color_from_hash(&s.title()));
        self.tint = self.tint.lerp_to_gamma(target, 0.12);
        if self.tint != target {
            ui.ctx().request_repaint();
        }

        let rect = ui.max_rect().shrink2(vec2(32.0, 24.0));
        let palette = shared.palette;
        let reduce = shared.settings.reduce_motion;
        self.beat = match &self.preview {
            Some(p) if !p.music.is_stopped() => {
                beatfx::Beat::from_music(&p.map, p.music.position())
            }
            _ => beatfx::Beat::idle(anim::since(shared.start, now)),
        };
        if reduce {
            self.beat = self.beat.calm();
        }
        ui.ctx().request_repaint();
        widgets::background(ui.painter(), ui.max_rect(), &palette, 0.0, Some(self.tint));
        if palette.dark {
            beatfx::draw_highway(ui.painter(), ui.max_rect(), self.tint, &self.beat);
        }

        // ── Header ──────────────────────────────────────────────
        let header = Rect::from_min_size(rect.min, vec2(rect.width(), 56.0));
        // Always-visible way back (not everyone reads the hints at the bottom).
        let back = Rect::from_min_size(header.left_center() - vec2(0.0, 20.0), vec2(120.0, 40.0));
        if ui
            .put(back, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, "‹  Kembali", back.size())
            })
            .on_hover_text("Kembali ke menu utama (Esc)")
            .clicked()
        {
            return Some(Nav::Title);
        }
        let title_x = back.right() + 24.0;
        widgets::text_tracked(
            ui.painter(),
            pos2(title_x, header.center().y),
            Align2::LEFT_CENTER,
            "Pilih Lagu",
            font_bold(theme::TEXT_XL),
            palette.text,
        );
        let count = if shared.library.loading && songs.is_empty() {
            "Memindai…".to_string()
        } else {
            format!("{} lagu", self.filtered.len())
        };
        ui.painter().text(
            pos2(title_x + 190.0, header.center().y + 4.0),
            Align2::LEFT_CENTER,
            count,
            font(theme::TEXT_S),
            palette.text_dim,
        );
        let search_rect = Rect::from_min_size(
            pos2(header.right() - 360.0, header.top() + 8.0),
            vec2(360.0, 40.0),
        );
        let search = ui.put(search_rect, |ui: &mut Ui| {
            egui::TextEdit::singleline(&mut self.query)
                .hint_text("🔎  Ketik untuk mencari judul atau artis…")
                .desired_width(search_rect.width())
                .margin(egui::Margin::symmetric(12, 10))
                .show(ui)
                .response
                .response
        });
        if self.focus_search {
            search.request_focus();
            self.focus_search = false;
        }
        if search.changed() {
            self.refilter(&songs);
            self.select(0, &songs);
        }

        let body = Rect::from_min_max(
            pos2(rect.left(), header.bottom() + 16.0),
            pos2(rect.right(), rect.bottom() - 28.0),
        );
        if songs.is_empty() {
            if shared.library.loading || !shared.library.loaded_once {
                self.draw_skeleton(ui, body, &palette, anim::since(shared.start, now));
                ui.ctx().request_repaint();
            } else {
                nav = self.draw_empty(ui, body, shared);
            }
            return nav;
        }

        let list_rect = Rect::from_min_max(
            body.min,
            pos2(body.left() + body.width() * 0.52, body.bottom()),
        );
        let detail_rect = Rect::from_min_max(pos2(list_rect.right() + 24.0, body.top()), body.max);

        if let Some(n) = self.draw_list(ui, list_rect, shared, &songs, now) {
            return Some(n);
        }
        if let Some(song) = current
            && let Some(n) = self.draw_details(ui, detail_rect, shared, &song)
        {
            return Some(n);
        }

        widgets::key_hints(
            ui.painter(),
            pos2(rect.left(), rect.bottom() + 8.0),
            Align2::LEFT_BOTTOM,
            &[
                ("↑ ↓", "Pilih lagu"),
                ("← →", "Kesulitan"),
                ("Enter", "Main"),
                ("Del", "Hapus"),
                ("Esc", "Kembali"),
            ],
            &palette,
        );
        self.delete_dialog_ui(ui.ctx(), shared, input, &songs);
        None
    }
}

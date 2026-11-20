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
use crate::ui::{anim, widgets};

/// Wait this long on a song before starting its preview.
const PREVIEW_DELAY: f32 = 0.35;
const CARD_H: f32 = 76.0;
const CARD_GAP: f32 = 10.0;

struct Preview {
    music: Music,
    dir: PathBuf,
    started: Instant,
    length: f32,
}

pub struct SongSelectScreen {
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
}

impl Default for SongSelectScreen {
    fn default() -> Self {
        Self {
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
                })
            }
            Err(e) => log::warn!("preview failed: {e:#}"),
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, shared: &mut Shared, input: &FrameInput) -> Option<Nav> {
        let now = input.now;
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
        for action in &input.menu {
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
        widgets::background(ui.painter(), ui.max_rect(), &palette, 0.0, Some(self.tint));

        // ── Header ──────────────────────────────────────────────
        let header = Rect::from_min_size(rect.min, vec2(rect.width(), 56.0));
        widgets::text_tracked(
            ui.painter(),
            header.left_center(),
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
            header.left_center() + vec2(190.0, 4.0),
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

        ui.painter().text(
            pos2(rect.left(), rect.bottom()),
            Align2::LEFT_BOTTOM,
            "↑↓ pilih lagu · ←→ tingkat kesulitan · Enter/Start main · Esc kembali",
            font(theme::TEXT_XS),
            palette.text_dim,
        );
        None
    }

    /// The song list. Scrolling is spring-driven (Apple style): the wheel and
    /// keyboard move a target the list glides to; dragging tracks the pointer
    /// 1:1 with rubber-band edges and a momentum-projected release; one focus
    /// highlight slides between cards instead of jumping.
    fn draw_list(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        shared: &mut Shared,
        songs: &[Arc<SongEntry>],
        now: Instant,
    ) -> Option<Nav> {
        let reduce = shared.settings.reduce_motion;
        let dt = ui.input(|i| i.stable_dt);
        let n = self.filtered.len();
        let pitch = CARD_H + CARD_GAP;
        let content_h = (n as f32 * pitch - CARD_GAP).max(0.0);
        let max_offset = (content_h - rect.height()).max(0.0);
        let row_top = |row: usize| row as f32 * pitch;

        let response = ui.interact(
            rect,
            ui.scope_id().with("song-list"),
            Sense::click_and_drag(),
        );

        // Keyboard / mat selection: glide so the selected card sits in the middle.
        let selected_y = row_top(self.selected);
        if std::mem::take(&mut self.scroll_to_selected) {
            let target = (selected_y - (rect.height() - CARD_H) / 2.0).clamp(0.0, max_offset);
            self.scroll.set_params(anim::Spring::SMOOTH);
            self.scroll.set_target(target);
        }
        self.focus.set_target(selected_y);

        // Mouse wheel / trackpad: move the target, the spring follows.
        let wheel = if response.hovered() {
            ui.input(|i| i.smooth_scroll_delta.y)
        } else {
            0.0
        };
        if wheel != 0.0 && self.drag_base.is_none() {
            let target = (self.scroll.target - wheel).clamp(0.0, max_offset);
            self.scroll.set_params(anim::Spring::SNAPPY);
            self.scroll.set_target(target);
        }

        // Direct manipulation: 1:1 drag, starting from the on-screen value.
        if response.drag_started() {
            self.drag_base = Some(self.scroll.value);
            self.drag_travel = 0.0;
        }
        if self.drag_base.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        if let Some(base) = self.drag_base {
            // drag_delta is the movement since the last frame.
            self.drag_travel += response.drag_delta().y;
            let raw = base - self.drag_travel;
            let shown = if raw < 0.0 {
                anim::rubberband(raw, rect.height())
            } else if raw > max_offset {
                max_offset + anim::rubberband(raw - max_offset, rect.height())
            } else {
                raw
            };
            self.scroll.value = shown;
            self.scroll.target = shown;
            if response.drag_stopped() || !response.dragged() {
                // Hand the finger's velocity to the spring and project where the
                // flick would come to rest.
                let velocity = -ui.input(|i| i.pointer.velocity().y);
                let projected = shown + anim::project(velocity, 0.998);
                self.scroll.velocity = velocity;
                self.scroll.set_params(if velocity.abs() > 300.0 {
                    anim::Spring::MOMENTUM
                } else {
                    anim::Spring::SMOOTH
                });
                self.scroll.set_target(projected.clamp(0.0, max_offset));
                self.drag_base = None;
                self.drag_travel = 0.0;
            }
        }
        if self.drag_base.is_none() {
            if reduce || !self.list_ready {
                self.scroll.snap(self.scroll.target.clamp(0.0, max_offset));
                self.focus.snap(selected_y);
                self.list_ready = true;
            } else {
                self.scroll.step(dt);
                self.focus.step(dt);
            }
        }
        if !self.scroll.is_settled(0.3) || !self.focus.is_settled(0.3) || self.drag_base.is_some() {
            ui.ctx().request_repaint();
        }
        let offset = self.scroll.value;
        let top = rect.top() - offset;

        // Which row is under the pointer (for hover and clicks).
        let row_at = |y: f32| -> Option<usize> {
            let local = y - top;
            let row = (local / pitch).floor();
            (row >= 0.0 && (row as usize) < n && local - row * pitch <= CARD_H)
                .then_some(row as usize)
        };
        let hovered_row = response.hover_pos().and_then(|p| row_at(p.y));
        let pressed_row = if response.is_pointer_button_down_on() && self.drag_base.is_none() {
            ui.input(|i| i.pointer.interact_pos())
                .and_then(|p| row_at(p.y))
        } else {
            None
        };

        let palette = shared.palette;
        let painter = ui.painter().with_clip_rect(rect.expand2(vec2(12.0, 0.0)));
        let first = ((offset / pitch).floor().max(0.0)) as usize;
        let last = (((offset + rect.height()) / pitch).ceil() as usize + 1).min(n);
        for row in first..last {
            let idx = self.filtered[row];
            let song = &songs[idx];
            let r = Rect::from_min_size(
                pos2(rect.left(), top + row_top(row)),
                vec2(rect.width() - 8.0, CARD_H),
            );
            let hover_id = ui.scope_id().with(("song-card", idx));
            let selected = row == self.selected;
            let hover = ui.ctx().animate_bool_with_time(
                hover_id,
                hovered_row == Some(row) || selected,
                0.12,
            );
            let pressed = pressed_row == Some(row);
            self.draw_card(&painter, r, song, selected, hover, pressed, shared, now);
        }

        // One focus highlight that slides between cards (spatial continuity).
        if n > 0
            && let Some(song) = songs.get(self.filtered[self.selected.min(n - 1)])
        {
            let y = top + self.focus.value;
            let r =
                Rect::from_min_size(pos2(rect.left() + 6.0, y), vec2(rect.width() - 8.0, CARD_H));
            let color = widgets::color_from_hash(&song.title());
            painter.rect_stroke(
                r,
                theme::RADIUS_CARD,
                Stroke::new(1.5, color),
                StrokeKind::Inside,
            );
            painter.rect_stroke(
                r.expand(3.0),
                theme::RADIUS_CARD + 3,
                Stroke::new(3.0, color.gamma_multiply(0.18)),
                StrokeKind::Outside,
            );
            let bar = Rect::from_min_size(
                pos2(r.left() - 6.0, r.top() + 18.0),
                vec2(3.0, r.height() - 36.0),
            );
            painter.rect_filled(bar, 2, color);
        }

        // Scroll edge effect: content fades under the edges instead of a hard cut.
        if offset > 1.0 {
            edge_fade(&painter, rect, true, &palette);
        }
        if offset < max_offset - 1.0 {
            edge_fade(&painter, rect, false, &palette);
        }

        let mut nav = None;
        if response.double_clicked() {
            if let Some(row) = response.interact_pointer_pos().and_then(|p| row_at(p.y)) {
                let song = &songs[self.filtered[row]];
                nav = Some(Nav::Play(self.play_request(song)));
            }
        } else if response.clicked()
            && let Some(row) = response.interact_pointer_pos().and_then(|p| row_at(p.y))
        {
            self.select(row, songs);
            self.scroll_to_selected = false; // clicking does not move the list under the pointer
        }
        nav
    }

    #[allow(clippy::too_many_arguments)] // drawing helper: all inputs are plain values
    fn draw_card(
        &self,
        painter: &egui::Painter,
        r: Rect,
        song: &SongEntry,
        selected: bool,
        hover: f32,
        pressed: bool,
        shared: &Shared,
        now: Instant,
    ) {
        let palette = shared.palette;
        let r = r.translate(vec2(if selected { 6.0 } else { hover * 3.0 }, 0.0));
        // Immediate feedback on press.
        let r = if pressed {
            Rect::from_center_size(r.center(), r.size() * 0.985)
        } else {
            r
        };
        widgets::elevation(painter, r, theme::RADIUS_CARD, widgets::Elevation::Low);
        let fill = palette.surface.lerp_to_gamma(palette.surface_hover, hover);
        painter.rect_filled(
            r,
            theme::RADIUS_CARD,
            if pressed {
                fill.lerp_to_gamma(Color32::BLACK, 0.12)
            } else {
                fill
            },
        );
        painter.rect_stroke(
            r,
            theme::RADIUS_CARD,
            palette.card_stroke(),
            StrokeKind::Inside,
        );
        widgets::top_highlight(painter, r, theme::RADIUS_CARD, 0.07);
        let color = widgets::color_from_hash(&song.title());

        // New-song highlight
        if let Some((dir, at)) = &shared.library.highlight
            && dir == &song.dir
        {
            let age = anim::since(*at, now);
            if age < 4.0 {
                let pulse = (1.0 - age / 4.0) * (0.5 + 0.5 * (age * 6.0).sin());
                painter.rect_stroke(
                    r.expand(2.0),
                    theme::RADIUS_CARD,
                    Stroke::new(3.0, theme::SUCCESS.gamma_multiply(pulse)),
                    StrokeKind::Outside,
                );
                painter.ctx().request_repaint();
            }
        }
        let swatch = Rect::from_min_size(r.min + vec2(14.0, 14.0), vec2(48.0, 48.0));
        widgets::gradient_rect(painter, swatch, color, color.gamma_multiply(0.4));
        painter.rect_stroke(swatch, 10, Stroke::new(6.0, fill), StrokeKind::Outside);
        painter.text(
            swatch.center(),
            Align2::CENTER_CENTER,
            initials(&song.title()),
            font_bold(theme::TEXT_M),
            Color32::WHITE,
        );

        let text_x = swatch.right() + 16.0;
        painter.text(
            pos2(text_x, r.top() + 26.0),
            Align2::LEFT_CENTER,
            truncate(&song.title(), 38),
            font_bold(theme::TEXT_M),
            palette.text,
        );
        let artist = if song.simfile.meta.artist.is_empty() {
            "Artis tidak diketahui"
        } else {
            &song.simfile.meta.artist
        };
        let sub = format!("{artist} · {} BPM", song.bpm_label);
        painter.text(
            pos2(text_x, r.top() + 52.0),
            Align2::LEFT_CENTER,
            truncate(&sub, 48),
            font(theme::TEXT_S),
            palette.text_dim,
        );

        let mut badge_x = r.right() - 16.0;
        if song.is_auto {
            let br =
                Rect::from_min_size(pos2(badge_x - 52.0, r.center().y - 12.0), vec2(52.0, 24.0));
            painter.rect_filled(br, theme::RADIUS_PILL, ACCENT_B.gamma_multiply(0.18));
            painter.text(
                br.center(),
                Align2::CENTER_CENTER,
                "Auto",
                font_bold(theme::TEXT_XS),
                ACCENT_B,
            );
            badge_x -= 64.0;
        }
        let best = song
            .playable
            .iter()
            .filter_map(|&i| {
                shared
                    .scores
                    .get(&chart_key(&song.dir, &song.simfile.charts[i]))
            })
            .map(|b| b.percent)
            .fold(None, |acc: Option<f64>, p| {
                Some(acc.map_or(p, |a| a.max(p)))
            });
        if let Some(p) = best {
            painter.text(
                pos2(badge_x, r.center().y),
                Align2::RIGHT_CENTER,
                format!("{p:.1}%"),
                font_bold(theme::TEXT_S),
                theme::SUCCESS,
            );
        }
    }

    fn draw_details(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        shared: &mut Shared,
        song: &Arc<SongEntry>,
    ) -> Option<Nav> {
        let palette = shared.palette;
        let painter = ui.painter().clone();
        widgets::card(&painter, rect, &palette);
        let color = widgets::color_from_hash(&song.title());

        // Banner area: gradient derived from the title.
        let banner = Rect::from_min_size(
            rect.min + vec2(20.0, 20.0),
            vec2(rect.width() - 40.0, (rect.height() * 0.32).min(170.0)),
        );
        widgets::gradient_rect(
            &painter,
            banner,
            color.gamma_multiply(0.9),
            theme::ACCENT_A.gamma_multiply(0.6),
        );
        painter.rect_stroke(
            banner,
            12,
            Stroke::new(10.0, palette.surface),
            StrokeKind::Outside,
        );
        crate::ui::notefield::draw_glow(
            &painter,
            banner.right_bottom() - vec2(60.0, 50.0),
            60.0,
            Color32::WHITE,
            0.4,
        );
        widgets::text_tracked(
            &painter,
            banner.left_bottom() + vec2(20.0, -20.0),
            Align2::LEFT_BOTTOM,
            &truncate(&song.title(), 30),
            font_bold(theme::TEXT_XL),
            Color32::WHITE,
        );
        if !song.simfile.meta.subtitle.is_empty() {
            painter.text(
                banner.left_top() + vec2(20.0, 18.0),
                Align2::LEFT_TOP,
                truncate(&song.simfile.meta.subtitle, 50),
                font(theme::TEXT_S),
                Color32::WHITE.gamma_multiply(0.8),
            );
        }

        let mut y = banner.bottom() + 24.0;
        let artist = if song.simfile.meta.artist.is_empty() {
            "Artis tidak diketahui"
        } else {
            &song.simfile.meta.artist
        };
        painter.text(
            pos2(rect.left() + 24.0, y),
            Align2::LEFT_TOP,
            truncate(artist, 50),
            font(theme::TEXT_M),
            palette.text,
        );
        y += 34.0;
        let mins = (song.length / 60.0).floor();
        let secs = (song.length - mins * 60.0).round();
        let facts = [
            ("BPM", song.bpm_label.clone()),
            ("Durasi", format!("{mins:.0}:{secs:02.0}")),
            (
                "Grup",
                if song.group.is_empty() {
                    "—".into()
                } else {
                    song.group.clone()
                },
            ),
        ];
        let fact_w = (rect.width() - 48.0) / 3.0;
        for (i, (label, value)) in facts.iter().enumerate() {
            let x = rect.left() + 24.0 + fact_w * i as f32;
            painter.text(
                pos2(x, y),
                Align2::LEFT_TOP,
                *label,
                font(theme::TEXT_XS),
                palette.text_dim,
            );
            painter.text(
                pos2(x, y + 16.0),
                Align2::LEFT_TOP,
                truncate(value, 18),
                font_bold(theme::TEXT_M),
                palette.text,
            );
        }
        y += 64.0;

        painter.text(
            pos2(rect.left() + 24.0, y),
            Align2::LEFT_TOP,
            "Tingkat kesulitan",
            font(theme::TEXT_XS),
            palette.text_dim,
        );
        y += 22.0;
        let slot = self.chart_slot(song);
        let chips_rect =
            Rect::from_min_size(pos2(rect.left() + 24.0, y), vec2(rect.width() - 48.0, 40.0));
        let mut chosen = None;
        ui.scope_builder(egui::UiBuilder::new().max_rect(chips_rect), |ui| {
            ui.horizontal_wrapped(|ui| {
                for (k, &i) in song.playable.iter().enumerate() {
                    let chart = &song.simfile.charts[i];
                    let label = format!("{} {}", difficulty_label(chart.difficulty), chart.meter);
                    if widgets::chip(ui, &label, difficulty_color(chart.difficulty), k == slot)
                        .clicked()
                    {
                        chosen = Some(chart.difficulty);
                    }
                }
            });
        });
        if let Some(d) = chosen {
            self.difficulty = d;
        }
        y += 56.0;

        let chart = &song.simfile.charts[song.playable[slot]];
        let notes = chart
            .notes
            .iter()
            .filter(|n| {
                !matches!(
                    n.kind,
                    crate::parser::NoteKind::Mine | crate::parser::NoteKind::Fake
                )
            })
            .count();
        let best = shared.scores.get(&chart_key(&song.dir, chart));
        painter.text(
            pos2(rect.left() + 24.0, y),
            Align2::LEFT_TOP,
            format!("{notes} panah"),
            font(theme::TEXT_S),
            palette.text_dim,
        );
        let best_text = best.map_or("Belum pernah dimainkan".to_string(), |b| {
            format!("Terbaik {} · {:.2}%", b.grade, b.percent)
        });
        painter.text(
            pos2(rect.right() - 24.0, y),
            Align2::RIGHT_TOP,
            best_text,
            font(theme::TEXT_S),
            if best.is_some() {
                theme::SUCCESS
            } else {
                palette.text_dim
            },
        );

        let button = Rect::from_min_size(
            pos2(rect.left() + 24.0, rect.bottom() - 76.0),
            vec2(rect.width() - 48.0, 52.0),
        );
        let play = ui.put(button, |ui: &mut Ui| {
            widgets::primary_button(ui, "Mainkan  ▶", button.size(), true)
        });
        if play.clicked() {
            return Some(Nav::Play(self.play_request(song)));
        }
        None
    }

    fn draw_skeleton(&self, ui: &mut Ui, rect: Rect, palette: &theme::Palette, time: f32) {
        let painter = ui.painter();
        let t = time * 3.0;
        for i in 0..6 {
            let r = Rect::from_min_size(
                rect.min + vec2(0.0, i as f32 * (CARD_H + 10.0)),
                vec2(rect.width() * 0.52, CARD_H),
            );
            let shimmer = 0.5 + 0.5 * (t - i as f32 * 0.3).sin();
            painter.rect_filled(
                r,
                theme::RADIUS_CARD,
                palette.surface.gamma_multiply(0.6 + shimmer * 0.4),
            );
        }
    }

    fn draw_empty(&mut self, ui: &mut Ui, rect: Rect, shared: &mut Shared) -> Option<Nav> {
        let palette = shared.palette;
        let card = Rect::from_center_size(rect.center(), vec2(560.0, 320.0));
        widgets::card(ui.painter(), card, &palette);
        let p = ui.painter();
        p.text(
            card.center_top() + vec2(0.0, 60.0),
            Align2::CENTER_CENTER,
            "♪",
            font_bold(48.0),
            theme::ACCENT_B,
        );
        widgets::text_tracked(
            p,
            card.center_top() + vec2(0.0, 120.0),
            Align2::CENTER_CENTER,
            "Belum ada lagu",
            font_bold(theme::TEXT_L),
            palette.text,
        );
        p.text(
            card.center_top() + vec2(0.0, 156.0),
            Align2::CENTER_CENTER,
            "Seret file MP3/MP4 ke jendela ini, atau pilih lewat tombol di bawah.",
            font(theme::TEXT_S),
            palette.text_dim,
        );
        let b1 = Rect::from_min_size(
            card.center_bottom() + vec2(-250.0, -90.0),
            vec2(240.0, 52.0),
        );
        let b2 = Rect::from_min_size(card.center_bottom() + vec2(10.0, -90.0), vec2(240.0, 52.0));
        if ui
            .put(b1, |ui: &mut Ui| {
                widgets::primary_button(ui, "Impor lagu", b1.size(), true)
            })
            .clicked()
        {
            return Some(Nav::Import(Vec::new()));
        }
        if ui
            .put(b2, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, "Buat lagu contoh", b2.size())
            })
            .clicked()
        {
            match crate::library::test_song::make_test_song(&shared.library.root) {
                Ok(dir) => shared.library.mark_new(dir),
                Err(e) => shared
                    .toasts
                    .warning(format!("Gagal membuat lagu contoh: {e}")),
            }
        }
        None
    }
}

/// Up to two initials for the song swatch.
fn initials(title: &str) -> String {
    title
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

/// Soft fade where the list meets the edge of its area (scroll edge effect).
fn edge_fade(painter: &egui::Painter, rect: Rect, top: bool, palette: &theme::Palette) {
    const H: f32 = 28.0;
    let base = if top {
        palette.bg_top
    } else {
        palette.bg_bottom
    };
    let band = if top {
        Rect::from_min_size(
            rect.left_top() - vec2(12.0, 0.0),
            vec2(rect.width() + 24.0, H),
        )
    } else {
        Rect::from_min_size(
            rect.left_bottom() - vec2(12.0, H),
            vec2(rect.width() + 24.0, H),
        )
    };
    let solid = base.gamma_multiply(0.9);
    let clear = Color32::TRANSPARENT;
    let (a, b) = if top { (solid, clear) } else { (clear, solid) };
    widgets::vertical_gradient(painter, band, a, b);
}

/// Shortens text to `max` characters with an ellipsis.
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut s: String = text.chars().take(max.saturating_sub(1)).collect();
        s.push('…');
        s
    }
}

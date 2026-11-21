//! Song select drawing: card list, details panel, skeleton and empty state.

use super::*;

impl SongSelectScreen {
    /// The song list. Scrolling is spring-driven (Apple style): the wheel and
    /// keyboard move a target the list glides to; dragging tracks the pointer
    /// 1:1 with rubber-band edges and a momentum-projected release; one focus
    /// highlight slides between cards instead of jumping.
    pub(super) fn draw_list(
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
            let pulse = self.beat.pulse;
            painter.rect_stroke(
                r,
                theme::RADIUS_CARD,
                Stroke::new(
                    1.5 + pulse,
                    color.lerp_to_gamma(Color32::WHITE, 0.4 * pulse),
                ),
                StrokeKind::Inside,
            );
            painter.rect_stroke(
                r.expand(3.0 + 3.0 * pulse),
                theme::RADIUS_CARD + 3,
                Stroke::new(3.0 + 3.0 * pulse, color.gamma_multiply(0.18 + 0.3 * pulse)),
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
    pub(super) fn draw_card(
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
            widgets::truncate(&song.title(), 38),
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
            widgets::truncate(&sub, 48),
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

    pub(super) fn draw_details(
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
        let beat = self.beat;
        let eq = Rect::from_min_max(
            pos2(
                banner.left() + banner.width() * 0.42,
                banner.top() + banner.height() * 0.3,
            ),
            banner.right_bottom() - vec2(16.0, 0.0),
        );
        beatfx::draw_equalizer(
            &painter.with_clip_rect(banner),
            eq,
            (Color32::WHITE, color.lerp_to_gamma(Color32::WHITE, 0.5)),
            &beat,
        );
        crate::ui::notefield::draw_glow(
            &painter,
            banner.right_top() + vec2(-44.0, 40.0),
            24.0 + 10.0 * beat.pulse,
            Color32::WHITE,
            0.3 + 0.4 * beat.pulse,
        );
        beatfx::draw_kick_ring(
            &painter.with_clip_rect(banner),
            banner.right_top() + vec2(-44.0, 40.0),
            18.0,
            Color32::WHITE,
            &beat,
        );
        widgets::text_tracked(
            &painter,
            banner.left_bottom() + vec2(20.0, -20.0),
            Align2::LEFT_BOTTOM,
            &widgets::truncate(&song.title(), 30),
            font_bold(theme::TEXT_XL),
            Color32::WHITE,
        );
        if !song.simfile.meta.subtitle.is_empty() {
            painter.text(
                banner.left_top() + vec2(20.0, 18.0),
                Align2::LEFT_TOP,
                widgets::truncate(&song.simfile.meta.subtitle, 50),
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
            widgets::truncate(artist, 50),
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
            let value_rect = painter.text(
                pos2(x, y + 16.0),
                Align2::LEFT_TOP,
                widgets::truncate(value, 18),
                font_bold(theme::TEXT_M),
                palette.text,
            );
            if i == 0 && beat.live {
                beatfx::draw_metronome(
                    &painter,
                    pos2(value_rect.right() + 14.0, value_rect.center().y),
                    color,
                    &beat,
                );
            }
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

        let delete = Rect::from_min_size(
            pos2(rect.right() - 24.0 - 120.0, rect.bottom() - 76.0),
            vec2(120.0, 52.0),
        );
        let button = Rect::from_min_max(
            pos2(rect.left() + 24.0, delete.top()),
            pos2(delete.left() - 12.0, delete.bottom()),
        );
        let play = ui.put(button, |ui: &mut Ui| {
            widgets::primary_button(ui, "Mainkan  ▶", button.size(), true)
        });
        if play.clicked() {
            return Some(Nav::Play(self.play_request(song)));
        }
        if ui
            .put(delete, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, "🗑  Hapus", delete.size())
            })
            .on_hover_text("Hapus lagu ini dari disk (Del)")
            .clicked()
        {
            self.ask_delete(song, Instant::now());
        }
        None
    }

    pub(super) fn draw_skeleton(
        &self,
        ui: &mut Ui,
        rect: Rect,
        palette: &theme::Palette,
        time: f32,
    ) {
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

    pub(super) fn draw_empty(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        shared: &mut Shared,
    ) -> Option<Nav> {
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
pub(super) fn initials(title: &str) -> String {
    title
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

/// Soft fade where the list meets the edge of its area (scroll edge effect).
pub(super) fn edge_fade(painter: &egui::Painter, rect: Rect, top: bool, palette: &theme::Palette) {
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

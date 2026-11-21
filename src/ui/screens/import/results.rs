//! Import screen results: one finished song, a failure, or a batch summary.

use super::*;

impl ImportScreen {
    pub(super) fn draw_done(
        &mut self,
        ui: &mut Ui,
        shared: &mut Shared,
        area: Rect,
    ) -> Option<Nav> {
        let palette = shared.palette;
        let painter = ui.painter().clone();
        let Phase::Done(result) = &mut self.phase else {
            return None;
        };
        widgets::card(&painter, area, &palette);
        let inner = area.shrink(32.0);
        let meta = &result.simfile.meta;
        painter.text(
            inner.left_top(),
            Align2::LEFT_TOP,
            format!("✓ Selesai dalam {:.1} detik", result.elapsed.as_secs_f32()),
            font(theme::TEXT_S),
            SUCCESS,
        );
        widgets::text_tracked(
            &painter,
            inner.left_top() + vec2(0.0, 24.0),
            Align2::LEFT_TOP,
            &widgets::truncate(&meta.title, 48),
            font_bold(theme::TEXT_XL),
            palette.text,
        );
        let artist = if meta.artist.is_empty() {
            "Artis tidak diketahui"
        } else {
            meta.artist.as_str()
        };
        painter.text(
            inner.left_top() + vec2(0.0, 66.0),
            Align2::LEFT_TOP,
            artist,
            font(theme::TEXT_M),
            palette.text_dim,
        );

        // BPM row with ×2 / ÷2 / manual.
        let mut new_bpm = None;
        let row = Rect::from_min_size(
            inner.left_top() + vec2(0.0, 112.0),
            vec2(inner.width(), 44.0),
        );
        painter.text(
            row.left_center(),
            Align2::LEFT_CENTER,
            "BPM terdeteksi",
            font(theme::TEXT_S),
            palette.text_dim,
        );
        let bpm = result.bpm();
        let mut x = row.left() + 130.0;
        let beat = self.beat;
        if beat.live {
            crate::ui::notefield::radial_light(
                &painter,
                pos2(x + 40.0, row.center().y),
                60.0,
                ACCENT_B,
                0.5 * beat.pulse,
            );
        }
        widgets::tabular_text(
            &painter,
            pos2(x, row.center().y),
            Align2::LEFT_CENTER,
            &format_bpm(bpm),
            font_bold(theme::TEXT_L),
            palette.text,
        );
        x += 110.0;
        for (label, factor) in [("÷2", 0.5), ("×2", 2.0)] {
            let r = Rect::from_min_size(pos2(x, row.top()), vec2(56.0, 40.0));
            if ui
                .put(r, |ui: &mut Ui| {
                    widgets::secondary_button(ui, &palette, label, r.size())
                })
                .clicked()
            {
                new_bpm = Some(bpm * factor);
            }
            x += 64.0;
        }
        let field = Rect::from_min_size(pos2(x + 16.0, row.top() + 4.0), vec2(90.0, 32.0));
        let resp = ui.put(
            field,
            egui::TextEdit::singleline(&mut self.bpm_text)
                .font(font(theme::TEXT_M))
                .hint_text("BPM"),
        );
        let apply = Rect::from_min_size(pos2(field.right() + 8.0, row.top()), vec2(100.0, 40.0));
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui
            .put(apply, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, "Terapkan", apply.size())
            })
            .clicked()
            || enter
        {
            match self.bpm_text.trim().replace(',', ".").parse::<f64>() {
                Ok(v) if (30.0..=400.0).contains(&v) => new_bpm = Some(v),
                _ => shared.toasts.warning("Masukkan BPM antara 30 dan 400."),
            }
        }
        if beat.live {
            beatfx::draw_metronome(
                &painter,
                pos2(apply.right() + 24.0, row.center().y),
                ACCENT_B,
                &beat,
            );
        }
        painter.text(
            pos2(row.left(), row.bottom() + 8.0),
            Align2::LEFT_TOP,
            "Dengarkan lagunya: titik harus berkedip pas di ketukan. Terlalu rapat atau jarang? Coba ÷2 atau ×2.",
            font(theme::TEXT_XS + 1.0),
            palette.text_dim,
        );
        if let Some(b) = new_bpm {
            match result.set_bpm(b) {
                Ok(()) => {
                    self.bpm_text = format_bpm(result.bpm());
                    shared.library.mark_new(result.dir.clone());
                    shared.toasts.success(format!(
                        "Chart dibuat ulang dengan {} BPM",
                        format_bpm(result.bpm())
                    ));
                }
                Err(e) => shared.toasts.warning(format!("Gagal menyimpan: {e}")),
            }
        }

        // Difficulty cards
        let nps = result.chart_nps();
        let cards_top = row.bottom() + 44.0;
        let n = result.simfile.charts.len().max(1);
        let gap = 12.0;
        let w = (inner.width() - gap * (n as f32 - 1.0)) / n as f32;
        for (i, chart) in result.simfile.charts.iter().enumerate() {
            let r = Rect::from_min_size(
                pos2(inner.left() + i as f32 * (w + gap), cards_top),
                vec2(w, 96.0),
            );
            let resp = ui.interact(r, ui.scope_id().with(("import-chart", i)), Sense::click());
            if resp.clicked() {
                self.chart = i;
            }
            let sel = self.chart == i;
            let color = difficulty_color(chart.difficulty);
            let hover = ui.ctx().animate_bool(resp.id, resp.hovered() || sel);
            painter.rect_filled(
                r,
                theme::RADIUS_BUTTON,
                color.gamma_multiply(0.08 + 0.1 * hover),
            );
            painter.rect_stroke(
                r,
                theme::RADIUS_BUTTON,
                Stroke::new(
                    if sel { 2.0 } else { 1.0 },
                    color.gamma_multiply(0.4 + 0.6 * hover),
                ),
                StrokeKind::Inside,
            );
            painter.text(
                r.left_top() + vec2(16.0, 14.0),
                Align2::LEFT_TOP,
                difficulty_label(chart.difficulty),
                font_bold(theme::TEXT_S),
                color,
            );
            widgets::text_tracked(
                &painter,
                r.right_top() + vec2(-16.0, 8.0),
                Align2::RIGHT_TOP,
                &chart.meter.to_string(),
                font_bold(theme::TEXT_XL),
                palette.text,
            );
            let rows = count_rows(chart);
            painter.text(
                r.left_bottom() + vec2(16.0, -14.0),
                Align2::LEFT_BOTTOM,
                format!(
                    "{rows} langkah • {:.1}/detik",
                    nps.get(i).copied().unwrap_or(0.0)
                ),
                font(theme::TEXT_XS + 1.0),
                palette.text_dim,
            );
        }

        if let Some(note) = &result.note {
            painter.text(
                pos2(inner.left(), cards_top + 112.0),
                Align2::LEFT_TOP,
                format!("ⓘ {note}"),
                font(theme::TEXT_XS + 1.0),
                palette.text_dim,
            );
        }

        // Live equalizer in the free space, dancing to the preview.
        let eq_top = cards_top + if result.note.is_some() { 140.0 } else { 116.0 };
        let eq = Rect::from_min_max(
            pos2(inner.left(), eq_top),
            pos2(inner.right(), inner.bottom() - 72.0),
        );
        if eq.height() > 24.0 {
            beatfx::draw_equalizer(&painter, eq, (ACCENT_A, ACCENT_B), &beat);
        }

        // Actions
        let y = inner.bottom() - 52.0;
        let play_r = Rect::from_min_size(pos2(inner.left(), y), vec2(240.0, 52.0));
        let mut nav = None;
        if ui
            .put(play_r, |ui: &mut Ui| {
                widgets::primary_button(ui, "▶  Mainkan Sekarang", play_r.size(), true)
            })
            .clicked()
        {
            nav = Some(play(result, self.chart));
        }
        let list_r = Rect::from_min_size(pos2(play_r.right() + 12.0, y + 4.0), vec2(190.0, 44.0));
        if ui
            .put(list_r, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, "Lihat di daftar lagu", list_r.size())
            })
            .clicked()
        {
            nav = Some(Nav::SongSelect);
        }
        let more = if self.queue.is_empty() {
            "Impor lagu lain".to_string()
        } else {
            format!("Impor berikutnya ({})", self.queue.len())
        };
        let again_r = Rect::from_min_size(pos2(list_r.right() + 12.0, y + 4.0), vec2(190.0, 44.0));
        if ui
            .put(again_r, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, &more, again_r.size())
            })
            .clicked()
        {
            self.phase = Phase::Idle;
        }
        nav
    }

    pub(super) fn draw_failed(&mut self, ui: &mut Ui, shared: &Shared, area: Rect) -> Option<Nav> {
        let palette = shared.palette;
        let painter = ui.painter().clone();
        let Phase::Failed(msg) = &self.phase else {
            return None;
        };
        let card = Rect::from_center_size(area.center(), vec2(area.width(), 280.0));
        widgets::card(&painter, card, &palette);
        painter.circle_stroke(
            card.center_top() + vec2(0.0, 56.0),
            24.0,
            Stroke::new(3.0, DANGER),
        );
        widgets::text_tracked(
            &painter,
            card.center_top() + vec2(0.0, 56.0),
            Align2::CENTER_CENTER,
            "!",
            font_bold(theme::TEXT_L),
            DANGER,
        );
        widgets::text_tracked(
            &painter,
            card.center_top() + vec2(0.0, 104.0),
            Align2::CENTER_TOP,
            "Lagu ini belum bisa diimpor",
            font_bold(theme::TEXT_L),
            palette.text,
        );
        let galley = painter.layout(
            msg.clone(),
            font(theme::TEXT_S),
            palette.text_dim,
            card.width() - 80.0,
        );
        painter.galley(
            pos2(card.center().x - galley.size().x / 2.0, card.top() + 144.0),
            galley,
            palette.text_dim,
        );
        painter.text(
            card.center_bottom() - vec2(0.0, 92.0),
            Align2::CENTER_CENTER,
            "Coba file lain, atau pastikan file bisa diputar di pemutar musik biasa.",
            font(theme::TEXT_XS + 1.0),
            palette.text_dim,
        );
        let r = Rect::from_center_size(card.center_bottom() - vec2(0.0, 46.0), vec2(200.0, 44.0));
        if ui
            .put(r, |ui: &mut Ui| {
                widgets::primary_button(ui, "Coba file lain", r.size(), true)
            })
            .clicked()
        {
            self.phase = Phase::Idle;
            self.open_picker(ui.ctx());
        }
        None
    }

    pub(super) fn draw_summary(&mut self, ui: &mut Ui, shared: &Shared, area: Rect) -> Option<Nav> {
        const MAX_ROWS: usize = 7;
        let palette = shared.palette;
        let painter = ui.painter().clone();
        let ok = self.batch.iter().filter(|e| e.outcome.is_ok()).count();
        let failed = self.batch.len() - ok;
        let rows = self.batch.len().min(MAX_ROWS);
        let extra = if self.batch.len() > MAX_ROWS {
            24.0
        } else {
            0.0
        };
        let height = (150.0_f32 + rows as f32 * 40.0 + extra + 76.0).min(area.height());
        let card = Rect::from_center_size(area.center(), vec2(area.width(), height));
        widgets::card(&painter, card, &palette);
        let inner = card.shrink(28.0);

        let title = if failed == 0 {
            format!("{ok} lagu siap dimainkan!")
        } else if ok == 0 {
            "Belum ada lagu yang berhasil diimpor".to_string()
        } else {
            format!("{ok} lagu berhasil, {failed} gagal")
        };
        widgets::text_tracked(
            &painter,
            inner.left_top(),
            Align2::LEFT_TOP,
            &title,
            font_bold(theme::TEXT_L),
            palette.text,
        );
        painter.text(
            inner.left_top() + vec2(0.0, 36.0),
            Align2::LEFT_TOP,
            if ok > 0 {
                "Lagu baru sudah ada di daftar lagu, ditandai badge \"Auto\"."
            } else {
                "Coba file lain, atau pastikan file bisa diputar di pemutar musik biasa."
            },
            font(theme::TEXT_S),
            palette.text_dim,
        );

        let mut y = inner.top() + 76.0;
        for entry in self.batch.iter().take(MAX_ROWS) {
            let row = Rect::from_min_size(pos2(inner.left(), y), vec2(inner.width(), 34.0));
            painter.rect_filled(
                row,
                theme::RADIUS_BUTTON,
                palette.surface_hover.gamma_multiply(0.6),
            );
            let icon = pos2(row.left() + 18.0, row.center().y);
            match &entry.outcome {
                Ok((title, bpm)) => {
                    painter.circle_filled(icon, 9.0, SUCCESS.gamma_multiply(0.2));
                    painter.text(icon, Align2::CENTER_CENTER, "✓", font_bold(12.0), SUCCESS);
                    painter.text(
                        pos2(row.left() + 38.0, row.center().y),
                        Align2::LEFT_CENTER,
                        widgets::truncate(title, 50),
                        font_bold(theme::TEXT_S),
                        palette.text,
                    );
                    painter.text(
                        pos2(row.right() - 14.0, row.center().y),
                        Align2::RIGHT_CENTER,
                        format!("{} BPM", format_bpm(*bpm)),
                        font(theme::TEXT_XS),
                        palette.text_dim,
                    );
                }
                Err(reason) => {
                    painter.circle_filled(icon, 9.0, DANGER.gamma_multiply(0.2));
                    painter.text(icon, Align2::CENTER_CENTER, "!", font_bold(12.0), DANGER);
                    painter.text(
                        pos2(row.left() + 38.0, row.center().y),
                        Align2::LEFT_CENTER,
                        widgets::truncate(&entry.file, 36),
                        font_bold(theme::TEXT_S),
                        palette.text,
                    );
                    painter.text(
                        pos2(row.right() - 14.0, row.center().y),
                        Align2::RIGHT_CENTER,
                        widgets::truncate(friendly_reason(reason), 44),
                        font(theme::TEXT_XS),
                        DANGER.lerp_to_gamma(palette.text, 0.3),
                    );
                }
            }
            y += 40.0;
        }
        if self.batch.len() > MAX_ROWS {
            painter.text(
                pos2(inner.left(), y),
                Align2::LEFT_TOP,
                format!("…dan {} lagu lainnya", self.batch.len() - MAX_ROWS),
                font(theme::TEXT_XS),
                palette.text_dim,
            );
        }

        let by = inner.bottom() - 48.0;
        let mut nav = None;
        let list_r = Rect::from_min_size(pos2(inner.left(), by), vec2(220.0, 48.0));
        if ok > 0
            && ui
                .put(list_r, |ui: &mut Ui| {
                    widgets::primary_button(ui, "Lihat di daftar lagu  ›", list_r.size(), true)
                })
                .clicked()
        {
            nav = Some(Nav::SongSelect);
        }
        let again_x = if ok > 0 {
            list_r.right() + 12.0
        } else {
            inner.left()
        };
        let again_r = Rect::from_min_size(pos2(again_x, by + 2.0), vec2(180.0, 44.0));
        if ui
            .put(again_r, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, "Impor lagu lain", again_r.size())
            })
            .clicked()
        {
            self.batch.clear();
            self.batch_size = 0;
            self.phase = Phase::Idle;
            self.open_picker(ui.ctx());
        }
        nav
    }
}

pub(super) fn play(result: &ImportResult, chart: usize) -> Nav {
    Nav::Play(PlayRequest {
        simfile: Arc::new(result.simfile.clone()),
        song_dir: result.dir.clone(),
        chart_index: chart,
    })
}

pub(super) fn format_bpm(bpm: f64) -> String {
    if (bpm - bpm.round()).abs() < 0.005 {
        format!("{bpm:.0}")
    } else {
        format!("{bpm:.2}")
    }
}

/// Number of steps (rows), counting a jump once.
pub(super) fn count_rows(chart: &crate::parser::Chart) -> usize {
    let mut beats: Vec<f64> = chart.notes.iter().map(|n| n.beat).collect();
    beats.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    beats.len()
}

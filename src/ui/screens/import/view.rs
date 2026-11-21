//! Import screen drawing while choosing files and while an import runs.

use super::*;

impl ImportScreen {
    pub(super) fn draw_idle(&mut self, ui: &mut Ui, shared: &Shared, area: Rect) -> Option<Nav> {
        let palette = shared.palette;
        let zone = Rect::from_min_size(area.min, vec2(area.width(), area.height() * 0.72));
        let response = ui.interact(zone, ui.scope_id().with("drop-zone"), Sense::click());
        let hovering_files = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
        let hot = ui
            .ctx()
            .animate_bool(response.id, response.hovered() || hovering_files);
        let painter = ui.painter();
        painter.rect_filled(
            zone,
            theme::RADIUS_CARD,
            palette.surface.lerp_to_gamma(palette.surface_hover, hot),
        );
        dashed_rect(
            painter,
            zone.shrink(10.0),
            palette
                .text_dim
                .lerp_to_gamma(ACCENT_B, hot)
                .gamma_multiply(0.7),
        );

        // Icon: four arrows falling onto a row of targets, one per beat.
        let beat = self.beat;
        let c = zone.center() - vec2(0.0, 56.0);
        let fall = 70.0;
        for col in 0..4u8 {
            let x = c.x + (f32::from(col) - 1.5) * 52.0;
            let target = pos2(x, c.y + 24.0);
            let color = theme::column_color(col);
            // Arrow `col` lands on beat ≡ col (mod 4), so they chase in turn.
            let since = (beat.beat - f64::from(col)).rem_euclid(4.0) as f32;
            let landed = since < 1.0;
            let flash = if landed {
                (1.0 - since).powi(3) * if beat.live { 1.0 } else { 0.6 }
            } else {
                0.0
            };
            if flash > 0.0 {
                crate::ui::notefield::radial_light(painter, target, 46.0, color, 0.8 * flash);
            }
            crate::ui::notefield::draw_arrow(
                painter,
                target,
                40.0,
                col,
                color.gamma_multiply(0.12 + 0.6 * flash),
                Stroke::new(1.5, color.gamma_multiply(0.5 + 0.5 * hot)),
            );
            if !shared.settings.reduce_motion {
                // Falling during the beat before it lands.
                let to_land = 4.0 - since; // beats until next landing
                if to_land < 1.0 || (hot > 0.5 && to_land < 2.0) {
                    let p = 1.0 - to_land.min(1.0);
                    let pos = target - vec2(0.0, fall * (1.0 - p));
                    crate::ui::notefield::draw_arrow(
                        painter,
                        pos,
                        34.0,
                        col,
                        color.gamma_multiply(0.3 + 0.7 * p),
                        Stroke::new(1.0, Color32::WHITE.gamma_multiply(0.6 * p)),
                    );
                }
            }
        }
        // Neon frame that breathes with the beat (stronger while dragging).
        painter.rect_stroke(
            zone.shrink(10.0),
            theme::RADIUS_CARD,
            Stroke::new(
                1.5 + 2.0 * hot,
                ACCENT_B.gamma_multiply((0.12 + 0.3 * hot) * (0.5 + 0.5 * beat.pulse)),
            ),
            StrokeKind::Inside,
        );
        if !shared.settings.reduce_motion {
            ui.ctx().request_repaint();
        }

        let headline = if drop_supported() {
            "Seret MP3/MP4 ke sini atau klik untuk memilih"
        } else {
            "Klik untuk memilih file MP3/MP4"
        };
        painter.text(
            zone.center() + vec2(0.0, 30.0),
            Align2::CENTER_CENTER,
            headline,
            font_bold(theme::TEXT_M),
            palette.text,
        );
        painter.text(
            zone.center() + vec2(0.0, 58.0),
            Align2::CENTER_CENTER,
            "BPM, offset, dan 4 tingkat kesulitan dibuat otomatis • Enter untuk memilih",
            font(theme::TEXT_S),
            palette.text_dim,
        );
        if response.clicked() {
            self.open_picker(ui.ctx());
        }
        if self.picker.is_some() {
            painter.text(
                zone.center_bottom() - vec2(0.0, 36.0),
                Align2::CENTER_CENTER,
                "Menunggu dialog pilih file…",
                font(theme::TEXT_S),
                ACCENT_B,
            );
        }

        // Info line about ffmpeg (friendly, never scary).
        let info = if self.has_ffmpeg {
            "✓ ffmpeg ditemukan: audio dari video disimpan sebagai .ogg yang ringkas."
        } else {
            "ⓘ ffmpeg tidak wajib. Tanpa ffmpeg, audio dari MP4 disimpan sebagai .wav (lebih besar)."
        };
        painter.text(
            pos2(area.center().x, zone.bottom() + 36.0),
            Align2::CENTER_CENTER,
            info,
            font(theme::TEXT_XS + 1.0),
            if self.has_ffmpeg {
                SUCCESS.gamma_multiply(0.85)
            } else {
                palette.text_dim
            },
        );
        painter.text(
            pos2(area.center().x, zone.bottom() + 62.0),
            Align2::CENTER_CENTER,
            format!(
                "Lagu disimpan di {}",
                shared.library.root.join("Imported").display()
            ),
            font(theme::TEXT_XS),
            palette.text_dim.gamma_multiply(0.8),
        );
        None
    }

    pub(super) fn draw_running(
        &mut self,
        ui: &mut Ui,
        shared: &Shared,
        area: Rect,
        now: Instant,
    ) -> Option<Nav> {
        let palette = shared.palette;
        let painter = ui.painter().clone();
        widgets::card(&painter, area, &palette);
        let inner = area.shrink(32.0);
        let name = self
            .job
            .as_ref()
            .and_then(|j| j.source.file_name())
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        painter.text(
            inner.left_top(),
            Align2::LEFT_TOP,
            "Sedang mengimpor",
            font(theme::TEXT_S),
            palette.text_dim,
        );
        widgets::text_tracked(
            &painter,
            inner.left_top() + vec2(0.0, 22.0),
            Align2::LEFT_TOP,
            &widgets::truncate(&name, 60),
            font_bold(theme::TEXT_L),
            palette.text,
        );
        if self.is_batch() {
            painter.text(
                inner.right_top(),
                Align2::RIGHT_TOP,
                format!("Lagu {} dari {}", self.batch.len() + 1, self.batch_size),
                font_bold(theme::TEXT_S),
                theme::ACCENT_B,
            );
        }

        // Stepper
        let top = inner.top() + 90.0;
        let step_w = inner.width() / 5.0;
        for (i, stage) in Stage::ALL.iter().enumerate() {
            let center = pos2(inner.left() + step_w * (i as f32 + 0.5), top + 18.0);
            if i > 0 {
                let prev = pos2(center.x - step_w, center.y);
                let done = self.stage_done[i - 1].is_some();
                painter.line_segment(
                    [prev + vec2(22.0, 0.0), center - vec2(22.0, 0.0)],
                    Stroke::new(
                        2.0,
                        if done {
                            ACCENT_B
                        } else {
                            palette.border.gamma_multiply(3.0)
                        },
                    ),
                );
            }
            let done_at = self.stage_done[i];
            let active = *stage == self.stage && done_at.is_none();
            let fill = if done_at.is_some() {
                SUCCESS
            } else if active {
                ACCENT_A
            } else {
                palette.surface_hover
            };
            let pop = done_at.map_or(1.0, |t| {
                anim::ease_out_back(anim::progress(t, now, 0.3, shared.settings.reduce_motion))
            });
            painter.circle_filled(center, 18.0 * pop.max(0.6), fill);
            if active {
                let spin = anim::since(self.started, now) * 4.0;
                painter.add(egui::Shape::line(
                    (0..16)
                        .map(|k| {
                            let a = spin + k as f32 / 15.0 * 4.0;
                            center + vec2(a.cos(), a.sin()) * 23.0
                        })
                        .collect(),
                    Stroke::new(2.5, ACCENT_B),
                ));
            }
            if done_at.is_some() {
                let s = 7.0 * pop;
                painter.line_segment(
                    [center + vec2(-s, 0.0), center + vec2(-s * 0.3, s * 0.7)],
                    Stroke::new(3.0, Color32::from_rgb(12, 12, 20)),
                );
                painter.line_segment(
                    [center + vec2(-s * 0.3, s * 0.7), center + vec2(s, -s * 0.7)],
                    Stroke::new(3.0, Color32::from_rgb(12, 12, 20)),
                );
            } else {
                painter.text(
                    center,
                    Align2::CENTER_CENTER,
                    format!("{}", i + 1),
                    font_bold(theme::TEXT_S),
                    if active {
                        Color32::WHITE
                    } else {
                        palette.text_dim
                    },
                );
            }
            painter.text(
                center + vec2(0.0, 34.0),
                Align2::CENTER_TOP,
                stage.label(),
                font(theme::TEXT_XS + 1.0),
                if active || done_at.is_some() {
                    palette.text
                } else {
                    palette.text_dim
                },
            );
        }

        // Overall progress
        let overall = (self.stage.index() as f32 + self.stage_progress) / 5.0;
        let bar = Rect::from_min_size(pos2(inner.left(), top + 90.0), vec2(inner.width(), 10.0));
        widgets::progress_bar(&painter, bar, overall, &palette);
        painter.text(
            bar.right_bottom() + vec2(0.0, 10.0),
            Align2::RIGHT_TOP,
            format!("{:.0}%", overall * 100.0),
            font(theme::TEXT_S),
            palette.text_dim,
        );

        // Waveform
        let wave = Rect::from_min_max(
            pos2(inner.left(), bar.bottom() + 40.0),
            pos2(inner.right(), inner.bottom() - 70.0),
        );
        draw_waveform(&painter, wave, &self.waveform, overall, &palette);
        if !self.waveform.is_empty() && !shared.settings.reduce_motion {
            // Glowing scanner at the analysis position.
            let x = wave.left() + wave.width() * overall.clamp(0.0, 1.0);
            crate::ui::notefield::radial_light(
                &painter,
                pos2(x, wave.center().y),
                wave.height() * 0.6,
                ACCENT_B,
                0.5 + 0.3 * self.beat.pulse,
            );
            painter.line_segment(
                [pos2(x, wave.top()), pos2(x, wave.bottom())],
                Stroke::new(2.0, Color32::WHITE.gamma_multiply(0.85)),
            );
        }

        let cancel = Rect::from_center_size(
            pos2(inner.center().x, inner.bottom() - 22.0),
            vec2(160.0, 44.0),
        );
        if ui
            .put(cancel, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, "Batal (Esc)", cancel.size())
            })
            .clicked()
        {
            self.cancel();
        }
        None
    }
}

/// Dashed rounded-ish border for the drop zone.
pub(super) fn dashed_rect(painter: &egui::Painter, r: Rect, color: Color32) {
    let stroke = Stroke::new(2.0, color);
    let corners = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    for w in corners.windows(2) {
        painter.extend(egui::Shape::dashed_line(&[w[0], w[1]], stroke, 10.0, 8.0));
    }
}

/// Mirrored waveform; the part already processed is drawn in the accent gradient.
pub(super) fn draw_waveform(
    painter: &egui::Painter,
    r: Rect,
    peaks: &[f32],
    progress: f32,
    palette: &theme::Palette,
) {
    if peaks.is_empty() {
        // Placeholder shimmer line until decoding finishes.
        painter.line_segment(
            [r.left_center(), r.right_center()],
            Stroke::new(2.0, palette.border.gamma_multiply(3.0)),
        );
        return;
    }
    let max = peaks.iter().copied().fold(1e-6f32, f32::max);
    let w = r.width() / peaks.len() as f32;
    for (i, p) in peaks.iter().enumerate() {
        let x = r.left() + (i as f32 + 0.5) * w;
        let h = (p / max).sqrt() * r.height() * 0.48;
        let frac = i as f32 / peaks.len() as f32;
        let color = if frac <= progress {
            ACCENT_A.lerp_to_gamma(ACCENT_B, frac)
        } else {
            palette.text_dim.gamma_multiply(0.35)
        };
        painter.line_segment(
            [
                pos2(x, r.center().y - h.max(1.0)),
                pos2(x, r.center().y + h.max(1.0)),
            ],
            Stroke::new((w * 0.6).max(1.0), color),
        );
    }
}

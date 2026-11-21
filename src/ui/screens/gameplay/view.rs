//! Gameplay drawing: stage, notes, judgment and combo, HUD and the pause menu.

use super::*;

impl GameplayScreen {
    pub(super) fn draw(&mut self, ui: &mut Ui, shared: &mut Shared, now: Instant) {
        let rect = ui.max_rect();
        let painter = ui.painter().clone();
        let reduce = shared.settings.reduce_motion;
        let dt = ui.input(|i| i.stable_dt);
        let time = anim::since(shared.start, now);

        let scale = shared
            .settings
            .lane_size
            .scale(shared.gamepad_name.is_some());
        // Keep a big lane clear of the song card (left) and score card (right).
        let layout = Layout::new(rect, self.columns, shared.settings.reverse, scale)
            .limit_width(self.columns, rect.width() - 2.0 * HUD_SIDE);
        let beat = self.timing.seconds_to_beat(self.song_time);
        let pulse = if reduce {
            0.0
        } else {
            (1.0 - beat.rem_euclid(1.0) as f32).powi(4)
        };
        let pressed: Vec<bool> = (0..self.columns)
            .map(|c| self.session.is_pressed(c))
            .collect();
        if self.pause.is_none() {
            let failed = self.session.score.failed && !shared.settings.no_fail;
            self.hud
                .update(&self.session.score, failed, &pressed, dt, reduce);
        }

        let stage = Stage {
            rect,
            layout: &layout,
            columns: self.columns,
            tint: self.tint,
            brightness: shared.settings.background_brightness.clamp(0.0, 1.0),
            energy: self.hud.energy(),
            pulse,
            time,
            reduce,
            beat,
        };
        let scroll = Scroll::new(&shared.settings, &layout, &self.timing, self.song_time);
        hud::draw_stage(&painter, &stage);
        hud::draw_lane(&painter, &stage, &scroll, &self.timing, &self.hud);
        if let Some((_, age)) = self.hud.milestone(now).filter(|_| !reduce) {
            hud::draw_milestone_surge(&painter, &stage, age);
        }

        notefield::draw_receptors(&painter, &layout, self.columns, &pressed, pulse, false);
        let clipped = painter.with_clip_rect(rect);
        notefield::draw_notes(&clipped, &layout, &scroll, &self.session.notes);
        for (fx, start) in &self.effects {
            let fx = HitEffect {
                age: anim::since(*start, now),
                ..*fx
            };
            notefield::draw_hit_effect(&painter, &layout, &fx);
        }

        let feedback = self.feedback_anchor(&layout, rect);
        self.draw_judgment(&painter, feedback, layout.scale(), now, reduce);
        hud::draw_hit_meter(
            &painter,
            feedback + vec2(0.0, 132.0 * layout.scale()),
            &self.hud,
            shared.settings.judge_scale,
            now,
        );
        self.draw_hud(&painter, &stage, shared);
        if self.pause.is_none() && self.pause_button(ui, rect) {
            self.set_paused(true, now);
        }

        if let Some(start) = self.resuming
            && self.pause.is_none()
        {
            painter.rect_filled(rect, 0, Color32::from_black_alpha(110));
            let t = f64::from(anim::since(start, now) - Self::RESUME_COUNT);
            hud::draw_countdown(
                &painter,
                rect,
                t.min(-0.001),
                &hud::CountdownInfo {
                    title: &self.title,
                    subtitle: &self.subtitle,
                    tint: self.tint,
                },
                reduce,
            );
            ui.ctx().request_repaint();
        } else if self.song_time < 1.0 && self.pause.is_none() {
            hud::draw_countdown(
                &painter,
                rect,
                self.song_time,
                &hud::CountdownInfo {
                    title: &self.title,
                    subtitle: &self.subtitle,
                    tint: self.tint,
                },
                reduce,
            );
        }
        if self.pause.is_some() {
            self.draw_pause(ui, shared);
        }
    }
}

impl GameplayScreen {
    /// Where judgment and combo appear: middle of the lane, away from the receptors.
    pub(super) fn feedback_anchor(&self, layout: &Layout, rect: Rect) -> egui::Pos2 {
        let y = if layout.dir > 0.0 {
            rect.center().y + rect.height() * 0.02
        } else {
            rect.center().y - rect.height() * 0.18
        };
        pos2(layout.left + layout.width(self.columns) / 2.0, y)
    }

    /// Judgment word and combo; `k` is the lane scale (bigger lane, bigger text).
    pub(super) fn draw_judgment(
        &self,
        painter: &egui::Painter,
        pos: egui::Pos2,
        k: f32,
        now: Instant,
        reduce: bool,
    ) {
        let score = &self.session.score;
        if let Some((judgment, at)) = self.last_judgment {
            let age = anim::since(at, now);
            if age <= 0.8 {
                // Pop in with a quick overshoot, then fade.
                let pop = if reduce {
                    1.0
                } else {
                    1.0 + 0.28 * (1.0 - anim::ease_out_cubic(age / 0.14))
                };
                let alpha = ((0.8 - age) / 0.25).clamp(0.0, 1.0);
                let mut color = judgment_color(judgment);
                if judgment == Judgment::Marvelous && !reduce {
                    let shimmer = 0.5 + 0.5 * (anim::since(at, now) * 14.0).sin();
                    color = color.lerp_to_gamma(Color32::WHITE, shimmer * 0.6);
                }
                notefield::draw_glow(painter, pos, 46.0 * k * pop, color, alpha * 0.9);
                widgets::text_tracked_scaled(
                    painter,
                    pos,
                    Align2::CENTER_CENTER,
                    judgment.label(),
                    font_bold(36.0 * k),
                    color.gamma_multiply(alpha),
                    pop,
                );
                // Early/late hint only when it matters (Great and worse).
                if let Some((_, ms)) = score.offsets.last().filter(|_| {
                    matches!(judgment, Judgment::Great | Judgment::Good | Judgment::Boo)
                }) {
                    let (word, c) = if *ms < 0.0 {
                        ("CEPAT", theme::ACCENT_B)
                    } else {
                        ("LAMBAT", Color32::from_rgb(0xFF, 0x9F, 0x5C))
                    };
                    widgets::text_tracked(
                        painter,
                        pos + vec2(0.0, -32.0 * k),
                        Align2::CENTER_CENTER,
                        &format!("{word}  {:.0} ms", ms.abs()),
                        font_bold(11.0 * k),
                        c.gamma_multiply(alpha),
                    );
                }
            }
        }

        let combo = score.combo;
        if combo >= 4 {
            let bump = if reduce {
                1.0
            } else {
                1.0 + 0.14 * (1.0 - anim::ease_out_cubic(anim::since(self.combo_bump, now) / 0.16))
            };
            // Full combo so far (no Good/Boo/Miss): the number turns gold.
            let full_combo = score.counts[Judgment::Good.index()]
                + score.counts[Judgment::Boo.index()]
                + score.counts[Judgment::Miss.index()]
                == 0;
            let color = if full_combo {
                Color32::from_rgb(0xFF, 0xE9, 0xA8)
            } else {
                Color32::WHITE.gamma_multiply(0.9)
            };
            let center = pos + vec2(0.0, 58.0 * k);
            if let Some((_, age)) = self.hud.milestone(now).filter(|_| !reduce) {
                let e = anim::ease_out_cubic((age / 0.9).min(1.0));
                let fade = (1.0 - age / 0.9).max(0.0);
                painter.circle_stroke(
                    center,
                    (30.0 + e * 120.0) * k,
                    Stroke::new(3.0 * fade, theme::ACCENT_B.gamma_multiply(fade)),
                );
                notefield::draw_glow(painter, center, 60.0 * k, theme::ACCENT_A, fade * 1.5);
            }
            widgets::tabular_text_scaled(
                painter,
                center,
                Align2::CENTER_CENTER,
                &combo.to_string(),
                font_bold(50.0 * k),
                color,
                bump,
            );
            let label_pos = center + vec2(0.0, 36.0 * k);
            match self.hud.milestone(now) {
                Some((c, age)) => {
                    let a = (1.4 - age).clamp(0.0, 1.0);
                    widgets::text_tracked(
                        painter,
                        label_pos,
                        Align2::CENTER_CENTER,
                        hud::milestone_word(c),
                        font_bold(theme::TEXT_S * k),
                        theme::ACCENT_B
                            .lerp_to_gamma(Color32::WHITE, 0.3)
                            .gamma_multiply(a),
                    );
                }
                None => {
                    widgets::text_tracked(
                        painter,
                        label_pos,
                        Align2::CENTER_CENTER,
                        "COMBO",
                        font_bold(11.0 * k),
                        color.gamma_multiply(0.55),
                    );
                }
            }
        }
    }

    /// A pause button under the song card, for mouse players (Esc works too).
    pub(super) fn pause_button(&self, ui: &mut Ui, rect: Rect) -> bool {
        let r = Rect::from_min_size(rect.left_top() + vec2(24.0, 112.0), vec2(96.0, 32.0));
        let resp = ui
            .interact(r, ui.scope_id().with("pause-button"), egui::Sense::click())
            .on_hover_text("Jeda (Esc)");
        let hover = ui.ctx().animate_bool(resp.id, resp.hovered());
        let painter = ui.painter();
        let r = if resp.is_pointer_button_down_on() {
            Rect::from_center_size(r.center(), r.size() * 0.96)
        } else {
            r
        };
        hud::glass(painter, r);
        painter.rect_filled(
            r,
            theme::RADIUS_CARD,
            Color32::WHITE.gamma_multiply(0.06 * hover),
        );
        let icon = pos2(r.left() + 22.0, r.center().y);
        for dx in [-4.0, 4.0] {
            painter.rect_filled(
                Rect::from_center_size(icon + vec2(dx, 0.0), vec2(4.0, 13.0)),
                1,
                Color32::WHITE.gamma_multiply(0.85),
            );
        }
        painter.text(
            pos2(r.left() + 38.0, r.center().y),
            Align2::LEFT_CENTER,
            "Jeda",
            font_bold(theme::TEXT_S),
            Color32::WHITE.gamma_multiply(0.85 + 0.15 * hover),
        );
        resp.clicked()
    }

    pub(super) fn draw_hud(&self, painter: &egui::Painter, stage: &Stage, shared: &Shared) {
        let rect = stage.rect;
        hud::draw_song_island(
            painter,
            rect,
            &SongInfo {
                title: &self.title,
                subtitle: &self.subtitle,
                difficulty_color: self.difficulty_color,
                tint: self.tint,
                time: self.song_time,
                length: self.song_length,
            },
        );
        let failed = self.session.score.failed && !shared.settings.no_fail;
        hud::draw_score_island(painter, rect, &self.session.score, &self.hud, failed);
        hud::draw_life_gauge(
            painter,
            stage,
            &self.hud,
            shared.settings.no_fail,
            self.session.score.failed,
        );
        if shared.autoplay {
            let pill = Rect::from_min_size(rect.left_top() + vec2(132.0, 116.0), vec2(96.0, 24.0));
            painter.rect_filled(
                pill,
                theme::RADIUS_PILL,
                theme::ACCENT_B.gamma_multiply(0.15),
            );
            widgets::text_tracked(
                painter,
                pill.center(),
                Align2::CENTER_CENTER,
                "AUTOPLAY",
                font_bold(11.0),
                theme::ACCENT_B,
            );
        }
        if shared.settings.show_fps {
            let dt = painter.ctx().input(|i| i.unstable_dt).max(1e-4);
            painter.text(
                rect.right_bottom() - vec2(16.0, 16.0),
                Align2::RIGHT_BOTTOM,
                format!("{:.0} FPS", 1.0 / dt),
                font(theme::TEXT_XS),
                Color32::WHITE.gamma_multiply(0.6),
            );
        }
    }

    pub(super) fn draw_pause(&mut self, ui: &mut Ui, shared: &mut Shared) {
        let rect = ui.max_rect();
        let painter = ui.painter().clone();
        painter.rect_filled(rect, 0, Color32::from_black_alpha(170));
        let palette = shared.palette;
        let card = Rect::from_center_size(rect.center(), vec2(420.0, 470.0));
        crate::ui::notefield::radial_light(&painter, card.center(), 420.0, self.tint, 0.25);
        widgets::card(&painter, card, &palette);
        // Four column colors across the top: this is a rhythm game.
        let stripe_w = (card.width() - 48.0) / 4.0;
        for c in 0..4u8 {
            let r = Rect::from_min_size(
                card.left_top() + vec2(24.0 + f32::from(c) * stripe_w, 0.0),
                vec2(stripe_w - 6.0, 4.0),
            );
            painter.rect_filled(r, 2, theme::column_color(c));
        }
        widgets::text_tracked(
            &painter,
            card.center_top() + vec2(0.0, 40.0),
            Align2::CENTER_CENTER,
            "D I J E D A",
            font_bold(theme::TEXT_L),
            palette.text,
        );
        painter.text(
            card.center_top() + vec2(0.0, 74.0),
            Align2::CENTER_CENTER,
            widgets::truncate(&self.title, 34),
            font_bold(theme::TEXT_M),
            palette.text,
        );
        painter.text(
            card.center_top() + vec2(0.0, 98.0),
            Align2::CENTER_CENTER,
            &self.subtitle,
            font(theme::TEXT_S),
            self.difficulty_color,
        );
        // Song progress + live stats.
        let progress = (self.song_time / self.song_length.max(0.001)).clamp(0.0, 1.0) as f32;
        let bar = Rect::from_min_size(
            card.left_top() + vec2(40.0, 124.0),
            vec2(card.width() - 80.0, 6.0),
        );
        painter.rect_filled(bar, 3, palette.border);
        widgets::gradient_pill(
            &painter,
            Rect::from_min_size(
                bar.min,
                vec2((bar.width() * progress).max(6.0), bar.height()),
            ),
            theme::ACCENT_A,
            theme::ACCENT_B,
        );
        let score = &self.session.score;
        let stats = [
            ("Progres", format!("{:.0}%", progress * 100.0)),
            ("Akurasi", format!("{:.2}%", score.accuracy_percent())),
            ("Combo", score.combo.to_string()),
        ];
        let col_w = bar.width() / 3.0;
        for (i, (label, value)) in stats.iter().enumerate() {
            let x = bar.left() + col_w * (i as f32 + 0.5);
            painter.text(
                pos2(x, bar.bottom() + 14.0),
                Align2::CENTER_TOP,
                *label,
                font(theme::TEXT_XS),
                palette.text_dim,
            );
            widgets::tabular_text(
                &painter,
                pos2(x, bar.bottom() + 32.0),
                Align2::CENTER_TOP,
                value,
                font_bold(theme::TEXT_M),
                palette.text,
            );
        }

        let mut clicked = None;
        let selected = self.pause.as_ref().map_or(0, |p| p.selected);
        for (i, label) in PAUSE_ITEMS.iter().enumerate() {
            let r = Rect::from_center_size(
                card.center_top() + vec2(0.0, 250.0 + i as f32 * 60.0),
                vec2(300.0, 48.0),
            );
            let resp = ui.put(r, |ui: &mut Ui| {
                if i == selected {
                    widgets::primary_button(ui, label, r.size(), true)
                } else {
                    widgets::secondary_button(ui, &palette, label, r.size())
                }
            });
            if resp.clicked() {
                clicked = Some(i);
            }
            if resp.hovered()
                && let Some(p) = &mut self.pause
            {
                p.selected = i;
            }
        }
        widgets::key_hints(
            &painter,
            card.center_bottom() + vec2(0.0, 36.0),
            Align2::CENTER_CENTER,
            &[("↑ ↓", "Pilih"), ("Enter", "OK"), ("Esc", "Lanjutkan")],
            &palette,
        );
        if let Some(i) = clicked {
            if let Some(p) = &mut self.pause {
                p.selected = i;
            }
            shared.pending_nav = self.pause_choose(Instant::now());
        }
    }
}

//! Window-level behavior shared by every screen: menu sounds, cursor, dropped files, look and FPS log.

use super::*;

impl FerrostepApp {
    /// Arcade-style feedback: a sound for every menu move, choice and click.
    /// Silent while playing (the music and your steps are the sound) and
    /// during calibration (it needs to hear only the metronome).
    pub(super) fn menu_sounds(&mut self, ctx: &egui::Context, input: &FrameInput) {
        let hovered = hovered_clickable(ctx);
        let new_hover = hovered.is_some() && hovered != self.last_hover;
        self.last_hover = hovered;
        let quiet = match &self.screen {
            Screen::Gameplay(g) => !g.is_paused(),
            Screen::Settings(s) => s.is_calibrating(),
            Screen::Onboarding(o) => o.is_calibrating() || o.in_tutorial(),
            Screen::PadSetup(_) | Screen::Tutorial(_) => true,
            _ => false,
        };
        if quiet {
            return;
        }
        let sound = input
            .menu
            .last()
            .map(|a| sfx_for(*a))
            .or_else(|| {
                ctx.interaction_snapshot(|i| i.clicked)
                    .map(|_| Sfx::Confirm)
            })
            .or_else(|| new_hover.then_some(Sfx::Hover));
        if let Some(sound) = sound {
            self.shared.sfx(sound);
        }
    }
}

impl FerrostepApp {
    /// Logs the frame rate every 2 s (`RUST_LOG=ferrostep=debug`).
    ///
    /// `work` is the CPU time this frame took to build. The FPS itself is often
    /// capped by the compositor/monitor, so the CPU time shows the real headroom.
    pub(super) fn log_fps(&mut self, now: Instant, work: f32) {
        self.fps_frames += 1;
        self.fps_work += work;
        let elapsed = anim::since(self.fps_since, now);
        if elapsed >= 2.0 {
            let avg_ms = self.fps_work / self.fps_frames as f32 * 1000.0;
            log::debug!(
                "fps: {:.0}, frame build {avg_ms:.2} ms (~{:.0} fps possible)",
                self.fps_frames as f32 / elapsed,
                1000.0 / avg_ms.max(0.01)
            );
            self.fps_frames = 0;
            self.fps_since = now;
            self.fps_work = 0.0;
        }
    }

    /// Re-applies theme and UI scale after they change in Settings.
    pub(super) fn apply_look(&mut self, ctx: &egui::Context) {
        let wanted = (self.shared.settings.theme, self.shared.settings.ui_scale);
        if wanted != self.shared.applied_look {
            self.shared.palette = Palette::for_choice(wanted.0);
            theme::apply(ctx, &self.shared.palette, wanted.1);
            self.shared.applied_look = wanted;
        }
    }

    /// Files dropped on the window open the import screen (from any screen but gameplay).
    pub(super) fn handle_dropped_files(
        &mut self,
        ui: &egui::Ui,
        ctx: &egui::Context,
    ) -> Option<Nav> {
        let (hovered, dropped) = ctx.input(|i| {
            let dropped: Vec<std::path::PathBuf> = i
                .raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect();
            (!i.raw.hovered_files.is_empty(), dropped)
        });
        let in_game = matches!(self.screen, Screen::Gameplay(_));
        if hovered && !in_game {
            let rect = ui.max_rect();
            let painter = ui.ctx().layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::unique("drop-overlay"),
            ));
            painter.rect_filled(rect, 0, Color32::from_black_alpha(170));
            painter.rect_stroke(
                rect.shrink(16.0),
                theme::RADIUS_CARD,
                egui::Stroke::new(3.0, theme::ACCENT_B),
                egui::StrokeKind::Inside,
            );
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Lepaskan untuk mengimpor",
                theme::font_bold(theme::TEXT_XL),
                Color32::WHITE,
            );
        }
        if dropped.is_empty() {
            return None;
        }
        let files: Vec<_> = dropped
            .into_iter()
            .filter(|p| crate::importer::is_importable(p))
            .collect();
        if files.is_empty() {
            self.shared
                .toasts
                .warning("Hanya file MP3, MP4, M4A, OGG, WAV, atau FLAC yang bisa diimpor.");
            None
        } else if in_game {
            self.shared
                .toasts
                .info("Selesaikan lagu dulu, lalu seret file lagi untuk mengimpor.");
            None
        } else {
            Some(Nav::Import(files))
        }
    }
}

/// Shows a pointing hand over anything clickable (our custom widgets draw
/// themselves, so egui would otherwise keep the arrow). Widgets that chose
/// their own cursor (text fields, dragged lists) keep it.
pub(super) fn pointer_cursor(ctx: &egui::Context) {
    if ctx.output(|o| o.cursor_icon) != egui::CursorIcon::Default {
        return;
    }
    if hovered_clickable(ctx).is_some() {
        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

/// The enabled, clickable widget under the pointer, if any.
pub(super) fn hovered_clickable(ctx: &egui::Context) -> Option<egui::Id> {
    ctx.interaction_snapshot(|i| i.hovered.clone())
        .into_iter()
        .find(|id| {
            ctx.read_response(*id)
                .is_some_and(|r| r.enabled() && r.sense.senses_click())
        })
}

/// Which sound a menu action makes.
pub(super) fn sfx_for(action: MenuAction) -> Sfx {
    match action {
        MenuAction::Up => Sfx::MoveUp,
        MenuAction::Down => Sfx::MoveDown,
        MenuAction::Left => Sfx::ChangeLeft,
        MenuAction::Right => Sfx::ChangeRight,
        MenuAction::Confirm => Sfx::Confirm,
        MenuAction::Back => Sfx::Back,
    }
}

/// Four light streaks in the arrow colors that fly across the screen during a
/// screen change, in the direction of travel (no extra delay: they ride the
/// same spring as the slide).
pub(super) fn draw_transition_streaks(painter: &egui::Painter, full: egui::Rect, e: f32, dir: f32) {
    for c in 0..4u8 {
        let p = ((e - f32::from(c) * 0.07) / 0.75).clamp(0.0, 1.0);
        if p <= 0.0 || p >= 1.0 {
            continue;
        }
        let travel = anim::ease_out_cubic(p);
        // Enter from the side the new screen comes from.
        let x = if dir > 0.0 {
            full.right() - travel * (full.width() + 120.0)
        } else {
            full.left() + travel * (full.width() + 120.0)
        };
        let color = theme::column_color(c);
        let alpha = (1.0 - p) * 0.8;
        let streak = egui::Rect::from_min_max(
            egui::pos2(x, full.top()),
            egui::pos2(x + 120.0 * dir.signum(), full.bottom()),
        );
        let streak =
            egui::Rect::from_min_max(streak.min.min(streak.max), streak.min.max(streak.max));
        let (left, right) = if dir > 0.0 {
            (color.gamma_multiply(alpha), Color32::TRANSPARENT)
        } else {
            (Color32::TRANSPARENT, color.gamma_multiply(alpha))
        };
        widgets::gradient_rect(painter, streak, left, right);
        let edge = if dir > 0.0 {
            streak.left()
        } else {
            streak.right()
        };
        painter.line_segment(
            [
                egui::pos2(edge, full.top()),
                egui::pos2(edge, full.bottom()),
            ],
            egui::Stroke::new(2.0, Color32::WHITE.gamma_multiply(alpha)),
        );
    }
}

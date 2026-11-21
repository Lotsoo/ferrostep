//! Deleting a song: confirmation dialog and the delayed removal.

use super::*;

/// "Delete this song?" confirmation.
pub(super) struct DeleteDialog {
    pub(super) dir: PathBuf,
    pub(super) title: String,
    /// 0 = Batal (default, safe), 1 = Hapus.
    pub(super) selected: usize,
    pub(super) opened: Instant,
}

/// Wait this long after stopping the preview before deleting, so the audio
/// stream has released the file (Windows refuses to delete open files).
pub(super) const DELETE_DELAY: f32 = 0.35;

impl SongSelectScreen {
    pub(super) fn ask_delete(&mut self, song: &SongEntry, now: Instant) {
        self.delete_dialog = Some(DeleteDialog {
            dir: song.dir.clone(),
            title: song.title(),
            selected: 0,
            opened: now,
        });
    }

    /// Confirmation card for deleting a song. Keyboard/mat: ← → choose,
    /// Enter confirm, Esc cancel.
    pub(super) fn delete_dialog_ui(
        &mut self,
        ctx: &egui::Context,
        shared: &mut Shared,
        input: &FrameInput,
        songs: &[Arc<SongEntry>],
    ) {
        let Some(dialog) = &mut self.delete_dialog else {
            return;
        };
        let mut choice = None;
        for action in &input.menu {
            match action {
                MenuAction::Left | MenuAction::Right | MenuAction::Up | MenuAction::Down => {
                    dialog.selected = 1 - dialog.selected;
                }
                MenuAction::Confirm => choice = Some(dialog.selected == 1),
                MenuAction::Back => choice = Some(false),
            }
        }
        let palette = shared.palette;
        let appear = anim::ease_out_cubic(anim::progress(
            dialog.opened,
            input.now,
            0.2,
            shared.settings.reduce_motion,
        ));
        let selected = dialog.selected;
        let title = dialog.title.clone();
        // Show where it lives relative to the songs folder ("Songs/Group/Song").
        let folder = dialog.dir.strip_prefix(&shared.library.root).map_or_else(
            |_| dialog.dir.display().to_string(),
            |rel| format!("Songs/{}", rel.to_string_lossy().replace('\\', "/")),
        );
        let modal = egui::Modal::new(egui::Id::unique("delete-song-dialog"))
            .backdrop_color(Color32::from_black_alpha((160.0 * appear) as u8))
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let size = vec2(480.0, 260.0);
                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                let rect = Rect::from_center_size(
                    rect.center() + vec2(0.0, (1.0 - appear) * 16.0),
                    rect.size() * (0.96 + 0.04 * appear),
                );
                let painter = ui.painter();
                widgets::card(painter, rect, &palette);
                // Red stripe: this action cannot be undone.
                painter.rect_filled(
                    Rect::from_min_size(rect.min + vec2(24.0, 0.0), vec2(rect.width() - 48.0, 3.0)),
                    2,
                    theme::DANGER,
                );
                widgets::text_tracked(
                    painter,
                    rect.left_top() + vec2(28.0, 28.0),
                    Align2::LEFT_TOP,
                    "Hapus lagu ini?",
                    font_bold(theme::TEXT_L),
                    palette.text,
                );
                let text_w = rect.width() - 56.0;
                painter.text(
                    rect.left_top() + vec2(28.0, 66.0),
                    Align2::LEFT_TOP,
                    widgets::elide(painter, &title, font_bold(theme::TEXT_M), text_w),
                    font_bold(theme::TEXT_M),
                    palette.text,
                );
                painter.text(
                    rect.left_top() + vec2(28.0, 96.0),
                    Align2::LEFT_TOP,
                    widgets::elide(painter, &folder, font(theme::TEXT_XS), text_w),
                    font(theme::TEXT_XS),
                    palette.text_dim,
                );
                painter.text(
                    rect.left_top() + vec2(28.0, 122.0),
                    Align2::LEFT_TOP,
                    "Folder ini beserta audio dan chart-nya dihapus permanen dari disk.",
                    font(theme::TEXT_S),
                    theme::DANGER,
                );
                let mut clicked = None;
                for (i, label) in ["Batal", "Hapus"].into_iter().enumerate() {
                    let r = Rect::from_min_size(
                        rect.left_bottom() + vec2(28.0 + i as f32 * 220.0, -76.0),
                        vec2(196.0, 48.0),
                    );
                    let resp = ui.put(r, |ui: &mut Ui| {
                        if selected == i && i == 1 {
                            danger_button(ui, label, r.size())
                        } else if selected == i {
                            widgets::primary_button(ui, label, r.size(), true)
                        } else {
                            widgets::secondary_button(ui, &palette, label, r.size())
                        }
                    });
                    if resp.hovered()
                        && let Some(d) = &mut self.delete_dialog
                    {
                        d.selected = i;
                    }
                    if resp.clicked() {
                        clicked = Some(i == 1);
                    }
                }
                clicked
            });
        let choice = choice
            .or(modal.inner)
            .or(modal.should_close().then_some(false));
        match choice {
            Some(true) => {
                if let Some(d) = self.delete_dialog.take() {
                    self.confirm_delete(d, songs, input.now);
                }
            }
            Some(false) => self.delete_dialog = None,
            None => {}
        }
        ctx.request_repaint();
    }

    /// Stops the preview, moves the selection to a neighbour and schedules
    /// the deletion.
    pub(super) fn confirm_delete(
        &mut self,
        dialog: DeleteDialog,
        songs: &[Arc<SongEntry>],
        now: Instant,
    ) {
        if let Some(p) = &mut self.preview
            && p.dir == dialog.dir
        {
            p.music.fade_out(0);
            self.preview = None;
        }
        let pos = self
            .filtered
            .iter()
            .position(|&i| songs[i].dir == dialog.dir);
        if let Some(pos) = pos {
            let neighbour = self
                .filtered
                .get(pos + 1)
                .or_else(|| pos.checked_sub(1).and_then(|p| self.filtered.get(p)));
            self.selected_dir = neighbour.map(|&i| songs[i].dir.clone());
        }
        self.pending_delete = Some((dialog.dir, dialog.title, now));
    }

    pub(super) fn run_pending_delete(&mut self, shared: &mut Shared, now: Instant) {
        let Some((_, _, at)) = &self.pending_delete else {
            return;
        };
        if anim::since(*at, now) < DELETE_DELAY {
            return;
        }
        let Some((dir, title, _)) = self.pending_delete.take() else {
            return;
        };
        match shared.library.delete_song(&dir) {
            Ok(()) => {
                log::info!("deleted song folder {}", dir.display());
                shared.sfx(crate::audio::Sfx::Back);
                shared
                    .toasts
                    .success(format!("Lagu dihapus: {}", widgets::truncate(&title, 40)));
            }
            Err(e) => {
                log::warn!("could not delete {}: {e}", dir.display());
                shared.toasts.warning(format!(
                    "Gagal menghapus \"{}\". Tutup aplikasi lain yang memakai filenya, lalu coba lagi.",
                    widgets::truncate(&title, 30)
                ));
            }
        }
    }
}

/// Red call-to-action for destructive actions (only when it has focus).
pub(super) fn danger_button(ui: &mut Ui, text: &str, size: egui::Vec2) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let pressed = response.is_pointer_button_down_on();
    let rect = if pressed { rect.shrink(1.5) } else { rect };
    let painter = ui.painter();
    let fill = if pressed {
        theme::DANGER.lerp_to_gamma(Color32::BLACK, 0.15)
    } else {
        theme::DANGER
    };
    painter.rect_filled(rect, theme::RADIUS_BUTTON, fill);
    painter.rect_stroke(
        rect.expand(2.0),
        theme::RADIUS_BUTTON,
        Stroke::new(2.0, Color32::WHITE.gamma_multiply(0.35)),
        StrokeKind::Outside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        font_bold(theme::TEXT_M),
        Color32::WHITE,
    );
    response
}

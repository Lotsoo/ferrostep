//! The "really quit?" dialog.

use super::*;

/// "Really quit?" dialog state.
pub(super) struct QuitDialog {
    /// 0 = Batal (default, safe), 1 = Keluar.
    pub(super) selected: usize,
    pub(super) opened: Instant,
}

impl FerrostepApp {
    /// Asks before quitting (window close button, Alt+F4, Esc on the title).
    pub(super) fn open_quit_dialog(&mut self) {
        if self.quit_dialog.is_none() {
            self.quit_dialog = Some(QuitDialog {
                selected: 0,
                opened: Instant::now(),
            });
            if let Screen::Gameplay(g) = &mut self.screen {
                g.pause_now();
            }
        }
    }

    /// Draws the quit dialog and handles its keys. Returns true when it is open.
    pub(super) fn quit_dialog_ui(&mut self, ctx: &egui::Context, input: &FrameInput) -> bool {
        let Some(dialog) = &mut self.quit_dialog else {
            return false;
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
        let palette = self.shared.palette;
        let appear = anim::ease_out_cubic(anim::progress(
            dialog.opened,
            input.now,
            0.2,
            self.shared.settings.reduce_motion,
        ));
        let importing = matches!(&self.screen, Screen::Import(s) if s.is_busy());
        let selected = dialog.selected;
        let modal = egui::Modal::new(egui::Id::unique("quit-dialog"))
            .backdrop_color(Color32::from_black_alpha((160.0 * appear) as u8))
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let size = vec2(420.0, 210.0);
                let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                let rect = egui::Rect::from_center_size(
                    rect.center() + vec2(0.0, (1.0 - appear) * 16.0),
                    rect.size() * (0.96 + 0.04 * appear),
                );
                let painter = ui.painter();
                widgets::card(painter, rect, &palette);
                widgets::text_tracked(
                    painter,
                    rect.left_top() + vec2(28.0, 28.0),
                    egui::Align2::LEFT_TOP,
                    "Keluar dari Ferrostep?",
                    theme::font_bold(theme::TEXT_L),
                    palette.text,
                );
                painter.text(
                    rect.left_top() + vec2(28.0, 68.0),
                    egui::Align2::LEFT_TOP,
                    if importing {
                        "Impor yang sedang berjalan akan dibatalkan."
                    } else {
                        "Pengaturan dan skormu sudah tersimpan otomatis."
                    },
                    theme::font(theme::TEXT_S),
                    palette.text_dim,
                );
                let mut clicked = None;
                let buttons = [("Batal", 0usize), ("Keluar", 1usize)];
                for (i, (label, value)) in buttons.into_iter().enumerate() {
                    let r = egui::Rect::from_min_size(
                        rect.left_bottom() + vec2(28.0 + i as f32 * 194.0, -76.0),
                        vec2(170.0, 48.0),
                    );
                    let resp = ui.put(r, |ui: &mut egui::Ui| {
                        if selected == value {
                            widgets::primary_button(ui, label, r.size(), true)
                        } else {
                            widgets::secondary_button(ui, &palette, label, r.size())
                        }
                    });
                    if resp.clicked() {
                        clicked = Some(value == 1);
                    }
                }
                clicked
            });
        let choice = choice
            .or(modal.inner)
            .or(modal.should_close().then_some(false));
        match choice {
            Some(true) => {
                self.allow_close = true;
                if let Screen::Import(s) = &mut self.screen {
                    s.cancel_all();
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Some(false) => self.quit_dialog = None,
            None => {}
        }
        ctx.request_repaint();
        true
    }
}

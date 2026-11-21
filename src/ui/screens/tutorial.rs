//! The tutorial as its own screen (opened from the main menu).

use eframe::egui::{Rect, Ui, pos2, vec2};

use super::Nav;
use crate::app::Shared;
use crate::input::{FrameInput, MenuAction};
use crate::ui::tutorial::{Tutorial, TutorialEvent};
use crate::ui::{anim, beatfx, theme, widgets};

pub struct TutorialScreen {
    tutorial: Tutorial,
}

impl Default for TutorialScreen {
    fn default() -> Self {
        Self {
            tutorial: Tutorial::new(false),
        }
    }
}

impl TutorialScreen {
    pub fn ui(&mut self, ui: &mut Ui, shared: &mut Shared, input: &FrameInput) -> Option<Nav> {
        let rect = ui.max_rect();
        let palette = shared.palette;
        if palette.dark {
            let beat = beatfx::Beat::idle(anim::since(shared.start, input.now)).calm();
            beatfx::draw_highway(ui.painter(), rect, theme::ACCENT_A, &beat);
        }
        if input.has(MenuAction::Back) {
            return Some(Nav::Title);
        }
        let exit = Rect::from_min_size(
            pos2(rect.right() - 172.0, rect.top() + 28.0),
            vec2(140.0, 40.0),
        );
        if ui
            .put(exit, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, "Keluar  (Esc)", exit.size())
            })
            .clicked()
        {
            return Some(Nav::Title);
        }
        let area = Rect::from_min_max(
            pos2(rect.left() + 32.0, exit.bottom() + 8.0),
            pos2(rect.right() - 32.0, rect.bottom() - 16.0),
        );
        match self.tutorial.ui(ui, area, shared, input)? {
            TutorialEvent::Play => Some(Nav::SongSelect),
            TutorialEvent::Calibrate => Some(Nav::Calibrate),
            TutorialEvent::Next | TutorialEvent::Menu => Some(Nav::Title),
        }
    }
}

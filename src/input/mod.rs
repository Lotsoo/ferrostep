//! Unified input: keyboard and (from phase 3) gamepads / dance mats.
//!
//! Screens never look at raw keys for gameplay; they receive
//! [`ColumnEvent`]s and [`MenuAction`]s from [`InputHub::poll`].

use std::time::Instant;

use eframe::egui;

use crate::config::Settings;

/// Where an input came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    Keyboard,
    Gamepad,
}

/// A gameplay panel/key changed state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnEvent {
    /// 0 = Left, 1 = Down, 2 = Up, 3 = Right.
    pub column: u8,
    pub pressed: bool,
    /// When the event happened (best estimate).
    pub at: Instant,
    pub device: Device,
}

/// Menu navigation, from keyboard arrows or dance mat panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
}

/// Everything that happened this frame.
#[derive(Debug, Clone)]
pub struct FrameInput {
    pub now: Instant,
    pub columns: Vec<ColumnEvent>,
    pub menu: Vec<MenuAction>,
    /// Last keyboard key pressed this frame (for the key-binding screen).
    pub last_key: Option<egui::Key>,
    /// True while a text field has keyboard focus (menu keys are then ignored).
    pub typing: bool,
}

impl FrameInput {
    pub fn has(&self, action: MenuAction) -> bool {
        self.menu.contains(&action)
    }
}

/// Keyboard binding lookup built from settings.
fn column_for_key(settings: &Settings, key: egui::Key) -> Option<u8> {
    settings
        .keybinds
        .iter()
        .position(|names| names.iter().any(|n| egui::Key::from_name(n) == Some(key)))
        .map(|c| c as u8)
}

fn menu_for_key(key: egui::Key) -> Option<MenuAction> {
    use egui::Key;
    Some(match key {
        Key::ArrowUp => MenuAction::Up,
        Key::ArrowDown => MenuAction::Down,
        Key::ArrowLeft => MenuAction::Left,
        Key::ArrowRight => MenuAction::Right,
        Key::Enter => MenuAction::Confirm,
        Key::Escape | Key::Backspace => MenuAction::Back,
        _ => return None,
    })
}

/// Collects input from all devices.
#[derive(Default)]
pub struct InputHub {}

impl InputHub {
    pub fn new() -> Self {
        Self {}
    }

    /// Reads this frame's events. Call once at the start of every frame.
    pub fn poll(&mut self, ctx: &egui::Context, settings: &Settings) -> FrameInput {
        let now = Instant::now();
        let typing = ctx.egui_wants_keyboard_input();
        let mut frame = FrameInput {
            now,
            columns: Vec::new(),
            menu: Vec::new(),
            last_key: None,
            typing,
        };

        let events = ctx.input(|i| i.events.clone());
        for event in events {
            let egui::Event::Key {
                key,
                pressed,
                repeat,
                ..
            } = event
            else {
                continue;
            };
            if repeat {
                continue;
            }
            if pressed {
                frame.last_key = Some(key);
                if !(typing && key == egui::Key::Backspace)
                    && let Some(action) = menu_for_key(key)
                {
                    frame.menu.push(action);
                }
            }
            if let Some(column) = column_for_key(settings, key) {
                frame.columns.push(ColumnEvent {
                    column,
                    pressed,
                    at: now,
                    device: Device::Keyboard,
                });
            }
        }
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bindings() {
        let s = Settings::default();
        assert_eq!(column_for_key(&s, egui::Key::D), Some(0));
        assert_eq!(column_for_key(&s, egui::Key::ArrowRight), Some(3));
        assert_eq!(column_for_key(&s, egui::Key::Q), None);
    }
}

//! Hold-to-repeat for menu navigation, the same on keyboard and dance mat.
//!
//! Holding a direction moves once right away, then after a short delay keeps
//! moving, a bit faster the longer it is held (like arcade menus). We time the
//! repeats ourselves instead of using the OS key repeat, so keyboard and mats
//! feel identical on every system.

use std::time::{Duration, Instant};

use super::MenuAction;

/// Wait before the first repeat.
pub const REPEAT_DELAY: Duration = Duration::from_millis(350);
/// Time between repeats at first.
pub const REPEAT_RATE: Duration = Duration::from_millis(85);
/// Time between repeats after holding for a while.
pub const REPEAT_RATE_FAST: Duration = Duration::from_millis(45);
/// Repeats before switching to the fast rate.
const FAST_AFTER: u32 = 8;

/// What is being held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatSource {
    Key(eframe::egui::Key),
    /// Dance mat / gamepad panel (column index).
    Column(u8),
}

#[derive(Debug, Clone, Copy)]
struct Held {
    source: RepeatSource,
    action: MenuAction,
    next: Instant,
    repeats: u32,
}

/// Tracks the direction currently held. Only one at a time: pressing another
/// direction takes over, like most game menus.
#[derive(Debug, Default)]
pub struct MenuRepeat {
    held: Option<Held>,
}

/// Only directions repeat; Confirm/Back must be pressed again on purpose.
fn repeats(action: MenuAction) -> bool {
    matches!(
        action,
        MenuAction::Up | MenuAction::Down | MenuAction::Left | MenuAction::Right
    )
}

impl MenuRepeat {
    /// A press happened (the caller already emitted the first action).
    pub fn press(&mut self, source: RepeatSource, action: MenuAction, now: Instant) {
        if repeats(action) {
            self.held = Some(Held {
                source,
                action,
                next: now + REPEAT_DELAY,
                repeats: 0,
            });
        }
    }

    /// The held key/panel was released.
    pub fn release(&mut self, source: RepeatSource) {
        if self.held.is_some_and(|h| h.source == source) {
            self.held = None;
        }
    }

    /// Forget everything (device unplugged, window lost focus).
    pub fn clear(&mut self) {
        self.held = None;
    }

    /// The source currently held, if any.
    pub fn source(&self) -> Option<RepeatSource> {
        self.held.map(|h| h.source)
    }

    /// Repeated actions due at `now` (usually zero or one per frame).
    pub fn tick(&mut self, now: Instant) -> Vec<MenuAction> {
        let mut out = Vec::new();
        if let Some(h) = &mut self.held {
            // A long frame (e.g. a stall) must not fire a burst of moves.
            if now >= h.next {
                out.push(h.action);
                h.repeats += 1;
                let rate = if h.repeats >= FAST_AFTER {
                    REPEAT_RATE_FAST
                } else {
                    REPEAT_RATE
                };
                h.next = (h.next + rate).max(now);
            }
        }
        out
    }

    /// When the next repeat is due (to schedule a repaint).
    pub fn next_due(&self) -> Option<Instant> {
        self.held.map(|h| h.next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simulates holding for `ms` milliseconds at 1 ms resolution.
    fn hold(r: &mut MenuRepeat, start: Instant, ms: u64) -> usize {
        (1..=ms)
            .map(|t| r.tick(start + Duration::from_millis(t)).len())
            .sum()
    }

    #[test]
    fn waits_then_repeats_then_speeds_up() {
        let start = Instant::now();
        let mut r = MenuRepeat::default();
        r.press(
            RepeatSource::Key(eframe::egui::Key::ArrowDown),
            MenuAction::Down,
            start,
        );
        assert_eq!(hold(&mut r, start, 349), 0, "nothing before the delay");
        let mut r2 = MenuRepeat::default();
        r2.press(RepeatSource::Column(1), MenuAction::Down, start);
        // 350 ms delay, then 8 × 85 ms, then 45 ms steps.
        let in_two_seconds = hold(&mut r2, start, 2000);
        assert!((25..=40).contains(&in_two_seconds), "{in_two_seconds}");
    }

    #[test]
    fn release_stops_and_confirm_never_repeats() {
        let start = Instant::now();
        let mut r = MenuRepeat::default();
        let src = RepeatSource::Column(2);
        r.press(src, MenuAction::Up, start);
        r.release(RepeatSource::Column(0)); // another panel: ignored
        assert!(r.next_due().is_some());
        r.release(src);
        assert_eq!(hold(&mut r, start, 1000), 0);
        r.press(
            RepeatSource::Key(eframe::egui::Key::Enter),
            MenuAction::Confirm,
            start,
        );
        assert_eq!(hold(&mut r, start, 1000), 0);
    }

    #[test]
    fn a_stall_fires_once_not_a_burst() {
        let start = Instant::now();
        let mut r = MenuRepeat::default();
        r.press(RepeatSource::Column(3), MenuAction::Right, start);
        assert_eq!(r.tick(start + Duration::from_secs(3)).len(), 1);
    }
}

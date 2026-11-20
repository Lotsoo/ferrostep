//! Gamepads and dance mats via `gilrs`: hot-plug, profiles and debounce.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};

use crossbeam_channel::Receiver;
use eframe::egui;
use gilrs::{Button, EventType, GamepadId, Gilrs};

use super::profiles::{DeviceProfile, Mapped, ProfileStore, RawInput, device_key};

/// Axis values beyond this count as "pressed".
const AXIS_THRESHOLD: f32 = 0.5;

/// Suppresses chatter from cheap soft mats.
///
/// The first change is reported immediately (so timing is not delayed);
/// changes within `window` after a reported change are ignored, and the
/// final state is reported once the window has passed.
#[derive(Debug, Clone, Copy, Default)]
pub struct Debouncer {
    reported: bool,
    raw: bool,
    last_report: Option<Instant>,
}

impl Debouncer {
    fn ready(&self, at: Instant, window: Duration) -> bool {
        self.last_report
            .is_none_or(|last| at.saturating_duration_since(last) >= window)
    }

    /// Feeds a raw state change. Returns `Some(new_state)` when it should be reported.
    pub fn feed(&mut self, pressed: bool, at: Instant, window: Duration) -> Option<bool> {
        self.raw = pressed;
        if pressed != self.reported && self.ready(at, window) {
            self.reported = pressed;
            self.last_report = Some(at);
            return Some(pressed);
        }
        None
    }

    /// Reports a change that was held back by the window, once it has passed.
    pub fn tick(&mut self, now: Instant, window: Duration) -> Option<bool> {
        if self.raw != self.reported && self.ready(now, window) {
            self.reported = self.raw;
            self.last_report = Some(now);
            return Some(self.raw);
        }
        None
    }
}

/// Something that happened on a gamepad.
#[derive(Debug, Clone, PartialEq)]
pub enum PadEvent {
    Connected {
        name: String,
        key: String,
        has_profile: bool,
    },
    Disconnected {
        name: String,
    },
    Column {
        column: u8,
        pressed: bool,
        at: Instant,
    },
    Start,
    Back,
    /// A raw input was pressed (used by the mapping wizard).
    Raw {
        key: String,
        name: String,
        raw: RawInput,
    },
}

struct Device {
    key: String,
    name: String,
}

/// A gilrs event copied out of the input thread.
enum ThreadEvent {
    Connected {
        id: GamepadId,
        name: String,
        vendor: Option<u16>,
        product: Option<u16>,
    },
    Disconnected {
        id: GamepadId,
    },
    Button {
        id: GamepadId,
        button: Button,
        code: u32,
        pressed: bool,
        time: SystemTime,
    },
    Axis {
        id: GamepadId,
        code: u32,
        value: f32,
        time: SystemTime,
    },
}

/// Runs gilrs on its own thread so mat presses wake the UI even when it is idle.
fn spawn_input_thread(ctx: egui::Context) -> Option<Receiver<ThreadEvent>> {
    let (tx, rx) = crossbeam_channel::unbounded();
    let spawned = std::thread::Builder::new()
        .name("gamepad-input".into())
        .spawn(move || {
            let mut gilrs = match Gilrs::new() {
                Ok(g) => g,
                Err(gilrs::Error::NotImplemented(g)) => {
                    log::warn!("gamepads are not supported on this platform");
                    g
                }
                Err(e) => {
                    log::warn!("gamepad support unavailable: {e}");
                    return;
                }
            };
            let info = |gilrs: &Gilrs, id: GamepadId| {
                let pad = gilrs.gamepad(id);
                ThreadEvent::Connected {
                    id,
                    name: pad.name().to_string(),
                    vendor: pad.vendor_id(),
                    product: pad.product_id(),
                }
            };
            for (id, _) in gilrs.gamepads() {
                if tx.send(info(&gilrs, id)).is_err() {
                    return;
                }
            }
            ctx.request_repaint();
            loop {
                let Some(ev) = gilrs.next_event_blocking(Some(Duration::from_millis(250))) else {
                    continue;
                };
                let msg = match ev.event {
                    EventType::Connected => info(&gilrs, ev.id),
                    EventType::Disconnected => ThreadEvent::Disconnected { id: ev.id },
                    EventType::ButtonPressed(button, code) => ThreadEvent::Button {
                        id: ev.id,
                        button,
                        code: code.into_u32(),
                        pressed: true,
                        time: ev.time,
                    },
                    EventType::ButtonReleased(button, code) => ThreadEvent::Button {
                        id: ev.id,
                        button,
                        code: code.into_u32(),
                        pressed: false,
                        time: ev.time,
                    },
                    EventType::AxisChanged(_, value, code) => ThreadEvent::Axis {
                        id: ev.id,
                        code: code.into_u32(),
                        value,
                        time: ev.time,
                    },
                    _ => continue,
                };
                if tx.send(msg).is_err() {
                    return; // app closed
                }
                ctx.request_repaint();
            }
        });
    match spawned {
        Ok(_) => Some(rx),
        Err(e) => {
            log::warn!("could not start gamepad thread: {e}");
            None
        }
    }
}

/// Receives gamepad events from the input thread and maps them with profiles.
pub struct Gamepads {
    rx: Option<Receiver<ThreadEvent>>,
    ctx: Option<egui::Context>,
    pub store: ProfileStore,
    devices: HashMap<GamepadId, Device>,
    debounce: HashMap<(GamepadId, u8), Debouncer>,
    /// Last direction of each axis (−1, 0, +1) to detect crossings.
    axes: HashMap<(GamepadId, u32), i8>,
}

/// Profile used when a device has none yet: standard D-pad + Start/Select.
fn default_lookup(button: Button) -> Option<Mapped> {
    Some(match button {
        Button::DPadLeft => Mapped::Column(0),
        Button::DPadDown => Mapped::Column(1),
        Button::DPadUp => Mapped::Column(2),
        Button::DPadRight => Mapped::Column(3),
        Button::Start => Mapped::Start,
        Button::Select => Mapped::Back,
        _ => return None,
    })
}

/// Converts a gilrs timestamp to an `Instant` (capped to 100 ms in the past).
fn event_instant(time: SystemTime, now: Instant) -> Instant {
    let age = SystemTime::now()
        .duration_since(time)
        .unwrap_or_default()
        .min(Duration::from_millis(100));
    now.checked_sub(age).unwrap_or(now)
}

impl Gamepads {
    /// Starts the gamepad thread. `ctx` is used to wake the UI on input.
    pub fn new(ctx: &egui::Context) -> Self {
        Self {
            rx: spawn_input_thread(ctx.clone()),
            ctx: Some(ctx.clone()),
            store: ProfileStore::load(),
            devices: HashMap::new(),
            debounce: HashMap::new(),
            axes: HashMap::new(),
        }
    }

    /// Name of the first connected device, if any.
    pub fn first_name(&self) -> Option<String> {
        self.devices.values().next().map(|d| d.name.clone())
    }

    /// Saves a profile made by the wizard.
    pub fn save_profile(&mut self, key: String, profile: DeviceProfile) {
        self.store.devices.insert(key, profile);
        if let Err(e) = self.store.save() {
            log::warn!("could not save device profile: {e:#}");
        }
    }

    /// Key and name of connected devices.
    pub fn connected(&self) -> Vec<(String, String)> {
        self.devices
            .values()
            .map(|d| (d.key.clone(), d.name.clone()))
            .collect()
    }

    /// Reads all pending gamepad events.
    pub fn poll(&mut self, now: Instant, debounce_ms: f64) -> Vec<PadEvent> {
        let window = Duration::from_secs_f64(debounce_ms.clamp(0.0, 100.0) / 1000.0);
        let mut out = Vec::new();
        let events: Vec<ThreadEvent> = self
            .rx
            .as_ref()
            .map(|rx| rx.try_iter().collect())
            .unwrap_or_default();

        for ev in events {
            match ev {
                ThreadEvent::Connected {
                    id,
                    name,
                    vendor,
                    product,
                } => {
                    let key = device_key(vendor, product, &name);
                    let has_profile = self.store.devices.contains_key(&key);
                    log::info!("gamepad connected: {name} ({key}), profile: {has_profile}");
                    self.devices.insert(
                        id,
                        Device {
                            key: key.clone(),
                            name: name.clone(),
                        },
                    );
                    out.push(PadEvent::Connected {
                        name,
                        key,
                        has_profile,
                    });
                }
                ThreadEvent::Disconnected { id } => {
                    if let Some(d) = self.devices.remove(&id) {
                        log::info!("gamepad disconnected: {}", d.name);
                        out.push(PadEvent::Disconnected { name: d.name });
                    }
                    self.debounce.retain(|(pad, _), _| *pad != id);
                }
                ThreadEvent::Button {
                    id,
                    button,
                    code,
                    pressed,
                    time,
                } => {
                    let at = event_instant(time, now);
                    self.raw(
                        id,
                        Some(button),
                        RawInput::Button(code),
                        pressed,
                        at,
                        window,
                        &mut out,
                    );
                }
                ThreadEvent::Axis {
                    id,
                    code,
                    value,
                    time,
                } => {
                    let at = event_instant(time, now);
                    self.axis(id, code, value, at, window, &mut out);
                }
            }
        }

        // Release changes that were held back by the debounce window.
        let mut pending = false;
        for (&(_, column), d) in &mut self.debounce {
            if let Some(pressed) = d.tick(now, window) {
                out.push(PadEvent::Column {
                    column,
                    pressed,
                    at: now,
                });
            }
            pending |= d.raw != d.reported;
        }
        if pending && let Some(ctx) = &self.ctx {
            ctx.request_repaint_after(window);
        }
        out
    }

    fn axis(
        &mut self,
        id: GamepadId,
        code: u32,
        value: f32,
        at: Instant,
        window: Duration,
        out: &mut Vec<PadEvent>,
    ) {
        let dir: i8 = if value > AXIS_THRESHOLD {
            1
        } else if value < -AXIS_THRESHOLD {
            -1
        } else {
            0
        };
        let prev = self.axes.insert((id, code), dir).unwrap_or(0);
        if prev == dir {
            return;
        }
        if prev != 0 {
            let raw = if prev > 0 {
                RawInput::AxisPos(code)
            } else {
                RawInput::AxisNeg(code)
            };
            self.raw(id, None, raw, false, at, window, out);
        }
        if dir != 0 {
            let raw = if dir > 0 {
                RawInput::AxisPos(code)
            } else {
                RawInput::AxisNeg(code)
            };
            self.raw(id, None, raw, true, at, window, out);
        }
    }

    #[allow(clippy::too_many_arguments)] // internal helper fed from several event kinds
    fn raw(
        &mut self,
        id: GamepadId,
        button: Option<Button>,
        raw: RawInput,
        pressed: bool,
        at: Instant,
        window: Duration,
        out: &mut Vec<PadEvent>,
    ) {
        let Some(device) = self.devices.get(&id) else {
            return;
        };
        if pressed {
            out.push(PadEvent::Raw {
                key: device.key.clone(),
                name: device.name.clone(),
                raw,
            });
        }
        let mapped = match self.store.devices.get(&device.key) {
            Some(profile) => profile.lookup(raw),
            None => button.and_then(default_lookup),
        };
        match mapped {
            Some(Mapped::Column(column)) => {
                let d = self.debounce.entry((id, column)).or_default();
                if let Some(state) = d.feed(pressed, at, window) {
                    out.push(PadEvent::Column {
                        column,
                        pressed: state,
                        at,
                    });
                }
            }
            Some(Mapped::Start) if pressed => out.push(PadEvent::Start),
            Some(Mapped::Back) if pressed => out.push(PadEvent::Back),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: Duration = Duration::from_millis(20);

    #[test]
    fn first_press_is_immediate() {
        let t0 = Instant::now();
        let mut d = Debouncer::default();
        assert_eq!(d.feed(true, t0, W), Some(true));
    }

    #[test]
    fn chatter_is_ignored() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut d = Debouncer::default();
        assert_eq!(d.feed(true, t0, W), Some(true));
        assert_eq!(d.feed(false, ms(3), W), None);
        assert_eq!(d.feed(true, ms(6), W), None);
        assert_eq!(d.tick(ms(30), W), None, "still pressed: nothing to report");
        assert_eq!(d.feed(false, ms(120), W), Some(false));
    }

    #[test]
    fn short_tap_release_is_reported_after_window() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut d = Debouncer::default();
        d.feed(true, t0, W);
        assert_eq!(d.feed(false, ms(8), W), None);
        assert_eq!(d.tick(ms(10), W), None);
        assert_eq!(d.tick(ms(21), W), Some(false));
    }

    #[test]
    fn default_profile_uses_dpad() {
        assert_eq!(default_lookup(Button::DPadUp), Some(Mapped::Column(2)));
        assert_eq!(default_lookup(Button::South), None);
    }
}

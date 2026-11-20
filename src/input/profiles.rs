//! Per-device button mappings ("profiles"), saved in the config folder.
//!
//! Mappings use *raw* codes so any mat works, whether its panels report as
//! buttons, a D-pad, a hat switch or analog axes.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::config_dir;

/// A physical control on a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RawInput {
    Button(u32),
    /// Axis pushed past +0.5.
    AxisPos(u32),
    /// Axis pushed past −0.5.
    AxisNeg(u32),
}

/// What a raw input means in the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mapped {
    Column(u8),
    Start,
    Back,
}

/// Mapping for one device.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DeviceProfile {
    /// Human-readable device name.
    pub name: String,
    /// Inputs for Left, Down, Up, Right.
    pub columns: [Vec<RawInput>; 4],
    pub start: Vec<RawInput>,
    pub back: Vec<RawInput>,
}

impl DeviceProfile {
    /// What `raw` is mapped to, if anything.
    pub fn lookup(&self, raw: RawInput) -> Option<Mapped> {
        if let Some(c) = self.columns.iter().position(|list| list.contains(&raw)) {
            return Some(Mapped::Column(c as u8));
        }
        if self.start.contains(&raw) {
            return Some(Mapped::Start);
        }
        if self.back.contains(&raw) {
            return Some(Mapped::Back);
        }
        None
    }

    /// True when all four panels have at least one input.
    pub fn is_complete(&self) -> bool {
        self.columns.iter().all(|c| !c.is_empty())
    }
}

/// Stable key for a device: vendor/product IDs plus name.
pub fn device_key(vendor: Option<u16>, product: Option<u16>, name: &str) -> String {
    format!(
        "{:04x}:{:04x}:{name}",
        vendor.unwrap_or(0),
        product.unwrap_or(0)
    )
}

/// All saved profiles.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProfileStore {
    pub devices: HashMap<String, DeviceProfile>,
}

impl ProfileStore {
    fn path() -> Result<PathBuf> {
        Ok(config_dir()?.join("devices.json"))
    }

    pub fn load() -> Self {
        let result = Self::path().and_then(|p| {
            if !p.exists() {
                return Ok(Self::default());
            }
            Ok(serde_json::from_str(&std::fs::read_to_string(p)?)?)
        });
        result.unwrap_or_else(|e| {
            log::warn!("could not load device profiles: {e:#}");
            Self::default()
        })
    }

    pub fn save(&self) -> Result<()> {
        std::fs::write(Self::path()?, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_buttons_hats_and_axes() {
        let p = DeviceProfile {
            name: "mat".into(),
            columns: [
                vec![RawInput::AxisNeg(16)], // hat X left
                vec![RawInput::Button(289)],
                vec![RawInput::Button(290)],
                vec![RawInput::AxisPos(16)],
            ],
            start: vec![RawInput::Button(297)],
            back: vec![],
        };
        assert_eq!(p.lookup(RawInput::AxisNeg(16)), Some(Mapped::Column(0)));
        assert_eq!(p.lookup(RawInput::AxisPos(16)), Some(Mapped::Column(3)));
        assert_eq!(p.lookup(RawInput::Button(290)), Some(Mapped::Column(2)));
        assert_eq!(p.lookup(RawInput::Button(297)), Some(Mapped::Start));
        assert_eq!(p.lookup(RawInput::Button(1)), None);
        assert!(p.is_complete());
    }

    #[test]
    fn profiles_survive_json() {
        let mut store = ProfileStore::default();
        store.devices.insert(
            device_key(Some(0x0b43), Some(0x0003), "Mat"),
            DeviceProfile::default(),
        );
        let text = serde_json::to_string(&store).unwrap();
        let back: ProfileStore = serde_json::from_str(&text).unwrap();
        assert!(back.devices.contains_key("0b43:0003:Mat"));
    }
}

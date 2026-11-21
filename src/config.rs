//! User settings, saved as TOML in the OS config folder.

use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

/// How notes scroll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScrollMode {
    /// Constant speed in "BPM-equivalent" (ignores song BPM changes).
    CMod,
    /// Multiplier of the song's BPM.
    XMod,
}

/// Color theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    Dark,
    Light,
}

/// Graphics API preference (applied at the next start).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphicsBackend {
    Auto,
    Vulkan,
    Dx12,
    OpenGl,
}

/// All user-adjustable settings. Missing fields fall back to defaults,
/// so old settings files keep working after updates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Audio latency compensation in milliseconds (positive = audio is late).
    pub global_offset_ms: f64,
    pub scroll_mode: ScrollMode,
    pub cmod: f32,
    pub xmod: f32,
    /// Notes move downwards to receptors at the bottom.
    pub reverse: bool,
    /// Master volume 0.0..=1.0.
    pub volume: f32,
    /// Menu sound effects volume 0.0..=1.0 (0 = off).
    pub sfx_volume: f32,
    /// Multiplier for the judgment windows (1.0 = normal, < 1 is stricter).
    pub judge_scale: f64,
    pub no_fail: bool,
    /// Keyboard keys per column (egui key names), Left/Down/Up/Right.
    pub keybinds: [Vec<String>; 4],
    /// Custom songs folder. `None` = automatic.
    pub songs_dir: Option<PathBuf>,
    /// Custom ffmpeg binary. `None` = search automatically.
    pub ffmpeg_path: Option<PathBuf>,
    pub ui_scale: f32,
    pub reduce_motion: bool,
    pub show_fps: bool,
    pub vsync: bool,
    pub theme: ThemeChoice,
    /// Gameplay background brightness 0.0..=1.0.
    pub background_brightness: f32,
    /// Ignore repeated triggers from a dance mat panel within this window.
    pub gamepad_debounce_ms: f64,
    pub graphics_backend: GraphicsBackend,
    /// First-run onboarding finished or skipped.
    pub onboarding_done: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let keys = |a: &str, b: &str| vec![a.to_string(), b.to_string()];
        Self {
            global_offset_ms: 0.0,
            scroll_mode: ScrollMode::CMod,
            cmod: 500.0,
            xmod: 2.0,
            reverse: false,
            volume: 0.8,
            sfx_volume: 1.0,
            judge_scale: 1.0,
            no_fail: true,
            keybinds: [
                keys("D", "ArrowLeft"),
                keys("F", "ArrowDown"),
                keys("J", "ArrowUp"),
                keys("K", "ArrowRight"),
            ],
            songs_dir: None,
            ffmpeg_path: None,
            ui_scale: 1.0,
            reduce_motion: false,
            show_fps: false,
            vsync: true,
            theme: ThemeChoice::Dark,
            background_brightness: 0.4,
            gamepad_debounce_ms: 18.0,
            graphics_backend: GraphicsBackend::Auto,
            onboarding_done: false,
        }
    }
}

/// OS-specific folders for Ferrostep.
pub fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("", "", "ferrostep")
}

/// Folder for settings, scores and device profiles (created if needed).
pub fn config_dir() -> Result<PathBuf> {
    let dirs = project_dirs().context("could not find the user's config folder")?;
    let dir = dirs.config_dir().to_path_buf();
    std::fs::create_dir_all(&dir).with_context(|| format!("could not create {}", dir.display()))?;
    Ok(dir)
}

impl Settings {
    fn path() -> Result<PathBuf> {
        Ok(config_dir()?.join("settings.toml"))
    }

    /// Loads settings, falling back to defaults (with a log message) on any problem.
    pub fn load() -> Self {
        let result = Self::path().and_then(|path| {
            if !path.exists() {
                return Ok(Self::default());
            }
            let text = std::fs::read_to_string(&path)?;
            Ok(toml::from_str(&text)?)
        });
        result.unwrap_or_else(|e| {
            log::warn!("using default settings: {e:#}");
            Self::default()
        })
    }

    /// Saves settings to disk.
    pub fn save(&self) -> Result<()> {
        let text = toml::to_string_pretty(self)?;
        let path = Self::path()?;
        std::fs::write(&path, text).with_context(|| format!("could not write {}", path.display()))
    }

    /// Global offset in seconds.
    pub fn global_offset(&self) -> f64 {
        self.global_offset_ms / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_toml() {
        let s = Settings {
            global_offset_ms: 12.5,
            songs_dir: Some(PathBuf::from("/tmp/songs")),
            ..Default::default()
        };
        let text = toml::to_string_pretty(&s).unwrap();
        let back: Settings = toml::from_str(&text).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let s: Settings = toml::from_str("volume = 0.5").unwrap();
        assert_eq!(s.volume, 0.5);
        assert_eq!(s.cmod, 500.0);
    }
}

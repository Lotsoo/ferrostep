//! Song library: where songs live, scanning and hot-reload.

pub mod scores;
pub mod test_song;

use std::path::{Path, PathBuf};

use crate::config::{Settings, project_dirs};

/// Audio extensions the game can play.
pub const AUDIO_EXTENSIONS: [&str; 5] = ["ogg", "mp3", "wav", "flac", "oga"];

/// Finds the song's audio: the `#MUSIC` file if it exists (case-insensitive),
/// otherwise the first playable audio file in the folder.
pub fn find_music(song_dir: &Path, music: &str) -> Option<PathBuf> {
    let music = music.trim();
    if !music.is_empty() {
        let direct = song_dir.join(music);
        if direct.is_file() {
            return Some(direct);
        }
    }
    let entries: Vec<PathBuf> = std::fs::read_dir(song_dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    if !music.is_empty()
        && let Some(p) = entries.iter().find(|p| {
            p.file_name()
                .is_some_and(|f| f.to_string_lossy().eq_ignore_ascii_case(music))
        })
    {
        return Some(p.clone());
    }
    entries.into_iter().find(|p| {
        p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
    })
}

/// Picks the `Songs/` folder:
/// 1. the folder set in Settings,
/// 2. the project folder when started with `cargo run` (development),
/// 3. next to the executable if writable (portable zip),
/// 4. the user's data folder (installed package).
pub fn songs_dir(settings: &Settings) -> PathBuf {
    if let Some(dir) = &settings.songs_dir {
        return dir.clone();
    }
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        return PathBuf::from(manifest).join("Songs");
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        let candidate = dir.join("Songs");
        if is_writable_dir(&candidate) || (!candidate.exists() && is_writable_dir(&dir)) {
            return candidate;
        }
    }
    project_dirs()
        .map(|d| d.data_dir().join("Songs"))
        .unwrap_or_else(|| PathBuf::from("Songs"))
}

fn is_writable_dir(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    let probe = dir.join(".ferrostep_write_test");
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

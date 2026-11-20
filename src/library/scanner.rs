//! Finds songs in `Songs/<Group>/<Song>/` (and `Songs/<Song>/`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::parser::{Simfile, load_simfile};
use crate::timing::{TimingMap, time_notes};

use super::find_music;

/// Credit written by the importer; used for the "Auto" badge.
pub const AUTO_CREDIT: &str = "Ferrostep Auto-Chart";

/// One song in the library.
#[derive(Debug, Clone)]
pub struct SongEntry {
    pub dir: PathBuf,
    /// Group folder name ("" when the song sits directly in `Songs/`).
    pub group: String,
    pub simfile_path: PathBuf,
    pub simfile: Arc<Simfile>,
    pub music: Option<PathBuf>,
    pub banner: Option<PathBuf>,
    /// Made by the importer.
    pub is_auto: bool,
    /// For example "128" or "90–180".
    pub bpm_label: String,
    /// Time of the last note (seconds), a good estimate of the song length.
    pub length: f64,
    /// Indices of playable (dance-single) charts, easiest first.
    pub playable: Vec<usize>,
}

impl SongEntry {
    /// Title for display (falls back to the folder name).
    pub fn title(&self) -> String {
        let t = self.simfile.meta.title.trim();
        if t.is_empty() {
            self.dir
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            t.to_string()
        }
    }

    /// Lowercase text used for searching.
    pub fn search_text(&self) -> String {
        let m = &self.simfile.meta;
        format!("{} {} {} {}", m.title, m.subtitle, m.artist, self.group).to_lowercase()
    }
}

/// Picks the simfile in a folder: `.ssc` is preferred over `.sm`.
pub fn find_simfile(dir: &Path) -> Option<PathBuf> {
    let mut sm = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("ssc") => return Some(path),
            Some("sm") => sm = Some(path),
            _ => {}
        }
    }
    sm
}

/// Loads one song folder. Returns `None` (with a log line) when it is unusable.
pub fn load_song(dir: &Path, group: &str) -> Option<SongEntry> {
    let simfile_path = find_simfile(dir)?;
    let simfile = match load_simfile(&simfile_path) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("skipping {}: {e}", simfile_path.display());
            return None;
        }
    };
    let mut playable: Vec<usize> = simfile
        .charts
        .iter()
        .enumerate()
        .filter(|(_, c)| c.is_dance_single() && !c.notes.is_empty())
        .map(|(i, _)| i)
        .collect();
    if playable.is_empty() {
        log::info!("{}: no dance-single charts", dir.display());
        return None;
    }
    playable.sort_by_key(|&i| (simfile.charts[i].difficulty, simfile.charts[i].meter));

    let bpms: Vec<f64> = simfile
        .timing
        .bpms
        .iter()
        .map(|b| b.1)
        .filter(|b| *b > 0.0)
        .collect();
    let min = bpms.iter().copied().fold(f64::INFINITY, f64::min);
    let max = bpms.iter().copied().fold(0.0, f64::max);
    let bpm_label = if bpms.is_empty() {
        "?".into()
    } else if (max - min).abs() < 0.5 {
        format!("{}", max.round())
    } else {
        format!("{}–{}", min.round(), max.round())
    };

    let length = playable
        .iter()
        .map(|&i| {
            let chart = &simfile.charts[i];
            let map = TimingMap::new(simfile.timing_for(chart));
            time_notes(chart, &map).last().map_or(0.0, |n| n.end_time)
        })
        .fold(0.0, f64::max);

    let banner = [&simfile.meta.banner, &simfile.meta.background]
        .into_iter()
        .map(|f| dir.join(f.trim()))
        .find(|p| p.is_file());

    Some(SongEntry {
        music: find_music(dir, &simfile.meta.music),
        is_auto: simfile.meta.credit.contains(AUTO_CREDIT),
        dir: dir.to_path_buf(),
        group: group.to_string(),
        simfile_path,
        simfile: Arc::new(simfile),
        banner,
        bpm_label,
        length,
        playable,
    })
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        // Hidden folders (e.g. the importer's work folder) are not songs.
        .filter(|p| {
            !p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .collect();
    dirs.sort();
    dirs
}

/// Scans the whole songs folder (two levels deep).
pub fn scan(root: &Path) -> Vec<SongEntry> {
    let mut songs = Vec::new();
    for first in subdirs(root) {
        let name = first
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        if find_simfile(&first).is_some() {
            songs.extend(load_song(&first, ""));
            continue;
        }
        for song in subdirs(&first) {
            songs.extend(load_song(&song, &name));
        }
    }
    songs.sort_by_key(|s| (s.group.to_lowercase(), s.title().to_lowercase()));
    songs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_groups_and_prefers_ssc() {
        let root = std::env::temp_dir().join(format!("ferrostep-scan-{}", std::process::id()));
        let song = root.join("Group").join("Song");
        std::fs::create_dir_all(&song).unwrap();
        let chart = "#TITLE:{T};#BPMS:0=120;#NOTES:dance-single::Easy:1::1000\n0000\n0000\n0000\n;";
        std::fs::write(song.join("a.sm"), chart.replace("{T}", "From SM")).unwrap();
        std::fs::write(
            song.join("a.ssc"),
            "#TITLE:From SSC;#BPMS:0=120;#NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Easy;#NOTES:1000\n0000\n0000\n0000\n;",
        )
        .unwrap();
        let songs = scan(&root);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(songs.len(), 1);
        assert_eq!(songs[0].group, "Group");
        assert_eq!(songs[0].title(), "From SSC");
        assert_eq!(songs[0].bpm_label, "120");
    }
}

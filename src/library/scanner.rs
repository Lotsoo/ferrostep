//! Finds songs in `Songs/<Group>/<Song>/` (and `Songs/<Song>/`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::parser::{NoteKind, Simfile, load_simfile};
use crate::timing::{TimingMap, time_notes};

use super::find_music;

/// Credit written by the importer; used for the "Auto" badge.
pub const AUTO_CREDIT: &str = "Ferrostep Auto-Chart";
/// Hidden file in an imported song folder that remembers which file it came
/// from (content hash), so the same file is never imported or listed twice.
pub const SOURCE_FILE: &str = ".ferrostep-source";

/// Content hash of the file an imported song was made from.
pub fn read_source_hash(dir: &Path) -> Option<u64> {
    let text = std::fs::read_to_string(dir.join(SOURCE_FILE)).ok()?;
    text.lines()
        .find_map(|l| l.strip_prefix("hash="))
        .and_then(|h| u64::from_str_radix(h.trim(), 16).ok())
}

/// Writes the source marker (see [`SOURCE_FILE`]).
pub fn write_source_hash(dir: &Path, hash: u64, size: u64) -> std::io::Result<()> {
    std::fs::write(
        dir.join(SOURCE_FILE),
        format!("hash={hash:016x}\nsize={size}\n"),
    )
}

/// Identifies an imported song by what was generated from its audio: timing
/// plus every note of every chart. Title and artist are left out on purpose:
/// for untagged files they come from the file name, which may differ.
pub fn fingerprint(simfile: &Simfile) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for chart in &simfile.charts {
        chart.steps_type.hash(&mut h);
        format!("{:?}", chart.difficulty).hash(&mut h);
        for n in &chart.notes {
            // Rounded like the .sm writer, so in-memory and on-disk versions agree.
            let kind = match n.kind {
                NoteKind::HoldHead { end_beat } => format!("hold{end_beat:.3}"),
                NoteKind::RollHead { end_beat } => format!("roll{end_beat:.3}"),
                other => format!("{other:?}"),
            };
            format!("{:.3}:{}:{kind}", n.beat, n.column).hash(&mut h);
        }
    }
    let bpms: Vec<String> = simfile
        .timing
        .bpms
        .iter()
        .map(|(b, v)| format!("{b:.3}={v:.3}"))
        .collect();
    format!("{bpms:?}|{:.3}|{:016x}", simfile.timing.offset, h.finish())
}

/// Hides repeated imports of the same file; the first folder (by name) wins.
/// Folders are only hidden, never deleted.
fn hide_duplicate_imports(songs: &mut Vec<SongEntry>) {
    let mut hashes = std::collections::HashSet::new();
    let mut prints = std::collections::HashSet::new();
    songs.retain(|song| {
        if !song.is_auto {
            return true;
        }
        let hash = read_source_hash(&song.dir);
        let print = fingerprint(&song.simfile);
        let duplicate = prints.contains(&print) || hash.is_some_and(|h| hashes.contains(&h));
        // Remember both identities even for hidden copies, so a copy that
        // matches a hidden one (by the other identity) is hidden too.
        prints.insert(print);
        hashes.extend(hash);
        if duplicate {
            log::info!("hiding duplicate import {}", song.dir.display());
            return false;
        }
        true
    });
}

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

/// Song folders inside `dir`, sorted, without hidden ones.
pub fn subdirs(dir: &Path) -> Vec<PathBuf> {
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
    hide_duplicate_imports(&mut songs);
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

    #[test]
    fn repeated_imports_are_listed_once() {
        let root = std::env::temp_dir().join(format!("ferrostep-dupes-{}", std::process::id()));
        let chart = |title: &str| {
            // Different songs get different notes, like real imports.
            let row = if title == "Different" { "0100" } else { "1000" };
            format!(
                "#TITLE:{title};#CREDIT:{AUTO_CREDIT};#BPMS:0=120;\
                 #NOTES:dance-single::Easy:1::{row}\n0000\n0000\n0000\n;"
            )
        };
        let make = |name: &str, title: &str, hash: Option<u64>| {
            let dir = root.join("Imported").join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("song.sm"), chart(title)).unwrap();
            if let Some(h) = hash {
                write_source_hash(&dir, h, 1).unwrap();
            }
        };
        make("Song", "Song", Some(7)); // original
        make("Song (2)", "Song", None); // old import: same content, no marker
        make("Renamed", "Other title", Some(7)); // same file, different tags
        make("Different", "Different", Some(8));
        let songs = scan(&root);
        let _ = std::fs::remove_dir_all(&root);
        let names: Vec<String> = songs.iter().map(|s| s.title()).collect();
        // Folders are checked in name order: "Renamed" comes first and keeps the
        // file; "Song" (same file) and "Song (2)" (same as "Song") are hidden.
        assert_eq!(names, ["Different", "Other title"]);
    }
}

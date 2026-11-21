//! Song library: where songs live, scanning and hot-reload.

pub mod scanner;
pub mod scores;
pub mod test_song;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use eframe::egui;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::config::{Settings, project_dirs};
use scanner::SongEntry;

/// Rescan this long after the last file change (debounce).
const RELOAD_DEBOUNCE: Duration = Duration::from_millis(500);

enum LibraryMsg {
    Scanned(Vec<SongEntry>),
    FilesChanged,
}

/// The song list, scanned in the background and kept up to date with `notify`.
pub struct Library {
    pub root: PathBuf,
    pub songs: Vec<Arc<SongEntry>>,
    /// True while a scan is running (UI shows skeleton cards).
    pub loading: bool,
    /// Has at least one scan finished?
    pub loaded_once: bool,
    /// Song folder to highlight (just imported) and when it appeared.
    pub highlight: Option<(PathBuf, Instant)>,
    /// Increases with every finished scan so screens can refresh.
    pub generation: u64,
    tx: Sender<LibraryMsg>,
    rx: Receiver<LibraryMsg>,
    _watcher: Option<RecommendedWatcher>,
    changed_at: Option<Instant>,
    rescan_queued: bool,
    ctx: egui::Context,
}

impl Library {
    /// Creates the library, starts the first scan and the folder watcher.
    pub fn new(root: PathBuf, ctx: &egui::Context) -> Self {
        if let Err(e) = std::fs::create_dir_all(&root) {
            log::warn!("could not create songs folder {}: {e}", root.display());
        }
        let (tx, rx) = crossbeam_channel::unbounded();
        let watcher = Self::watch(&root, tx.clone(), ctx.clone());
        let mut lib = Self {
            root,
            songs: Vec::new(),
            loading: false,
            loaded_once: false,
            highlight: None,
            generation: 0,
            tx,
            rx,
            _watcher: watcher,
            changed_at: None,
            rescan_queued: false,
            ctx: ctx.clone(),
        };
        lib.rescan();
        lib
    }

    fn watch(
        root: &Path,
        tx: Sender<LibraryMsg>,
        ctx: egui::Context,
    ) -> Option<RecommendedWatcher> {
        let handler = move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res
                && !matches!(ev.kind, notify::EventKind::Access(_))
            {
                let _ = tx.send(LibraryMsg::FilesChanged);
                ctx.request_repaint_after(RELOAD_DEBOUNCE + Duration::from_millis(50));
            }
        };
        match notify::recommended_watcher(handler) {
            Ok(mut w) => match w.watch(root, RecursiveMode::Recursive) {
                Ok(()) => Some(w),
                Err(e) => {
                    log::warn!("cannot watch {}: {e}", root.display());
                    None
                }
            },
            Err(e) => {
                log::warn!("file watcher unavailable: {e}");
                None
            }
        }
    }

    /// Starts a background rescan (queued if one is already running).
    pub fn rescan(&mut self) {
        if self.loading {
            self.rescan_queued = true;
            return;
        }
        self.loading = true;
        let root = self.root.clone();
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("library-scan".into())
            .spawn(move || {
                let songs = scanner::scan(&root);
                let _ = tx.send(LibraryMsg::Scanned(songs));
                ctx.request_repaint();
            });
        if let Err(e) = spawned {
            log::error!("could not start library scan: {e}");
            self.loading = false;
        }
    }

    /// Processes scan results and file changes. Call every frame.
    pub fn poll(&mut self, now: Instant) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                LibraryMsg::Scanned(songs) => {
                    self.songs = songs.into_iter().map(Arc::new).collect();
                    self.loading = false;
                    self.loaded_once = true;
                    self.generation += 1;
                    if std::mem::take(&mut self.rescan_queued) {
                        self.rescan();
                    }
                }
                LibraryMsg::FilesChanged => self.changed_at = Some(now),
            }
        }
        if let Some(t) = self.changed_at
            && now.saturating_duration_since(t) >= RELOAD_DEBOUNCE
        {
            self.changed_at = None;
            self.rescan();
        }
    }

    /// Marks a freshly imported song so the list can highlight it.
    pub fn mark_new(&mut self, dir: PathBuf) {
        self.highlight = Some((dir, Instant::now()));
        self.rescan();
    }

    /// Permanently deletes one song folder from disk and removes it from the
    /// list right away (the watcher rescan follows).
    pub fn delete_song(&mut self, dir: &Path) -> std::io::Result<()> {
        delete_song_dir(&self.root, dir)?;
        self.songs.retain(|s| s.dir != dir);
        self.generation += 1;
        Ok(())
    }
}

/// Deletes `dir` and everything in it, but only when it is safely a single
/// song folder: strictly inside `root` (not `root` itself, not a group folder
/// holding other songs) and containing a simfile. Anything else is refused so a
/// bug can never wipe the whole library or a folder outside it.
pub fn delete_song_dir(root: &Path, dir: &Path) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    let root = root.canonicalize()?;
    let dir = dir.canonicalize()?;
    let depth = dir
        .strip_prefix(&root)
        .map_err(|_| Error::new(ErrorKind::PermissionDenied, "folder is outside Songs/"))?
        .components()
        .count();
    if !(1..=2).contains(&depth) {
        return Err(Error::new(
            ErrorKind::PermissionDenied,
            "not a song folder (must be Songs/<song> or Songs/<group>/<song>)",
        ));
    }
    let mut has_simfile = false;
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.is_dir()
            && std::fs::read_dir(&path)?
                .flatten()
                .any(|e| is_simfile(&e.path()))
        {
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                "folder contains other songs (it is a group folder)",
            ));
        }
        has_simfile |= is_simfile(&path);
    }
    if !has_simfile {
        return Err(Error::new(
            ErrorKind::PermissionDenied,
            "no .sm/.ssc file in this folder",
        ));
    }
    std::fs::remove_dir_all(&dir)?;
    // A group folder left empty goes too (remove_dir refuses non-empty ones).
    if depth == 2
        && let Some(group) = dir.parent()
    {
        let _ = std::fs::remove_dir(group);
    }
    Ok(())
}

fn is_simfile(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("sm") || e.eq_ignore_ascii_case("ssc"))
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn song(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("song.sm"), "#TITLE:x;").unwrap();
        std::fs::write(dir.join("song.ogg"), b"audio").unwrap();
    }

    #[test]
    fn deletes_only_a_single_song_folder() {
        let tmp = std::env::temp_dir().join(format!("ferrostep-del-{}", std::process::id()));
        let root = tmp.join("Songs");
        let a = root.join("Imported").join("A");
        let b = root.join("Imported").join("B");
        let loose = root.join("Loose");
        song(&a);
        song(&b);
        song(&loose);
        let outside = tmp.join("Outside");
        song(&outside);

        // Refused: the root, a group folder, a folder outside, a non-song folder.
        assert!(delete_song_dir(&root, &root).is_err());
        assert!(delete_song_dir(&root, &root.join("Imported")).is_err());
        assert!(delete_song_dir(&root, &outside).is_err());
        assert!(delete_song_dir(&root, &root.join("Imported/A/../../../Outside")).is_err());
        std::fs::create_dir_all(root.join("Empty")).unwrap();
        assert!(delete_song_dir(&root, &root.join("Empty")).is_err());

        // Allowed: one song; its neighbours stay.
        delete_song_dir(&root, &a).unwrap();
        assert!(!a.exists());
        assert!(b.join("song.sm").exists());
        assert!(root.join("Imported").exists(), "group still holds B");
        delete_song_dir(&root, &b).unwrap();
        assert!(!root.join("Imported").exists(), "empty group removed");
        assert!(root.exists());
        delete_song_dir(&root, &loose).unwrap();
        assert!(!loose.exists());
        assert!(outside.exists());
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}

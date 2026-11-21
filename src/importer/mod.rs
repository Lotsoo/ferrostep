//! MP3/MP4 → playable song folder, running on a background thread.
//!
//! Stages: read file → extract audio → analyze beats → make charts → save.
//! The UI receives [`ImportEvent`]s over a channel and can cancel via an atomic flag.

pub mod ffmpeg;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender};

use crate::audio_dsp::decode::{Cancelled, decode_file};
use crate::audio_dsp::onset::waveform_peaks;
use crate::audio_dsp::tempo::best_phase;
use crate::audio_dsp::{Analysis, analyze};
use crate::chart_gen::{SongInfo, average_nps, generate_all};
use crate::library::scanner::{
    AUTO_CREDIT, find_simfile, fingerprint, read_source_hash, subdirs, write_source_hash,
};
use crate::parser::writer::write_sm;
use crate::parser::{Chart, Metadata, Simfile, TimingData, load_simfile};

/// File types the importer accepts.
pub const IMPORT_EXTENSIONS: [&str; 8] = ["mp3", "mp4", "m4a", "ogg", "wav", "flac", "aac", "oga"];
/// Length of the song-select preview written to `#SAMPLELENGTH`.
const SAMPLE_LENGTH: f64 = 15.0;
/// Name of the simfile inside every imported folder.
const SIMFILE_NAME: &str = "song.sm";

/// Whether `path` looks like something we can import.
pub fn is_importable(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMPORT_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// The five steps shown in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    Reading,
    Extracting,
    Analyzing,
    Charting,
    Saving,
}

impl Stage {
    pub const ALL: [Stage; 5] = [
        Stage::Reading,
        Stage::Extracting,
        Stage::Analyzing,
        Stage::Charting,
        Stage::Saving,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    /// Indonesian label.
    pub fn label(self) -> &'static str {
        match self {
            Stage::Reading => "Membaca file",
            Stage::Extracting => "Mengekstrak audio",
            Stage::Analyzing => "Menganalisis ketukan",
            Stage::Charting => "Membuat chart",
            Stage::Saving => "Menyimpan",
        }
    }
}

/// Summary of a finished import.
#[derive(Debug, Clone)]
pub struct ImportResult {
    pub dir: PathBuf,
    pub simfile: Simfile,
    /// Beat analysis; `None` when an already imported song was reused (it is
    /// computed on demand if the BPM is corrected).
    pub analysis: Option<Arc<Analysis>>,
    pub seed: u64,
    /// Whether ffmpeg was used to make a compact `.ogg`.
    pub used_ffmpeg: bool,
    /// Info for the user, e.g. "install ffmpeg for smaller files".
    pub note: Option<String>,
    pub elapsed: Duration,
}

impl ImportResult {
    pub fn bpm(&self) -> f64 {
        self.simfile.timing.bpms.first().map_or(0.0, |b| b.1)
    }

    /// Average notes per second of each chart.
    pub fn chart_nps(&self) -> Vec<f64> {
        let song = self.song_info(self.bpm(), self.simfile.timing.offset);
        self.simfile
            .charts
            .iter()
            .map(|c| average_nps(c, &song))
            .collect()
    }

    fn song_info(&self, bpm: f64, offset: f64) -> SongInfo<'_> {
        SongInfo {
            onsets: self.analysis.as_ref().map_or(&[], |a| &a.onsets),
            bpm,
            offset,
            duration: self.analysis.as_ref().map_or(0.0, |a| a.duration),
            seed: self.seed,
        }
    }

    /// Analyzes the song's audio if that was skipped (reused import).
    fn ensure_analysis(&mut self) -> Result<()> {
        if self.analysis.is_some() {
            return Ok(());
        }
        let music = crate::library::find_music(&self.dir, &self.simfile.meta.music)
            .context("file audio lagu tidak ditemukan")?;
        let decoded = decode_file(&music, None, &AtomicBool::new(false), &mut |_| {})?;
        let analysis = analyze(&decoded.samples, decoded.sample_rate, &mut |_| {})?;
        self.analysis = Some(Arc::new(analysis));
        Ok(())
    }

    /// Regenerates the charts with a corrected BPM and rewrites the simfile.
    pub fn set_bpm(&mut self, bpm: f64) -> Result<()> {
        let bpm = bpm.clamp(20.0, 400.0);
        self.ensure_analysis()?;
        let onsets = self.analysis.as_ref().map_or(&[][..], |a| &a.onsets);
        let first_beat = best_phase(onsets, 60.0 / bpm);
        let offset = -first_beat;
        let charts = generate_all(&self.song_info(bpm, offset));
        self.simfile.timing = timing(bpm, offset);
        self.simfile.charts = charts;
        fs::write(self.dir.join(SIMFILE_NAME), write_sm(&self.simfile))
            .context("gagal menyimpan simfile")?;
        Ok(())
    }
}

/// Messages from the import thread.
#[derive(Debug)]
pub enum ImportEvent {
    /// Stage and progress inside the stage (0..=1).
    Progress(Stage, f32),
    /// Waveform peaks, sent right after decoding.
    Waveform(Vec<f32>),
    Done(Box<ImportResult>),
    Failed(String),
    Cancelled,
}

/// A running import.
pub struct ImportJob {
    pub source: PathBuf,
    pub events: Receiver<ImportEvent>,
    cancel: Arc<AtomicBool>,
}

impl ImportJob {
    /// Starts importing `source` into `<songs_root>/Imported/`.
    pub fn start(
        source: PathBuf,
        songs_root: &Path,
        ffmpeg_path: Option<PathBuf>,
        ctx: eframe::egui::Context,
    ) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        let job = ImportJob {
            source: source.clone(),
            events: rx,
            cancel: cancel.clone(),
        };
        let target = songs_root.join("Imported");
        let spawned = std::thread::Builder::new()
            .name("importer".into())
            .spawn(move || {
                let reporter = Reporter { tx, ctx };
                let staging = target.join(format!(
                    ".import-{}-{}",
                    std::process::id(),
                    unique_number()
                ));
                let outcome = run(
                    &source,
                    &target,
                    &staging,
                    ffmpeg_path.as_deref(),
                    &cancel,
                    &reporter,
                );
                let event = match outcome {
                    Ok(result) => ImportEvent::Done(Box::new(result)),
                    Err(e) => {
                        let _ = fs::remove_dir_all(&staging);
                        if e.downcast_ref::<Cancelled>().is_some() {
                            ImportEvent::Cancelled
                        } else {
                            log::warn!("import failed: {e:#}");
                            ImportEvent::Failed(friendly_error(&e))
                        }
                    }
                };
                reporter.send(event);
            });
        if let Err(e) = spawned {
            log::error!("could not start the import thread: {e}");
        }
        job
    }

    /// Asks the import thread to stop. Partial files are removed.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn unique_number() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}

struct Reporter {
    tx: Sender<ImportEvent>,
    ctx: eframe::egui::Context,
}

impl Reporter {
    fn send(&self, ev: ImportEvent) {
        let _ = self.tx.send(ev);
        self.ctx.request_repaint();
    }

    fn progress(&self, stage: Stage, p: f32) {
        self.send(ImportEvent::Progress(stage, p.clamp(0.0, 1.0)));
    }
}

/// Short, helpful message for the user.
fn friendly_error(e: &anyhow::Error) -> String {
    let root = e.root_cause().to_string();
    let top = e.to_string();
    if top == root {
        top
    } else {
        format!("{top} ({root})")
    }
}

fn check(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(Cancelled.into())
    } else {
        Ok(())
    }
}

/// FNV-1a 64-bit hash, used as the chart seed.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn timing(bpm: f64, offset: f64) -> TimingData {
    TimingData {
        offset: (offset * 1000.0).round() / 1000.0,
        bpms: vec![(0.0, bpm)],
        stops: Vec::new(),
    }
}

/// The whole pipeline. Everything is written to `staging` first and moved at the end,
/// so a cancelled or failed import never leaves a half-written song behind.
fn run(
    source: &Path,
    target: &Path,
    staging: &Path,
    ffmpeg_path: Option<&Path>,
    cancel: &AtomicBool,
    rep: &Reporter,
) -> Result<ImportResult> {
    let started = Instant::now();

    // 1. Read
    rep.progress(Stage::Reading, 0.0);
    let bytes =
        fs::read(source).with_context(|| format!("tidak bisa membaca {}", file_name(source)))?;
    let seed = fnv1a(&bytes);
    let source_size = bytes.len() as u64;
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    // Same file imported before: reuse that song, no analysis, no second copy.
    if let Some(existing) = find_existing(target, seed, &bytes, &ext)
        && let Some(result) = reuse(&existing, seed, started)
    {
        log::info!(
            "{} was imported before: reusing {}",
            file_name(source),
            existing.display()
        );
        rep.progress(Stage::Saving, 1.0);
        return Ok(result);
    }
    fs::create_dir_all(staging).context("tidak bisa membuat folder di Songs/Imported")?;
    rep.progress(Stage::Reading, 1.0);
    check(cancel)?;

    // 2. Extract audio
    rep.progress(Stage::Extracting, 0.0);
    let needs_conversion = matches!(ext.as_str(), "mp4" | "m4a" | "aac");
    let ffmpeg = if needs_conversion {
        ffmpeg::find_ffmpeg(ffmpeg_path)
    } else {
        None
    };
    let mut used_ffmpeg = false;
    let mut note = None;
    let (mut music_name, wav_out) = if !needs_conversion {
        let name = format!("audio.{ext}");
        fs::write(staging.join(&name), &bytes).context("gagal menyalin audio")?;
        (name, None)
    } else if ffmpeg.is_some() {
        ("audio.ogg".to_string(), None)
    } else {
        note = Some(
            "Tip: pasang ffmpeg agar file lagu hasil impor jauh lebih kecil (.ogg, bukan .wav)."
                .to_string(),
        );
        ("audio.wav".to_string(), Some(staging.join("audio.wav")))
    };
    drop(bytes);
    let mut wav_out = wav_out;
    if let Some(ff) = &ffmpeg {
        rep.progress(Stage::Extracting, 0.1);
        match ffmpeg::convert_to_ogg(ff, source, &staging.join("audio.ogg"), cancel) {
            Ok(()) => used_ffmpeg = true,
            Err(e) if e.downcast_ref::<Cancelled>().is_some() => return Err(e),
            Err(e) => {
                log::warn!("ffmpeg failed, writing WAV instead: {e:#}");
                note =
                    Some("ffmpeg gagal mengonversi, jadi audio disimpan sebagai .wav.".to_string());
                music_name = "audio.wav".into();
                wav_out = Some(staging.join("audio.wav"));
            }
        }
    }
    // Analyze exactly the file that will be played: decoders disagree on how much
    // AAC encoder delay to trim, which would shift the offset by ~20 ms.
    let analyzed = if used_ffmpeg {
        staging.join("audio.ogg")
    } else {
        source.to_path_buf()
    };
    let (base, share) = if used_ffmpeg { (0.6, 0.4) } else { (0.0, 1.0) };
    let mut decoded = decode_file(&analyzed, wav_out.as_deref(), cancel, &mut |p| {
        rep.progress(Stage::Extracting, base + p * share)
    })?;
    if used_ffmpeg && (decoded.title.is_none() || decoded.artist.is_none()) {
        // Tags normally survive the conversion; read the source just in case.
        if let Ok((t, a)) = crate::audio_dsp::decode::read_file_tags(source) {
            decoded.title = decoded.title.or(t);
            decoded.artist = decoded.artist.or(a);
        }
    }
    rep.progress(Stage::Extracting, 1.0);
    rep.send(ImportEvent::Waveform(waveform_peaks(&decoded.samples, 400)));
    check(cancel)?;

    // 3. Analyze
    rep.progress(Stage::Analyzing, 0.0);
    let analysis = analyze(&decoded.samples, decoded.sample_rate, &mut |p| {
        rep.progress(Stage::Analyzing, p)
    })?;
    check(cancel)?;

    // 4. Charts
    rep.progress(Stage::Charting, 0.0);
    let tempo = analysis.tempo;
    let song = SongInfo {
        onsets: &analysis.onsets,
        bpm: tempo.bpm,
        offset: tempo.offset,
        duration: analysis.duration,
        seed,
    };
    let charts: Vec<Chart> = generate_all(&song);
    rep.progress(Stage::Charting, 1.0);
    check(cancel)?;

    // 5. Save
    rep.progress(Stage::Saving, 0.0);
    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let title = decoded
        .title
        .clone()
        .unwrap_or_else(|| stem.replace('_', " ").trim().to_string());
    let simfile = Simfile {
        meta: Metadata {
            title: title.clone(),
            artist: decoded.artist.clone().unwrap_or_default(),
            credit: AUTO_CREDIT.into(),
            music: music_name,
            sample_start: (analysis.sample_start * 1000.0).round() / 1000.0,
            sample_length: SAMPLE_LENGTH,
            ..Metadata::default()
        },
        timing: timing(tempo.bpm, tempo.offset),
        charts,
    };
    // Same song already imported from a different encoding of the file (e.g. an
    // old MP4 import whose .ogg differs byte for byte): keep the existing one.
    if let Some(existing) = find_same_song(target, &simfile) {
        let _ = fs::remove_dir_all(staging);
        if let Err(e) = write_source_hash(&existing, seed, source_size) {
            log::debug!("could not mark {}: {e}", existing.display());
        }
        log::info!(
            "{} matches {}: not saved again",
            file_name(source),
            existing.display()
        );
        let mut result = reuse(&existing, seed, started).context("lagu tidak bisa dibaca")?;
        result.analysis = Some(Arc::new(analysis));
        rep.progress(Stage::Saving, 1.0);
        return Ok(result);
    }
    fs::write(staging.join(SIMFILE_NAME), write_sm(&simfile)).context("gagal menulis simfile")?;
    if let Err(e) = write_source_hash(staging, seed, source_size) {
        log::warn!("could not write the source marker: {e}");
    }
    check(cancel)?;
    let dir = unique_dir(target, &sanitize_folder_name(&title));
    fs::rename(staging, &dir).context("gagal memindahkan folder lagu")?;
    rep.progress(Stage::Saving, 1.0);
    Ok(ImportResult {
        dir,
        simfile,
        analysis: Some(Arc::new(analysis)),
        seed,
        used_ffmpeg,
        note,
        elapsed: started.elapsed(),
    })
}

/// An imported song made from the same file (same content hash), if any.
fn find_existing(target: &Path, hash: u64, bytes: &[u8], ext: &str) -> Option<PathBuf> {
    subdirs(target).into_iter().find(|dir| {
        if find_simfile(dir).is_none() {
            return false;
        }
        match read_source_hash(dir) {
            Some(h) => h == hash,
            // Imported before the marker existed: MP3/FLAC/... were copied as
            // they are, so compare the audio file itself (size first: cheap).
            None => {
                let music = dir.join(format!("audio.{ext}"));
                let same = fs::metadata(&music).is_ok_and(|m| m.len() == bytes.len() as u64)
                    && fs::read(&music).is_ok_and(|b| b == bytes);
                if same && let Err(e) = write_source_hash(dir, hash, bytes.len() as u64) {
                    log::debug!("could not mark {}: {e}", dir.display());
                }
                same
            }
        }
    })
}

/// An imported song with the same content fingerprint as `simfile`.
fn find_same_song(target: &Path, simfile: &Simfile) -> Option<PathBuf> {
    let print = fingerprint(simfile);
    subdirs(target).into_iter().find(|dir| {
        find_simfile(dir)
            .and_then(|p| load_simfile(&p).ok())
            .is_some_and(|s| fingerprint(&s) == print)
    })
}

/// Result for a song that was imported before.
fn reuse(dir: &Path, seed: u64, started: Instant) -> Option<ImportResult> {
    let simfile = load_simfile(&find_simfile(dir)?).ok()?;
    Some(ImportResult {
        dir: dir.to_path_buf(),
        simfile,
        analysis: None,
        seed,
        used_ffmpeg: false,
        note: None,
        elapsed: started.elapsed(),
    })
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Makes a folder name that is valid on Windows and Linux.
pub fn sanitize_folder_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = collapsed.chars().take(80).collect();
    // Windows does not allow trailing dots or spaces.
    while out.ends_with('.') || out.ends_with(' ') {
        out.pop();
    }
    while out.starts_with('.') {
        out.remove(0);
    }
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if out.is_empty() || RESERVED.contains(&out.to_ascii_uppercase().as_str()) {
        out = format!("Lagu {out}").trim().to_string();
    }
    out
}

/// `base/name`, or `base/name (2)`, `(3)`, … if it already exists.
pub fn unique_dir(base: &Path, name: &str) -> PathBuf {
    let first = base.join(name);
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| base.join(format!("{name} ({n})")))
        .find(|p| !p.exists())
        .expect("some numbered folder name is free")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_names() {
        assert_eq!(sanitize_folder_name("  AC/DC: Back?  "), "AC DC Back");
        assert_eq!(sanitize_folder_name("song..."), "song");
        assert_eq!(sanitize_folder_name("con"), "Lagu con");
        assert_eq!(sanitize_folder_name("***"), "Lagu");
        assert_eq!(sanitize_folder_name(&"x".repeat(200)).len(), 80);
    }

    #[test]
    fn numbered_folders() {
        let base = std::env::temp_dir().join(format!("ferrostep-unique-{}", std::process::id()));
        fs::create_dir_all(base.join("Song")).unwrap();
        fs::create_dir_all(base.join("Song (2)")).unwrap();
        assert_eq!(unique_dir(&base, "Song"), base.join("Song (3)"));
        assert_eq!(unique_dir(&base, "Other"), base.join("Other"));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn importable_extensions() {
        assert!(is_importable(Path::new("a/b/Song.MP3")));
        assert!(is_importable(Path::new("clip.mp4")));
        assert!(!is_importable(Path::new("notes.txt")));
    }
}

//! Simfile data types plus readers (`.sm`, `.ssc`) and a `.sm` writer.
//!
//! Nothing in here touches the UI, so everything can be tested with `cargo test`.

mod notes;
pub mod sm;
pub mod ssc;
mod tags;
pub mod writer;

use std::path::Path;

pub use notes::{columns_for_steps_type, parse_note_data};

/// Errors that can happen while reading a simfile.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("could not read file: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported file extension (expected .sm or .ssc)")]
    UnsupportedExtension,
    #[error("the file has no BPM information (#BPMS)")]
    MissingBpm,
}

/// A whole song: metadata, song-wide timing and all charts.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Simfile {
    pub meta: Metadata,
    pub timing: TimingData,
    pub charts: Vec<Chart>,
}

/// Human-readable information about a song.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Metadata {
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub credit: String,
    pub banner: String,
    pub background: String,
    /// Audio file name, relative to the simfile's folder.
    pub music: String,
    /// Where the song-select preview starts (seconds).
    pub sample_start: f64,
    /// How long the preview lasts (seconds).
    pub sample_length: f64,
    /// Tags we do not understand, kept so nothing is lost.
    pub extra: Vec<(String, String)>,
}

/// Timing information: offset, BPM changes and stops.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TimingData {
    /// StepMania `#OFFSET`. The time (seconds) of beat 0 is `-offset`.
    pub offset: f64,
    /// `(beat, bpm)` pairs, sorted by beat.
    pub bpms: Vec<(f64, f64)>,
    /// `(beat, seconds)` pairs, sorted by beat.
    pub stops: Vec<(f64, f64)>,
}

/// Chart difficulty slot, as in StepMania.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Difficulty {
    Beginner,
    Easy,
    Medium,
    Hard,
    Challenge,
    Edit,
}

impl Difficulty {
    /// Parses StepMania difficulty names, including old aliases.
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "beginner" => Self::Beginner,
            "easy" | "basic" | "light" => Self::Easy,
            "medium" | "another" | "trick" | "standard" => Self::Medium,
            "hard" | "ssr" | "maniac" | "heavy" => Self::Hard,
            "challenge" | "smaniac" | "expert" | "oni" => Self::Challenge,
            _ => Self::Edit,
        }
    }

    /// Name written to `.sm` files.
    pub fn as_sm_str(self) -> &'static str {
        match self {
            Self::Beginner => "Beginner",
            Self::Easy => "Easy",
            Self::Medium => "Medium",
            Self::Hard => "Hard",
            Self::Challenge => "Challenge",
            Self::Edit => "Edit",
        }
    }
}

/// One playable chart (a difficulty of a song).
#[derive(Debug, Clone, PartialEq)]
pub struct Chart {
    /// For example `dance-single`.
    pub steps_type: String,
    pub description: String,
    pub difficulty: Difficulty,
    pub meter: u32,
    /// Radar values string, kept as-is.
    pub radar: String,
    /// Number of columns (4 for `dance-single`).
    pub columns: u8,
    /// Notes sorted by beat, then column.
    pub notes: Vec<Note>,
    /// Per-chart timing (`.ssc` only). Overrides song timing when present.
    pub timing_override: Option<TimingData>,
}

impl Chart {
    /// True for charts the game can play (4-panel single).
    pub fn is_dance_single(&self) -> bool {
        self.steps_type.eq_ignore_ascii_case("dance-single")
    }
}

/// A note in beat units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Note {
    pub beat: f64,
    pub column: u8,
    pub kind: NoteKind,
}

/// What kind of note this is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NoteKind {
    Tap,
    HoldHead { end_beat: f64 },
    RollHead { end_beat: f64 },
    Mine,
    Lift,
    Fake,
}

impl Simfile {
    /// Timing that applies to a chart (its own override, or the song's).
    pub fn timing_for<'a>(&'a self, chart: &'a Chart) -> &'a TimingData {
        chart.timing_override.as_ref().unwrap_or(&self.timing)
    }
}

/// Reads a `.sm` or `.ssc` file from disk.
///
/// Bytes that are not valid UTF-8 (old Latin-1 files) are replaced instead of failing.
pub fn load_simfile(path: &Path) -> Result<Simfile, ParseError> {
    let bytes = std::fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("sm") => sm::parse(&text),
        Some("ssc") => ssc::parse(&text),
        _ => Err(ParseError::UnsupportedExtension),
    }
}

/// Parses `beat=value,beat=value` lists used by `#BPMS` and `#STOPS`.
///
/// Malformed entries are skipped with a warning. The result is sorted by beat.
pub fn parse_beat_pairs(value: &str) -> Vec<(f64, f64)> {
    let mut pairs: Vec<(f64, f64)> = value
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .filter_map(|entry| {
            let (b, v) = entry.split_once('=')?;
            match (b.trim().parse::<f64>(), v.trim().parse::<f64>()) {
                (Ok(b), Ok(v)) if b.is_finite() && v.is_finite() => Some((b, v)),
                _ => {
                    log::warn!("ignoring malformed timing entry {entry:?}");
                    None
                }
            }
        })
        .collect();
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
    pairs
}

/// Parses a number, returning 0.0 for empty or invalid text.
pub(crate) fn parse_f64_or_zero(s: &str) -> f64 {
    s.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
}

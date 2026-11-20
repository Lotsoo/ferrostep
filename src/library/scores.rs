//! Best scores per chart, saved as JSON in the config folder.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::config_dir;
use crate::parser::Chart;

/// Best result for one chart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BestScore {
    pub percent: f64,
    pub grade: String,
    pub max_combo: u32,
}

/// All best scores, keyed by [`chart_key`].
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ScoreStore {
    pub best: HashMap<String, BestScore>,
}

/// Stable key for a chart: song folder name + steps type + difficulty + description.
pub fn chart_key(song_dir: &std::path::Path, chart: &Chart) -> String {
    let folder = song_dir
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!(
        "{folder}|{}|{:?}|{}",
        chart.steps_type, chart.difficulty, chart.description
    )
}

impl ScoreStore {
    fn path() -> Result<PathBuf> {
        Ok(config_dir()?.join("scores.json"))
    }

    /// Loads scores; a missing or broken file gives an empty store (logged).
    pub fn load() -> Self {
        let result = Self::path().and_then(|p| {
            if !p.exists() {
                return Ok(Self::default());
            }
            Ok(serde_json::from_str(&std::fs::read_to_string(p)?)?)
        });
        result.unwrap_or_else(|e| {
            log::warn!("could not load scores: {e:#}");
            Self::default()
        })
    }

    pub fn save(&self) -> Result<()> {
        std::fs::write(Self::path()?, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<&BestScore> {
        self.best.get(key)
    }

    /// Stores `score` if it beats the previous best. Returns the previous best percentage.
    pub fn record(&mut self, key: String, score: BestScore) -> Option<f64> {
        let previous = self.best.get(&key).map(|b| b.percent);
        if previous.is_none_or(|p| score.percent > p) {
            self.best.insert(key, score);
            if let Err(e) = self.save() {
                log::warn!("could not save scores: {e:#}");
            }
        }
        previous
    }
}

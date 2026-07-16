//! Shared vocabulary for the WAGA tick kernel.

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Convenient result alias for library crates.
pub type Result<T> = std::result::Result<T, WagaError>;

/// Domain and I/O errors surfaced by WAGA crates.
#[derive(Debug, thiserror::Error)]
pub enum WagaError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML error: {0}")]
    Toml(String),

    #[error("git sensor error: {0}")]
    Git(String),

    #[error("{0}")]
    Msg(String),
}

/// Git facts observed for a single tick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitStatus {
    pub repo_path: PathBuf,
    pub branch: String,
    pub dirty: bool,
}

/// Soft narrative fields (not ground truth for the park).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StoryState {
    pub last_beat: String,
    pub theme: Option<String>,
}

/// Persistent “what is true now” snapshot for the park.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub tick: u64,
    pub observed_at: DateTime<Local>,
    pub timezone: String,
    pub git: Option<GitStatus>,
    pub story: StoryState,
    pub active_persona: String,
}

impl WorldSnapshot {
    /// Brand-new park state before the first tick advances.
    pub fn fresh(active_persona: impl Into<String>) -> Self {
        Self {
            tick: 0,
            observed_at: Local::now(),
            timezone: iana_timezone_or_offset(),
            git: None,
            story: StoryState::default(),
            active_persona: active_persona.into(),
        }
    }
}

/// Best-effort timezone label for display.
pub fn iana_timezone_or_offset() -> String {
    // chrono Local offset is always available; IANA name needs OS help.
    let offset = Local::now().offset().local_minus_utc();
    let hours = offset / 3600;
    let mins = (offset.abs() % 3600) / 60;
    if mins == 0 {
        format!("UTC{hours:+}")
    } else {
        format!("UTC{hours:+}:{mins:02}")
    }
}

/// One append-only narrative line after a tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeEntry {
    pub tick: u64,
    pub at: DateTime<Local>,
    pub persona: String,
    pub git_dirty: Option<bool>,
    pub notice: String,
    pub pet_mood: String,
}

/// Outcome of a full tick (world + character + pet).
#[derive(Debug, Clone)]
pub struct TickResult {
    pub snapshot: WorldSnapshot,
    pub notice: String,
    pub pet_mood: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_snapshot_starts_at_tick_zero() {
        let snap = WorldSnapshot::fresh("strict-cto");
        assert_eq!(snap.tick, 0);
        assert_eq!(snap.active_persona, "strict-cto");
        assert!(snap.git.is_none());
        assert!(snap.story.last_beat.is_empty());
    }
}

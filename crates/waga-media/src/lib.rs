//! System media: now playing + control via MPRIS (`playerctl`).

use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("playerctl not found — install playerctl for MPRIS control")]
    PlayerctlMissing,
    #[error("media command failed: {0}")]
    Command(String),
    #[error("no active player")]
    NoPlayer,
}

pub type Result<T> = std::result::Result<T, MediaError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackState {
    Playing,
    Paused,
    Stopped,
    Unknown,
}

impl PlaybackState {
    pub fn as_str(self) -> &'static str {
        match self {
            PlaybackState::Playing => "playing",
            PlaybackState::Paused => "paused",
            PlaybackState::Stopped => "stopped",
            PlaybackState::Unknown => "unknown",
        }
    }
}

/// Snapshot of current system media (MPRIS).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NowPlaying {
    pub available: bool,
    pub player: Option<String>,
    pub state: PlaybackState,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub raw_status: String,
}

impl NowPlaying {
    pub fn display_line(&self) -> String {
        if !self.available {
            return "♪ now: (playerctl not installed)".into();
        }
        if self.player.is_none() && self.title.is_none() {
            return "♪ now: (nothing playing)".into();
        }
        let who = self.artist.as_deref().unwrap_or("Unknown");
        let what = self.title.as_deref().unwrap_or("(no title)");
        let player = self.player.as_deref().unwrap_or("?");
        format!(
            "♪ now: [{}] {} — {} ({})",
            self.state.as_str(),
            who,
            what,
            player
        )
    }
}

fn playerctl_exists() -> bool {
    Command::new("playerctl")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn run_playerctl(args: &[&str]) -> Result<String> {
    if !playerctl_exists() {
        return Err(MediaError::PlayerctlMissing);
    }
    let out = Command::new("playerctl")
        .args(args)
        .output()
        .map_err(|e| MediaError::Command(e.to_string()))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if !out.status.success() {
        if stderr.contains("No players found") || stdout.is_empty() {
            return Err(MediaError::NoPlayer);
        }
        return Err(MediaError::Command(if stderr.is_empty() {
            stdout
        } else {
            stderr
        }));
    }
    Ok(stdout)
}

fn meta(key: &str) -> Option<String> {
    run_playerctl(&["metadata", key]).ok().filter(|s| !s.is_empty())
}

/// Read current MPRIS now-playing info.
pub fn now_playing() -> NowPlaying {
    if !playerctl_exists() {
        return NowPlaying {
            available: false,
            player: None,
            state: PlaybackState::Unknown,
            title: None,
            artist: None,
            album: None,
            raw_status: "playerctl missing".into(),
        };
    }

    let player = run_playerctl(&["-l"]).ok().and_then(|s| {
        s.lines()
            .next()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
    });

    let status = run_playerctl(&["status"]).unwrap_or_default();
    let state = match status.to_lowercase().as_str() {
        "playing" => PlaybackState::Playing,
        "paused" => PlaybackState::Paused,
        "stopped" => PlaybackState::Stopped,
        _ => PlaybackState::Unknown,
    };

    let title = meta("title").or_else(|| meta("xesam:title"));
    let artist = meta("artist").or_else(|| meta("xesam:artist"));
    let album = meta("album").or_else(|| meta("xesam:album"));

    let no_player = player.is_none() && title.is_none() && status.is_empty();

    NowPlaying {
        available: true,
        player,
        state: if no_player {
            PlaybackState::Stopped
        } else {
            state
        },
        title,
        artist,
        album,
        raw_status: if status.is_empty() {
            "idle".into()
        } else {
            status
        },
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MediaCommand {
    Play,
    Pause,
    PlayPause,
    Next,
    Previous,
    Stop,
}

impl MediaCommand {
    fn args(self) -> &'static [&'static str] {
        match self {
            MediaCommand::Play => &["play"],
            MediaCommand::Pause => &["pause"],
            MediaCommand::PlayPause => &["play-pause"],
            MediaCommand::Next => &["next"],
            MediaCommand::Previous => &["previous"],
            MediaCommand::Stop => &["stop"],
        }
    }
}

/// Send a control command to the active MPRIS player.
pub fn control(cmd: MediaCommand) -> Result<()> {
    run_playerctl(cmd.args())?;
    Ok(())
}

/// Format multi-line now-playing block for CLI.
pub fn format_now_playing(np: &NowPlaying) -> String {
    if !np.available {
        return [
            "♪ Now playing",
            "  playerctl is not installed.",
            "  Install: sudo apt install playerctl  (or your distro)",
        ]
        .join("\n");
    }
    if np.title.is_none() && np.player.is_none() {
        return "♪ Now playing\n  (no active MPRIS player)".into();
    }
    format!(
        "♪ Now playing\n  state:  {}\n  title:  {}\n  artist: {}\n  album:  {}\n  player: {}\n  keys:   waga music play|pause|next|prev|toggle",
        np.state.as_str(),
        np.title.as_deref().unwrap_or("—"),
        np.artist.as_deref().unwrap_or("—"),
        np.album.as_deref().unwrap_or("—"),
        np.player.as_deref().unwrap_or("—"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_line_handles_missing_playerctl() {
        // Should not panic whether playerctl exists or not
        let np = now_playing();
        let line = np.display_line();
        assert!(line.starts_with('♪') || line.contains("now"));
    }

    #[test]
    fn format_block_nonempty() {
        let np = NowPlaying {
            available: true,
            player: Some("spotify".into()),
            state: PlaybackState::Playing,
            title: Some("Song".into()),
            artist: Some("Artist".into()),
            album: Some("Album".into()),
            raw_status: "Playing".into(),
        };
        let s = format_now_playing(&np);
        assert!(s.contains("Song"));
        assert!(s.contains("Artist"));
    }
}

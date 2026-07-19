//! Always-on park daemon helpers: status file + file-backed notify bus.

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use waga_core::{Result, TickResult};

/// Live daemon status (projection cache under data dir).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub pid: u32,
    pub started_at: DateTime<Local>,
    pub every_secs: u64,
    pub running: bool,
    pub last_tick: u64,
    pub last_at: Option<DateTime<Local>>,
    pub last_mood: String,
    pub last_notice: String,
    pub last_interesting: bool,
    pub ticks_total: u64,
    pub interesting_total: u64,
}

impl DaemonStatus {
    pub fn path(root: &Path) -> PathBuf {
        root.join("daemon.json")
    }

    pub fn start(every_secs: u64) -> Self {
        Self {
            pid: std::process::id(),
            started_at: Local::now(),
            every_secs,
            running: true,
            last_tick: 0,
            last_at: None,
            last_mood: "idle".into(),
            last_notice: String::new(),
            last_interesting: false,
            ticks_total: 0,
            interesting_total: 0,
        }
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        fs::create_dir_all(root)?;
        let text = serde_json::to_string_pretty(self)?;
        fs::write(Self::path(root), text)?;
        Ok(())
    }

    pub fn load(root: &Path) -> Result<Option<Self>> {
        let path = Self::path(root);
        if !path.exists() {
            return Ok(None);
        }
        let text = fs::read_to_string(path)?;
        Ok(Some(serde_json::from_str(&text)?))
    }

    pub fn mark_stopped(&mut self, root: &Path) -> Result<()> {
        self.running = false;
        self.save(root)
    }

    pub fn record_tick(&mut self, result: &TickResult, interesting: bool) {
        self.last_tick = result.snapshot.tick;
        self.last_at = Some(result.snapshot.observed_at);
        self.last_mood = result.pet_mood.clone();
        self.last_notice = result.notice.clone();
        self.last_interesting = interesting;
        self.ticks_total = self.ticks_total.saturating_add(1);
        if interesting {
            self.interesting_total = self.interesting_total.saturating_add(1);
        }
    }
}

/// One high-signal notify bus entry (append-only JSONL).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotifyEntry {
    pub at: DateTime<Local>,
    pub tick: u64,
    pub kind: String,
    pub text: String,
    pub mood: String,
}

/// File-backed notify bus for daemons / future bridges.
pub struct NotifyBus {
    root: PathBuf,
}

impl NotifyBus {
    pub fn open(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn path(&self) -> PathBuf {
        self.root.join("notify.jsonl")
    }

    pub fn append(&self, entry: &NotifyEntry) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path())?;
        writeln!(file, "{}", serde_json::to_string(entry)?)?;
        file.flush()?;
        Ok(())
    }

    /// Load last `n` entries (full scan; fine for v0).
    pub fn load_last(&self, n: usize) -> Result<Vec<NotifyEntry>> {
        let path = self.path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let text = fs::read_to_string(path)?;
        let mut all = Vec::new();
        for line in text.lines() {
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            match serde_json::from_str::<NotifyEntry>(t) {
                Ok(e) => all.push(e),
                Err(e) => tracing::warn!("skip corrupt notify line: {e}"),
            }
        }
        let start = all.len().saturating_sub(n);
        Ok(all[start..].to_vec())
    }
}

/// Snapshot of open-story title for high-signal detection across ticks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DaemonWatch {
    pub last_mood: String,
    pub open_story_title: Option<String>,
}

/// True when the tick is worth printing / bus-notifying (not a quiet heartbeat).
pub fn is_interesting_tick(
    watch: &DaemonWatch,
    result: &TickResult,
    open_story_title: Option<&str>,
) -> bool {
    if result.memories_formed > 0 || result.xp_granted > 0 {
        return true;
    }
    if !watch.last_mood.is_empty() && watch.last_mood != result.pet_mood {
        return true;
    }
    if watch.open_story_title.as_deref() != open_story_title {
        return true;
    }
    false
}

/// Build notify entries from a high-signal tick (0+ lines).
pub fn notify_entries_for_tick(
    watch: &DaemonWatch,
    result: &TickResult,
    open_story_title: Option<&str>,
) -> Vec<NotifyEntry> {
    let at = result.snapshot.observed_at;
    let tick = result.snapshot.tick;
    let mood = result.pet_mood.clone();
    let mut out = Vec::new();

    if watch.open_story_title.is_none() && open_story_title.is_some() {
        out.push(NotifyEntry {
            at,
            tick,
            kind: "story_opened".into(),
            text: format!("Story opened: {}", open_story_title.unwrap_or("?")),
            mood: mood.clone(),
        });
    }
    if watch.open_story_title.is_some() && open_story_title.is_none() {
        out.push(NotifyEntry {
            at,
            tick,
            kind: "story_closed".into(),
            text: format!(
                "Story closed (was: {})",
                watch.open_story_title.as_deref().unwrap_or("?")
            ),
            mood: mood.clone(),
        });
    }
    if !watch.last_mood.is_empty() && watch.last_mood != result.pet_mood {
        out.push(NotifyEntry {
            at,
            tick,
            kind: "mood".into(),
            text: format!("Mood {} → {}", watch.last_mood, result.pet_mood),
            mood: mood.clone(),
        });
    }
    if result.memories_formed > 0 {
        out.push(NotifyEntry {
            at,
            tick,
            kind: "memory".into(),
            text: format!(
                "{} memor{} formed",
                result.memories_formed,
                if result.memories_formed == 1 {
                    "y"
                } else {
                    "ies"
                }
            ),
            mood: mood.clone(),
        });
    }
    if result.xp_granted > 0 {
        out.push(NotifyEntry {
            at,
            tick,
            kind: "xp".into(),
            text: format!("XP +{}", result.xp_granted),
            mood: mood.clone(),
        });
    }
    if out.is_empty() {
        out.push(NotifyEntry {
            at,
            tick,
            kind: "signal".into(),
            text: result.notice.clone(),
            mood,
        });
    }
    out
}

/// Apply tick outcomes to watch state.
pub fn update_watch(
    watch: &mut DaemonWatch,
    result: &TickResult,
    open_story_title: Option<String>,
) {
    watch.last_mood = result.pet_mood.clone();
    watch.open_story_title = open_story_title;
}

/// One-line status for CLI.
pub fn format_daemon_status(s: &DaemonStatus) -> String {
    let last = s
        .last_at
        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "never".into());
    format!(
        "daemon {} pid={} every={}s ticks={} interesting={} last_tick={} last_at={} mood={}\nnotice: {}",
        if s.running { "RUNNING" } else { "stopped" },
        s.pid,
        s.every_secs,
        s.ticks_total,
        s.interesting_total,
        s.last_tick,
        last,
        s.last_mood,
        if s.last_notice.is_empty() {
            "—"
        } else {
            &s.last_notice
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use waga_core::WorldSnapshot;

    fn dummy_result(tick: u64, mood: &str, mem: usize, xp: u32) -> TickResult {
        let mut snap = WorldSnapshot::fresh("strict-cto");
        snap.tick = tick;
        TickResult {
            snapshot: snap,
            notice: "hello".into(),
            pet_mood: mood.into(),
            new_event_ids: vec![],
            memories_formed: mem,
            xp_granted: xp,
        }
    }

    #[test]
    fn quiet_tick_not_interesting() {
        let watch = DaemonWatch {
            last_mood: "content".into(),
            open_story_title: None,
        };
        let r = dummy_result(3, "content", 0, 0);
        assert!(!is_interesting_tick(&watch, &r, None));
    }

    #[test]
    fn mood_change_is_interesting() {
        let watch = DaemonWatch {
            last_mood: "content".into(),
            open_story_title: None,
        };
        let r = dummy_result(3, "grumpy", 0, 0);
        assert!(is_interesting_tick(&watch, &r, None));
    }

    #[test]
    fn story_open_is_interesting() {
        let watch = DaemonWatch {
            last_mood: "grumpy".into(),
            open_story_title: None,
        };
        let r = dummy_result(3, "grumpy", 0, 0);
        assert!(is_interesting_tick(&watch, &r, Some("dirty")));
    }

    #[test]
    fn notify_bus_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let bus = NotifyBus::open(dir.path());
        let e = NotifyEntry {
            at: Local::now(),
            tick: 1,
            kind: "xp".into(),
            text: "XP +10".into(),
            mood: "content".into(),
        };
        bus.append(&e).unwrap();
        let last = bus.load_last(5).unwrap();
        assert_eq!(last.len(), 1);
        assert_eq!(last[0].kind, "xp");
    }

    #[test]
    fn daemon_status_persists() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = DaemonStatus::start(15);
        let r = dummy_result(4, "content", 1, 10);
        s.record_tick(&r, true);
        s.save(dir.path()).unwrap();
        let loaded = DaemonStatus::load(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.last_tick, 4);
        assert_eq!(loaded.interesting_total, 1);
        assert!(loaded.running);
    }
}

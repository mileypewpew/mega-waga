//! File bridge between Mega Waga and Grok Build (no A2A yet).
//!
//! Export (WAGA → Build): `.waga/bridge/world.md` + `world.json`
//! Inbox  (Build → WAGA): `.waga/bridge/inbox.jsonl`

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use waga_core::{Result, StoryStatus, WorldSnapshot};
use waga_events::StoryStore;
use waga_memory::{list_skills, recent_memories, skills_summary_line};
use waga_pet::mood_from_snapshot;

use crate::{DaemonStatus, NotifyBus};

/// Directory under data dir for bridge files.
pub fn bridge_dir(root: &Path) -> PathBuf {
    root.join("bridge")
}

/// Structured park digest for tools / agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeWorld {
    pub exported_at: DateTime<Local>,
    pub tick: u64,
    pub persona: String,
    pub timezone: String,
    pub mood: String,
    pub git_dirty: Option<bool>,
    pub git_branch: Option<String>,
    pub git_repo: Option<String>,
    pub notice: String,
    pub open_story: Option<String>,
    pub recent_memories: Vec<String>,
    pub skills_line: String,
    pub daemon_running: Option<bool>,
    pub recent_notifies: Vec<String>,
    /// Short blurb safe to paste into agent context.
    pub blurb: String,
}

/// One message from Grok Build (or any external agent) into the park.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeInboxMessage {
    pub at: DateTime<Local>,
    #[serde(default = "default_source")]
    pub source: String,
    /// `status` | `blocked` | `note` | free-form
    pub kind: String,
    pub text: String,
    #[serde(default)]
    pub session: Option<String>,
}

fn default_source() -> String {
    "grok-build".into()
}

/// Paths used by the bridge.
pub struct BridgePaths {
    pub root: PathBuf,
}

impl BridgePaths {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            root: data_dir.into(),
        }
    }

    pub fn dir(&self) -> PathBuf {
        bridge_dir(&self.root)
    }

    pub fn world_md(&self) -> PathBuf {
        self.dir().join("world.md")
    }

    pub fn world_json(&self) -> PathBuf {
        self.dir().join("world.json")
    }

    pub fn inbox_jsonl(&self) -> PathBuf {
        self.dir().join("inbox.jsonl")
    }

    pub fn outbox_jsonl(&self) -> PathBuf {
        self.dir().join("outbox.jsonl")
    }

    /// How many inbox lines we have already spoken (cursor).
    pub fn inbox_spoken_cursor(&self) -> PathBuf {
        self.dir().join("inbox_spoken.cursor")
    }

    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(self.dir())?;
        Ok(())
    }
}

/// One message from human / WAGA → Grok Build.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeOutboxMessage {
    pub at: DateTime<Local>,
    #[serde(default = "default_user_source")]
    pub source: String,
    /// `chat` | `note` | free-form
    pub kind: String,
    pub text: String,
    #[serde(default)]
    pub session: Option<String>,
}

fn default_user_source() -> String {
    "user".into()
}

/// Unified thread row for display.
#[derive(Debug, Clone)]
pub struct BridgeThreadLine {
    pub at: DateTime<Local>,
    pub direction: &'static str, // "→" outbox (to Build) | "←" inbox (from Build)
    pub kind: String,
    pub source: String,
    pub text: String,
}

impl BridgeThreadLine {
    pub fn format(&self) -> String {
        format!(
            "{} {} [{}] {} — {}",
            self.at.format("%Y-%m-%d %H:%M:%S"),
            self.direction,
            self.kind,
            self.source,
            self.text
        )
    }
}

/// Build a digest from current park state (does not write).
pub fn build_bridge_world(data_dir: &Path) -> Result<BridgeWorld> {
    let snap = crate::peek_snapshot(data_dir, "strict-cto")?;
    let store = StoryStore::load(data_dir)?;
    let open_story = store
        .stories
        .iter()
        .find(|s| s.status == StoryStatus::Open)
        .map(|s| s.title.clone());
    let mems = recent_memories(data_dir, 3).unwrap_or_default();
    let mem_titles: Vec<String> = mems.iter().map(|m| m.title.clone()).collect();
    let skills_line = skills_summary_line(data_dir).unwrap_or_else(|_| "skills: ?".into());
    let _ = list_skills(data_dir); // warm skills path; summary already loaded
    let daemon = DaemonStatus::load(data_dir).ok().flatten();
    let notifies = NotifyBus::open(data_dir)
        .load_last(5)
        .unwrap_or_default()
        .into_iter()
        .map(|n| format!("[{}] {}", n.kind, n.text))
        .collect::<Vec<_>>();

    let mood = mood_from_snapshot(&snap).as_str().to_string();
    let notice = if snap.story.last_beat.is_empty() {
        "(no notice yet)".into()
    } else {
        snap.story.last_beat.clone()
    };

    let blurb = format_blurb(&snap, &mood, &notice, open_story.as_deref(), &mem_titles, &skills_line);

    Ok(BridgeWorld {
        exported_at: Local::now(),
        tick: snap.tick,
        persona: snap.active_persona.clone(),
        timezone: snap.timezone.clone(),
        mood,
        git_dirty: snap.git.as_ref().map(|g| g.dirty),
        git_branch: snap.git.as_ref().map(|g| g.branch.clone()),
        git_repo: snap.git.as_ref().map(|g| g.repo_path.display().to_string()),
        notice,
        open_story,
        recent_memories: mem_titles,
        skills_line,
        daemon_running: daemon.map(|d| d.running),
        recent_notifies: notifies,
        blurb,
    })
}

fn format_blurb(
    snap: &WorldSnapshot,
    mood: &str,
    notice: &str,
    open_story: Option<&str>,
    mems: &[String],
    skills: &str,
) -> String {
    let git = match &snap.git {
        Some(g) => format!(
            "{} @ {} ({})",
            g.branch,
            g.repo_path.display(),
            if g.dirty { "DIRTY" } else { "clean" }
        ),
        None => "git: (none)".into(),
    };
    let story = open_story
        .map(|t| format!("open story: {t}"))
        .unwrap_or_else(|| "open story: none".into());
    let mem = if mems.is_empty() {
        "memories: (none)".into()
    } else {
        format!("memories: {}", mems.join(" · "))
    };
    format!(
        "WAGA park tick {} · persona {} · mood {} · {}\n{}\n{}\nnotice: {}\n{}\n{}",
        snap.tick,
        snap.active_persona,
        mood,
        snap.timezone,
        git,
        story,
        notice,
        mem,
        skills
    )
}

/// Write world.md + world.json for Grok Build (and humans) to read.
pub fn export_bridge(data_dir: &Path) -> Result<BridgeWorld> {
    let paths = BridgePaths::new(data_dir);
    paths.ensure()?;
    let world = build_bridge_world(data_dir)?;
    fs::write(&paths.world_json(), serde_json::to_string_pretty(&world)?)?;
    fs::write(&paths.world_md(), render_world_md(&world))?;
    Ok(world)
}

fn render_world_md(w: &BridgeWorld) -> String {
    let mut out = String::new();
    out.push_str("# WAGA park → Grok Build\n\n");
    out.push_str(&format!(
        "_Exported {} · tick {} · persona `{}`_\n\n",
        w.exported_at.format("%Y-%m-%d %H:%M:%S"),
        w.tick,
        w.persona
    ));
    out.push_str("## Blurb (paste into agent context)\n\n");
    out.push_str("```\n");
    out.push_str(&w.blurb);
    out.push_str("\n```\n\n");
    out.push_str("## Snapshot\n\n");
    out.push_str(&format!("- **Mood:** {}\n", w.mood));
    if let (Some(branch), Some(dirty)) = (&w.git_branch, w.git_dirty) {
        out.push_str(&format!(
            "- **Git:** `{}` ({})\n",
            branch,
            if dirty { "DIRTY" } else { "clean" }
        ));
    }
    if let Some(repo) = &w.git_repo {
        out.push_str(&format!("- **Repo:** `{repo}`\n"));
    }
    out.push_str(&format!("- **Notice:** {}\n", w.notice));
    match &w.open_story {
        Some(t) => out.push_str(&format!("- **Open story:** {t}\n")),
        None => out.push_str("- **Open story:** none\n"),
    }
    out.push_str(&format!("- **Skills:** {}\n", w.skills_line));
    if let Some(d) = w.daemon_running {
        out.push_str(&format!(
            "- **Daemon:** {}\n",
            if d { "running" } else { "stopped" }
        ));
    }
    if !w.recent_memories.is_empty() {
        out.push_str("\n## Recent memories\n\n");
        for m in &w.recent_memories {
            out.push_str(&format!("- {m}\n"));
        }
    }
    if !w.recent_notifies.is_empty() {
        out.push_str("\n## Recent notifies\n\n");
        for n in &w.recent_notifies {
            out.push_str(&format!("- {n}\n"));
        }
    }
    out.push_str("\n## Conversation\n\n");
    out.push_str("- **You → Build:** `bridge/outbox.jsonl` (or `waga talk`)\n");
    out.push_str("- **Build → you:** `bridge/inbox.jsonl` (`status` · `blocked` · `reply` · `note`)\n\n");
    out.push_str("### Inbox line example\n\n");
    out.push_str("```json\n");
    out.push_str(r#"{"at":"2026-07-19T15:00:00+02:00","source":"grok-build","kind":"blocked","text":"cargo test failed","session":"optional"}"#);
    out.push('\n');
    out.push_str("```\n");
    out
}

/// Append a message from Grok Build (or CLI) into the inbox.
pub fn append_inbox(data_dir: &Path, msg: &BridgeInboxMessage) -> Result<()> {
    let paths = BridgePaths::new(data_dir);
    paths.ensure()?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.inbox_jsonl())?;
    writeln!(file, "{}", serde_json::to_string(msg)?)?;
    file.flush()?;
    Ok(())
}

/// Convenience: post a Build → park message with current timestamp.
pub fn post_inbox(
    data_dir: &Path,
    kind: impl Into<String>,
    text: impl Into<String>,
    source: impl Into<String>,
    session: Option<String>,
) -> Result<BridgeInboxMessage> {
    let msg = BridgeInboxMessage {
        at: Local::now(),
        source: source.into(),
        kind: kind.into(),
        text: text.into(),
        session,
    };
    append_inbox(data_dir, &msg)?;
    Ok(msg)
}

/// Load inbox messages (all lines; skip corrupt).
pub fn load_inbox(data_dir: &Path) -> Result<Vec<BridgeInboxMessage>> {
    let path = BridgePaths::new(data_dir).inbox_jsonl();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = fs::File::open(path)?;
    let mut out = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line?;
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        match serde_json::from_str::<BridgeInboxMessage>(t) {
            Ok(m) => out.push(m),
            Err(e) => tracing::warn!("skip corrupt inbox line: {e}"),
        }
    }
    Ok(out)
}

/// Last `n` inbox messages.
pub fn load_inbox_last(data_dir: &Path, n: usize) -> Result<Vec<BridgeInboxMessage>> {
    let all = load_inbox(data_dir)?;
    let start = all.len().saturating_sub(n);
    Ok(all[start..].to_vec())
}

/// Human line for an inbox message.
pub fn format_inbox_line(m: &BridgeInboxMessage) -> String {
    let session = m
        .session
        .as_deref()
        .map(|s| format!(" session={s}"))
        .unwrap_or_default();
    format!(
        "{} [{}] {} — {}{}",
        m.at.format("%Y-%m-%d %H:%M:%S"),
        m.kind,
        m.source,
        m.text,
        session
    )
}

/// Count inbox messages.
pub fn inbox_len(data_dir: &Path) -> usize {
    load_inbox(data_dir).map(|v| v.len()).unwrap_or(0)
}

/// Append a human/WAGA → Build message.
pub fn append_outbox(data_dir: &Path, msg: &BridgeOutboxMessage) -> Result<()> {
    let paths = BridgePaths::new(data_dir);
    paths.ensure()?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.outbox_jsonl())?;
    writeln!(file, "{}", serde_json::to_string(msg)?)?;
    file.flush()?;
    Ok(())
}

/// Post chat/note to outbox with timestamp; refreshes world export.
pub fn post_outbox(
    data_dir: &Path,
    kind: impl Into<String>,
    text: impl Into<String>,
    source: impl Into<String>,
    session: Option<String>,
) -> Result<BridgeOutboxMessage> {
    let text = text.into();
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(waga_core::WagaError::Msg("empty outbox text".into()));
    }
    // Keep park blurb current for Build.
    let _ = export_bridge(data_dir);
    let msg = BridgeOutboxMessage {
        at: Local::now(),
        source: source.into(),
        kind: kind.into(),
        text,
        session,
    };
    append_outbox(data_dir, &msg)?;
    Ok(msg)
}

/// Load all outbox messages.
pub fn load_outbox(data_dir: &Path) -> Result<Vec<BridgeOutboxMessage>> {
    let path = BridgePaths::new(data_dir).outbox_jsonl();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = fs::File::open(path)?;
    let mut out = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line?;
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        match serde_json::from_str::<BridgeOutboxMessage>(t) {
            Ok(m) => out.push(m),
            Err(e) => tracing::warn!("skip corrupt outbox line: {e}"),
        }
    }
    Ok(out)
}

pub fn load_outbox_last(data_dir: &Path, n: usize) -> Result<Vec<BridgeOutboxMessage>> {
    let all = load_outbox(data_dir)?;
    let start = all.len().saturating_sub(n);
    Ok(all[start..].to_vec())
}

pub fn format_outbox_line(m: &BridgeOutboxMessage) -> String {
    let session = m
        .session
        .as_deref()
        .map(|s| format!(" session={s}"))
        .unwrap_or_default();
    format!(
        "{} [{}] {} — {}{}",
        m.at.format("%Y-%m-%d %H:%M:%S"),
        m.kind,
        m.source,
        m.text,
        session
    )
}

/// Text ready for clipboard paste into Grok Build input (after review).
pub fn clipboard_payload_for_outbox(msg: &BridgeOutboxMessage) -> String {
    format!(
        "[WAGA → Grok Build]\nPark context: .waga/bridge/world.md\n\n{}\n",
        msg.text
    )
}

/// Merge inbox + outbox sorted by time (last `n` lines).
pub fn load_thread_last(data_dir: &Path, n: usize) -> Result<Vec<BridgeThreadLine>> {
    let mut lines = Vec::new();
    for m in load_outbox(data_dir)? {
        lines.push(BridgeThreadLine {
            at: m.at,
            direction: "→",
            kind: m.kind,
            source: m.source,
            text: m.text,
        });
    }
    for m in load_inbox(data_dir)? {
        lines.push(BridgeThreadLine {
            at: m.at,
            direction: "←",
            kind: m.kind,
            source: m.source,
            text: m.text,
        });
    }
    lines.sort_by(|a, b| a.at.cmp(&b.at));
    let start = lines.len().saturating_sub(n);
    Ok(lines[start..].to_vec())
}

/// Whether this inbox kind should be spoken (Slice 4).
pub fn inbox_kind_is_speakable(kind: &str) -> bool {
    matches!(
        kind.to_ascii_lowercase().as_str(),
        "blocked" | "reply" | "done"
    )
}

/// Spoken line for an inbox message, if any.
pub fn speak_text_for_inbox(m: &BridgeInboxMessage) -> Option<String> {
    if !inbox_kind_is_speakable(&m.kind) {
        return None;
    }
    let t = m.text.trim();
    if t.is_empty() {
        return None;
    }
    let kind = m.kind.to_ascii_lowercase();
    Some(match kind.as_str() {
        "blocked" => format!("Build blocked. {t}"),
        "reply" => format!("Build says. {t}"),
        "done" => format!("Build done. {t}"),
        _ => t.to_string(),
    })
}

fn read_spoken_cursor(data_dir: &Path) -> usize {
    let path = BridgePaths::new(data_dir).inbox_spoken_cursor();
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn write_spoken_cursor(data_dir: &Path, n: usize) -> Result<()> {
    let paths = BridgePaths::new(data_dir);
    paths.ensure()?;
    fs::write(paths.inbox_spoken_cursor(), format!("{n}"))?;
    Ok(())
}

/// Collect speakable lines from inbox messages not yet spoken; advance cursor.
/// Returns TTS strings. Call even with voice off to advance cursor if `advance` is true.
pub fn drain_speakable_inbox(data_dir: &Path, advance: bool) -> Result<Vec<String>> {
    let all = load_inbox(data_dir)?;
    let cursor = read_spoken_cursor(data_dir).min(all.len());
    let mut lines = Vec::new();
    for m in &all[cursor..] {
        if let Some(s) = speak_text_for_inbox(m) {
            lines.push(s);
        }
    }
    if advance {
        write_spoken_cursor(data_dir, all.len())?;
    }
    Ok(lines)
}

/// Mark all current inbox as already spoken (e.g. before first daemon start).
pub fn mark_inbox_spoken_up_to_date(data_dir: &Path) -> Result<()> {
    let n = load_inbox(data_dir)?.len();
    write_spoken_cursor(data_dir, n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run_tick;

    #[test]
    fn export_writes_md_and_json() {
        let dir = tempfile::tempdir().unwrap();
        run_tick(dir.path(), None, None).unwrap();
        let w = export_bridge(dir.path()).unwrap();
        assert!(w.tick >= 1);
        assert!(BridgePaths::new(dir.path()).world_md().is_file());
        assert!(BridgePaths::new(dir.path()).world_json().is_file());
        let md = fs::read_to_string(BridgePaths::new(dir.path()).world_md()).unwrap();
        assert!(md.contains("WAGA park"));
        assert!(md.contains("Blurb"));
    }

    #[test]
    fn inbox_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let msg = BridgeInboxMessage {
            at: Local::now(),
            source: "grok-build".into(),
            kind: "blocked".into(),
            text: "tests failed".into(),
            session: Some("sess-1".into()),
        };
        append_inbox(dir.path(), &msg).unwrap();
        let loaded = load_inbox(dir.path()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].kind, "blocked");
        assert_eq!(loaded[0].text, "tests failed");
    }

    #[test]
    fn outbox_and_thread() {
        let dir = tempfile::tempdir().unwrap();
        run_tick(dir.path(), None, None).unwrap();
        post_outbox(dir.path(), "chat", "please fix the test", "user", None).unwrap();
        post_inbox(
            dir.path(),
            "reply",
            "looking at waga-world",
            "grok-build",
            None,
        )
        .unwrap();
        let out = load_outbox(dir.path()).unwrap();
        assert_eq!(out.len(), 1);
        let thread = load_thread_last(dir.path(), 10).unwrap();
        assert_eq!(thread.len(), 2);
        assert!(thread.iter().any(|l| l.direction == "→"));
        assert!(thread.iter().any(|l| l.direction == "←"));
    }

    #[test]
    fn speakable_blocked_and_cursor() {
        let dir = tempfile::tempdir().unwrap();
        post_inbox(dir.path(), "note", "silent", "grok-build", None).unwrap();
        post_inbox(dir.path(), "blocked", "tests failed", "grok-build", None).unwrap();
        let lines = drain_speakable_inbox(dir.path(), true).unwrap();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("Build blocked"));
        // Second drain empty
        let again = drain_speakable_inbox(dir.path(), true).unwrap();
        assert!(again.is_empty());
    }

    #[test]
    fn clipboard_payload_contains_text() {
        let m = BridgeOutboxMessage {
            at: Local::now(),
            source: "user".into(),
            kind: "chat".into(),
            text: "hello build".into(),
            session: None,
        };
        let p = clipboard_payload_for_outbox(&m);
        assert!(p.contains("hello build"));
        assert!(p.contains("world.md"));
    }
}

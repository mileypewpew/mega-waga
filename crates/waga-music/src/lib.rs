//! HumanMusic — world-steered live bed (SuperCollider first; Lyria later).

use rosc::{OscMessage, OscPacket, OscType};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use waga_core::{Event, EventBody, WorldSnapshot};

#[derive(Debug, thiserror::Error)]
pub enum MusicError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("OSC: {0}")]
    Osc(String),
    #[error("SuperCollider not available: {0}")]
    SuperCollider(String),
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, MusicError>;

/// Continuous control parameters for the live bed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MusicParams {
    /// 0.0 calm → 1.0 intense
    pub tension: f32,
    pub bpm: f32,
    /// Free-form mood tag for display / future Lyria prompts
    pub mood: String,
    pub story_open: bool,
    pub git_dirty: bool,
    /// Short label for now-playing of Waga bed
    pub title: String,
}

impl Default for MusicParams {
    fn default() -> Self {
        Self {
            tension: 0.2,
            bpm: 90.0,
            mood: "idle".into(),
            story_open: false,
            git_dirty: false,
            title: "Waga HumanMusic Bed".into(),
        }
    }
}

/// Persistent music session under data dir.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicSession {
    pub bed_running: bool,
    pub backend: String,
    pub params: MusicParams,
    pub sc_host: String,
    pub sc_port: u16,
    pub sc_pid: Option<u32>,
    pub last_error: Option<String>,
}

impl Default for MusicSession {
    fn default() -> Self {
        Self {
            bed_running: false,
            backend: "supercollider".into(),
            params: MusicParams::default(),
            sc_host: "127.0.0.1".into(),
            sc_port: 57120,
            sc_pid: None,
            last_error: None,
        }
    }
}

impl MusicSession {
    pub fn path(data_dir: &Path) -> PathBuf {
        data_dir.join("music_session.json")
    }

    pub fn load(data_dir: &Path) -> Result<Self> {
        let p = Self::path(data_dir);
        if !p.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(p)?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn save(&self, data_dir: &Path) -> Result<()> {
        fs::create_dir_all(data_dir)?;
        fs::write(Self::path(data_dir), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

/// Derive music params from world projection + recent events.
pub fn direct_from_world(snapshot: &WorldSnapshot, recent: &[Event]) -> MusicParams {
    let mut p = MusicParams::default();
    p.git_dirty = snapshot.git.as_ref().map(|g| g.dirty).unwrap_or(false);

    // Recent events override / enrich
    for e in recent {
        match &e.body {
            EventBody::StoryOpened { title, .. } => {
                p.story_open = true;
                p.mood = "tension".into();
                p.title = format!("Quest: {title}");
            }
            EventBody::StoryClosed { summary, .. } => {
                p.story_open = false;
                p.mood = "resolve".into();
                p.title = format!("Resolved: {summary}");
            }
            EventBody::XpGranted { skill_id, .. } => {
                p.mood = "growth".into();
                p.title = format!("Growth · {skill_id}");
            }
            EventBody::GitObserved { dirty, branch, .. } => {
                p.git_dirty = *dirty;
                if *dirty {
                    p.mood = "focus".into();
                    p.title = format!("Dirty tree · {branch}");
                }
            }
            EventBody::PetMoodChanged { to, .. } => {
                if to == "grumpy" {
                    p.mood = "grumpy".into();
                } else if to == "content" {
                    p.mood = "content".into();
                }
            }
            _ => {}
        }
    }

    // If stories file not in events but git dirty
    if p.git_dirty {
        p.tension = p.tension.max(0.65);
        p.bpm = 118.0;
        if p.mood == "idle" {
            p.mood = "focus".into();
        }
    } else if p.story_open {
        p.tension = 0.75;
        p.bpm = 124.0;
    } else if p.mood == "resolve" || p.mood == "growth" {
        p.tension = 0.35;
        p.bpm = 100.0;
    } else if p.mood == "content" {
        p.tension = 0.25;
        p.bpm = 92.0;
    } else {
        p.tension = 0.2;
        p.bpm = 88.0;
    }

    p.tension = p.tension.clamp(0.0, 1.0);
    p.bpm = p.bpm.clamp(60.0, 160.0);
    p
}

/// Send full param bundle to SuperCollider via OSC.
pub fn osc_steer(session: &MusicSession, params: &MusicParams) -> Result<()> {
    let addr = format!("{}:{}", session.sc_host, session.sc_port);
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    sock.connect(&addr)
        .map_err(|e| MusicError::Osc(format!("connect {addr}: {e}")))?;

    let msgs = [
        ("/waga/tension", vec![OscType::Float(params.tension)]),
        ("/waga/bpm", vec![OscType::Float(params.bpm)]),
        ("/waga/mood", vec![OscType::String(params.mood.clone())]),
        (
            "/waga/story_open",
            vec![OscType::Int(if params.story_open { 1 } else { 0 })],
        ),
        (
            "/waga/git_dirty",
            vec![OscType::Int(if params.git_dirty { 1 } else { 0 })],
        ),
        ("/waga/title", vec![OscType::String(params.title.clone())]),
        ("/waga/gate", vec![OscType::Int(1)]),
    ];

    for (addr_pat, args) in msgs {
        let msg = OscMessage {
            addr: addr_pat.into(),
            args,
        };
        let packet = OscPacket::Message(msg);
        let buf =
            rosc::encoder::encode(&packet).map_err(|e| MusicError::Osc(e.to_string()))?;
        sock.send(&buf)
            .map_err(|e| MusicError::Osc(format!("send {addr_pat}: {e}")))?;
    }
    Ok(())
}

fn osc_gate(session: &MusicSession, on: bool) -> Result<()> {
    let addr = format!("{}:{}", session.sc_host, session.sc_port);
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    sock.connect(&addr)
        .map_err(|e| MusicError::Osc(format!("connect {addr}: {e}")))?;
    let msg = OscMessage {
        addr: "/waga/gate".into(),
        args: vec![OscType::Int(if on { 1 } else { 0 })],
    };
    let buf = rosc::encoder::encode(&OscPacket::Message(msg))
        .map_err(|e| MusicError::Osc(e.to_string()))?;
    sock.send(&buf)?;
    Ok(())
}

/// Locate SuperCollider bed script (repo assets or data dir copy).
pub fn find_sc_script(data_dir: &Path) -> Option<PathBuf> {
    let candidates = [
        data_dir.join("waga_bed.scd"),
        PathBuf::from("assets/sc/waga_bed.scd"),
        PathBuf::from("crates/waga-music/assets/waga_bed.scd"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

/// Start sclang with the bed script if SuperCollider is installed.
pub fn start_supercollider_bed(script: &Path) -> Result<Child> {
    let sclang = which("sclang").ok_or_else(|| {
        MusicError::SuperCollider(
            "sclang not found — install SuperCollider, or run the .scd manually".into(),
        )
    })?;
    let child = Command::new(sclang)
        .arg(script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| MusicError::SuperCollider(e.to_string()))?;
    Ok(child)
}

fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let p = dir.join(bin);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Ensure bed is marked running; try OSC gate + optional sclang spawn.
pub fn bed_start(data_dir: &Path, params: MusicParams) -> Result<MusicSession> {
    let mut session = MusicSession::load(data_dir).unwrap_or_default();
    session.params = params;
    session.backend = "supercollider".into();
    session.last_error = None;

    // Try start sclang if not already
    if session.sc_pid.is_none() {
        if let Some(script) = find_sc_script(data_dir) {
            match start_supercollider_bed(&script) {
                Ok(child) => {
                    session.sc_pid = Some(child.id());
                    // Detach: leak handle intentionally so process keeps running
                    std::mem::forget(child);
                    // Give sclang a moment
                    std::thread::sleep(std::time::Duration::from_millis(800));
                }
                Err(e) => {
                    session.last_error = Some(e.to_string());
                    tracing::warn!("sclang start: {e}");
                }
            }
        } else {
            session.last_error = Some(
                "waga_bed.scd not found — copy assets/sc/waga_bed.scd into .waga/ or run from repo root"
                    .into(),
            );
        }
    }

    // Always try OSC (user may have sclang already open)
    match osc_steer(&session, &session.params) {
        Ok(()) => {
            session.bed_running = true;
        }
        Err(e) => {
            session.last_error = Some(format!(
                "OSC steer failed ({e}). Open SuperCollider and run waga_bed.scd, then `waga music bed steer`"
            ));
            // Still mark intent running so UI shows params
            session.bed_running = true;
        }
    }
    session.save(data_dir)?;
    Ok(session)
}

pub fn bed_stop(data_dir: &Path) -> Result<MusicSession> {
    let mut session = MusicSession::load(data_dir).unwrap_or_default();
    let _ = osc_gate(&session, false);
    session.bed_running = false;
    session.save(data_dir)?;
    Ok(session)
}

pub fn bed_steer(data_dir: &Path, params: MusicParams) -> Result<MusicSession> {
    let mut session = MusicSession::load(data_dir).unwrap_or_default();
    session.params = params;
    match osc_steer(&session, &session.params) {
        Ok(()) => {
            session.last_error = None;
            session.bed_running = true;
        }
        Err(e) => session.last_error = Some(e.to_string()),
    }
    session.save(data_dir)?;
    Ok(session)
}

/// Apply director after a tick (if bed is running).
pub fn on_tick_music(data_dir: &Path, snapshot: &WorldSnapshot, new_events: &[Event]) -> Result<()> {
    let session = MusicSession::load(data_dir)?;
    if !session.bed_running {
        return Ok(());
    }
    let params = direct_from_world(snapshot, new_events);
    bed_steer(data_dir, params)?;
    Ok(())
}

pub fn format_music_status(session: &MusicSession) -> String {
    let p = &session.params;
    format!(
        "♫ HumanMusic bed\n  running: {}\n  backend: {}\n  title:   {}\n  mood:    {}\n  tension: {:.2}\n  bpm:     {:.0}\n  story:   {}\n  dirty:   {}\n  osc:     {}:{}\n  note:    {}\n  (Lyria RealTime = future backend on same MusicDirector)",
        session.bed_running,
        session.backend,
        p.title,
        p.mood,
        p.tension,
        p.bpm,
        p.story_open,
        p.git_dirty,
        session.sc_host,
        session.sc_port,
        session
            .last_error
            .as_deref()
            .unwrap_or("ok"),
    )
}

pub fn waga_bed_line(session: &MusicSession) -> String {
    if !session.bed_running {
        return "♫ bed: off (waga music bed start)".into();
    }
    format!(
        "♫ bed: {} · {} · t={:.2} · {}bpm",
        session.params.title, session.params.mood, session.params.tension, session.params.bpm as i32
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use waga_core::{EventId, GitStatus};

    #[test]
    fn dirty_raises_tension() {
        let mut snap = WorldSnapshot::fresh("strict-cto");
        snap.git = Some(GitStatus {
            repo_path: ".".into(),
            branch: "main".into(),
            dirty: true,
        });
        let p = direct_from_world(&snap, &[]);
        assert!(p.tension >= 0.6);
        assert!(p.git_dirty);
    }

    #[test]
    fn story_open_event_sets_mood() {
        let snap = WorldSnapshot::fresh("p");
        let ev = Event {
            id: EventId("e".into()),
            tick: 1,
            at: snap.observed_at,
            actor: "s".into(),
            links: vec![],
            body: EventBody::StoryOpened {
                story_id: waga_core::StoryId("s".into()),
                title: "Working tree dirty on main".into(),
            },
        };
        let p = direct_from_world(&snap, &[ev]);
        assert!(p.story_open);
        assert_eq!(p.mood, "tension");
    }

    #[test]
    fn session_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = MusicSession::default();
        s.params.bpm = 111.0;
        s.save(dir.path()).unwrap();
        let loaded = MusicSession::load(dir.path()).unwrap();
        assert_eq!(loaded.params.bpm, 111.0);
    }
}

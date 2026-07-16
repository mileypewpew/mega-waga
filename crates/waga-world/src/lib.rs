//! World Engine: observe the park, advance ticks, persist truth.

use chrono::Local;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use waga_character::{strict_cto_builtin, Persona};
use waga_core::{
    iana_timezone_or_offset, GitStatus, NarrativeEntry, Result, TickResult, WorldSnapshot,
};
use waga_pet::{mood_from_snapshot, PetMood};

/// Paths under a data directory (default `.waga`).
pub struct DataPaths {
    pub root: PathBuf,
}

impl DataPaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn world_json(&self) -> PathBuf {
        self.root.join("world.json")
    }

    pub fn narrative_jsonl(&self) -> PathBuf {
        self.root.join("narrative.jsonl")
    }

    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        Ok(())
    }
}

/// Load snapshot or create a fresh one.
pub fn load_snapshot(paths: &DataPaths, default_persona: &str) -> Result<WorldSnapshot> {
    let path = paths.world_json();
    if !path.exists() {
        return Ok(WorldSnapshot::fresh(default_persona));
    }
    let text = fs::read_to_string(&path)?;
    match serde_json::from_str(&text) {
        Ok(s) => Ok(s),
        Err(e) => {
            tracing::warn!("corrupt world.json ({e}); starting fresh");
            let bak = paths.root.join(format!(
                "world.corrupt.{}.json",
                Local::now().format("%Y%m%d%H%M%S")
            ));
            let _ = fs::rename(&path, &bak);
            Ok(WorldSnapshot::fresh(default_persona))
        }
    }
}

/// Save the current world snapshot.
pub fn save_snapshot(paths: &DataPaths, snapshot: &WorldSnapshot) -> Result<()> {
    paths.ensure()?;
    let text = serde_json::to_string_pretty(snapshot)?;
    fs::write(paths.world_json(), text)?;
    Ok(())
}

/// Append one narrative line.
pub fn append_narrative(paths: &DataPaths, entry: &NarrativeEntry) -> Result<()> {
    paths.ensure()?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.narrative_jsonl())?;
    writeln!(file, "{}", serde_json::to_string(entry)?)?;
    Ok(())
}

/// Observe clock (always succeeds).
pub fn observe_clock() -> (chrono::DateTime<Local>, String) {
    (Local::now(), iana_timezone_or_offset())
}

/// Observe git via the `git` CLI (local-only, no network).
///
/// Uses porcelain status so dirty includes staged, unstaged, and untracked.
pub fn observe_git(repo_hint: Option<&Path>) -> Option<GitStatus> {
    let start = repo_hint
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let top = run_git(&start, &["rev-parse", "--show-toplevel"])?;
    let repo_path = PathBuf::from(top.trim());
    if !repo_path.is_dir() {
        return None;
    }

    let branch = run_git(&repo_path, &["rev-parse", "--abbrev-ref", "HEAD"])
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "HEAD".into());

    let porcelain = run_git(&repo_path, &["status", "--porcelain"]).unwrap_or_default();
    let dirty = porcelain.lines().any(|l| !l.trim().is_empty());

    Some(GitStatus {
        repo_path,
        branch,
        dirty,
    })
}

fn run_git(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// Resolve persona from an optional file path, else built-in Strict CTO.
pub fn load_persona(persona_path: Option<&Path>) -> Result<Persona> {
    match persona_path {
        Some(p) => Persona::load(p),
        None => Ok(strict_cto_builtin()),
    }
}

/// Full tick: sensors → merge → notice → pet mood → persist → narrative.
pub fn run_tick(
    data_dir: impl AsRef<Path>,
    persona_path: Option<&Path>,
    repo_hint: Option<&Path>,
) -> Result<TickResult> {
    let paths = DataPaths::new(data_dir.as_ref());
    paths.ensure()?;

    let persona = load_persona(persona_path)?;
    let mut snapshot = load_snapshot(&paths, &persona.id)?;

    let (now, tz) = observe_clock();
    let git = observe_git(repo_hint);

    snapshot.tick = snapshot.tick.saturating_add(1);
    snapshot.observed_at = now;
    snapshot.timezone = tz;
    snapshot.git = git;
    snapshot.active_persona = persona.id.clone();

    let notice = persona.notice(&snapshot);
    let mood = mood_from_snapshot(&snapshot);
    snapshot.story.last_beat = format!("[{}] {notice}", mood.as_str());

    let entry = NarrativeEntry {
        tick: snapshot.tick,
        at: snapshot.observed_at,
        persona: persona.id.clone(),
        git_dirty: snapshot.git.as_ref().map(|g| g.dirty),
        notice: notice.clone(),
        pet_mood: mood.as_str().to_string(),
    };

    save_snapshot(&paths, &snapshot)?;
    append_narrative(&paths, &entry)?;

    Ok(TickResult {
        snapshot,
        notice,
        pet_mood: mood.as_str().to_string(),
    })
}

/// Re-read last snapshot without advancing (for pet UI initial draw).
pub fn peek_snapshot(data_dir: impl AsRef<Path>, default_persona: &str) -> Result<WorldSnapshot> {
    let paths = DataPaths::new(data_dir.as_ref());
    load_snapshot(&paths, default_persona)
}

/// Convenience: mood label from an existing snapshot.
pub fn mood_label(snapshot: &WorldSnapshot) -> PetMood {
    mood_from_snapshot(snapshot)
}

/// Format a human-readable tick summary for CLI output.
pub fn format_tick_summary(result: &TickResult) -> String {
    let s = &result.snapshot;
    let git_line = match &s.git {
        Some(g) => format!(
            "git: {} @ {} ({})",
            g.branch,
            g.repo_path.display(),
            if g.dirty { "dirty" } else { "clean" }
        ),
        None => "git: (none)".into(),
    };
    format!(
        "tick {} | {} | persona={} | mood={}\n{}\nnotice: {}",
        s.tick,
        s.observed_at.format("%Y-%m-%d %H:%M:%S"),
        s.active_persona,
        result.pet_mood,
        git_line,
        result.notice
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_increments_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let r1 = run_tick(dir.path(), None, None).unwrap();
        assert_eq!(r1.snapshot.tick, 1);
        let r2 = run_tick(dir.path(), None, None).unwrap();
        assert_eq!(r2.snapshot.tick, 2);

        let loaded = peek_snapshot(dir.path(), "strict-cto").unwrap();
        assert_eq!(loaded.tick, 2);

        let log = fs::read_to_string(dir.path().join("narrative.jsonl")).unwrap();
        assert_eq!(log.lines().count(), 2);
    }

    #[test]
    fn corrupt_snapshot_starts_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(dir.path());
        paths.ensure().unwrap();
        fs::write(paths.world_json(), "{not json").unwrap();
        let r = run_tick(dir.path(), None, None).unwrap();
        assert_eq!(r.snapshot.tick, 1);
    }

    #[test]
    fn git_sensor_on_this_repo_if_available() {
        // Running inside a git worktree may or may not apply; just ensure no panic.
        let status = observe_git(Some(Path::new(".")));
        // Either Some or None is fine; type shape is what matters.
        if let Some(g) = status {
            assert!(!g.branch.is_empty());
        }
    }
}

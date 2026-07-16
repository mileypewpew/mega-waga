//! World Engine: sensors + event-backed tick (event log is ground truth).

use chrono::Local;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use waga_character::{strict_cto_builtin, Persona};
use waga_core::{
    iana_timezone_or_offset, Event, EventBody, EventLink, GitStatus, LinkRel, Result, TickResult,
    WorldSnapshot,
};
use waga_events::{
    apply_git_story_rules, link_members_to_open_story, make_event, project_world, EventLog,
    GitStoryInput, StoryStore,
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

    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        Ok(())
    }
}

/// Write projection cache only (disposable).
pub fn save_snapshot(paths: &DataPaths, snapshot: &WorldSnapshot) -> Result<()> {
    paths.ensure()?;
    let text = serde_json::to_string_pretty(snapshot)?;
    fs::write(paths.world_json(), text)?;
    Ok(())
}

/// Observe clock (always succeeds).
pub fn observe_clock() -> (chrono::DateTime<Local>, String) {
    (Local::now(), iana_timezone_or_offset())
}

/// Observe git via the `git` CLI (local-only, no network).
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

/// Full tick: sensors → events → story rules → project → cache world.json.
pub fn run_tick(
    data_dir: impl AsRef<Path>,
    persona_path: Option<&Path>,
    repo_hint: Option<&Path>,
) -> Result<TickResult> {
    let root = data_dir.as_ref();
    let paths = DataPaths::new(root);
    paths.ensure()?;

    let log = EventLog::open(root)?;
    let mut story_store = StoryStore::load(root)?;
    let history = log.load_all()?;

    let persona = load_persona(persona_path)?;
    let w0 = project_world(&history, &persona.id);
    let prev_mood = mood_from_snapshot(&w0);
    let prev_dirty = w0.git.as_ref().map(|g| g.dirty);

    let tick = w0.tick.saturating_add(1);
    let (now, tz) = observe_clock();
    let git = observe_git(repo_hint);

    let mut tick_ev = make_event(tick, "system", EventBody::TickStarted);
    tick_ev.at = now;

    let mut clock_ev = make_event(
        tick,
        "sensor:clock",
        EventBody::ClockObserved {
            timezone: tz.clone(),
        },
    );
    clock_ev.at = now;
    clock_ev.links.push(EventLink {
        rel: LinkRel::Follows,
        to: tick_ev.id.clone(),
    });

    let mut batch: Vec<Event> = vec![tick_ev.clone(), clock_ev];

    let mut git_event = git.as_ref().map(|g| {
        let mut e = make_event(
            tick,
            "sensor:git",
            EventBody::GitObserved {
                repo_path: g.repo_path.display().to_string(),
                branch: g.branch.clone(),
                dirty: g.dirty,
            },
        );
        e.at = now;
        e.links.push(EventLink {
            rel: LinkRel::Follows,
            to: tick_ev.id.clone(),
        });
        e
    });

    // Intermediate projection for persona notice (git + tick).
    let mut interim = w0.clone();
    interim.tick = tick;
    interim.observed_at = now;
    interim.timezone = tz;
    interim.git = git.clone();
    interim.active_persona = persona.id.clone();
    let notice = persona.notice(&interim);
    interim.story.last_beat = notice.clone();

    let mut persona_ev = make_event(
        tick,
        format!("persona:{}", persona.id),
        EventBody::PersonaNoticed {
            persona_id: persona.id.clone(),
            notice: notice.clone(),
        },
    );
    persona_ev.at = now;
    let follow_target = git_event
        .as_ref()
        .map(|e| e.id.clone())
        .unwrap_or_else(|| tick_ev.id.clone());
    persona_ev.links.push(EventLink {
        rel: LinkRel::Follows,
        to: follow_target,
    });

    let new_mood = mood_from_snapshot(&interim);
    let mood_ev = if prev_mood != new_mood {
        let mut e = make_event(
            tick,
            "system",
            EventBody::PetMoodChanged {
                from: prev_mood.as_str().into(),
                to: new_mood.as_str().into(),
            },
        );
        e.at = now;
        e.links.push(EventLink {
            rel: LinkRel::Follows,
            to: persona_ev.id.clone(),
        });
        Some(e)
    } else {
        None
    };

    // Story rules on git event
    let mut story_extras = Vec::new();
    if let Some(ref mut ge) = git_event {
        let member_ids: Vec<_> = std::iter::once(persona_ev.id.clone())
            .chain(mood_ev.as_ref().map(|e| e.id.clone()))
            .collect();
        story_extras = apply_git_story_rules(
            &mut story_store,
            GitStoryInput {
                prev_dirty,
                git_event: ge,
                member_ids: &member_ids,
                tick,
                at: now,
            },
        )?;
    }

    if let Some(ge) = git_event {
        batch.push(ge);
    }
    batch.push(persona_ev);
    if let Some(me) = mood_ev {
        batch.push(me);
    }

    // Link persona/mood into open story if still open after rules
    link_members_to_open_story(&mut story_store, &mut batch);
    batch.extend(story_extras);

    let new_event_ids: Vec<_> = batch.iter().map(|e| e.id.clone()).collect();
    log.append(&batch)?;
    story_store.save()?;

    let mut all = history;
    all.extend(batch);
    let snapshot = project_world(&all, &persona.id);
    // Prefer notice we just emitted for last_beat clarity
    let mut snapshot = snapshot;
    if snapshot.story.last_beat.is_empty() {
        snapshot.story.last_beat = notice.clone();
    }
    save_snapshot(&paths, &snapshot)?;

    Ok(TickResult {
        snapshot,
        notice,
        pet_mood: new_mood.as_str().to_string(),
        new_event_ids,
    })
}

/// Project world from event log (falls back to empty if no events).
/// If world.json exists and events empty, still prefer events (empty → fresh).
pub fn peek_snapshot(data_dir: impl AsRef<Path>, default_persona: &str) -> Result<WorldSnapshot> {
    let root = data_dir.as_ref();
    let log = EventLog::open(root)?;
    let events = log.load_all()?;
    if events.is_empty() {
        // Optional: read cache for display-only before first tick
        let paths = DataPaths::new(root);
        if paths.world_json().exists() {
            if let Ok(text) = fs::read_to_string(paths.world_json()) {
                if let Ok(s) = serde_json::from_str(&text) {
                    return Ok(s);
                }
            }
        }
        return Ok(WorldSnapshot::fresh(default_persona));
    }
    Ok(project_world(&events, default_persona))
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
        "tick {} | {} | persona={} | mood={} | events+{}\n{}\nnotice: {}",
        s.tick,
        s.observed_at.format("%Y-%m-%d %H:%M:%S"),
        s.active_persona,
        result.pet_mood,
        result.new_event_ids.len(),
        git_line,
        result.notice
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_increments_and_writes_events() {
        let dir = tempfile::tempdir().unwrap();
        let r1 = run_tick(dir.path(), None, None).unwrap();
        assert_eq!(r1.snapshot.tick, 1);
        assert!(!r1.new_event_ids.is_empty());
        let r2 = run_tick(dir.path(), None, None).unwrap();
        assert_eq!(r2.snapshot.tick, 2);

        let loaded = peek_snapshot(dir.path(), "strict-cto").unwrap();
        assert_eq!(loaded.tick, 2);

        let log = fs::read_to_string(dir.path().join("events.jsonl")).unwrap();
        assert!(log.lines().count() >= 2);
        // narrative.jsonl must not be required
        assert!(!dir.path().join("narrative.jsonl").exists());
    }

    #[test]
    fn rebuild_without_world_json() {
        let dir = tempfile::tempdir().unwrap();
        run_tick(dir.path(), None, None).unwrap();
        run_tick(dir.path(), None, None).unwrap();
        let before = peek_snapshot(dir.path(), "strict-cto").unwrap();
        fs::remove_file(dir.path().join("world.json")).unwrap();
        let after = peek_snapshot(dir.path(), "strict-cto").unwrap();
        assert_eq!(before.tick, after.tick);
        assert_eq!(before.active_persona, after.active_persona);
    }

    #[test]
    fn corrupt_cache_ignored_when_events_exist() {
        let dir = tempfile::tempdir().unwrap();
        run_tick(dir.path(), None, None).unwrap();
        fs::write(dir.path().join("world.json"), "{not json").unwrap();
        let s = peek_snapshot(dir.path(), "strict-cto").unwrap();
        assert_eq!(s.tick, 1);
    }

    #[test]
    fn git_sensor_on_this_repo_if_available() {
        let status = observe_git(Some(Path::new(".")));
        if let Some(g) = status {
            assert!(!g.branch.is_empty());
        }
    }
}

//! Classified memories + park-level XP, paired and event-cited.

use chrono::Local;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use waga_core::{
    level_from_xp, Event, EventBody, EventLink, LinkRel, Memory, MemoryClass, MemoryId,
    MemoryScope, MemorySource, Result, SkillState, StoryId, XpBeneficiary,
};
use waga_events::{make_event, EventLog};

pub const SKILL_REPO_HYGIENE: &str = "repo_hygiene";
pub const SKILL_PRESENCE: &str = "presence";

const XP_STORY_CLOSE_HYGIENE: u32 = 10;
const XP_MOOD_PRESENCE: u32 = 2;

/// Allocate a memory id.
pub fn new_memory_id() -> MemoryId {
    MemoryId(format!("mem_{}", uuid::Uuid::new_v4().as_simple()))
}

/// Memory index (JSONL), rebuildable from events.
pub struct MemoryStore {
    pub root: PathBuf,
    pub memories: Vec<Memory>,
}

impl MemoryStore {
    pub fn load(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        let path = root.join("memories.jsonl");
        let mut memories = Vec::new();
        if path.exists() {
            let file = fs::File::open(&path)?;
            for line in BufReader::new(file).lines() {
                let line = line?;
                let t = line.trim();
                if t.is_empty() {
                    continue;
                }
                match serde_json::from_str::<Memory>(t) {
                    Ok(m) => memories.push(m),
                    Err(e) => tracing::warn!("skip corrupt memory line: {e}"),
                }
            }
        }
        Ok(Self { root, memories })
    }

    pub fn append(&mut self, memory: &Memory) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.root.join("memories.jsonl"))?;
        writeln!(file, "{}", serde_json::to_string(memory)?)?;
        self.memories.push(memory.clone());
        Ok(())
    }

    pub fn path(root: &Path) -> PathBuf {
        root.join("memories.jsonl")
    }
}

/// Park skill projection cache.
pub struct SkillStore {
    pub root: PathBuf,
    pub skills: BTreeMap<String, SkillState>,
}

impl SkillStore {
    pub fn load(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        let path = root.join("skills.json");
        let skills = if path.exists() {
            let text = fs::read_to_string(&path)?;
            if text.trim().is_empty() {
                BTreeMap::new()
            } else {
                serde_json::from_str(&text)?
            }
        } else {
            BTreeMap::new()
        };
        Ok(Self { root, skills })
    }

    pub fn save(&self) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        let text = serde_json::to_string_pretty(&self.skills)?;
        fs::write(self.root.join("skills.json"), text)?;
        Ok(())
    }

    pub fn apply_grant(&mut self, skill_id: &str, amount: u32) {
        let entry = self
            .skills
            .entry(skill_id.to_string())
            .or_insert_with(|| SkillState {
                skill_id: skill_id.to_string(),
                xp: 0,
                level: 0,
            });
        entry.xp = entry.xp.saturating_add(u64::from(amount));
        entry.level = level_from_xp(entry.xp);
    }

    /// Rebuild totals from event log only.
    pub fn rebuild_from_events(events: &[Event]) -> BTreeMap<String, SkillState> {
        let mut map = BTreeMap::new();
        for e in events {
            if let EventBody::XpGranted {
                skill_id, amount, ..
            } = &e.body
            {
                let entry = map.entry(skill_id.clone()).or_insert_with(|| SkillState {
                    skill_id: skill_id.clone(),
                    xp: 0,
                    level: 0,
                });
                entry.xp = entry.xp.saturating_add(u64::from(*amount));
                entry.level = level_from_xp(entry.xp);
            }
        }
        map
    }
}

/// Outcome of running memory+XP rules on a tick batch.
pub struct MemoryTickOutcome {
    pub events: Vec<Event>,
    pub memories_formed: Vec<Memory>,
}

/// Process new tick events: StoryClosed → episodic memory + repo_hygiene XP;
/// optional Affective on mood recovery.
pub fn process_new_events(new_events: &[Event], tick: u64) -> MemoryTickOutcome {
    let mut out_events = Vec::new();
    let mut memories = Vec::new();

    for e in new_events {
        match &e.body {
            EventBody::StoryClosed {
                story_id, summary, ..
            } => {
                let (mem, evs) = form_story_close_memory(e, story_id, summary, tick);
                memories.push(mem);
                out_events.extend(evs);
            }
            EventBody::PetMoodChanged { from, to } if from == "grumpy" && to == "content" => {
                let (mem, evs) = form_mood_memory(e, tick);
                memories.push(mem);
                out_events.extend(evs);
            }
            _ => {}
        }
    }

    MemoryTickOutcome {
        events: out_events,
        memories_formed: memories,
    }
}

fn form_story_close_memory(
    close_ev: &Event,
    story_id: &StoryId,
    summary: &str,
    tick: u64,
) -> (Memory, Vec<Event>) {
    let mem_id = new_memory_id();
    let title = if summary.is_empty() {
        "Closed a working-tree story".into()
    } else {
        summary.to_string()
    };
    let body = format!(
        "Park learned from completing a git working-tree arc: {title}"
    );
    let memory = Memory {
        id: mem_id.clone(),
        class: MemoryClass::Episodic,
        scope: MemoryScope::Park,
        title: title.clone(),
        body,
        tags: vec!["git".into(), "hygiene".into(), "story".into()],
        importance: 4,
        confidence: 0.9,
        source: MemorySource::SystemRule,
        source_event_ids: vec![close_ev.id.clone()],
        story_id: Some(story_id.clone()),
        formed_at_tick: tick,
        formed_at: Local::now(),
    };

    let mut mem_ev = make_event(
        tick,
        "system:memory",
        EventBody::MemoryFormed {
            memory_id: mem_id.clone(),
            class: MemoryClass::Episodic,
            title: title.clone(),
        },
    );
    mem_ev.links.push(EventLink {
        rel: LinkRel::Follows,
        to: close_ev.id.clone(),
    });
    // Carry story membership if close was part of story
    for l in &close_ev.links {
        if l.rel == LinkRel::PartOfStory {
            mem_ev.links.push(EventLink {
                rel: LinkRel::PartOfStory,
                to: l.to.clone(),
            });
        }
    }

    let mut xp_ev = make_event(
        tick,
        "system:xp",
        EventBody::XpGranted {
            skill_id: SKILL_REPO_HYGIENE.into(),
            amount: XP_STORY_CLOSE_HYGIENE,
            beneficiary: XpBeneficiary::Park,
            memory_id: Some(mem_id),
            reason: format!("story closed: {title}"),
        },
    );
    xp_ev.links.push(EventLink {
        rel: LinkRel::Follows,
        to: mem_ev.id.clone(),
    });

    (memory, vec![mem_ev, xp_ev])
}

fn form_mood_memory(mood_ev: &Event, tick: u64) -> (Memory, Vec<Event>) {
    let mem_id = new_memory_id();
    let title = "Pet recovered from grumpy to content".to_string();
    let memory = Memory {
        id: mem_id.clone(),
        class: MemoryClass::Affective,
        scope: MemoryScope::Park,
        title: title.clone(),
        body: "The park companion felt relief as conditions improved.".into(),
        tags: vec!["pet".into(), "mood".into()],
        importance: 2,
        confidence: 0.85,
        source: MemorySource::SystemRule,
        source_event_ids: vec![mood_ev.id.clone()],
        story_id: None,
        formed_at_tick: tick,
        formed_at: Local::now(),
    };

    let mut mem_ev = make_event(
        tick,
        "system:memory",
        EventBody::MemoryFormed {
            memory_id: mem_id.clone(),
            class: MemoryClass::Affective,
            title: title.clone(),
        },
    );
    mem_ev.links.push(EventLink {
        rel: LinkRel::Follows,
        to: mood_ev.id.clone(),
    });

    let mut xp_ev = make_event(
        tick,
        "system:xp",
        EventBody::XpGranted {
            skill_id: SKILL_PRESENCE.into(),
            amount: XP_MOOD_PRESENCE,
            beneficiary: XpBeneficiary::Park,
            memory_id: Some(mem_id),
            reason: title,
        },
    );
    xp_ev.links.push(EventLink {
        rel: LinkRel::Follows,
        to: mem_ev.id.clone(),
    });

    (memory, vec![mem_ev, xp_ev])
}

/// Persist memory index + skill cache after forming memories and XP events.
pub fn commit_memory_outcome(
    root: &Path,
    outcome: &MemoryTickOutcome,
) -> Result<()> {
    let mut mem_store = MemoryStore::load(root)?;
    let mut skill_store = SkillStore::load(root)?;

    for m in &outcome.memories_formed {
        mem_store.append(m)?;
    }
    for e in &outcome.events {
        if let EventBody::XpGranted {
            skill_id, amount, ..
        } = &e.body
        {
            skill_store.apply_grant(skill_id, *amount);
        }
    }
    skill_store.save()?;
    Ok(())
}

/// Load memories for CLI (from index, or rebuild hints from events if empty).
pub fn list_memories(root: &Path) -> Result<Vec<Memory>> {
    let store = MemoryStore::load(root)?;
    if !store.memories.is_empty() {
        return Ok(store.memories);
    }
    // Fallback: reconstruct thin memories from MemoryFormed events
    let log = EventLog::open(root)?;
    let events = log.load_all()?;
    let mut out = Vec::new();
    for e in events {
        if let EventBody::MemoryFormed {
            memory_id,
            class,
            title,
        } = &e.body
        {
            out.push(Memory {
                id: memory_id.clone(),
                class: *class,
                scope: MemoryScope::Park,
                title: title.clone(),
                body: title.clone(),
                tags: Vec::new(),
                importance: 3,
                confidence: 0.7,
                source: MemorySource::SystemRule,
                source_event_ids: vec![e.id.clone()],
                story_id: None,
                formed_at_tick: e.tick,
                formed_at: e.at,
            });
        }
    }
    Ok(out)
}

/// Load skill projection; rebuild from events if cache missing/empty but events have XP.
pub fn list_skills(root: &Path) -> Result<Vec<SkillState>> {
    let store = SkillStore::load(root)?;
    if !store.skills.is_empty() {
        return Ok(store.skills.values().cloned().collect());
    }
    let log = EventLog::open(root)?;
    let events = log.load_all()?;
    let map = SkillStore::rebuild_from_events(&events);
    Ok(map.into_values().collect())
}

pub fn format_memory_line(m: &Memory) -> String {
    format!(
        "{} [{}] \"{}\" tags={:?} tick={} importance={}",
        m.id,
        m.class,
        m.title,
        m.tags,
        m.formed_at_tick,
        m.importance
    )
}

pub fn format_skill_line(s: &SkillState) -> String {
    format!("{}  xp={}  level={}", s.skill_id, s.xp, s.level)
}

/// Most recent memories first (index is append order).
pub fn recent_memories(root: &Path, limit: usize) -> Result<Vec<Memory>> {
    let mut mems = list_memories(root)?;
    if limit == 0 || mems.is_empty() {
        return Ok(mems);
    }
    let start = mems.len().saturating_sub(limit);
    Ok(mems.split_off(start))
}

/// Compact skill strip for pet/status (e.g. `repo_hygiene L0/10xp`).
pub fn skills_summary_line(root: &Path) -> Result<String> {
    let mut skills = list_skills(root)?;
    if skills.is_empty() {
        return Ok("skills: (none yet)".into());
    }
    skills.sort_by(|a, b| a.skill_id.cmp(&b.skill_id));
    let parts: Vec<String> = skills
        .iter()
        .map(|s| format!("{} L{}/{}xp", s.skill_id, s.level, s.xp))
        .collect();
    Ok(format!("skills: {}", parts.join(" · ")))
}

/// One-line last memory for pet footer.
pub fn last_memory_line(root: &Path) -> Result<String> {
    let mems = recent_memories(root, 1)?;
    match mems.last() {
        Some(m) => Ok(format!("memory: [{}] {}", m.class, m.title)),
        None => Ok("memory: (none — dirty→clean a tree to learn)".into()),
    }
}

/// Full multi-line park status for `waga status`.
pub fn format_park_status(
    root: &Path,
    snapshot: &waga_core::WorldSnapshot,
    open_story_title: Option<&str>,
) -> Result<String> {
    let git = match &snapshot.git {
        Some(g) => format!(
            "{} @ {} ({})",
            g.branch,
            g.repo_path.display(),
            if g.dirty { "DIRTY" } else { "clean" }
        ),
        None => "(no git)".into(),
    };
    let story = open_story_title
        .map(|t| format!("OPEN \"{t}\""))
        .unwrap_or_else(|| "none open".into());
    let mem_line = last_memory_line(root)?;
    let skill_line = skills_summary_line(root)?;
    let recent = recent_memories(root, 3)?;
    let mut lines = vec![
        format!("tick {}  |  persona {}  |  {}", snapshot.tick, snapshot.active_persona, snapshot.timezone),
        format!("git: {git}"),
        format!("story: {story}"),
        mem_line,
        skill_line,
    ];
    if !recent.is_empty() {
        lines.push("recent memories:".into());
        for m in recent.iter().rev() {
            lines.push(format!("  - [{}] {}", m.class, m.title));
        }
    }
    if snapshot.git.as_ref().map(|g| !g.dirty).unwrap_or(true) {
        lines.push("hint: dirty the tree, tick, clean, tick → memory + XP".into());
    }
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use waga_core::StoryId;
    use waga_events::make_event;

    #[test]
    fn story_close_forms_memory_and_xp() {
        let close = make_event(
            2,
            "system",
            EventBody::StoryClosed {
                story_id: StoryId("sty_1".into()),
                summary: "Tree clean on main".into(),
            },
        );
        let out = process_new_events(&[close.clone()], 2);
        assert_eq!(out.memories_formed.len(), 1);
        assert_eq!(out.memories_formed[0].class, MemoryClass::Episodic);
        assert_eq!(out.events.len(), 2);
        assert!(matches!(
            out.events[0].body,
            EventBody::MemoryFormed { .. }
        ));
        match &out.events[1].body {
            EventBody::XpGranted {
                skill_id,
                amount,
                beneficiary,
                memory_id,
                ..
            } => {
                assert_eq!(skill_id, SKILL_REPO_HYGIENE);
                assert_eq!(*amount, 10);
                assert_eq!(*beneficiary, XpBeneficiary::Park);
                assert!(memory_id.is_some());
            }
            _ => panic!("expected XpGranted"),
        }
    }

    #[test]
    fn no_xp_without_story_close() {
        let dirty = make_event(
            1,
            "sensor:git",
            EventBody::GitObserved {
                repo_path: "/r".into(),
                branch: "main".into(),
                dirty: true,
            },
        );
        let out = process_new_events(&[dirty], 1);
        assert!(out.memories_formed.is_empty());
        assert!(out.events.is_empty());
    }

    #[test]
    fn skill_rebuild_from_events() {
        let dir = tempfile::tempdir().unwrap();
        let close = make_event(
            1,
            "system",
            EventBody::StoryClosed {
                story_id: StoryId("sty_x".into()),
                summary: "clean".into(),
            },
        );
        let out = process_new_events(&[close], 1);
        let log = EventLog::open(dir.path()).unwrap();
        log.append(&out.events).unwrap();
        commit_memory_outcome(dir.path(), &out).unwrap();

        let skills = list_skills(dir.path()).unwrap();
        let hygiene = skills.iter().find(|s| s.skill_id == SKILL_REPO_HYGIENE);
        assert!(hygiene.is_some());
        assert_eq!(hygiene.unwrap().xp, 10);

        // Drop cache; rebuild from events
        fs::remove_file(dir.path().join("skills.json")).unwrap();
        let rebuilt = SkillStore::rebuild_from_events(&log.load_all().unwrap());
        assert_eq!(rebuilt.get(SKILL_REPO_HYGIENE).unwrap().xp, 10);
    }

    #[test]
    fn recent_and_summary_helpers() {
        let dir = tempfile::tempdir().unwrap();
        let close = make_event(
            1,
            "system",
            EventBody::StoryClosed {
                story_id: StoryId("sty_z".into()),
                summary: "clean".into(),
            },
        );
        let out = process_new_events(&[close], 1);
        let log = EventLog::open(dir.path()).unwrap();
        log.append(&out.events).unwrap();
        commit_memory_outcome(dir.path(), &out).unwrap();
        let recent = recent_memories(dir.path(), 1).unwrap();
        assert_eq!(recent.len(), 1);
        let skills = skills_summary_line(dir.path()).unwrap();
        assert!(skills.contains("repo_hygiene"));
        let last = last_memory_line(dir.path()).unwrap();
        assert!(last.contains("episodic") || last.contains("Episodic") || last.contains('['));
    }

    #[test]
    fn mood_recovery_affective() {
        let mood = make_event(
            3,
            "system",
            EventBody::PetMoodChanged {
                from: "grumpy".into(),
                to: "content".into(),
            },
        );
        let out = process_new_events(&[mood], 3);
        assert_eq!(out.memories_formed[0].class, MemoryClass::Affective);
        match &out.events[1].body {
            EventBody::XpGranted { skill_id, amount, .. } => {
                assert_eq!(skill_id, SKILL_PRESENCE);
                assert_eq!(*amount, 2);
            }
            _ => panic!("xp"),
        }
    }
}

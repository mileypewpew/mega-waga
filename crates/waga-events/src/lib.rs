//! Event log spine: append-only ground truth, projection, stories.

use chrono::Local;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use waga_core::{
    Event, EventBody, EventId, EventLink, GitStatus, LinkRel, Result, Story, StoryId, StoryStatus,
    WorldSnapshot,
};

/// Allocate a new event id.
pub fn new_event_id() -> EventId {
    EventId(format!("evt_{}", uuid::Uuid::new_v4().as_simple()))
}

/// Allocate a new story id.
pub fn new_story_id() -> StoryId {
    StoryId(format!("sty_{}", uuid::Uuid::new_v4().as_simple()))
}

/// Append-only JSONL event log under a data directory.
pub struct EventLog {
    pub root: PathBuf,
}

impl EventLog {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn path(&self) -> PathBuf {
        self.root.join("events.jsonl")
    }

    /// Load all events; skip blank lines and a corrupt trailing line.
    pub fn load_all(&self) -> Result<Vec<Event>> {
        let path = self.path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = fs::File::open(&path)?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();
        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<Event>(trimmed) {
                Ok(e) => events.push(e),
                Err(err) => {
                    tracing::warn!("skipping corrupt event line: {err}");
                }
            }
        }
        Ok(events)
    }

    /// Append events (one JSON object per line).
    pub fn append(&self, events: &[Event]) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }
        fs::create_dir_all(&self.root)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path())?;
        for e in events {
            writeln!(file, "{}", serde_json::to_string(e)?)?;
        }
        file.flush()?;
        Ok(())
    }
}

/// Project park “what is now” from the full event stream.
pub fn project_world(events: &[Event], default_persona: &str) -> WorldSnapshot {
    let mut snap = WorldSnapshot::fresh(default_persona);
    if events.is_empty() {
        return snap;
    }

    for e in events {
        snap.observed_at = e.at;
        match &e.body {
            EventBody::TickStarted => {
                snap.tick = e.tick;
            }
            EventBody::ClockObserved { timezone } => {
                snap.timezone = timezone.clone();
                snap.tick = snap.tick.max(e.tick);
            }
            EventBody::GitObserved {
                repo_path,
                branch,
                dirty,
            } => {
                snap.git = Some(GitStatus {
                    repo_path: PathBuf::from(repo_path),
                    branch: branch.clone(),
                    dirty: *dirty,
                });
                snap.tick = snap.tick.max(e.tick);
            }
            EventBody::PersonaNoticed {
                persona_id,
                notice,
            } => {
                snap.active_persona = persona_id.clone();
                snap.story.last_beat = notice.clone();
                snap.tick = snap.tick.max(e.tick);
            }
            EventBody::StoryOpened { title, .. } => {
                snap.story.last_beat = format!("story opened: {title}");
                snap.tick = snap.tick.max(e.tick);
            }
            EventBody::StoryClosed { summary, .. } => {
                snap.story.last_beat = format!("story closed: {summary}");
                snap.tick = snap.tick.max(e.tick);
            }
            EventBody::PetMoodChanged { .. } => {
                snap.tick = snap.tick.max(e.tick);
            }
        }
    }

    // Ensure tick reflects max tick seen on any event.
    if let Some(max_tick) = events.iter().map(|e| e.tick).max() {
        snap.tick = snap.tick.max(max_tick);
    }
    snap
}

/// In-memory + disk story records.
pub struct StoryStore {
    pub root: PathBuf,
    pub stories: Vec<Story>,
}

impl StoryStore {
    pub fn load(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        let path = root.join("stories.json");
        let stories = if path.exists() {
            let text = fs::read_to_string(&path)?;
            if text.trim().is_empty() {
                Vec::new()
            } else {
                serde_json::from_str(&text)?
            }
        } else {
            Vec::new()
        };
        Ok(Self { root, stories })
    }

    pub fn save(&self) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        let text = serde_json::to_string_pretty(&self.stories)?;
        fs::write(self.root.join("stories.json"), text)?;
        Ok(())
    }

    pub fn open_git_story(&self) -> Option<&Story> {
        self.stories
            .iter()
            .find(|s| s.status == StoryStatus::Open)
    }

    pub fn open_git_story_mut(&mut self) -> Option<&mut Story> {
        self.stories
            .iter_mut()
            .find(|s| s.status == StoryStatus::Open)
    }
}

/// Context for applying git dirty/clean story rules.
pub struct GitStoryInput<'a> {
    pub prev_dirty: Option<bool>,
    pub git_event: &'a mut Event,
    pub member_ids: &'a [EventId],
    pub tick: u64,
    pub at: chrono::DateTime<Local>,
}

/// Apply auto story open/close. May push extra events and mutate store + git links.
pub fn apply_git_story_rules(
    store: &mut StoryStore,
    input: GitStoryInput<'_>,
) -> Result<Vec<Event>> {
    let EventBody::GitObserved {
        branch, dirty, ..
    } = &input.git_event.body
    else {
        return Ok(Vec::new());
    };
    let branch = branch.clone();
    let dirty = *dirty;
    let prev = input.prev_dirty;
    let mut extra = Vec::new();

    let became_dirty = dirty && (prev.is_none() || prev == Some(false));
    let became_clean = !dirty && prev == Some(true);

    if became_dirty && store.open_git_story().is_none() {
        let story_id = new_story_id();
        let open_id = new_event_id();
        let title = format!("Working tree dirty on {branch}");
        let open_ev = Event {
            id: open_id.clone(),
            tick: input.tick,
            at: input.at,
            actor: "system".into(),
            links: vec![EventLink {
                rel: LinkRel::Follows,
                to: input.git_event.id.clone(),
            }],
            body: EventBody::StoryOpened {
                story_id: story_id.clone(),
                title: title.clone(),
            },
        };
        input.git_event.links.push(EventLink {
            rel: LinkRel::PartOfStory,
            to: open_id.clone(),
        });
        let mut members = vec![open_id.clone(), input.git_event.id.clone()];
        for id in input.member_ids {
            if !members.contains(id) {
                members.push(id.clone());
            }
        }
        store.stories.push(Story {
            id: story_id,
            title,
            summary: String::new(),
            status: StoryStatus::Open,
            opened_at_tick: input.tick,
            closed_at_tick: None,
            anchor_event_id: open_id,
            member_event_ids: members,
        });
        extra.push(open_ev);
    } else if dirty {
        if let Some(story) = store.open_git_story_mut() {
            let anchor = story.anchor_event_id.clone();
            input.git_event.links.push(EventLink {
                rel: LinkRel::PartOfStory,
                to: anchor,
            });
            if !story.member_event_ids.contains(&input.git_event.id) {
                story.member_event_ids.push(input.git_event.id.clone());
            }
            for id in input.member_ids {
                if !story.member_event_ids.contains(id) {
                    story.member_event_ids.push(id.clone());
                }
            }
        }
    }

    if became_clean {
        if let Some(story) = store.open_git_story_mut() {
            let story_id = story.id.clone();
            let anchor = story.anchor_event_id.clone();
            let summary = format!("Tree clean on {branch}");
            let close_id = new_event_id();
            input.git_event.links.push(EventLink {
                rel: LinkRel::PartOfStory,
                to: anchor.clone(),
            });
            if !story.member_event_ids.contains(&input.git_event.id) {
                story.member_event_ids.push(input.git_event.id.clone());
            }
            story.member_event_ids.push(close_id.clone());
            story.status = StoryStatus::Closed;
            story.closed_at_tick = Some(input.tick);
            story.summary = summary.clone();
            extra.push(Event {
                id: close_id,
                tick: input.tick,
                at: input.at,
                actor: "system".into(),
                links: vec![
                    EventLink {
                        rel: LinkRel::PartOfStory,
                        to: anchor,
                    },
                    EventLink {
                        rel: LinkRel::Follows,
                        to: input.git_event.id.clone(),
                    },
                ],
                body: EventBody::StoryClosed { story_id, summary },
            });
        }
    }

    Ok(extra)
}

/// Attach PartOfStory links on events toward the open story anchor, if any.
pub fn link_members_to_open_story(store: &mut StoryStore, events: &mut [Event]) {
    let Some(anchor) = store
        .open_git_story()
        .map(|s| s.anchor_event_id.clone())
    else {
        return;
    };
    let open = store.open_git_story_mut().unwrap();
    for e in events.iter_mut() {
        if matches!(
            e.body,
            EventBody::PersonaNoticed { .. } | EventBody::PetMoodChanged { .. }
        ) {
            if !e.links.iter().any(|l| l.rel == LinkRel::PartOfStory) {
                e.links.push(EventLink {
                    rel: LinkRel::PartOfStory,
                    to: anchor.clone(),
                });
            }
            if !open.member_event_ids.contains(&e.id) {
                open.member_event_ids.push(e.id.clone());
            }
        }
    }
}

/// Helper to build a skeleton event.
pub fn make_event(tick: u64, actor: impl Into<String>, body: EventBody) -> Event {
    Event {
        id: new_event_id(),
        tick,
        at: Local::now(),
        actor: actor.into(),
        links: Vec::new(),
        body,
    }
}

/// Format one event for CLI listing.
pub fn format_event_line(e: &Event) -> String {
    let kind = match &e.body {
        EventBody::TickStarted => "TickStarted".into(),
        EventBody::ClockObserved { timezone } => format!("ClockObserved tz={timezone}"),
        EventBody::GitObserved {
            branch, dirty, ..
        } => format!(
            "GitObserved branch={branch} dirty={dirty}"
        ),
        EventBody::PersonaNoticed {
            persona_id,
            notice,
        } => format!("PersonaNoticed {persona_id}: {notice}"),
        EventBody::PetMoodChanged { from, to } => {
            format!("PetMoodChanged {from}->{to}")
        }
        EventBody::StoryOpened { story_id, title } => {
            format!("StoryOpened {story_id} \"{title}\"")
        }
        EventBody::StoryClosed { story_id, summary } => {
            format!("StoryClosed {story_id} \"{summary}\"")
        }
    };
    let links: Vec<String> = e
        .links
        .iter()
        .map(|l| {
            let rel = match l.rel {
                LinkRel::CausedBy => "caused_by",
                LinkRel::Follows => "follows",
                LinkRel::RefersTo => "refers_to",
                LinkRel::PartOfStory => "part_of_story",
            };
            format!("{rel}:{}", l.to)
        })
        .collect();
    let link_s = if links.is_empty() {
        String::new()
    } else {
        format!(" links=[{}]", links.join(", "))
    };
    format!("{} tick={} {}{link_s}", e.id, e.tick, kind)
}

/// Format one story for CLI listing.
pub fn format_story_line(s: &Story) -> String {
    let status = match s.status {
        StoryStatus::Open => "OPEN",
        StoryStatus::Closed => "CLOSED",
    };
    format!(
        "{} {status} \"{}\" members={} tick={}..{:?}",
        s.id,
        s.title,
        s.member_event_ids.len(),
        s.opened_at_tick,
        s.closed_at_tick
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_ev(tick: u64, dirty: bool, branch: &str) -> Event {
        make_event(
            tick,
            "sensor:git",
            EventBody::GitObserved {
                repo_path: "/tmp/r".into(),
                branch: branch.into(),
                dirty,
            },
        )
    }

    #[test]
    fn append_load_project_tick() {
        let dir = tempfile::tempdir().unwrap();
        let log = EventLog::open(dir.path()).unwrap();
        let e = make_event(1, "system", EventBody::TickStarted);
        log.append(&[e]).unwrap();
        let all = log.load_all().unwrap();
        assert_eq!(all.len(), 1);
        let w = project_world(&all, "strict-cto");
        assert_eq!(w.tick, 1);
    }

    #[test]
    fn last_git_wins() {
        let mut events = vec![
            make_event(1, "system", EventBody::TickStarted),
            git_ev(1, true, "main"),
            make_event(2, "system", EventBody::TickStarted),
            git_ev(2, false, "main"),
        ];
        events[1].tick = 1;
        events[3].tick = 2;
        let w = project_world(&events, "p");
        assert_eq!(w.tick, 2);
        assert_eq!(w.git.as_ref().unwrap().dirty, false);
    }

    #[test]
    fn dirty_opens_story() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = StoryStore::load(dir.path()).unwrap();
        let mut git = git_ev(1, true, "main");
        let extra = apply_git_story_rules(
            &mut store,
            GitStoryInput {
                prev_dirty: Some(false),
                git_event: &mut git,
                member_ids: &[],
                tick: 1,
                at: Local::now(),
            },
        )
        .unwrap();
        assert_eq!(extra.len(), 1);
        assert!(matches!(extra[0].body, EventBody::StoryOpened { .. }));
        assert_eq!(store.stories.len(), 1);
        assert_eq!(store.stories[0].status, StoryStatus::Open);
        assert!(git
            .links
            .iter()
            .any(|l| l.rel == LinkRel::PartOfStory));
    }

    #[test]
    fn clean_closes_story() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = StoryStore::load(dir.path()).unwrap();
        let mut git_dirty = git_ev(1, true, "main");
        apply_git_story_rules(
            &mut store,
            GitStoryInput {
                prev_dirty: Some(false),
                git_event: &mut git_dirty,
                member_ids: &[],
                tick: 1,
                at: Local::now(),
            },
        )
        .unwrap();
        let mut git_clean = git_ev(2, false, "main");
        let extra = apply_git_story_rules(
            &mut store,
            GitStoryInput {
                prev_dirty: Some(true),
                git_event: &mut git_clean,
                member_ids: &[],
                tick: 2,
                at: Local::now(),
            },
        )
        .unwrap();
        assert!(extra.iter().any(|e| matches!(e.body, EventBody::StoryClosed { .. })));
        assert_eq!(store.stories[0].status, StoryStatus::Closed);
    }

    #[test]
    fn project_without_world_json() {
        let dir = tempfile::tempdir().unwrap();
        let log = EventLog::open(dir.path()).unwrap();
        log.append(&[
            make_event(1, "system", EventBody::TickStarted),
            make_event(
                1,
                "sensor:clock",
                EventBody::ClockObserved {
                    timezone: "UTC+0".into(),
                },
            ),
        ])
        .unwrap();
        assert!(!dir.path().join("world.json").exists());
        let w = project_world(&log.load_all().unwrap(), "strict-cto");
        assert_eq!(w.tick, 1);
        assert_eq!(w.timezone, "UTC+0");
    }
}

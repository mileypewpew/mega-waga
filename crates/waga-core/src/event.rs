//! Append-only event log domain types (canonical ground truth).

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::{MemoryClass, MemoryId, XpBeneficiary};

/// Stable event identifier (`evt_<uuid>`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(pub String);

impl EventId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Stable story identifier (`sty_<uuid>`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StoryId(pub String);

impl StoryId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for StoryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Typed edge from this event to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkRel {
    CausedBy,
    Follows,
    RefersTo,
    PartOfStory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventLink {
    pub rel: LinkRel,
    pub to: EventId,
}

/// Kind-specific payload for an event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventBody {
    TickStarted,
    ClockObserved {
        timezone: String,
    },
    GitObserved {
        repo_path: String,
        branch: String,
        dirty: bool,
    },
    PersonaNoticed {
        persona_id: String,
        notice: String,
    },
    PetMoodChanged {
        from: String,
        to: String,
    },
    StoryOpened {
        story_id: StoryId,
        title: String,
    },
    StoryClosed {
        story_id: StoryId,
        summary: String,
    },
    MemoryFormed {
        memory_id: MemoryId,
        class: MemoryClass,
        title: String,
    },
    XpGranted {
        skill_id: String,
        amount: u32,
        beneficiary: XpBeneficiary,
        memory_id: Option<MemoryId>,
        reason: String,
    },
}

/// One immutable fact in the park log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub tick: u64,
    pub at: DateTime<Local>,
    pub actor: String,
    pub links: Vec<EventLink>,
    pub body: EventBody,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoryStatus {
    Open,
    Closed,
}

/// Curated narrative arc over linked events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Story {
    pub id: StoryId,
    pub title: String,
    pub summary: String,
    pub status: StoryStatus,
    pub opened_at_tick: u64,
    pub closed_at_tick: Option<u64>,
    pub anchor_event_id: EventId,
    pub member_event_ids: Vec<EventId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_json_roundtrip() {
        let e = Event {
            id: EventId("evt_test1".into()),
            tick: 3,
            at: Local::now(),
            actor: "sensor:git".into(),
            links: vec![EventLink {
                rel: LinkRel::Follows,
                to: EventId("evt_prev".into()),
            }],
            body: EventBody::GitObserved {
                repo_path: "/tmp/demo".into(),
                branch: "main".into(),
                dirty: true,
            },
        };
        let s = serde_json::to_string(&e).unwrap();
        let back: Event = serde_json::from_str(&s).unwrap();
        assert_eq!(e.id, back.id);
        assert_eq!(e.tick, back.tick);
        assert_eq!(e.links, back.links);
        assert_eq!(e.body, back.body);
    }
}

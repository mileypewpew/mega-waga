# Event Log Spine v1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `events.jsonl` the canonical append-only ground truth with linked events, projected `WorldSnapshot`, and git-transition **Stories**, while keeping `waga tick` / `waga pet` working.

**Architecture:** New `waga-events` crate owns log I/O, projection, and story rules. `waga-core` gains Event/Story types. `waga-world::run_tick` emits event batches instead of writing `narrative.jsonl`. CLI gains `events` and `stories`.

**Tech Stack:** Existing workspace (Rust edition 2021, serde_json JSONL, clap, chrono). Event ids via `uuid` crate (`evt_<uuid_simple>`) to avoid new heavy deps.

**Spec:** [docs/superpowers/specs/2026-07-16-event-log-spine-design.md](../specs/2026-07-16-event-log-spine-design.md)

---

## File map

| Path | Action | Responsibility |
|------|--------|----------------|
| `Cargo.toml` | Modify | Add `waga-events` member + `uuid` workspace dep |
| `crates/waga-core/src/lib.rs` | Modify | Event, Story, Link types; keep WorldSnapshot |
| `crates/waga-core/src/event.rs` | Create | Event domain (optional split if lib grows) |
| `crates/waga-events/Cargo.toml` | Create | New crate |
| `crates/waga-events/src/lib.rs` | Create | Log, project, stories |
| `crates/waga-world/src/lib.rs` | Modify | Event-backed `run_tick` |
| `crates/waga-world/Cargo.toml` | Modify | Depend on `waga-events` |
| `crates/waga-tui/src/main.rs` | Modify | `events`, `stories` subcommands |
| `README.md` | Modify | Document event spine |

---

### Task 1: Core event types

**Files:**
- Modify: `crates/waga-core/Cargo.toml` (add `uuid` if ids generated here — prefer generate in waga-events)
- Modify: `crates/waga-core/src/lib.rs` (or split `event.rs` + `mod event`)

- [ ] **Step 1: Add types and serde tests**

Add to `waga-core`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryId(pub String);

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    TickStarted,
    ClockObserved { timezone: String },
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub tick: u64,
    pub at: DateTime<Local>,
    pub actor: String,
    pub links: Vec<EventLink>,
    #[serde(flatten)]
    pub kind: EventKind,
}

// Note: #[serde(flatten)] on enum with tag may need adjacent tagging instead:
// prefer: kind as field + body, OR externally tagged without flatten.
// **Decision for implementer:** use
//   pub kind: EventKind
// with #[serde(tag = "kind", content = "data")] OR internally tagged variants
// that include common fields only on Event wrapper:
//
// Event { id, tick, at, actor, links, body: EventBody }
// EventBody enum tagged "kind"
```

**Concrete shape to implement (avoids flatten pitfalls):**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub tick: u64,
    pub at: DateTime<Local>,
    pub actor: String,
    pub links: Vec<EventLink>,
    pub body: EventBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventBody {
    TickStarted,
    ClockObserved { timezone: String },
    GitObserved { repo_path: String, branch: String, dirty: bool },
    PersonaNoticed { persona_id: String, notice: String },
    PetMoodChanged { from: String, to: String },
    StoryOpened { story_id: StoryId, title: String },
    StoryClosed { story_id: StoryId, summary: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoryStatus {
    Open,
    Closed,
}

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
```

Keep existing `WorldSnapshot`, `NarrativeEntry` (mark narrative deprecated in doc comments), `TickResult` (extend later with `new_event_ids: Vec<EventId>`).

- [ ] **Step 2: Unit test JSON round-trip for one Event with a link**

```rust
#[test]
fn event_json_roundtrip() {
    let e = Event { /* ... Follows link ... */ };
    let s = serde_json::to_string(&e).unwrap();
    let back: Event = serde_json::from_str(&s).unwrap();
    assert_eq!(e, back);
}
```

- [ ] **Step 3: `cargo test -p waga-core` passes; commit**

```bash
git add crates/waga-core
git commit -m "feat(core): event and story domain types for log spine"
```

---

### Task 2: `waga-events` store + projection

**Files:**
- Create: `crates/waga-events/Cargo.toml`
- Create: `crates/waga-events/src/lib.rs`
- Modify: root `Cargo.toml` members + `waga-events` path dep + `uuid = { version = "1", features = ["v4", "serde"] }`

- [ ] **Step 1: Implement id helper + EventLog**

```rust
pub fn new_event_id() -> EventId {
    EventId(format!("evt_{}", uuid::Uuid::new_v4().simple()))
}
pub fn new_story_id() -> StoryId {
    StoryId(format!("sty_{}", uuid::Uuid::new_v4().simple()))
}

pub struct EventLog {
    pub root: PathBuf,
}

impl EventLog {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self>;
    pub fn path(&self) -> PathBuf { self.root.join("events.jsonl") }
    pub fn load_all(&self) -> Result<Vec<Event>>; // skip empty/corrupt last line
    pub fn append(&self, events: &[Event]) -> Result<()>;
}

pub fn project_world(events: &[Event], default_persona: &str) -> WorldSnapshot;
```

Projection rules per spec §3.6.

- [ ] **Step 2: Tests with tempfile**

```rust
#[test]
fn append_load_project_tick() { /* append TickStarted tick=1; project.tick == 1 */ }

#[test]
fn last_git_wins() { /* two GitObserved; project.git.dirty matches last */ }

#[test]
fn project_without_world_json() { /* only events file */ }
```

- [ ] **Step 3: `cargo test -p waga-events`; commit**

```bash
git commit -m "feat(events): JSONL event log and world projection"
```

---

### Task 3: Story store + git transition rules

**Files:**
- Modify: `crates/waga-events/src/lib.rs` (or `story.rs`)

- [ ] **Step 1: StoryStore load/save `stories.json`**

```rust
pub struct StoryStore {
    pub root: PathBuf,
    pub stories: Vec<Story>,
}
// load default empty; save pretty JSON array
```

- [ ] **Step 2: `apply_git_story_rules(...)`**

Inputs: previous projected git, new `GitObserved` event (with id), tick, batch of other new event ids to attach.  
Outputs: extra events (`StoryOpened` / `StoryClosed`), mutations to StoryStore, links to add on git/persona events.

Behavior per spec §4 (single open git-story per data dir).

- [ ] **Step 3: Tests**

```rust
#[test]
fn dirty_opens_story() { ... }

#[test]
fn clean_closes_story() { ... }
```

- [ ] **Step 4: commit**

```bash
git commit -m "feat(events): story open/close from git dirty transitions"
```

---

### Task 4: Rewire `waga-world::run_tick`

**Files:**
- Modify: `crates/waga-world/Cargo.toml` — add `waga-events`
- Modify: `crates/waga-world/src/lib.rs`

- [ ] **Step 1: Replace narrative append with event batch**

`run_tick` flow:

1. `EventLog::open` + `StoryStore::load`  
2. `events = load_all`; `W0 = project_world`  
3. Build batch (TickStarted, Clock, Git, Persona, optional PetMood, story rules)  
4. Wire `Follows` links: Clock→Tick, Git→Tick, Persona→Git (or Tick if no git)  
5. `append`; save stories; `save_snapshot` cache from `project_world`  
6. Return `TickResult` including notice + mood  

Remove calls to `append_narrative` from `run_tick` (keep fn deprecated or delete).

- [ ] **Step 2: Update world tests**

- tick increments via projection  
- narrative.jsonl **not** required  
- optional: events.jsonl line count ≥ 1 per tick  

- [ ] **Step 3: `cargo test -p waga-world`; commit**

```bash
git commit -m "feat(world): event-backed tick; narrative.jsonl no longer written"
```

---

### Task 5: CLI `events` and `stories`

**Files:**
- Modify: `crates/waga-tui/src/main.rs`
- Modify: `crates/waga-tui/Cargo.toml` if direct `waga-events` dep needed

- [ ] **Step 1: Subcommands**

```rust
Events {
  #[arg(long, default_value = ".waga")]
  data_dir: PathBuf,
  #[arg(long, default_value_t = 20)]
  last: usize,
},
Stories {
  #[arg(long, default_value = ".waga")]
  data_dir: PathBuf,
},
```

Print readable lines:

```text
evt_… tick=3 GitObserved dirty=true links=[follows:evt_…]
sty_… OPEN "Working tree dirty on main" members=4
```

- [ ] **Step 2: Manual smoke**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo run -p waga-tui -- tick
cargo run -p waga-tui -- events --last 10
cargo run -p waga-tui -- stories
```

Expected: events listed; if dirty, a story appears.

- [ ] **Step 3: commit**

```bash
git commit -m "feat(cli): waga events and waga stories commands"
```

---

### Task 6: Docs + rebuild proof

**Files:**
- Modify: `README.md`, `roadmap.md`, `AGENTS.md` (brief)
- Modify: design status if needed

- [ ] **Step 1: Document event spine, rustup PATH, deprecation of narrative.jsonl**

- [ ] **Step 2: Proof script**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --workspace
cargo run -p waga-tui -- tick
rm -f .waga/world.json
cargo run -p waga-tui -- events --last 5
# project still works on next tick
cargo run -p waga-tui -- tick
```

- [ ] **Step 3: Final commit**

```bash
git commit -m "docs: event log spine usage and rebuild semantics"
```

---

## Spec coverage

| Spec section | Task |
|--------------|------|
| Event/Story types | 1 |
| events.jsonl + project | 2 |
| Story rules git | 3 |
| Tick algorithm | 4 |
| CLI events/stories | 5 |
| Migration / docs | 6 |
| Pet still works | 4–5 (no pet rewrite) |

## Type names locked

- `Event`, `EventId`, `EventBody`, `EventLink`, `LinkRel`
- `Story`, `StoryId`, `StoryStatus`
- `EventLog`, `StoryStore`, `project_world`, `new_event_id`, `new_story_id`

---

## Execution handoff

Plan complete. After user confirms the written **spec** (and this plan), implement task-by-task with `cargo test` after each task.

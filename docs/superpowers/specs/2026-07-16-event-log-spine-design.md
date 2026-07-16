# Event Log Spine v1 — Design

**Status:** Approved and implemented (inline session)  
**Date:** 2026-07-16  
**Project:** grok-waga  
**Depends on:** [Tick Kernel v0](./2026-07-16-tick-kernel-v0-design.md)  
**Decisions:** Event log is **king** (canonical ground truth) · JSONL store · Stories promoted from linked events · Git dirty transitions auto-open stories

---

## 1. Goal

Replace the flat `narrative.jsonl` diary with an **append-only event log** where:

1. Every meaningful park change is an **Event** with a stable id.  
2. Events can **link** to other events (graph edges, not only a list).  
3. **WorldSnapshot** is a **projection** of the log (cacheable, disposable).  
4. Clusters of events become **Stories** (curated narrative arcs).

**First “oh it works” moment**

```text
waga tick  (repo dirty)  → GitObserved + links + Story opened
waga tick  (still dirty) → more events PartOfStory
# clean tree
waga tick                → GitObserved clean; story continues/closes
waga events --last 20    → see ids + links
waga stories             → see the arc
# delete world.json
waga tick or project     → world rebuilt from events.jsonl
```

---

## 2. Truth model

| Layer | Role | Mutability |
|-------|------|------------|
| **Event log** | Canonical record of what happened | Append-only |
| **Links** | Typed edges between events | Set at append time (v1); no silent rewrite |
| **WorldSnapshot** | “What is true *now*” | Derived; `world.json` is cache only |
| **Story** | Human-shaped arc over event ids | Append story records + link events; summary may be updated via *new* events later |

**Hard rule:** No important world mutation without an event. Sensors, persona notices, and story opens all append.

**Park for life:** Real sensors (git, clock) feed events; stories are light arcs over real life, not a fiction sim.

---

## 3. Domain types

### 3.1 Identifiers

- `EventId`: string, globally unique per data dir. **Format v1:** `evt_<ulid>` or `evt_<uuid_v4_simple>` (implementation picks one; must be sortable or random but unique).  
- `StoryId`: `sty_<ulid|uuid>`.

### 3.2 Link relations

```text
LinkRel:
  CausedBy      // this event happened because of `to`
  Follows       // temporal/narrative succession
  RefersTo      // soft reference
  PartOfStory   // `to` is not used; story_id in payload OR link target is a story marker event
```

**v1 simplification for PartOfStory:**  
`EventLink { rel: PartOfStory, to: EventId }` where `to` is the **StoryOpened** event id for that story (anchor). All members point at the open event. Story record also lists `member_event_ids` for fast listing (denormalized; rebuildable from links).

### 3.3 Event

```text
Event {
  id: EventId
  tick: u64
  at: DateTime<Local>    // or Utc consistently — pick Local to match v0 snapshot
  kind: EventKind
  actor: String          // "sensor:git" | "sensor:clock" | "persona:strict-cto" | "system" | "user"
  links: Vec<EventLink>
  // kind-specific body as enum externally tagged for serde
}
```

### 3.4 EventKind (v1 closed set)

| Kind | Payload | When |
|------|---------|------|
| `TickStarted` | `{}` | Start of every `run_tick` |
| `GitObserved` | `{ repo_path, branch, dirty }` | After git sensor |
| `ClockObserved` | `{ timezone }` | After clock sensor (optional if folded into TickStarted — **v1: include for clarity**) |
| `PersonaNoticed` | `{ persona_id, notice }` | After template notice |
| `PetMoodChanged` | `{ from, to }` | Only when mood differs from previous projection |
| `StoryOpened` | `{ story_id, title }` | Auto on git dirty transition / rules |
| `StoryClosed` | `{ story_id, summary }` | Auto when arc ends (e.g. dirty→clean after open) |

No LLM kinds in v1.

### 3.5 Story

```text
Story {
  id: StoryId
  title: String
  summary: String
  status: Open | Closed
  opened_at_tick: u64
  closed_at_tick: Option<u64>
  anchor_event_id: EventId    // StoryOpened event
  member_event_ids: Vec<EventId>
}
```

### 3.6 Projection

```text
project(events: &[Event]) -> WorldSnapshot
```

Rules (v1):

- `tick` = max `TickStarted.tick` (or last event’s tick).  
- `git` = last `GitObserved` payload (if any).  
- `active_persona` = last `PersonaNoticed.persona_id` or default.  
- `story.last_beat` = last `PersonaNoticed.notice` or last story title.  
- `observed_at` / `timezone` from last clock/tick event.  
- Pet mood is **not** stored on snapshot necessarily; derive via `waga-pet` from projected git (same as v0).

**Checkpoint:** After each tick, write `world.json` as cache. Deleting it must not lose history; next load reprojects from `events.jsonl`.

---

## 4. Story promotion rules (v1)

**Auto-open:** On `GitObserved` where `dirty == true` and previous projected git was `None` or `dirty == false`:

1. Append `StoryOpened` with title like `Working tree dirty on {branch}`.  
2. Link `GitObserved` → `PartOfStory` → anchor (`StoryOpened` id).  
3. Create/update `Story` record status Open.

**While open:** Subsequent `GitObserved`, `PersonaNoticed`, `PetMoodChanged` in the same “dirty episode” get `PartOfStory` → same anchor and are pushed to `member_event_ids`.

**Auto-close:** On `GitObserved` where `dirty == false` and an Open story exists for “working tree” on that repo/branch (v1: single open git-story at a time per data dir):

1. Append `StoryClosed` with short summary (e.g. notice text or “Tree clean on {branch}”).  
2. Link close event PartOfStory; set story Closed.

**Non-git ticks:** Still emit Tick/Clock/Persona events; no story required.

Later rules (out of scope): multi-story, manual open, calendar arcs, LLM titles.

---

## 5. Persistence layout

```text
.waga/
  events.jsonl     # one JSON object per line; append-only ground truth
  stories.json     # map or list of Story records (rewrite whole file v1 OK)
  world.json       # projection cache only
```

- **Do not** require `narrative.jsonl` for new ticks.  
- Optional later: one-shot import of old narrative lines as synthetic events.  
- **Atomicity v1:** append events line-by-line; then write stories.json; then world.json. Crash between may leave cache stale — reproject fixes. Partial last line: ignore corrupt trailing line on read.

---

## 6. Crate architecture

```text
crates/
  waga-core/       # Event, EventId, EventKind, EventLink, LinkRel, Story, StoryId, WorldSnapshot, …
  waga-events/     # NEW: EventLog store, append, load, project_world, story engine helpers
  waga-world/      # sensors + run_tick orchestration (calls waga-events)
  waga-character/  # unchanged templates
  waga-pet/        # unchanged mood
  waga-tui/        # tick, pet, events, stories commands
```

```text
waga-tui → waga-world → waga-events → waga-core
         → waga-character, waga-pet (display)
waga-world → waga-character, waga-pet (for notices/mood during tick)
```

### 6.1 `waga-events` API (sketch)

```text
EventLog::open(data_dir) -> EventLog
EventLog::load_all() -> Vec<Event>
EventLog::append(&[Event]) -> Result<()>
EventLog::project_world(default_persona) -> WorldSnapshot
StoryStore::load/save
apply_tick_batch(...) -> TickResult   // or live in waga-world
```

---

## 7. Tick algorithm (authoritative)

```text
1. Open EventLog + StoryStore under data_dir
2. Load events (or use in-memory if already loaded); project current world W0
3. tick = W0.tick + 1
4. Build batch:
   a. TickStarted { tick }
   b. ClockObserved
   c. GitObserved (sensor)
   d. PersonaNoticed (templates from projected+new git)
   e. If mood(W0) != mood(after git): PetMoodChanged
   f. Story rules → maybe StoryOpened / StoryClosed + links on relevant events
5. Set links: e.g. GitObserved Follows TickStarted; PersonaNoticed Follows GitObserved or TickStarted
6. Append batch
7. Update StoryStore members
8. Project W1; write world.json cache
9. Return TickResult { snapshot: W1, notice, pet_mood, new_event_ids }
```

---

## 8. CLI

| Command | Behavior |
|---------|----------|
| `waga tick` | Full event-backed tick (existing flags) |
| `waga events [--last N]` | Print recent events: id, tick, kind, links |
| `waga stories` | List stories (status, title, member count) |
| `waga pet` | Unchanged UX; data from projection |

---

## 9. Testing

| Test | Expect |
|------|--------|
| Append + load round-trip | Events equal |
| project after N ticks | tick == N; git matches last observe |
| Delete world.json; project | Same essentials as before delete |
| Dirty then clean | Open then Closed story; members ≥ 2 git events |
| Links present | GitObserved.links contains Follows or PartOfStory |
| Existing pet/character unit tests | Still pass |

---

## 10. Migration from Tick Kernel v0

- New installs: only events.jsonl.  
- Existing `.waga/narrative.jsonl`: leave in place; unread by v1 (document in README).  
- `run_tick` must not append to narrative.jsonl after this ships.

---

## 11. Out of scope

- SQLite / remote sync / multi-process writers  
- LLM story summaries  
- Graph TUI beyond text listing  
- Full Character Engine validation pipeline  
- Editing or deleting historical events  

---

## 12. Success criteria

- [ ] `events.jsonl` is the only required history for rebuild  
- [ ] `waga events` and `waga stories` work  
- [ ] Git dirty→clean produces a visible story arc  
- [ ] `cargo test --workspace` green  
- [ ] `waga pet` still runs  

---

## 13. Implementation order (summary)

1. Types in `waga-core`  
2. `waga-events` store + project + tests  
3. Story rules + tests  
4. Rewire `waga-world::run_tick`  
5. CLI `events` / `stories`  
6. Docs + deprecation note for narrative.jsonl  

Detailed steps: see plan `docs/superpowers/plans/2026-07-16-event-log-spine.md` (written with this spec).

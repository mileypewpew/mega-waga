# Memory + XP v1 — Design

**Status:** Approved and implemented (park / user XP)  
**Date:** 2026-07-16  
**Project:** grok-waga  
**Depends on:** [Event Log Spine](./2026-07-16-event-log-spine-design.md)  
**Decisions:**
- Memory and XP are **paired** (grants cite events and usually a memory)
- Memories are **classified** (closed primary class + tags)
- XP beneficiary v1 = **Park / user** (one skill sheet), not per-persona
- Event log remains **king**; memory/skill files are indexes/projections
- v1 classifier = **deterministic rules** (no LLM)

---

## 1. Goal

Give WAGA a durable learning loop on top of the event spine:

```text
what happened (Event/Story)
  → classify
  → Memory (subjective, typed)
  → XP on the park skill sheet (cited)
  → append MemoryFormed + XpGranted events
  → retrieve later for tick/pet/persona context
```

**First “oh it works” moment**

```text
dirty tree → ticks → clean tree → StoryClosed
  → Episodic memory formed (refs story + events)
  → repo_hygiene XP granted on park sheet
waga memories
waga skills
# optional: pet footer shows last memory + skill level
```

---

## 2. Truth model

| Layer | Role | Canonical? |
|-------|------|------------|
| **Event log** | What happened | Yes |
| **Story** | Arc over events | Yes (as records + events) |
| **Memory** | What was learned / noted | Yes once accepted; **subjective** content |
| **XP grant** | Progression fact | Yes (as events); totals are projections |
| **Skill totals** | Level display | Cache only |

**Hard rules**

1. No XP without citation (`event_ids` non-empty; `memory_id` preferred).  
2. No durable memory without `class` + at least one `source_event_id` (or explicit user remember later).  
3. Personas do **not** own separate XP ledgers in v1 (they only color notices).  
4. Classification runs **before** write; class influences retrieval and which skill gets XP.

---

## 3. Decision: Park XP (A), not persona XP (B)

| | Park (A) — **locked** | Persona (B) — later optional |
|--|------------------------|------------------------------|
| Beneficiary | Single park/user sheet | Per `persona_id` |
| “Did I improve?” | Clear | Split by who was active |
| Fits park-for-life | Best | Better for multi-host sim |

Schema leaves a door:

```text
XpBeneficiary::Park   // v1 only this
// later: Persona { id }
```

---

## 4. Memory classification

### 4.1 Primary class (`MemoryClass`)

| Class | Meaning | Default decay | XP tendency |
|-------|---------|---------------|-------------|
| `Episodic` | Specific episode | Medium | Domain skill for the episode |
| `Semantic` | General pattern/fact | Slow | Knowledge / domain |
| `Procedural` | How we do something | Slow | Craft / workflow |
| `Affective` | Feeling/mood lesson | Medium | Light “presence” skill |
| `Preference` | User like/dislike | Sticky | Usually 0 or tiny |
| `Working` | Scratch / short-term | Fast | **No** long-term XP |

### 4.2 Metadata

```text
Memory {
  id: MemoryId                 // mem_<uuid>
  class: MemoryClass
  scope: Park                  // v1; later Persona/User
  title: String
  body: String
  tags: Vec<String>            // e.g. git, main, hygiene
  importance: u8               // 1–5
  confidence: f32              // 0–1; rules ~0.9
  source: SystemRule | User | Persona | Llm  // v1: SystemRule
  source_event_ids: Vec<EventId>
  story_id: Option<StoryId>
  formed_at_tick: u64
  formed_at: DateTime
  superseded_by: Option<MemoryId>  // v1 unused
}
```

### 4.3 Why classify

- **Retrieval:** CTO-facing context boosts Semantic/Procedural+git; pet may boost Affective/Episodic.  
- **XP routing:** class + tags → skill_id.  
- **Noise control:** Working never clogs long-term sheet.

---

## 5. Skills + XP

### 5.1 Skill catalog (v1 minimal)

| skill_id | Name | Fed by (v1) |
|----------|------|-------------|
| `repo_hygiene` | Repo hygiene | Git story close, git-tagged Episodic/Procedural |
| `presence` | Presence | Affective (mood arcs), optional |

Levels: simple thresholds, e.g. level = f(xp) with fixed table  
`0, 20, 50, 100, 200, …` or `level = floor(sqrt(xp/10))` — pick one in implementation and test it.

### 5.2 Grant

```text
XpGrant {
  id: GrantId                  // or only as event
  beneficiary: Park
  skill_id: String
  amount: u32
  reason: String
  memory_id: Option<MemoryId>
  event_ids: Vec<EventId>
  tick: u64
}
```

### 5.3 Projection

```text
SkillState { skill_id, xp, level }
// skills.json = map skill_id → SkillState (cache)
// rebuild by folding all XpGranted events
```

---

## 6. Event log extensions

New `EventBody` variants:

```text
MemoryFormed {
  memory_id: MemoryId,
  class: MemoryClass,          // or string serde of enum
  title: String,
  // full memory may live in memories index; event carries enough to rebuild essentials
}

XpGranted {
  skill_id: String,
  amount: u32,
  beneficiary: "park",
  memory_id: Option<MemoryId>,
  reason: String,
}
```

Optional later: `MemorySuperseded`.

Links:

- `MemoryFormed` **Follows** triggering event (e.g. StoryClosed)  
- `XpGranted` **Follows** `MemoryFormed` (or the story close if no memory)  
- Both may **PartOfStory** if inside an open/closed arc  

---

## 7. Pipeline (after tick batch / on story close)

```text
1. Tick already appended sensor/persona/story events
2. Memory engine inspects *new* events (especially StoryClosed)
3. For each rule hit:
   a. build Memory (class, tags, refs)
   b. choose skill_id + amount from class/tags/rule
   c. append MemoryFormed (+ links)
   d. append XpGranted (+ links)
   e. update memories index + skills projection cache
4. Retrieve API available for CLI / pet
```

**v1 rules (must implement)**

| Trigger | Class | Memory | XP |
|---------|--------|--------|-----|
| `StoryClosed` (git working-tree story) | `Episodic` | title/body from summary + branch tags `git` | `repo_hygiene` +10 |
| `PetMoodChanged` to `content` after grumpy in same tick window (optional stretch) | `Affective` | short mood note | `presence` +2 |

**Non-goals v1:** consolidate Semantic from repeats, user `waga remember`, LLM candidates.

---

## 8. Persistence

```text
.waga/
  events.jsonl       # ground truth including MemoryFormed, XpGranted
  stories.json
  world.json         # world projection cache
  memories.jsonl     # memory index (rebuildable from events)
  skills.json        # XP projection cache
```

---

## 9. Crate layout

```text
waga-core      += MemoryId, Memory, MemoryClass, MemorySource, SkillState, …
waga-memory    NEW: rules, form_memory, apply_xp, load/save indexes, retrieve
waga-events    += new EventBody variants; serde compat
waga-world     += call memory pipeline after story-related tick work
waga-tui       += `memories`, `skills` subcommands; optional pet footer
```

Dependency:

```text
waga-world → waga-memory → waga-events → waga-core
waga-tui → waga-memory
```

---

## 10. CLI

```bash
waga memories [--last N] [--class episodic]
waga skills
waga tick   # may form memory+XP when story closes
```

---

## 11. Testing

| Test | Expect |
|------|--------|
| Story open then close (simulated events) | 1 Episodic memory, 1 XpGranted repo_hygiene |
| skills projection | xp == 10, level >= 1 per table |
| Replay events only | same skill totals without skills.json |
| No StoryClosed | no hygiene XP from mere dirty observe |
| cargo test workspace | green; pet/tick still work |

---

## 12. Out of scope

- Per-persona XP ledgers  
- Vector search / embeddings  
- Full skill trees / unlocks  
- LLM memory proposals  
- Preference editor UI  

---

## 13. Success criteria

- [ ] Classified memory on git story close  
- [ ] Park `repo_hygiene` XP with citations  
- [ ] `waga memories` / `waga skills`  
- [ ] Event log contains MemoryFormed + XpGranted  
- [ ] Rebuild skills from events alone  

---

## 14. Implementation order

1. Core types + event body variants  
2. `waga-memory` form + XP apply + indexes  
3. Hook from world tick when StoryClosed in batch  
4. CLI + docs  
5. Optional pet footer  

---

## Review ask

Reply:

- **Approved** — write implementation plan and/or implement inline  
- **Change X** — e.g. XP amounts, only hygiene skill, skip Affective rule  

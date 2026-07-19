# Roadmap — grok-waga

**Status:** Always-on daemon v0 **implemented** (interval ticks + notify bus); voice notify v1 earlier  
**Last updated:** 2026-07-19  

This is a living map, not a contract. We move one phase at a time.

### Product decisions (locked for now)

| Decision | Choice |
|----------|--------|
| North star | **Park for life** — real world is the park; characters help you live *here*; story is light (goals, themes, daily arc). |
| First build | **Tick kernel first** — discrete ticks that refresh world + character notice + narrative beat. |
| Stack | **Path A** — greenfield Cargo workspace, Grok Build–aligned (Rust, clap, tokio, ratatui, gix, serde…). |
| Companion UI | **Waga pet** — thin Ratatui on-screen pet whose mood tracks world state (not Godot/desktop overlay yet). |
| Spec (tick v0) | [docs/superpowers/specs/2026-07-16-tick-kernel-v0-design.md](./docs/superpowers/specs/2026-07-16-tick-kernel-v0-design.md) |
| Spec (event spine) | [docs/superpowers/specs/2026-07-16-event-log-spine-design.md](./docs/superpowers/specs/2026-07-16-event-log-spine-design.md) |
| Plan (event spine) | [docs/superpowers/plans/2026-07-16-event-log-spine.md](./docs/superpowers/plans/2026-07-16-event-log-spine.md) |
| Memory + XP | **Park/user sheet (A)**; classified memories; paired XP. Spec: [memory-xp design](./docs/superpowers/specs/2026-07-16-memory-xp-design.md) |
| Not first | Per-persona XP, full agent chat, email/browser sensors, fiction park, A2A, clone of entire grok-build monorepo. |

---

## The big picture (why this exists)

Grok Build is already an excellent coding/agent TUI. **WAGA** gently turns that spirit into an **always-on personal agent platform**:

| Pillar | Name | Promise |
|--------|------|---------|
| 1 | **World Engine** | Every agent always knows “what is now” without re-asking. |
| 2 | **Character Engine** | Instant, consistent personas (detective, CTO, researcher…). |
| 3 | **Long-term memory** | Smarter recall that works *with* world state. |
| ★ | **A2A** (nice-to-have) | Agents can talk to other agents. |

We keep: modular crates, Rust speed, open-source friendliness, xAI-aligned values, and a path to contribute good pieces upstream later.

---

## Decomposition (so nobody panics)

This is **four products in a trench coat**. We will **not** build them all at once.

**Recommended order (updated after product decisions):**

1. **Foundation** — docs, repo hygiene, shared types, rustup  
2. **Tick Kernel v0** — `tick()` + world + persona templates + **Waga pet** (Ratatui)  
3. **Memory v0** — store ticks / important beats that reference world  
4. **Character Engine v0** — richer persona files & consistency (+ optional LLM later)  
5. **Always-on shell** — richer TUI / background service  
6. **A2A** — only after the above feel real  

You can reorder later; Tick Kernel + pet stay the spine of the first demo.

---

## Phase 0 — Orientation (NOW)

**Goal:** Feel oriented, not overloaded.

- [x] Empty repo becomes a named project with vision in writing  
- [x] `AGENTS.md` — how humans & agents work here  
- [x] `roadmap.md` — this file  
- [x] Product decisions: Park for life + Tick kernel first + Path A stack + Waga pet  
- [x] Design draft written: `docs/superpowers/specs/2026-07-16-tick-kernel-v0-design.md`  
- [x] User **approved** design (“Looks like a good place to start! Lets do this!”)  
- [x] Implementation plan + Cargo workspace + `waga tick` / `waga pet`  

**Exit criteria:** Met — design approved and first runnable kernel shipped.

---

## Phase 1–2 progress (done)

- [x] rustup + workspace + crates  
- [x] World snapshot, clock + git sensors, narrative log  
- [x] Strict CTO templates  
- [x] Waga pet mood + Ratatui screen  
- [x] `cargo test --workspace` green

---

## Phase 1 — Repo foundation (tiny & safe)

**Goal:** A real Rust workspace *skeleton* with almost no features.

- [ ] `git init` (if not already) + simple `.gitignore`  
- [ ] Cargo workspace with **only** `waga-core` (types + version + hello test)  
- [ ] `README.md` with setup + “what exists / what doesn’t”  
- [ ] Optional: `docs/superpowers/specs/` for the first real design  

**Exit criteria:** `cargo test` passes with one trivial test. Zero world/character magic yet.

---

## Phase 2 — World Engine v0 (“what is now”, local-only)

**Goal:** A persistent world snapshot from **safe, local** signals.

Suggested first sensors (pick 1–2):

| Sensor | Why it’s a good first pick |
|--------|----------------------------|
| **Git status** (cwd / watched repos) | High signal, no cloud, easy to test |
| **Local files watchlist** | User-chosen paths only |
| **Host clock + timezone** | Trivial, teaches snapshot shape |
| Weather / email / browser | **Later** — privacy & OAuth complexity |

Deliverables:

- [ ] `WorldSnapshot` type in `waga-core` or `waga-world`  
- [ ] `waga-world`: load/save snapshot to disk (e.g. JSON or SQLite)  
- [ ] `waga-state-sync`: poll or notify for *one* sensor  
- [ ] CLI or test that prints “current world”  

**Exit criteria:** After `git status` changes, a refresh shows updated world state without re-prompting an LLM.

---

## Phase 3 — Memory v0 (with World)

**Goal:** Memory that can *reference* world facts, not only chat logs.

- [ ] Episodes + facts store (start simple: SQLite)  
- [ ] Write path: “important event” → memory  
- [ ] Read path: query by time / tag / similarity (embeddings can wait)  
- [ ] Inject a short “memory + world” preamble into agent context  

**Exit criteria:** Agent context includes “last known world” + 1–3 relevant memories automatically.

---

## Phase 4 — Character Engine v0

**Goal:** Load a persona file and stay in character.

- [ ] Persona format (YAML/TOML/Markdown frontmatter — decide later)  
- [ ] `load` / `list` / `activate` personas  
- [ ] Consistency rules (name, voice, hard constraints)  
- [ ] 2 example personas (e.g. *Helpful Researcher*, *Strict CTO*)  

**Note:** A sibling Python “Character Engine” experiment may inspire domain ideas (event log, blueprint vs runtime). We **port concepts**, not the whole stack, unless we explicitly decide to bridge.

**Exit criteria:** Switching persona changes tone/constraints immediately; world + memory still attach.

---

## Phase 5 — Always-on shell + TUI

**Goal:** Feels like a living desk companion, not a one-shot CLI.

- [x] Background refresh loop v0 — `waga daemon` (interval ticks, `daemon.json`, `notify.jsonl`)  
- [ ] Ratatui views: World · Memory · Character · Chat  
- [ ] Graceful pause of sensors; clear privacy indicators  
- [ ] Windows service / tray (later); Linux systemd unit (later)  

**Exit criteria (v0):** Leave `waga daemon` running; come back; world ticks advanced; high-signal events on notify bus.

---

## Phase 6 — A2A (nice-to-have)

**Goal:** Talk to other agents without rewriting the core.

- [ ] Choose a protocol target (e.g. emerging A2A standards)  
- [ ] Thin adapter crate `waga-a2a`  
- [ ] One demo: WAGA persona ↔ external agent message  

**Exit criteria:** One happy-path interop demo documented in README.

---

## Explicit non-goals (for now)

- Full email/browser/OS automation on day one  
- Replacing upstream Grok Build  
- Perfect AGI memory  
- Shipping all crates empty “for architecture points”

---

## First three steps (human checklist)

See the chat message for the live “do these now” list. When those are done, we start **Phase 1** together.

---

## How we update this file

After each phase exit, check boxes and add a one-line “what we learned.” Keep it kind and short.

# Tick Kernel v0 + Waga Pet — Design

**Status:** Approved — implemented as Tick Kernel v0  
**Date:** 2026-07-16  
**Project:** grok-waga (World-Aware General Agent)  
**Decisions locked:** Park for life · Tick kernel first · Stack Path A (greenfield, Grok Build–aligned) · Thin Ratatui **Waga pet**

---

## 1. Goal

Ship a **tiny, runnable** first slice of WAGA that:

1. Advances a **tick-based world** from real local signals (park for life).  
2. Keeps a light **persona notice** + **narrative log**.  
3. Shows a **Waga pet** on screen (Ratatui) whose mood/pose reflects world state.  
4. Uses the **same tech family as Grok Build** without forking the whole monorepo.

**First “oh it works” moment**

```text
Dirty the git repo → run WAGA → pet looks concerned / grumpy
 → narrative log notes the dirty tree
 → clean the repo → tick again → pet calms down
```

No pasting context. No email/browser. No full agent chat loop required.

---

## 2. Product framing

| Concept | Meaning in v0 |
|---------|----------------|
| **Park** | Your machine / project (real world), not a fiction sim |
| **Tick** | One discrete advance of park time |
| **World** | `WorldSnapshot` = what is true *now* |
| **Host / character** | Active persona; interprets world, does not invent facts |
| **Story** | Light narrative log line per tick (daily arc seeds) |
| **Waga pet** | On-screen companion (ASCII/Unicode sprite in Ratatui) that *embodies* world mood |

Westworld inspiration = architecture metaphor (ticks, hosts, park state), not the product genre.

---

## 3. Tech stack (Path A — Grok Build–aligned, greenfield)

Mirror upstream patterns; **do not** clone `xai-org/grok-build` into this repo for v0.

| Concern | Choice | Upstream echo |
|---------|--------|----------------|
| Language | Rust, edition **2024** | Grok Build workspace |
| Toolchain | Pin via `rust-toolchain.toml` (target **1.92** or latest stable if 1.92 unavailable) | `rust-toolchain.toml` |
| Build | Cargo **workspace**, modular crates | Many `xai-grok-*` crates |
| CLI | **clap** (derive) | clap |
| TUI | **ratatui** + **crossterm** | ratatui 0.29 family |
| Async | **tokio** (tick loop / UI event loop) | tokio full |
| Serde | **serde** + **serde_json** | snapshot persistence |
| Config | **toml** (later); v0 may use defaults + flags | toml |
| Time | **chrono** | chrono |
| Errors | **thiserror** + **anyhow** | same family |
| Logging | **tracing** + `RUST_LOG` | tracing / RUST_LOG |
| Git sensor | **gix** (native, no shell) | gix + `xai-gix-status` idea |
| HTTP / LLM | **out of scope for v0** | later reqwest |
| License lean | Apache-2.0 when formalized | Grok Build Apache-2.0 |

**Local prerequisite:** install Rust via `rustup` (machine may not have `cargo` yet).

---

## 4. Crate map (v0 only)

```text
grok-waga/
├── Cargo.toml                 # workspace root
├── rust-toolchain.toml
├── crates/
│   ├── waga-core/             # shared types, errors
│   ├── waga-world/            # sensors, snapshot load/save, tick merge
│   ├── waga-character/        # persona load + template notice
│   ├── waga-pet/              # pet mood + sprite frames (pure logic)
│   └── waga-tui/              # binary: clap + ratatui pet + tick commands
└── personas/                  # example persona files
    └── strict-cto.toml
```

| Crate | Responsibility | Depends on |
|-------|----------------|------------|
| `waga-core` | `TickId`, `WorldSnapshot`, `PersonaId`, error types | serde, chrono, thiserror |
| `waga-world` | clock + git sensors; merge; persist snapshot JSON | waga-core, gix |
| `waga-character` | load persona; `notice(snapshot) -> String` (templates) | waga-core |
| `waga-pet` | `PetMood` from snapshot; frame/sprite selection | waga-core |
| `waga-tui` | CLI binary: `tick`, `pet` (live view), wiring | all above, clap, ratatui, crossterm, tokio |

**Explicitly later crates:** `waga-memory`, `waga-state-sync` (as standalone), `waga-a2a`, full agent shell.

---

## 5. Data model (v0)

### 5.1 `WorldSnapshot` (JSON on disk)

```text
tick: u64
observed_at: DateTime<Local>
clock: { local: ..., timezone: ... }
git: Option<{
  repo_path: PathBuf,
  branch: String,
  dirty: bool,
  // ahead/behind optional if cheap
}>
story: {
  last_beat: String,      // last narrative line summary
  theme: Option<String>,  // optional light theme
}
active_persona: String    // id / filename stem
```

Default path: `~/.local/share/waga/world.json` (or project-local `.waga/world.json` via flag). Prefer **project-local** when `WAGA_HOME` / `--data-dir` not set: `.waga/` under cwd for easy demos.

### 5.2 Persona file (TOML)

```toml
id = "strict-cto"
name = "Strict CTO"
voice = "terse, high standards, no fluff"
constraints = ["Never invent git or filesystem facts"]

[templates]
git_dirty = "Repo dirty on {branch}. Clean tree before we talk merge."
git_clean = "Tree clean on {branch}. Good."
default = "Tick {tick}. Standing by."
```

v0 notices are **templates only** (no LLM).

### 5.3 Narrative log

Append-only JSONL: `.waga/narrative.jsonl`

```json
{"tick":3,"at":"...","persona":"strict-cto","git_dirty":true,"notice":"...","pet_mood":"grumpy"}
```

### 5.4 Pet mood (derived, not stored as truth)

Derived each tick from world:

| Condition | Mood |
|-----------|------|
| git dirty | `Grumpy` / `Concerned` |
| git clean | `Content` / `Happy` |
| no git repo configured | `Idle` / `Curious` |
| (later) other sensors | extend mapping in `waga-pet` only |

Pet does **not** invent world state; it only maps facts → mood → sprite.

---

## 6. Tick algorithm

```text
1. Load WorldSnapshot (or create tick=0 empty)
2. Run sensors (clock always; git if repo path known)
3. Merge facts; tick += 1; observed_at = now
4. Load active persona → notice = template(snapshot)
5. pet_mood = derive(snapshot)
6. story.last_beat = short summary(notice, mood)
7. Append narrative log line
8. Save snapshot
9. Return TickResult { snapshot, notice, pet_mood }
```

CLI / TUI both call the same function in `waga-world` (or a thin `tick` module in core/world). **Single source of truth for park advance.**

---

## 7. Waga pet (Ratatui) — scope

### 7.1 What it is

A **terminal companion** (not a desktop overlay / Godot window in v0):

- Small multi-line Unicode/ASCII sprite  
- Mood label + optional one-line “speech bubble” (persona notice, truncated)  
- World strip: tick #, branch, dirty/clean, time  

### 7.2 Modes

| Command | Behavior |
|---------|----------|
| `waga tick` | Run one tick; print text summary (CI-friendly) |
| `waga pet` | Fullscreen Ratatui: show pet; auto-tick on interval (e.g. 5–30s) or key `t` for manual tick; `q` quit |
| `waga pet --once` | Draw pet for current snapshot without looping (optional) |

### 7.3 Interaction (minimal)

- `q` / `Esc` — quit  
- `t` — tick once  
- `r` — force sensor refresh + tick  
- (later) persona switch, theme, pet skins  

### 7.4 Non-goals for pet v0

- Transparent desktop overlay / always-on-top OS window  
- Godot / game engine body  
- Animation framework beyond 2–3 static frames per mood  
- Voice, image gen, MCP  

Sibling Python “Aether overlay pet” remains **inspiration only**.

---

## 8. Sensors (v0)

| Sensor | Required | Notes |
|--------|----------|--------|
| Clock / timezone | Yes | chrono |
| Git status | Yes (if path set) | gix; default: discover repo from cwd |
| Files, calendar, weather, browser, email | No | later, opt-in |

Privacy: local only; no network in v0.

---

## 9. Error handling

- No git repo: snapshot.git = None; pet mood Idle; notice uses `default` template.  
- Corrupt snapshot: backup aside, start fresh, log warning.  
- TUI backend fail: exit non-zero with message; `waga tick` still works headless.  
- Never panic on sensor failure; degrade gracefully.

---

## 10. Testing

| Area | Tests |
|------|--------|
| Merge / tick counter | unit in waga-world |
| Git dirty → mood Grumpy | unit in waga-pet (fixture snapshot) |
| Template fill | unit in waga-character |
| Snapshot round-trip JSON | unit in waga-world |
| CLI `tick` smoke | optional integration later |

No UI snapshot tests required in v0.

---

## 11. Milestone slices (implementation order)

After this design is approved:

1. **Foundation** — git init, rustup, workspace, `waga-core` hello test  
2. **World + tick** — sensors, persist, `waga tick` text  
3. **Character templates** — Strict CTO persona  
4. **Pet logic** — mood mapping + frames  
5. **Ratatui pet screen** — `waga pet` live view  
6. **Polish** — README demo gif/script, narrative log visible in UI  

Each slice stays `cargo test` / manually runnable.

---

## 12. Out of scope (explicit)

- LLM-backed character  
- Memory engine, A2A  
- Forking/vendoring full grok-build tree  
- Python Character Engine runtime bridge  
- Rich narrative arcs / multi-host park  
- Always-on systemd service (interval loop inside `waga pet` is enough)

---

## 13. Success criteria checklist

- [ ] `cargo test` green on workspace  
- [ ] `waga tick` advances tick and writes `.waga/world.json` + narrative line  
- [ ] Dirty git → notice + pet mood change without user pasting status  
- [ ] `waga pet` shows sprite + world strip; `t` ticks; `q` quits  
- [ ] Docs match reality (README + this spec)

---

## 14. Open points (resolved for v0)

| Topic | Decision |
|-------|----------|
| Greenfield vs clone upstream | **Path A** greenfield |
| Persistence | Project `.waga/` JSON + JSONL |
| Pet surface | **Ratatui in-terminal**, not OS overlay |
| Character intelligence | Templates only |
| First persona | Strict CTO |

---

## 15. Review ask

Please read this spec and reply:

- **“Approved”** — proceed to implementation plan (`writing-plans`) then Phase 1 foundation  
- **“Change X”** — list tweaks (e.g. binary name `waga` vs `grok-waga`, pet only no CLI tick, different data dir)

No Rust code until you approve this document.

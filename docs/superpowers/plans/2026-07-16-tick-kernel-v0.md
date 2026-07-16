# Tick Kernel v0 + Waga Pet Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a runnable greenfield Rust workspace where `waga tick` advances park state from git+clock and `waga pet` shows a Ratatui companion whose mood tracks that world.

**Architecture:** Modular Cargo crates (`waga-core` types → `waga-world` sensors/tick → `waga-character` templates → `waga-pet` mood/sprites → `waga-tui` binary). Single `run_tick()` path shared by CLI and TUI. Project-local `.waga/` persistence.

**Tech Stack:** Rust (edition 2024), Cargo workspace, clap, tokio, serde/serde_json, chrono, thiserror/anyhow, tracing, gix, ratatui, crossterm, toml.

**Spec:** [docs/superpowers/specs/2026-07-16-tick-kernel-v0-design.md](../specs/2026-07-16-tick-kernel-v0-design.md)

---

## File map

| Path | Responsibility |
|------|----------------|
| `Cargo.toml` | Workspace members + shared dependency versions |
| `rust-toolchain.toml` | Pin stable Rust |
| `.gitignore` | `target/`, `.waga/`, etc. |
| `crates/waga-core/src/lib.rs` | `WorldSnapshot`, `GitStatus`, `StoryState`, errors |
| `crates/waga-world/src/lib.rs` | load/save, git+clock sensors, `run_tick` |
| `crates/waga-character/src/lib.rs` | persona TOML + template notice |
| `crates/waga-pet/src/lib.rs` | `PetMood`, sprite frames |
| `crates/waga-tui/src/main.rs` | clap + tick command + pet TUI |
| `personas/strict-cto.toml` | First persona |
| `README.md` | How to build/run demo |

---

### Task 1: Foundation (toolchain + workspace + waga-core)

**Files:**
- Create: `rust-toolchain.toml`, `.gitignore`, `Cargo.toml`
- Create: `crates/waga-core/Cargo.toml`, `crates/waga-core/src/lib.rs`

- [ ] **Step 1: Install Rust if missing**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
rustc --version
```

Expected: rustc version prints (stable).

- [ ] **Step 2: Init git + ignore**

```bash
cd /home/miley/dev/grok-waga
git init
```

`.gitignore`:
```
/target
**/*.rs.bk
.waga/
.DS_Store
```

- [ ] **Step 3: Workspace + waga-core types**

Root `Cargo.toml` members: `crates/waga-core` (add others as created).  
`waga-core` defines:

- `GitStatus { repo_path, branch, dirty }`
- `StoryState { last_beat, theme }`
- `WorldSnapshot { tick, observed_at, timezone, git, story, active_persona }`
- `WagaError` / `Result`

With serde derives and a unit test that default snapshot has `tick == 0`.

- [ ] **Step 4: `cargo test -p waga-core` passes; commit**

```bash
cargo test -p waga-core
git add -A && git commit -m "feat: workspace foundation and waga-core snapshot types"
```

---

### Task 2: waga-world (sensors + tick + persist)

**Files:**
- Create: `crates/waga-world/Cargo.toml`, `crates/waga-world/src/lib.rs`

- [ ] **Step 1: Implement clock sensor + empty tick**

`observe_clock() -> (DateTime<Local>, String timezone)`  
`run_tick(data_dir, persona_id, repo_hint) -> TickResult` increments tick, sets observed_at.

- [ ] **Step 2: Git sensor via gix**

Discover repo from `repo_hint` or cwd; set `branch` + `dirty` (any unstaged/staged/untracked change = dirty). On failure, `git = None`.

- [ ] **Step 3: Persist**

`data_dir/world.json` + append `data_dir/narrative.jsonl`.  
Unit tests with tempfile: tick 0→1→2; round-trip JSON.

- [ ] **Step 4: `cargo test -p waga-world`; commit**

```bash
git commit -m "feat: waga-world tick, git sensor, and .waga persistence"
```

---

### Task 3: waga-character (Strict CTO templates)

**Files:**
- Create: `crates/waga-character/Cargo.toml`, `crates/waga-character/src/lib.rs`
- Create: `personas/strict-cto.toml`

- [ ] **Step 1: Load TOML persona; fill templates**

Templates: `git_dirty`, `git_clean`, `default` with `{branch}`, `{tick}`.

- [ ] **Step 2: Unit tests for dirty/clean/default paths**

- [ ] **Step 3: Wire notice into `run_tick` (world calls character or tui wires both)**

Prefer: `waga-tui` or `waga-world` orchestrates — **orchestration in waga-world via optional notice string built by character**, or keep world pure and compose in tui/`tick` module. **Decision:** compose in `waga-world::run_tick` accepting a `NoticeFn` or call character from a thin `waga_world::tick_with_persona` in tui. Cleaner: **`waga-world` stays pure sensors/persist; `waga-tui` composes character+pet after world tick.** Simpler API for tests: add `waga_world::advance_world` + `waga_tui`/`lib` compose.

**Revised:** put `run_full_tick` in `waga-tui` as private fn, OR add crate-free composition in `waga-world` that takes notice string. Spec says single `run_tick` — implement full pipeline in `waga-world` depending on `waga-character` and `waga-pet` for cohesion.

Dependency direction:
```text
waga-tui → waga-world → waga-character, waga-pet, waga-core
         → waga-character, waga-pet (for display)
```

`waga-world::run_tick` loads persona, computes notice, mood, writes log.

- [ ] **Step 4: commit**

```bash
git commit -m "feat: waga-character persona templates (Strict CTO)"
```

---

### Task 4: waga-pet (mood + sprites)

**Files:**
- Create: `crates/waga-pet/Cargo.toml`, `crates/waga-pet/src/lib.rs`

- [ ] **Step 1: `PetMood` enum + `mood_from_snapshot`**

dirty→Grumpy, clean→Content, no git→Idle

- [ ] **Step 2: `sprite(mood) -> &'static str` multi-line ASCII**

- [ ] **Step 3: unit tests; commit**

```bash
git commit -m "feat: waga-pet mood mapping and sprites"
```

---

### Task 5: waga-tui binary (clap + ratatui)

**Files:**
- Create: `crates/waga-tui/Cargo.toml`, `crates/waga-tui/src/main.rs`

- [ ] **Step 1: `waga tick` command**

Flags: `--data-dir` (default `.waga`), `--persona` path, `--repo` path.  
Print tick, git, notice, mood.

- [ ] **Step 2: `waga pet` Ratatui**

Draw sprite + speech bubble + world strip. Keys: `q` quit, `t` tick, `r` refresh tick. Optional auto-tick every 10s with tokio/crossterm poll.

- [ ] **Step 3: `cargo build -p waga-tui`; manual smoke**

```bash
cargo run -p waga-tui -- tick
cargo run -p waga-tui -- pet   # interactive
```

- [ ] **Step 4: commit**

```bash
git commit -m "feat: waga CLI tick and Ratatui pet screen"
```

---

### Task 6: Docs polish

- [ ] Update `README.md` with install, build, demo steps  
- [ ] Mark design status Approved; update roadmap Phase 0 complete / Phase 1–2 progress  
- [ ] Final `cargo test --workspace`  
- [ ] commit: `docs: README demo for tick kernel and Waga pet`

---

## Spec coverage check

| Spec item | Task |
|-----------|------|
| WorldSnapshot + persist | 1–2 |
| Clock + git sensors | 2 |
| Tick algorithm + narrative log | 2–3 |
| Persona templates | 3 |
| Pet mood + Ratatui | 4–5 |
| CLI tick + pet | 5 |
| No LLM / Path A stack | all |
| Tests | 1–4 |

## Execution

Implement inline in session (user said "let's do this") with frequent commits and `cargo test` after each task.

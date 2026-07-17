# AGENTS.md — grok-waga (World-Aware General Agent)

> Friendly fork / evolution of Grok Build into an always-on personal agent platform.
> **Go slow. Stay modular. Keep upstream-compatible spirit.**

## What this project is

**grok-waga** = **W**orld-**A**ware **G**eneral **A**gent.

A Rust-first, TUI-friendly (Ratatui) agent platform that:

1. **Knows the world** — a live “what-is-now” layer (World Engine)
2. **Can become anyone** — consistent personas via Character Engine
3. **Remembers well** — long-term memory beyond flat `MEMORY.md` files
4. **Plays nice** — modular crates, open-source friendly, xAI-aligned, A2A-ready later

Philosophy: experiment boldly in *this* repo; keep pieces clean enough to contribute good ideas back upstream later.

## Instruction priority

1. User’s explicit requests (highest)
2. This file + `roadmap.md`
3. Superpowers / project skills when present
4. Default agent behavior (lowest)

## How to work here (for humans & AI agents)

- **Bite-sized only.** Prefer one small milestone that stays runnable over a grand rewrite.
- **Design before code** for anything creative (new engines, APIs, architecture). Present a short design; get a yes; then implement.
- **No big-bang scaffold.** Add crates/modules only when the next milestone needs them.
- **Upstream compatibility.** Prefer *additive* modules and clean interfaces over forking core TUI/agent loops in incompatible ways.
- **Privacy-first world sync.** Sensors (email, browser, location, etc.) are opt-in, local-first, and documented.
- **Tests where it matters.** Domain logic (world state merge, character consistency, memory retrieval) gets tests; pure glue can wait.

## Suggested crate / module map (target, not day-one)

These are **planned** boundaries. Do not create all of them at once.

```text
grok-waga/                    # workspace root (eventually)
├── AGENTS.md                 # this file
├── roadmap.md                # phased plan
├── README.md
├── docs/
│   ├── vision.md             # short product vision (optional)
│   └── superpowers/          # specs & plans when we use that workflow
│       ├── specs/
│       └── plans/
│
├── crates/                   # when we start Rust workspace
│   ├── waga-core/            # shared types, errors, config, traits
│   ├── waga-world/           # World Engine: state model + sensors + merge
│   ├── waga-state-sync/      # StateSync: schedule, dirty flags, pub/sub
│   ├── waga-character/       # Character Engine: persona load/run/consistency
│   ├── waga-memory/          # long-term memory store + retrieval
│   ├── waga-a2a/             # (later) agent-to-agent protocol adapters
│   └── waga-tui/             # Ratatui shell / CLI entry (Grok Build–inspired)
│
└── sensors/                  # optional later: thin adapters (git, calendar, …)
```

### Responsibility cheat-sheet

| Module | Job in one sentence |
|--------|---------------------|
| **waga-core** | Shared vocabulary: `WorldSnapshot`, `PersonaId`, errors, config paths. |
| **World Engine (`waga-world`)** | Maintains a persistent “what is true *now*” snapshot from sensors. |
| **StateSync (`waga-state-sync`)** | When/how sensors refresh; notifies consumers of updates. |
| **Character Engine (`waga-character`)** | Load/create/run personas; keep voice & constraints consistent. |
| **Memory (`waga-memory`)** | Durable episodic + semantic memory; feeds World + Character. |
| **A2A (`waga-a2a`)** | Nice-to-have: talk to other agents via a standard protocol. |
| **TUI (`waga-tui`)** | The beautiful always-available surface (Ratatui). |

## Relationship to other projects

- **Upstream Grok Build:** spiritual parent — TUI quality, modular agent skills, xAI spirit. Stay compatible in *ideas* and clean APIs even if binary/source lineage differs.
- **Sibling experiments** (e.g. Python Character Engine repos): inspiration only. Port concepts carefully; do not couple runtimes unless we deliberately choose to.

## Tech stack (aligned with Grok Build)

Greenfield workspace (**Path A** — do not clone the full upstream monorepo for day one):

- **Rust** (edition **2021**), Cargo workspace, pin via `rust-toolchain.toml` (use **rustup** cargo, not distro `/usr/bin/cargo`)
- **clap**, **tokio**, **serde** / **serde_json**, **chrono**, **thiserror** / **anyhow**, **tracing**
- **gix** for git sensors (native)
- **ratatui** + **crossterm** for the **Waga pet** TUI
- LLM/HTTP (**reqwest**) later — Tick Kernel v0 uses persona **templates** only

Authoritative v0 design: [docs/superpowers/specs/2026-07-16-tick-kernel-v0-design.md](./docs/superpowers/specs/2026-07-16-tick-kernel-v0-design.md)

## Current phase

See [roadmap.md](./roadmap.md). **Tick Kernel v0 + Waga pet** is implemented.

```bash
export PATH="$HOME/.cargo/bin:$PATH"   # rustup cargo, not distro 1.75
cargo test --workspace
cargo run -p waga-tui -- tick
cargo run -p waga-tui -- events --last 10
cargo run -p waga-tui -- stories
cargo run -p waga-tui -- memories
cargo run -p waga-tui -- skills
cargo run -p waga-tui -- pet
```

Event log spine: `events.jsonl` is canonical.  
Memory + park XP: `docs/superpowers/specs/2026-07-16-memory-xp-design.md`.

### Build note (no system gcc)

If `cc`/`gcc` is missing, this host may use Zig as the linker via `.cargo/config.toml` (`zig cc`). Prefer a normal system toolchain when available.

## Definition of “done” for early work

A change is done when:

1. It is small and explained in plain language.
2. Docs (`AGENTS.md` / `roadmap.md` / a short spec) still match reality.
3. Nothing is half-broken for the next session.
4. Privacy/opt-in notes exist for any new sensor.

## Encouragement

You do not need to build the whole platform this week. One clear interface, one sensor, one persona file format — that is real progress. We will stack tiny wins.

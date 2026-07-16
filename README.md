# grok-waga

**World-Aware General Agent** — a friendly, modular evolution of the Grok Build spirit into an always-on personal agent platform.

> Status: **Tick Kernel v0 + Waga pet** is runnable.

## Vision (short)

A fast Rust agent platform that **always knows the current world**, can **run consistent characters/personas**, and has **real long-term memory** — with a beautiful TUI and room to grow (A2A later). Open-source friendly, privacy-conscious, xAI-aligned.

## Quick start

### Prerequisites

- **Rust via [rustup](https://rustup.rs)** (recommended: current stable).  
  This project needs a modern toolchain — **not** old distro packages like Ubuntu’s `cargo 1.75`.
- **git** on `PATH` (for the world sensor)
- A C linker: system `gcc`/`cc`, or Zig as `zig cc` (see `.cargo/config.toml` / AGENTS.md)

### Use the right Cargo (common gotcha)

If you see `edition2024` errors or `Cargo (1.75.0)`, your shell is using **system** cargo ahead of rustup:

```bash
which cargo          # bad if this is /usr/bin/cargo
cargo --version      # want 1.85+ / current stable, not 1.75

# Fix for this shell:
source "$HOME/.cargo/env"
# or:
export PATH="$HOME/.cargo/bin:$PATH"

which cargo          # should be ~/.cargo/bin/cargo
cargo --version
```

Make it permanent (bash):

```bash
echo 'source "$HOME/.cargo/env"' >> ~/.bashrc
source ~/.bashrc
```

### Build & test

```bash
source "$HOME/.cargo/env"
cd grok-waga
cargo test --workspace
cargo build -p waga-tui --release
```

### Demo: tick the park

```bash
# One headless tick (writes .waga/world.json + narrative.jsonl)
cargo run -p waga-tui -- tick

# Use the Strict CTO persona file
cargo run -p waga-tui -- tick --persona personas/strict-cto.toml

# Meet the Waga pet (Ratatui). Keys: t/r = tick, q = quit
cargo run -p waga-tui -- pet
```

**First “oh it works” moment**

1. Run `cargo run -p waga-tui -- tick` in this repo.  
2. Edit a tracked file (or leave uncommitted changes) so git is dirty.  
3. Tick again — notice should warn; pet mood becomes **grumpy**.  
4. Commit/stash clean → tick → pet becomes **content**.

## Architecture (v0)

| Crate | Role |
|-------|------|
| `waga-core` | Shared types (`WorldSnapshot`, errors) |
| `waga-world` | Sensors (clock + git), tick, `.waga/` persist |
| `waga-character` | Persona TOML + template notices |
| `waga-pet` | Mood mapping + ASCII sprites |
| `waga-tui` | Binary `waga`: `tick` + `pet` |

Stack aligns with Grok Build: Rust, Cargo workspace, clap, ratatui/crossterm, serde, chrono, tracing. Greenfield (**Path A**), not a monorepo fork.

## Docs

| Doc | Purpose |
|-----|---------|
| [AGENTS.md](./AGENTS.md) | How humans & agents work here |
| [roadmap.md](./roadmap.md) | Phases and product decisions |
| [Tick Kernel design](./docs/superpowers/specs/2026-07-16-tick-kernel-v0-design.md) | v0 design |
| [Implementation plan](./docs/superpowers/plans/2026-07-16-tick-kernel-v0.md) | Build plan |

## License

Apache-2.0 (see crate manifests).

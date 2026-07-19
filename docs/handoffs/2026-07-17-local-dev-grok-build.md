# Handoff — Mega Waga → local Grok Build

**Date:** 2026-07-17  
**For:** Grok Build (or any agent) on **local Windows dev machine**  
**Repo:** https://github.com/mileypewpew/mega-waga  
**Remote:** `git@github.com:mileypewpew/mega-waga.git`  
**Server path (origin of this work):** `/home/miley/dev/grok-waga` (also pushed as `mega-waga`)

---

## 0. Read this first (30 seconds)

You are continuing **Mega Waga** (folder may still be named `grok-waga` in older paths): a **world-aware, always-on personal agent platform** in Rust. Spiritual sibling of Grok Build (TUI, modular crates, xAI-aligned), **not** a full fork of the Build monorepo.

**Spine is real:** events → stories → classified memories → park XP → status/pet → premium TTS → now-playing + SuperCollider HumanMusic bed.

**Philosophy:** Park for life · event log is king · bite-sized · design before big creative work · local-first / opt-in sensors.

**Instruction priority:** user request > `AGENTS.md` + `roadmap.md` > specs in `docs/superpowers/` > defaults.

---

## 1. Clone & toolchain (Windows)

```powershell
cd C:\Users\wac-2\dev
# Prefer SSH once keys work; HTTPS unblocks faster:
git clone https://github.com/mileypewpew/mega-waga.git
# or: git clone git@github.com:mileypewpew/mega-waga.git
cd mega-waga

# Rust: rustup, NOT ancient MSVC-only / old cargo
# https://rustup.rs
rustc --version   # want recent stable
cargo --version

cargo test --workspace
cargo run -p waga-tui -- status
```

**If `Permission denied (publickey)`:** add SSH key at https://github.com/settings/keys or use HTTPS + PAT.

**PATH tip:** use rustup’s cargo (`%USERPROFILE%\.cargo\bin`) first.

---

## 2. What exists (crates)

| Crate | Role |
|-------|------|
| `waga-core` | `WorldSnapshot`, `Event`, `Story`, `Memory`, XP types |
| `waga-events` | Append-only `events.jsonl`, projection, story open/close |
| `waga-memory` | Classified memories + **park** skill XP (beneficiary A, not per-persona) |
| `waga-world` | Sensors (clock, git), `run_tick` / `run_tick_with` |
| `waga-character` | Persona templates + memory aside in notices |
| `waga-pet` | Mood sprites |
| `waga-voice` | TTS: **xAI · OpenAI · ElevenLabs**, auto + fallback |
| `waga-media` | Now playing + control via **playerctl** (MPRIS) |
| `waga-music` | **MusicDirector** + SuperCollider OSC bed (Lyria = later backup) |
| `waga-tui` | Binary `waga`: tick, status, events, stories, memories, skills, say, now, music, pet |

**Persistence (local only, gitignored):** `.waga/`  
`events.jsonl` = ground truth · `world.json` / `skills.json` = caches · `memories.jsonl` · `stories.json` · `music_session.json` · `voice.toml` optional

---

## 3. Commands that should work

```bash
# Park
cargo run -p waga-tui -- tick
cargo run -p waga-tui -- tick --no-voice
cargo run -p waga-tui -- status
cargo run -p waga-tui -- events --last 20
cargo run -p waga-tui -- stories
cargo run -p waga-tui -- memories
cargo run -p waga-tui -- skills

# Demo memory+XP: dirty tree → tick → clean → tick
# (git dirty/clean; .waga paths ignored in dirty detection)

# Voice (needs API keys + audio player: ffplay/mpv/mpg123)
export XAI_API_KEY=...          # or OPENAI_API_KEY / ELEVENLABS_API_KEY
# Windows: $env:XAI_API_KEY="..."
cargo run -p waga-tui -- voice-config
# copy examples/voice.toml → .waga/voice.toml (set elevenlabs.voice_id if using EL)
cargo run -p waga-tui -- say "Hello from Mega Waga"

# Media (needs playerctl + a MPRIS player on Linux; Windows MPRIS may need different path later)
cargo run -p waga-tui -- now
cargo run -p waga-tui -- music toggle

# HumanMusic bed (needs SuperCollider sclang + assets/sc/waga_bed.scd)
cargo run -p waga-tui -- music bed start
cargo run -p waga-tui -- music bed status
cargo run -p waga-tui -- music bed stop

# Pet TUI
cargo run -p waga-tui -- pet
# keys: t tick · space media · n/p track · q quit
```

---

## 4. Product decisions (locked)

| Decision | Choice |
|----------|--------|
| North star | **Park for life** — real world is the park; light narrative |
| Ground truth | **Event log** (`events.jsonl`) |
| XP | **Park / user sheet (A)** — not per-persona |
| Memories | **Classified** (Episodic, Semantic, …); paired with cited XP |
| Stack | Greenfield Rust, Grok Build–aligned (not full monorepo clone) |
| Voice | Tri-provider TTS, **notify-first** (not full duplex realtime yet) |
| Music | **D first:** SuperCollider live bed; **E backup:** Lyria RealTime later |
| Mega Waga vision | Always-on world layer; notify via voice / HA / messaging; bridge to **Grok Build** while coding |

---

## 5. Specs & plans (read when touching a area)

| Doc | Topic |
|-----|--------|
| `AGENTS.md` | How to work in this repo |
| `roadmap.md` | Phases / status |
| `docs/superpowers/specs/2026-07-16-tick-kernel-v0-design.md` | Tick + pet v0 |
| `docs/superpowers/specs/2026-07-16-event-log-spine-design.md` | Event spine |
| `docs/superpowers/specs/2026-07-16-memory-xp-design.md` | Memory + XP |
| `docs/superpowers/specs/2026-07-17-voice-notify-design.md` | TTS notify |
| `docs/superpowers/specs/2026-07-17-media-music-design.md` | Media + HumanMusic |
| `docs/superpowers/plans/*` | Implementation plans where present |
| `assets/sc/waga_bed.scd` | SuperCollider OSC bed |
| `examples/voice.toml` | Voice config example |

---

## 6. Recent git history (high level)

```
5326fce chore: drop unused Path import
12b53ff feat: now-playing + SuperCollider HumanMusic bed
df50472 feat: tri-provider TTS (xAI, OpenAI, ElevenLabs)
cf0f205 feat: status, pet growth, memory-aware notices
c1f5ca3 feat: classified memories + park XP
2453dcd feat: event log spine
74546ee feat: tick kernel + Waga pet
```

---

## 7. Known gaps / platform notes

- **Windows:** `playerctl` / MPRIS is Linux-centric — media control may need a Windows backend later (not blocking core park).  
- **SuperCollider / playerctl** optional; park core works without them.  
- **Always-on daemon v0:** `waga daemon` (interval ticks, `daemon.json`, `notify.jsonl`); not a Windows service yet.  
- **No Home Assistant / messaging / A2A bridge** yet (designed direction only).  
- **No Grok Build integration** yet (world digest / “agent blocked” notify).  
- Clean-only ticks do **not** grant XP — need dirty→clean story arc.  
- Voice silent if no API keys (warns, does not fail tick).

---

## 8. Suggested next work (priority)

1. **Local verify** — `cargo test`, dirty/clean demo, optional voice keys  
2. ~~**Always-on daemon**~~ — v0 shipped (`waga daemon` + notify bus)  
3. **`MusicBackend` trait** — SC primary, Lyria RealTime adapter later  
4. **Windows media** if needed (or skip until Linux always-on)  
5. **Notify channels** — HA / Telegram sharing same decisions as voice  
6. **Bridge to Grok Build** — world blurb in / status out (file, MCP, or A2A)  
7. **STT converse** — after notify TTS feels solid  

---

## 9. How to hand off *to* Grok Build on the laptop

Paste or attach this file, plus:

```text
Repo: mileypewpew/mega-waga on main.
Continue Mega Waga. Read AGENTS.md and docs/handoffs/2026-07-17-local-dev-grok-build.md.
Do not rewrite the event spine. Prefer small milestones. Next: <user picks>.
```

Optional: open the repo root as the project so Grok Build loads `AGENTS.md`.

---

## 10. A2A vs this handoff (for the human)

**This document + git** solves: *another machine / another session continues the same codebase and decisions.*

**A2A (agent-to-agent)** would solve: *live agents talk* — e.g. Mega Waga daemon ↔ Grok Build mid-session (“build blocked”, “inject world snapshot”, “start music bed”). That is **not required** to clone and keep coding, but it **is** a natural Mega Waga feature so handoffs become *runtime messages*, not only markdown files.

Until A2A exists: **git push/pull + this handoff + AGENTS.md** is the protocol.

---

## 11. Definition of “session start success” on local

- [ ] `git clone` / `git pull` on `main`  
- [ ] `cargo test --workspace` green  
- [ ] `waga tick` + `waga status` work  
- [ ] User can dirty→clean and see memories/skills  
- [ ] Agent has read this handoff and `AGENTS.md`  

---

*End of handoff. Build something cool; keep it bite-sized.*

# Grok Build ↔ Mega Waga bridge (file v0)

**Date:** 2026-07-19  
**Status:** Implemented (file bus; not A2A/MCP yet)

## Idea

Until live A2A exists, the two agents share a **folder**:

| Direction | Path | Who writes |
|-----------|------|------------|
| Park → Build | `.waga/bridge/world.md` (+ `world.json`) | WAGA (`tick`, `daemon`, `bridge export`) |
| Build → Park | `.waga/bridge/inbox.jsonl` | Grok Build (or `waga bridge post`) |

## For Grok Build (this machine)

1. Open the mega-waga repo (so `AGENTS.md` loads).
2. Read the park blurb when starting a coding session:

```powershell
# from repo root
Get-Content .waga\bridge\world.md
# or:
cargo run -p waga-tui -- bridge status
```

3. When Build is blocked or has a status to share, append one JSON line:

```powershell
cargo run -p waga-tui -- bridge post "cargo test failed on waga-world" --kind blocked
# or --kind status / note
```

Equivalent file append to `.waga/bridge/inbox.jsonl`:

```json
{"at":"2026-07-19T15:00:00+02:00","source":"grok-build","kind":"blocked","text":"cargo test failed","session":null}
```

4. Optional: keep the park warm in another terminal:

```powershell
cargo run -p waga-tui -- daemon --every 60 --quiet --no-voice
```

Daemon refreshes `world.md` after every tick.

## For humans

```text
waga bridge export    # force-write world.md / world.json
waga bridge status    # blurb + last inbox lines
waga bridge inbox     # list Build → park messages
waga bridge post "…"  # simulate Build
```

## Non-goals v0

- MCP server  
- Realtime A2A  
- Auto-inject into Grok Build context without reading the file  
- Mutating Build from park (Build pulls; park does not push network)

## Next

- Optional: WAGA speaks inbox `blocked` via TTS notify  
- Optional: MCP tool `waga_world` wrapping the same files  
- Later: true A2A

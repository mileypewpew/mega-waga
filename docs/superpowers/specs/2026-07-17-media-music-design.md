# Media + HumanMusic v1 (path D)

**Status:** Implementing  
**Date:** 2026-07-17  

## Goal

1. **Now playing** — show & control system media (MPRIS via `playerctl`)  
2. **HumanMusic bed** — SuperCollider live bed steered by park events  
3. Lyria RealTime = backup plan (not v1)

## Commands

```text
waga now              # what's playing
waga music play|pause|next|prev|toggle
waga music bed start|stop|status|steer
```

## MusicDirector params

tension 0–1, bpm, mood tag, story_open bool → OSC to SuperCollider

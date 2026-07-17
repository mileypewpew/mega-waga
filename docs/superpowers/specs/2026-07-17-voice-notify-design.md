# Voice Notify v1 + Mega Waga sketch

**Status:** Implementing (tri-provider TTS, notify-first)  
**Date:** 2026-07-17  

## Goal

Premium **text-to-speech notify** when the park has something worth saying.  
Backends: **xAI · OpenAI · ElevenLabs** behind one router. Realtime duplex later.

## Speak on

- StoryOpened  
- StoryClosed  
- XpGranted (short)  
- Manual `waga say "…"`

Not every clean tick.

## Config

`.waga/voice.toml` or `~/.config/waga/voice.toml` + env keys  
`XAI_API_KEY`, `OPENAI_API_KEY`, `ELEVENLABS_API_KEY`

## Non-goals v1

Full duplex voice agent, HA, messaging (next Mega slices).

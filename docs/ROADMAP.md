# Roadmap: beyond questions and answers

Ideas for making the assistant *do* things, not only answer questions. Most of them build
on **tool calling**: the LLM (`gemma4:12b` supports it) is given a list of actions with
their parameters, decides which one a request needs, and the assistant runs it.

Status: ✅ done · 🔜 planned / easy next step · 💡 idea

## Foundation

| | Feature | Notes |
|---|---|---|
| ✅ | Tool calling | Ollama `/api/chat` with a tool list; the model picks actions and arguments |

## Works with tools already on a typical desktop

| | Feature | Example | Needs |
|---|---|---|---|
| ✅ | Timers | "Computer, set a timer for 10 minutes" / "cancel the timer" | — |
| ✅ | Reminders | "Remind me at 3 pm to call Paul" (spoken + desktop notification, survives restarts) | `notify-send` |
| ✅ | Status report | "Computer, status report" → battery, CPU, memory, disk, uptime | — |
| ✅ | Volume control | "Set volume to 40 percent", "louder", "mute" | `wpctl` (PipeWire), else `pactl` (PulseAudio) |
| ✅ | Captain's log | "Begin captain's log" … "end log"; "read back today's log" | — |
| 🔜 | Notes and dictation | "Take a note: buy coffee filters", "what are my notes?" | — |
| 🔜 | Open things | "Open my downloads folder", "open GitHub" | `xdg-open` |
| 💡 | Low-battery warning | Speaks up on its own when the battery runs low | — |

## Star Trek touches

| | Feature | Notes |
|---|---|---|
| ✅ | Stardates | Captain's log entries are dated `YEAR.DAY-OF-YEAR` (e.g. 2026.287) |
| ✅ | Computer chirp | Synthesized TNG-style blip instead of "I'm listening" (`tts.listening_sound`) |
| 💡 | Processing chirp | Same for "Processing…" |
| 💡 | Red alert | Klaxon sound on command |
| ✅ | Kokoro voices | `tts.engine = "kokoro"`: 28 natural-sounding English voices (default `af_sky`) |
| 💡 | Fan computer voices | Community Piper voices resemble the TNG computer (e.g. *fedcomp*, *ships_computer*); unofficial, check their terms |

## Needs a small install or setup

| | Feature | Example | Needs |
|---|---|---|---|
| 💡 | Media control | "Pause the music", "next track", "what's playing?" | `playerctl` |
| 💡 | Web search | "Search for the latest ICC news" (already wired in; summarized by the LLM) | `~/startrek/search_tool.py` |
| 💡 | Calendar | "What's on my calendar tomorrow?" | `khal` or `gcalcli` |
| 💡 | Smart home | "Lights to 30 percent", "is the front door locked?" | Home Assistant API |

## Bigger ideas

| | Feature | Notes |
|---|---|---|
| 💡 | Vision | "What am I holding?", "read this label": take a webcam photo (`/dev/video0`) and ask gemma4, which accepts images |
| 💡 | Follow-up questions | Keep listening ~5 s after an answer so "and what about Germany?" works without the wake word |
| 💡 | Saying "unknown" | Stricter prompting so it admits not knowing instead of inventing answers |
| 💡 | Shorter acknowledgements | Skip "You have requested… Processing…" for quick actions like timers and volume |

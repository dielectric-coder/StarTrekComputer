# Desktop Setup (PipeWire, per-user service)

This guide covers running the assistant on a Linux desktop as your own user, with
audio through PipeWire/PulseAudio, instead of the `/opt/startrek` server install
described in [INSTALL.md](INSTALL.md). It also records the fixes that were needed to
get the Rust assistant building and working end to end (commit `134dca3`).

Tested on Manjaro (kernel 6.18), Rust stable, PipeWire, Ollama 0.40 with `gemma4:12b`.

## Quick start

```bash
cd voice-assistant
cargo build --release
./target/release/voice-assistant
```

Wait for `Wake word detector started, listening for 'computer'`, then speak.
Stop with Ctrl-C.

## Talking to it

- **One breath:** "Computer, what is the speed of light?" It confirms the request
  ("You have requested: …"), says "Processing…", and answers.
- **Two steps:** "Computer" … *"I'm listening"* … then ask your question.

After answering it returns to waiting for the wake word.

- Pause after speaking. About 0.8 s of silence ends an utterance.
- The microphone is muted while the assistant talks, so let it finish first.
- Transcription takes a second or two after you stop talking.
- Requests containing "search", "look up" or "find information" try the web search
  script (`~/startrek/search_tool.py`). If it isn't installed they fall back to the LLM.

## Voice commands

Besides answering questions, the assistant can run actions. The LLM picks the right one
through Ollama tool calling, so the wording doesn't have to be exact.

| Feature | Say, for example | Result |
|---|---|---|
| Timer | "Computer, set a timer for 10 minutes for the tea" | "Timer set for 10 minutes: tea." … later "Timer complete: tea." plus a desktop notification |
| Reminder | "Computer, remind me at 15:00 to call Paul" / "remind me in 20 minutes to check the oven" | Spoken and shown as a notification when due |
| List | "Computer, what timers are active?" | Remaining time of each timer, and reminder times |
| Cancel | "Computer, cancel the tea timer" / "cancel all timers" | "Cancelled." |
| Status report | "Computer, status report" / "how much battery is left?" | Battery, CPU load and temperature, memory, disk, uptime |
| Volume | "Computer, set the volume to 40 percent" / "louder" / "quieter" / "mute" | "Volume 40 percent." |
| Captain's log | "Computer, begin captain's log" … dictate … "End log." | "Captain's log, stardate 2026.287. Recording." … "Log entry recorded, 15 words." |
| Read the log | "Computer, read back today's captain's log" / "yesterday's log" | Reads each entry with its stardate |

- **Reminders survive restarts.** They're stored in `~/startrek/.voice-assistant/reminders.json`.
  Any that came due while the assistant was off are announced at startup. Timers are
  memory-only.
- **Captain's log** entries are saved as one Markdown file per day in
  `~/startrek/.voice-assistant/captains_log/YYYY-MM-DD.md`. While recording, the wake word
  isn't needed and everything you say is written down. Finish with "end log" (or
  "end of log", "stop recording"). It only counts at the end of a sentence, so the words
  can still appear in dictation. A recording stops on its own after 60 s of silence.
- **Stardates** are year and day of year (`2026.287`), as in the 2009 Star Trek film.
- Answers from actions aren't stored as RAG knowledge.

The full list of ideas for further features is in [ROADMAP.md](ROADMAP.md).

## Prerequisites

| Component | Location | How to install |
|---|---|---|
| Rust toolchain | — | `rustup` |
| PipeWire/PulseAudio tools | `parecord`, `aplay` | `pipewire-pulse`, `alsa-utils` |
| whisper.cpp (`whisper-cli`) | `~/startrek/whisper.cpp/build/bin/whisper-cli` | see below |
| Whisper model | `~/startrek/whisper.cpp/models/ggml-base.en.bin` | see below |
| Piper (Python `piper-tts`) | on `PATH`, e.g. `~/.local/bin/piper` | `pip install --user piper-tts` |
| Piper voice | `~/startrek/piper/voices/en_US-lessac-medium.onnx` (+ `.onnx.json`) | see below |
| Ollama ≥ 0.30.9 + a model | `http://localhost:11434` | `ollama pull gemma4:12b` |

### whisper.cpp

```bash
cd ~/startrek
git clone --depth 1 https://github.com/ggml-org/whisper.cpp.git
cd whisper.cpp
cmake -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DWHISPER_BUILD_TESTS=OFF
cmake --build build -j"$(nproc)" --target whisper-cli
sh ./models/download-ggml-model.sh base.en
```

### Piper voice

```bash
python -m piper.download_voices --download-dir ~/startrek/piper/voices en_US-lessac-medium
```

## Configuration

`~/startrek/.voice-assistant/config.toml` is created with defaults on first run.
Restart the assistant after editing it.

| Setting | Current value | Meaning |
|---|---|---|
| `[llm] model` | `"gemma4:12b"` | Any model pulled in Ollama (see [Choosing a model](#choosing-a-model)) |
| `[llm] think` | `false` (default) | Let reasoning models think before answering. 20–45 s per answer on this hardware |
| `[tts] speed` | `0.77` | Speech rate; 1.0 = the voice's natural pace, lower = slower |
| `[tts] piper_binary` | `"piper"` (default) | Piper executable; a bare name is looked up on `PATH` |
| `[stt] whisper_binary` | `~/startrek/whisper.cpp/build/bin/whisper-cli` (default) | whisper.cpp CLI |
| `[audio] vad_threshold` | `0.015` (default) | Minimum RMS treated as speech. Lower it if quiet speech is missed, raise it if noise triggers it |
| `[audio] input_device` | unset (system default) | A source name from `pactl list short sources` |

`piper_binary`, `whisper_binary`, `vad_threshold` and `think` are optional, so older config
files keep working.

## Choosing a model

Benchmarked on an i7-11800H with a 4 GB RTX A2000 Laptop GPU, using the assistant's
own prompt and 24 general-knowledge questions:

| Model | Correct | Median answer time | Notes |
|---|---|---|---|
| `gemma3:4b` | 22/24 | 0.6 s | Missed the 2nd UN Secretary-General; said the Euphrates flows through Baghdad |
| **`gemma4:12b`**, thinking off | **24/24** | 1.6 s | Current choice. 8 GB, mostly on CPU, ~13 tokens/s |
| `gemma4:12b`, thinking on | same answers | 22–45 s | Too slow for voice. Keep `think = false` |

`gemma4` needs Ollama 0.30.9 or newer. Older versions fail the pull with
`412: requires a newer version of Ollama`. A cold model load takes 20–40 s, which is
why the assistant preloads the model at startup and keeps it loaded for 30 minutes.

## Running at login (systemd user service)

The unit in [`systemd/startrek-computer.service`](../systemd/startrek-computer.service)
expects `/opt/startrek` and a dedicated `startrek` user, which cannot reach a desktop
user's PipeWire session. On a desktop, use a **user** service instead,
`~/.config/systemd/user/startrek-computer.service`:

```ini
[Unit]
Description=Star Trek Computer voice assistant
After=pipewire.service pipewire-pulse.service
Wants=pipewire-pulse.service

[Service]
Type=simple
ExecStart=%h/GitSpace/StarTrekComputer/voice-assistant/target/release/voice-assistant
WorkingDirectory=%h/GitSpace/StarTrekComputer/voice-assistant
# piper lives in ~/.local/bin, which isn't on systemd's default PATH
Environment=PATH=%h/.local/bin:/usr/local/bin:/usr/bin:/bin
Restart=on-failure
RestartSec=5
# Ctrl-C equivalent, so the assistant shuts down cleanly
KillSignal=SIGINT
TimeoutStopSec=10

[Install]
WantedBy=default.target
```

Adjust the paths to where you cloned the repo. Then:

```bash
systemctl --user daemon-reload
systemctl --user enable --now startrek-computer
```

| Task | Command |
|---|---|
| Stop / start | `systemctl --user stop startrek-computer` / `start` |
| Apply config changes or a new build | `systemctl --user restart startrek-computer` |
| Disable auto-start | `systemctl --user disable startrek-computer` |
| Watch what it hears and says | `journalctl --user -u startrek-computer -f` |

Don't run `./target/release/voice-assistant` by hand while the service is running,
or both copies will answer. After `cargo build --release`, restart the service; until
then it keeps running the old binary.

## Memory database

Conversations are stored in `~/startrek/.voice-assistant/rag_memory.db`:

```bash
./target/release/rag-manager stats
./target/release/rag-manager conversations
./target/release/rag-manager search <query>
./target/release/rag-manager cleanup
```

## What was fixed

### Build

- **`candle-core` 0.3 failed to compile** with ~20 `rand_distr` trait errors.
  `half` ≥ 2.5 implements its `f16` sampling for `rand_distr` 0.5 / `rand` 0.9,
  but candle 0.3 uses `rand_distr` 0.4. `half` is pinned to 2.4.1 in the now-committed
  `Cargo.lock` (`cargo update -p half --precise 2.4.1`). Upgrading candle is the
  long-term fix.
- **`rag-manager` never compiled.** Added `src/lib.rs` exposing `config` and `rag`,
  added the missing `clap` dependency, and fixed invalid `println!("=" .repeat(50))` calls.

### Runtime

- **RAG database:** `unable to open database file` on first run. sqlx doesn't create
  SQLite files by default. Now the data directory and database are created if missing.
- **Microphone input was a stub** that never read audio. `src/audio.rs` now runs
  `parecord` (16 kHz mono s16le) and cuts the stream into utterances: 30 ms frames,
  300 ms pre-roll, 0.8 s of silence ends an utterance, and anything with under
  250 ms of speech is discarded.
- **VAD** (`src/vad.rs`): the fixed threshold of 0.01 on mean energy (an RMS of 0.1)
  was about 10× above this laptop mic's level. It is now an RMS detector that
  follows the background noise floor (threshold = max(`vad_threshold`, 3 × noise floor)).
- **Wake word:** whisper now runs once per utterance instead of on a rolling buffer.
  "Computer, \<command\>" is handled immediately, and "Computer" alone switches to
  listening mode.
- **Self-hearing:** capture is paused while the assistant speaks.
- **Speech-to-text:** the hardcoded `/home/edward/startrek/whisper.cpp/main` path was
  replaced by `stt.whisper_binary`. Startup fails with a clear message if the binary
  or model is missing, and annotations like `[BLANK_AUDIO]` are stripped.
- **Text-to-speech:** the hardcoded `/home/edward/startrek/piper/piper/piper` path was
  replaced by `tts.piper_binary`. The Python piper writes nothing to stdout with
  `--output_file -`, so it now uses `--output-raw` and plays at the sample rate in the
  voice's `.onnx.json`. `tts.speed` was ignored before (length scale hardcoded to 0.9);
  now length scale = 1 / speed.
- **LLM:** the 30 s request timeout was shorter than a cold model load plus generation.
  It is now 120 s, the model is preloaded at startup, and Ollama is asked to keep it
  loaded for 30 minutes.
- **Thinking models:** requests send `think` from the config (default `false`), since
  `gemma4` thinks by default and would take 20–45 s per answer.
- **Wrong dates ("Fourteenth of Paris"):** the prompt never included the date, and RAG
  stored bare answers ("Affirmative. Paris.") under the catch-all `general` topic,
  which were then injected into almost every prompt. Now the current local date and
  time are in every prompt, knowledge is stored as "Q: … A: …", time-sensitive questions
  (date, time, weather, news, …) aren't stored, and `general` no longer matches everything.
- **Shutdown:** Ctrl-C logged "Shutting down" but the process kept running. It now exits.

## Testing without speaking

Speech can be fed through a virtual PipeWire sink whose monitor acts as the mic:

```bash
espeak-ng -s 140 -w /tmp/cmd.wav "Computer, what is the capital of France?"
MOD=$(pactl load-module module-null-sink sink_name=va_test)
PULSE_SOURCE=va_test.monitor ./target/release/voice-assistant &
sleep 3; paplay --device=va_test /tmp/cmd.wav
# ... watch the log, then:
kill -INT %1; pactl unload-module "$MOD"
```

Expected log:

```
Whisper transcription: Computer, what is the capital of France?
Speaking: You have requested: what is the capital of France?
Speaking: Processing...
LLM response: Affirmative. Paris.
```

Stop the service first so it doesn't answer too.

## Known gaps

- Every command is preceded by "You have requested… Processing…", which adds about 7 s
  before quick actions like timers and volume changes.

- `AudioManager::play_audio` is still a stub. All speech goes through piper + `aplay`.
- `~/startrek/search_tool.py` isn't installed, so search requests fall back to the LLM.
- The wake word is fixed to "computer". `[wake_word] wake_word` and `sensitivity` are
  not used.
- If the TTS step fails, the request stops before reaching the LLM.
- Only tested with synthetic (espeak) speech and room-noise checks so far, not with a
  live human voice.

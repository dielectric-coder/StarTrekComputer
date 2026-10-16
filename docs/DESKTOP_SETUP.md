# Desktop Setup (PipeWire, per-user service)

This guide covers running the assistant on a Linux desktop as your own user, with
audio through PipeWire/PulseAudio, instead of the `/opt/startrek` server install
described in [INSTALL.md](INSTALL.md). It also records the fixes that were needed to
get the Rust assistant building and working end to end (commit `134dca3`).

Tested on two machines:

| Machine | OS | Hardware | Ollama | Model |
|---|---|---|---|---|
| Laptop 1 | Manjaro (kernel 6.18) | i7-11800H, 4 GB RTX A2000 Laptop GPU | 0.40 | `gemma4:12b` |
| Laptop 2 | CachyOS (kernel 7.2) | Ryzen AI 9 HX 370, Radeon 890M iGPU, 86 GB RAM | 0.30.7 | `gemma4:26b` |

Both use Rust stable, PipeWire and the repo at `~/Workspace/GitSpace/StarTrekComputer`
(laptop 1 had it at `~/GitSpace/StarTrekComputer`).

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
- **Two steps:** "Computer" … *chirp* … then ask your question.
- **Changed your mind?** After "You have requested: …" it waits 3 seconds before acting.
  Say "negative" to call it off. While it's thinking, "stop" does the same. Either way
  it answers "Request canceled." Both words work in both phases, as do "cancel",
  "abort", "belay that" and "never mind". The word has to be said on its own.

After answering it returns to waiting for the wake word.

- Pause after speaking. About 0.8 s of silence ends an utterance.
- The microphone is muted while the assistant talks, so let it finish first. That also
  means "stop" can't cut off an answer that is already being spoken.
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
| Piper (Python `piper-tts`) | on `PATH`, e.g. `~/.local/bin/piper` | `uv tool install piper-tts` (or `pip install --user piper-tts`) |
| Piper voice | `~/startrek/piper/voices/en_US-lessac-medium.onnx` (+ `.onnx.json`) | see below |
| Kokoro (optional, more natural voice) | `~/.local/share/kokoro-models/` | see below |
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
~/.local/share/uv/tools/piper-tts/bin/python -m piper.download_voices --download-dir ~/startrek/piper/voices en_US-lessac-medium
```

### Kokoro voices (optional)

Kokoro sounds more natural than Piper and is about as fast once loaded. It needs the
`kokoro-onnx` Python package and the model files:

```bash
uv tool install kokoro-tts      # provides a Python with kokoro-onnx
mkdir -p ~/.local/share/kokoro-models && cd ~/.local/share/kokoro-models
wget https://github.com/nazdridoy/kokoro-tts/releases/download/v1.0.0/kokoro-v1.0.onnx
wget https://github.com/nazdridoy/kokoro-tts/releases/download/v1.0.0/voices-v1.0.bin
```

Then set in `config.toml`:

```toml
[tts]
engine = "kokoro"
voice = "af_sky"     # af_* American, bf_* British; run `kokoro-tts --help-voices` for all
speed = 1.0
kokoro_python = "/home/mikel/.local/share/uv/tools/kokoro-tts/bin/python3"
```

The assistant runs a small helper (embedded in the binary, written to the data directory)
that keeps the model loaded, so each sentence takes about 1 s to synthesize. A wrong
voice name is reported at startup with the list of available voices.

## Configuration

`~/startrek/.voice-assistant/config.toml` is created with defaults on first run.
Restart the assistant after editing it.

| Setting | Current value | Meaning |
|---|---|---|
| `[llm] model` | `"gemma4:12b"` (laptop 1), `"gemma4:26b"` (laptop 2) | Any model pulled in Ollama (see [Choosing a model](#choosing-a-model)) |
| `[llm] think` | `false` (default) | Let reasoning models think before answering. 20–45 s per answer on this hardware |
| `[tts] engine` | `"kokoro"` | `"piper"` or `"kokoro"` |
| `[tts] voice` | `"af_sky"` | Kokoro voice name (Piper uses `model_path` instead) |
| `[tts] speed` | `1.0` | Speech rate; 1.0 = the voice's natural pace, lower = slower (0.77 suited lessac) |
| `[tts] listening_sound` | `"chirp"` (default) | Played after "Computer" on its own: `"chirp"` (a synthesized TNG-style blip), `"voice"` (says "I'm listening") or a path to a WAV file |
| `[tts] piper_binary` | `"piper"` (default) | Piper executable; a bare name is looked up on `PATH`. If `piper` isn't on `PATH`, the default is the standalone binary that `scripts/install.sh` unpacks (`piper/piper/piper`) |
| `[stt] whisper_binary` | `~/startrek/whisper.cpp/build/bin/whisper-cli` (default) | whisper.cpp CLI |
| `[audio] vad_threshold` | `0.015` (default), `0.006` on laptop 2 | Minimum RMS treated as speech. Lower it if quiet speech is missed, raise it if noise triggers it |
| `[audio] input_device` | unset (system default) | A source name from `pactl list short sources` |

`piper_binary`, `listening_sound`, `kokoro_python`, `kokoro_model`, `kokoro_voices`, `whisper_binary`, `vad_threshold` and `think` are optional, so older config
files keep working.

## If it doesn't react to "Computer"

If `journalctl --user -u startrek-computer -f` shows no `Whisper transcription` lines
when you speak, your voice is probably below the speech threshold. The assistant only
treats audio as speech above an RMS of max(`vad_threshold`, 3 × background noise).

1. Check that the right mic is the default and isn't muted:

   ```bash
   pactl get-default-source
   pactl list sources | grep -E 'Name:|Mute:|Volume:'
   ```

2. Measure your speech level. Run this and say "Computer, what time is it?" a few
   times within 15 seconds:

   ```bash
   timeout 15 parecord --raw --format=s16le --rate=16000 --channels=1 /tmp/voice.raw
   python3 - /tmp/voice.raw <<'PY'
   import sys, array, math
   a = array.array('h', open(sys.argv[1], 'rb').read())
   r = sorted(math.sqrt(sum(x*x for x in a[i:i+480]) / 480) / 32768
              for i in range(0, len(a) - 480, 480))
   print(f"noise (median) {r[len(r)//2]:.4f}  speech (95th percentile) {r[int(len(r)*.95)]:.4f}")
   PY
   ```

   Speech should be comfortably above the threshold, and the threshold above 3 × noise.

3. Raise the mic gain, lower `vad_threshold`, or both, then restart the service.

On laptop 2 the built-in digital mic was at 72%. Speech peaked at about 0.007 RMS
(the 95th percentile) against the default threshold of 0.015, so nothing was ever
detected. Room noise was about 0.0014. The fix was:

```bash
pactl set-source-volume @DEFAULT_SOURCE@ 100%     # about 2.7× louder
sed -i 's/^vad_threshold = .*/vad_threshold = 0.006/' ~/startrek/.voice-assistant/config.toml
systemctl --user restart startrek-computer
```

The mic is muted while the assistant speaks, so wait for it to finish before trying
again. Repeating yourself during its answer looks like it isn't listening.

## Choosing a model

### Laptop 1 (i7-11800H, RTX A2000)

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

### Laptop 2 (Ryzen AI 9 HX 370, Radeon 890M)

Ollama here is 0.30.7, so `gemma4:12b` can't be pulled. `gemma4:26b` was already
installed and runs fine:

| Model | Placement | Cold answer | Warm answer | Speed |
|---|---|---|---|---|
| **`gemma4:26b`**, thinking off | 17 GB, 100% on the iGPU | 9.3 s | 0.9 s | ~21 tokens/s |

These are from two test prompts plus one end-to-end voice test, not the 24-question
benchmark above. With 86 GB of RAM shared with the iGPU, the memory pressure described
under [Known gaps](#known-gaps) doesn't apply.

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
ExecStart=%h/Workspace/GitSpace/StarTrekComputer/voice-assistant/target/release/voice-assistant
WorkingDirectory=%h/Workspace/GitSpace/StarTrekComputer/voice-assistant
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

- **Slow answers under memory pressure.** gemma4 (8 GB) runs from a memory-mapped file.
  With no swap and little free RAM, Linux evicts parts of it and answers slow from about
  2 s to 20 s (1 token/s). `ollama stop gemma4:12b` (the assistant reloads it) restores
  normal speed. Check with `free -h`: "available" should stay well above 8 GB.

- Every command is preceded by "You have requested… Processing…", which adds about 7 s
  before quick actions like timers and volume changes.

- `AudioManager::play_audio` is still a stub. All speech goes through piper + `aplay`.
- `~/startrek/search_tool.py` isn't installed, so search requests fall back to the LLM.
- The wake word is fixed to "computer". `[wake_word] wake_word` and `sensitivity` are
  not used.
- If the TTS step fails, the request stops before reaching the LLM.
- Only tested with synthetic (espeak) speech and room-noise checks so far, not with a
  live human voice.

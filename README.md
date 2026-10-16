# 🖖 Star Trek Enterprise Computer Voice Assistant

> **About this fork**
>
> This is a fork of [oakeley/StarTrekComputer](https://github.com/oakeley/StarTrekComputer)
> by Edward ([@oakeley](https://github.com/oakeley)), who created the project. All credit
> for the original idea, design and code goes to him.
>
> The fork was made to get the assistant working on **Arch-based Linux** (Manjaro,
> CachyOS) as a per-user service with PipeWire audio. Along the way it fixes the build,
> real microphone input, speech-to-text and text-to-speech, and adds Kokoro voices and
> voice commands (timers, reminders, status report, volume, captain's log). The original
> Jetson Nano install path is kept working.
>
> 👉 Desktop setup and the full list of changes:
> [docs/DESKTOP_SETUP.md](docs/DESKTOP_SETUP.md)
>
> **Built on:** [whisper.cpp](https://github.com/ggml-org/whisper.cpp) (speech recognition),
> [Piper](https://github.com/rhasspy/piper) and
> [Kokoro](https://huggingface.co/hexgrad/Kokoro-82M) via
> [kokoro-onnx](https://github.com/thewh1teagle/kokoro-onnx) (speech synthesis), and
> [Ollama](https://ollama.com) running Google's [Gemma](https://ai.google.dev/gemma) models.
> Star Trek is a trademark of CBS Studios / Paramount; this is an unofficial fan project.
>
> Released under the [MIT License](LICENSE), like the original.

A voice-activated AI assistant that emulates the iconic Star Trek Enterprise computer, complete with fact-checking capabilities and the distinctive computer voice style from the original series.

> **"Computer. Status report."**

## ✨ Features

- 🎯 **Wake Word Activation**: Say "Computer" to activate (just like in Star Trek!)
- 🧠 **Enterprise AI Personality**: Responds in the clipped, precise style of the ship's computer
- 🗣️ **Voice Confirmation**: Echoes back your request before processing; say "negative" (or "stop" while it's thinking) to cancel
- 🔍 **Fact-Checking Integration**: Searches Bing and verifies information against trusted sources
- 🔊 **Authentic TTS**: Computer-like voice synthesis optimized for Star Trek responses
- 🤖 **RAG Learning System**: Remembers conversations and adapts to user preferences
- 💾 **Memory & Context**: Builds knowledge base and maintains conversation context
- 📱 **Jetson Nano Optimized**: Designed for ARM64 systems with resource optimization
- 🚀 **Auto-Start on Boot**: Runs as a system service from startup

## 🎬 Demo

### First Interaction
```
You: "Computer, what is the distance from Earth to Mars?"
Computer: "You have requested: what is the distance from Earth to Mars?"
Computer: "Processing..."
Computer: "Distance from Earth to Mars ranges from 54.6 million to 401 million kilometers, depending on orbital positions. Average distance is 225 million kilometers."
```

### Later Interaction (with RAG learning)
```
You: "Computer, what about Mars again?"
Computer: "You have requested: what about Mars again?"
Computer: "Processing..."
Computer: "Referencing previous discussion: Mars distance varies from 54.6 to 401 million kilometers based on orbital positions. Current approximate distance: 225 million kilometers."
```

## 🚀 Quick Installation

### Automated Installation (Recommended)

```bash
curl -sSL https://raw.githubusercontent.com/dielectric-coder/StarTrekComputer/main/scripts/install.sh | bash
```

### Manual Installation

1. **Clone Repository**
   ```bash
   git clone https://github.com/dielectric-coder/StarTrekComputer.git
   cd StarTrekComputer
   ```

2. **Run Installation Script**
   ```bash
   chmod +x install.sh
   ./install.sh
   ```

3. **Deploy as System Service**
   ```bash
   sudo ./deploy.sh
   ```

## 📋 System Requirements

- **Hardware**: Jetson Nano 16GB (or compatible ARM64 system)
- **OS**: Ubuntu 20.04 LTS or newer
- **RAM**: 8GB+ recommended
- **Storage**: 32GB+ SD card/SSD
- **Audio**: USB microphone + speakers/headphones
- **Network**: Internet connection for setup and fact-checking

## 🏗️ Architecture

```
┌─────────────────┐    ┌──────────────┐    ┌─────────────────┐
│   Microphone    │───▶│  Wake Word   │───▶│ Speech-to-Text  │
│                 │    │  Detection   │    │   (Whisper)     │
└─────────────────┘    └──────────────┘    └─────────────────┘
                                                     │
┌─────────────────┐    ┌──────────────┐    ┌─────────────────┐
│    Speakers     │◀───│ Text-to-Speech│◀───│  Main Control   │
│                 │    │   (Piper)     │    │  (Rust App)     │
└─────────────────┘    └──────────────┘    └─────────────────┘
                                                     │
                                            ┌─────────────────┐
                                            │  LLM + Search   │
                                            │ (Ollama+Bing)   │
                                            └─────────────────┘
```

## 🛠️ Core Components

- **Main Application**: Rust-based voice assistant controller
- **Wake Word Detection**: Custom "Computer" detection using Whisper STT
- **Speech Recognition**: Whisper.cpp for accurate offline STT
- **Language Model**: Ollama with Qwen3:4b for Star Trek personality
- **Text-to-Speech**: Piper TTS with optimized computer voice
- **Fact-Checking**: Python-based web scraping with Bing search
- **System Service**: Systemd integration for auto-startup

## ⚙️ Configuration

The system uses a TOML configuration file located at `/var/lib/startrek-computer/config.toml`:

```toml
[audio]
sample_rate = 16000
channels = 1
buffer_size = 1024

[wake_word]
wake_word = "computer"
sensitivity = 0.5

[llm]
model = "qwen3:4b"
temperature = 0.3  # Lower for more factual responses

[search]
enabled = true
max_results = 3
```

### Pre-configured Templates

- `configs/jetson-nano.toml` - Optimized for Jetson Nano
- `configs/development.toml` - Development/testing setup
- `configs/high-performance.toml` - High-resource systems

## 📖 Usage

### Basic Commands

```bash
# Service Management
sudo systemctl start startrek-computer      # Start service
sudo systemctl stop startrek-computer       # Stop service
sudo systemctl status startrek-computer     # Check status

# View Logs
sudo journalctl -u startrek-computer -f     # Follow logs
sudo /opt/startrek/startrek/logs.sh         # Log viewer script

# Health Check
sudo /opt/startrek/startrek/healthcheck.sh  # System diagnostics

# RAG Memory Management
/opt/startrek/startrek/voice-assistant/target/release/rag-manager stats      # View learning stats
/opt/startrek/startrek/voice-assistant/target/release/rag-manager search "query"  # Search memory
/opt/startrek/startrek/voice-assistant/target/release/rag-manager cleanup --days 30  # Cleanup old data
```

### Voice Interaction

1. Say "**Computer**" (wait for acknowledgment beep)
2. State your question or command
3. System responds with confirmation and processes
4. Hear the response in Enterprise computer voice

### Example Interactions

```
"Computer, what is the current time?"
"Computer, search for information about black holes"
"Computer, what is two plus two?"
"Computer, tell me about the weather"
```

## 🔧 Maintenance

### Health Monitoring
```bash
# Automated health check
/opt/startrek/startrek/healthcheck.sh

# Component testing
curl http://localhost:11434/api/tags        # Test LLM
arecord -d 3 test.wav && aplay test.wav     # Test audio
python3 /opt/startrek/startrek/search_tool.py "test"  # Test search
```

### Updates
```bash
# Update system
/opt/startrek/startrek/update.sh

# Manual rebuild
cd /opt/startrek/startrek/voice-assistant
cargo build --release
sudo systemctl restart startrek-computer
```

## 🐛 Troubleshooting

### Common Issues

**Service won't start:**
```bash
sudo systemctl status startrek-computer
sudo journalctl -u startrek-computer -l
```

**Wake word not detected:**
```bash
# Test microphone
arecord -d 5 test.wav && aplay test.wav
# Check audio settings in config.toml
```

**LLM not responding:**
```bash
# Check Ollama
curl http://localhost:11434/api/tags
sudo systemctl restart ollama
```

**No audio output:**
```bash
# Test TTS
echo "Test" | /opt/startrek/startrek/piper/piper/piper \
  --model /opt/startrek/startrek/piper/voices/en_US-lessac-medium.onnx \
  --output_file - | aplay
```

For detailed troubleshooting, see [TROUBLESHOOTING.md](TROUBLESHOOTING.md).

## 📚 Documentation

- [**INSTALL.md**](INSTALL.md) - Comprehensive installation guide
- [**DEPENDENCIES.md**](DEPENDENCIES.md) - Complete dependency documentation
- [**TROUBLESHOOTING.md**](TROUBLESHOOTING.md) - Problem solving guide
- [**RAG_SYSTEM.md**](RAG_SYSTEM.md) - Learning and memory system documentation
- [**DESKTOP_SETUP.md**](docs/DESKTOP_SETUP.md) - Running on a Linux desktop with PipeWire and a systemd user service
- [**ROADMAP.md**](docs/ROADMAP.md) - Voice commands available now and ideas for more

## 🏗️ Development

### Building from Source

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build voice assistant
cd voice-assistant
cargo build --release

# Install dependencies (see DEPENDENCIES.md)
# Run development version
RUST_LOG=debug cargo run
```

### Project Structure

```
startrek-computer/
├── voice-assistant/          # Main Rust application
│   ├── src/
│   │   ├── main.rs          # Main orchestrator
│   │   ├── wake_word.rs     # Wake word detection
│   │   ├── stt.rs           # Speech-to-text
│   │   ├── llm.rs           # Language model interface
│   │   ├── tts.rs           # Text-to-speech
│   │   ├── search.rs        # Web search integration
│   │   └── config.rs        # Configuration management
│   └── Cargo.toml
├── search_tool.py           # Python search/fact-check script
├── install.sh               # Automated installer
├── deploy.sh               # Service deployment script
├── systemd/                # Systemd service files
├── configs/                # Configuration templates
└── docs/                   # Documentation
```

## 🤝 Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## 👨‍💻 Author

**Edward J. Oakeley** - [GitHub](https://github.com/oakeley)

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🖖 Acknowledgments

- Inspired by the Star Trek universe and the Enterprise computer
- Built with modern AI technologies for an authentic retro-futuristic experience
- Optimized for edge computing on ARM64 devices
- Special thanks to the open-source communities behind Whisper, Ollama, and Piper

---

**Live long and prosper!** 🖖

*"The computer is working perfectly, Captain."*
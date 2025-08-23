# Star Trek Computer Dependencies

This document lists all dependencies required for the Star Trek Computer voice assistant system.

## System Requirements

### Hardware
- **Platform**: Jetson Nano 16GB (ARM64) or compatible
- **RAM**: 8GB+ recommended (16GB for optimal performance)
- **Storage**: 32GB+ SD card or SSD
- **Audio**: USB microphone and speakers/headphones
- **Network**: Internet connection for initial setup and search features

### Operating System
- **OS**: Ubuntu 20.04 LTS or newer
- **Architecture**: ARM64 (aarch64)
- **Kernel**: 4.15+ with sound support

## Core Dependencies

### 1. System Libraries

```bash
# Build tools and compilers
build-essential
cmake
pkg-config
git
curl
wget

# SSL/TLS support
libssl-dev
libssl1.1
ca-certificates

# Audio system
alsa-utils
pulseaudio
libasound2-dev
libpulse-dev

# System utilities
htop
iotop
tree
unzip
```

### 2. Programming Languages

#### Rust (Latest Stable)
- **Purpose**: Main application development
- **Installation**: Via rustup
- **Components**: rustc, cargo, rust-std

#### Python 3.8+
- **Purpose**: Search and web scraping functionality
- **Packages**: pip, venv
- **Virtual environment recommended**

### 3. Runtime Dependencies

#### Ollama
- **Purpose**: LLM inference backend
- **Version**: Latest stable
- **Models**: qwen3:4b (downloaded automatically)
- **Memory**: ~4-6GB RAM usage

#### Whisper.cpp
- **Purpose**: Speech-to-text conversion
- **Repository**: https://github.com/ggerganov/whisper.cpp
- **Models**: base.en (~150MB)
- **Build**: Native ARM64 compilation required

#### Piper TTS
- **Purpose**: Text-to-speech synthesis
- **Version**: 1.2.0 ARM64
- **Models**: en_US-lessac-medium (~100MB)
- **Dependencies**: ONNX Runtime (included)

### 4. Python Dependencies

Install via `pip install -r requirements.txt`:

```python
requests>=2.31.0          # HTTP client for web requests
beautifulsoup4>=4.12.0    # HTML parsing for search results
lxml>=4.9.0              # Fast XML/HTML parser backend
urllib3>=1.26.0          # HTTP library (requests dependency)
certifi>=2023.0.0        # SSL certificate validation
html5lib>=1.1            # Alternative HTML parser (optional)
cchardet>=2.1.7          # Character encoding detection (optional)
```

### 5. Rust Dependencies (Cargo.toml)

```toml
[dependencies]
# Core dependencies
tokio = { version = "1.0", features = ["full"] }
reqwest = { version = "0.11", features = ["json"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
anyhow = "1.0"
thiserror = "1.0"
tempfile = "3.8"
regex = "1.10"
dirs = "5.0"
hound = "3.5"
toml = "0.8"

# RAG System dependencies
sqlx = { version = "0.7", features = ["runtime-tokio-rustls", "sqlite", "chrono", "uuid"] }
candle-core = "0.3"
candle-nn = "0.3"
candle-transformers = "0.3"
uuid = { version = "1.0", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
bincode = "1.3"
tokenizers = "0.15"
```

## Installation Order

### Automated Installation
```bash
curl -sSL https://raw.githubusercontent.com/your-repo/startrek-computer/main/install.sh | bash
```

### Manual Installation Order

1. **System Update**
   ```bash
   sudo apt update && sudo apt upgrade -y
   ```

2. **System Dependencies**
   ```bash
   sudo apt install -y build-essential cmake pkg-config git curl wget \
                       libssl-dev alsa-utils pulseaudio firefox
   ```

3. **Rust Installation**
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source $HOME/.cargo/env
   ```

4. **Ollama Installation**
   ```bash
   curl -fsSL https://ollama.ai/install.sh | sh
   sudo systemctl enable ollama
   sudo systemctl start ollama
   ollama pull qwen3:4b
   ```

5. **Whisper.cpp Build**
   ```bash
   git clone https://github.com/ggerganov/whisper.cpp.git
   cd whisper.cpp
   make -j4
   bash ./models/download-ggml-model.sh base.en
   ```

6. **Piper TTS Download**
   ```bash
   wget https://github.com/rhasspy/piper/releases/download/v1.2.0/piper_arm64.tar.gz
   tar -xzf piper_arm64.tar.gz
   # Download voice models
   ```

7. **Python Environment**
   ```bash
   python3 -m venv venv
   source venv/bin/activate
   pip install -r requirements.txt
   ```

8. **Voice Assistant Build**
   ```bash
   cd voice-assistant
   cargo build --release
   ```

## Version Compatibility

### Tested Versions

| Component | Version | Notes |
|-----------|---------|-------|
| Ubuntu | 20.04 LTS | Minimum supported |
| Ubuntu | 22.04 LTS | Recommended |
| Rust | 1.70+ | Latest stable recommended |
| Python | 3.8+ | 3.10+ recommended |
| Ollama | Latest | Auto-updates available |
| Whisper.cpp | Latest main | Built from source |
| Piper | 1.2.0 | ARM64 binary |

### Model Versions

| Model | Size | Purpose | Download Location |
|-------|------|---------|-------------------|
| Whisper base.en | 150MB | Speech recognition | Automatic via script |
| Piper lessac-medium | 100MB | Text-to-speech | HuggingFace |
| Qwen3 4B | 2.3GB | Language model | Ollama registry |

## Resource Requirements

### Memory Usage (Typical)
- **Whisper.cpp**: ~200MB during transcription
- **Ollama + Qwen3:4b**: ~4-6GB persistent
- **Piper TTS**: ~100MB during synthesis  
- **Voice Assistant**: ~50-100MB
- **System overhead**: ~2GB
- **Total recommended**: 8GB+ RAM

### Storage Requirements
- **Base system**: ~10GB
- **Dependencies**: ~2GB
- **Models**: ~3GB
- **Logs and data**: ~1GB
- **Total recommended**: 32GB+ storage

### CPU Requirements
- **Minimum**: ARM Cortex-A57 quad-core
- **Recommended**: ARM Cortex-A78 or better
- **Features**: NEON SIMD support beneficial
- **Cores**: 4+ cores recommended for concurrent processing

## Network Requirements

### Initial Setup
- **Bandwidth**: Broadband connection for model downloads
- **Data**: ~3GB for initial model downloads
- **Duration**: 30-60 minutes depending on connection

### Runtime Operation
- **Search features**: Internet access required
- **Local operation**: Possible without internet (no search)
- **Bandwidth**: Minimal (search queries only)

## Audio System Requirements

### Input Devices
- **USB microphone**: Recommended (built-in may work)
- **Sample rates**: 16kHz, 44.1kHz, 48kHz supported
- **Channels**: Mono preferred, stereo supported
- **Latency**: <100ms for best experience

### Output Devices
- **Speakers**: Any ALSA/PulseAudio compatible
- **Headphones**: USB or 3.5mm jack
- **Sample rates**: 22kHz (TTS output), 44.1kHz, 48kHz
- **Quality**: 16-bit minimum

### Audio Configuration
```bash
# Test input device
arecord -l

# Test output device  
aplay -l

# Test recording
arecord -d 5 -f cd test.wav && aplay test.wav
```

## Security Dependencies

### SSL/TLS
- **OpenSSL**: 1.1.1+ for HTTPS requests
- **Certificates**: ca-certificates package
- **Rust TLS**: rustls for secure connections

### System Security
- **User isolation**: Dedicated service user
- **Permissions**: Minimal required access
- **Network**: Localhost-only LLM communication

## Development Dependencies (Optional)

For development and customization:

```bash
# Rust development
rustfmt
clippy
rust-analyzer

# Python development
black
flake8
mypy

# Audio development
audacity          # For testing audio files
sox               # Audio file manipulation
```

## Troubleshooting Dependencies

### Common Issues

**Missing audio devices:**
```bash
sudo apt install linux-modules-extra-$(uname -r)
sudo usermod -a -G audio $USER
```

**SSL certificate errors:**
```bash
sudo apt update
sudo apt install ca-certificates
sudo update-ca-certificates
```

**Rust compilation errors:**
```bash
rustup update stable
cargo clean
cargo build --release
```

**Python import errors:**
```bash
python3 -m pip install --upgrade pip
pip install -r requirements.txt --force-reinstall
```

## Optional Dependencies

### Performance Monitoring
```bash
htop              # Process monitor
iotop             # I/O monitor  
nvtop             # GPU monitor (if applicable)
```

### Audio Enhancement
```bash
pulseaudio-utils  # PulseAudio utilities
pavucontrol       # PulseAudio GUI control
```

### Development Tools
```bash
valgrind          # Memory debugging
gdb               # Debugger
strace            # System call tracer
```

This dependency documentation ensures all required components are properly installed and configured for the Star Trek Computer system.
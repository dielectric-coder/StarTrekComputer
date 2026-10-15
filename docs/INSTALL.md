# Star Trek Computer Installation Guide

Complete installation guide for setting up the Star Trek Enterprise computer voice assistant on Jetson Nano 16GB.

## System Requirements

- **Hardware**: Jetson Nano 16GB (or compatible ARM64 system)
- **OS**: Ubuntu 20.04 LTS or newer 
- **Storage**: 32GB+ SD card or SSD
- **Audio**: USB microphone and speakers/headphones
- **Network**: Internet connection for initial setup and fact-checking

## Quick Installation

```bash
# Download and run the automated installer
curl -sSL https://raw.githubusercontent.com/dielectric-coder/StarTrekComputer/main/scripts/install.sh | bash
```

## Manual Installation

### 1. System Preparation

```bash
# Update system
sudo apt update && sudo apt upgrade -y

# Install base dependencies
sudo apt install -y \
    curl wget git build-essential \
    python3 python3-pip python3-venv \
    alsa-utils pulseaudio \
    firefox \
    cmake pkg-config \
    libssl-dev

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### 2. Audio System Setup

```bash
# Configure audio permissions
sudo usermod -a -G audio $USER
sudo chmod 666 /dev/snd/*

# Test audio devices
arecord -l  # List input devices
aplay -l    # List output devices

# Test microphone (record 5 seconds)
arecord -d 5 -f cd test.wav && aplay test.wav
```

### 3. Install Ollama (LLM Backend)

```bash
# Install Ollama
curl -fsSL https://ollama.ai/install.sh | sh

# Start Ollama service
sudo systemctl enable ollama
sudo systemctl start ollama

# Install the qwen3:4b model
ollama pull qwen3:4b

# Test Ollama
curl http://localhost:11434/api/generate -d '{
  "model": "qwen3:4b",
  "prompt": "You are the Enterprise computer. State your status.",
  "stream": false
}'
```

### 4. Install Whisper.cpp (Speech Recognition)

```bash
# Clone and build Whisper.cpp
cd $HOME/startrek
git clone https://github.com/ggerganov/whisper.cpp.git
cd whisper.cpp

# Build for ARM64
make -j4

# Download base English model
bash ./models/download-ggml-model.sh base.en

# Test whisper
echo "Computer, what is the weather today?" | ./build/bin/whisper-cli -m models/ggml-base.en.bin -f -
```

### 5. Install Piper TTS (Text-to-Speech)

```bash
cd $HOME/startrek

# Download Piper for ARM64
wget https://github.com/rhasspy/piper/releases/download/v1.2.0/piper_arm64.tar.gz
tar -xzf piper_arm64.tar.gz

# Download voice model
mkdir -p piper/voices
cd piper/voices
wget https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/lessac/medium/en_US-lessac-medium.onnx
wget https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/lessac/medium/en_US-lessac-medium.onnx.json

# Test Piper
cd $HOME/startrek
echo "Working. All systems nominal." | ./piper/piper/piper --model ./piper/voices/en_US-lessac-medium.onnx --output_file - | aplay
```

### 6. Install Python Dependencies

```bash
cd $HOME/startrek

# Create virtual environment
python3 -m venv venv
source venv/bin/activate

# Install Python packages
pip install -r requirements.txt
```

Create `requirements.txt`:

```txt
requests>=2.31.0
beautifulsoup4>=4.12.0
lxml>=4.9.0
```

### 7. Build Voice Assistant

```bash
cd $HOME/startrek/voice-assistant

# Build the Rust application
cargo build --release

# Create symlink for easy access
sudo ln -sf $PWD/target/release/voice-assistant /usr/local/bin/startrek-computer
```

## System Service Setup

### 1. Create Service User

```bash
# Create dedicated service user
sudo useradd -r -s /bin/false -d /var/lib/startrek-computer startrek
sudo mkdir -p /var/lib/startrek-computer
sudo chown startrek:audio /var/lib/startrek-computer

# Copy application files
sudo mkdir -p /opt/startrek
sudo cp -r $HOME/startrek /opt/startrek/
sudo chown -R startrek:audio /opt/startrek
```

### 2. Create Systemd Service

Create `/etc/systemd/system/startrek-computer.service`:

```ini
[Unit]
Description=Star Trek Enterprise Computer Voice Assistant
After=network.target sound.target ollama.service
Wants=network.target sound.target ollama.service
StartLimitInterval=60s
StartLimitBurst=3

[Service]
Type=simple
User=startrek
Group=audio
ExecStart=/opt/startrek/startrek/voice-assistant/target/release/voice-assistant
Restart=always
RestartSec=10
KillMode=mixed
TimeoutStopSec=30

# Environment variables
Environment=RUST_LOG=info
Environment=OLLAMA_HOST=http://localhost:11434
WorkingDirectory=/opt/startrek/startrek

# Security settings
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/startrek-computer /tmp
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

### 3. Enable Auto-Start

```bash
# Reload systemd and enable service
sudo systemctl daemon-reload
sudo systemctl enable startrek-computer.service

# Start the service
sudo systemctl start startrek-computer.service

# Check status
sudo systemctl status startrek-computer.service
```

## Audio Configuration

### 1. Set Default Audio Devices

Create `/etc/asound.conf`:

```
pcm.!default {
    type pulse
}
ctl.!default {
    type pulse
}
```

### 2. Configure PulseAudio

Edit `/etc/pulse/system.pa`:

```bash
# Add at the end
load-module module-alsa-source device=hw:1,0  # USB microphone
load-module module-alsa-sink device=hw:0,0    # Default output

# Set default source and sink
set-default-source alsa_input.usb-*
set-default-sink alsa_output.platform-*
```

## Configuration

The system will create a default configuration at `~/startrek/.voice-assistant/config.toml`
(for the service: `/var/lib/startrek-computer/startrek/.voice-assistant/config.toml`). You can customize:

### Sample Configuration

```toml
[audio]
sample_rate = 16000
channels = 1
buffer_size = 1024
input_device = null   # null = system default
output_device = null  # null = system default

[stt]
engine = "whisper"
model_path = "/opt/startrek/startrek/whisper.cpp/models/ggml-base.en.bin"
language = "en"

[tts]
engine = "piper"
model_path = "/opt/startrek/startrek/piper/voices/en_US-lessac-medium.onnx"
voice = "lessac"
speed = 0.9

[llm]
base_url = "http://localhost:11434"
model = "qwen3:4b"
context_length = 2048
temperature = 0.3

[search]
enabled = true
search_script = "/opt/startrek/startrek/search_tool.py"
max_results = 3

[wake_word]
enabled = true
wake_word = "computer"
sensitivity = 0.5

[general]
log_level = "info"
data_dir = "/var/lib/startrek-computer"
```

## Testing the System

### 1. Manual Test

```bash
# Test the service
sudo systemctl status startrek-computer

# View logs
sudo journalctl -u startrek-computer -f

# Test wake word (speak into microphone)
# Say: "Computer, what is two plus two?"
```

### 2. Component Tests

```bash
# Test STT
echo "Computer, status report" | /opt/startrek/startrek/whisper.cpp/build/bin/whisper-cli -m /opt/startrek/startrek/whisper.cpp/models/ggml-base.en.bin -f -

# Test LLM
curl http://localhost:11434/api/generate -d '{
  "model": "qwen3:4b",
  "prompt": "You are the Enterprise computer. Report system status.",
  "stream": false
}'

# Test TTS
echo "All systems operating normally, Captain." | /opt/startrek/startrek/piper/piper/piper --model /opt/startrek/startrek/piper/voices/en_US-lessac-medium.onnx --output_file - | aplay

# Test search
python3 /opt/startrek/startrek/search_tool.py "current weather conditions"
```

## Boot-Time Startup

### 1. Enable Services

```bash
# Enable required services for boot
sudo systemctl enable ollama.service
sudo systemctl enable startrek-computer.service

# Ensure audio starts before our service
sudo systemctl edit startrek-computer.service
```

Add to override file:

```ini
[Unit]
After=sound.target pulseaudio.service alsa-state.service
Wants=sound.target pulseaudio.service alsa-state.service
```

### 2. Test Boot Sequence

```bash
# Simulate boot test
sudo systemctl stop startrek-computer
sudo systemctl stop ollama

# Start in order
sudo systemctl start ollama
sleep 5
sudo systemctl start startrek-computer

# Check all running
sudo systemctl is-active ollama startrek-computer
```

## Monitoring and Maintenance

### 1. View Logs

```bash
# Service logs
sudo journalctl -u startrek-computer -f --no-pager

# System logs
sudo journalctl -f --no-pager | grep -i "startrek\|computer\|ollama"

# Audio logs
sudo journalctl -u pulseaudio -f --no-pager
```

### 2. Performance Monitoring

```bash
# CPU/Memory usage
htop
iotop

# Audio monitoring
alsamixer
pavucontrol  # if using PulseAudio GUI

# Service status
sudo systemctl status startrek-computer ollama
```

### 3. Health Checks

Create `/opt/startrek/startrek/healthcheck.sh`:

```bash
#!/bin/bash

echo "=== Star Trek Computer Health Check ==="
echo "Date: $(date)"
echo

# Check services
echo "Services:"
systemctl is-active ollama startrek-computer
echo

# Check audio
echo "Audio devices:"
aplay -l | grep -E "card|device" | head -5
echo

# Check models
echo "Models:"
ls -la /opt/startrek/startrek/whisper.cpp/models/ggml-*.bin
ls -la /opt/startrek/startrek/piper/voices/*.onnx
echo

# Check logs for errors
echo "Recent errors:"
journalctl -u startrek-computer --since "1 hour ago" | grep -i error | tail -5
echo

# Test components
echo "Component tests:"
timeout 10 curl -s http://localhost:11434/api/tags > /dev/null && echo "✓ Ollama responding" || echo "✗ Ollama not responding"
test -f /opt/startrek/startrek/search_tool.py && echo "✓ Search tool present" || echo "✗ Search tool missing"
echo

echo "=== Health check complete ==="
```

```bash
# Make executable
chmod +x /opt/startrek/startrek/healthcheck.sh

# Run health check
sudo -u startrek /opt/startrek/startrek/healthcheck.sh
```

## Automatic Updates

Create `/opt/startrek/startrek/update.sh`:

```bash
#!/bin/bash

set -e

echo "=== Star Trek Computer Update ==="

# Backup current config
cp -r /var/lib/startrek-computer /var/lib/startrek-computer.backup.$(date +%Y%m%d_%H%M%S)

# Update models if needed
cd /opt/startrek/startrek/whisper.cpp
git pull
make -j4

# Update voice assistant
cd /opt/startrek/startrek/voice-assistant
git pull
cargo build --release

# Restart service
systemctl restart startrek-computer

echo "Update complete. Check status with: systemctl status startrek-computer"
```

```bash
chmod +x /opt/startrek/startrek/update.sh
```

## Troubleshooting

### Common Issues

**Service won't start:**
```bash
# Check dependencies
systemctl status ollama
systemctl status pulseaudio

# Check permissions
sudo -u startrek ls -la /opt/startrek/startrek/
```

**No audio input:**
```bash
# List devices
arecord -l
pavucontrol

# Test microphone
sudo -u startrek arecord -d 3 -f cd test.wav
aplay test.wav
```

**LLM not responding:**
```bash
# Check Ollama
curl http://localhost:11434/api/tags
ollama list

# Restart Ollama
sudo systemctl restart ollama
```

**Wake word not detecting:**
```bash
# Check audio levels
alsamixer
# Boost microphone gain

# Test STT manually
arecord -f cd -d 5 test.wav
/opt/startrek/startrek/whisper.cpp/build/bin/whisper-cli -m /opt/startrek/startrek/whisper.cpp/models/ggml-base.en.bin -f test.wav
```

### Log Analysis

```bash
# Common log patterns to check
sudo journalctl -u startrek-computer | grep -E "(error|failed|panic)"

# Audio issues
sudo journalctl | grep -E "(alsa|pulse|audio)" | tail -20

# Performance issues  
sudo journalctl -u startrek-computer | grep -E "(timeout|slow|memory)"
```

## Security Considerations

1. **Network Security**: The system uses localhost for LLM communication
2. **File Permissions**: Service runs as dedicated user with minimal privileges
3. **Audio Access**: Only audio group access required
4. **Updates**: Regular security updates recommended

## Performance Tuning

### For Jetson Nano

```bash
# Set performance governor
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Increase GPU memory for audio processing
echo "gpu_mem=128" | sudo tee -a /boot/config.txt

# Optimize audio buffer sizes in config.toml
buffer_size = 512  # Smaller for lower latency
```

### Memory Management

```bash
# Monitor memory usage
free -h
ps aux --sort=-%mem | head -10

# Configure swap if needed
sudo fallocate -l 4G /swapfile
sudo chmod 600 /swapfile  
sudo mkswap /swapfile
sudo swapon /swapfile
echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab
```

This completes the comprehensive installation and deployment guide for the Star Trek computer system.
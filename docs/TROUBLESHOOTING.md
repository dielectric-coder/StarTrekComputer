# Star Trek Computer Troubleshooting Guide

This guide provides solutions to common issues encountered with the Star Trek Computer voice assistant system.

## Quick Diagnostics

### Health Check Script
```bash
sudo /opt/startrek/startrek/healthcheck.sh
```

### Service Status
```bash
sudo systemctl status startrek-computer
sudo systemctl status startrek-ollama
```

### View Logs
```bash
sudo /opt/startrek/startrek/logs.sh
```

## Common Issues and Solutions

### 1. Service Won't Start

**Symptoms:**
- Service fails to start
- Exit codes 1, 125, or 127
- Error messages in logs

**Diagnosis:**
```bash
sudo systemctl status startrek-computer
sudo journalctl -u startrek-computer --no-pager -l
```

**Solutions:**

**A. Check Dependencies**
```bash
# Verify Ollama is running
sudo systemctl status ollama
curl http://localhost:11434/api/tags

# Restart Ollama if needed
sudo systemctl restart ollama
sleep 10
sudo systemctl start startrek-computer
```

**B. Check Permissions**
```bash
# Verify service user permissions
sudo -u startrek ls -la /opt/startrek/startrek/
sudo -u startrek test -r /opt/startrek/startrek/voice-assistant/target/release/voice-assistant
```

**C. Check File Paths**
```bash
# Verify all required files exist
ls -la /opt/startrek/startrek/whisper.cpp/models/ggml-base.en.bin
ls -la /opt/startrek/startrek/piper/voices/en_US-lessac-medium.onnx
ls -la /opt/startrek/startrek/search_tool.py
```

### 2. No Audio Input Detected

**Symptoms:**
- Wake word not detected
- STT not responding
- Microphone not working

**Diagnosis:**
```bash
# List audio devices
arecord -l
aplay -l

# Test microphone
arecord -d 3 -f cd test.wav
aplay test.wav
```

**Solutions:**

**A. USB Microphone Not Recognized**
```bash
# Check USB devices
lsusb | grep -i audio

# Restart audio services
sudo systemctl restart pulseaudio
sudo systemctl restart alsa-state

# Add audio permissions
sudo usermod -a -G audio $USER
sudo usermod -a -G audio startrek
```

**B. Wrong Audio Device Selected**
```bash
# Edit configuration
sudo nano /var/lib/startrek-computer/config.toml

# Set specific device
[audio]
input_device = "hw:1,0"    # Replace with your device
```

**C. Audio Levels Too Low**
```bash
# Open audio mixer
alsamixer

# Or use PulseAudio control
pavucontrol

# Set microphone gain to 70-90%
```

### 3. Wake Word Detection Issues

**Symptoms:**
- "Computer" not recognized
- False positives
- Intermittent detection

**Solutions:**

**A. Adjust Sensitivity**
```bash
# Edit configuration
sudo nano /var/lib/startrek-computer/config.toml

[wake_word]
sensitivity = 0.7    # Increase for better detection (0.1-1.0)
```

**B. Test STT Manually**
```bash
# Record and test
arecord -d 5 -f cd test.wav
/opt/startrek/startrek/whisper.cpp/main -m /opt/startrek/startrek/whisper.cpp/models/ggml-base.en.bin -f test.wav
```

**C. Check Background Noise**
```bash
# Test in quiet environment
# Speak clearly and distinctly
# Position microphone 6-12 inches from mouth
```

### 4. LLM Not Responding

**Symptoms:**
- Long delays
- Timeout errors
- Generic error responses

**Diagnosis:**
```bash
# Test Ollama directly
curl http://localhost:11434/api/generate -d '{
  "model": "qwen3:4b",
  "prompt": "Test response",
  "stream": false
}'
```

**Solutions:**

**A. Ollama Service Issues**
```bash
# Check Ollama status
sudo systemctl status ollama

# Restart Ollama
sudo systemctl restart ollama

# Wait for model to load
sleep 30
sudo systemctl restart startrek-computer
```

**B. Model Not Loaded**
```bash
# Check available models
ollama list

# Pull model if missing
ollama pull qwen3:4b

# Test model
ollama run qwen3:4b "Test message"
```

**C. Memory Issues**
```bash
# Check memory usage
free -h
sudo journalctl -u ollama | grep -i memory

# If low memory, restart system
sudo reboot
```

### 5. TTS Not Working

**Symptoms:**
- No voice output
- Audio errors
- Garbled speech

**Solutions:**

**A. Test TTS Manually**
```bash
echo "Working" | /opt/startrek/startrek/piper/piper/piper \
  --model /opt/startrek/startrek/piper/voices/en_US-lessac-medium.onnx \
  --output_file - | aplay
```

**B. Check Audio Output Device**
```bash
# List playback devices
aplay -l

# Test with specific device
echo "Test" | /opt/startrek/startrek/piper/piper/piper \
  --model /opt/startrek/startrek/piper/voices/en_US-lessac-medium.onnx \
  --output_file - | aplay -D hw:0,0
```

**C. Audio Format Issues**
```bash
# Try different audio settings
echo "Test" | /opt/startrek/startrek/piper/piper/piper \
  --model /opt/startrek/startrek/piper/voices/en_US-lessac-medium.onnx \
  --output_file test.wav

aplay -f S16_LE -r 22050 -c 1 test.wav
```

### 6. Search/Fact-Checking Not Working

**Symptoms:**
- Search timeouts
- No fact-check results
- Internet connection errors

**Solutions:**

**A. Test Search Script**
```bash
cd /opt/startrek/startrek
python3 search_tool.py "test query"
```

**B. Check Internet Connection**
```bash
ping -c 3 google.com
curl -I https://www.bing.com
```

**C. Python Dependencies**
```bash
cd /opt/startrek/startrek
source venv/bin/activate
pip install -r requirements.txt --upgrade
```

### 7. High Memory Usage

**Symptoms:**
- System sluggish
- Out of memory errors
- Service crashes

**Solutions:**

**A. Monitor Memory**
```bash
htop
free -h
sudo journalctl -u startrek-computer | grep -i memory
```

**B. Optimize Configuration**
```bash
# Edit configuration for lower memory usage
sudo nano /var/lib/startrek-computer/config.toml

[llm]
context_length = 1024    # Reduce context size
temperature = 0.2

[audio]
buffer_size = 512        # Smaller audio buffers
```

**C. Add Swap Space**
```bash
sudo fallocate -l 4G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile
echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab
```

### 8. Boot/Startup Issues

**Symptoms:**
- Service doesn't start on boot
- Delayed startup
- Startup failures

**Solutions:**

**A. Check Service Dependencies**
```bash
sudo systemctl list-dependencies startrek-computer
sudo systemctl enable ollama
sudo systemctl enable startrek-computer
```

**B. Increase Startup Timeout**
```bash
sudo systemctl edit startrek-computer

# Add:
[Service]
TimeoutStartSec=120
```

**C. Check Boot Logs**
```bash
sudo journalctl -b | grep -i startrek
sudo journalctl -b | grep -i ollama
```

### 9. Audio Quality Issues

**Symptoms:**
- Robotic or distorted voice
- Choppy audio playback
- Echo or feedback

**Solutions:**

**A. Adjust Audio Settings**
```bash
# Edit ALSA configuration
sudo nano /etc/asound.conf

# Add:
pcm.!default {
    type plug
    slave.pcm "hw:0,0"
    slave.rate 44100
}
```

**B. PulseAudio Configuration**
```bash
# Edit PulseAudio config
sudo nano /etc/pulse/daemon.conf

# Uncomment and set:
default-sample-format = s16le
default-sample-rate = 44100
default-sample-channels = 2
```

**C. Test Different Sample Rates**
```bash
# Try different configurations in config.toml
[audio]
sample_rate = 44100
[tts]
speed = 0.9
```

### 10. Performance Issues

**Symptoms:**
- Slow response times
- High CPU usage
- System lag

**Solutions:**

**A. CPU Governor**
```bash
# Set performance mode
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Make permanent
echo 'GOVERNOR="performance"' | sudo tee /etc/default/cpufrequtils
```

**B. Process Priorities**
```bash
# Edit service file
sudo systemctl edit startrek-computer

# Add:
[Service]
Nice=-10
IOSchedulingClass=1
IOSchedulingPriority=4
```

**C. Memory Management**
```bash
# Reduce swappiness
echo 'vm.swappiness=10' | sudo tee -a /etc/sysctl.conf

# Apply immediately
sudo sysctl vm.swappiness=10
```

## Advanced Debugging

### 1. Enable Debug Logging

```bash
# Edit configuration
sudo nano /var/lib/startrek-computer/config.toml

[general]
log_level = "debug"

# Restart service
sudo systemctl restart startrek-computer

# View detailed logs
sudo journalctl -u startrek-computer -f
```

### 2. Component Isolation Testing

**Test Individual Components:**

```bash
# Test STT only
echo "Computer status report" | /opt/startrek/startrek/whisper.cpp/main \
  -m /opt/startrek/startrek/whisper.cpp/models/ggml-base.en.bin -f -

# Test LLM only  
curl http://localhost:11434/api/generate -d '{
  "model": "qwen3:4b",
  "prompt": "You are the Enterprise computer. Report status.",
  "stream": false
}'

# Test TTS only
echo "All systems nominal" | /opt/startrek/startrek/piper/piper/piper \
  --model /opt/startrek/startrek/piper/voices/en_US-lessac-medium.onnx \
  --output_file - | aplay

# Test search only
python3 /opt/startrek/startrek/search_tool.py "current weather"
```

### 3. System Resource Monitoring

```bash
# Real-time monitoring
htop
iotop -ao

# Disk usage
df -h
du -sh /opt/startrek/
du -sh /var/lib/startrek-computer/

# Network usage
sudo netstat -tulnp | grep -E "(11434|startrek)"
```

### 4. Audio System Deep Dive

```bash
# Detailed audio device info
cat /proc/asound/cards
cat /proc/asound/devices

# ALSA configuration test
alsactl store
alsactl restore

# PulseAudio information
pulseaudio --check -v
pactl info
pactl list sources
pactl list sinks
```

## Recovery Procedures

### 1. Complete System Reset

```bash
# Stop services
sudo systemctl stop startrek-computer
sudo systemctl stop ollama

# Reset configuration
sudo rm -f /var/lib/startrek-computer/config.toml
sudo cp /opt/startrek/startrek/config.toml.example /var/lib/startrek-computer/config.toml
sudo chown startrek:audio /var/lib/startrek-computer/config.toml

# Clear logs
sudo journalctl --vacuum-time=1d

# Restart services
sudo systemctl start ollama
sleep 10
sudo systemctl start startrek-computer
```

### 2. Reinstall Models

```bash
# Whisper model
cd /opt/startrek/startrek/whisper.cpp
rm -f models/ggml-*.bin
bash ./models/download-ggml-model.sh base.en

# Ollama model
ollama rm qwen3:4b
ollama pull qwen3:4b

# Piper model
cd /opt/startrek/startrek/piper/voices
rm -f *.onnx*
wget https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/lessac/medium/en_US-lessac-medium.onnx
wget https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/lessac/medium/en_US-lessac-medium.onnx.json
```

### 3. Emergency Debugging Mode

```bash
# Run service manually for debugging
sudo systemctl stop startrek-computer
sudo -u startrek /opt/startrek/startrek/voice-assistant/target/release/voice-assistant

# Or with full debug output
RUST_LOG=debug RUST_BACKTRACE=full sudo -u startrek /opt/startrek/startrek/voice-assistant/target/release/voice-assistant
```

## Getting Help

### 1. Collect System Information

```bash
# Create diagnostic report
{
  echo "=== System Information ==="
  uname -a
  lsb_release -a
  df -h
  free -h
  echo
  
  echo "=== Service Status ==="
  systemctl status startrek-computer --no-pager
  systemctl status ollama --no-pager
  echo
  
  echo "=== Audio Devices ==="
  arecord -l
  aplay -l
  echo
  
  echo "=== Recent Logs ==="
  journalctl -u startrek-computer --since "1 hour ago" --no-pager | tail -50
  
} > startrek-diagnostics.txt
```

### 2. Log Collection

```bash
# Collect all relevant logs
sudo journalctl -u startrek-computer --since "24 hours ago" > computer-logs.txt
sudo journalctl -u ollama --since "24 hours ago" > ollama-logs.txt
sudo journalctl | grep -i audio > audio-logs.txt
```

### 3. Configuration Backup

```bash
# Backup current configuration
cp /var/lib/startrek-computer/config.toml config-backup.toml
sudo systemctl show startrek-computer > service-config.txt
```

## Prevention and Maintenance

### 1. Regular Health Checks

Add to crontab for automated monitoring:
```bash
# Check every hour
0 * * * * /opt/startrek/startrek/healthcheck.sh >> /var/log/startrek-health.log 2>&1

# Weekly system cleanup
0 3 * * 0 /usr/bin/journalctl --vacuum-time=7d
```

### 2. Update Procedures

```bash
# Regular update script
/opt/startrek/startrek/update.sh

# Manual update
cd /opt/startrek/startrek
git pull
cd voice-assistant
cargo build --release
sudo systemctl restart startrek-computer
```

### 3. Monitoring Scripts

Create monitoring alerts for critical issues:
```bash
#!/bin/bash
# /opt/startrek/startrek/monitor.sh

if ! systemctl is-active --quiet startrek-computer; then
    echo "Star Trek Computer service is down!" | wall
    logger "ALERT: Star Trek Computer service failed"
fi

if ! curl -s http://localhost:11434/api/tags >/dev/null; then
    echo "Ollama service is not responding!" | wall
    logger "ALERT: Ollama service not responding"
fi
```

This troubleshooting guide covers the most common issues and their solutions. For persistent problems, check the GitHub repository for updates or submit a detailed issue report with diagnostic information.
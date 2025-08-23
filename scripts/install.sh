#!/bin/bash

# Star Trek Computer Automated Installation Script
# For Jetson Nano 16GB and compatible ARM64 systems
# Run with: curl -sSL https://raw.githubusercontent.com/oakeley/StarTrekComputer/main/scripts/install.sh | bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
INSTALL_DIR="/opt/startrek"
SERVICE_USER="startrek"
SERVICE_GROUP="audio"
HOME_DIR="/home/$USER"
STARTREK_DIR="$HOME_DIR/startrek"

# Helper functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

check_requirements() {
    log_info "Checking system requirements..."
    
    # Check if running on ARM64
    if [[ $(uname -m) != "aarch64" ]]; then
        log_warn "This installer is optimized for ARM64 systems (Jetson Nano)"
        read -p "Continue anyway? (y/N): " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            exit 1
        fi
    fi
    
    # Check available space (need at least 8GB)
    AVAILABLE_GB=$(df / | tail -1 | awk '{print int($4/1024/1024)}')
    if [[ $AVAILABLE_GB -lt 8 ]]; then
        log_error "Insufficient disk space. Need at least 8GB, have ${AVAILABLE_GB}GB"
        exit 1
    fi
    
    log_success "System requirements check passed"
}

install_system_deps() {
    log_info "Installing system dependencies..."
    
    # Update package list
    sudo apt update
    
    # Install required packages
    sudo apt install -y \
        curl wget git build-essential \
        python3 python3-pip python3-venv \
        alsa-utils pulseaudio \
        firefox \
        cmake pkg-config \
        libssl-dev \
        libssl1.1 \
        ca-certificates \
        gnupg \
        lsb-release \
        htop iotop \
        tree \
        unzip
        
    log_success "System dependencies installed"
}

setup_audio() {
    log_info "Setting up audio system..."
    
    # Add user to audio group
    sudo usermod -a -G audio $USER
    
    # Set audio device permissions
    sudo chmod 666 /dev/snd/* 2>/dev/null || true
    
    # Create basic ALSA configuration
    sudo tee /etc/asound.conf > /dev/null <<EOF
pcm.!default {
    type pulse
}
ctl.!default {
    type pulse
}
EOF

    log_success "Audio system configured"
}

install_rust() {
    log_info "Installing Rust..."
    
    if command -v rustc >/dev/null 2>&1; then
        log_info "Rust already installed"
    else
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source $HOME/.cargo/env
        echo 'source $HOME/.cargo/env' >> $HOME/.bashrc
    fi
    
    # Ensure we have the latest stable
    source $HOME/.cargo/env
    rustup update stable
    
    log_success "Rust installed"
}

install_ollama() {
    log_info "Installing Ollama..."
    
    if command -v ollama >/dev/null 2>&1; then
        log_info "Ollama already installed"
    else
        curl -fsSL https://ollama.ai/install.sh | sh
    fi
    
    # Start Ollama service
    sudo systemctl enable ollama 2>/dev/null || true
    sudo systemctl start ollama 2>/dev/null || true
    
    # Wait for Ollama to start
    log_info "Waiting for Ollama to start..."
    for i in {1..30}; do
        if curl -s http://localhost:11434/api/tags >/dev/null 2>&1; then
            break
        fi
        sleep 2
    done
    
    # Install the model
    log_info "Installing qwen3:4b model (this may take several minutes)..."
    ollama pull qwen3:4b
    
    log_success "Ollama installed and model downloaded"
}

install_whisper() {
    log_info "Installing Whisper.cpp..."
    
    cd $STARTREK_DIR
    
    if [[ -d "whisper.cpp" ]]; then
        log_info "Whisper.cpp directory exists, updating..."
        cd whisper.cpp
        git pull
    else
        git clone https://github.com/ggerganov/whisper.cpp.git
        cd whisper.cpp
    fi
    
    # Build for ARM64
    make clean
    make -j4
    
    # Download model
    if [[ ! -f "models/ggml-base.en.bin" ]]; then
        log_info "Downloading Whisper model..."
        bash ./models/download-ggml-model.sh base.en
    fi
    
    log_success "Whisper.cpp installed"
}

install_piper() {
    log_info "Installing Piper TTS..."
    
    cd $STARTREK_DIR
    
    if [[ ! -d "piper" ]]; then
        mkdir -p piper
        cd piper
        
        # Download Piper for ARM64
        wget -q https://github.com/rhasspy/piper/releases/download/v1.2.0/piper_arm64.tar.gz
        tar -xzf piper_arm64.tar.gz
        rm piper_arm64.tar.gz
        
        # Download voice model
        mkdir -p voices
        cd voices
        
        log_info "Downloading TTS voice model..."
        wget -q https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/lessac/medium/en_US-lessac-medium.onnx
        wget -q https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/lessac/medium/en_US-lessac-medium.onnx.json
    else
        log_info "Piper already installed"
    fi
    
    log_success "Piper TTS installed"
}

install_python_deps() {
    log_info "Installing Python dependencies..."
    
    cd $STARTREK_DIR
    
    # Create requirements.txt if it doesn't exist
    cat > requirements.txt <<EOF
requests>=2.31.0
beautifulsoup4>=4.12.0
lxml>=4.9.0
urllib3>=1.26.0
certifi>=2023.0.0
EOF

    # Create virtual environment
    python3 -m venv venv
    source venv/bin/activate
    
    # Install packages
    pip install --upgrade pip
    pip install -r requirements.txt
    
    log_success "Python dependencies installed"
}

build_voice_assistant() {
    log_info "Building voice assistant..."
    
    cd $STARTREK_DIR/voice-assistant
    
    # Ensure we have cargo
    source $HOME/.cargo/env
    
    # Build release version
    cargo build --release
    
    log_success "Voice assistant built"
}

create_service_user() {
    log_info "Creating service user..."
    
    # Create service user if it doesn't exist
    if ! id "$SERVICE_USER" >/dev/null 2>&1; then
        sudo useradd -r -s /bin/false -d /var/lib/startrek-computer -G audio $SERVICE_USER
    fi
    
    # Create service directories
    sudo mkdir -p /var/lib/startrek-computer
    sudo mkdir -p $INSTALL_DIR
    
    log_success "Service user created"
}

install_service_files() {
    log_info "Installing service files..."
    
    # Copy application to system location
    sudo cp -r $STARTREK_DIR $INSTALL_DIR/
    sudo chown -R $SERVICE_USER:$SERVICE_GROUP $INSTALL_DIR
    
    # Make voice assistant executable globally
    sudo ln -sf $INSTALL_DIR/startrek/voice-assistant/target/release/voice-assistant /usr/local/bin/startrek-computer
    
    # Create systemd service file
    sudo tee /etc/systemd/system/startrek-computer.service > /dev/null <<EOF
[Unit]
Description=Star Trek Enterprise Computer Voice Assistant
After=network.target sound.target ollama.service
Wants=network.target sound.target ollama.service
StartLimitInterval=60s
StartLimitBurst=3

[Service]
Type=simple
User=$SERVICE_USER
Group=$SERVICE_GROUP
ExecStart=$INSTALL_DIR/startrek/voice-assistant/target/release/voice-assistant
Restart=always
RestartSec=10
KillMode=mixed
TimeoutStopSec=30

# Environment variables
Environment=RUST_LOG=info
Environment=OLLAMA_HOST=http://localhost:11434
WorkingDirectory=$INSTALL_DIR/startrek

# Security settings
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/startrek-computer /tmp
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF

    # Create health check script
    sudo tee $INSTALL_DIR/startrek/healthcheck.sh > /dev/null <<'EOF'
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
ls -la /opt/startrek/startrek/whisper.cpp/models/ggml-*.bin 2>/dev/null || echo "No Whisper models found"
ls -la /opt/startrek/startrek/piper/voices/*.onnx 2>/dev/null || echo "No TTS models found"
echo

# Check logs for errors
echo "Recent errors:"
journalctl -u startrek-computer --since "1 hour ago" 2>/dev/null | grep -i error | tail -5 || echo "No recent errors"
echo

# Test components
echo "Component tests:"
timeout 10 curl -s http://localhost:11434/api/tags > /dev/null && echo "✓ Ollama responding" || echo "✗ Ollama not responding"
test -f /opt/startrek/startrek/search_tool.py && echo "✓ Search tool present" || echo "✗ Search tool missing"
echo

echo "=== Health check complete ==="
EOF

    sudo chmod +x $INSTALL_DIR/startrek/healthcheck.sh
    
    # Reload systemd
    sudo systemctl daemon-reload
    
    log_success "Service files installed"
}

configure_startup() {
    log_info "Configuring auto-startup..."
    
    # Enable services
    sudo systemctl enable ollama.service
    sudo systemctl enable startrek-computer.service
    
    # Start services
    sudo systemctl start ollama.service
    sleep 5
    sudo systemctl start startrek-computer.service
    
    log_success "Auto-startup configured"
}

run_tests() {
    log_info "Running system tests..."
    
    # Test Ollama
    log_info "Testing LLM..."
    if curl -s http://localhost:11434/api/generate -d '{"model": "qwen3:4b", "prompt": "Test", "stream": false}' | grep -q "response"; then
        log_success "✓ LLM test passed"
    else
        log_warn "✗ LLM test failed"
    fi
    
    # Test Whisper
    log_info "Testing Speech Recognition..."
    if echo "test" | $INSTALL_DIR/startrek/whisper.cpp/main -m $INSTALL_DIR/startrek/whisper.cpp/models/ggml-base.en.bin -f - >/dev/null 2>&1; then
        log_success "✓ STT test passed"
    else
        log_warn "✗ STT test failed"
    fi
    
    # Test Piper
    log_info "Testing Text-to-Speech..."
    if echo "test" | $INSTALL_DIR/startrek/piper/piper/piper --model $INSTALL_DIR/startrek/piper/voices/en_US-lessac-medium.onnx --output_file /tmp/test.wav 2>/dev/null; then
        log_success "✓ TTS test passed"
    else
        log_warn "✗ TTS test failed"
    fi
    
    # Test service
    log_info "Testing service status..."
    if sudo systemctl is-active --quiet startrek-computer; then
        log_success "✓ Service is running"
    else
        log_warn "✗ Service is not running"
    fi
}

show_completion_message() {
    echo
    echo "=================================================================="
    log_success "Star Trek Computer Installation Complete!"
    echo "=================================================================="
    echo
    echo "The system is now configured to start automatically on boot."
    echo
    echo "Usage:"
    echo "  - Say 'Computer, [your request]' to interact"
    echo "  - The system will respond with Enterprise computer voice"
    echo "  - Searches will be fact-checked automatically"
    echo
    echo "Service Management:"
    echo "  sudo systemctl start startrek-computer    # Start service"
    echo "  sudo systemctl stop startrek-computer     # Stop service"
    echo "  sudo systemctl status startrek-computer   # Check status"
    echo "  sudo journalctl -u startrek-computer -f   # View logs"
    echo
    echo "Health Check:"
    echo "  sudo $INSTALL_DIR/startrek/healthcheck.sh"
    echo
    echo "Configuration:"
    echo "  /var/lib/startrek-computer/config.toml"
    echo
    log_warn "Please reboot the system to ensure all services start correctly:"
    echo "  sudo reboot"
    echo
}

# Main installation flow
main() {
    echo "=================================================================="
    echo "Star Trek Enterprise Computer Installation"
    echo "=================================================================="
    echo
    
    # Create working directory
    mkdir -p $STARTREK_DIR
    cd $STARTREK_DIR
    
    check_requirements
    install_system_deps
    setup_audio
    install_rust
    install_ollama
    install_whisper
    install_piper
    install_python_deps
    
    # Clone or update the voice assistant code
    if [[ -d "voice-assistant" ]]; then
        log_info "Updating voice assistant code..."
        cd voice-assistant
        git pull
        cd ..
    else
        log_info "Cloning voice assistant code..."
        # This would be replaced with the actual repository
        # git clone https://github.com/oakeley/StarTrekComputer.git voice-assistant
        
        # For now, we'll assume the code is already in place
        if [[ ! -d "voice-assistant" ]]; then
            log_error "Voice assistant code not found. Please ensure the code is in $STARTREK_DIR/voice-assistant"
            exit 1
        fi
    fi
    
    build_voice_assistant
    create_service_user
    install_service_files
    configure_startup
    
    # Wait a moment for services to stabilize
    sleep 10
    
    run_tests
    show_completion_message
}

# Run main function
main "$@"
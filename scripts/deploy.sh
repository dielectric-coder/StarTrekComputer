#!/bin/bash

# Star Trek Computer Deployment Script
# Deploys the system as a service with auto-startup

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Configuration
INSTALL_DIR="/opt/startrek"
SERVICE_USER="startrek"
SERVICE_GROUP="audio"
CURRENT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

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

check_prerequisites() {
    log_info "Checking prerequisites..."
    
    # Check if running as root for service installation
    if [[ $EUID -ne 0 ]]; then
        log_error "This script must be run as root (use sudo)"
        exit 1
    fi
    
    # Check if voice assistant is built
    if [[ ! -f "$CURRENT_DIR/voice-assistant/target/release/voice-assistant" ]]; then
        log_error "Voice assistant not built. Run 'cargo build --release' first."
        exit 1
    fi
    
    # Check if required components exist
    local missing_components=()
    
    [[ ! -d "$CURRENT_DIR/whisper.cpp" ]] && missing_components+=("whisper.cpp")
    [[ ! -d "$CURRENT_DIR/piper" ]] && missing_components+=("piper")
    [[ ! -f "$CURRENT_DIR/search_tool.py" ]] && missing_components+=("search_tool.py")
    
    if [[ ${#missing_components[@]} -gt 0 ]]; then
        log_error "Missing components: ${missing_components[*]}"
        log_error "Please run the installation script first"
        exit 1
    fi
    
    log_success "Prerequisites check passed"
}

create_users_and_dirs() {
    log_info "Creating service user and directories..."
    
    # Create ollama user if it doesn't exist
    if ! id "ollama" >/dev/null 2>&1; then
        useradd -r -s /bin/false -d /var/lib/ollama -m ollama
        log_info "Created ollama user"
    fi
    
    # Create startrek service user if it doesn't exist
    if ! id "$SERVICE_USER" >/dev/null 2>&1; then
        useradd -r -s /bin/false -d /var/lib/startrek-computer -G audio $SERVICE_USER
        log_info "Created $SERVICE_USER user"
    fi
    
    # Create directories
    mkdir -p /var/lib/startrek-computer
    mkdir -p /var/lib/ollama/models
    mkdir -p $INSTALL_DIR
    
    # Set permissions
    chown $SERVICE_USER:$SERVICE_GROUP /var/lib/startrek-computer
    chown ollama:ollama /var/lib/ollama
    
    log_success "Users and directories created"
}

deploy_application() {
    log_info "Deploying application files..."
    
    # Stop services if running
    systemctl stop startrek-computer.service 2>/dev/null || true
    systemctl stop startrek-ollama.service 2>/dev/null || true
    
    # Copy application files
    cp -r $CURRENT_DIR $INSTALL_DIR/startrek
    chown -R $SERVICE_USER:$SERVICE_GROUP $INSTALL_DIR
    
    # Create executable symlinks
    ln -sf $INSTALL_DIR/StarTrekComputer/voice-assistant/target/release/voice-assistant /usr/local/bin/startrek-computer
    
    # Make scripts executable
    chmod +x $INSTALL_DIR/startrek/install.sh
    chmod +x $INSTALL_DIR/startrek/search_tool.py
    
    log_success "Application deployed"
}

install_systemd_services() {
    log_info "Installing systemd services..."
    
    # Install service files
    cp $CURRENT_DIR/systemd/startrek-computer.service /etc/systemd/system/
    cp $CURRENT_DIR/systemd/startrek-ollama.service /etc/systemd/system/
    
    # Reload systemd
    systemctl daemon-reload
    
    log_success "Systemd services installed"
}

configure_audio() {
    log_info "Configuring audio system..."
    
    # Create ALSA configuration
    cat > /etc/asound.conf <<EOF
pcm.!default {
    type pulse
}
ctl.!default {
    type pulse
}
EOF

    # Configure audio permissions
    usermod -a -G audio $SERVICE_USER
    
    # Create udev rule for audio devices
    cat > /etc/udev/rules.d/99-startrek-audio.rules <<EOF
# Star Trek Computer audio device permissions
SUBSYSTEM=="sound", GROUP="audio", MODE="0664"
KERNEL=="controlC[0-9]*", GROUP="audio", MODE="0664"
EOF

    log_success "Audio system configured"
}

create_config_template() {
    log_info "Creating configuration template..."
    
    # Create config directory
    mkdir -p /var/lib/startrek-computer
    
    # Create default configuration
    cat > /var/lib/startrek-computer/config.toml <<EOF
[audio]
sample_rate = 16000
channels = 1
buffer_size = 1024
input_device = null
output_device = null

[stt]
engine = "whisper"
model_path = "$INSTALL_DIR/startrek/whisper.cpp/models/ggml-base.en.bin"
language = "en"

[tts]
engine = "piper"
model_path = "$INSTALL_DIR/startrek/piper/voices/en_US-lessac-medium.onnx"
voice = "lessac"
speed = 0.9

[llm]
base_url = "http://localhost:11434"
model = "qwen3:4b"
context_length = 2048
temperature = 0.3

[search]
enabled = true
search_script = "$INSTALL_DIR/startrek/search_tool.py"
max_results = 3

[wake_word]
enabled = true
wake_word = "computer"
sensitivity = 0.5

[general]
log_level = "info"
data_dir = "/var/lib/startrek-computer"
EOF

    chown $SERVICE_USER:$SERVICE_GROUP /var/lib/startrek-computer/config.toml
    
    log_success "Configuration template created"
}

create_maintenance_scripts() {
    log_info "Creating maintenance scripts..."
    
    # Health check script
    cat > $INSTALL_DIR/startrek/healthcheck.sh <<'EOF'
#!/bin/bash

echo "=== Star Trek Computer Health Check ==="
echo "Date: $(date)"
echo

# Check services
echo "Services:"
systemctl is-active startrek-ollama startrek-computer 2>/dev/null || echo "Some services not running"
echo

# Check audio devices
echo "Audio devices:"
aplay -l 2>/dev/null | grep -E "card|device" | head -5 || echo "No audio devices found"
echo

# Check models
echo "Models:"
ls -la $INSTALL_DIR/startrek/whisper.cpp/models/ggml-*.bin 2>/dev/null || echo "Whisper models not found"
ls -la $INSTALL_DIR/startrek/piper/voices/*.onnx 2>/dev/null || echo "TTS models not found"
echo

# Check recent errors
echo "Recent errors:"
journalctl -u startrek-computer --since "1 hour ago" 2>/dev/null | grep -i error | tail -5 || echo "No recent errors"
echo

# Test components
echo "Component tests:"
timeout 5 curl -s http://localhost:11434/api/tags >/dev/null 2>&1 && echo "✓ Ollama responding" || echo "✗ Ollama not responding"
test -f $INSTALL_DIR/startrek/search_tool.py && echo "✓ Search tool present" || echo "✗ Search tool missing"
echo

echo "=== Health check complete ==="
EOF

    # Update script
    cat > $INSTALL_DIR/startrek/update.sh <<'EOF'
#!/bin/bash

set -e

echo "=== Star Trek Computer Update ==="

# Backup current config
cp /var/lib/startrek-computer/config.toml /var/lib/startrek-computer/config.toml.backup.$(date +%Y%m%d_%H%M%S) 2>/dev/null || true

# Stop services
systemctl stop startrek-computer.service

# Update code (this would pull from git in a real deployment)
echo "Code update would happen here..."

# Rebuild
cd $INSTALL_DIR/startrek/voice-assistant
cargo build --release

# Restart services
systemctl start startrek-computer.service

echo "Update complete. Status:"
systemctl status startrek-computer --no-pager -l
EOF

    # Log viewer script
    cat > $INSTALL_DIR/startrek/logs.sh <<'EOF'
#!/bin/bash

case "${1:-live}" in
    "live"|"f"|"follow")
        echo "Following live logs (Ctrl+C to exit):"
        journalctl -u startrek-computer -u startrek-ollama -f --no-pager
        ;;
    "recent"|"r")
        echo "Recent logs (last hour):"
        journalctl -u startrek-computer -u startrek-ollama --since "1 hour ago" --no-pager
        ;;
    "errors"|"e")
        echo "Recent errors:"
        journalctl -u startrek-computer -u startrek-ollama --since "24 hours ago" | grep -i error
        ;;
    "help"|"h")
        echo "Usage: $0 [live|recent|errors|help]"
        echo "  live/f     - Follow live logs (default)"
        echo "  recent/r   - Show logs from last hour"
        echo "  errors/e   - Show recent errors only"
        echo "  help/h     - Show this help"
        ;;
    *)
        echo "Unknown option: $1"
        echo "Use '$0 help' for usage information"
        exit 1
        ;;
esac
EOF

    # Make scripts executable
    chmod +x $INSTALL_DIR/startrek/healthcheck.sh
    chmod +x $INSTALL_DIR/startrek/update.sh
    chmod +x $INSTALL_DIR/startrek/logs.sh
    
    log_success "Maintenance scripts created"
}

enable_services() {
    log_info "Enabling and starting services..."
    
    # Enable services
    systemctl enable startrek-ollama.service
    systemctl enable startrek-computer.service
    
    # Start ollama first
    systemctl start startrek-ollama.service
    
    # Wait for ollama to be ready
    log_info "Waiting for Ollama to start..."
    for i in {1..30}; do
        if curl -s http://localhost:11434/api/tags >/dev/null 2>&1; then
            break
        fi
        sleep 2
    done
    
    # Start main service
    systemctl start startrek-computer.service
    
    # Check status
    sleep 5
    if systemctl is-active --quiet startrek-computer; then
        log_success "Services started successfully"
    else
        log_warn "Service may not be running correctly. Check logs."
    fi
}

create_boot_optimization() {
    log_info "Creating boot optimization..."
    
    # Create boot optimization script
    cat > /etc/systemd/system/startrek-boot-optimize.service <<EOF
[Unit]
Description=Star Trek Computer Boot Optimization
Before=startrek-computer.service
After=sound.target

[Service]
Type=oneshot
ExecStart=/bin/bash -c 'echo performance | tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor'
ExecStart=/bin/bash -c 'echo 0 > /proc/sys/vm/swappiness'
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
EOF

    systemctl enable startrek-boot-optimize.service
    
    log_success "Boot optimization configured"
}

show_completion() {
    echo
    echo "=================================================================="
    log_success "Star Trek Computer Deployment Complete!"
    echo "=================================================================="
    echo
    echo "Services installed and enabled:"
    echo "  ✓ startrek-ollama.service    - LLM backend"  
    echo "  ✓ startrek-computer.service  - Main voice assistant"
    echo
    echo "Management commands:"
    echo "  systemctl status startrek-computer     - Check service status"
    echo "  systemctl stop startrek-computer       - Stop service"
    echo "  systemctl start startrek-computer      - Start service"
    echo "  systemctl restart startrek-computer    - Restart service"
    echo
    echo "Maintenance scripts:"
    echo "  $INSTALL_DIR/startrek/healthcheck.sh   - System health check"
    echo "  $INSTALL_DIR/startrek/logs.sh          - View logs"
    echo "  $INSTALL_DIR/startrek/update.sh        - Update system"
    echo
    echo "Configuration:"
    echo "  /var/lib/startrek-computer/config.toml - Main configuration file"
    echo
    echo "The system will now start automatically on boot."
    echo "Say 'Computer, [your request]' to interact with the system."
    echo
    log_info "Checking current status..."
    systemctl status startrek-computer --no-pager -l || true
}

# Main deployment process
main() {
    echo "=================================================================="
    echo "Star Trek Computer Deployment"
    echo "=================================================================="
    echo
    
    check_prerequisites
    create_users_and_dirs
    deploy_application
    install_systemd_services
    configure_audio
    create_config_template
    create_maintenance_scripts
    enable_services
    create_boot_optimization
    show_completion
}

# Run main function
main "$@"
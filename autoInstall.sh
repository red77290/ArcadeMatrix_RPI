#!/bin/bash
# ArcadeMatrix_RPi Auto-Installation Script (Non-Interactive / Rust Native)
# Recommended OS: Raspberry Pi OS Lite (32-bit or 64-bit)
set -e

echo "======================================"
echo "    ArcadeMatrix RPi Auto-Installer   "
echo "======================================"
echo "⚠️ WARNING for Raspberry Pi 5 users: "
echo "The hzeller rgb-led-matrix library does NOT support the Pi 5 natively"
echo "due to the new RP1 GPIO chip. You MUST use an active adapter board."
echo "Pi 3, Pi 4, and Zero 2 W are fully supported out of the box."
echo "======================================"

# 1. Stop & clean existing Python/Rust services if installed
echo "Stopping existing ArcadeMatrix services & cleaning legacy Python files..."
sudo systemctl stop arcadematrix.service arcadematrix_py.service matrix.service 2>/dev/null || true
sudo systemctl disable arcadematrix.service arcadematrix_py.service matrix.service 2>/dev/null || true
sudo rm -f /etc/systemd/system/arcadematrix_py.service /etc/systemd/system/matrix.service
rm -rf venv/ .venv/ env/ __pycache__/ *.pyc

# 2. Update and install system dependencies (Linux / Debian)
if command -v apt-get &> /dev/null; then
    echo "Updating packages and installing system dependencies..."
    sudo apt-get update
    sudo DEBIAN_FRONTEND=noninteractive apt-get install -y git build-essential curl mosquitto mosquitto-clients libssl-dev pkg-config

    # Configure Mosquitto to allow external anonymous connections (Required for Recalbox/Batocera)
    echo "Configuring Mosquitto MQTT Broker..."
    sudo bash -c 'echo -e "listener 1883 0.0.0.0\nallow_anonymous true" > /etc/mosquitto/conf.d/arcadematrix.conf'
    sudo systemctl restart mosquitto || true
fi

# 3. Check if we are in the project root
if [ -z "$SKIP_BUILD" ]; then
    if [ ! -f "Cargo.toml" ]; then
        echo "Cargo.toml not found. It looks like you ran this script standalone."
        ACTUAL_USER=${SUDO_USER:-$USER}
        ACTUAL_HOME=$(eval echo ~$ACTUAL_USER)
        cd "$ACTUAL_HOME" || true
        
        if [ -d "ArcadeMatrix_RPi" ]; then
            echo "Directory ArcadeMatrix_RPi already exists, updating via git pull..."
            cd ArcadeMatrix_RPi
            git pull || true
        elif [ -d "ArcadeMatrix_RPI" ]; then
            echo "Directory ArcadeMatrix_RPI already exists, updating via git pull..."
            cd ArcadeMatrix_RPI
            git pull || true
        else
            echo "Cloning the ArcadeMatrix_RPi repository..."
            git clone https://github.com/red77290/ArcadeMatrix_RPI.git ArcadeMatrix_RPi
            cd ArcadeMatrix_RPi || { echo "Failed to enter directory"; exit 1; }
        fi
    else
        echo "Found Cargo.toml, proceeding with local files..."
    fi
else
    echo "SKIP_BUILD is set, skipping git clone."
    ACTUAL_USER=${SUDO_USER:-$USER}
    ACTUAL_HOME=$(eval echo ~$ACTUAL_USER)
    
        echo "Navigating to existing repository at $ACTUAL_HOME/ArcadeMatrix_RPi"
        cd "$ACTUAL_HOME/ArcadeMatrix_RPi" || true
    elif [ -d "$ACTUAL_HOME/ArcadeMatrix_RPI" ]; then
        echo "Navigating to existing repository at $ACTUAL_HOME/ArcadeMatrix_RPI"
        cd "$ACTUAL_HOME/ArcadeMatrix_RPI" || true
    else
        echo "WARNING: Could not find ArcadeMatrix_RPi in $ACTUAL_HOME"
    fi
fi

CURRENT_DIR=$(pwd)

if [ -z "$SKIP_BUILD" ]; then
    # 4. Install Rust toolchain & Compile binary
    if ! command -v cargo &> /dev/null; then
        echo "Installing Rust toolchain..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env" || true
        export PATH="$HOME/.cargo/bin:$PATH"
    fi

    echo "Compiling ArcadeMatrix Rust binary (release mode)..."
    cargo build --release

    echo "Copying binary to project root..."
    cp target/release/arcadematrix ./arcadematrix
    chmod +x ./arcadematrix
else
    echo "SKIP_BUILD is set, skipping Rust compilation..."
fi

# 5. Anti-Flicker Performance Tweaks (Disable Audio - Raspberry Pi only)
if [ -d "/boot" ] || [ -d "/boot/firmware" ]; then
    echo "Applying anti-flicker optimizations (Disabling Onboard Audio)..."

    # Blacklist sound module
    if [ -d "/etc/modprobe.d" ]; then
        sudo bash -c "cat > /etc/modprobe.d/snd-blacklist.conf << EOF
blacklist snd_bcm2835
EOF"
    fi

    # Disable audio in config.txt (handling both older OS and Bookworm paths)
    CONFIG_TXT="/boot/config.txt"
    if [ -f "/boot/firmware/config.txt" ]; then
        CONFIG_TXT="/boot/firmware/config.txt"
    fi

    if [ -f "$CONFIG_TXT" ]; then
        if grep -q "dtparam=audio=on" "$CONFIG_TXT"; then
            sudo sed -i 's/dtparam=audio=on/dtparam=audio=off/g' "$CONFIG_TXT"
            echo "Disabled audio in $CONFIG_TXT"
        elif ! grep -q "dtparam=audio=off" "$CONFIG_TXT"; then
            echo "dtparam=audio=off" | sudo tee -a "$CONFIG_TXT" > /dev/null
        fi

        # Disable HDMI audio loaded by vc4 driver which causes PWM conflicts
        if grep -q "dtoverlay=vc4-kms-v3d$" "$CONFIG_TXT"; then
            sudo sed -i 's/dtoverlay=vc4-kms-v3d$/dtoverlay=vc4-kms-v3d,noaudio/g' "$CONFIG_TXT"
            echo "Disabled vc4 HDMI audio in $CONFIG_TXT"
        fi
    fi

    CMDLINE_TXT="/boot/cmdline.txt"
    if [ -f "/boot/firmware/cmdline.txt" ]; then
        CMDLINE_TXT="/boot/firmware/cmdline.txt"
    fi

    if [ -f "$CMDLINE_TXT" ] && ! grep -q "isolcpus=" "$CMDLINE_TXT"; then
        sudo sed -i '1 s/$/ isolcpus=3/' "$CMDLINE_TXT"
        echo "Isolated CPU core 3 in $CMDLINE_TXT for LED matrix"
    fi
fi

# Disable triggerhappy service which is known to cause PWM flickering
sudo systemctl disable triggerhappy 2>/dev/null || true

# 6. Setup Systemd Service (Linux only)
if command -v systemctl &> /dev/null; then
    sudo systemctl disable triggerhappy 2>/dev/null || true
    echo "Setting up systemd service for auto-start..."
    SERVICE_FILE="/etc/systemd/system/arcadematrix.service"

    # No CPUAffinity here: we rely on isolcpus=3 in cmdline.txt (like the Python version).
    # The kernel reserves core 3 for the hzeller DMA thread, and the OS scheduler
    # freely distributes our process across cores 0, 1, 2.

    sudo bash -c "cat > $SERVICE_FILE <<EOF
[Unit]
Description=ArcadeMatrix RPi Daemon (Rust)
After=network.target

[Service]
ExecStart=$CURRENT_DIR/arcadematrix
WorkingDirectory=$CURRENT_DIR
StandardOutput=inherit
StandardError=inherit
Restart=always
RestartSec=3
TimeoutStopSec=10
User=root

[Install]
WantedBy=multi-user.target
EOF"

    sudo systemctl daemon-reload
    sudo systemctl enable arcadematrix.service
    sudo systemctl restart arcadematrix.service || echo "Warning: Could not start service (this is normal in chroot)"
fi

echo "======================================"
echo "Installation Complete (Rust Mode)!"
echo "======================================"

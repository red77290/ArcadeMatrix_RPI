#!/bin/bash
# ArcadeMatrix Smart Deploy (macOS / Linux)
# Auto-detects the Raspberry Pi architecture, builds via Docker, and deploys.

set -e

PI_IP=${1:-192.168.1.149}
PI_USER=${2:-pi}
PI_PASS=${3:-raspberry}

if ! command -v sshpass &> /dev/null; then
    echo "❌ Error: sshpass is not installed."
    echo "macOS: brew install hudochenkov/sshpass/sshpass"
    echo "Linux: sudo apt install sshpass"
    exit 1
fi

echo "=========================================================="
echo "      ArcadeMatrix Smart Deploy to Raspberry Pi"
echo "=========================================================="
echo "🚀 Connecting to ${PI_USER}@${PI_IP} to detect OS architecture..."

# Detect remote architecture
REMOTE_ARCH=$(sshpass -p "${PI_PASS}" ssh -o StrictHostKeyChecking=no "${PI_USER}@${PI_IP}" "uname -m")

if [ "$REMOTE_ARCH" = "aarch64" ]; then
    echo "✅ Detected 64-bit OS (aarch64) on Raspberry Pi."
    BIN_PATH="target/aarch64-unknown-linux-gnu/release/arcadematrix"
elif [[ "$REMOTE_ARCH" == armv* ]]; then
    echo "✅ Detected 32-bit OS ($REMOTE_ARCH) on Raspberry Pi."
    BIN_PATH="target/armv7-unknown-linux-gnueabihf/release/arcadematrix"
else
    echo "❌ Unknown architecture: $REMOTE_ARCH"
    exit 1
fi

echo "🔨 1. Building binary for $REMOTE_ARCH using Docker..."
bash scripts/build.sh "$REMOTE_ARCH"

if [ ! -f "$BIN_PATH" ]; then
    echo "❌ Error: Compiled binary not found at $BIN_PATH"
    exit 1
fi

echo "🛑 2. Stopping arcadematrix service on Raspberry Pi..."
sshpass -p "${PI_PASS}" ssh -o StrictHostKeyChecking=no "${PI_USER}@${PI_IP}" "echo '${PI_PASS}' | sudo -S systemctl stop arcadematrix.service || true"

echo "🚀 Detecting project directory on Raspberry Pi..."
TARGET_DIR=$(sshpass -p "${PI_PASS}" ssh -o StrictHostKeyChecking=no "${PI_USER}@${PI_IP}" "ls -d /home/${PI_USER}/ArcadeMatrix_RP* 2>/dev/null | head -n 1")

if [ -z "$TARGET_DIR" ]; then
    TARGET_DIR="/home/${PI_USER}/ArcadeMatrix_RPi"
    sshpass -p "${PI_PASS}" ssh -o StrictHostKeyChecking=no "${PI_USER}@${PI_IP}" "mkdir -p $TARGET_DIR"
fi
echo "✅ Target directory: $TARGET_DIR"

echo "📤 3. Uploading correct binary to $TARGET_DIR..."
sshpass -p "${PI_PASS}" scp -o StrictHostKeyChecking=no "$BIN_PATH" "${PI_USER}@${PI_IP}:$TARGET_DIR/arcadematrix"

echo "⚙️  4. Starting service..."
sshpass -p "${PI_PASS}" ssh -o StrictHostKeyChecking=no "${PI_USER}@${PI_IP}" "
    chmod +x $TARGET_DIR/arcadematrix && \
    echo '🔧 Forcing correct systemd service configuration...' && \
    echo '${PI_PASS}' | sudo -S bash -c 'cat > /etc/systemd/system/arcadematrix.service <<EOF
[Unit]
Description=ArcadeMatrix RPi Daemon (Rust)
After=network.target

[Service]
ExecStart=$TARGET_DIR/arcadematrix
WorkingDirectory=$TARGET_DIR
StandardOutput=inherit
StandardError=inherit
Restart=always
RestartSec=3
TimeoutStopSec=10
User=root

[Install]
WantedBy=multi-user.target
EOF' && \
    echo '${PI_PASS}' | sudo -S systemctl daemon-reload && \
    echo '${PI_PASS}' | sudo -S systemctl enable arcadematrix.service && \
    echo '${PI_PASS}' | sudo -S systemctl restart arcadematrix.service"

echo "✅ Deployment successful!"
echo "=========================================================="

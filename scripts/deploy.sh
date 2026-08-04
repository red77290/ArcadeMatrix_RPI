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

echo "📤 3. Uploading correct binary..."
sshpass -p "${PI_PASS}" scp -o StrictHostKeyChecking=no "$BIN_PATH" "${PI_USER}@${PI_IP}:/home/${PI_USER}/arcadematrix_temp"

echo "⚙️  4. Moving binary and starting service..."
sshpass -p "${PI_PASS}" ssh -o StrictHostKeyChecking=no "${PI_USER}@${PI_IP}" "
    echo '${PI_PASS}' | sudo -S mv /home/${PI_USER}/arcadematrix_temp /usr/local/bin/arcadematrix && \
    echo '${PI_PASS}' | sudo -S chmod +x /usr/local/bin/arcadematrix && \
    if systemctl list-unit-files | grep -q arcadematrix.service; then
        echo '✅ Service already installed, restarting only...'
        echo '${PI_PASS}' | sudo -S systemctl restart arcadematrix.service
    else
        echo '⚠️ Service not found, running full autoInstall.sh setup...'
        echo '${PI_PASS}' | sudo -S env SKIP_BUILD=1 bash /home/${PI_USER}/autoInstall.sh
    fi"

echo "✅ Deployment successful!"
echo "=========================================================="

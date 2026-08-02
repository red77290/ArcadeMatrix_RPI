#!/bin/bash
# ArcadeMatrix Smart Deploy
# Auto-detects the Raspberry Pi architecture and deploys the correct binary

set -e

PI_HOST=${1:-pi@raspberrypi.local}

echo "=========================================================="
echo "      ArcadeMatrix Smart Deploy to Raspberry Pi"
echo "=========================================================="
echo "🚀 Connecting to $PI_HOST to detect OS architecture..."

# Detect remote architecture
REMOTE_ARCH=$(ssh "$PI_HOST" "uname -m")

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

if [ ! -f "$BIN_PATH" ]; then
    echo "❌ Error: Compiled binary not found at $BIN_PATH"
    echo "Please run 'bash scripts/build_local.sh' first to generate multi-arch binaries!"
    exit 1
fi

echo "1. Uploading correct binary..."
scp "$BIN_PATH" "$PI_HOST:/tmp/arcadematrix"

echo "2. Installing binary and restarting service..."
ssh "$PI_HOST" "sudo mv /tmp/arcadematrix /usr/local/bin/arcadematrix && sudo chmod +x /usr/local/bin/arcadematrix && sudo systemctl restart arcadematrix"

echo "✅ Deployment successful! Service has been restarted."
echo "=========================================================="

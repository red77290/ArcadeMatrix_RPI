#!/bin/bash
# ArcadeMatrix Fast Deploy
# Copies the locally compiled binary to a Raspberry Pi and restarts the service

set -e

PI_HOST=${1:-pi@raspberrypi.local}
BIN_PATH="target/aarch64-unknown-linux-gnu/release/arcadematrix"

echo "=========================================================="
echo "      ArcadeMatrix Fast Deploy to Raspberry Pi"
echo "=========================================================="

if [ ! -f "$BIN_PATH" ]; then
    echo "❌ Error: Compiled binary not found at $BIN_PATH"
    echo "Please run 'bash scripts/build_local.sh' first!"
    exit 1
fi

echo "🚀 Deploying to $PI_HOST..."
echo "1. Uploading binary..."
scp "$BIN_PATH" "$PI_HOST:/tmp/arcadematrix"

echo "2. Installing binary and restarting service..."
ssh "$PI_HOST" "sudo mv /tmp/arcadematrix /usr/local/bin/arcadematrix && sudo chmod +x /usr/local/bin/arcadematrix && sudo systemctl restart arcadematrix"

echo "✅ Deployment successful! Service has been restarted."
echo "=========================================================="

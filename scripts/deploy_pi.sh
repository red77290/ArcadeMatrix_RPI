#!/bin/bash
set -e

PI_IP="192.168.1.169"
PI_USER="pi"
PI_PASS="raspberry"
BIN_PATH="target/aarch64-unknown-linux-gnu/release/arcadematrix"

if ! command -v sshpass &> /dev/null; then
    echo "❌ Error: sshpass is not installed."
    echo "Please install it using 'brew install eugene' or 'brew install hudochenkov/sshpass/sshpass' on macOS, or 'sudo apt install sshpass' on Linux."
    exit 1
fi

echo "========================================="
echo "   🚀 ArcadeMatrix RPi Deploy Script"
echo "========================================="

echo "🔨 1. Building 64-bit binary..."
bash scripts/build_local_64.sh

if [ ! -f "$BIN_PATH" ]; then
    echo "❌ Error: Binary not found at $BIN_PATH. Build failed?"
    exit 1
fi

echo "🛑 2. Stopping arcadematrix service on Raspberry Pi..."
sshpass -p "${PI_PASS}" ssh -4 ${PI_USER}@${PI_IP} "echo '${PI_PASS}' | sudo -S systemctl stop arcadematrix.service || true"

echo "📤 3. Uploading new binary to Raspberry Pi..."
sshpass -p "${PI_PASS}" scp -4 ${BIN_PATH} ${PI_USER}@${PI_IP}:/home/${PI_USER}/arcadematrix_temp

echo "⚙️  4. Moving binary, setting permissions, and restarting service..."
sshpass -p "${PI_PASS}" ssh -4 ${PI_USER}@${PI_IP} "echo '${PI_PASS}' | sudo -S mv /home/${PI_USER}/arcadematrix_temp /usr/local/bin/arcadematrix && \
                         echo '${PI_PASS}' | sudo -S chmod +x /usr/local/bin/arcadematrix && \
                         echo '${PI_PASS}' | sudo -S systemctl restart arcadematrix.service"

echo "✅ Deployment successful! Service restarted."

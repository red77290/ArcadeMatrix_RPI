#!/bin/bash
# ArcadeMatrix Smart Deploy (macOS / Linux)
# Auto-detects the Raspberry Pi architecture, builds via Docker, and deploys.

set -e

if [ -f "scripts/defaults.sh" ]; then
    source scripts/defaults.sh
fi

PI_IP="192.168.1.149"
PI_USER="${AM_USER:-pi}"
PI_PASS="${AM_PASS:-raspberry}"
SKIP_BUILD=0
BIN_PATH=""

while [[ "$#" -gt 0 ]]; do
    case $1 in
        --ip) PI_IP="$2"; shift ;;
        --user) PI_USER="$2"; shift ;;
        --pass) PI_PASS="$2"; shift ;;
        --skip-build) SKIP_BUILD=1 ;;
        --binary-path) BIN_PATH="$2"; shift ;;
        *) echo "Unknown parameter passed: $1"; exit 1 ;;
    esac
    shift
done

if ! command -v sshpass &> /dev/null; then
    echo "[Error] sshpass is not installed."
    echo "macOS: brew install hudochenkov/sshpass/sshpass"
    echo "Linux: sudo apt install sshpass"
    exit 1
fi

echo "=========================================================="
echo "      ArcadeMatrix Smart Deploy to Raspberry Pi"
echo "=========================================================="
echo "[Info] Connecting to ${PI_USER}@${PI_IP} to detect OS architecture..."

REMOTE_ARCH=$(sshpass -p "${PI_PASS}" ssh -o StrictHostKeyChecking=no "${PI_USER}@${PI_IP}" "uname -m")

if [ "$REMOTE_ARCH" = "aarch64" ]; then
    echo "[OK] Detected 64-bit OS (aarch64) on Raspberry Pi."
    DEFAULT_BIN_PATH="target/aarch64-unknown-linux-gnu/release/arcadematrix"
elif [[ "$REMOTE_ARCH" == armv* ]]; then
    echo "[OK] Detected 32-bit OS ($REMOTE_ARCH) on Raspberry Pi."
    DEFAULT_BIN_PATH="target/armv7-unknown-linux-gnueabihf/release/arcadematrix"
else
    echo "[Error] Unknown architecture: $REMOTE_ARCH"
    exit 1
fi

if [ -n "$BIN_PATH" ]; then
    echo "[Info] Using custom binary path: $BIN_PATH"
elif [ "$SKIP_BUILD" -eq 1 ]; then
    echo "[Info] SkipBuild is set. Skipping Docker build..."
    BIN_PATH=$DEFAULT_BIN_PATH
else
    echo "[Step 1] Building binary for $REMOTE_ARCH using Docker..."
    if ! command -v docker &> /dev/null; then
        echo "[Warning] Docker is not installed. Attempting to deploy existing binary if available..."
        echo "If this fails, you can specify a pre-compiled binary with: ./deploy.sh --binary-path 'path/to/arcadematrix'"
        BIN_PATH=$DEFAULT_BIN_PATH
    else
        bash scripts/build.sh "$REMOTE_ARCH"
        BIN_PATH=$DEFAULT_BIN_PATH
    fi
fi

if [ ! -f "$BIN_PATH" ]; then
    echo "[Error] Compiled binary not found at $BIN_PATH"
    echo "Please compile the project, or download the pre-compiled binary from GitHub Actions and provide it via --binary-path"
    exit 1
fi

CHECK_SERVICE=$(sshpass -p "${PI_PASS}" ssh -o StrictHostKeyChecking=no "${PI_USER}@${PI_IP}" "if systemctl list-unit-files | grep -q arcadematrix.service; then echo 'installed'; else echo 'missing'; fi")

if [ "$CHECK_SERVICE" = "missing" ]; then
    echo "[Warning] Service not found, running full autoInstall.sh setup first..."
    sshpass -p "${PI_PASS}" ssh -o StrictHostKeyChecking=no "${PI_USER}@${PI_IP}" "echo '${PI_PASS}' | sudo -S env SKIP_BUILD=1 bash -c \"\$(curl -sSL https://raw.githubusercontent.com/red77290/ArcadeMatrix_RPI/main/autoInstall.sh)\""
fi

echo "[Step 2] Stopping arcadematrix service on Raspberry Pi..."
sshpass -p "${PI_PASS}" ssh -o StrictHostKeyChecking=no "${PI_USER}@${PI_IP}" "echo '${PI_PASS}' | sudo -S systemctl stop arcadematrix.service || true"

TARGET_DIR="/home/${PI_USER}/ArcadeMatrix_RPi"

echo "[Step 3] Uploading correct binary to $TARGET_DIR..."
sshpass -p "${PI_PASS}" ssh -o StrictHostKeyChecking=no "${PI_USER}@${PI_IP}" "mkdir -p $TARGET_DIR"
sshpass -p "${PI_PASS}" scp -o StrictHostKeyChecking=no "$BIN_PATH" "${PI_USER}@${PI_IP}:$TARGET_DIR/arcadematrix_temp"

echo "[Step 4] Moving binary and starting service..."
sshpass -p "${PI_PASS}" ssh -o StrictHostKeyChecking=no "${PI_USER}@${PI_IP}" "echo '${PI_PASS}' | sudo -S mv $TARGET_DIR/arcadematrix_temp $TARGET_DIR/arcadematrix"
sshpass -p "${PI_PASS}" ssh -o StrictHostKeyChecking=no "${PI_USER}@${PI_IP}" "echo '${PI_PASS}' | sudo -S chmod +x $TARGET_DIR/arcadematrix"

echo "[OK] Service installed, restarting..."
sshpass -p "${PI_PASS}" ssh -o StrictHostKeyChecking=no "${PI_USER}@${PI_IP}" "echo '${PI_PASS}' | sudo -S systemctl restart arcadematrix.service"

echo "[Success] Deployment successful!"
echo "=========================================================="

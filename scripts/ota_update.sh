#!/bin/bash
# ArcadeMatrix RPi OTA Updater Script
set -e

LOG_FILE="/tmp/arcadematrix_ota.log"
exec > "$LOG_FILE" 2>&1

echo "[$(date)] --- ArcadeMatrix OTA Update Starting ---"

TEMP_BIN="/tmp/arcadematrix_update"
TARGET_BIN="${1:-/home/pi/ArcadeMatrix_RPi/arcadematrix}"

echo "Target binary location: $TARGET_BIN"
echo "Temporary binary: $TEMP_BIN"

if [ ! -f "$TEMP_BIN" ]; then
    echo "ERROR: Uploaded binary $TEMP_BIN not found!"
    exit 1
fi

chmod +x "$TEMP_BIN"

# Give web server 1.5 seconds to flush HTTP 200 response to client
sleep 1.5

# Step 1: Stop systemd service
if command -v systemctl &>/dev/null; then
    if systemctl is-active --quiet arcadematrix.service; then
        echo "Stopping arcadematrix.service..."
        systemctl stop arcadematrix.service || true
    elif systemctl is-active --quiet arcadematrix; then
        echo "Stopping arcadematrix unit..."
        systemctl stop arcadematrix || true
    fi
fi

# Step 2: Ensure all lingering instances of arcadematrix are terminated
CURRENT_PID=$$
for pid in $(pgrep -f "arcadematrix" 2>/dev/null); do
    if [ "$pid" != "$CURRENT_PID" ]; then
        echo "Terminating process $pid..."
        kill -15 "$pid" 2>/dev/null || true
    fi
done
sleep 1

# Force kill if still running
for pid in $(pgrep -f "arcadematrix" 2>/dev/null); do
    if [ "$pid" != "$CURRENT_PID" ]; then
        echo "Force killing process $pid..."
        kill -9 "$pid" 2>/dev/null || true
    fi
done

# Step 3: Replace binary
echo "Overwriting target binary $TARGET_BIN..."
cp -f "$TEMP_BIN" "$TARGET_BIN"
chmod +x "$TARGET_BIN"
rm -f "$TEMP_BIN"
echo "Binary successfully replaced."

# Step 4: Restart systemd service or relaunch daemon
if command -v systemctl &>/dev/null && systemctl list-unit-files 2>/dev/null | grep -q "^arcadematrix.service"; then
    echo "Starting arcadematrix.service via systemctl..."
    systemctl start arcadematrix.service
elif [ -x "$TARGET_BIN" ]; then
    echo "Relaunching $TARGET_BIN in background..."
    nohup "$TARGET_BIN" >/dev/null 2>&1 &
fi

echo "[$(date)] --- OTA Update Finished Successfully ---"

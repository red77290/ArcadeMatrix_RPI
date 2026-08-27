#!/bin/bash
# ArcadeMatrix RPi OTA Updater Script
set -e

LOG_FILE="/tmp/arcadematrix_ota.log"
exec > "$LOG_FILE" 2>&1

echo "[$(date)] --- ArcadeMatrix OTA Update Starting ---"

TEMP_BIN="/tmp/arcadematrix_update"

if [ -n "$1" ]; then
    TARGET_BIN="$1"
else
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    if [ -f "$SCRIPT_DIR/../arcadematrix" ]; then
        TARGET_BIN="$(cd "$SCRIPT_DIR/.." && pwd)/arcadematrix"
    else
        TARGET_BIN="/home/${USER:-pi}/ArcadeMatrix_RPi/arcadematrix"
    fi
fi

if [ ! -f "$TEMP_BIN" ]; then
    echo "ERROR: Uploaded binary $TEMP_BIN not found!"
    exit 1
fi

chmod +x "$TEMP_BIN"

# Give web server 1.5 seconds to flush HTTP 200 response to client
sleep 1.5

# Step 1: Replace binary atomically on disk
echo "Replacing target binary $TARGET_BIN atomically..."
if [ -f "$TARGET_BIN" ]; then
    mv -f "$TARGET_BIN" "${TARGET_BIN}.old" 2>/dev/null || true
fi
mv -f "$TEMP_BIN" "$TARGET_BIN"
chmod +x "$TARGET_BIN"

if [ -f "${TARGET_BIN}.old" ]; then
    OWNER=$(stat -c '%u:%g' "${TARGET_BIN}.old" 2>/dev/null || true)
    if [ -n "$OWNER" ]; then
        chown "$OWNER" "$TARGET_BIN" 2>/dev/null || true
    fi
fi
echo "Binary successfully replaced."

# Step 2: Restart systemd service or relaunch daemon
if command -v systemctl &>/dev/null && (systemctl is-enabled arcadematrix.service &>/dev/null || systemctl list-unit-files 2>/dev/null | grep -q "arcadematrix.service"); then
    echo "Restarting arcadematrix.service via systemctl..."
    systemctl restart arcadematrix.service || systemctl start arcadematrix.service
else
    echo "Standalone mode: Terminating existing process and restarting..."
    CURRENT_PID=$$
    for pid in $(pgrep -f "$TARGET_BIN" 2>/dev/null); do
        if [ "$pid" != "$CURRENT_PID" ]; then
            kill -15 "$pid" 2>/dev/null || true
        fi
    done
    sleep 1
    for pid in $(pgrep -f "$TARGET_BIN" 2>/dev/null); do
        if [ "$pid" != "$CURRENT_PID" ]; then
            kill -9 "$pid" 2>/dev/null || true
        fi
    done
    if [ -x "$TARGET_BIN" ]; then
        nohup "$TARGET_BIN" >/dev/null 2>&1 &
    fi
fi

rm -f "${TARGET_BIN}.old" 2>/dev/null || true
echo "[$(date)] --- OTA Update Finished Successfully ---"

#!/bin/bash
# ArcadeMatrix Recovery Script
# This script runs before the main service starts. It checks the SD card boot partition
# for a recovery firmware. If found, it installs it and backs it up to prevent infinite loops.

PROJ_DIR="$1"
if [ -z "$PROJ_DIR" ]; then
    echo "[Recovery] Error: PROJ_DIR not provided."
    exit 0
fi

# Define possible boot mount points (Bookworm uses /boot/firmware, older uses /boot)
BOOT_PATHS=("/boot/firmware" "/boot")
RECOVERY_FILE="arcadematrix_recovery.bin"

for BOOT_DIR in "${BOOT_PATHS[@]}"; do
    if [ -f "$BOOT_DIR/$RECOVERY_FILE" ]; then
        echo "====================================================="
        echo "🚨 [Recovery] Firmware detected at $BOOT_DIR/$RECOVERY_FILE"
        echo "🚨 [Recovery] Installing recovery firmware..."
        
        # Backup the current binary just in case
        if [ -f "$PROJ_DIR/arcadematrix" ]; then
            mv "$PROJ_DIR/arcadematrix" "$PROJ_DIR/arcadematrix.bak"
        fi
        
        # Install the new binary
        cp "$BOOT_DIR/$RECOVERY_FILE" "$PROJ_DIR/arcadematrix"
        chmod +x "$PROJ_DIR/arcadematrix"
        
        # Rename the recovery file on the SD card so it doesn't run on next boot
        mv "$BOOT_DIR/$RECOVERY_FILE" "$BOOT_DIR/${RECOVERY_FILE}.installed"
        
        echo "✅ [Recovery] Firmware installed successfully."
        echo "====================================================="
        exit 0
    fi
done

# Normal boot, no recovery needed.
exit 0

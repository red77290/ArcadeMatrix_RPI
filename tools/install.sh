#!/usr/bin/env bash
# ArcadeMatrix Gaming OS daemon installer - run this on your PC (macOS/Linux), NOT on the
# retro-gaming device itself. It connects over SSH, auto-detects (or prompts for) whether
# the target is Recalbox, Batocera, or RetroPie, uploads the right daemon/hook script
# (with your ArcadeMatrix device's IP and topic baked in), and starts/reboots the target so
# it starts sending game events over MQTT to system/playing/#.
#
# Requirements: a standard OpenSSH client (ssh/scp), present by default on macOS and virtually all
# Linux distros. `sshpass` is optional but recommended.
set -uo pipefail

RECALBOX_PASS="recalboxroot"
BATOCERA_PASS="linux"
RETROPIE_PASS="raspberry"
SSH_USER=""
SSH_OPTS=(-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=============================================="
echo " ArcadeMatrix Gaming OS Daemon Installer"
echo " (Recalbox / Batocera / RetroPie)"
echo "=============================================="
echo

read -rp "IP address of your Console (Recalbox/Batocera/RetroPie): " TARGET_IP

read -rp "Action (1: Install Daemon, 2: Check Logs) [1]: " ACTION
ACTION=${ACTION:-1}

if [ "$ACTION" = "1" ]; then
    read -rp "IP address of your ArcadeMatrix device (ESP32 or Raspberry Pi): " BROKER_IP
    if [ -z "$BROKER_IP" ]; then
        echo "Broker IP is required for installation. Aborting." >&2
        exit 1
    fi
fi

if [ -z "$TARGET_IP" ]; then
    echo "Target IP address is required. Aborting." >&2
    exit 1
fi

echo
echo "Select Target Console OS:"
echo "  1) Auto-Detect"
echo "  2) Recalbox"
echo "  3) Batocera"
echo "  4) RetroPie"
read -rp "Choice [1]: " OS_CHOICE
OS_CHOICE=${OS_CHOICE:-1}

read -rp "Custom SSH Username (leave blank for defaults): " CUSTOM_USER
read -rp "Custom SSH Password (leave blank for defaults): " CUSTOM_PASS

have_sshpass=0
if command -v sshpass >/dev/null 2>&1; then
    have_sshpass=1
else
    echo
    echo "NOTE: 'sshpass' is not installed - you'll be prompted for the SSH password manually."
    echo "Install sshpass (e.g. 'brew install hudochenkov/sshpass/sshpass' on macOS, 'apt install sshpass' on Debian/Ubuntu) to skip this."
    echo
fi

ssh_run() {
    local user="$1" password="$2"; shift 2
    if [ "$have_sshpass" = "1" ]; then
        sshpass -p "$password" ssh "${SSH_OPTS[@]}" "${user}@${TARGET_IP}" "$@"
    else
        ssh "${SSH_OPTS[@]}" "${user}@${TARGET_IP}" "$@"
    fi
}

scp_run() {
    local user="$1" password="$2" src="$3" dst="$4"
    if [ "$have_sshpass" = "1" ]; then
        cat "$src" | sshpass -p "$password" ssh "${SSH_OPTS[@]}" "${user}@${TARGET_IP}" "cat > '${dst}'"
    else
        cat "$src" | ssh "${SSH_OPTS[@]}" "${user}@${TARGET_IP}" "cat > '${dst}'"
    fi
}

detect_system() {
    local user="$1" password="$2"
    if ssh_run "$user" "$password" "test -d /opt/retropie" 2>/dev/null; then
        echo "retropie"
    elif ssh_run "$user" "$password" "test -d /userdata/system" 2>/dev/null; then
        echo "batocera"
    elif ssh_run "$user" "$password" "test -d /recalbox/share" 2>/dev/null; then
        echo "recalbox"
    else
        echo "unknown"
    fi
}

SYSTEM=""
PASSWORD=""
ACTIVE_USER=""

if [ "$OS_CHOICE" = "2" ]; then
    SYSTEM="recalbox"
elif [ "$OS_CHOICE" = "3" ]; then
    SYSTEM="batocera"
elif [ "$OS_CHOICE" = "4" ]; then
    SYSTEM="retropie"
fi

if [ -n "$CUSTOM_USER" ] && [ -n "$CUSTOM_PASS" ]; then
    ACTIVE_USER="$CUSTOM_USER"
    PASSWORD="$CUSTOM_PASS"
    if [ -z "$SYSTEM" ]; then
        echo "Trying Custom Credentials as $ACTIVE_USER..."
        SYSTEM=$(detect_system "$ACTIVE_USER" "$PASSWORD" || true)
    fi
else
    if [ "$SYSTEM" = "retropie" ]; then
        ACTIVE_USER="pi"
        PASSWORD="$RETROPIE_PASS"
        if ! ssh_run "$ACTIVE_USER" "$PASSWORD" "true" 2>/dev/null; then
            ACTIVE_USER="root"
            PASSWORD="root"
        fi
    elif [ "$SYSTEM" = "batocera" ]; then
        ACTIVE_USER="root"
        PASSWORD="$BATOCERA_PASS"
    elif [ "$SYSTEM" = "recalbox" ]; then
        ACTIVE_USER="root"
        PASSWORD="$RECALBOX_PASS"
    else
        # Auto detection loop
        echo "Trying Recalbox defaults..."
        if ssh_run "root" "$RECALBOX_PASS" "test -d /recalbox/share" 2>/dev/null; then
            SYSTEM="recalbox"
            ACTIVE_USER="root"
            PASSWORD="$RECALBOX_PASS"
        elif ssh_run "root" "$BATOCERA_PASS" "test -d /userdata/system" 2>/dev/null; then
            SYSTEM="batocera"
            ACTIVE_USER="root"
            PASSWORD="$BATOCERA_PASS"
        elif ssh_run "pi" "$RETROPIE_PASS" "test -d /opt/retropie" 2>/dev/null; then
            SYSTEM="retropie"
            ACTIVE_USER="pi"
            PASSWORD="$RETROPIE_PASS"
        fi
    fi
fi

if [ -z "$SYSTEM" ] || [ "$SYSTEM" = "unknown" ]; then
    echo
    echo "ERROR: Could not detect or authenticate to $TARGET_IP." >&2
    echo "Please verify credentials and target OS selection." >&2
    exit 1
fi

echo "Console identified as: $SYSTEM (User: $ACTIVE_USER)"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

if [ "$ACTION" = "2" ]; then
    if [ "$SYSTEM" = "recalbox" ]; then
        LOG_PATH="/recalbox/share/userscripts/daemon.log"
    elif [ "$SYSTEM" = "batocera" ]; then
        LOG_PATH="/userdata/system/scripts/daemon.log"
    else
        LOG_PATH="/opt/retropie/configs/all/daemon.log"
    fi
    echo "=============================================="
    echo " Fetching logs from $LOG_PATH..."
    echo "=============================================="
    ssh_run "$ACTIVE_USER" "$PASSWORD" "tail -n 100 $LOG_PATH 2>/dev/null || echo 'Log file not found or empty'"
    exit 0
fi

TOPIC="system/playing/$SYSTEM"
sed -e "s/{{BROKER}}/$BROKER_IP/g" -e "s|{{TOPIC}}|$TOPIC|g" "$SCRIPT_DIR/arcadematrix_daemon.py" > "$TMP_DIR/arcadematrix_daemon.py"

if [ "$SYSTEM" = "recalbox" ]; then
    TARGET_DIR="/recalbox/share/userscripts"
    echo "Cleaning up any previous install..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" "pkill -f arcadematrix_daemon.py || true; pkill -f arcadematrix_mqtt.sh || true; rm -f $TARGET_DIR/arcadematrix_mqtt.sh || true" || true
    ssh_run "$ACTIVE_USER" "$PASSWORD" "mkdir -p $TARGET_DIR" || true

    echo "Uploading daemon (topic: $TOPIC)..."
    scp_run "$ACTIVE_USER" "$PASSWORD" "$TMP_DIR/arcadematrix_daemon.py" "/recalbox/share/arcadematrix_daemon.py" || { echo "SCP failed!"; exit 1; }
    scp_run "$ACTIVE_USER" "$PASSWORD" "$SCRIPT_DIR/arcadematrix_launcher(permanent).sh" "'$TARGET_DIR/arcadematrix_launcher(permanent).sh'" || { echo "SCP failed!"; exit 1; }
    ssh_run "$ACTIVE_USER" "$PASSWORD" "chmod +x '$TARGET_DIR/arcadematrix_launcher(permanent).sh'" || true

    echo "Rebooting $TARGET_IP to apply changes..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" "sleep 1 && reboot" || true

elif [ "$SYSTEM" = "batocera" ]; then
    TARGET_DIR="/userdata/system/scripts"
    echo "Cleaning up legacy daemons..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" "pkill -f arcadematrix_daemon.py 2>/dev/null; pkill -f arcadematrix_mqtt.sh 2>/dev/null; true" || true
    echo "Removing legacy script files..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" "rm -f /userdata/system/arcadematrix_daemon.py $TARGET_DIR/arcadematrix_hook.sh $TARGET_DIR/arcadematrix_mqtt.sh" || true
    echo "Removing legacy ES event directories..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" "rm -rf $TARGET_DIR/game-selected $TARGET_DIR/game-start $TARGET_DIR/game-end $TARGET_DIR/system-selected" || true
    ssh_run "$ACTIVE_USER" "$PASSWORD" "rm -rf /userdata/system/configs/emulationstation/scripts/game-selected /userdata/system/configs/emulationstation/scripts/game-start /userdata/system/configs/emulationstation/scripts/game-end /userdata/system/configs/emulationstation/scripts/system-selected" || true
    ssh_run "$ACTIVE_USER" "$PASSWORD" "if [ -f /userdata/system/custom.sh ]; then sed -i '/arcadematrix_daemon.py/d' /userdata/system/custom.sh; fi" || true
    ssh_run "$ACTIVE_USER" "$PASSWORD" "mkdir -p $TARGET_DIR" || true

    echo "Preparing Batocera event hook..."
    sed -e "s/{{BROKER}}/$BROKER_IP/g" "$SCRIPT_DIR/arcadematrix_mqtt_batocera.sh" > "$TMP_DIR/arcadematrix_mqtt.sh"

    echo "Uploading hook to $TARGET_DIR/arcadematrix_mqtt.sh..."
    scp_run "$ACTIVE_USER" "$PASSWORD" "$TMP_DIR/arcadematrix_mqtt.sh" "$TARGET_DIR/arcadematrix_mqtt.sh" || { echo "Upload failed!"; exit 1; }
    ssh_run "$ACTIVE_USER" "$PASSWORD" "chmod 755 $TARGET_DIR/arcadematrix_mqtt.sh" || true

    echo "Verifying hook deployment..."
    VERIFY=$(ssh_run "$ACTIVE_USER" "$PASSWORD" "test -f $TARGET_DIR/arcadematrix_mqtt.sh && wc -c < $TARGET_DIR/arcadematrix_mqtt.sh && head -1 $TARGET_DIR/arcadematrix_mqtt.sh" 2>/dev/null || true)
    if [ -z "$VERIFY" ]; then
        echo "ERROR: Hook script was NOT written to the Batocera filesystem!" >&2
        exit 1
    fi
    echo "  Hook verified: $VERIFY"

    echo "Configuring EmulationStation UI hooks (game-selected, system-selected)..."
    for evt in game-selected system-selected game-start game-end; do
        ssh_run "$ACTIVE_USER" "$PASSWORD" "
            mkdir -p /userdata/system/configs/emulationstation/scripts/$evt && \
            printf '#!/bin/sh\n/userdata/system/scripts/arcadematrix_mqtt.sh $evt \"\$@\"\n' \
            > /userdata/system/configs/emulationstation/scripts/$evt/arcadematrix_mqtt.sh && \
            chmod 755 /userdata/system/configs/emulationstation/scripts/$evt/arcadematrix_mqtt.sh
        " || true
    done
    ssh_run "$ACTIVE_USER" "$PASSWORD" "chmod -R 755 /userdata/system/configs/emulationstation/scripts" || true

    echo "Syncing filesystem..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" "sync" || true

    echo "Batocera one-shot event hooks successfully installed!"

    echo "Rebooting $TARGET_IP to apply changes..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" "sync && sleep 1 && reboot" || true

elif [ "$SYSTEM" = "retropie" ]; then
    echo "Installing for RetroPie..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" "pkill -f arcadematrix_daemon.py || true; mkdir -p /opt/retropie/configs/all" || true

    echo "Uploading daemon (topic: $TOPIC)..."
    scp_run "$ACTIVE_USER" "$PASSWORD" "$TMP_DIR/arcadematrix_daemon.py" "/opt/retropie/configs/all/arcadematrix_daemon.py" || { echo "SCP failed!"; exit 1; }
    ssh_run "$ACTIVE_USER" "$PASSWORD" "chmod +x /opt/retropie/configs/all/arcadematrix_daemon.py" || true

    echo "Configuring runcommand hooks..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" "
    touch /opt/retropie/configs/all/runcommand-onstart.sh
    if ! grep -q 'ArcadeMatrix Runcommand Hook' /opt/retropie/configs/all/runcommand-onstart.sh; then
        cat >> /opt/retropie/configs/all/runcommand-onstart.sh << 'EOF'
# ArcadeMatrix Runcommand Hook
cat > /tmp/es_state.inf << STATEEOF
SystemId=\$1
GamePath=\$3
State=playing
STATEEOF
EOF
    fi
    chmod +x /opt/retropie/configs/all/runcommand-onstart.sh

    touch /opt/retropie/configs/all/runcommand-onend.sh
    if ! grep -q 'ArcadeMatrix Runcommand Hook' /opt/retropie/configs/all/runcommand-onend.sh; then
        cat >> /opt/retropie/configs/all/runcommand-onend.sh << 'EOF'
# ArcadeMatrix Runcommand Hook
cat > /tmp/es_state.inf << STATEEOF
SystemId=
GamePath=
State=stopped
STATEEOF
EOF
    fi
    chmod +x /opt/retropie/configs/all/runcommand-onend.sh

    if [ -f /opt/retropie/configs/all/autostart.sh ] && ! grep -q 'arcadematrix_daemon.py' /opt/retropie/configs/all/autostart.sh; then
        sed -i '/emulationstation/i python3 /opt/retropie/configs/all/arcadematrix_daemon.py > /opt/retropie/configs/all/daemon.log 2>&1 &' /opt/retropie/configs/all/autostart.sh
    fi
    " || true

    echo "Starting RetroPie daemon..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" "nohup python3 /opt/retropie/configs/all/arcadematrix_daemon.py > /opt/retropie/configs/all/daemon.log 2>&1 &" || true
fi

echo
echo "Installation complete on $SYSTEM!"

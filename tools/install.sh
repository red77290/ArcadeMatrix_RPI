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
SSH_MUX_SOCK="/tmp/am_ssh_$$"
SSH_OPTS=(-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP_DIR="$(mktemp -d)"
cleanup_installer() {
    if [ -S "$SSH_MUX_SOCK" ] && [ -n "${ACTIVE_USER:-}" ] && [ -n "${TARGET_IP:-}" ]; then
        ssh -S "$SSH_MUX_SOCK" -O exit "${ACTIVE_USER}@${TARGET_IP}" >/dev/null 2>&1 || true
    fi
    rm -f "$SSH_MUX_SOCK" 2>/dev/null || true
    rm -rf "$TMP_DIR"
}
trap cleanup_installer EXIT

echo "=============================================="
echo " ArcadeMatrix Gaming OS Daemon Installer"
echo " (Recalbox / Batocera / RetroPie)"
echo "=============================================="
echo

read -rp "IP address of your Console (Recalbox/Batocera/RetroPie): " TARGET_IP

read -rp "Action (1: Install Daemon, 2: Check Logs) [1]: " ACTION
ACTION=${ACTION:-1}

if [ "$ACTION" = "1" ]; then
    read -rp "IP address of your ArcadeMatrix DMD (ESP32 or Raspberry Pi): " DMD_IP
    if [ -z "$DMD_IP" ]; then
        echo "ArcadeMatrix DMD IP is required for installation. Aborting." >&2
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
    SSH_OPTS+=(-o ControlMaster=auto -o ControlPath="$SSH_MUX_SOCK" -o ControlPersist=60)
    echo
    echo "NOTE: 'sshpass' is not installed - SSH multiplexing enabled (you will only enter the password once)."
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
    local user="$1" password="$2" src="$3" dst="${4#\'}"
    dst="${dst%\'}"
    if [ "$have_sshpass" = "1" ]; then
        tr -d '\r' < "$src" | sshpass -p "$password" ssh "${SSH_OPTS[@]}" "${user}@${TARGET_IP}" "cat > '${dst}'"
    else
        tr -d '\r' < "$src" | ssh "${SSH_OPTS[@]}" "${user}@${TARGET_IP}" "cat > '${dst}'"
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
sed -e "s/{{DMD_HOST}}/$DMD_IP/g" -e "s/{{BROKER}}/$DMD_IP/g" -e "s|{{TOPIC}}|$TOPIC|g" "$SCRIPT_DIR/arcadematrix_daemon.py" > "$TMP_DIR/arcadematrix_daemon.py"
sed -e "s/{{DMD_HOST}}/$DMD_IP/g" -e "s/{{BROKER}}/$DMD_IP/g" -e "s|{{TOPIC}}|$TOPIC|g" "$SCRIPT_DIR/arcadematrix_companion_sync.py" > "$TMP_DIR/arcadematrix_companion_sync.py"

if [ "$SYSTEM" = "recalbox" ]; then
    TARGET_DIR="/recalbox/share/userscripts"
    WAKEUP_HOOK="arcadematrix_wakeup[start,systembrowsing,rungame,endgame,rundemo,enddemo,wakeup].sh"
    echo "Cleaning up any previous install..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" "
        touch /tmp/arcadematrix_daemon.stop
        if [ -f /tmp/arcadematrix_supervisor.pid ]; then kill -9 \$(cat /tmp/arcadematrix_supervisor.pid 2>/dev/null) 2>/dev/null || true; fi
        if [ -f /tmp/arcadematrix_daemon.pid ]; then kill \$(cat /tmp/arcadematrix_daemon.pid 2>/dev/null) 2>/dev/null || true; fi
        pkill -f '[a]rcadematrix_daemon.py' 2>/dev/null || true
        sleep 0.2
        pkill -9 -f '[a]rcadematrix_daemon.py' 2>/dev/null || true
        pkill -f '[a]rcadematrix_companion_sync.py' 2>/dev/null || true
        pkill -f '[a]rcadematrix_mqtt.sh' 2>/dev/null || true
        rm -f /tmp/arcadematrix_daemon.pid /tmp/arcadematrix_supervisor.pid '$TARGET_DIR/arcadematrix_mqtt.sh' || true
        mkdir -p '$TARGET_DIR' /recalbox/share/system
    " || true

    echo "Uploading daemon, watchdog & ES self-healer (topic: $TOPIC)..."
    scp_run "$ACTIVE_USER" "$PASSWORD" "$TMP_DIR/arcadematrix_daemon.py" "/recalbox/share/arcadematrix_daemon.py" || { echo "SCP failed!"; exit 1; }
    scp_run "$ACTIVE_USER" "$PASSWORD" "$TMP_DIR/arcadematrix_companion_sync.py" "/recalbox/share/arcadematrix_companion_sync.py" || { echo "SCP failed!"; exit 1; }
    scp_run "$ACTIVE_USER" "$PASSWORD" "$SCRIPT_DIR/arcadematrix_launcher(permanent).sh" "$TARGET_DIR/arcadematrix_launcher(permanent).sh" || { echo "SCP failed!"; exit 1; }
    scp_run "$ACTIVE_USER" "$PASSWORD" "$SCRIPT_DIR/$WAKEUP_HOOK" "$TARGET_DIR/$WAKEUP_HOOK" || { echo "SCP failed!"; exit 1; }

    echo "Configuring Recalbox boot persistence & starting daemon live (no reboot needed)..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" "
        chmod +x /recalbox/share/arcadematrix_companion_sync.py '$TARGET_DIR/arcadematrix_launcher(permanent).sh' '$TARGET_DIR/$WAKEUP_HOOK' 2>/dev/null || true
        if [ ! -f /recalbox/share/system/custom.sh ]; then
            printf '#!/bin/sh\ncase \"\$1\" in\n  \"\"|start) sh \"/recalbox/share/userscripts/arcadematrix_launcher(permanent).sh\" -action start >/dev/null 2>&1 & ;;\nesac\n' > /recalbox/share/system/custom.sh
        elif ! grep -q 'arcadematrix_launcher' /recalbox/share/system/custom.sh; then
            printf '\n# ArcadeMatrix Daemon\ncase \"\$1\" in \"\"|start) sh \"/recalbox/share/userscripts/arcadematrix_launcher(permanent).sh\" -action start >/dev/null 2>&1 & ;; esac\n' >> /recalbox/share/system/custom.sh
        fi
        chmod +x /recalbox/share/system/custom.sh 2>/dev/null || true
        rm -f /tmp/arcadematrix_daemon.stop
        : > '$TARGET_DIR/daemon.log'
        sh '$TARGET_DIR/arcadematrix_launcher(permanent).sh' -action start
        sync
        sleep 0.6
        echo 'Active Recalbox processes:'
        (ps w 2>/dev/null || ps) | grep -E '[a]rcadematrix_daemon.py|[a]rcadematrix_supervisor' || true
        cat '$TARGET_DIR/daemon.log' 2>/dev/null || true
    " || true

elif [ "$SYSTEM" = "batocera" ]; then
    TARGET_DIR="/userdata/system/scripts"
    echo "Cleaning up legacy daemons..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" "pkill -f '[a]rcadematrix_daemon.py' 2>/dev/null || true; pkill -f '[a]rcadematrix_companion_sync.py' 2>/dev/null || true; pkill -f '[a]rcadematrix_mqtt.sh' 2>/dev/null || true" || true
    echo "Removing legacy script files..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" "rm -f /userdata/system/arcadematrix_daemon.py $TARGET_DIR/arcadematrix_hook.sh $TARGET_DIR/arcadematrix_mqtt.sh $TARGET_DIR/arcadematrix_companion_sync.py" || true
    echo "Removing legacy ES event directories..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" "rm -rf $TARGET_DIR/game-selected $TARGET_DIR/game-start $TARGET_DIR/game-end $TARGET_DIR/system-selected" || true
    ssh_run "$ACTIVE_USER" "$PASSWORD" "rm -rf /userdata/system/configs/emulationstation/scripts/game-selected /userdata/system/configs/emulationstation/scripts/game-start /userdata/system/configs/emulationstation/scripts/game-end /userdata/system/configs/emulationstation/scripts/system-selected" || true
    ssh_run "$ACTIVE_USER" "$PASSWORD" "if [ -f /userdata/system/custom.sh ]; then sed -i '/arcadematrix_daemon.py/d' /userdata/system/custom.sh; fi" || true
    ssh_run "$ACTIVE_USER" "$PASSWORD" "mkdir -p $TARGET_DIR" || true

    echo "Preparing Batocera event hook (dynamic MQTT broker discovery via DMD $DMD_IP)..."
    sed -e "s/{{DMD_HOST}}/$DMD_IP/g" -e "s/{{BROKER}}/$DMD_IP/g" "$SCRIPT_DIR/arcadematrix_mqtt_batocera.sh" > "$TMP_DIR/arcadematrix_mqtt.sh"

    echo "Uploading hook & companion sync to $TARGET_DIR..."
    scp_run "$ACTIVE_USER" "$PASSWORD" "$TMP_DIR/arcadematrix_mqtt.sh" "$TARGET_DIR/arcadematrix_mqtt.sh" || { echo "Upload failed!"; exit 1; }
    scp_run "$ACTIVE_USER" "$PASSWORD" "$TMP_DIR/arcadematrix_companion_sync.py" "$TARGET_DIR/arcadematrix_companion_sync.py" || { echo "Upload failed!"; exit 1; }
    ssh_run "$ACTIVE_USER" "$PASSWORD" "chmod 755 $TARGET_DIR/arcadematrix_mqtt.sh $TARGET_DIR/arcadematrix_companion_sync.py" || true

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

elif [ "$SYSTEM" = "retropie" ]; then
    echo "Installing for RetroPie..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" "touch /tmp/arcadematrix_daemon.stop; pkill -f '[a]rcadematrix_daemon.py' 2>/dev/null || true; sleep 0.2; pkill -9 -f '[a]rcadematrix_daemon.py' 2>/dev/null || true; pkill -f '[a]rcadematrix_companion_sync.py' 2>/dev/null || true; rm -f /tmp/arcadematrix_daemon.stop /tmp/arcadematrix_daemon.pid; mkdir -p /opt/retropie/configs/all" || true

    echo "Uploading daemon & companion sync (topic: $TOPIC)..."
    scp_run "$ACTIVE_USER" "$PASSWORD" "$TMP_DIR/arcadematrix_daemon.py" "/opt/retropie/configs/all/arcadematrix_daemon.py" || { echo "SCP failed!"; exit 1; }
    scp_run "$ACTIVE_USER" "$PASSWORD" "$TMP_DIR/arcadematrix_companion_sync.py" "/opt/retropie/configs/all/arcadematrix_companion_sync.py" || { echo "SCP failed!"; exit 1; }
    ssh_run "$ACTIVE_USER" "$PASSWORD" "chmod +x /opt/retropie/configs/all/arcadematrix_daemon.py /opt/retropie/configs/all/arcadematrix_companion_sync.py" || true

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
        sed -i '/emulationstation/i python3 /opt/retropie/configs/all/arcadematrix_daemon.py --daemon --log-file /opt/retropie/configs/all/daemon.log' /opt/retropie/configs/all/autostart.sh
    fi
    " || true

    echo "Starting RetroPie daemon..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" ": > /opt/retropie/configs/all/daemon.log; python3 /opt/retropie/configs/all/arcadematrix_daemon.py --daemon --log-file /opt/retropie/configs/all/daemon.log; sleep 0.5" || true
fi

echo
echo "Installation complete on $SYSTEM (active immediately, no reboot required)!"

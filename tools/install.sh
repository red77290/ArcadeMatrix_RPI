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
        sshpass -p "$password" scp "${SSH_OPTS[@]}" "$src" "${user}@${TARGET_IP}:${dst}"
    else
        scp "${SSH_OPTS[@]}" "$src" "${user}@${TARGET_IP}:${dst}"
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
    echo "Cleaning up any previous install..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" "pkill -f arcadematrix_daemon.py || true; pkill -f arcadematrix_mqtt.sh || true; rm -f $TARGET_DIR/arcadematrix_mqtt.sh || true" || true
    ssh_run "$ACTIVE_USER" "$PASSWORD" "mkdir -p $TARGET_DIR /userdata/system/configs/emulationstation/scripts" || true

    echo "Uploading daemon (topic: $TOPIC)..."
    scp_run "$ACTIVE_USER" "$PASSWORD" "$TMP_DIR/arcadematrix_daemon.py" "/userdata/system/arcadematrix_daemon.py" || { echo "SCP failed!"; exit 1; }

    echo "Installing Batocera event hooks..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" "
    cat > /userdata/system/scripts/arcadematrix_hook.sh << 'EOF'
#!/bin/sh
EVENT=\"\$1\"
[ -z \"\$EVENT\" ] && EVENT=\"\$(basename \"\$0\")\"
SYSTEM=\"\$2\"
ROMPATH=\"\$3\"

case \"\$(basename \"\$0\")\" in
    game-start|game_start|gameStart) EVENT=\"game-start\"; SYSTEM=\"\$1\"; ROMPATH=\"\$2\" ;;
    game-end|game_end|gameStop)     EVENT=\"game-end\"; SYSTEM=\"\$1\"; ROMPATH=\"\$2\" ;;
    game-selected)                   EVENT=\"game-selected\"; SYSTEM=\"\$1\"; ROMPATH=\"\$2\" ;;
    system-selected)                 EVENT=\"system-selected\"; SYSTEM=\"\$1\" ;;
esac

case \"\$EVENT\" in
    game-selected) STATE=\"browsing\" ;;
    game-start)    STATE=\"playing\" ;;
    game-end)      STATE=\"stopped\" ;;
    system-selected) STATE=\"browsing\"; ROMPATH=\"\" ;;
    *)             STATE=\"browsing\" ;;
esac

cat > /tmp/es_state.inf << STATEEOF
SystemId=\$SYSTEM
GamePath=\$ROMPATH
State=\$STATE
STATEEOF
EOF
    chmod +x /userdata/system/scripts/arcadematrix_hook.sh
    for evt in game-selected game-start game-end system-selected; do
        ln -sf /userdata/system/scripts/arcadematrix_hook.sh /userdata/system/scripts/\$evt
        ln -sf /userdata/system/scripts/arcadematrix_hook.sh /userdata/system/configs/emulationstation/scripts/\$evt
    done
    " || true

    echo "Configuring custom.sh for startup..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" "
    if [ ! -f /userdata/system/custom.sh ]; then
        echo '#!/bin/sh' > /userdata/system/custom.sh
        echo '[ \"\$1\" = \"start\" ] && python3 /userdata/system/arcadematrix_daemon.py > /userdata/system/scripts/daemon.log 2>&1 &' >> /userdata/system/custom.sh
        chmod +x /userdata/system/custom.sh
    else
        if ! grep -q 'arcadematrix_daemon.py' /userdata/system/custom.sh; then
            echo '[ \"\$1\" = \"start\" ] && python3 /userdata/system/arcadematrix_daemon.py > /userdata/system/scripts/daemon.log 2>&1 &' >> /userdata/system/custom.sh
        fi
    fi
    " || true

    echo "Rebooting $TARGET_IP to apply changes..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" "sleep 1 && reboot" || true

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

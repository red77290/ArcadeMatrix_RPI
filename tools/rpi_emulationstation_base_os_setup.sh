#!/bin/bash
# ==============================================================================
# ArcadeMatrix - Gaming Console MQTT Setup Script
# (A EXÉCUTER DIRECTEMENT SUR LA CONSOLE : Recalbox, Batocera ou RetroPie)
# ==============================================================================
set -euo pipefail

echo "=============================================================================="
echo " ArcadeMatrix - Configuration MQTT pour Console de Jeu"
echo " (Recalbox / Batocera / RetroPie)"
echo "=============================================================================="
echo

read -rp "Adresse IP de ton ArcadeMatrix (ESP32 ou Raspberry Pi) : " MQTT_BROKER
if [ -z "$MQTT_BROKER" ]; then
    echo "Erreur : L'adresse IP du broker est obligatoire." >&2
    exit 1
fi

echo
echo "Choisis ton système de jeu :"
echo "  1) Détection automatique"
echo "  2) Recalbox"
echo "  3) Batocera"
echo "  4) RetroPie"
read -rp "Choix [1] : " OS_CHOICE
OS_CHOICE=${OS_CHOICE:-1}

SYSTEM=""
if [ "$OS_CHOICE" = "2" ]; then
    SYSTEM="recalbox"
elif [ "$OS_CHOICE" = "3" ]; then
    SYSTEM="batocera"
elif [ "$OS_CHOICE" = "4" ]; then
    SYSTEM="retropie"
else
    if [ -d "/opt/retropie" ]; then
        SYSTEM="retropie"
    elif [ -d "/userdata/system" ]; then
        SYSTEM="batocera"
    elif [ -d "/recalbox/share" ]; then
        SYSTEM="recalbox"
    else
        echo "Impossible de détecter l'OS automatiquement. Choisis manuellement."
        exit 1
    fi
fi

echo "Système sélectionné : $SYSTEM"
TOPIC="system/playing/$SYSTEM"

# Nettoyage des anciens démons
pkill -f arcadematrix_daemon.py || true
pkill -f arcadematrix_mqtt.sh || true

if [ "$SYSTEM" = "recalbox" ]; then
    TARGET_DIR="/recalbox/share/userscripts"
    DAEMON_FILE="/recalbox/share/arcadematrix_daemon.py"
    LAUNCHER_FILE="$TARGET_DIR/arcadematrix_launcher(permanent).sh"

    mkdir -p "$TARGET_DIR"
    rm -f "$TARGET_DIR/arcadematrix_mqtt.sh"

    cat << 'PYEOF' > "$DAEMON_FILE"
import subprocess
import time
import os
import json

BROKER = "MQTT_BROKER_IP_PLACEHOLDER"
TOPIC = "MQTT_TOPIC_PLACEHOLDER"

def parse_statefile():
    game, system, state = None, None, "browsing"
    try:
        with open("/tmp/es_state.inf", "r") as f:
            for line in f:
                if line.startswith("GamePath="):
                    game = line.split("=", 1)[1].strip()
                elif line.startswith("SystemId="):
                    system = line.split("=", 1)[1].strip()
                elif line.startswith("State="):
                    state = line.split("=", 1)[1].strip()
    except Exception:
        pass
    return game, system, state

def clean_system_name(s):
    if not s:
        return ""
    s_clean = str(s).strip()
    s_lower = s_clean.lower()
    prefixes = [
        "arcade manufacturer ",
        "arcade system ",
        "arcade genre ",
        "arcade collection ",
        "manufacturer ",
        "system ",
        "genre ",
        "collection ",
    ]
    for p in prefixes:
        if s_lower.startswith(p):
            return s_clean[len(p):].strip()
    return s_clean

def main():
    import socket
    import sys
    lock_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        lock_socket.bind(("127.0.0.1", 49132))
    except socket.error:
        print("Daemon is already running, exiting...")
        sys.exit(1)
        
    time.sleep(3)
    last_state_key = None
    last_sent_key = None
    pending_since = 0

    while True:
        try:
            rom_path, system, state = parse_statefile()
            if not system and not rom_path:
                time.sleep(0.1)
                continue

            system = clean_system_name(system)

            if state == "stopped":
                current_key = (None, None, "stopped")
            else:
                is_system = True
                if rom_path and not os.path.isdir(rom_path):
                    is_system = False
                
                if is_system:
                    current_key = (None, system, "browsing")
                else:
                    current_key = (rom_path, system, state)

            if current_key != last_state_key:
                last_state_key = current_key
                pending_since = time.time()

            elapsed = time.time() - pending_since
            if elapsed >= 0.15 and current_key != last_sent_key:
                last_sent_key = current_key

                if current_key[2] == "stopped":
                    msg = '{"status": "stopped"}'
                elif current_key[0] is None:
                    msg = json.dumps({"status": "browsing", "system": str(current_key[1]), "type": "system"})
                else:
                    gbase = os.path.splitext(os.path.basename(current_key[0]))[0]
                    gbase = clean_system_name(gbase)
                    msg = json.dumps({"status": current_key[2], "game": gbase, "system": str(current_key[1])})

                try:
                    subprocess.run(["mosquitto_pub", "-h", BROKER, "-t", TOPIC, "-m", msg], timeout=2, check=False)
                except Exception:
                    pass
        except Exception as e:
            print("Error: " + str(e), flush=True)

        time.sleep(0.1)

if __name__ == "__main__":
    main()
PYEOF

    sed -i "s/MQTT_BROKER_IP_PLACEHOLDER/$MQTT_BROKER/g" "$DAEMON_FILE"
    sed -i "s|MQTT_TOPIC_PLACEHOLDER|$TOPIC|g" "$DAEMON_FILE"

    cat << 'SHEOF' > "$LAUNCHER_FILE"
#!/bin/sh
if [ -z "$1" ] || [ "$1" = "-action" -a "$2" = "start" ]; then
    pkill -f arcadematrix_daemon.py || true
    python3 /recalbox/share/arcadematrix_daemon.py > /recalbox/share/userscripts/daemon.log 2>&1 &
fi
SHEOF
    chmod +x "$LAUNCHER_FILE"

elif [ "$SYSTEM" = "batocera" ]; then
    TARGET_DIR="/userdata/system/scripts"
    HOOK_FILE="$TARGET_DIR/arcadematrix_mqtt.sh"

    # Clean up any legacy daemons or broken shims
    pkill -f arcadematrix_daemon.py || true
    pkill -f arcadematrix_mqtt.sh || true
    rm -f /userdata/system/arcadematrix_daemon.py "$TARGET_DIR/arcadematrix_hook.sh"
    rm -f "$TARGET_DIR/game-selected" "$TARGET_DIR/game-start" "$TARGET_DIR/game-end" "$TARGET_DIR/system-selected"
    rm -f /userdata/system/configs/emulationstation/scripts/game-selected /userdata/system/configs/emulationstation/scripts/game-start /userdata/system/configs/emulationstation/scripts/game-end /userdata/system/configs/emulationstation/scripts/system-selected
    if [ -f /userdata/system/custom.sh ]; then
        sed -i '/arcadematrix_daemon.py/d' /userdata/system/custom.sh
    fi

    mkdir -p "$TARGET_DIR"

    cat << 'SHEOF' > "$HOOK_FILE"
#!/bin/sh
BROKER="MQTT_BROKER_IP_PLACEHOLDER"
TOPIC="system/playing/batocera"
LOG_FILE="/userdata/system/scripts/daemon.log"

clean_name() {
    echo "$1" | sed -E \
        -e 's/^[Aa]rcade [Mm]anufacturer //' \
        -e 's/^[Aa]rcade [Ss]ystem //' \
        -e 's/^[Aa]rcade [Gg]enre //' \
        -e 's/^[Aa]rcade [Cc]ollection //' \
        -e 's/^[Mm]anufacturer //' \
        -e 's/^[Ss]ystem //' \
        -e 's/^[Gg]enre //' \
        -e 's/^[Cc]ollection //' | sed -E 's/^[_-]//' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//'
}

EVENT="$1"
shift

if [ -z "$EVENT" ] || [ ! -z "${EVENT##game*}" -a ! -z "${EVENT##system*}" ]; then
    PARENT_DIR="$(basename "$(dirname "$0")")"
    case "$PARENT_DIR" in
        game-selected|system-selected|game-start|game-end)
            exec /userdata/system/scripts/arcadematrix_mqtt.sh "$PARENT_DIR" "$EVENT" "$@"
            ;;
    esac
fi

if ! command -v mosquitto_pub >/dev/null 2>&1; then
    echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] ERROR: mosquitto_pub not found in PATH" >> "$LOG_FILE"
    exit 1
fi

case "$EVENT" in
    gameStart)
        SYS_NAME="$1"
        if [ -n "$4" ]; then
            ROM_PATH="$4"
        elif [ -n "$2" ] && echo "$2" | grep -qE '/|\.'; then
            ROM_PATH="$2"
        elif [ -n "$1" ] && echo "$1" | grep -qE '/|\.'; then
            ROM_PATH="$1"
            SYS_NAME="$2"
        else
            ROM_PATH="$1"
        fi
        GAME_BASENAME=$(basename "$ROM_PATH" | sed 's/\.[^.]*$//')
        GAME_CLEAN=$(clean_name "$GAME_BASENAME")
        SYS_CLEAN=$(clean_name "$SYS_NAME")
        PAYLOAD="{\"status\": \"playing\", \"game\": \"$GAME_CLEAN\", \"system\": \"$SYS_CLEAN\"}"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: gameStart | Rom: $ROM_PATH | Sys: $SYS_NAME | Sent: $PAYLOAD" >> "$LOG_FILE"
        mosquitto_pub -h "$BROKER" -t "$TOPIC" -m "$PAYLOAD" >> "$LOG_FILE" 2>&1 &
        ;;

    gameStop)
        PAYLOAD="{\"status\": \"stopped\"}"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: gameStop | Sent: $PAYLOAD" >> "$LOG_FILE"
        mosquitto_pub -h "$BROKER" -t "$TOPIC" -m "$PAYLOAD" >> "$LOG_FILE" 2>&1 &
        ;;

    game-selected|gameSelected)
        SYS_NAME="$1"
        ROM_PATH="$2"
        TITLE="$3"
        if echo "$1" | grep -qE '/|\.'; then
            ROM_PATH="$1"
            SYS_NAME="$2"
            TITLE="$3"
        fi
        if [ -n "$TITLE" ]; then
            GAME_CLEAN=$(clean_name "$TITLE")
        else
            GAME_BASENAME=$(basename "$ROM_PATH" | sed 's/\.[^.]*$//')
            GAME_CLEAN=$(clean_name "$GAME_BASENAME")
        fi
        SYS_CLEAN=$(clean_name "$SYS_NAME")
        PAYLOAD="{\"status\": \"browsing\", \"game\": \"$GAME_CLEAN\", \"system\": \"$SYS_CLEAN\"}"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: game-selected | Rom: $ROM_PATH | Sys: $SYS_NAME | Title: $TITLE | Sent: $PAYLOAD" >> "$LOG_FILE"
        mosquitto_pub -h "$BROKER" -t "$TOPIC" -m "$PAYLOAD" >> "$LOG_FILE" 2>&1 &
        ;;

    system-selected|systemSelected)
        SYS_CLEAN=$(clean_name "$1")
        PAYLOAD="{\"status\": \"browsing\", \"system\": \"$SYS_CLEAN\", \"type\": \"system\"}"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: system-selected | Sys: $1 | Sent: $PAYLOAD" >> "$LOG_FILE"
        mosquitto_pub -h "$BROKER" -t "$TOPIC" -m "$PAYLOAD" >> "$LOG_FILE" 2>&1 &
        ;;

    game-start)
        SYS_NAME="$1"
        ROM_PATH="$2"
        TITLE="$3"
        if echo "$1" | grep -qE '/|\.'; then
            ROM_PATH="$1"
            SYS_NAME="$2"
            TITLE="$3"
        fi
        if [ -n "$TITLE" ]; then
            GAME_CLEAN=$(clean_name "$TITLE")
        else
            GAME_BASENAME=$(basename "$ROM_PATH" | sed 's/\.[^.]*$//')
            GAME_CLEAN=$(clean_name "$GAME_BASENAME")
        fi
        SYS_CLEAN=$(clean_name "$SYS_NAME")
        PAYLOAD="{\"status\": \"playing\", \"game\": \"$GAME_CLEAN\", \"system\": \"$SYS_CLEAN\"}"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: game-start | Rom: $ROM_PATH | Sys: $SYS_NAME | Title: $TITLE | Sent: $PAYLOAD" >> "$LOG_FILE"
        mosquitto_pub -h "$BROKER" -t "$TOPIC" -m "$PAYLOAD" >> "$LOG_FILE" 2>&1 &
        ;;

    game-end)
        PAYLOAD="{\"status\": \"stopped\"}"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: game-end | Sent: $PAYLOAD" >> "$LOG_FILE"
        mosquitto_pub -h "$BROKER" -t "$TOPIC" -m "$PAYLOAD" >> "$LOG_FILE" 2>&1 &
        ;;

    *)
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Unhandled Event: $EVENT | Args: $*" >> "$LOG_FILE"
        ;;
esac
SHEOF
    sed -i "s/MQTT_BROKER_IP_PLACEHOLDER/$MQTT_BROKER/g" "$HOOK_FILE"
    chmod +x "$HOOK_FILE"

    for evt in game-selected system-selected game-start game-end; do
        dir="/userdata/system/configs/emulationstation/scripts/$evt"
        mkdir -p "$dir"
        cat << EOFEVT > "$dir/arcadematrix_mqtt.sh"
#!/bin/sh
/userdata/system/scripts/arcadematrix_mqtt.sh "$evt" "\$@"
EOFEVT
        chmod +x "$dir/arcadematrix_mqtt.sh"
    done
    echo "Batocera one-shot event hooks successfully installed into $HOOK_FILE and ES scripts"

elif [ "$SYSTEM" = "retropie" ]; then
    DAEMON_FILE="/opt/retropie/configs/all/arcadematrix_daemon.py"
    mkdir -p /opt/retropie/configs/all

    if ! command -v mosquitto_pub >/dev/null 2>&1; then
        echo "Installation de mosquitto-clients pour RetroPie..."
        sudo apt-get update && sudo apt-get install -y mosquitto-clients || true
    fi

    cat << 'PYEOF' > "$DAEMON_FILE"
import subprocess
import time
import os
import json

BROKER = "MQTT_BROKER_IP_PLACEHOLDER"
TOPIC = "MQTT_TOPIC_PLACEHOLDER"

def parse_statefile():
    game, system, state = None, None, "browsing"
    try:
        with open("/tmp/es_state.inf", "r") as f:
            for line in f:
                if line.startswith("GamePath="):
                    game = line.split("=", 1)[1].strip()
                elif line.startswith("SystemId="):
                    system = line.split("=", 1)[1].strip()
                elif line.startswith("State="):
                    state = line.split("=", 1)[1].strip()
    except Exception:
        pass
    return game, system, state

def clean_system_name(s):
    if not s:
        return ""
    s_clean = str(s).strip()
    s_lower = s_clean.lower()
    prefixes = [
        "arcade manufacturer ",
        "arcade system ",
        "arcade genre ",
        "arcade collection ",
        "manufacturer ",
        "system ",
        "genre ",
        "collection ",
    ]
    for p in prefixes:
        if s_lower.startswith(p):
            return s_clean[len(p):].strip()
    return s_clean

def main():
    import socket
    import sys
    lock_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        lock_socket.bind(("127.0.0.1", 49132))
    except socket.error:
        print("Daemon is already running, exiting...")
        sys.exit(1)
        
    time.sleep(3)
    last_state_key = None
    last_sent_key = None
    pending_since = 0

    while True:
        try:
            rom_path, system, state = parse_statefile()
            if not system and not rom_path:
                time.sleep(0.1)
                continue

            system = clean_system_name(system)

            if state == "stopped":
                current_key = (None, None, "stopped")
            else:
                is_system = True
                if rom_path and not os.path.isdir(rom_path):
                    is_system = False
                
                if is_system:
                    current_key = (None, system, "browsing")
                else:
                    current_key = (rom_path, system, state)

            if current_key != last_state_key:
                last_state_key = current_key
                pending_since = time.time()

            elapsed = time.time() - pending_since
            if elapsed >= 0.15 and current_key != last_sent_key:
                last_sent_key = current_key

                if current_key[2] == "stopped":
                    msg = '{"status": "stopped"}'
                elif current_key[0] is None:
                    msg = json.dumps({"status": "browsing", "system": str(current_key[1]), "type": "system"})
                else:
                    gbase = os.path.splitext(os.path.basename(current_key[0]))[0]
                    gbase = clean_system_name(gbase)
                    msg = json.dumps({"status": current_key[2], "game": gbase, "system": str(current_key[1])})

                try:
                    subprocess.run(["mosquitto_pub", "-h", BROKER, "-t", TOPIC, "-m", msg], timeout=2, check=False)
                except Exception:
                    pass
        except Exception as e:
            print("Error: " + str(e), flush=True)

        time.sleep(0.1)

if __name__ == "__main__":
    main()
PYEOF

    sed -i "s/MQTT_BROKER_IP_PLACEHOLDER/$MQTT_BROKER/g" "$DAEMON_FILE"
    sed -i "s|MQTT_TOPIC_PLACEHOLDER|$TOPIC|g" "$DAEMON_FILE"
    chmod +x "$DAEMON_FILE"

    touch /opt/retropie/configs/all/runcommand-onstart.sh
    if ! grep -q 'ArcadeMatrix Runcommand Hook' /opt/retropie/configs/all/runcommand-onstart.sh; then
        cat >> /opt/retropie/configs/all/runcommand-onstart.sh << 'EOF'
# ArcadeMatrix Runcommand Hook
cat > /tmp/es_state.inf << STATEEOF
SystemId=$1
GamePath=$3
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

    nohup python3 /opt/retropie/configs/all/arcadematrix_daemon.py > /opt/retropie/configs/all/daemon.log 2>&1 &
fi

echo
echo "=============================================================================="
echo "SUCCÈS ! Le daemon ArcadeMatrix a été installé pour $SYSTEM sur le topic $TOPIC."
if [ "$SYSTEM" != "retropie" ]; then
    echo "Un redémarrage est nécessaire. Tape 'reboot' pour l'appliquer."
else
    echo "Le daemon tourne déjà en arrière-plan !"
fi
echo "=============================================================================="

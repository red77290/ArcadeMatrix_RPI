#!/bin/bash
# ==============================================================================
# ArcadeMatrix - Gaming Console MQTT Setup Script
# (RUN DIRECTLY ON THE CONSOLE: Recalbox, Batocera, or RetroPie)
# ==============================================================================
set -euo pipefail

echo "=============================================================================="
echo " ArcadeMatrix - Gaming Console MQTT Setup Script"
echo " (Recalbox / Batocera / RetroPie)"
echo "=============================================================================="
echo

read -rp "ArcadeMatrix DMD IP address (ESP32 or Raspberry Pi): " DMD_HOST
if [ -z "$DMD_HOST" ]; then
    echo "Error: ArcadeMatrix DMD IP address is required." >&2
    exit 1
fi

echo
echo "Select your gaming operating system:"
echo "  1) Auto-detect"
echo "  2) Recalbox"
echo "  3) Batocera"
echo "  4) RetroPie"
read -rp "Choice [1]: " OS_CHOICE
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
        echo "Unable to auto-detect gaming OS. Please select manually."
        exit 1
    fi
fi

echo "Selected system: $SYSTEM"
TOPIC="system/playing/$SYSTEM"

# Cleanup legacy daemons
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
import socket
import urllib.request

DMD_HOST = "DMD_HOST_IP_PLACEHOLDER"
BROKER = "MQTT_BROKER_IP_PLACEHOLDER"
TOPIC = "MQTT_TOPIC_PLACEHOLDER"

_cached_broker_host = None
_cached_broker_port = 1883
_cached_broker_ts = 0.0
BROKER_CACHE_TTL_SEC = 15.0

def get_default_host():
    if DMD_HOST and "PLACEHOLDER" not in DMD_HOST:
        return DMD_HOST
    return BROKER

def resolve_broker():
    global _cached_broker_host, _cached_broker_port, _cached_broker_ts
    now = time.time()
    if _cached_broker_host and (now - _cached_broker_ts) < BROKER_CACHE_TTL_SEC:
        return _cached_broker_host, _cached_broker_port

    dmd = get_default_host()
    host = dmd
    port = 1883
    for http_port in (80, 8080):
        url = "http://{}:{}/api/mqtt/broker-target".format(dmd, http_port)
        try:
            req = urllib.request.Request(url, headers={"Accept": "application/json"})
            with urllib.request.urlopen(req, timeout=0.8) as resp:
                if resp.status == 200:
                    data = json.loads(resp.read().decode("utf-8", errors="ignore"))
                    cand_host = str(data.get("host") or "").strip()
                    cand_port = int(data.get("port") or 1883)
                    if cand_host and cand_host not in ("127.0.0.1", "localhost", "0.0.0.0"):
                        host = cand_host
                    elif cand_host in ("127.0.0.1", "localhost"):
                        host = dmd
                    if 1 <= cand_port <= 65535:
                        port = cand_port
                    break
        except Exception:
            continue

    _cached_broker_host = host
    _cached_broker_port = port
    _cached_broker_ts = now
    return host, port

def _enc_mqtt_len(length):
    out = bytearray()
    while True:
        b = length % 128
        length //= 128
        if length > 0:
            b |= 0x80
        out.append(b)
        if length == 0:
            break
    return bytes(out)

def publish_mqtt(topic, payload):
    global _cached_broker_ts
    host, port = resolve_broker()
    try:
        res = subprocess.run(
            ["mosquitto_pub", "-h", host, "-p", str(port), "-t", topic, "-m", payload],
            timeout=2,
            check=False,
        )
        if res.returncode == 0:
            return
    except FileNotFoundError:
        pass
    except Exception:
        _cached_broker_ts = 0.0
        return

    try:
        cid = "recalbox_es_{}".format(os.getpid()).encode("utf-8")
        c_body = b"\x00\x04MQTT\x04\x02\x00\x3c" + len(cid).to_bytes(2, "big") + cid
        c_pkt = b"\x10" + _enc_mqtt_len(len(c_body)) + c_body
        t_b = topic.encode("utf-8")
        m_b = payload.encode("utf-8")
        p_body = len(t_b).to_bytes(2, "big") + t_b + m_b
        p_pkt = b"\x30" + _enc_mqtt_len(len(p_body)) + p_body
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(2.0)
        s.connect((host, int(port)))
        s.sendall(c_pkt)
        s.recv(4)
        s.sendall(p_pkt)
        s.sendall(b"\xe0\x00")
        s.close()
    except Exception:
        _cached_broker_ts = 0.0

def parse_statefile():
    game, title, system, state = None, None, None, "browsing"
    try:
        with open("/tmp/es_state.inf", "r") as f:
            for line in f:
                if line.startswith("GamePath="):
                    game = line.split("=", 1)[1].strip()
                elif line.startswith("Game="):
                    title = line.split("=", 1)[1].strip()
                elif line.startswith("SystemId="):
                    system = line.split("=", 1)[1].strip()
                elif line.startswith("State="):
                    state = line.split("=", 1)[1].strip()
    except Exception:
        pass
    return game, title, system, state

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
            rom_path, game_title, system, state = parse_statefile()
            if state != "stopped" and not system and not rom_path:
                time.sleep(0.1)
                continue

            system = clean_system_name(system)

            if state == "stopped":
                current_key = (None, None, None, "stopped")
            else:
                is_system = True
                if rom_path and not os.path.isdir(rom_path):
                    is_system = False
                
                if is_system:
                    current_key = (None, None, system, "browsing")
                else:
                    current_key = (rom_path, game_title, system, state)

            if current_key != last_state_key:
                last_state_key = current_key
                pending_since = time.time()

            elapsed = time.time() - pending_since
            if elapsed >= 0.15 and current_key != last_sent_key:
                last_sent_key = current_key

                if current_key[3] == "stopped":
                    msg = '{"status": "stopped"}'
                elif current_key[0] is None:
                    msg = json.dumps({"status": "browsing", "system": str(current_key[2]), "type": "system"})
                else:
                    gbase = os.path.splitext(os.path.basename(current_key[0]))[0]
                    gbase = clean_system_name(gbase)
                    payload_obj = {"status": current_key[3], "game": gbase, "system": str(current_key[2])}
                    if current_key[1]:
                        tclean = clean_system_name(current_key[1])
                        if tclean and tclean.lower() != gbase.lower():
                            payload_obj["title"] = tclean
                    msg = json.dumps(payload_obj)

                publish_mqtt(TOPIC, msg)
        except Exception as e:
            print("Error: " + str(e), flush=True)

        time.sleep(0.1)

if __name__ == "__main__":
    main()
PYEOF

    sed -i "s/DMD_HOST_IP_PLACEHOLDER/$DMD_HOST/g" "$DAEMON_FILE"
    sed -i "s/MQTT_BROKER_IP_PLACEHOLDER/$DMD_HOST/g" "$DAEMON_FILE"
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
    pkill -f arcadematrix_daemon.py 2>/dev/null || true
    pkill -f arcadematrix_mqtt.sh 2>/dev/null || true
    rm -f /userdata/system/arcadematrix_daemon.py "$TARGET_DIR/arcadematrix_hook.sh" "$TARGET_DIR/arcadematrix_mqtt.sh"
    rm -rf "$TARGET_DIR/game-selected" "$TARGET_DIR/game-start" "$TARGET_DIR/game-end" "$TARGET_DIR/system-selected"
    rm -rf /userdata/system/configs/emulationstation/scripts/game-selected /userdata/system/configs/emulationstation/scripts/game-start /userdata/system/configs/emulationstation/scripts/game-end /userdata/system/configs/emulationstation/scripts/system-selected
    if [ -f /userdata/system/custom.sh ]; then
        sed -i '/arcadematrix_daemon.py/d' /userdata/system/custom.sh
    fi

    mkdir -p "$TARGET_DIR"

    cat << 'SHEOF' > "$HOOK_FILE"
#!/bin/sh
DMD_HOST="DMD_HOST_IP_PLACEHOLDER"
BROKER="MQTT_BROKER_IP_PLACEHOLDER"
TOPIC="system/playing/batocera"
LOG_FILE="/userdata/system/scripts/daemon.log"
DEBOUNCE_TOKEN_FILE="/tmp/arcadematrix_debounce_token"
MQTT_PUB_PID_FILE="/tmp/arcadematrix_mqtt_pub.pid"
BROKER_CACHE_FILE="/tmp/arcadematrix_broker_target.cache"

resolve_broker_target() {
    _DMD="$DMD_HOST"
    if [ -z "$_DMD" ] || echo "$_DMD" | grep -q 'PLACEHOLDER'; then
        _DMD="$BROKER"
    fi
    BROKER_HOST="$_DMD"
    BROKER_PORT="1883"

    NOW=$(date +%s 2>/dev/null || echo 0)
    if [ -f "$BROKER_CACHE_FILE" ]; then
        C_TS=$(awk 'NR==1{print $1}' "$BROKER_CACHE_FILE" 2>/dev/null)
        C_HOST=$(awk 'NR==1{print $2}' "$BROKER_CACHE_FILE" 2>/dev/null)
        C_PORT=$(awk 'NR==1{print $3}' "$BROKER_CACHE_FILE" 2>/dev/null)
        if [ -n "$C_TS" ] && [ -n "$C_HOST" ] && [ $((NOW - C_TS)) -lt 15 ] 2>/dev/null; then
            BROKER_HOST="$C_HOST"
            BROKER_PORT="${C_PORT:-1883}"
            return 0
        fi
    fi

    RESP=""
    for HPORT in 80 8080; do
        if command -v curl >/dev/null 2>&1; then
            RESP=$(curl -s -m 1 "http://${_DMD}:${HPORT}/api/mqtt/broker-target" 2>/dev/null)
        elif command -v wget >/dev/null 2>&1; then
            RESP=$(wget -q -T 1 -O - "http://${_DMD}:${HPORT}/api/mqtt/broker-target" 2>/dev/null)
        fi
        if echo "$RESP" | grep -q '"host"'; then
            break
        fi
        RESP=""
    done

    if [ -n "$RESP" ]; then
        P_HOST=$(echo "$RESP" | sed -n 's/.*"host"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
        P_PORT=$(echo "$RESP" | sed -n 's/.*"port"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/p')
        if [ -n "$P_HOST" ] && [ "$P_HOST" != "127.0.0.1" ] && [ "$P_HOST" != "localhost" ] && [ "$P_HOST" != "0.0.0.0" ]; then
            BROKER_HOST="$P_HOST"
        fi
        if [ -n "$P_PORT" ]; then
            BROKER_PORT="$P_PORT"
        fi
    fi
    echo "$NOW $BROKER_HOST $BROKER_PORT" > "$BROKER_CACHE_FILE" 2>/dev/null || true
}

send_mqtt() {
    _PAYLOAD="$1"
    resolve_broker_target

    # Terminate any previously running publication to avoid concurrent socket accumulation
    if [ -f "$MQTT_PUB_PID_FILE" ]; then
        OLD_PID=$(cat "$MQTT_PUB_PID_FILE" 2>/dev/null)
        if [ -n "$OLD_PID" ]; then
            kill "$OLD_PID" 2>/dev/null || true
        fi
        rm -f "$MQTT_PUB_PID_FILE"
    fi

    if command -v mosquitto_pub >/dev/null 2>&1; then
        mosquitto_pub -h "$BROKER_HOST" -p "$BROKER_PORT" -t "$TOPIC" -i "batocera_es_$$" -m "$_PAYLOAD" >> "$LOG_FILE" 2>&1 &
        echo $! > "$MQTT_PUB_PID_FILE"
        return 0
    fi
    for p in /usr/bin/mosquitto_pub /usr/local/bin/mosquitto_pub /bin/mosquitto_pub; do
        if [ -x "$p" ]; then
            "$p" -h "$BROKER_HOST" -p "$BROKER_PORT" -t "$TOPIC" -i "batocera_es_$$" -m "$_PAYLOAD" >> "$LOG_FILE" 2>&1 &
            echo $! > "$MQTT_PUB_PID_FILE"
            return 0
        fi
    done
    PYTHON_BIN=""
    if command -v python3 >/dev/null 2>&1; then
        PYTHON_BIN="python3"
    elif command -v python >/dev/null 2>&1; then
        PYTHON_BIN="python"
    elif [ -x /usr/bin/python3 ]; then
        PYTHON_BIN="/usr/bin/python3"
    fi
    if [ -n "$PYTHON_BIN" ]; then
        "$PYTHON_BIN" -c "
import sys, socket, os, time
broker, port, topic, payload = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]
def enc(l):
    r = bytearray()
    while True:
        b = l % 128
        l //= 128
        if l > 0: b |= 0x80
        r.append(b)
        if l == 0: break
    return bytes(r)
try:
    cid = f'batocera_es_{os.getpid()}'.encode('utf-8')
    c_body = b'\x00\x04MQTT\x04\x02\x00\x3c' + len(cid).to_bytes(2, 'big') + cid
    c_pkt = b'\x10' + enc(len(c_body)) + c_body
    t_b = topic.encode('utf-8')
    m_b = payload.encode('utf-8')
    p_body = len(t_b).to_bytes(2, 'big') + t_b + m_b
    p_pkt = b'\x30' + enc(len(p_body)) + p_body
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(2.0)
    s.connect((broker, port))
    s.sendall(c_pkt)
    s.recv(4)
    s.sendall(p_pkt)
    s.sendall(b'\xe0\x00')
    s.close()
except Exception as e:
    print(f'MQTT Error: {e}', file=sys.stderr)
" "$BROKER_HOST" "$BROKER_PORT" "$TOPIC" "$_PAYLOAD" >> "$LOG_FILE" 2>&1 &
        echo $! > "$MQTT_PUB_PID_FILE"
        return 0
    fi
    echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] ERROR: Neither mosquitto_pub nor python3 found" >> "$LOG_FILE"
    return 1
}

send_mqtt_debounced() {
    _PAYLOAD="$1"
    TOKEN="$(date +%s%N 2>/dev/null || date +%s)_$$"
    echo "$TOKEN" > "$DEBOUNCE_TOKEN_FILE"
    (
        sleep 0.15
        CURRENT_TOKEN="$(cat "$DEBOUNCE_TOKEN_FILE" 2>/dev/null)"
        if [ "$CURRENT_TOKEN" = "$TOKEN" ]; then
            rm -f "$DEBOUNCE_TOKEN_FILE"
            send_mqtt "$_PAYLOAD"
        fi
    ) &
}

clean_name() {
    echo "$1" | sed -E \
        -e 's/^[Aa]rcade [Mm]anufacturer //' \
        -e 's/^[Aa]rcade [Ss]ystem //' \
        -e 's/^[Aa]rcade [Gg]enre //' \
        -e 's/^[Aa]rcade [Cc]ollection //' \
        -e 's/^[Mm]anufacturer //' \
        -e 's/^[Ss]ystem //' \
        -e 's/^[Gg]enre //' \
        -e 's/^[Cc]ollection //' \
        -e 's/"//g' | sed -E 's/^[_-]//' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//'
}

extract_system() {
    _sys="$1"
    _rom="$2"
    _base="$3"
    if [ -z "$_sys" ] || [ "$_sys" = "$_base" ] || [ "$_sys" = "$_rom" ] || echo "$_sys" | grep -qE '/|\.'; then
        if echo "$_rom" | grep -q '/roms/'; then
            echo "$_rom" | sed -E 's|.*/roms/([^/]+)/.*|\1|'
        elif [ -n "$_rom" ]; then
            basename "$(dirname "$_rom")"
        else
            echo "$_sys"
        fi
    else
        echo "$_sys"
    fi
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

case "$EVENT" in
    gameStart)
        rm -f "$DEBOUNCE_TOKEN_FILE"
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
        SYS_NAME=$(extract_system "$SYS_NAME" "$ROM_PATH" "$GAME_BASENAME")
        GAME_CLEAN=$(clean_name "$GAME_BASENAME")
        SYS_CLEAN=$(clean_name "$SYS_NAME")
        TITLE_CLEAN=$(clean_name "$5")
        if [ -n "$TITLE_CLEAN" ] && [ "$TITLE_CLEAN" != "$GAME_CLEAN" ]; then
            PAYLOAD="{\"status\": \"playing\", \"game\": \"$GAME_CLEAN\", \"title\": \"$TITLE_CLEAN\", \"system\": \"$SYS_CLEAN\"}"
        else
            PAYLOAD="{\"status\": \"playing\", \"game\": \"$GAME_CLEAN\", \"system\": \"$SYS_CLEAN\"}"
        fi
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: gameStart | Rom: $ROM_PATH | Sys: $SYS_NAME | Sent: $PAYLOAD" >> "$LOG_FILE"
        send_mqtt "$PAYLOAD"
        ;;

    gameStop)
        rm -f "$DEBOUNCE_TOKEN_FILE"
        PAYLOAD="{\"status\": \"stopped\"}"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: gameStop | Sent: $PAYLOAD" >> "$LOG_FILE"
        send_mqtt "$PAYLOAD"
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
        if [ -n "$ROM_PATH" ]; then
            GAME_BASENAME=$(basename "$ROM_PATH" | sed 's/\.[^.]*$//')
            GAME_CLEAN=$(clean_name "$GAME_BASENAME")
        elif [ -n "$TITLE" ]; then
            GAME_BASENAME=""
            GAME_CLEAN=$(clean_name "$TITLE")
        else
            GAME_BASENAME=""
            GAME_CLEAN=""
        fi
        SYS_NAME=$(extract_system "$SYS_NAME" "$ROM_PATH" "$GAME_BASENAME")
        SYS_CLEAN=$(clean_name "$SYS_NAME")
        TITLE_CLEAN=$(clean_name "$TITLE")
        if [ -n "$TITLE_CLEAN" ] && [ "$TITLE_CLEAN" != "$GAME_CLEAN" ]; then
            PAYLOAD="{\"status\": \"browsing\", \"game\": \"$GAME_CLEAN\", \"title\": \"$TITLE_CLEAN\", \"system\": \"$SYS_CLEAN\"}"
        else
            PAYLOAD="{\"status\": \"browsing\", \"game\": \"$GAME_CLEAN\", \"system\": \"$SYS_CLEAN\"}"
        fi
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: game-selected | Rom: $ROM_PATH | Sys: $SYS_NAME | Title: $TITLE | Sent: $PAYLOAD" >> "$LOG_FILE"
        send_mqtt_debounced "$PAYLOAD"
        ;;

    system-selected|systemSelected)
        SYS_CLEAN=$(clean_name "$1")
        PAYLOAD="{\"status\": \"browsing\", \"system\": \"$SYS_CLEAN\", \"type\": \"system\"}"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: system-selected | Sys: $1 | Sent: $PAYLOAD" >> "$LOG_FILE"
        send_mqtt_debounced "$PAYLOAD"
        ;;

    game-start)
        rm -f "$DEBOUNCE_TOKEN_FILE"
        SYS_NAME="$1"
        ROM_PATH="$2"
        TITLE="$3"
        if echo "$1" | grep -qE '/|\.'; then
            ROM_PATH="$1"
            SYS_NAME="$2"
            TITLE="$3"
        fi
        if [ -n "$ROM_PATH" ]; then
            GAME_BASENAME=$(basename "$ROM_PATH" | sed 's/\.[^.]*$//')
            GAME_CLEAN=$(clean_name "$GAME_BASENAME")
        elif [ -n "$TITLE" ]; then
            GAME_BASENAME=""
            GAME_CLEAN=$(clean_name "$TITLE")
        else
            GAME_BASENAME=""
            GAME_CLEAN=""
        fi
        SYS_NAME=$(extract_system "$SYS_NAME" "$ROM_PATH" "$GAME_BASENAME")
        SYS_CLEAN=$(clean_name "$SYS_NAME")
        TITLE_CLEAN=$(clean_name "$TITLE")
        if [ -n "$TITLE_CLEAN" ] && [ "$TITLE_CLEAN" != "$GAME_CLEAN" ]; then
            PAYLOAD="{\"status\": \"playing\", \"game\": \"$GAME_CLEAN\", \"title\": \"$TITLE_CLEAN\", \"system\": \"$SYS_CLEAN\"}"
        else
            PAYLOAD="{\"status\": \"playing\", \"game\": \"$GAME_CLEAN\", \"system\": \"$SYS_CLEAN\"}"
        fi
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: game-start | Rom: $ROM_PATH | Sys: $SYS_NAME | Title: $TITLE | Sent: $PAYLOAD" >> "$LOG_FILE"
        send_mqtt "$PAYLOAD"
        ;;

    game-end)
        rm -f "$DEBOUNCE_TOKEN_FILE"
        PAYLOAD="{\"status\": \"stopped\"}"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: game-end | Sent: $PAYLOAD" >> "$LOG_FILE"
        send_mqtt "$PAYLOAD"
        ;;

    *)
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Unhandled Event: $EVENT | Args: $*" >> "$LOG_FILE"
        ;;
esac
SHEOF
    sed -i "s/DMD_HOST_IP_PLACEHOLDER/$DMD_HOST/g" "$HOOK_FILE"
    sed -i "s/MQTT_BROKER_IP_PLACEHOLDER/$DMD_HOST/g" "$HOOK_FILE"
    chmod 755 "$HOOK_FILE"

    for evt in game-selected system-selected game-start game-end; do
        dir="/userdata/system/configs/emulationstation/scripts/$evt"
        mkdir -p "$dir"
        cat << EOFEVT > "$dir/arcadematrix_mqtt.sh"
#!/bin/sh
/userdata/system/scripts/arcadematrix_mqtt.sh "$evt" "\$@"
EOFEVT
        chmod 755 "$dir/arcadematrix_mqtt.sh"
    done
    chmod -R 755 /userdata/system/configs/emulationstation/scripts
    echo "Batocera one-shot event hooks successfully installed into $HOOK_FILE and ES scripts"

elif [ "$SYSTEM" = "retropie" ]; then
    DAEMON_FILE="/opt/retropie/configs/all/arcadematrix_daemon.py"
    mkdir -p /opt/retropie/configs/all

    if ! command -v mosquitto_pub >/dev/null 2>&1; then
        echo "Installing mosquitto-clients for RetroPie..."
        sudo apt-get update && sudo apt-get install -y mosquitto-clients || true
    fi

    cat << 'PYEOF' > "$DAEMON_FILE"
import subprocess
import time
import os
import json
import socket
import urllib.request

DMD_HOST = "DMD_HOST_IP_PLACEHOLDER"
BROKER = "MQTT_BROKER_IP_PLACEHOLDER"
TOPIC = "MQTT_TOPIC_PLACEHOLDER"

_cached_broker_host = None
_cached_broker_port = 1883
_cached_broker_ts = 0.0
BROKER_CACHE_TTL_SEC = 15.0

def get_default_host():
    if DMD_HOST and "PLACEHOLDER" not in DMD_HOST:
        return DMD_HOST
    return BROKER

def resolve_broker():
    global _cached_broker_host, _cached_broker_port, _cached_broker_ts
    now = time.time()
    if _cached_broker_host and (now - _cached_broker_ts) < BROKER_CACHE_TTL_SEC:
        return _cached_broker_host, _cached_broker_port

    dmd = get_default_host()
    host = dmd
    port = 1883
    for http_port in (80, 8080):
        url = "http://{}:{}/api/mqtt/broker-target".format(dmd, http_port)
        try:
            req = urllib.request.Request(url, headers={"Accept": "application/json"})
            with urllib.request.urlopen(req, timeout=0.8) as resp:
                if resp.status == 200:
                    data = json.loads(resp.read().decode("utf-8", errors="ignore"))
                    cand_host = str(data.get("host") or "").strip()
                    cand_port = int(data.get("port") or 1883)
                    if cand_host and cand_host not in ("127.0.0.1", "localhost", "0.0.0.0"):
                        host = cand_host
                    elif cand_host in ("127.0.0.1", "localhost"):
                        host = dmd
                    if 1 <= cand_port <= 65535:
                        port = cand_port
                    break
        except Exception:
            continue

    _cached_broker_host = host
    _cached_broker_port = port
    _cached_broker_ts = now
    return host, port

def _enc_mqtt_len(length):
    out = bytearray()
    while True:
        b = length % 128
        length //= 128
        if length > 0:
            b |= 0x80
        out.append(b)
        if length == 0:
            break
    return bytes(out)

def publish_mqtt(topic, payload):
    global _cached_broker_ts
    host, port = resolve_broker()
    try:
        res = subprocess.run(
            ["mosquitto_pub", "-h", host, "-p", str(port), "-t", topic, "-m", payload],
            timeout=2,
            check=False,
        )
        if res.returncode == 0:
            return
    except FileNotFoundError:
        pass
    except Exception:
        _cached_broker_ts = 0.0
        return

    try:
        cid = "recalbox_es_{}".format(os.getpid()).encode("utf-8")
        c_body = b"\x00\x04MQTT\x04\x02\x00\x3c" + len(cid).to_bytes(2, "big") + cid
        c_pkt = b"\x10" + _enc_mqtt_len(len(c_body)) + c_body
        t_b = topic.encode("utf-8")
        m_b = payload.encode("utf-8")
        p_body = len(t_b).to_bytes(2, "big") + t_b + m_b
        p_pkt = b"\x30" + _enc_mqtt_len(len(p_body)) + p_body
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(2.0)
        s.connect((host, int(port)))
        s.sendall(c_pkt)
        s.recv(4)
        s.sendall(p_pkt)
        s.sendall(b"\xe0\x00")
        s.close()
    except Exception:
        _cached_broker_ts = 0.0

def parse_statefile():
    game, title, system, state = None, None, None, "browsing"
    try:
        with open("/tmp/es_state.inf", "r") as f:
            for line in f:
                if line.startswith("GamePath="):
                    game = line.split("=", 1)[1].strip()
                elif line.startswith("Game="):
                    title = line.split("=", 1)[1].strip()
                elif line.startswith("SystemId="):
                    system = line.split("=", 1)[1].strip()
                elif line.startswith("State="):
                    state = line.split("=", 1)[1].strip()
    except Exception:
        pass
    return game, title, system, state

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
            rom_path, game_title, system, state = parse_statefile()
            if state != "stopped" and not system and not rom_path:
                time.sleep(0.1)
                continue

            system = clean_system_name(system)

            if state == "stopped":
                current_key = (None, None, None, "stopped")
            else:
                is_system = True
                if rom_path and not os.path.isdir(rom_path):
                    is_system = False
                
                if is_system:
                    current_key = (None, None, system, "browsing")
                else:
                    current_key = (rom_path, game_title, system, state)

            if current_key != last_state_key:
                last_state_key = current_key
                pending_since = time.time()

            elapsed = time.time() - pending_since
            if elapsed >= 0.15 and current_key != last_sent_key:
                last_sent_key = current_key

                if current_key[3] == "stopped":
                    msg = '{"status": "stopped"}'
                elif current_key[0] is None:
                    msg = json.dumps({"status": "browsing", "system": str(current_key[2]), "type": "system"})
                else:
                    gbase = os.path.splitext(os.path.basename(current_key[0]))[0]
                    gbase = clean_system_name(gbase)
                    payload_obj = {"status": current_key[3], "game": gbase, "system": str(current_key[2])}
                    if current_key[1]:
                        tclean = clean_system_name(current_key[1])
                        if tclean and tclean.lower() != gbase.lower():
                            payload_obj["title"] = tclean
                    msg = json.dumps(payload_obj)

                publish_mqtt(TOPIC, msg)
        except Exception as e:
            print("Error: " + str(e), flush=True)

        time.sleep(0.1)

if __name__ == "__main__":
    main()
PYEOF

    sed -i "s/DMD_HOST_IP_PLACEHOLDER/$DMD_HOST/g" "$DAEMON_FILE"
    sed -i "s/MQTT_BROKER_IP_PLACEHOLDER/$DMD_HOST/g" "$DAEMON_FILE"
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
echo "SUCCESS! ArcadeMatrix daemon installed for $SYSTEM on topic $TOPIC."
if [ "$SYSTEM" != "retropie" ]; then
    echo "A system reboot is required. Run 'reboot' to apply."
else
    echo "The daemon is now running in the background!"
fi
echo "=============================================================================="

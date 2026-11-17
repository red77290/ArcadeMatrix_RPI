#!/bin/sh
# ArcadeMatrix Batocera MQTT event hook script
# Handles Batocera system runner events (gameStart, gameStop) in /userdata/system/scripts/
# and EmulationStation UI events (game-selected, system-selected, game-start, game-end)
# in /userdata/system/configs/emulationstation/scripts/
#
# {{DMD_HOST}} is substituted by the installer with the ArcadeMatrix DMD IP address.
# The active MQTT broker IP/port is resolved dynamically from http://<DMD_HOST>/api/mqtt/broker-target
# (cached for 15s in /tmp, force-refreshed on gameStart/game-start) with automatic fallback to <DMD_HOST>:1883.

DMD_HOST="{{DMD_HOST}}"
TOPIC="system/playing/batocera"
LOG_FILE="/userdata/system/scripts/daemon.log"
DEBOUNCE_TOKEN_FILE="/tmp/arcadematrix_debounce_token"
MQTT_PUB_PID_FILE="/tmp/arcadematrix_mqtt_pub.pid"
BROKER_CACHE_FILE="/tmp/arcadematrix_broker_target.cache"
LAST_LIFECYCLE_FILE="/tmp/arcadematrix_last_lifecycle"
ACTIVE_GAME_FILE="/tmp/arcadematrix_active_game"
COMPANION_SCRIPT="/userdata/system/scripts/arcadematrix_companion_sync.py"

spawn_companion_sync() {
    _ACTION="$1"
    _SYS="$2"
    _GAME="$3"
    _ROM="$4"
    _TITLE="$5"
    if [ ! -f "$COMPANION_SCRIPT" ]; then
        return 0
    fi
    _PY=""
    if command -v python3 >/dev/null 2>&1; then
        _PY="python3"
    elif [ -x /usr/bin/python3 ]; then
        _PY="/usr/bin/python3"
    elif command -v python >/dev/null 2>&1; then
        _PY="python"
    fi
    if [ -n "$_PY" ]; then
        "$_PY" "$COMPANION_SCRIPT" "$_ACTION" \
            --dmd-host "$DMD_HOST" \
            --topic "$TOPIC" \
            --system "$_SYS" \
            --game "$_GAME" \
            --rom-path "$_ROM" \
            --title "$_TITLE" >> "$LOG_FILE" 2>&1 &
    fi
}

# Prevent duplicate publish + companion_sync when Batocera v33+ triggers both
# EmulationStation (game-start/game-end) and configgen (gameStart/gameStop) within ~100ms,
# while keeping game-start/game-end fully functional on Batocera <= v32 and standalone ports.
should_emit_lifecycle() {
    _KIND="$1"
    _KEY="$2"
    _NOW=$(date +%s 2>/dev/null || echo 0)
    if [ -f "$LAST_LIFECYCLE_FILE" ]; then
        _PREV=$(cat "$LAST_LIFECYCLE_FILE" 2>/dev/null)
        _P_TS=$(echo "$_PREV" | cut -d'|' -f1)
        _P_KIND=$(echo "$_PREV" | cut -d'|' -f2)
        _P_KEY=$(echo "$_PREV" | cut -d'|' -f3)
        if [ -n "$_P_TS" ] && [ "$_P_KIND" = "$_KIND" ]; then
            _DIFF=$((_NOW - _P_TS))
            if [ "$_DIFF" -ge 0 ] && [ "$_DIFF" -le 2 ]; then
                if [ "$_KIND" = "stopped" ] || [ "$_P_KEY" = "$_KEY" ]; then
                    return 1
                fi
            fi
        fi
    fi
    printf '%s|%s|%s\n' "$_NOW" "$_KIND" "$_KEY" > "$LAST_LIFECYCLE_FILE" 2>/dev/null || true
    return 0
}

resolve_broker_target() {
    _FORCE="${1:-0}"
    NOW_TS=$(date +%s 2>/dev/null || echo 0)
    if [ "$_FORCE" != "1" ] && [ -f "$BROKER_CACHE_FILE" ]; then
        CACHE_LINE=$(cat "$BROKER_CACHE_FILE" 2>/dev/null)
        CACHE_TS=$(echo "$CACHE_LINE" | cut -d'|' -f1)
        CACHE_HOST=$(echo "$CACHE_LINE" | cut -d'|' -f2)
        CACHE_PORT=$(echo "$CACHE_LINE" | cut -d'|' -f3)
        if [ -n "$CACHE_TS" ] && [ -n "$CACHE_HOST" ] && [ -n "$CACHE_PORT" ]; then
            AGE=$((NOW_TS - CACHE_TS))
            if [ "$AGE" -ge 0 ] && [ "$AGE" -lt 15 ]; then
                BROKER_HOST="$CACHE_HOST"
                BROKER_PORT="$CACHE_PORT"
                return 0
            fi
        fi
    fi

    RESP=""
    for HTTP_PORT in 80 8080; do
        URL="http://${DMD_HOST}:${HTTP_PORT}/api/mqtt/broker-target"
        if command -v curl >/dev/null 2>&1; then
            RESP=$(curl -fsS --connect-timeout 1 --max-time 1 "$URL" 2>/dev/null || true)
        elif command -v wget >/dev/null 2>&1; then
            RESP=$(wget -qO- -T 1 "$URL" 2>/dev/null || true)
        fi
        if [ -n "$RESP" ]; then
            break
        fi
    done

    PARSED_HOST=$(echo "$RESP" | sed -n 's/.*"host"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)
    PARSED_PORT=$(echo "$RESP" | sed -n 's/.*"port"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/p' | head -n 1)
    if [ -n "$PARSED_HOST" ]; then
        BROKER_HOST="$PARSED_HOST"
        BROKER_PORT="${PARSED_PORT:-1883}"
    else
        BROKER_HOST="$DMD_HOST"
        BROKER_PORT="1883"
    fi
    echo "${NOW_TS}|${BROKER_HOST}|${BROKER_PORT}" > "$BROKER_CACHE_FILE" 2>/dev/null || true
}

send_mqtt() {
    _PAYLOAD="$1"
    _FORCE_RESOLVE="${2:-0}"
    resolve_broker_target "$_FORCE_RESOLVE"

    # Terminate any previously running publication directly by PID (no wrapper subshell!)
    # to prevent concurrent TCP socket accumulation on ESP32 PicoMQTT during rapid scrolling.
    if [ -f "$MQTT_PUB_PID_FILE" ]; then
        OLD_PID=$(cat "$MQTT_PUB_PID_FILE" 2>/dev/null)
        if [ -n "$OLD_PID" ] && [ "$OLD_PID" != "$$" ]; then
            kill "$OLD_PID" 2>/dev/null || true
        fi
        rm -f "$MQTT_PUB_PID_FILE"
    fi

    _CID_SUFFIX="$$_$(cut -d' ' -f1 /proc/self/stat 2>/dev/null || echo 0)"

    # 1. Try mosquitto_pub if available in PATH (QoS 0 fast LAN fire-and-disconnect)
    if command -v mosquitto_pub >/dev/null 2>&1; then
        mosquitto_pub -h "$BROKER_HOST" -p "$BROKER_PORT" -t "$TOPIC" -i "batocera_es_${_CID_SUFFIX}" -m "$_PAYLOAD" >> "$LOG_FILE" 2>&1 &
        echo $! > "$MQTT_PUB_PID_FILE"
        return 0
    fi

    # 2. Check standard bin locations for mosquitto_pub
    for p in /usr/bin/mosquitto_pub /usr/local/bin/mosquitto_pub /bin/mosquitto_pub; do
        if [ -x "$p" ]; then
            "$p" -h "$BROKER_HOST" -p "$BROKER_PORT" -t "$TOPIC" -i "batocera_es_${_CID_SUFFIX}" -m "$_PAYLOAD" >> "$LOG_FILE" 2>&1 &
            echo $! > "$MQTT_PUB_PID_FILE"
            return 0
        fi
    done

    # 3. Fallback to native Python 3 raw socket publisher (zero external dependencies)
    PYTHON_BIN=""
    if command -v python3 >/dev/null 2>&1; then
        PYTHON_BIN="python3"
    elif command -v python >/dev/null 2>&1; then
        PYTHON_BIN="python"
    elif [ -x /usr/bin/python3 ]; then
        PYTHON_BIN="/usr/bin/python3"
    fi

    if [ -n "$PYTHON_BIN" ]; then
        "$PYTHON_BIN" -c '
import sys, socket, os
broker, port, topic, payload, cache_file = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4], sys.argv[5]
def enc(l):
    r = bytearray()
    while True:
        b = l % 128
        l //= 128
        if l > 0: b |= 0x80
        r.append(b)
        if l == 0: break
    return bytes(r)
s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.settimeout(2.0)
try:
    cid = f"batocera_es_{os.getpid()}".encode("utf-8")
    c_body = b"\x00\x04MQTT\x04\x02\x00\x3c" + len(cid).to_bytes(2, "big") + cid
    c_pkt = b"\x10" + enc(len(c_body)) + c_body
    t_b = topic.encode("utf-8")
    m_b = payload.encode("utf-8")
    p_body = len(t_b).to_bytes(2, "big") + t_b + m_b
    p_pkt = b"\x30" + enc(len(p_body)) + p_body
    s.connect((broker, port))
    s.sendall(c_pkt)
    s.recv(4)
    s.sendall(p_pkt)
    s.sendall(b"\xe0\x00")
except Exception as e:
    try:
        os.unlink(cache_file)
    except Exception:
        pass
    print(f"MQTT Error: {e}", file=sys.stderr)
finally:
    s.close()
' "$BROKER_HOST" "$BROKER_PORT" "$TOPIC" "$_PAYLOAD" "$BROKER_CACHE_FILE" >> "$LOG_FILE" 2>&1 &
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
        sleep 0.08
        CURRENT_TOKEN="$(cat "$DEBOUNCE_TOKEN_FILE" 2>/dev/null)"
        if [ "$CURRENT_TOKEN" = "$TOKEN" ]; then
            rm -f "$DEBOUNCE_TOKEN_FILE"
            send_mqtt "$_PAYLOAD" 0
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

json_escape() {
    printf '%s' "$1" | tr -d '\r\n\t' | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'
}

is_virtual_sys() {
    _low=$(echo "$1" | tr '[:upper:]' '[:lower:]' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
    case "$_low" in
        ""|favorites|favourite|favourites|fav|favs|all|allgames|all_games|lastplayed|recent|recentlyplayed|neverplayed|multiplayer|2players|3players|4players|retroachievements|lightgun|vertical|tate|shmups|shootemup|fighting|beatemup|platform|puzzle|racing|rpg|sports|auto-*|auto_*|custom-*|custom_*|collection*)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

extract_system() {
    _sys="$1"
    _rom="$2"
    _base="$3"
    if [ -z "$_sys" ] || [ "$_sys" = "$_base" ] || [ "$_sys" = "$_rom" ] || is_virtual_sys "$_sys" || echo "$_sys" | grep -qE '/|\.'; then
        if echo "$_rom" | grep -q '/roms/'; then
            _cand=$(echo "$_rom" | sed -E 's|.*/roms/([^/]+)/.*|\1|')
            if [ -n "$_cand" ] && ! is_virtual_sys "$_cand"; then
                echo "$_cand"
                return
            fi
        elif [ -n "$_rom" ] && ! is_virtual_sys "$_sys"; then
            basename "$(dirname "$_rom")"
            return
        fi
        echo "$_sys"
    else
        echo "$_sys"
    fi
}

build_game_payload() {
    _STATUS="$1"
    _GAME="$2"
    _SYS="$3"
    _TITLE="$4"
    _ROM="$5"
    _G_ESC=$(json_escape "$_GAME")
    _S_ESC=$(json_escape "$_SYS")
    _T_ESC=$(json_escape "$_TITLE")
    _R_ESC=$(json_escape "$_ROM")
    _EXTRA=""
    if [ -n "$_T_ESC" ] && [ "$_T_ESC" != "$_G_ESC" ]; then
        _EXTRA=", \"title\": \"$_T_ESC\""
    fi
    if [ -n "$_R_ESC" ]; then
        _EXTRA="${_EXTRA}, \"rom_path\": \"$_R_ESC\""
    fi
    printf '{"status": "%s", "game": "%s", "system": "%s"%s}' "$_STATUS" "$_G_ESC" "$_S_ESC" "$_EXTRA"
}

EVENT="$1"
shift

# Check for parent directory event name if invoked via direct symlink
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
        printf '%s|%s|%s\n' "$SYS_CLEAN" "$GAME_CLEAN" "$ROM_PATH" > "$ACTIVE_GAME_FILE" 2>/dev/null || true
        if should_emit_lifecycle "playing" "${SYS_CLEAN}:${GAME_CLEAN}"; then
            PAYLOAD=$(build_game_payload "playing" "$GAME_CLEAN" "$SYS_CLEAN" "$TITLE_CLEAN" "$ROM_PATH")
            echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: gameStart | Rom: $ROM_PATH | Sys: $SYS_NAME | Sent: $PAYLOAD" >> "$LOG_FILE"
            send_mqtt "$PAYLOAD" 1
            spawn_companion_sync "start" "$SYS_CLEAN" "$GAME_CLEAN" "$ROM_PATH" "$TITLE_CLEAN"
        fi
        ;;

    gameStop)
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
        GAME_BASENAME=""
        if [ -n "$ROM_PATH" ]; then
            GAME_BASENAME=$(basename "$ROM_PATH" | sed 's/\.[^.]*$//')
        fi
        SYS_NAME=$(extract_system "$SYS_NAME" "$ROM_PATH" "$GAME_BASENAME")
        GAME_CLEAN=$(clean_name "$GAME_BASENAME")
        SYS_CLEAN=$(clean_name "$SYS_NAME")
        rm -f "$ACTIVE_GAME_FILE" 2>/dev/null || true
        if should_emit_lifecycle "stopped" "${SYS_CLEAN}:${GAME_CLEAN}"; then
            G_ESC=$(json_escape "$GAME_CLEAN")
            S_ESC=$(json_escape "$SYS_CLEAN")
            PAYLOAD="{\"status\": \"stopped\", \"game\": \"$G_ESC\", \"system\": \"$S_ESC\"}"
            echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: gameStop | Rom: $ROM_PATH | Sys: $SYS_NAME | Sent: $PAYLOAD" >> "$LOG_FILE"
            send_mqtt "$PAYLOAD" 1
            spawn_companion_sync "stop" "$SYS_CLEAN" "$GAME_CLEAN" "$ROM_PATH" ""
        fi
        ;;

    game-selected|gameSelected)
        SYS_NAME="$1"
        ROM_PATH="$2"
        TITLE="$3"
        if [ "$SYS_NAME" = "NULL" ] || [ "$ROM_PATH" = "NULL" ]; then
            exit 0
        fi
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
        PAYLOAD=$(build_game_payload "browsing" "$GAME_CLEAN" "$SYS_CLEAN" "$TITLE_CLEAN" "$ROM_PATH")
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: game-selected | Rom: $ROM_PATH | Sys: $SYS_NAME | Title: $TITLE | Sent: $PAYLOAD" >> "$LOG_FILE"
        send_mqtt_debounced "$PAYLOAD"
        ;;

    system-selected|systemSelected)
        if [ -z "$1" ] || [ "$1" = "NULL" ]; then
            exit 0
        fi
        SYS_CLEAN=$(clean_name "$1")
        S_ESC=$(json_escape "$SYS_CLEAN")
        PAYLOAD="{\"status\": \"browsing\", \"system\": \"$S_ESC\", \"type\": \"system\"}"
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
        printf '%s|%s|%s\n' "$SYS_CLEAN" "$GAME_CLEAN" "$ROM_PATH" > "$ACTIVE_GAME_FILE" 2>/dev/null || true
        if should_emit_lifecycle "playing" "${SYS_CLEAN}:${GAME_CLEAN}"; then
            PAYLOAD=$(build_game_payload "playing" "$GAME_CLEAN" "$SYS_CLEAN" "$TITLE_CLEAN" "$ROM_PATH")
            echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: game-start | Rom: $ROM_PATH | Sys: $SYS_NAME | Title: $TITLE | Sent: $PAYLOAD" >> "$LOG_FILE"
            send_mqtt "$PAYLOAD" 1
            spawn_companion_sync "start" "$SYS_CLEAN" "$GAME_CLEAN" "$ROM_PATH" "$TITLE_CLEAN"
        fi
        ;;

    game-end)
        rm -f "$DEBOUNCE_TOKEN_FILE"
        SYS_NAME="$1"
        ROM_PATH="$2"
        if echo "$1" | grep -qE '/|\.'; then
            ROM_PATH="$1"
            SYS_NAME="$2"
        fi
        GAME_BASENAME=""
        if [ -n "$ROM_PATH" ]; then
            GAME_BASENAME=$(basename "$ROM_PATH" | sed 's/\.[^.]*$//')
        fi
        SYS_NAME=$(extract_system "$SYS_NAME" "$ROM_PATH" "$GAME_BASENAME")
        GAME_CLEAN=$(clean_name "$GAME_BASENAME")
        SYS_CLEAN=$(clean_name "$SYS_NAME")
        if [ -z "$GAME_CLEAN" ] && [ -f "$ACTIVE_GAME_FILE" ]; then
            _ACT=$(cat "$ACTIVE_GAME_FILE" 2>/dev/null)
            SYS_CLEAN=$(echo "$_ACT" | cut -d'|' -f1)
            GAME_CLEAN=$(echo "$_ACT" | cut -d'|' -f2)
            ROM_PATH=$(echo "$_ACT" | cut -d'|' -f3)
        fi
        rm -f "$ACTIVE_GAME_FILE" 2>/dev/null || true
        if should_emit_lifecycle "stopped" "${SYS_CLEAN}:${GAME_CLEAN}"; then
            if [ -n "$GAME_CLEAN" ] || [ -n "$SYS_CLEAN" ]; then
                G_ESC=$(json_escape "$GAME_CLEAN")
                S_ESC=$(json_escape "$SYS_CLEAN")
                PAYLOAD="{\"status\": \"stopped\", \"game\": \"$G_ESC\", \"system\": \"$S_ESC\"}"
            else
                PAYLOAD="{\"status\": \"stopped\"}"
            fi
            echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: game-end | Rom: $ROM_PATH | Sys: $SYS_CLEAN | Sent: $PAYLOAD" >> "$LOG_FILE"
            send_mqtt "$PAYLOAD" 1
            spawn_companion_sync "stop" "$SYS_CLEAN" "$GAME_CLEAN" "$ROM_PATH" ""
        fi
        ;;

    *)
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Unhandled Event: $EVENT | Args: $*" >> "$LOG_FILE"
        ;;
esac

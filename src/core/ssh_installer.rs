use ssh2::Session;
use std::io::Read;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOS {
    Recalbox,
    Batocera,
    RetroPie,
}

impl TargetOS {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "recalbox" => Some(TargetOS::Recalbox),
            "batocera" => Some(TargetOS::Batocera),
            "retropie" | "retro-pie" => Some(TargetOS::RetroPie),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            TargetOS::Recalbox => "Recalbox",
            TargetOS::Batocera => "Batocera",
            TargetOS::RetroPie => "RetroPie",
        }
    }

    pub fn topic(&self) -> &'static str {
        match self {
            TargetOS::Recalbox => "system/playing/recalbox",
            TargetOS::Batocera => "system/playing/batocera",
            TargetOS::RetroPie => "system/playing/retropie",
        }
    }

    pub fn log_path(&self) -> &'static str {
        match self {
            TargetOS::Recalbox => "/recalbox/share/userscripts/daemon.log",
            TargetOS::Batocera => "/userdata/system/scripts/daemon.log",
            TargetOS::RetroPie => "/opt/retropie/configs/all/daemon.log",
        }
    }
}

fn detect_os_remotely(sess: &Session) -> Option<TargetOS> {
    let mut channel = match sess.channel_session() {
        Ok(c) => c,
        Err(_) => return None,
    };
    let cmd = "if [ -d /opt/retropie ]; then echo RETROPIE; elif [ -d /userdata/system ]; then echo BATOCERA; elif [ -d /recalbox/share ]; then echo RECALBOX; else echo UNKNOWN; fi";
    if channel.exec(cmd).is_err() {
        return None;
    }
    let mut out = String::new();
    channel.read_to_string(&mut out).ok();
    channel.wait_close().ok();
    let trimmed = out.trim();
    if trimmed.contains("RETROPIE") {
        Some(TargetOS::RetroPie)
    } else if trimmed.contains("BATOCERA") {
        Some(TargetOS::Batocera)
    } else if trimmed.contains("RECALBOX") {
        Some(TargetOS::Recalbox)
    } else {
        None
    }
}

fn generate_daemon_code(matrix_ip: &str, topic: &str) -> String {
    format!(
        r#"import subprocess
import time
import os
import json
import urllib.request

DMD_HOST = "{}"
DMD_HTTP_PORTS = (8080, 80)
TOPIC = "{}"
BROKER_TTL_SEC = 15.0

_cached_broker = None
_cached_at = 0.0

def resolve_broker(force=False):
    global _cached_broker, _cached_at
    now = time.time()
    if not force and _cached_broker is not None and (now - _cached_at) < BROKER_TTL_SEC:
        return _cached_broker
    for http_port in DMD_HTTP_PORTS:
        url = f"http://{{DMD_HOST}}:{{http_port}}/api/mqtt/broker-target"
        try:
            req = urllib.request.Request(url, headers={{"Accept": "application/json"}})
            with urllib.request.urlopen(req, timeout=0.8) as resp:
                data = json.loads(resp.read().decode("utf-8"))
                host = str(data.get("host") or "").strip()
                port = int(data.get("port") or 1883)
                if host:
                    _cached_broker = (host, port)
                    _cached_at = now
                    return _cached_broker
        except Exception:
            continue
    _cached_broker = (DMD_HOST, 1883)
    _cached_at = now
    return _cached_broker

def publish_mqtt(msg, force_refresh=False):
    host, port = resolve_broker(force=force_refresh)
    res = subprocess.run(
        ["mosquitto_pub", "-h", host, "-p", str(port), "-t", TOPIC, "-m", msg],
        timeout=2,
        check=False,
    )
    if res.returncode != 0 and not force_refresh:
        host2, port2 = resolve_broker(force=True)
        if (host2, port2) != (host, port):
            subprocess.run(
                ["mosquitto_pub", "-h", host2, "-p", str(port2), "-t", TOPIC, "-m", msg],
                timeout=2,
                check=False,
            )

def parse_statefile():
    game = None
    system = None
    state = "browsing"
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
        print("Another daemon is already running, exiting...")
        sys.exit(1)
        
    print("Daemon started (dynamic broker discovery via DMD)!", flush=True)
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
                    msg = '{{"status": "stopped"}}'
                elif current_key[0] is None:
                    msg = json.dumps({{"status": "browsing", "system": str(current_key[1]), "type": "system"}})
                else:
                    gbase = os.path.splitext(os.path.basename(current_key[0]))[0]
                    gbase = clean_system_name(gbase)
                    msg = json.dumps({{"status": current_key[2], "game": gbase, "system": str(current_key[1])}})

                try:
                    publish_mqtt(msg, force_refresh=(current_key[2] == "playing"))
                except Exception:
                    pass
        except Exception as e:
            print("Error: " + str(e), flush=True)

        time.sleep(0.1)

if __name__ == "__main__":
    main()
"#,
        matrix_ip, topic
    )
}

fn generate_batocera_hook_code(matrix_ip: &str) -> String {
    format!(
        r#"#!/bin/sh
DMD_HOST="{dmd_host}"
BROKER_CACHE="/tmp/arcadematrix_broker.cache"
BROKER_HOST="$DMD_HOST"
BROKER_PORT="1883"
TOPIC="system/playing/batocera"
LOG_FILE="/userdata/system/scripts/daemon.log"
DEBOUNCE_TOKEN_FILE="/tmp/arcadematrix_debounce_token"
MQTT_PUB_PID_FILE="/tmp/arcadematrix_mqtt_pub.pid"

resolve_broker() {{
    _FORCE="$1"
    _NOW=$(date +%s 2>/dev/null || echo 0)
    if [ "$_FORCE" != "1" ] && [ -f "$BROKER_CACHE" ]; then
        read _TS _H _P < "$BROKER_CACHE" 2>/dev/null || true
        if [ -n "$_H" ] && [ -n "$_P" ] && [ $((_NOW - _TS)) -lt 15 ] 2>/dev/null; then
            BROKER_HOST="$_H"
            BROKER_PORT="$_P"
            return 0
        fi
    fi

    _JSON=""
    for _HTTP_PORT in 8080 80; do
        _URL="http://$DMD_HOST:$_HTTP_PORT/api/mqtt/broker-target"
        if command -v curl >/dev/null 2>&1; then
            _JSON=$(curl -fsS --connect-timeout 0.5 --max-time 0.9 "$_URL" 2>/dev/null || true)
        elif command -v wget >/dev/null 2>&1; then
            _JSON=$(wget -qO- -T 1 "$_URL" 2>/dev/null || true)
        fi
        if [ -n "$_JSON" ]; then
            _H=$(printf '%s' "$_JSON" | sed -n 's/.*"host"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
            _P=$(printf '%s' "$_JSON" | sed -n 's/.*"port"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/p')
            if [ -n "$_H" ]; then
                BROKER_HOST="$_H"
                BROKER_PORT="${{_P:-1883}}"
                echo "$_NOW $BROKER_HOST $BROKER_PORT" > "$BROKER_CACHE" 2>/dev/null || true
                return 0
            fi
        fi
    done

    BROKER_HOST="$DMD_HOST"
    BROKER_PORT="1883"
    echo "$_NOW $BROKER_HOST $BROKER_PORT" > "$BROKER_CACHE" 2>/dev/null || true
    return 0
}}

publish_once() {{
    _PAYLOAD="$1"
    _MOSQ=""
    if command -v mosquitto_pub >/dev/null 2>&1; then
        _MOSQ="mosquitto_pub"
    else
        for p in /usr/bin/mosquitto_pub /usr/local/bin/mosquitto_pub /bin/mosquitto_pub; do
            if [ -x "$p" ]; then
                _MOSQ="$p"
                break
            fi
        done
    fi

    if [ -n "$_MOSQ" ]; then
        "$_MOSQ" -h "$BROKER_HOST" -p "$BROKER_PORT" -t "$TOPIC" -i "batocera_es_$$" -m "$_PAYLOAD"
        return $?
    fi

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
import sys, socket, os
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
cid = f'batocera_es_{{os.getpid()}}'.encode('utf-8')
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
" "$BROKER_HOST" "$BROKER_PORT" "$TOPIC" "$_PAYLOAD"
        return $?
    fi
    echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] ERROR: Neither mosquitto_pub nor python3 found" >> "$LOG_FILE"
    return 1
}}

send_mqtt() {{
    _PAYLOAD="$1"
    _FORCE_RESOLVE="${{2:-0}}"

    # Terminate any previously running publication to avoid concurrent socket accumulation
    if [ -f "$MQTT_PUB_PID_FILE" ]; then
        OLD_PID=$(cat "$MQTT_PUB_PID_FILE" 2>/dev/null)
        if [ -n "$OLD_PID" ]; then
            kill "$OLD_PID" 2>/dev/null || true
        fi
        rm -f "$MQTT_PUB_PID_FILE"
    fi

    (
        resolve_broker "$_FORCE_RESOLVE"
        if ! publish_once "$_PAYLOAD"; then
            resolve_broker 1
            publish_once "$_PAYLOAD" || true
        fi
    ) >> "$LOG_FILE" 2>&1 &
    echo $! > "$MQTT_PUB_PID_FILE"
    return 0
}}

send_mqtt_debounced() {{
    _PAYLOAD="$1"
    TOKEN="$(date +%s%N 2>/dev/null || date +%s)_$$"
    echo "$TOKEN" > "$DEBOUNCE_TOKEN_FILE"
    (
        sleep 0.15
        CURRENT_TOKEN="$(cat "$DEBOUNCE_TOKEN_FILE" 2>/dev/null)"
        if [ "$CURRENT_TOKEN" = "$TOKEN" ]; then
            rm -f "$DEBOUNCE_TOKEN_FILE"
            send_mqtt "$_PAYLOAD" 0
        fi
    ) &
}}

clean_name() {{
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
}}

extract_system() {{
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
}}

EVENT="$1"
shift

if [ -z "$EVENT" ] || [ ! -z "${{EVENT##game*}}" -a ! -z "${{EVENT##system*}}" ]; then
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
        PAYLOAD="{{\"status\": \"playing\", \"game\": \"$GAME_CLEAN\", \"system\": \"$SYS_CLEAN\"}}"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: gameStart | Rom: $ROM_PATH | Sys: $SYS_NAME | Sent: $PAYLOAD" >> "$LOG_FILE"
        send_mqtt "$PAYLOAD" 1
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
        PAYLOAD="{{\"status\": \"stopped\", \"game\": \"$GAME_CLEAN\", \"system\": \"$SYS_CLEAN\"}}"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: gameStop | Rom: $ROM_PATH | Sys: $SYS_NAME | Sent: $PAYLOAD" >> "$LOG_FILE"
        send_mqtt "$PAYLOAD" 0
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
        PAYLOAD="{{\"status\": \"browsing\", \"game\": \"$GAME_CLEAN\", \"system\": \"$SYS_CLEAN\"}}"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: game-selected | Rom: $ROM_PATH | Sys: $SYS_NAME | Title: $TITLE | Sent: $PAYLOAD" >> "$LOG_FILE"
        send_mqtt_debounced "$PAYLOAD"
        ;;

    system-selected|systemSelected)
        SYS_CLEAN=$(clean_name "$1")
        PAYLOAD="{{\"status\": \"browsing\", \"system\": \"$SYS_CLEAN\", \"type\": \"system\"}}"
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
        PAYLOAD="{{\"status\": \"playing\", \"game\": \"$GAME_CLEAN\", \"system\": \"$SYS_CLEAN\"}}"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: game-start | Rom: $ROM_PATH | Sys: $SYS_NAME | Title: $TITLE | Sent: $PAYLOAD" >> "$LOG_FILE"
        send_mqtt "$PAYLOAD" 1
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
        PAYLOAD="{{\"status\": \"stopped\", \"game\": \"$GAME_CLEAN\", \"system\": \"$SYS_CLEAN\"}}"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: game-end | Sent: $PAYLOAD" >> "$LOG_FILE"
        send_mqtt "$PAYLOAD" 0
        ;;

    *)
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Unhandled Event: $EVENT | Args: $*" >> "$LOG_FILE"
        ;;
esac
"#,
        dmd_host = matrix_ip
    )
}

pub fn install_sync_script(
    target_ip: &str,
    matrix_ip: &str,
    custom_user: Option<String>,
    custom_pass: Option<String>,
    target_os: Option<String>,
) -> Result<String, String> {
    use std::net::ToSocketAddrs;
    let addr = (target_ip.trim(), 22)
        .to_socket_addrs()
        .map_err(|e| format!("Invalid target IP or hostname '{}': {}", target_ip, e))?
        .next()
        .ok_or_else(|| format!("Could not resolve target host '{}'", target_ip))?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(8))
        .map_err(|e| format!("Failed to connect to {}: {}", target_ip, e))?;

    let mut sess = Session::new().map_err(|e| format!("SSH session error: {}", e))?;
    sess.set_tcp_stream(tcp);
    sess.handshake()
        .map_err(|e| format!("SSH handshake failed: {}", e))?;

    let explicit_os = target_os.as_deref().and_then(TargetOS::from_str);

    let mut auth_candidates: Vec<(&str, String, String)> = Vec::new();

    if let (Some(u), Some(p)) = (custom_user, custom_pass) {
        if !u.is_empty() && !p.is_empty() {
            auth_candidates.push(("Custom", u, p));
        }
    }

    match explicit_os {
        Some(TargetOS::RetroPie) => {
            auth_candidates.push(("RetroPie", "pi".to_string(), "raspberry".to_string()));
            auth_candidates.push(("RetroPie Root", "root".to_string(), "root".to_string()));
        }
        Some(TargetOS::Recalbox) => {
            auth_candidates.push(("Recalbox", "root".to_string(), "recalboxroot".to_string()));
        }
        Some(TargetOS::Batocera) => {
            auth_candidates.push(("Batocera", "root".to_string(), "linux".to_string()));
        }
        None => {
            auth_candidates.push(("Recalbox", "root".to_string(), "recalboxroot".to_string()));
            auth_candidates.push(("Batocera", "root".to_string(), "linux".to_string()));
            auth_candidates.push(("RetroPie", "pi".to_string(), "raspberry".to_string()));
            auth_candidates.push(("RetroPie Root", "root".to_string(), "root".to_string()));
        }
    }

    let mut authenticated = false;
    for (label, user, pwd) in &auth_candidates {
        tracing::info!("Trying to connect to {} as {} ({})", target_ip, user, label);
        if sess.userauth_password(user, pwd).is_ok() {
            authenticated = true;
            break;
        }
    }

    if !authenticated {
        return Err(
            "Failed to authenticate via SSH. Check your credentials or target OS selection."
                .to_string(),
        );
    }

    let os = if let Some(resolved) = explicit_os {
        resolved
    } else {
        match detect_os_remotely(&sess) {
            Some(detected) => detected,
            None => {
                return Err("Failed to auto-detect gaming OS (checked /recalbox/share, /userdata/system, /opt/retropie). Please select OS explicitly.".to_string());
            }
        }
    };

    tracing::info!("Target OS identified as: {}", os.name());
    let daemon_code = generate_daemon_code(matrix_ip, os.topic());

    match os {
        TargetOS::RetroPie => {
            tracing::info!("Installing for RetroPie...");
            let daemon_path = "/opt/retropie/configs/all/arcadematrix_daemon.py";
            let onstart_path = "/opt/retropie/configs/all/runcommand-onstart.sh";
            let onend_path = "/opt/retropie/configs/all/runcommand-onend.sh";
            let autostart_path = "/opt/retropie/configs/all/autostart.sh";

            {
                let mut channel = sess.channel_session().map_err(|e| e.to_string())?;
                channel.exec("pkill -f arcadematrix_daemon.py || true; if ! command -v mosquitto_pub >/dev/null 2>&1; then sudo apt-get update -y && sudo apt-get install -y mosquitto-clients || true; fi").ok();
                channel.wait_close().ok();
            }

            {
                use std::io::Write;
                let mut channel = sess.channel_session().map_err(|e| e.to_string())?;
                channel.exec(&format!("cat > {}", daemon_path)).ok();
                channel.write_all(daemon_code.as_bytes()).ok();
                channel.send_eof().ok();
                channel.wait_close().ok();
            }
            {
                let mut channel = sess.channel_session().map_err(|e| e.to_string())?;
                channel.exec(&format!("chmod +x {}", daemon_path)).ok();
                channel.wait_close().ok();
            }

            let onstart_hook = r#"
# ArcadeMatrix Runcommand Hook
SYSTEM="$1"
EMULATOR="$2"
ROMPATH="$3"
cat > /tmp/es_state.inf << EOF
SystemId=$SYSTEM
GamePath=$ROMPATH
State=playing
EOF
"#;
            {
                let mut channel = sess.channel_session().map_err(|e| e.to_string())?;
                let cmd = format!(
                    "touch {path} && if ! grep -q 'ArcadeMatrix Runcommand Hook' {path}; then cat >> {path} << 'EOF'\n{hook}\nEOF\nfi\nchmod +x {path}\n",
                    path = onstart_path,
                    hook = onstart_hook
                );
                channel.exec(&cmd).ok();
                channel.wait_close().ok();
            }

            let onend_hook = r#"
# ArcadeMatrix Runcommand Hook
cat > /tmp/es_state.inf << EOF
SystemId=
GamePath=
State=stopped
EOF
"#;
            {
                let mut channel = sess.channel_session().map_err(|e| e.to_string())?;
                let cmd = format!(
                    "touch {path} && if ! grep -q 'ArcadeMatrix Runcommand Hook' {path}; then cat >> {path} << 'EOF'\n{hook}\nEOF\nfi\nchmod +x {path}\n",
                    path = onend_path,
                    hook = onend_hook
                );
                channel.exec(&cmd).ok();
                channel.wait_close().ok();
            }

            {
                let mut channel = sess.channel_session().map_err(|e| e.to_string())?;
                let cmd = format!(
                    "if [ -f {auto} ] && ! grep -q 'arcadematrix_daemon.py' {auto}; then sed -i '/emulationstation/i python3 {daemon} > /opt/retropie/configs/all/daemon.log 2>&1 &' {auto}; fi\n",
                    auto = autostart_path,
                    daemon = daemon_path
                );
                channel.exec(&cmd).ok();
                channel.wait_close().ok();
            }

            {
                let mut channel = sess.channel_session().map_err(|e| e.to_string())?;
                channel
                    .exec(&format!(
                        "nohup python3 {} > /opt/retropie/configs/all/daemon.log 2>&1 &",
                        daemon_path
                    ))
                    .ok();
                channel.wait_close().ok();
            }
        }
        TargetOS::Batocera => {
            tracing::info!("Installing for Batocera...");
            let hook_path = "/userdata/system/scripts/arcadematrix_mqtt.sh";

            // Check Batocera version
            if let Ok(mut channel) = sess.channel_session() {
                if channel.exec("cat /usr/share/batocera/batocera.version 2>/dev/null || cat /etc/batocera-version 2>/dev/null || true").is_ok() {
                    let mut version_str = String::new();
                    let _ = channel.read_to_string(&mut version_str);
                    let _ = channel.wait_close();
                    let trimmed = version_str.trim();
                    if !trimmed.is_empty() {
                        tracing::info!("Detected Batocera version: {}", trimmed);
                        if let Some(first_tok) = trimmed.split_whitespace().next() {
                            if let Ok(ver_num) = first_tok.parse::<u32>() {
                                if ver_num < 33 {
                                    tracing::warn!(
                                        "Batocera version {} is < 33. EmulationStation dynamic marquee browsing (game-selected) requires Batocera v33+. Game launch/stop events will still work.",
                                        ver_num
                                    );
                                }
                            }
                        }
                    }
                }
            }

            // 1. Clean up legacy daemons (separate commands to avoid Dropbear CMD_LEN issues)
            {
                let mut channel = sess
                    .channel_session()
                    .map_err(|e| format!("SSH channel error (cleanup kill): {}", e))?;
                channel.exec("pkill -f arcadematrix_daemon.py 2>/dev/null; pkill -f arcadematrix_mqtt.sh 2>/dev/null; true")
                    .map_err(|e| format!("exec failed (cleanup kill): {}", e))?;
                let mut out = String::new();
                let _ = channel.read_to_string(&mut out);
                channel.wait_close().ok();
                tracing::info!("Batocera cleanup kill: done");
            }

            // 2. Remove legacy script files
            {
                let mut channel = sess
                    .channel_session()
                    .map_err(|e| format!("SSH channel error (cleanup rm): {}", e))?;
                channel.exec("rm -f /userdata/system/arcadematrix_daemon.py /userdata/system/scripts/arcadematrix_hook.sh /userdata/system/scripts/arcadematrix_mqtt.sh")
                    .map_err(|e| format!("exec failed (cleanup rm files): {}", e))?;
                let mut out = String::new();
                let _ = channel.read_to_string(&mut out);
                channel.wait_close().ok();
                tracing::info!("Batocera cleanup rm files: done");
            }

            // 3. Remove legacy ES event directories
            {
                let mut channel = sess
                    .channel_session()
                    .map_err(|e| format!("SSH channel error (cleanup rm dirs): {}", e))?;
                channel.exec("rm -rf /userdata/system/scripts/game-selected /userdata/system/scripts/game-start /userdata/system/scripts/game-end /userdata/system/scripts/system-selected")
                    .map_err(|e| format!("exec failed (cleanup rm script dirs): {}", e))?;
                let mut out = String::new();
                let _ = channel.read_to_string(&mut out);
                channel.wait_close().ok();
                tracing::info!("Batocera cleanup rm script dirs: done");
            }

            {
                let mut channel = sess
                    .channel_session()
                    .map_err(|e| format!("SSH channel error (cleanup rm ES dirs): {}", e))?;
                channel.exec("rm -rf /userdata/system/configs/emulationstation/scripts/game-selected /userdata/system/configs/emulationstation/scripts/game-start /userdata/system/configs/emulationstation/scripts/game-end /userdata/system/configs/emulationstation/scripts/system-selected")
                    .map_err(|e| format!("exec failed (cleanup rm ES dirs): {}", e))?;
                let mut out = String::new();
                let _ = channel.read_to_string(&mut out);
                channel.wait_close().ok();
                tracing::info!("Batocera cleanup rm ES dirs: done");
            }

            // 4. Clean custom.sh and ensure scripts directory exists
            {
                let mut channel = sess
                    .channel_session()
                    .map_err(|e| format!("SSH channel error (cleanup custom.sh): {}", e))?;
                channel.exec("if [ -f /userdata/system/custom.sh ]; then sed -i '/arcadematrix_daemon.py/d' /userdata/system/custom.sh; fi; mkdir -p /userdata/system/scripts")
                    .map_err(|e| format!("exec failed (cleanup custom.sh + mkdir): {}", e))?;
                let mut out = String::new();
                let _ = channel.read_to_string(&mut out);
                channel.wait_close().ok();
                tracing::info!("Batocera cleanup custom.sh + mkdir: done");
            }

            // 5. Install one-shot shell hook publishing via mosquitto_pub or python3 raw socket
            let hook_code = generate_batocera_hook_code(matrix_ip);

            let expected_size = hook_code.len();
            tracing::info!("Batocera hook_code size: {} bytes", expected_size);

            // 6. Write hook script via stdin pipe (bypasses Dropbear MAX_CMD_LEN)
            {
                use std::io::Write;
                let mut channel = sess
                    .channel_session()
                    .map_err(|e| format!("SSH channel error (write hook): {}", e))?;
                channel
                    .exec(&format!("cat > {}", hook_path))
                    .map_err(|e| format!("exec 'cat > {}' failed: {}", hook_path, e))?;
                channel.write_all(hook_code.as_bytes()).map_err(|e| {
                    format!(
                        "write_all to {} failed ({} bytes): {}",
                        hook_path, expected_size, e
                    )
                })?;
                channel
                    .send_eof()
                    .map_err(|e| format!("send_eof failed after writing {}: {}", hook_path, e))?;
                channel.wait_close().ok();
                tracing::info!(
                    "Batocera hook written: {} bytes to {}",
                    expected_size,
                    hook_path
                );
            }

            // 7. Verify the script was actually written
            {
                let mut channel = sess
                    .channel_session()
                    .map_err(|e| format!("SSH channel error (verify): {}", e))?;
                channel
                    .exec(&format!(
                        "test -f {} && wc -c < {} && head -1 {}",
                        hook_path, hook_path, hook_path
                    ))
                    .map_err(|e| format!("exec verify failed: {}", e))?;
                let mut verify_out = String::new();
                let _ = channel.read_to_string(&mut verify_out);
                channel.wait_close().ok();
                let trimmed = verify_out.trim();
                tracing::info!("Batocera verify hook: output={}", trimmed);
                if trimmed.is_empty() {
                    return Err(format!(
                        "CRITICAL: Hook script {} was NOT written to the Batocera filesystem. \
                         The file does not exist after write_all. This typically means Dropbear \
                         did not pipe stdin to the 'cat' command. Batocera version or SSH server \
                         may not support exec+stdin piping.",
                        hook_path
                    ));
                }
                // Check first line starts with shebang
                if !trimmed.contains("#!/bin/sh") {
                    tracing::warn!(
                        "Batocera hook verify: file exists but may be empty or corrupt. Output: {}",
                        trimmed
                    );
                }
            }

            // 8. chmod and create ES event directory wrappers (separate commands)
            {
                let mut channel = sess
                    .channel_session()
                    .map_err(|e| format!("SSH channel error (chmod): {}", e))?;
                channel
                    .exec(&format!("chmod 755 {}", hook_path))
                    .map_err(|e| format!("chmod 755 {} failed: {}", hook_path, e))?;
                let mut out = String::new();
                let _ = channel.read_to_string(&mut out);
                channel.wait_close().ok();
                tracing::info!("Batocera chmod hook: done");
            }

            // 9. Create ES event wrapper scripts in separate channels
            for evt in &["game-selected", "system-selected", "game-start", "game-end"] {
                let mut channel = sess
                    .channel_session()
                    .map_err(|e| format!("SSH channel error (ES dir {}): {}", evt, e))?;
                let cmd = format!(
                    "mkdir -p /userdata/system/configs/emulationstation/scripts/{evt} && \
                     printf '#!/bin/sh\\n/userdata/system/scripts/arcadematrix_mqtt.sh {evt} \"$@\"\\n' \
                     > /userdata/system/configs/emulationstation/scripts/{evt}/arcadematrix_mqtt.sh && \
                     chmod 755 /userdata/system/configs/emulationstation/scripts/{evt}/arcadematrix_mqtt.sh",
                    evt = evt
                );
                channel
                    .exec(&cmd)
                    .map_err(|e| format!("exec ES wrapper {} failed: {}", evt, e))?;
                let mut out = String::new();
                let _ = channel.read_to_string(&mut out);
                channel.wait_close().ok();
                tracing::info!("Batocera ES wrapper {}: done", evt);
            }

            // 10. Final chmod on ES scripts root
            {
                let mut channel = sess
                    .channel_session()
                    .map_err(|e| format!("SSH channel error (chmod ES): {}", e))?;
                channel
                    .exec("chmod -R 755 /userdata/system/configs/emulationstation/scripts")
                    .map_err(|e| format!("chmod -R ES scripts failed: {}", e))?;
                let mut out = String::new();
                let _ = channel.read_to_string(&mut out);
                channel.wait_close().ok();
                tracing::info!("Batocera chmod -R ES scripts: done");
            }

            // 11. Sync filesystem before reboot to flush all pending writes
            {
                let mut channel = sess
                    .channel_session()
                    .map_err(|e| format!("SSH channel error (sync): {}", e))?;
                channel
                    .exec("sync")
                    .map_err(|e| format!("sync failed: {}", e))?;
                let mut out = String::new();
                let _ = channel.read_to_string(&mut out);
                channel.wait_close().ok();
                tracing::info!("Batocera sync: done");
            }
        }
        TargetOS::Recalbox => {
            tracing::info!("Installing for Recalbox...");
            let target_dir = "/recalbox/share/userscripts";
            {
                let mut channel = sess.channel_session().map_err(|e| e.to_string())?;
                channel.exec(&format!("mkdir -p {}", target_dir)).ok();
                channel.wait_close().ok();
            }

            {
                let mut channel = sess.channel_session().map_err(|e| e.to_string())?;
                channel.exec(&format!(
                    "cd {} && for f in *.sh; do case \"$f\" in 'arcadematrix_launcher(permanent).sh') ;; *) rm -f \"$f\" ;; esac; done; rm -f /recalbox/share/arcadematrix_daemon.py; pkill -f recalbox_mqtt_status || true; pkill -f arcadematrix_mqtt || true; pkill -f arcadematrix_daemon.py || true",
                    target_dir
                )).ok();
                channel.wait_close().ok();
            }

            let daemon_path = "/recalbox/share/arcadematrix_daemon.py";
            {
                use std::io::Write;
                let mut channel = sess.channel_session().map_err(|e| e.to_string())?;
                channel.exec(&format!("cat > {}", daemon_path)).ok();
                channel.write_all(daemon_code.as_bytes()).ok();
                channel.send_eof().ok();
                channel.wait_close().ok();
            }

            let launcher_code = r#"#!/bin/sh
if [ -z "$1" ] || [ "$1" = "-action" -a "$2" = "start" ]; then
    pkill -f arcadematrix_daemon.py || true
    python3 /recalbox/share/arcadematrix_daemon.py > /recalbox/share/userscripts/daemon.log 2>&1 &
fi
"#;
            let launcher_path = format!("{}/arcadematrix_launcher(permanent).sh", target_dir);
            {
                use std::io::Write;
                let mut channel = sess.channel_session().map_err(|e| e.to_string())?;
                channel.exec(&format!("cat > '{}'", launcher_path)).ok();
                channel.write_all(launcher_code.as_bytes()).ok();
                channel.send_eof().ok();
                channel.wait_close().ok();
            }
            {
                let mut channel = sess.channel_session().map_err(|e| e.to_string())?;
                channel.exec(&format!("chmod +x '{}'", launcher_path)).ok();
                channel.wait_close().ok();
            }

            {
                let mut channel = sess.channel_session().map_err(|e| e.to_string())?;
                channel
                    .exec(&format!("rm -f {}/arcadematrix_mqtt.sh", target_dir))
                    .ok();
                channel.wait_close().ok();
            }
        }
    }

    if os != TargetOS::RetroPie {
        tracing::info!("Rebooting target system...");
        let mut channel = sess.channel_session().map_err(|e| e.to_string())?;
        channel.exec("sleep 1 && reboot").ok();
        channel.wait_close().ok();
        Ok(format!(
            "Successfully installed! {} is now rebooting...",
            os.name()
        ))
    } else {
        Ok(format!(
            "Successfully installed! {} daemon is running.",
            os.name()
        ))
    }
}

pub fn fetch_sync_logs(
    target_ip: &str,
    custom_user: Option<String>,
    custom_pass: Option<String>,
    target_os: Option<String>,
) -> Result<String, String> {
    let addr = format!("{}:22", target_ip)
        .to_socket_addrs()
        .map_err(|e| format!("Invalid target address '{}': {}", target_ip, e))?
        .next()
        .ok_or_else(|| format!("Could not resolve target address '{}'", target_ip))?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(8))
        .map_err(|e| format!("Failed to connect to {}: {}", target_ip, e))?;

    let mut sess = Session::new().map_err(|e| format!("SSH session error: {}", e))?;
    sess.set_tcp_stream(tcp);
    sess.handshake()
        .map_err(|e| format!("SSH handshake failed: {}", e))?;

    let explicit_os = target_os.as_deref().and_then(TargetOS::from_str);

    let mut auth_candidates: Vec<(&str, String, String)> = Vec::new();

    if let (Some(u), Some(p)) = (custom_user, custom_pass) {
        if !u.is_empty() && !p.is_empty() {
            auth_candidates.push(("Custom", u, p));
        }
    }

    match explicit_os {
        Some(TargetOS::RetroPie) => {
            auth_candidates.push(("RetroPie", "pi".to_string(), "raspberry".to_string()));
            auth_candidates.push(("RetroPie Root", "root".to_string(), "root".to_string()));
        }
        Some(TargetOS::Recalbox) => {
            auth_candidates.push(("Recalbox", "root".to_string(), "recalboxroot".to_string()));
        }
        Some(TargetOS::Batocera) => {
            auth_candidates.push(("Batocera", "root".to_string(), "linux".to_string()));
        }
        None => {
            auth_candidates.push(("Recalbox", "root".to_string(), "recalboxroot".to_string()));
            auth_candidates.push(("Batocera", "root".to_string(), "linux".to_string()));
            auth_candidates.push(("RetroPie", "pi".to_string(), "raspberry".to_string()));
            auth_candidates.push(("RetroPie Root", "root".to_string(), "root".to_string()));
        }
    }

    let mut authenticated = false;
    for (label, user, pwd) in &auth_candidates {
        tracing::info!("Trying to connect to {} as {} ({})", target_ip, user, label);
        if sess.userauth_password(user, pwd).is_ok() {
            authenticated = true;
            break;
        }
    }

    if !authenticated {
        return Err(
            "Failed to authenticate via SSH. Check credentials or OS selection.".to_string(),
        );
    }

    let os = if let Some(resolved) = explicit_os {
        resolved
    } else {
        detect_os_remotely(&sess).unwrap_or(TargetOS::Recalbox)
    };

    let log_path = os.log_path();

    let mut channel = sess
        .channel_session()
        .map_err(|_| "Failed to open SSH channel")?;
    channel
        .exec(&format!(
            "tail -n 100 {} 2>/dev/null || echo 'Log file not found or empty'",
            log_path
        ))
        .map_err(|_| "Failed to execute command on target")?;

    let mut logs = String::new();
    channel
        .read_to_string(&mut logs)
        .map_err(|_| "Failed to read output")?;
    channel.wait_close().ok();

    if logs.trim().is_empty() {
        Ok("Log file is empty.".to_string())
    } else {
        Ok(logs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generated_recalbox_daemon_uses_dynamic_broker_target() {
        let code = generate_daemon_code("192.168.1.42", "system/playing/recalbox");
        assert!(code.contains("DMD_HOST = \"192.168.1.42\""));
        assert!(code.contains("/api/mqtt/broker-target"));
        assert!(code.contains("def resolve_broker"));
        assert!(!code.contains("BROKER = \"192.168.1.42\""));
    }

    #[test]
    fn test_generated_batocera_hook_uses_dynamic_broker_target() {
        let hook = generate_batocera_hook_code("192.168.1.42");
        assert!(hook.starts_with("#!/bin/sh\n"));
        assert!(hook.contains("DMD_HOST=\"192.168.1.42\""));
        assert!(hook.contains("/api/mqtt/broker-target"));
        assert!(hook.contains("resolve_broker()"));
        assert!(!hook.contains("\nBROKER=\"192.168.1.42\""));
    }
}

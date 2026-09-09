use ssh2::Session;
use std::io::Read;
use std::net::TcpStream;
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

BROKER = "{}"
TOPIC = "{}"

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
        
    print("Daemon started (lightweight)!", flush=True)
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
                    subprocess.run(["mosquitto_pub", "-h", BROKER, "-t", TOPIC, "-m", msg], timeout=2, check=False)
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

pub fn install_sync_script(
    target_ip: &str,
    matrix_ip: &str,
    custom_user: Option<String>,
    custom_pass: Option<String>,
    target_os: Option<String>,
) -> Result<String, String> {
    let tcp = TcpStream::connect_timeout(
        &format!("{}:22", target_ip).parse().unwrap(),
        Duration::from_secs(5),
    )
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
                let mut channel = sess.channel_session().map_err(|e| e.to_string())?;
                channel
                    .exec(&format!(
                        "cat > {} << 'EOF'\n{}\nEOF\nchmod +x {}\n",
                        daemon_path, daemon_code, daemon_path
                    ))
                    .ok();
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

            // 1. Clean up legacy daemons, shims, and custom.sh lines
            {
                let mut channel = sess.channel_session().map_err(|e| e.to_string())?;
                channel
                    .exec("pkill -f arcadematrix_daemon.py || true; pkill -f arcadematrix_mqtt.sh || true; rm -f /userdata/system/arcadematrix_daemon.py /userdata/system/scripts/arcadematrix_hook.sh /userdata/system/scripts/arcadematrix_mqtt.sh; rm -f /userdata/system/scripts/game-selected /userdata/system/scripts/game-start /userdata/system/scripts/game-end /userdata/system/scripts/system-selected; rm -f /userdata/system/configs/emulationstation/scripts/game-selected /userdata/system/configs/emulationstation/scripts/game-start /userdata/system/configs/emulationstation/scripts/game-end /userdata/system/configs/emulationstation/scripts/system-selected; if [ -f /userdata/system/custom.sh ]; then sed -i '/arcadematrix_daemon.py/d' /userdata/system/custom.sh; fi; mkdir -p /userdata/system/scripts")
                    .ok();
                channel.wait_close().ok();
            }

            // 2. Install one-shot shell hook publishing via mosquitto_pub
            let hook_code = format!(
                r#"#!/bin/sh
BROKER="{broker}"
TOPIC="system/playing/batocera"
LOG_FILE="/userdata/system/scripts/daemon.log"

clean_name() {{
    echo "$1" | sed -E \
        -e 's/^[Aa]rcade [Mm]anufacturer //' \
        -e 's/^[Aa]rcade [Ss]ystem //' \
        -e 's/^[Aa]rcade [Gg]enre //' \
        -e 's/^[Aa]rcade [Cc]ollection //' \
        -e 's/^[Mm]anufacturer //' \
        -e 's/^[Ss]ystem //' \
        -e 's/^[Gg]enre //' \
        -e 's/^[Cc]ollection //' | sed -E 's/^[_-]//' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//'
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
        PAYLOAD="{{\"status\": \"playing\", \"game\": \"$GAME_CLEAN\", \"system\": \"$SYS_CLEAN\"}}"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: gameStart | Rom: $ROM_PATH | Sys: $SYS_NAME | Sent: $PAYLOAD" >> "$LOG_FILE"
        mosquitto_pub -h "$BROKER" -t "$TOPIC" -m "$PAYLOAD" >> "$LOG_FILE" 2>&1 &
        ;;

    gameStop)
        PAYLOAD="{{\"status\": \"stopped\"}}"
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
        PAYLOAD="{{\"status\": \"browsing\", \"game\": \"$GAME_CLEAN\", \"system\": \"$SYS_CLEAN\"}}"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: game-selected | Rom: $ROM_PATH | Sys: $SYS_NAME | Title: $TITLE | Sent: $PAYLOAD" >> "$LOG_FILE"
        mosquitto_pub -h "$BROKER" -t "$TOPIC" -m "$PAYLOAD" >> "$LOG_FILE" 2>&1 &
        ;;

    system-selected|systemSelected)
        SYS_CLEAN=$(clean_name "$1")
        PAYLOAD="{{\"status\": \"browsing\", \"system\": \"$SYS_CLEAN\", \"type\": \"system\"}}"
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
        PAYLOAD="{{\"status\": \"playing\", \"game\": \"$GAME_CLEAN\", \"system\": \"$SYS_CLEAN\"}}"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: game-start | Rom: $ROM_PATH | Sys: $SYS_NAME | Title: $TITLE | Sent: $PAYLOAD" >> "$LOG_FILE"
        mosquitto_pub -h "$BROKER" -t "$TOPIC" -m "$PAYLOAD" >> "$LOG_FILE" 2>&1 &
        ;;

    game-end)
        PAYLOAD="{{\"status\": \"stopped\"}}"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Event: game-end | Sent: $PAYLOAD" >> "$LOG_FILE"
        mosquitto_pub -h "$BROKER" -t "$TOPIC" -m "$PAYLOAD" >> "$LOG_FILE" 2>&1 &
        ;;

    *)
        echo "$(date '+%Y-%m-%d %H:%M:%S') [arcadematrix] Unhandled Event: $EVENT | Args: $*" >> "$LOG_FILE"
        ;;
esac
"#,
                broker = matrix_ip
            );

            {
                let mut channel = sess.channel_session().map_err(|e| e.to_string())?;
                channel
                    .exec(&format!(
                        "cat > {path} << 'EOF'\n{code}\nEOF\nchmod +x {path}\nfor evt in game-selected system-selected game-start game-end; do dir=\"/userdata/system/configs/emulationstation/scripts/$evt\"; mkdir -p \"$dir\"; cat > \"$dir/arcadematrix_mqtt.sh\" << 'EOFEVT'\n#!/bin/sh\n/userdata/system/scripts/arcadematrix_mqtt.sh \"$evt\" \"$@\"\nEOFEVT\nchmod +x \"$dir/arcadematrix_mqtt.sh\"; done\n",
                        path = hook_path,
                        code = hook_code
                    ))
                    .ok();
                channel.wait_close().ok();
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
                let mut channel = sess.channel_session().map_err(|e| e.to_string())?;
                channel
                    .exec(&format!(
                        "cat > {} << 'EOF'\n{}\nEOF\n",
                        daemon_path, daemon_code
                    ))
                    .ok();
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
                let mut channel = sess.channel_session().map_err(|e| e.to_string())?;
                channel
                    .exec(&format!(
                        "cat > '{}' << 'EOF'\n{}\nEOF\nchmod +x '{}'",
                        launcher_path, launcher_code, launcher_path
                    ))
                    .ok();
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
    let tcp = TcpStream::connect_timeout(
        &format!("{}:22", target_ip).parse().unwrap(),
        Duration::from_secs(5),
    )
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

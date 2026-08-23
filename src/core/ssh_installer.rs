use ssh2::Session;
use std::io::Read;
use std::net::TcpStream;
use std::time::Duration;

pub fn install_sync_script(
    target_ip: &str,
    matrix_ip: &str,
    custom_user: Option<String>,
    custom_pass: Option<String>,
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

    let mut targets = vec![
        (
            "Recalbox",
            "root".to_string(),
            "recalboxroot".to_string(),
            "/recalbox/share/userscripts",
        ),
        (
            "Batocera",
            "root".to_string(),
            "linux".to_string(),
            "/userdata/system/scripts",
        ),
    ];

    if let (Some(u), Some(p)) = (custom_user, custom_pass) {
        if !u.is_empty() && !p.is_empty() {
            // Insert custom targets at the beginning so they are tried first
            targets.insert(
                0,
                (
                    "Custom (Batocera path)",
                    u.clone(),
                    p.clone(),
                    "/userdata/system/scripts",
                ),
            );
            targets.insert(
                0,
                (
                    "Custom (Recalbox path)",
                    u,
                    p,
                    "/recalbox/share/userscripts",
                ),
            );
        }
    }

    let mut connected = false;
    let mut system_name = "";
    let mut target_dir = "";

    for (sys_name, user, pwd, t_dir) in targets.iter() {
        tracing::info!(
            "Trying to connect to {} as {} (OS: {})...",
            target_ip,
            user,
            sys_name
        );
        if sess.userauth_password(user, pwd).is_ok() {
            connected = true;
            system_name = sys_name;
            target_dir = t_dir;
            break;
        }
    }

    if !connected {
        return Err(
            "Failed to authenticate. Is it Recalbox (pwd: recalboxroot) or Batocera (pwd: linux)?"
                .to_string(),
        );
    }

    let daemon_code = format!(
        r#"import subprocess
import time
import os

BROKER = "{}"
TOPIC = "recalbox/system/playing"

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
    time.sleep(5)
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
                    msg = '{{"status": "browsing", "system": "' + str(current_key[1]) + '", "type": "system"}}'
                else:
                    gbase = os.path.splitext(os.path.basename(current_key[0]))[0]
                    gbase = clean_system_name(gbase)
                    msg = '{{"status": "' + current_key[2] + '", "game": "' + gbase + '", "system": "' + str(current_key[1]) + '"}}'

                try:
                    subprocess.run(["mosquitto_pub", "-h", BROKER, "-t", TOPIC, "-m", msg], timeout=2, check=False)
                except subprocess.TimeoutExpired:
                    pass
        except Exception as e:
            print("Error: " + str(e), flush=True)

        time.sleep(0.1)

if __name__ == "__main__":
    main()
"#,
        matrix_ip
    );

    if system_name == "Batocera" {
        tracing::info!("Creating directory {}...", target_dir);
        {
            let mut channel = sess.channel_session().unwrap();
            channel
                .exec(&format!(
                    "mkdir -p {} /userdata/system/configs/emulationstation/scripts",
                    target_dir
                ))
                .ok();
            channel.wait_close().ok();
        }

        tracing::info!("Cleaning up previous scripts...");
        {
            let mut channel = sess.channel_session().unwrap();
            channel
                .exec("pkill -f arcadematrix_daemon.py || true; pkill -f arcadematrix_mqtt.sh || true; rm -f /userdata/system/scripts/arcadematrix_mqtt.sh")
                .ok();
            channel.wait_close().ok();
        }

        let daemon_path = "/userdata/system/arcadematrix_daemon.py";
        tracing::info!("Uploading script to {}...", daemon_path);
        {
            let mut channel = sess.channel_session().unwrap();
            channel
                .exec(&format!(
                    "cat > {} << 'EOF'\n{}\nEOF\n",
                    daemon_path, daemon_code
                ))
                .ok();
            channel.wait_close().ok();
        }

        let hook_path = "/userdata/system/scripts/arcadematrix_hook.sh";
        let hook_code = r#"#!/bin/sh
EVENT="$(basename "$0")"
SYSTEM="$1"
ROMPATH="$2"
GAMENAME="$3"

case "$EVENT" in
    game-selected)
        STATE="browsing"
        ;;
    game-start)
        STATE="playing"
        ;;
    game-end)
        STATE="stopped"
        ;;
    system-selected)
        STATE="browsing"
        ROMPATH=""
        ;;
    *)
        STATE="browsing"
        ;;
esac

cat > /tmp/es_state.inf << EOF
SystemId=$SYSTEM
GamePath=$ROMPATH
State=$STATE
EOF
"#;
        tracing::info!("Installing Batocera event hooks...");
        {
            let mut channel = sess.channel_session().unwrap();
            let install_hooks_cmd = format!(
                "cat > {} << 'EOF'\n{}\nEOF\nchmod +x {}\nfor evt in game-selected game-start game-end system-selected; do ln -sf {} /userdata/system/scripts/$evt; ln -sf {} /userdata/system/configs/emulationstation/scripts/$evt; done\n",
                hook_path, hook_code, hook_path, hook_path, hook_path
            );
            channel.exec(&install_hooks_cmd).ok();
            channel.wait_close().ok();
        }

        tracing::info!("Configuring custom.sh on Batocera...");
        {
            let mut channel = sess.channel_session().unwrap();
            let setup_cmd = r#"
if [ ! -f /userdata/system/custom.sh ]; then
    echo '#!/bin/sh' > /userdata/system/custom.sh
    echo '[ "$1" = "start" ] && python3 /userdata/system/arcadematrix_daemon.py > /userdata/system/scripts/daemon.log 2>&1 &' >> /userdata/system/custom.sh
    chmod +x /userdata/system/custom.sh
else
    if ! grep -q 'arcadematrix_daemon.py' /userdata/system/custom.sh; then
        echo '[ "$1" = "start" ] && python3 /userdata/system/arcadematrix_daemon.py > /userdata/system/scripts/daemon.log 2>&1 &' >> /userdata/system/custom.sh
    fi
fi
"#;
            channel.exec(setup_cmd).ok();
            channel.wait_close().ok();
        }
    } else {
        let launcher_code = r#"#!/bin/sh
if [ -z "$1" ] || [ "$1" = "-action" -a "$2" = "start" ]; then
    pkill -f arcadematrix_daemon.py || true
    python3 /recalbox/share/arcadematrix_daemon.py > /recalbox/share/userscripts/daemon.log 2>&1 &
fi
"#;

        tracing::info!("Creating directory {}...", target_dir);
        {
            let mut channel = sess.channel_session().unwrap();
            channel.exec(&format!("mkdir -p {}", target_dir)).ok();
            channel.wait_close().ok();
        }

        tracing::info!("Cleaning up ALL legacy scripts...");
        {
            let mut channel = sess.channel_session().unwrap();
            channel.exec(&format!(
                "cd {} && for f in *.sh; do case \"$f\" in 'arcadematrix_launcher(permanent).sh') ;; *) rm -f \"$f\" ;; esac; done; rm -f /recalbox/share/arcadematrix_daemon.py; pkill -f recalbox_mqtt_status || true; pkill -f arcadematrix_mqtt || true; pkill -f arcadematrix_daemon.py || true",
                target_dir
            )).ok();
            channel.wait_close().ok();
        }

        let daemon_path = "/recalbox/share/arcadematrix_daemon.py";
        tracing::info!("Uploading script to {}...", daemon_path);
        {
            let mut channel = sess.channel_session().unwrap();
            channel
                .exec(&format!(
                    "cat > {} << 'EOF'\n{}\nEOF\n",
                    daemon_path, daemon_code
                ))
                .ok();
            channel.wait_close().ok();
        }

        let launcher_path = format!("{}/arcadematrix_launcher(permanent).sh", target_dir);
        tracing::info!("Uploading script to {}...", launcher_path);
        {
            let mut channel = sess.channel_session().unwrap();
            channel
                .exec(&format!(
                    "cat > '{}' << 'EOF'\n{}\nEOF\nchmod +x '{}'",
                    launcher_path, launcher_code, launcher_path
                ))
                .ok();
            channel.wait_close().ok();
        }

        {
            let mut channel = sess.channel_session().unwrap();
            channel
                .exec(&format!("rm -f {}/arcadematrix_mqtt.sh", target_dir))
                .ok();
            channel.wait_close().ok();
        }
    }

    tracing::info!("Rebooting target system...");
    {
        let mut channel = sess.channel_session().unwrap();
        channel.exec("sleep 1 && reboot").ok();
        channel.wait_close().ok();
    }

    Ok(format!(
        "Successfully installed! {} is now rebooting...",
        system_name
    ))
}

pub fn fetch_sync_logs(
    target_ip: &str,
    custom_user: Option<String>,
    custom_pass: Option<String>,
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

    let mut targets = vec![
        (
            "Recalbox",
            "root".to_string(),
            "recalboxroot".to_string(),
            "/recalbox/share/userscripts/daemon.log",
        ),
        (
            "Batocera",
            "root".to_string(),
            "linux".to_string(),
            "/userdata/system/scripts/daemon.log",
        ),
    ];

    if let (Some(u), Some(p)) = (custom_user, custom_pass) {
        if !u.is_empty() && !p.is_empty() {
            targets.insert(
                0,
                (
                    "Custom (Batocera path)",
                    u.clone(),
                    p.clone(),
                    "/userdata/system/scripts/daemon.log",
                ),
            );
            targets.insert(
                0,
                (
                    "Custom (Recalbox path)",
                    u,
                    p,
                    "/recalbox/share/userscripts/daemon.log",
                ),
            );
        }
    }

    let mut connected = false;
    let mut log_path = "";

    for (_sys_name, user, pwd, path) in targets.iter() {
        if sess.userauth_password(user, pwd).is_ok() {
            connected = true;
            log_path = path;
            break;
        }
    }

    if !connected {
        return Err("Failed to authenticate via SSH. Check credentials.".to_string());
    }

    let mut channel = sess
        .channel_session()
        .map_err(|_| "Failed to open SSH channel")?;
    channel
        .exec(&format!(
            "tail -n 100 {} || echo 'Log file not found or empty'",
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

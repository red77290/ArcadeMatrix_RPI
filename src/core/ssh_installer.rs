use ssh2::Session;
use std::net::TcpStream;
use std::time::Duration;

pub fn install_sync_script(target_ip: &str, matrix_ip: &str) -> Result<String, String> {
    let tcp = TcpStream::connect_timeout(
        &format!("{}:22", target_ip).parse().unwrap(),
        Duration::from_secs(5),
    )
    .map_err(|e| format!("Failed to connect to {}: {}", target_ip, e))?;

    let mut sess = Session::new().map_err(|e| format!("SSH session error: {}", e))?;
    sess.set_tcp_stream(tcp);
    sess.handshake()
        .map_err(|e| format!("SSH handshake failed: {}", e))?;

    let passwords = vec![
        ("Recalbox", "recalboxroot", "/recalbox/share/userscripts"),
        ("Batocera", "linux", "/userdata/system/scripts"),
    ];

    let mut connected = false;
    let mut system_name = "";
    let mut target_dir = "";

    for (sys_name, pwd, t_dir) in passwords {
        tracing::info!("Trying to connect to {} as {}...", target_ip, sys_name);
        if sess.userauth_password("root", pwd).is_ok() {
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

    if system_name == "Batocera" {
        let script_content = format!(
            r#"#!/bin/sh
# ArcadeMatrix Auto-Sync Script for Batocera

BROKER="{}"
TOPIC="recalbox/system/playing"
ACTION=$1
ROM_PATH=$2
SYSTEM_NAME=$3

if [ "$ACTION" = "gameStart" ] || [ "$ACTION" = "gameSelected" ]; then
    GAME_BASENAME=$(basename "$ROM_PATH" | sed 's/\.[^.]*$//')
    ROM_DIR=$(dirname "$ROM_PATH")
    
    MARQUEE_PATH=""
    for ext in png jpg gif; do
        for prefix in "images/" "downloaded_images/" "media/marquees/" "media/images/" "media/wheels/" ""; do
            for suffix in "-marquee" "-wheel" "-image" "-thumb" ""; do
                if [ -f "$ROM_DIR/$prefix${{GAME_BASENAME}}$suffix.$ext" ]; then
                    MARQUEE_PATH="$ROM_DIR/$prefix${{GAME_BASENAME}}$suffix.$ext"
                    break 3
                fi
            done
        done
    done
    
    if [ -n "$MARQUEE_PATH" ]; then
        curl -s -X POST -F "image=@$MARQUEE_PATH" http://$BROKER:8080/api/marquee > /dev/null &
    else
        STATUS="playing"
        if [ "$ACTION" = "gameSelected" ]; then STATUS="browsing"; fi
        mosquitto_pub -h "$BROKER" -t "$TOPIC" -m "{{\"status\": \"$STATUS\", \"game\": \"$GAME_BASENAME\", \"system\": \"$SYSTEM_NAME\"}}" &
    fi
elif [ "$ACTION" = "gameStop" ]; then
    mosquitto_pub -h "$BROKER" -t "$TOPIC" -m "{{\"status\": \"stopped\"}}" &
fi
"#,
            matrix_ip
        );

        let script_path = format!("{}/arcadematrix_mqtt.sh", target_dir);

        tracing::info!("Creating directory {}...", target_dir);
        let mut channel = sess.channel_session().unwrap();
        channel.exec(&format!("mkdir -p {}", target_dir)).ok();
        channel.wait_close().ok();

        tracing::info!("Uploading script to {}...", script_path);
        let mut channel = sess.channel_session().unwrap();
        channel
            .exec(&format!(
                "cat > {} << 'EOF'\n{}\nEOF\nchmod +x {}",
                script_path, script_content, script_path
            ))
            .ok();
        channel.wait_close().ok();
    } else {
        // Recalbox Daemon — ultra-lightweight, zero image processing
        let daemon_code = format!(
            r#"import subprocess
import time
import os

BROKER = "{}"
TOPIC = "recalbox/system/playing"

def parse_statefile():
    game = None
    system = None
    image = None
    state = "browsing"
    try:
        with open("/tmp/es_state.inf", "r") as f:
            for line in f:
                if line.startswith("GamePath="):
                    game = line.split("=", 1)[1].strip()
                elif line.startswith("SystemId="):
                    system = line.split("=", 1)[1].strip()
                elif line.startswith("ImagePath="):
                    image = line.split("=", 1)[1].strip()
                elif line.startswith("State="):
                    state = line.split("=", 1)[1].strip()
    except Exception:
        pass
    return game, system, image, state

def main():
    import socket
    import sys
    lock_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        lock_socket.bind(("127.0.0.1", 49132))
    except socket.error:
        print("Another daemon is already running, exiting...")
        sys.exit(1)
        
    print("Daemon started (v5 - lightweight)!", flush=True)
    time.sleep(5)
    last_game = None
    last_sent = None
    pending_since = 0

    while True:
        try:
            rom_path, system, img, state = parse_statefile()
            if not rom_path:
                time.sleep(0.1)
                continue

            if rom_path != last_game:
                last_game = rom_path
                pending_since = time.time()

            elapsed = time.time() - pending_since
            if elapsed >= 0.15 and rom_path != last_sent:
                last_sent = rom_path
                gbase = os.path.splitext(os.path.basename(rom_path))[0]

                msg = '{{"status": "' + state + '", "game": "' + gbase + '", "system": "' + str(system) + '"}}'
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

        let launcher_code = r#"#!/bin/sh
if [ -z "$1" ] || [ "$1" = "-action" -a "$2" = "start" ]; then
    pkill -f arcadematrix_daemon.py || true
    python3 /recalbox/share/arcadematrix_daemon.py > /recalbox/share/userscripts/daemon.log 2>&1 &
fi
"#;

        tracing::info!("Creating directory {}...", target_dir);
        let mut channel = sess.channel_session().unwrap();
        channel.exec(&format!("mkdir -p {}", target_dir)).ok();
        channel.wait_close().ok();

        tracing::info!("Cleaning up ALL legacy scripts...");
        let mut channel = sess.channel_session().unwrap();
        channel.exec(&format!(
            "cd {} && for f in *.sh; do case \"$f\" in 'arcadematrix_launcher(permanent).sh') ;; *) rm -f \"$f\" ;; esac; done; rm -f /recalbox/share/arcadematrix_daemon.py; pkill -f recalbox_mqtt_status || true; pkill -f arcadematrix_mqtt || true; pkill -f arcadematrix_daemon.py || true",
            target_dir
        )).ok();
        channel.wait_close().ok();

        let daemon_path = "/recalbox/share/arcadematrix_daemon.py";
        let mut channel = sess.channel_session().unwrap();
        channel
            .exec(&format!(
                "cat > {} << 'EOF'\n{}\nEOF\n",
                daemon_path, daemon_code
            ))
            .ok();
        channel.wait_close().ok();

        let launcher_path = format!("{}/arcadematrix_launcher(permanent).sh", target_dir);
        let mut channel = sess.channel_session().unwrap();
        channel
            .exec(&format!(
                "cat > {} << 'EOF'\n{}\nEOF\nchmod +x {}",
                launcher_path, launcher_code, launcher_path
            ))
            .ok();
        channel.wait_close().ok();

        let mut channel = sess.channel_session().unwrap();
        channel
            .exec(&format!("rm -f {}/arcadematrix_mqtt.sh", target_dir))
            .ok();
        channel.wait_close().ok();
    }

    tracing::info!("Rebooting target system...");
    let mut channel = sess.channel_session().unwrap();
    channel.exec("sleep 1 && reboot").ok();
    channel.wait_close().ok();

    Ok(format!(
        "Successfully installed! {} is now rebooting...",
        system_name
    ))
}

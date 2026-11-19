#!/usr/bin/env bash
# ArcadeMatrix Gaming OS daemon installer - run this on your PC (macOS/Linux), NOT on the
# retro-gaming device itself. It connects over SSH, auto-detects (or prompts for) whether
# the target is Recalbox, Batocera, or RetroPie, uploads the right daemon/hook script
# (with your ArcadeMatrix device's IP and topic baked in), and starts the target live so
# it immediately sends game events, arcade high scores, and RetroAchievements over MQTT.
#
# Requirements: a standard OpenSSH client (ssh), present by default on macOS and virtually all
# Linux distros. `sshpass` is optional.
set -uo pipefail

RECALBOX_PASS="recalboxroot"
BATOCERA_PASS="linux"
RETROPIE_PASS="raspberry"
SSH_MUX_SOCK="/tmp/am_ssh_$$"
SSH_OPTS=(-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o GlobalKnownHostsFile=/dev/null -o CheckHostIP=no -o LogLevel=ERROR -o ConnectTimeout=6)

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

normalize_host_input() {
    local val="${1:-}"
    # Strip surrounding whitespace and quotes
    val="${val#"${val%%[![:space:]]*}"}"
    val="${val%"${val##*[![:space:]]}"}"
    val="${val#\"}"
    val="${val%\"}"
    val="${val#\'}"
    val="${val%\'}"
    # Strip http:// or https:// scheme
    val="${val#http://}"
    val="${val#https://}"
    # Strip any trailing /path
    val="${val%%/*}"
    # Strip :port for IPv4 / hostnames (preserve IPv6 with multiple colons)
    case "$val" in
        *:*:*) ;;
        *:[0-9]*)
            val="${val%:*}"
            ;;
    esac
    val="${val#"${val%%[![:space:]]*}"}"
    val="${val%"${val##*[![:space:]]}"}"
    printf '%s' "$val"
}

assert_safe_host() {
    local val="$1" label="$2"
    if [ -z "$val" ]; then
        echo "ERROR: $label is required. Aborting." >&2
        exit 1
    fi
    if ! printf '%s' "$val" | grep -Eq '^[][A-Za-z0-9][][A-Za-z0-9._:-]*$'; then
        echo "ERROR: $label contains invalid characters: '$val'. Provide a valid IP address or hostname." >&2
        exit 1
    fi
}

render_template() {
    local src="$1" dst="$2" dmd_ip="$3" topic="$4"
    if [ ! -f "$src" ]; then
        echo "ERROR: Required template file not found: $src" >&2
        exit 1
    fi
    if command -v python3 >/dev/null 2>&1; then
        python3 - "$src" "$dst" "$dmd_ip" "$topic" << 'PYEOF' || { echo "ERROR: Failed rendering template $src" >&2; exit 1; }
import sys
src_p, dst_p, dmd_ip, topic = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
with open(src_p, "rb") as f:
    raw = f.read().decode("utf-8")
if raw.startswith("\ufeff"):
    raw = raw[1:]
raw = raw.replace("{{DMD_HOST}}", dmd_ip).replace("{{BROKER}}", dmd_ip).replace("{{TOPIC}}", topic)
raw = raw.replace("\r\n", "\n").replace("\r", "\n")
with open(dst_p, "wb") as f:
    f.write(raw.encode("utf-8"))
PYEOF
    else
        tr -d '\r' < "$src" | sed -e "s|{{DMD_HOST}}|$dmd_ip|g" -e "s|{{BROKER}}|$dmd_ip|g" -e "s|{{TOPIC}}|$topic|g" > "$dst" || {
            echo "ERROR: Failed rendering template $src" >&2
            exit 1
        }
    fi
    if [ ! -s "$dst" ]; then
        echo "ERROR: Rendered template $dst is empty!" >&2
        exit 1
    fi
}

echo "=============================================="
echo " ArcadeMatrix Gaming OS Daemon Installer"
echo " (Recalbox / Batocera / RetroPie)"
echo "=============================================="
echo

read -rp "IP address of your Console (Recalbox/Batocera/RetroPie): " RAW_TARGET_IP
TARGET_IP="$(normalize_host_input "$RAW_TARGET_IP")"
assert_safe_host "$TARGET_IP" "Target Console IP address"

read -rp "Action (1: Install Daemon, 2: Check Logs) [1]: " ACTION
ACTION="${ACTION:-1}"
ACTION="$(printf '%s' "$ACTION" | tr -d '[:space:]')"
if [ "$ACTION" != "1" ] && [ "$ACTION" != "2" ]; then
    echo "ERROR: Invalid action '$ACTION'. Expected 1 (Install Daemon) or 2 (Check Logs)." >&2
    exit 1
fi

DMD_IP=""
if [ "$ACTION" = "1" ]; then
    read -rp "IP address of your ArcadeMatrix DMD (ESP32 or Raspberry Pi): " RAW_DMD_IP
    DMD_IP="$(normalize_host_input "$RAW_DMD_IP")"
    assert_safe_host "$DMD_IP" "ArcadeMatrix DMD IP address"
fi

echo
echo "Select Target Console OS:"
echo "  1) Auto-Detect"
echo "  2) Recalbox"
echo "  3) Batocera"
echo "  4) RetroPie"
read -rp "Choice [1]: " OS_CHOICE
OS_CHOICE="${OS_CHOICE:-1}"
OS_CHOICE="$(printf '%s' "$OS_CHOICE" | tr -d '[:space:]')"
case "$OS_CHOICE" in
    1|2|3|4) ;;
    *)
        echo "ERROR: Invalid OS choice '$OS_CHOICE'. Expected 1, 2, 3, or 4." >&2
        exit 1
        ;;
esac

read -rp "Custom SSH Username (leave blank for defaults): " CUSTOM_USER
CUSTOM_USER="$(printf '%s' "$CUSTOM_USER" | tr -d '[:space:]')"
if [ -n "$CUSTOM_USER" ] && ! printf '%s' "$CUSTOM_USER" | grep -Eq '^[A-Za-z0-9_][A-Za-z0-9._-]*$'; then
    echo "ERROR: Custom SSH Username contains invalid characters: '$CUSTOM_USER'." >&2
    exit 1
fi
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
    if [ "$have_sshpass" = "1" ] && [ -n "$password" ]; then
        sshpass -p "$password" ssh "${SSH_OPTS[@]}" "${user}@${TARGET_IP}" "$@"
    else
        ssh "${SSH_OPTS[@]}" "${user}@${TARGET_IP}" "$@"
    fi
}

scp_run() {
    local user="$1" password="$2" src="$3" dst="${4#\'}"
    dst="${dst%\'}"
    if [ "$have_sshpass" = "1" ] && [ -n "$password" ]; then
        tr -d '\r' < "$src" | sshpass -p "$password" ssh "${SSH_OPTS[@]}" "${user}@${TARGET_IP}" "cat > '${dst}.amtmp.\$\$' && mv -f '${dst}.amtmp.\$\$' '${dst}'"
    else
        tr -d '\r' < "$src" | ssh "${SSH_OPTS[@]}" "${user}@${TARGET_IP}" "cat > '${dst}.amtmp.\$\$' && mv -f '${dst}.amtmp.\$\$' '${dst}'"
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

if [ -n "$CUSTOM_USER" ] || [ -n "$CUSTOM_PASS" ]; then
    if [ -n "$CUSTOM_USER" ]; then
        ACTIVE_USER="$CUSTOM_USER"
    elif [ "$SYSTEM" = "retropie" ]; then
        ACTIVE_USER="pi"
    else
        ACTIVE_USER="root"
    fi
    if [ -n "$CUSTOM_PASS" ]; then
        PASSWORD="$CUSTOM_PASS"
    elif [ "$SYSTEM" = "retropie" ]; then
        PASSWORD="$RETROPIE_PASS"
    elif [ "$SYSTEM" = "batocera" ]; then
        PASSWORD="$BATOCERA_PASS"
    else
        PASSWORD="$RECALBOX_PASS"
    fi
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
        echo "Trying Recalbox / Batocera / RetroPie defaults..."
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
echo "Verifying SSH session to ${ACTIVE_USER}@${TARGET_IP}..."
SSH_AUTH_OK=0
for _try in 1 2 3; do
    if ssh_run "$ACTIVE_USER" "$PASSWORD" "true"; then
        SSH_AUTH_OK=1
        break
    fi
    if [ "$have_sshpass" = "1" ] && [ -n "$PASSWORD" ]; then
        break
    fi
    echo "SSH authentication failed (attempt $_try/3). Please try again." >&2
done
if [ "$SSH_AUTH_OK" != "1" ]; then
    echo "ERROR: Unable to authenticate over SSH to ${ACTIVE_USER}@${TARGET_IP}. Aborting." >&2
    exit 1
fi

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
render_template "$SCRIPT_DIR/arcadematrix_daemon.py" "$TMP_DIR/arcadematrix_daemon.py" "$DMD_IP" "$TOPIC"
render_template "$SCRIPT_DIR/arcadematrix_companion_sync.py" "$TMP_DIR/arcadematrix_companion_sync.py" "$DMD_IP" "$TOPIC"

echo "Checking DMD target ($DMD_IP) from console ($TARGET_IP)..."
ssh_run "$ACTIVE_USER" "$PASSWORD" "
    _bt=\$(curl -s -m 2 'http://$DMD_IP/api/mqtt/broker-target' 2>/dev/null || wget -qO- -T 2 'http://$DMD_IP/api/mqtt/broker-target' 2>/dev/null || true)
    if [ -n \"\$_bt\" ]; then
        echo \"  DMD /api/mqtt/broker-target -> \$_bt\"
    else
        echo \"  WARNING: Could not reach http://$DMD_IP/api/mqtt/broker-target from console!\"
    fi
" || true

if [ "$SYSTEM" = "recalbox" ]; then
    TARGET_DIR="/recalbox/share/userscripts"
    WAKEUP_HOOK="arcadematrix_wakeup[start,systembrowsing,rungame,endgame,rundemo,enddemo,wakeup].sh"
    echo "Cleaning up any previous install..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" '
        _pfx="/tmp/arcadematrix_"
        touch "${_pfx}daemon.stop"
        for _pf in "${_pfx}supervisor.pid" "${_pfx}daemon.pid"; do
            if [ -f "$_pf" ]; then
                _kp=$(cat "$_pf" 2>/dev/null || true)
                if [ -n "$_kp" ] && [ "$_kp" != "$$" ] && [ "$_kp" != "$PPID" ]; then
                    kill -9 "$_kp" 2>/dev/null || true
                fi
            fi
        done
        for _pd in /proc/[0-9]*; do
            _p=${_pd#/proc/}
            [ "$_p" = "1" ] || [ "$_p" = "$$" ] || [ "$_p" = "$PPID" ] && continue
            [ -r "$_pd/cmdline" ] || continue
            _a0=$(tr "\0" "\n" < "$_pd/cmdline" 2>/dev/null | sed -n "1p")
            _cl=$(tr "\0" " " < "$_pd/cmdline" 2>/dev/null || true)
            case "${_a0##*/}" in
                python*)
                    case "$_cl" in
                        *arcadematrix_[d]aemon.py*|*arcadematrix_[c]ompanion_sync.py*)
                            kill -9 "$_p" 2>/dev/null || true
                            ;;
                    esac
                    ;;
                sh|ash|dash|bash)
                    case "$_cl" in
                        *arcadematrix_[l]auncher*--supervisor-loop*|*arcadematrix_[s]upervisor.pid*|*arcadematrix_[m]qtt.sh*)
                            kill -9 "$_p" 2>/dev/null || true
                            ;;
                    esac
                    ;;
            esac
        done
        rm -f "${_pfx}daemon.pid" "${_pfx}supervisor.pid" "${_pfx}daemon.lock" "/recalbox/share/userscripts/arcadematrix_"mqtt.sh || true
        mkdir -p /recalbox/share/userscripts /recalbox/share/system /recalbox/share/roms
    ' || { echo "ERROR: Cleanup step failed on Recalbox!" >&2; exit 1; }

    echo "Uploading daemon, companion sync, watchdog & ES self-healer (topic: $TOPIC)..."
    scp_run "$ACTIVE_USER" "$PASSWORD" "$TMP_DIR/arcadematrix_daemon.py" "/recalbox/share/arcadematrix_daemon.py" || { echo "SCP failed!"; exit 1; }
    scp_run "$ACTIVE_USER" "$PASSWORD" "$TMP_DIR/arcadematrix_companion_sync.py" "/recalbox/share/arcadematrix_companion_sync.py" || { echo "SCP failed!"; exit 1; }
    scp_run "$ACTIVE_USER" "$PASSWORD" "$SCRIPT_DIR/arcadematrix_launcher(permanent).sh" "$TARGET_DIR/arcadematrix_launcher(permanent).sh" || { echo "SCP failed!"; exit 1; }
    scp_run "$ACTIVE_USER" "$PASSWORD" "$SCRIPT_DIR/$WAKEUP_HOOK" "$TARGET_DIR/$WAKEUP_HOOK" || { echo "SCP failed!"; exit 1; }

    echo "Configuring Recalbox boot persistence, RAM RetroArch RCHEEVOS logging & starting daemon live..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" "
        chmod 755 /recalbox/share/arcadematrix_daemon.py /recalbox/share/arcadematrix_companion_sync.py '$TARGET_DIR/arcadematrix_launcher(permanent).sh' '$TARGET_DIR/$WAKEUP_HOOK' 2>/dev/null || true
        _ra_ovr='/recalbox/share/roms/.retroarch.cfg'
        touch \"\$_ra_ovr\" 2>/dev/null || true
        if [ -w \"\$_ra_ovr\" ]; then
            for _kv in \
                'log_verbosity = \"true\"' \
                'log_to_file = \"true\"' \
                'log_to_file_timestamp = \"false\"' \
                'log_dir = \"/tmp\"' \
                'frontend_log_level = \"1\"' \
                'libretro_log_level = \"2\"'; do
                _k=\${_kv%% *}
                if ! grep -Eq \"^[[:space:]]*\${_k}[[:space:]]*=\" \"\$_ra_ovr\" 2>/dev/null; then
                    printf '%s\n' \"\$_kv\" >> \"\$_ra_ovr\"
                fi
            done
        fi
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
        sleep 0.9
        echo 'Active Recalbox processes:'
        _dpid=\$(cat /tmp/arcadematrix_daemon.pid 2>/dev/null || true)
        _spid=\$(cat /tmp/arcadematrix_supervisor.pid 2>/dev/null || true)
        if [ -n \"\$_dpid\" ] && kill -0 \"\$_dpid\" 2>/dev/null; then
            echo \"  Daemon PID \$_dpid: \$(tr '\\0' ' ' < /proc/\$_dpid/cmdline 2>/dev/null)\"
        else
            echo '  ERROR: Daemon PID not running!' >&2
        fi
        if [ -n \"\$_spid\" ] && kill -0 \"\$_spid\" 2>/dev/null; then
            echo \"  Supervisor PID \$_spid: \$(tr '\\0' ' ' < /proc/\$_spid/cmdline 2>/dev/null)\"
        fi
        echo 'Daemon log:'
        cat '$TARGET_DIR/daemon.log' 2>/dev/null || true
        [ -n \"\$_dpid\" ] && kill -0 \"\$_dpid\" 2>/dev/null
    " || { echo "ERROR: ArcadeMatrix daemon failed to start on Recalbox!" >&2; exit 1; }

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
    render_template "$SCRIPT_DIR/arcadematrix_mqtt_batocera.sh" "$TMP_DIR/arcadematrix_mqtt.sh" "$DMD_IP" "$TOPIC"

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

    echo "Configuring EmulationStation UI hooks (game-selected, system-selected, game-start, game-end)..."
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
    echo "Cleaning up any previous RetroPie install..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" '
        _pfx="/tmp/arcadematrix_"
        touch "${_pfx}daemon.stop"
        for _pf in "${_pfx}supervisor.pid" "${_pfx}daemon.pid"; do
            if [ -f "$_pf" ]; then
                _kp=$(cat "$_pf" 2>/dev/null || true)
                if [ -n "$_kp" ] && [ "$_kp" != "$$" ] && [ "$_kp" != "$PPID" ]; then
                    kill -9 "$_kp" 2>/dev/null || true
                fi
            fi
        done
        for _pd in /proc/[0-9]*; do
            _p=${_pd#/proc/}
            [ "$_p" = "1" ] || [ "$_p" = "$$" ] || [ "$_p" = "$PPID" ] && continue
            [ -r "$_pd/cmdline" ] || continue
            _a0=$(tr "\0" "\n" < "$_pd/cmdline" 2>/dev/null | sed -n "1p")
            _cl=$(tr "\0" " " < "$_pd/cmdline" 2>/dev/null || true)
            case "${_a0##*/}" in
                python*)
                    case "$_cl" in
                        *arcadematrix_[d]aemon.py*|*arcadematrix_[c]ompanion_sync.py*)
                            kill -9 "$_p" 2>/dev/null || true
                            ;;
                    esac
                    ;;
                sh|ash|dash|bash)
                    case "$_cl" in
                        *arcadematrix_[l]auncher*--supervisor-loop*|*arcadematrix_[s]upervisor.pid*)
                            kill -9 "$_p" 2>/dev/null || true
                            ;;
                    esac
                    ;;
            esac
        done
        rm -f "${_pfx}daemon.pid" "${_pfx}supervisor.pid" "${_pfx}daemon.lock" || true
        mkdir -p /opt/retropie/configs/all/emulationstation/scripts/system-select \
                 /opt/retropie/configs/all/emulationstation/scripts/game-select
    ' || { echo "ERROR: Cleanup step failed on RetroPie!" >&2; exit 1; }

    echo "Uploading daemon, watchdog & companion sync (topic: $TOPIC)..."
    scp_run "$ACTIVE_USER" "$PASSWORD" "$TMP_DIR/arcadematrix_daemon.py" "/opt/retropie/configs/all/arcadematrix_daemon.py" || { echo "SCP failed!"; exit 1; }
    scp_run "$ACTIVE_USER" "$PASSWORD" "$TMP_DIR/arcadematrix_companion_sync.py" "/opt/retropie/configs/all/arcadematrix_companion_sync.py" || { echo "SCP failed!"; exit 1; }
    scp_run "$ACTIVE_USER" "$PASSWORD" "$SCRIPT_DIR/arcadematrix_launcher(permanent).sh" "/opt/retropie/configs/all/arcadematrix_launcher.sh" || { echo "SCP failed!"; exit 1; }

    echo "Configuring RetroPie EmulationStation browsing hooks, runcommand hooks & watchdog..."
    ssh_run "$ACTIVE_USER" "$PASSWORD" '
        chmod 755 /opt/retropie/configs/all/arcadematrix_daemon.py \
                  /opt/retropie/configs/all/arcadematrix_companion_sync.py \
                  /opt/retropie/configs/all/arcadematrix_launcher.sh 2>/dev/null || true

        cat > /opt/retropie/configs/all/emulationstation/scripts/system-select/arcadematrix_es.sh << '\''ESEOF'\''
#!/bin/sh
[ -z "$1" ] || [ "$1" = "NULL" ] && exit 0
_tmp="/tmp/es_state.inf.tmp.$$"
printf "State=browsing\nAction=systembrowsing\nSystemId=%s\nGamePath=\nGame=\n" "$1" > "$_tmp" && mv -f "$_tmp" /tmp/es_state.inf
ESEOF
        chmod 755 /opt/retropie/configs/all/emulationstation/scripts/system-select/arcadematrix_es.sh

        cat > /opt/retropie/configs/all/emulationstation/scripts/game-select/arcadematrix_es.sh << '\''ESEOF'\''
#!/bin/sh
[ -z "$1" ] || [ "$1" = "NULL" ] && exit 0
_tmp="/tmp/es_state.inf.tmp.$$"
if [ -z "$2" ] || [ "$2" = "NULL" ]; then
    printf "State=browsing\nAction=systembrowsing\nSystemId=%s\nGamePath=\nGame=\n" "$1" > "$_tmp" && mv -f "$_tmp" /tmp/es_state.inf
else
    printf "State=browsing\nAction=gamelistbrowsing\nSystemId=%s\nGamePath=%s\nGame=%s\n" "$1" "$2" "$3" > "$_tmp" && mv -f "$_tmp" /tmp/es_state.inf
fi
ESEOF
        chmod 755 /opt/retropie/configs/all/emulationstation/scripts/game-select/arcadematrix_es.sh

        if [ -d "$HOME/.emulationstation" ] && [ ! -L "$HOME/.emulationstation" ]; then
            mkdir -p "$HOME/.emulationstation/scripts/system-select" "$HOME/.emulationstation/scripts/game-select"
            cp -f /opt/retropie/configs/all/emulationstation/scripts/system-select/arcadematrix_es.sh "$HOME/.emulationstation/scripts/system-select/arcadematrix_es.sh"
            cp -f /opt/retropie/configs/all/emulationstation/scripts/game-select/arcadematrix_es.sh "$HOME/.emulationstation/scripts/game-select/arcadematrix_es.sh"
            chmod 755 "$HOME/.emulationstation/scripts/system-select/arcadematrix_es.sh" "$HOME/.emulationstation/scripts/game-select/arcadematrix_es.sh"
        fi

        touch /opt/retropie/configs/all/runcommand-onstart.sh
        sed -i "/# ArcadeMatrix Runcommand Hook/,/STATEEOF/d" /opt/retropie/configs/all/runcommand-onstart.sh 2>/dev/null || true
        sed -i "/# ArcadeMatrix Runcommand Hook/,/# End ArcadeMatrix Hook/d" /opt/retropie/configs/all/runcommand-onstart.sh 2>/dev/null || true
        cat >> /opt/retropie/configs/all/runcommand-onstart.sh << '\''RCEOF'\''
# ArcadeMatrix Runcommand Hook
_am_tmp="/tmp/es_state.inf.tmp.$$"
printf "State=playing\nAction=rungame\nSystemId=%s\nGamePath=%s\n" "$1" "$3" > "$_am_tmp" && mv -f "$_am_tmp" /tmp/es_state.inf
# End ArcadeMatrix Hook
RCEOF
        chmod +x /opt/retropie/configs/all/runcommand-onstart.sh

        touch /opt/retropie/configs/all/runcommand-onend.sh
        sed -i "/# ArcadeMatrix Runcommand Hook/,/STATEEOF/d" /opt/retropie/configs/all/runcommand-onend.sh 2>/dev/null || true
        sed -i "/# ArcadeMatrix Runcommand Hook/,/# End ArcadeMatrix Hook/d" /opt/retropie/configs/all/runcommand-onend.sh 2>/dev/null || true
        cat >> /opt/retropie/configs/all/runcommand-onend.sh << '\''RCEOF'\''
# ArcadeMatrix Runcommand Hook
_am_tmp="/tmp/es_state.inf.tmp.$$"
printf "State=stopped\nAction=endgame\nSystemId=\nGamePath=\n" > "$_am_tmp" && mv -f "$_am_tmp" /tmp/es_state.inf
# End ArcadeMatrix Hook
RCEOF
        chmod +x /opt/retropie/configs/all/runcommand-onend.sh

        if [ -f /opt/retropie/configs/all/autostart.sh ]; then
            sed -i "/arcadematrix_daemon\.py/d;/arcadematrix_launcher\.sh/d" /opt/retropie/configs/all/autostart.sh 2>/dev/null || true
            if grep -q "emulationstation" /opt/retropie/configs/all/autostart.sh; then
                sed -i "/emulationstation/i sh /opt/retropie/configs/all/arcadematrix_launcher.sh -action start >/dev/null 2>&1 &" /opt/retropie/configs/all/autostart.sh
            else
                printf "\nsh /opt/retropie/configs/all/arcadematrix_launcher.sh -action start >/dev/null 2>&1 &\n" >> /opt/retropie/configs/all/autostart.sh
            fi
        fi

        rm -f /tmp/arcadematrix_daemon.stop
        : > /opt/retropie/configs/all/daemon.log
        sh /opt/retropie/configs/all/arcadematrix_launcher.sh -action start
        sync
        sleep 0.8
        _dpid=$(cat /tmp/arcadematrix_daemon.pid 2>/dev/null || true)
        if [ -n "$_dpid" ] && kill -0 "$_dpid" 2>/dev/null; then
            echo "  RetroPie Daemon PID $_dpid active."
        else
            echo "  ERROR: RetroPie Daemon PID not running!" >&2
            exit 1
        fi
    ' || { echo "ERROR: ArcadeMatrix daemon failed to start on RetroPie!" >&2; exit 1; }
fi

echo
echo "Installation complete on $SYSTEM (active immediately, no reboot required)!"

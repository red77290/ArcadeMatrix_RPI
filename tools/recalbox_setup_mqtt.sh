#!/bin/bash
# ==============================================================================
# ArcadeMatrix - Recalbox MQTT Installer (A EXÉCUTER SUR LA RECALBOX DIRECTEMENT)
# ==============================================================================

# Mets ici l'adresse IP de ton Raspberry Pi (celui avec la matrice LED)
MQTT_BROKER="192.168.1.169"

TARGET_DIR="/recalbox/share/userscripts"
DAEMON_FILE="/recalbox/share/arcadematrix_daemon.py"
LAUNCHER_FILE="$TARGET_DIR/arcadematrix_launcher(permanent).sh"

echo "Configuration du daemon ArcadeMatrix sur la Recalbox..."

# Nettoyage des vieux scripts qui spammaient
echo "Nettoyage des anciens scripts..."
pkill -f arcadematrix_daemon.py || true
pkill -f arcadematrix_mqtt.sh || true
rm -f "$TARGET_DIR"/arcadematrix_mqtt.sh
rm -f "$TARGET_DIR"/arcadematrix_daemon.py

mkdir -p "$TARGET_DIR"

# 1. Création du Daemon Python
echo "Création du daemon Python dans $DAEMON_FILE..."
cat << 'PYEOF' > "$DAEMON_FILE"
import subprocess
import time
import os

BROKER = "MQTT_BROKER_IP_PLACEHOLDER"
TOPIC = "recalbox/system/playing"

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

def main():
    import socket
    import sys
    # Single instance lock: empêche les clones fantômes
    lock_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        lock_socket.bind(("127.0.0.1", 49132))
    except socket.error:
        print("Daemon is already running, exiting...")
        sys.exit(1)
        
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

            # Debounce: on attend 150ms de survol avant d'envoyer (anti-spam)
            elapsed = time.time() - pending_since
            if elapsed >= 0.15 and current_key != last_sent_key:
                last_sent_key = current_key

                if current_key[2] == "stopped":
                    msg = '{"status": "stopped"}'
                elif current_key[0] is None:
                    # System browsing
                    msg = '{"status": "browsing", "system": "' + str(current_key[1]) + '", "type": "system"}'
                else:
                    # Game event
                    gbase = os.path.splitext(os.path.basename(current_key[0]))[0]
                    msg = '{"status": "' + current_key[2] + '", "game": "' + gbase + '", "system": "' + str(current_key[1]) + '"}'

                try:
                    subprocess.run(["mosquitto_pub", "-h", BROKER, "-t", TOPIC, "-m", msg], timeout=2, check=False)
                except subprocess.TimeoutExpired:
                    pass
        except Exception as e:
            print("Error: " + str(e), flush=True)

        time.sleep(0.1)

if __name__ == "__main__":
    main()
PYEOF

# Remplacement de l'IP
sed -i "s/MQTT_BROKER_IP_PLACEHOLDER/$MQTT_BROKER/g" "$DAEMON_FILE"

# 2. Création du Launcher Bash (qui démarre avec EmulationStation)
echo "Création du script de démarrage dans $LAUNCHER_FILE..."
cat << 'SHEOF' > "$LAUNCHER_FILE"
#!/bin/sh
# Lance le daemon au démarrage d'EmulationStation
if [ -z "$1" ] || [ "$1" = "-action" -a "$2" = "start" ]; then
    pkill -f arcadematrix_daemon.py || true
    python3 /recalbox/share/arcadematrix_daemon.py > /recalbox/share/userscripts/daemon.log 2>&1 &
fi
SHEOF

chmod +x "$LAUNCHER_FILE"

echo "=============================================================================="
echo "SUCCÈS ! Le daemon ArcadeMatrix a été installé."
echo "Un redémarrage est nécessaire. Tape 'reboot' pour l'appliquer."
echo "=============================================================================="

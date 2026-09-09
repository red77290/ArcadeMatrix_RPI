"""
ArcadeMatrix Recalbox event daemon (lightweight, zero image processing).

Runs directly ON the Recalbox device (not the ESP32/RPi). Watches EmulationStation's
/tmp/es_state.inf state file and publishes a compact JSON event over MQTT whenever the
selected/playing game changes, in the exact format both ArcadeMatrix projects expect:

    {"status": "browsing"|"playing"|"stopped", "game": "<rom basename, no extension>", "system": "<SystemId>"}

- ArcadeMatrix_RPi subscribes to this on `recalbox/system/playing` (see main.py) and looks up/
  downloads the matching Pixelcade marquee image live.
- ArcadeMatrix (ESP32) subscribes to the same topic (see src/engines/RetroFrontendListener.cpp)
  and looks up a pre-synced Pixelcade image on its own SD card (see tools/pixelcade_sync/) - no
  network access needed on the ESP32 at all.

This file is a *template* - {{BROKER}} is substituted by install.sh/install.ps1 with the actual
IP address of your ArcadeMatrix device (ESP32 or Raspberry Pi) before being uploaded. If you are
installing manually over SSH instead of using the installer scripts, replace {{BROKER}} yourself
before copying this file to the device.
"""
import subprocess
import time
import os
import json

BROKER = "{{BROKER}}"
TOPIC = "{{TOPIC}}"


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
    s_lower = s_clean.lower().replace("_", " ").replace("-", " ")
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
            remainder = s_clean[len(p):]
            if remainder.startswith("_") or remainder.startswith("-"):
                remainder = remainder[1:]
            return remainder.strip()
    return s_clean


def main():
    import socket
    import sys
    # Single instance lock: guarantee only one daemon runs at a time, even if EmulationStation
    # (re)starts this launcher script multiple times in a row.
    lock_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        lock_socket.bind(("127.0.0.1", 49132))
    except socket.error:
        print("Another daemon is already running, exiting...")
        sys.exit(1)

    print("ArcadeMatrix daemon started (lightweight)!", flush=True)
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
                    msg = '{"status": "stopped"}'
                elif current_key[0] is None:
                    msg = json.dumps({"status": "browsing", "system": str(current_key[1]), "type": "system"})
                else:
                    gbase = os.path.splitext(os.path.basename(current_key[0]))[0]
                    gbase = clean_system_name(gbase)
                    msg = json.dumps({"status": current_key[2], "game": gbase, "system": str(current_key[1])})

                try:
                    subprocess.run(["mosquitto_pub", "-h", BROKER, "-t", TOPIC, "-m", msg], timeout=2, check=False)
                except subprocess.TimeoutExpired:
                    pass
        except Exception as e:
            print("Error: " + str(e), flush=True)

        time.sleep(0.1)


if __name__ == "__main__":
    main()

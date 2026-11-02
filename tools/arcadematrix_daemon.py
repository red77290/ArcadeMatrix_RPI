"""
ArcadeMatrix Recalbox / RetroPie event daemon (lightweight, zero image processing).

Runs directly ON the gaming console (not the ESP32/RPi). Watches EmulationStation's
/tmp/es_state.inf state file and publishes a compact JSON event over MQTT whenever the
selected/playing game changes:

    {"status": "browsing"|"playing"|"stopped", "game": "<rom basename, no extension>", "system": "<SystemId>"}

Instead of hardcoding an MQTT broker IP at installation time, this daemon stores the DMD's own IP
(`DMD_HOST = "{{DMD_HOST}}"`) and dynamically resolves the active MQTT broker endpoint by querying
`GET http://<DMD_HOST>/api/mqtt/broker-target` (ports 80 and 8080, cached for 15s, falling back to
`<DMD_HOST>:1883` if unreachable). This allows seamless dynamic switching between the DMD's local
broker and a companion DMD Media Showcase broker with zero console reconfiguration.
"""
import json
import os
import socket
import subprocess
import sys
import time
import urllib.request

DMD_HOST = "{{DMD_HOST}}"
TOPIC = "{{TOPIC}}"
BROKER_CACHE_TTL_SEC = 15.0

_cached_broker_host = None
_cached_broker_port = 1883
_cached_broker_ts = 0.0


def resolve_broker():
    global _cached_broker_host, _cached_broker_port, _cached_broker_ts
    now = time.time()
    if _cached_broker_host and (now - _cached_broker_ts) < BROKER_CACHE_TTL_SEC:
        return _cached_broker_host, _cached_broker_port

    for http_port in (80, 8080):
        url = f"http://{DMD_HOST}:{http_port}/api/mqtt/broker-target"
        try:
            req = urllib.request.Request(url, headers={"Accept": "application/json"})
            with urllib.request.urlopen(req, timeout=0.8) as resp:
                if 200 <= resp.status < 300:
                    data = json.loads(resp.read().decode("utf-8", errors="ignore"))
                    host = str(data.get("host") or "").strip()
                    port = int(data.get("port") or 1883)
                    if host and 0 < port <= 65535:
                        _cached_broker_host = host
                        _cached_broker_port = port
                        _cached_broker_ts = now
                        return host, port
        except Exception:
            continue

    _cached_broker_host = DMD_HOST
    _cached_broker_port = 1883
    _cached_broker_ts = now
    return _cached_broker_host, _cached_broker_port


def publish_mqtt_raw(host, port, topic, payload):
    def enc_len(length):
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

    cid = f"am_sync_{os.getpid()}".encode("utf-8")
    c_body = b"\x00\x04MQTT\x04\x02\x00\x3c" + len(cid).to_bytes(2, "big") + cid
    c_pkt = b"\x10" + enc_len(len(c_body)) + c_body
    t_b = topic.encode("utf-8")
    m_b = payload.encode("utf-8")
    p_body = len(t_b).to_bytes(2, "big") + t_b + m_b
    p_pkt = b"\x30" + enc_len(len(p_body)) + p_body

    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.settimeout(1.5)
    try:
        sock.connect((host, int(port)))
        sock.sendall(c_pkt)
        sock.recv(4)
        sock.sendall(p_pkt)
        sock.sendall(b"\xe0\x00")
    finally:
        sock.close()


def publish_event(msg):
    broker_host, broker_port = resolve_broker()
    try:
        res = subprocess.run(
            ["mosquitto_pub", "-h", broker_host, "-p", str(broker_port), "-t", TOPIC, "-m", msg],
            timeout=2,
            check=False,
        )
        if res.returncode == 0:
            return
    except FileNotFoundError:
        pass
    except subprocess.TimeoutExpired:
        return
    except Exception:
        pass

    try:
        publish_mqtt_raw(broker_host, broker_port, TOPIC, msg)
    except Exception:
        pass


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
    # Single instance lock: guarantee only one daemon runs at a time, even if EmulationStation
    # (re)starts this launcher script multiple times in a row.
    lock_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        lock_socket.bind(("127.0.0.1", 49132))
    except socket.error:
        print("Another daemon is already running, exiting...")
        sys.exit(1)

    print(f"ArcadeMatrix daemon started (DMD_HOST={DMD_HOST}, dynamic broker discovery)!", flush=True)
    time.sleep(5)
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

                publish_event(msg)
        except Exception as e:
            print("Error: " + str(e), flush=True)

        time.sleep(0.1)


if __name__ == "__main__":
    main()

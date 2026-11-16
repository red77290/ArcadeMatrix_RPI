"""
ArcadeMatrix Recalbox / RetroPie event daemon (lightweight, zero image processing).

Runs directly ON the gaming console (not the ESP32/RPi). Watches EmulationStation's
/tmp/es_state.inf state file and publishes a compact JSON event over a persistent MQTT TCP
connection whenever the selected system or selected/playing game changes:

    {"status": "browsing", "system": "<SystemId>", "type": "system"}
    {"status": "browsing"|"playing", "game": "<rom basename>", "system": "<SystemId>", ...}
    {"status": "stopped"}

Instead of hardcoding an MQTT broker IP at installation time, this daemon stores the DMD's own IP
(`DMD_HOST = "{{DMD_HOST}}"`) and dynamically resolves the active MQTT broker endpoint by querying
`GET http://<DMD_HOST>/api/mqtt/broker-target` (ports 80 and 8080, cached for 15s, falling back to
`<DMD_HOST>:1883` if unreachable). This allows seamless dynamic switching between the DMD's local
broker and a companion DMD Media Showcase broker with zero console reconfiguration.
"""
import errno
import json
import os
import signal
import socket
import subprocess
import sys
import time
import urllib.request

DMD_HOST = "{{DMD_HOST}}"
TOPIC = "{{TOPIC}}"
BROKER_CACHE_TTL_SEC = 15.0

PID_FILE = "/tmp/arcadematrix_daemon.pid"
STOP_FILE = "/tmp/arcadematrix_daemon.stop"
DEFAULT_RECALBOX_LOG = "/recalbox/share/userscripts/daemon.log"
MAX_LOG_BYTES = 65536

_log_file_path = None
_cached_broker_host = None
_cached_broker_port = 1883
_cached_broker_ts = 0.0


def log_msg(msg):
    line = f"[{time.strftime('%H:%M:%S')}] {msg}\n"
    if _log_file_path:
        try:
            try:
                if os.path.getsize(_log_file_path) > MAX_LOG_BYTES:
                    with open(_log_file_path, "rb") as rf:
                        rf.seek(-MAX_LOG_BYTES // 2, os.SEEK_END)
                        tail = rf.read()
                    nl = tail.find(b"\n")
                    if nl >= 0:
                        tail = tail[nl + 1:]
                    with open(_log_file_path, "wb") as wf:
                        wf.write(tail)
            except Exception:
                pass
            with open(_log_file_path, "a", encoding="utf-8", errors="ignore") as f:
                f.write(line)
            return
        except Exception:
            pass
    try:
        sys.stdout.write(line)
        sys.stdout.flush()
    except Exception:
        pass


def _daemonize(log_path):
    """Classic UNIX double-fork so SSH exit or ES process-group cleanup never kills the daemon."""
    if not hasattr(os, "fork"):
        return
    try:
        pid = os.fork()
        if pid > 0:
            os._exit(0)
    except OSError:
        return

    try:
        os.setsid()
    except OSError:
        pass

    try:
        pid = os.fork()
        if pid > 0:
            os._exit(0)
    except OSError:
        pass

    try:
        os.chdir("/")
    except OSError:
        pass

    try:
        devnull_fd = os.open("/dev/null", os.O_RDWR)
        os.dup2(devnull_fd, 0)
        out_fd = devnull_fd
        if log_path:
            try:
                out_fd = os.open(log_path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o644)
            except OSError:
                out_fd = devnull_fd
        os.dup2(out_fd, 1)
        os.dup2(out_fd, 2)
        if out_fd > 2:
            os.close(out_fd)
        if devnull_fd > 2 and devnull_fd != out_fd:
            os.close(devnull_fd)
    except Exception:
        pass


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


def _mqtt_enc_len(length):
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


class PersistentMqttClient:
    """Persistent QoS-0 MQTT v3.1.1 TCP client (eliminates subprocess & LwIP socket churn on ESP32)."""

    KEEPALIVE_SEC = 60
    PING_INTERVAL_SEC = 20.0

    def __init__(self):
        self.sock = None
        self.host = None
        self.port = None
        self.last_io = 0.0

    def close(self):
        if self.sock is not None:
            try:
                self.sock.close()
            except Exception:
                pass
            self.sock = None

    def _drain_nonblocking(self):
        if self.sock is None:
            return False
        try:
            self.sock.setblocking(False)
            while True:
                try:
                    chunk = self.sock.recv(256)
                    if not chunk:
                        # Broker closed TCP connection cleanly (FIN)
                        self.close()
                        return False
                except BlockingIOError:
                    break
                except socket.error as e:
                    if e.errno in (errno.EAGAIN, errno.EWOULDBLOCK):
                        break
                    self.close()
                    return False
        except Exception:
            self.close()
            return False
        finally:
            if self.sock is not None:
                try:
                    self.sock.settimeout(1.5)
                except Exception:
                    pass
        return True

    def ensure_connected(self, host, port):
        port = int(port)
        if self.sock is not None and (self.host != host or self.port != port):
            self.close()

        if self.sock is not None and self._drain_nonblocking():
            return True

        self.close()
        cid = f"am_rcbx_{os.getpid()}".encode("utf-8")
        c_body = (
            b"\x00\x04MQTT\x04\x02"
            + self.KEEPALIVE_SEC.to_bytes(2, "big")
            + len(cid).to_bytes(2, "big")
            + cid
        )
        c_pkt = b"\x10" + _mqtt_enc_len(len(c_body)) + c_body

        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        try:
            if hasattr(sock, "set_inheritable"):
                sock.set_inheritable(False)
            sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_KEEPALIVE, 1)
        except Exception:
            pass
        sock.settimeout(1.5)
        try:
            sock.connect((host, port))
            sock.sendall(c_pkt)
            ack = sock.recv(4)
            if len(ack) < 4 or ack[0] != 0x20 or ack[3] != 0x00:
                sock.close()
                return False
            self.sock = sock
            self.host = host
            self.port = port
            self.last_io = time.time()
            return True
        except Exception:
            try:
                sock.close()
            except Exception:
                pass
            return False

    def publish(self, host, port, topic, payload):
        t_b = topic.encode("utf-8")
        m_b = payload.encode("utf-8")
        p_body = len(t_b).to_bytes(2, "big") + t_b + m_b
        p_pkt = b"\x30" + _mqtt_enc_len(len(p_body)) + p_body

        for _ in range(2):
            if not self.ensure_connected(host, port):
                continue
            try:
                self.sock.sendall(p_pkt)
                self.last_io = time.time()
                return True
            except Exception:
                self.close()
        return False

    def tick_keepalive(self, now):
        if self.sock is None:
            return
        if not self._drain_nonblocking():
            return
        if (now - self.last_io) >= self.PING_INTERVAL_SEC:
            try:
                self.sock.sendall(b"\xc0\x00")
                self.last_io = now
            except Exception:
                self.close()


_mqtt_client = PersistentMqttClient()


def publish_event(msg):
    global _cached_broker_ts
    broker_host, broker_port = resolve_broker()
    if _mqtt_client.publish(broker_host, broker_port, TOPIC, msg):
        return True

    # Fallback to mosquitto_pub if available
    try:
        res = subprocess.run(
            ["mosquitto_pub", "-h", broker_host, "-p", str(broker_port), "-t", TOPIC, "-m", msg],
            timeout=1.5,
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        if res.returncode == 0:
            return True
    except Exception:
        pass

    _cached_broker_ts = 0.0
    return False


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


def _usable_action_data_system(action_data):
    if not action_data:
        return ""
    ad = action_data.strip()
    if not ad or "/" in ad or "\\" in ad or ad.isdigit():
        return ""
    return clean_system_name(ad)


def parse_statefile():
    rom_path = None
    title = None
    sys_id = None
    sys_label = None
    action_data = ""
    state = "browsing"
    action = ""
    is_folder = ""
    try:
        with open("/tmp/es_state.inf", "r", encoding="utf-8", errors="ignore") as f:
            for line in f:
                if line.startswith("GamePath="):
                    rom_path = line.split("=", 1)[1].strip() or None
                elif line.startswith("Game="):
                    title = line.split("=", 1)[1].strip() or None
                elif line.startswith("SystemId="):
                    sys_id = line.split("=", 1)[1].strip() or None
                elif line.startswith("System="):
                    sys_label = line.split("=", 1)[1].strip() or None
                elif line.startswith("ActionData="):
                    action_data = line.split("=", 1)[1].strip()
                elif line.startswith("State="):
                    state = line.split("=", 1)[1].strip()
                elif line.startswith("Action="):
                    action = line.split("=", 1)[1].strip()
                elif line.startswith("IsFolder="):
                    is_folder = line.split("=", 1)[1].strip()
    except Exception:
        pass

    action_low = (action or "").strip().lower()
    if not rom_path and action_data and "/" in action_data and action_low in ("rungame", "rundemo", "gamelistbrowsing"):
        rom_path = action_data

    ad_sys = _usable_action_data_system(action_data)
    lbl_sys = clean_system_name(sys_label) if sys_label else ""

    if not sys_id:
        resolved_system = ad_sys or lbl_sys or None
    elif sys_id.lower() == "arcade":
        # Recalbox Virtual Arcade sub-systems (Capcom, CPS1, Toaplan, etc.) often report
        # SystemId=arcade while placing the specific sub-system in ActionData or System.
        if ad_sys and ad_sys.lower() not in ("arcade", "mame"):
            resolved_system = ad_sys
        elif lbl_sys and lbl_sys.lower() not in ("arcade", "mame", "virtual arcade", "arcade virtual"):
            resolved_system = lbl_sys
        else:
            resolved_system = sys_id
    else:
        resolved_system = sys_id

    return rom_path, title, resolved_system, state, action, is_folder


def spawn_companion_sync(action, system="", game="", rom_path="", title=""):
    script_dir = os.path.dirname(os.path.abspath(__file__))
    candidates = [
        os.path.join(script_dir, "arcadematrix_companion_sync.py"),
        "/recalbox/share/arcadematrix_companion_sync.py",
        "/opt/retropie/configs/all/arcadematrix_companion_sync.py",
        "/userdata/system/scripts/arcadematrix_companion_sync.py",
    ]
    comp_script = None
    for c in candidates:
        if os.path.isfile(c):
            comp_script = c
            break
    if not comp_script:
        return

    cmd = [
        sys.executable or "python3",
        comp_script,
        action,
        "--dmd-host",
        str(DMD_HOST),
        "--topic",
        str(TOPIC),
        "--system",
        str(system or ""),
        "--game",
        str(game or ""),
        "--rom-path",
        str(rom_path or ""),
    ]
    if title:
        cmd.extend(["--title", str(title)])
    try:
        subprocess.Popen(
            cmd,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
            close_fds=True,
        )
    except Exception:
        pass


_VIRTUAL_SYSTEMS = {
    "arcade",
    "favorites", "favourite", "favourites", "fav", "favs",
    "all", "allgames", "lastplayed", "recent", "recentlyplayed",
    "neverplayed", "multiplayer", "2players", "3players", "4players",
    "retroachievements", "lightgun", "vertical", "tate", "shmups",
    "fighting", "beatemup", "platform", "puzzle", "racing", "rpg", "sports",
}


def is_virtual_system(sys_name):
    if not sys_name:
        return True
    low = sys_name.strip().lower()
    if not low:
        return True
    if low.startswith(("auto-", "auto_", "custom-", "custom_", "collection")):
        return True
    compact = "".join(ch for ch in low if ch.isalnum())
    return compact in _VIRTUAL_SYSTEMS


def extract_real_system(sys_name, rom_path):
    if not is_virtual_system(sys_name) or not rom_path:
        return sys_name
    norm = rom_path.replace("\\", "/")
    idx = norm.lower().find("/roms/")
    if idx >= 0:
        rest = norm[idx + 6:].lstrip("/")
        parts = rest.split("/", 1)
        if len(parts) == 2 and parts[0]:
            candidate = clean_system_name(parts[0])
            if candidate and (candidate.lower() == "arcade" or not is_virtual_system(candidate)):
                return candidate
    return sys_name


def _parse_cli_args(argv):
    daemon_mode = False
    log_path = None
    i = 1
    while i < len(argv):
        arg = argv[i]
        if arg == "--daemon":
            daemon_mode = True
        elif arg == "--log-file" and (i + 1) < len(argv):
            log_path = argv[i + 1]
            i += 1
        i += 1
    if log_path is None and os.path.isdir("/recalbox/share/userscripts"):
        log_path = DEFAULT_RECALBOX_LOG
    return daemon_mode, log_path


def _install_signal_handlers():
    for sig_name in ("SIGHUP", "SIGPIPE"):
        sig = getattr(signal, sig_name, None)
        if sig is not None:
            try:
                signal.signal(sig, signal.SIG_IGN)
            except Exception:
                pass

    def _on_term(signum, _frame):
        if os.path.exists(STOP_FILE):
            log_msg(f"Stop sentinel detected on signal {signum}; exiting cleanly.")
            _mqtt_client.close()
            try:
                if os.path.exists(PID_FILE):
                    os.unlink(PID_FILE)
            except Exception:
                pass
            os._exit(0)
        # During Recalbox rungame / rundemo transitions, external scripts or ES helpers can
        # broadcast SIGTERM/SIGINT; ignore unless explicit stop sentinel is present.
        log_msg(f"Ignored external signal {signum} during EmulationStation transition.")

    for sig_name in ("SIGTERM", "SIGINT"):
        sig = getattr(signal, sig_name, None)
        if sig is not None:
            try:
                signal.signal(sig, _on_term)
            except Exception:
                pass


def main():
    global _log_file_path
    daemon_mode, log_path = _parse_cli_args(sys.argv)
    _log_file_path = log_path

    if daemon_mode:
        _daemonize(log_path)
    elif hasattr(os, "setsid"):
        try:
            os.setsid()
        except Exception:
            pass

    _install_signal_handlers()

    # Single-instance TCP lock (non-inheritable so child processes never hold port 49132).
    lock_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        if hasattr(lock_socket, "set_inheritable"):
            lock_socket.set_inheritable(False)
    except Exception:
        pass

    bound = False
    for _ in range(10):
        try:
            lock_socket.bind(("127.0.0.1", 49132))
            bound = True
            break
        except socket.error:
            time.sleep(0.1)
    if not bound:
        sys.exit(0)

    try:
        with open(PID_FILE, "w", encoding="utf-8") as pf:
            pf.write(f"{os.getpid()}\n")
    except Exception:
        pass

    log_msg(f"ArcadeMatrix daemon started (pid={os.getpid()}, DMD_HOST={DMD_HOST}, persistent MQTT)!")
    time.sleep(0.25)
    last_state_key = None
    last_sent_key = None
    last_playing_ctx = None
    pending_since = 0.0
    next_retry_at = 0.0

    while True:
        try:
            if os.path.exists(STOP_FILE):
                log_msg("Stop sentinel detected in main loop; exiting cleanly.")
                _mqtt_client.close()
                try:
                    os.unlink(PID_FILE)
                except Exception:
                    pass
                sys.exit(0)

            rom_path, game_title, system, state, action, is_folder = parse_statefile()
            state_low = (state or "").strip().lower()
            action_low = (action or "").strip().lower()

            if state_low == "stopped" or action_low in ("shutdown", "reboot", "quit", "stop"):
                norm_state = "stopped"
            elif (state_low == "playing" or action_low == "rungame") and action_low not in (
                "endgame",
                "enddemo",
                "rundemo",
                "systembrowsing",
                "gamelistbrowsing",
            ):
                norm_state = "playing"
            else:
                norm_state = "browsing"

            if norm_state != "stopped" and not system and not rom_path:
                _mqtt_client.tick_keepalive(time.time())
                time.sleep(0.05)
                continue

            system = clean_system_name(system)

            if norm_state == "stopped":
                current_key = (None, None, None, "stopped")
            elif action_low == "systembrowsing" or not rom_path:
                current_key = (None, None, system, "browsing")
            elif is_folder == "1" or (is_folder != "0" and os.path.isdir(rom_path)):
                folder_sys = clean_system_name(game_title) or clean_system_name(os.path.basename(rom_path.rstrip("/")))
                current_key = (None, None, folder_sys or system, "browsing")
            else:
                resolved_sys = extract_real_system(system, rom_path)
                current_key = (rom_path, game_title, resolved_sys, norm_state)

            now = time.time()
            if current_key != last_state_key:
                last_state_key = current_key
                pending_since = now
                next_retry_at = 0.0

            debounce_sec = 0.0 if current_key[3] in ("playing", "stopped") else 0.04
            if (now - pending_since) >= debounce_sec and current_key != last_sent_key and now >= next_retry_at:
                gbase = ""
                tclean = ""
                if current_key[3] == "stopped":
                    msg = '{"status": "stopped"}'
                elif current_key[0] is None:
                    msg = json.dumps({"status": "browsing", "system": str(current_key[2]), "type": "system"})
                else:
                    gbase = os.path.splitext(os.path.basename(current_key[0]))[0]
                    gbase = clean_system_name(gbase)
                    payload_obj = {
                        "status": current_key[3],
                        "game": gbase,
                        "system": str(current_key[2]),
                        "rom_path": str(current_key[0]),
                    }
                    if current_key[1]:
                        tclean = clean_system_name(current_key[1])
                        if tclean and tclean.lower() != gbase.lower():
                            payload_obj["title"] = tclean
                    msg = json.dumps(payload_obj)

                if publish_event(msg):
                    last_sent_key = current_key
                    if current_key[3] == "playing" and gbase:
                        last_playing_ctx = (str(current_key[2]), gbase, str(current_key[0] or ""), tclean)
                        spawn_companion_sync(
                            "start",
                            system=last_playing_ctx[0],
                            game=last_playing_ctx[1],
                            rom_path=last_playing_ctx[2],
                            title=last_playing_ctx[3],
                        )
                    elif current_key[3] == "stopped" or last_playing_ctx is not None:
                        ctx = last_playing_ctx or ("", "", "", "")
                        last_playing_ctx = None
                        spawn_companion_sync(
                            "stop",
                            system=ctx[0],
                            game=ctx[1],
                            rom_path=ctx[2],
                        )
                else:
                    next_retry_at = time.time() + 1.0
            else:
                _mqtt_client.tick_keepalive(now)
        except SystemExit:
            raise
        except Exception as e:
            log_msg("Loop error: " + str(e))

        time.sleep(0.05)


if __name__ == "__main__":
    main()

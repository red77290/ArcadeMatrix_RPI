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
import struct
import subprocess
import sys
import time

DMD_HOST = "{{DMD_HOST}}"
TOPIC = "{{TOPIC}}"
BROKER_CACHE_TTL_SEC = 30.0

PID_FILE = "/tmp/arcadematrix_daemon.pid"
STOP_FILE = "/tmp/arcadematrix_daemon.stop"
DEFAULT_RECALBOX_LOG = "/recalbox/share/userscripts/daemon.log"
MAX_LOG_BYTES = 65536

_log_file_path = None
_cached_broker_host = None
_cached_broker_port = 1883
_cached_broker_ts = 0.0
_preferred_http_port = None


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


def _abort_close_socket(sock):
    if sock is None:
        return
    try:
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_LINGER, struct.pack("ii", 1, 0))
    except Exception:
        pass
    try:
        sock.close()
    except Exception:
        pass


def _http_get_broker_target(host, http_port, timeout_sec=0.45):
    """Raw HTTP/1.0 GET with strict wall-clock deadline (zero urllib/keep-alive hang risk).
    Returns (resolved_host, resolved_port, connection_refused_bool).
    """
    deadline = time.monotonic() + timeout_sec
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        if hasattr(sock, "set_inheritable"):
            sock.set_inheritable(False)
        sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        tcp_uto = getattr(socket, "TCP_USER_TIMEOUT", 18)
        if sys.platform.startswith("linux"):
            sock.setsockopt(socket.IPPROTO_TCP, tcp_uto, 1500)
    except Exception:
        pass

    try:
        rem = deadline - time.monotonic()
        if rem <= 0.02:
            _abort_close_socket(sock)
            return None, None, False
        sock.settimeout(rem)
        sock.connect((host, int(http_port)))

        req = (
            f"GET /api/mqtt/broker-target HTTP/1.0\r\n"
            f"Host: {host}:{http_port}\r\n"
            f"Accept: application/json\r\n"
            f"Connection: close\r\n\r\n"
        ).encode("ascii", errors="ignore")
        sock.sendall(req)

        raw = bytearray()
        while len(raw) < 4096:
            rem = deadline - time.monotonic()
            if rem <= 0.01:
                break
            sock.settimeout(rem)
            chunk = sock.recv(1024)
            if not chunk:
                break
            raw.extend(chunk)
        try:
            sock.close()
        except Exception:
            pass
    except ConnectionRefusedError:
        _abort_close_socket(sock)
        return None, None, True
    except OSError as e:
        _abort_close_socket(sock)
        return None, None, (e.errno == errno.ECONNREFUSED)
    except Exception:
        _abort_close_socket(sock)
        return None, None, False

    sep = raw.find(b"\r\n\r\n")
    if sep < 0:
        return None, None, False
    head_line = raw[:sep].split(b"\r\n", 1)[0]
    parts = head_line.split()
    if len(parts) < 2 or parts[1] != b"200":
        return None, None, False
    try:
        data = json.loads(raw[sep + 4:].decode("utf-8", errors="ignore"))
        b_host = str(data.get("host") or "").strip()
        b_port = int(data.get("port") or 1883)
        if b_host and 0 < b_port <= 65535:
            return b_host, b_port, False
    except Exception:
        pass
    return None, None, False


def _probe_broker_endpoint():
    global _cached_broker_host, _cached_broker_port, _cached_broker_ts, _preferred_http_port
    now = time.monotonic()
    if _preferred_http_port == 8080:
        ports = (8080, 80)
    else:
        ports = (80, 8080)

    for idx, http_port in enumerate(ports):
        host, port, refused = _http_get_broker_target(DMD_HOST, http_port, timeout_sec=0.45)
        if host and port:
            changed = (_cached_broker_host != host or _cached_broker_port != port)
            _cached_broker_host = host
            _cached_broker_port = port
            _cached_broker_ts = now
            _preferred_http_port = http_port
            if changed:
                log_msg(f"Broker target resolved via HTTP :{http_port} -> {host}:{port}")
            return host, port
        # If the previously working HTTP port simply timed out while ESP32 was busy, don't waste
        # another 450 ms probing the secondary port unless the primary port actively refused TCP.
        if idx == 0 and _preferred_http_port == http_port and not refused:
            break

    if not _cached_broker_host:
        _cached_broker_host = DMD_HOST
        _cached_broker_port = 1883
    # Retry failed HTTP discovery after 10s (on idle ticks only) rather than every event
    _cached_broker_ts = now - max(0.0, BROKER_CACHE_TTL_SEC - 10.0)
    return _cached_broker_host, _cached_broker_port


def resolve_broker(force=False):
    if not force and _cached_broker_host:
        return _cached_broker_host, _cached_broker_port
    return _probe_broker_endpoint()


def refresh_broker_if_stale(now_mono):
    if not _cached_broker_host or (now_mono - _cached_broker_ts) >= BROKER_CACHE_TTL_SEC:
        _probe_broker_endpoint()


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


def _build_client_id():
    host_tag = ""
    try:
        raw_h = socket.gethostname()
        host_tag = "".join(ch for ch in raw_h if ch.isalnum())[:12]
    except Exception:
        pass
    if not host_tag:
        host_tag = "console"
    return f"am_rcbx_{host_tag}".encode("ascii", errors="ignore")[:23]


class PersistentMqttClient:
    """Persistent QoS-1 MQTT v3.1.1 TCP client with end-to-end PUBACK & PINGRESP verification.

    Guarantees that half-open TCP sockets (e.g. Wi-Fi packet loss, LwIP reset, or ESP32 reboot)
    are detected within <= 0.8s instead of silently buffering into Linux's 15-minute SO_SNDBUF.
    """

    KEEPALIVE_SEC = 60
    PING_INTERVAL_SEC = 20.0
    CONNECT_TIMEOUT_SEC = 1.0
    ACK_TIMEOUT_SEC = 0.8

    def __init__(self):
        self.sock = None
        self.host = None
        self.port = None
        self.last_io = 0.0
        self.pkt_id = 0
        self.rx_buf = bytearray()
        self.client_id = _build_client_id()

    def _next_packet_id(self):
        self.pkt_id = (self.pkt_id % 65535) + 1
        return self.pkt_id

    def close(self, abort=False):
        if self.sock is not None:
            if abort:
                _abort_close_socket(self.sock)
            else:
                try:
                    self.sock.close()
                except Exception:
                    pass
            self.sock = None
        self.rx_buf.clear()

    def _pop_buffered_packet(self):
        buf_len = len(self.rx_buf)
        if buf_len < 2:
            return None
        hdr = self.rx_buf[0]
        mult = 1
        rem_len = 0
        idx = 1
        while True:
            if idx >= buf_len:
                return None
            enc = self.rx_buf[idx]
            idx += 1
            rem_len += (enc & 0x7F) * mult
            if (enc & 0x80) == 0:
                break
            mult *= 128
            if idx > 4:
                # Malformed MQTT varint
                return (-1, 0, b"")
        total_len = idx + rem_len
        if buf_len < total_len:
            return None
        body = bytes(self.rx_buf[idx:total_len])
        del self.rx_buf[:total_len]
        return (hdr & 0xF0, hdr & 0x0F, body)

    def _read_packet(self, deadline):
        if self.sock is None:
            return None
        while True:
            parsed = self._pop_buffered_packet()
            if parsed is not None:
                if parsed[0] < 0:
                    self.close(abort=True)
                    return None
                return parsed
            rem = deadline - time.monotonic()
            if rem <= 0.005:
                return None
            try:
                self.sock.settimeout(rem)
                chunk = self.sock.recv(256)
                if not chunk:
                    self.close(abort=True)
                    return None
                self.rx_buf.extend(chunk)
            except Exception:
                self.close(abort=True)
                return None

    def _drain_nonblocking(self):
        if self.sock is None:
            return False
        try:
            self.sock.setblocking(False)
            while len(self.rx_buf) < 4096:
                try:
                    chunk = self.sock.recv(256)
                    if not chunk:
                        self.close(abort=True)
                        return False
                    self.rx_buf.extend(chunk)
                except BlockingIOError:
                    break
                except socket.error as e:
                    if e.errno in (errno.EAGAIN, errno.EWOULDBLOCK):
                        break
                    self.close(abort=True)
                    return False
            while True:
                pkt = self._pop_buffered_packet()
                if pkt is None:
                    break
                if pkt[0] < 0:
                    self.close(abort=True)
                    return False
        except Exception:
            self.close(abort=True)
            return False
        finally:
            if self.sock is not None:
                try:
                    self.sock.settimeout(self.ACK_TIMEOUT_SEC)
                except Exception:
                    pass
        return True

    def _configure_socket(self, sock):
        try:
            if hasattr(sock, "set_inheritable"):
                sock.set_inheritable(False)
            sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_KEEPALIVE, 1)
            if sys.platform.startswith("linux"):
                if hasattr(socket, "TCP_KEEPIDLE"):
                    sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_KEEPIDLE, 15)
                if hasattr(socket, "TCP_KEEPINTVL"):
                    sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_KEEPINTVL, 5)
                if hasattr(socket, "TCP_KEEPCNT"):
                    sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_KEEPCNT, 3)
                tcp_uto = getattr(socket, "TCP_USER_TIMEOUT", 18)
                sock.setsockopt(socket.IPPROTO_TCP, tcp_uto, 2000)
        except Exception:
            pass

    def ensure_connected(self, host, port):
        port = int(port)
        if self.sock is not None and (self.host != host or self.port != port):
            self.close(abort=False)

        if self.sock is not None and self._drain_nonblocking():
            return True

        self.close(abort=True)
        cid = self.client_id
        c_body = (
            b"\x00\x04MQTT\x04\x02"
            + self.KEEPALIVE_SEC.to_bytes(2, "big")
            + len(cid).to_bytes(2, "big")
            + cid
        )
        c_pkt = b"\x10" + _mqtt_enc_len(len(c_body)) + c_body

        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._configure_socket(sock)
        deadline = time.monotonic() + self.CONNECT_TIMEOUT_SEC
        try:
            sock.settimeout(self.CONNECT_TIMEOUT_SEC)
            sock.connect((host, port))
            sock.sendall(c_pkt)
            self.sock = sock
            self.rx_buf.clear()
            ack = self._read_packet(deadline)
            if ack is None or ack[0] != 0x20 or len(ack[2]) < 2 or ack[2][1] != 0x00:
                self.close(abort=True)
                return False
            self.host = host
            self.port = port
            self.last_io = time.monotonic()
            return True
        except Exception:
            self.sock = None
            _abort_close_socket(sock)
            return False

    def publish(self, host, port, topic, payload):
        t_b = topic.encode("utf-8")
        m_b = payload.encode("utf-8")
        pkt_id = self._next_packet_id()
        p_body = len(t_b).to_bytes(2, "big") + t_b + pkt_id.to_bytes(2, "big") + m_b
        p_pkt = b"\x32" + _mqtt_enc_len(len(p_body)) + p_body

        for attempt in range(2):
            if not self.ensure_connected(host, port):
                continue
            try:
                deadline = time.monotonic() + self.ACK_TIMEOUT_SEC
                self.sock.settimeout(self.ACK_TIMEOUT_SEC)
                self.sock.sendall(p_pkt)
                while True:
                    resp = self._read_packet(deadline)
                    if resp is None:
                        if attempt == 0:
                            log_msg(f"WARN MQTT PUBACK timeout on {host}:{port}; reconnecting...")
                        self.close(abort=True)
                        break
                    if resp[0] == 0xD0:
                        # Interleaved PINGRESP
                        continue
                    if resp[0] == 0x40 and len(resp[2]) == 2 and int.from_bytes(resp[2], "big") == pkt_id:
                        self.last_io = time.monotonic()
                        return True
                    self.close(abort=True)
                    break
            except Exception as e:
                if attempt == 0:
                    log_msg(f"WARN MQTT send error ({e}); reconnecting...")
                self.close(abort=True)
        return False

    def tick_keepalive(self, now_mono):
        if self.sock is None:
            return
        if not self._drain_nonblocking():
            return
        if (now_mono - self.last_io) >= self.PING_INTERVAL_SEC:
            try:
                deadline = time.monotonic() + self.ACK_TIMEOUT_SEC
                self.sock.settimeout(self.ACK_TIMEOUT_SEC)
                self.sock.sendall(b"\xc0\x00")
                while True:
                    resp = self._read_packet(deadline)
                    if resp is None:
                        log_msg("WARN MQTT PINGRESP timeout; closed stale idle socket.")
                        self.close(abort=True)
                        return
                    if resp[0] == 0xD0:
                        self.last_io = time.monotonic()
                        return
            except Exception:
                self.close(abort=True)


_mqtt_client = PersistentMqttClient()


def publish_event(msg):
    t0 = time.monotonic()
    broker_host, broker_port = resolve_broker(force=False)
    if _mqtt_client.publish(broker_host, broker_port, TOPIC, msg):
        dt_ms = (time.monotonic() - t0) * 1000.0
        log_msg(f"PUB ok ({dt_ms:.1f}ms -> {broker_host}:{broker_port}): {msg}")
        return True

    # Re-probe broker endpoint immediately in case Showcase/local broker target switched
    new_host, new_port = resolve_broker(force=True)
    if (new_host != broker_host or new_port != broker_port) and _mqtt_client.publish(new_host, new_port, TOPIC, msg):
        dt_ms = (time.monotonic() - t0) * 1000.0
        log_msg(f"PUB ok after broker switch ({dt_ms:.1f}ms -> {new_host}:{new_port}): {msg}")
        return True

    # Fallback to mosquitto_pub QoS 1 if available
    try:
        res = subprocess.run(
            ["mosquitto_pub", "-q", "1", "-h", new_host, "-p", str(new_port), "-t", TOPIC, "-m", msg],
            timeout=1.5,
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        if res.returncode == 0:
            log_msg(f"PUB ok via mosquitto_pub (-> {new_host}:{new_port}): {msg}")
            return True
    except Exception:
        pass

    log_msg(f"WARN publish failed (-> {new_host}:{new_port}); will retry in 1.0s: {msg}")
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

    log_msg(f"ArcadeMatrix daemon started (pid={os.getpid()}, DMD_HOST={DMD_HOST}, QoS1 persistent MQTT)!")
    resolve_broker(force=True)
    time.sleep(0.15)
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

            now = time.monotonic()
            if norm_state != "stopped" and not system and not rom_path:
                refresh_broker_if_stale(now)
                _mqtt_client.tick_keepalive(now)
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
                    sys_str = str(current_key[2])
                    payload_obj = {
                        "status": current_key[3],
                        "game": gbase,
                        "system": sys_str,
                    }
                    if is_virtual_system(sys_str):
                        payload_obj["rom_path"] = str(current_key[0])
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
                    next_retry_at = time.monotonic() + 1.0
            else:
                if current_key == last_sent_key:
                    refresh_broker_if_stale(now)
                _mqtt_client.tick_keepalive(now)
        except SystemExit:
            raise
        except Exception as e:
            log_msg("Loop error: " + str(e))

        time.sleep(0.05)


if __name__ == "__main__":
    main()

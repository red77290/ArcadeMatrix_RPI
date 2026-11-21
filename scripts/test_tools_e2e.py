#!/usr/bin/env python3
"""
scripts/test_tools_e2e.py
Hermetic End-to-End (E2E) validation suite for ArcadeMatrix_RPI PC & Console Companion Tools.

Validates every companion script directly from an isolated unzipped `ArcadeMatrix-tools.zip`
bundle (mirroring exact end-user usage without a Git repository checkout):
  1. Standalone `ArcadeMatrix-tools.zip` packaging, UNIX permissions, LF line-endings,
     and `sh -n` / `bash -n` syntax validation across all `.sh` scripts.
  2. Live local DMD mock servers:
       - HTTP server answering `GET /api/mqtt/broker-target` (port 8080 when free)
       - Raw TCP MQTT v3.1.1 broker capturing `CONNECT` + `PUBLISH` JSON payloads
  3. `ArcadeMatrix-tools/install.sh` E2E against isolated Batocera, Recalbox, and RetroPie
     SSH sandboxes + Action 2 (Check Logs) + live Batocera ES/configgen hook & universal
     `hi2txt` (`sf2.hi`) high-score celebration (`celebrate` / `highscore`) + live
     RetroPie/Recalbox double-forked daemon & watchdog (`arcadematrix_daemon.py` + `es_state.inf`).
  4. `ArcadeMatrix-tools/install.ps1` E2E executed through real PowerShell (`pwsh`, when
     installed on the host/CI runner) in `Auto-Detect` mode (>600 KB Base64 stdin payload
     piped to `/bin/sh -s`) across Batocera, Recalbox, and RetroPie + `-Action 2` Logs,
     verifying byte-for-byte SHA-256 integrity of the 150 KB `arcadematrix_companion_sync.py`.
  5. `ArcadeMatrix-tools/mugen_extractor/mugen_extractor.py` end-to-end SFFv1 + AIR + DEF
     character extraction into `.fgt` RGB565 sprites + `index.txt` / `index.json`.
"""

import ast
import hashlib
import http.server
import io
import json
import os
import re
import shutil
import signal
import socket
import socketserver
import struct
import subprocess
import sys
import tempfile
import threading
import time
import unittest
import zipfile

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACKAGE_TOOLS_PY = os.path.join(ROOT_DIR, "scripts", "package_tools.py")


def read_bytes(path: str) -> bytes:
    with open(path, "rb") as f:
        return f.read()


def write_bytes(path: str, data: bytes) -> None:
    with open(path, "wb") as f:
        f.write(data)


def read_text(path: str) -> str:
    with open(path, "r", encoding="utf-8") as f:
        return f.read()


def write_text(path: str, data: str) -> None:
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(data)


def find_pwsh() -> str | None:
    env_pwsh = os.environ.get("PWSH_BIN", "").strip()
    if env_pwsh and os.path.isfile(env_pwsh) and os.access(env_pwsh, os.X_OK):
        return env_pwsh
    which_pwsh = shutil.which("pwsh")
    if which_pwsh:
        return which_pwsh
    if os.path.isfile("/tmp/pwsh/pwsh") and os.access("/tmp/pwsh/pwsh", os.X_OK):
        return "/tmp/pwsh/pwsh"
    return None


PWSH_BIN = find_pwsh()


def _decode_mqtt_rem_len(sock: socket.socket) -> int | None:
    mult = 1
    val = 0
    for _ in range(4):
        b = sock.recv(1)
        if not b:
            return None
        digit = b[0]
        val += (digit & 0x7F) * mult
        if (digit & 0x80) == 0:
            return val
        mult *= 128
    return None


def _recv_exact(sock: socket.socket, n: int) -> bytes | None:
    buf = bytearray()
    while len(buf) < n:
        chunk = sock.recv(n - len(buf))
        if not chunk:
            return None
        buf.extend(chunk)
    return bytes(buf)


class MiniMqttBroker:
    """Minimal thread-safe TCP MQTT v3.1.1 broker capturing all PUBLISH messages."""

    def __init__(self) -> None:
        self._srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self._srv.bind(("127.0.0.1", 0))
        self.port = self._srv.getsockname()[1]
        self._srv.listen(32)
        self._stop = threading.Event()
        self._lock = threading.Lock()
        self.messages: list[tuple[str, dict | str]] = []
        self._thread = threading.Thread(target=self._accept_loop, daemon=True)
        self._thread.start()

    def clear(self) -> None:
        with self._lock:
            self.messages.clear()

    def snapshot(self) -> list[tuple[str, dict | str]]:
        with self._lock:
            return list(self.messages)

    def wait_for(self, predicate, timeout: float = 5.0) -> dict | None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            for topic, payload in self.snapshot():
                if isinstance(payload, dict) and predicate(topic, payload):
                    return payload
            time.sleep(0.05)
        return None

    def _accept_loop(self) -> None:
        self._srv.settimeout(0.2)
        while not self._stop.is_set():
            try:
                conn, _ = self._srv.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            threading.Thread(target=self._client_loop, args=(conn,), daemon=True).start()

    def _client_loop(self, conn: socket.socket) -> None:
        conn.settimeout(3.0)
        try:
            while not self._stop.is_set():
                hdr = conn.recv(1)
                if not hdr:
                    break
                pkt_type = hdr[0] & 0xF0
                flags = hdr[0] & 0x0F
                rem_len = _decode_mqtt_rem_len(conn)
                if rem_len is None:
                    break
                body = _recv_exact(conn, rem_len) if rem_len > 0 else b""
                if body is None:
                    break

                if pkt_type == 0x10:
                    conn.sendall(b"\x20\x02\x00\x00")
                elif pkt_type == 0x30:
                    if len(body) < 2:
                        continue
                    tlen = struct.unpack(">H", body[:2])[0]
                    idx = 2 + tlen
                    topic = body[2:idx].decode("utf-8", errors="replace")
                    qos = (flags >> 1) & 0x03
                    pkt_id = None
                    if qos > 0:
                        if idx + 2 > len(body):
                            continue
                        pkt_id = body[idx : idx + 2]
                        idx += 2
                    raw_msg = body[idx:].decode("utf-8", errors="replace")
                    try:
                        parsed = json.loads(raw_msg)
                    except Exception:
                        parsed = raw_msg
                    with self._lock:
                        self.messages.append((topic, parsed))
                    if qos == 1 and pkt_id is not None:
                        conn.sendall(b"\x40\x02" + pkt_id)
                elif pkt_type == 0xC0:
                    conn.sendall(b"\xd0\x00")
                elif pkt_type == 0xE0:
                    break
        except (socket.timeout, OSError):
            pass
        finally:
            try:
                conn.close()
            except OSError:
                pass

    def close(self) -> None:
        self._stop.set()
        try:
            self._srv.close()
        except OSError:
            pass
        self._thread.join(timeout=1.5)


class ReusableTCPServer(socketserver.ThreadingMixIn, http.server.HTTPServer):
    allow_reuse_address = True
    daemon_threads = True


class MockDmdHttpServer:
    """Local HTTP server answering GET /api/mqtt/broker-target on port 8080 (or ephemeral)."""

    def __init__(self, mqtt_port: int) -> None:
        self.mqtt_port = mqtt_port
        self.requests: int = 0
        parent = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                if self.path.startswith("/api/mqtt/broker-target"):
                    parent.requests += 1
                    body = json.dumps(
                        {
                            "host": "127.0.0.1",
                            "port": parent.mqtt_port,
                            "mode": "local",
                            "sync_show_hiscores": True,
                            "sync_show_ra": True,
                            "sync_celebrate_events": True,
                        }
                    ).encode("utf-8")
                    self.send_response(200)
                    self.send_header("Content-Type", "application/json")
                    self.send_header("Content-Length", str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)
                else:
                    self.send_response(404)
                    self.end_headers()

            def log_message(self, fmt, *args):
                pass

        try:
            self._httpd = ReusableTCPServer(("127.0.0.1", 8080), Handler)
            self.port = 8080
        except OSError:
            self._httpd = ReusableTCPServer(("127.0.0.1", 0), Handler)
            self.port = self._httpd.server_address[1]

        self._thread = threading.Thread(target=self._httpd.serve_forever, daemon=True)
        self._thread.start()

    def close(self) -> None:
        try:
            self._httpd.shutdown()
            self._httpd.server_close()
        except OSError:
            pass
        self._thread.join(timeout=1.5)


def create_mock_ssh_bin(
    bin_dir: str,
    console_root: str,
    rewrite_stdin_and_b64: bool,
    http_port: int = 8080,
) -> dict[str, str]:
    os.makedirs(bin_dir, exist_ok=True)
    os.makedirs(os.path.join(console_root, "tmp"), exist_ok=True)

    helper_py = os.path.join(bin_dir, "_mock_ssh_prep.py")
    write_text(
        helper_py,
        """import json
import os
import re
import sys

root = os.environ["AM_MOCK_CONSOLE_ROOT"]
rewrite_payload = os.environ.get("AM_MOCK_REWRITE_PAYLOAD") == "1"
http_port = os.environ.get("AM_MOCK_HTTP_PORT", "8080")
args_file, out_script_file, out_stdin_file = sys.argv[1], sys.argv[2], sys.argv[3]

with open(args_file, "r", encoding="utf-8") as af:
    args = json.load(af)

_REMAP_RE = re.compile(r"(?<![A-Za-z0-9_])(/recalbox|/userdata|/opt/retropie|/tmp/arcadematrix_|/tmp/am_|/tmp/es_state\\.inf|/tmp/retroarch)")

def remap(text: str) -> str:
    text = _REMAP_RE.sub(lambda m: root + m.group(1), text)
    if http_port != "8080":
        text = text.replace("80 8080", f"80 {http_port}").replace("(80, 8080)", f"(80, {http_port})")
    return text

i = 0
target = None
cmd_parts = []
while i < len(args):
    a = args[i]
    if a == "--":
        i += 1
        if i < len(args):
            target = args[i]
            cmd_parts = args[i + 1 :]
        break
    if a in ("-T", "-N", "-f", "-q", "-v"):
        i += 1
        continue
    if a in ("-o", "-S", "-p", "-i", "-l", "-F"):
        i += 2
        continue
    if a == "-O":
        sys.exit(10)
    if target is None:
        target = a
    else:
        cmd_parts.append(a)
    i += 1

user = "root"
if target and "@" in target:
    user = target.split("@", 1)[0]

if os.environ.get("AM_MOCK_DENY_ROOT") == "1" and user == "root":
    sys.exit(255)

remote_cmd = " ".join(cmd_parts).strip()
if not remote_cmd or remote_cmd == "true":
    sys.exit(10)

stdin_bytes = sys.stdin.buffer.read()

if "sh -s" in remote_cmd:
    script_text = stdin_bytes.decode("utf-8", errors="surrogateescape").replace("\\r\\n", "\\n").replace("\\r", "\\n")
    out_lines = []
    in_b64 = False
    for line in script_text.splitlines(keepends=True):
        stripped = line.strip()
        if not in_b64 and "am_write_b64 " in line and "<<" in line:
            out_lines.append(remap(line))
            in_b64 = True
            continue
        if in_b64 and stripped.startswith("AM_B64_"):
            in_b64 = False
            out_lines.append(line)
            continue
        if in_b64:
            out_lines.append(line)
        else:
            out_lines.append(remap(line))
    with open(out_script_file, "wb") as sf:
        sf.write("".join(out_lines).encode("utf-8", errors="surrogateescape"))
    with open(out_stdin_file, "wb") as inf:
        inf.write(b"")
    sys.exit(0)

remapped_cmd = remap(remote_cmd)
if rewrite_payload and stdin_bytes:
    try:
        txt = stdin_bytes.decode("utf-8")
        stdin_bytes = remap(txt).encode("utf-8")
    except UnicodeDecodeError:
        pass

with open(out_script_file, "w", encoding="utf-8") as sf:
    sf.write(remapped_cmd + "\\n")
with open(out_stdin_file, "wb") as inf:
    inf.write(stdin_bytes)
sys.exit(0)
""",
    )

    ssh_sh = os.path.join(bin_dir, "ssh")
    write_text(
        ssh_sh,
        f"""#!/bin/sh
_af="{console_root}/tmp/ssh_args_$$.json"
_sf="{console_root}/tmp/ssh_cmd_$$.sh"
_if="{console_root}/tmp/ssh_in_$$.bin"
python3 -c 'import json,sys; open(sys.argv[1],"w").write(json.dumps(sys.argv[2:]))' "$_af" "$@"
python3 "{helper_py}" "$_af" "$_sf" "$_if"
_rc=$?
rm -f "$_af"
if [ "$_rc" = "10" ]; then
    rm -f "$_sf" "$_if"
    exit 0
fi
if [ "$_rc" != "0" ]; then
    rm -f "$_sf" "$_if"
    exit "$_rc"
fi
/bin/sh "$_sf" < "$_if"
_rc=$?
rm -f "$_sf" "$_if"
exit "$_rc"
""",
    )
    os.chmod(ssh_sh, 0o755)

    if rewrite_stdin_and_b64:
        b64_py = os.path.join(bin_dir, "base64")
        write_text(
            b64_py,
            """#!/usr/bin/env python3
import base64
import os
import re
import sys

root = os.environ["AM_MOCK_CONSOLE_ROOT"]
http_port = os.environ.get("AM_MOCK_HTTP_PORT", "8080")
_REMAP_RE = re.compile(r"(?<![A-Za-z0-9_])(/recalbox|/userdata|/opt/retropie|/tmp/arcadematrix_|/tmp/am_|/tmp/es_state\\.inf|/tmp/retroarch)")

if "-d" in sys.argv[1:] or "--decode" in sys.argv[1:]:
    raw = base64.b64decode(sys.stdin.buffer.read())
    try:
        txt = raw.decode("utf-8")
        txt = _REMAP_RE.sub(lambda m: root + m.group(1), txt)
        if http_port != "8080":
            txt = txt.replace("80 8080", f"80 {http_port}").replace("(80, 8080)", f"(80, {http_port})")
        raw = txt.encode("utf-8")
    except UnicodeDecodeError:
        pass
    sys.stdout.buffer.write(raw)
else:
    encoded = base64.b64encode(sys.stdin.buffer.read())
    sys.stdout.buffer.write(encoded)
""",
        )
        os.chmod(b64_py, 0o755)

    env = os.environ.copy()
    env["PATH"] = f"{bin_dir}:{env.get('PATH', '')}"
    env["AM_MOCK_CONSOLE_ROOT"] = console_root
    env["AM_MOCK_REWRITE_PAYLOAD"] = "1" if rewrite_stdin_and_b64 else "0"
    env["AM_MOCK_HTTP_PORT"] = str(http_port)
    env["HOME"] = os.path.join(console_root, "home_client")
    os.makedirs(env["HOME"], exist_ok=True)
    env["NO_PROXY"] = "127.0.0.1,localhost"
    env["no_proxy"] = "127.0.0.1,localhost"
    return env


def stop_sandbox_daemons(console_root: str) -> None:
    tmp_dir = os.path.join(console_root, "tmp")
    stop_file = os.path.join(tmp_dir, "arcadematrix_daemon.stop")
    try:
        os.makedirs(tmp_dir, exist_ok=True)
        write_text(stop_file, "stop\n")
    except OSError:
        pass

    pids: list[int] = []
    for name in ("arcadematrix_supervisor.pid", "arcadematrix_daemon.pid", "arcadematrix_mqtt_pub.pid"):
        pf = os.path.join(tmp_dir, name)
        if os.path.isfile(pf):
            try:
                val = int(read_text(pf).strip())
                if val > 1:
                    pids.append(val)
            except Exception:
                pass

    if os.path.isdir("/proc"):
        for entry in os.listdir("/proc"):
            if not entry.isdigit():
                continue
            pid = int(entry)
            if pid in (1, os.getpid()):
                continue
            try:
                cmdline = read_bytes(f"/proc/{pid}/cmdline").decode("utf-8", errors="ignore")
                if console_root in cmdline:
                    pids.append(pid)
            except OSError:
                continue

    for pid in set(pids):
        try:
            os.kill(pid, signal.SIGKILL)
        except OSError:
            pass

    deadline = time.monotonic() + 2.0
    while time.monotonic() < deadline:
        alive = False
        for pid in set(pids):
            try:
                os.kill(pid, 0)
                alive = True
            except OSError:
                pass
        if not alive:
            break
        time.sleep(0.05)


def make_sf2_hi_bytes(top_score: int, initials: str = "KEN") -> bytes:
    def bcd4(val: int) -> bytes:
        s = f"{val:08d}"[-8:]
        return bytes(int(s[i : i + 2], 16) for i in range(0, 8, 2))

    tag = (initials.upper() + "   ")[:3].encode("ascii")
    buf = bytearray(80)
    buf[0:8] = bcd4(top_score) + tag + b"\x00"
    buf[8:16] = bcd4(max(10000, top_score // 2)) + b"RYU\x00"
    return bytes(buf)


class RpiCompanionToolsE2ETest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.work_dir = tempfile.mkdtemp(prefix=".am_rpi_tools_e2e_", dir="/tmp")
        cls.zip_path = os.path.join(cls.work_dir, "ArcadeMatrix-tools.zip")
        res = subprocess.run(
            [sys.executable, PACKAGE_TOOLS_PY, "--output", cls.zip_path],
            capture_output=True,
            text=True,
            check=False,
        )
        if res.returncode != 0:
            raise RuntimeError(f"package_tools.py failed:\n{res.stdout}\n{res.stderr}")

        cls.extract_root = os.path.join(cls.work_dir, "unzipped")
        with zipfile.ZipFile(cls.zip_path, "r") as zf:
            for info in zf.infolist():
                target = os.path.join(cls.extract_root, info.filename)
                os.makedirs(os.path.dirname(target), exist_ok=True)
                write_bytes(target, zf.read(info.filename))
                mode = (info.external_attr >> 16) & 0o777
                if mode:
                    os.chmod(target, mode)

        cls.tools_dir = os.path.join(cls.extract_root, "ArcadeMatrix-tools")
        cls.mqtt = MiniMqttBroker()
        cls.http = MockDmdHttpServer(cls.mqtt.port)

    @classmethod
    def tearDownClass(cls) -> None:
        cls.http.close()
        cls.mqtt.close()
        shutil.rmtree(cls.work_dir, ignore_errors=True)

    # ------------------------------------------------------------------
    # 1. Standalone Archive Integrity & Shell Syntax (`sh -n` / `bash -n`)
    # ------------------------------------------------------------------
    def test_01_standalone_archive_permissions_and_shell_syntax(self) -> None:
        sh_count = 0
        py_count = 0
        for root, _dirs, files in os.walk(self.tools_dir):
            for fname in sorted(files):
                fpath = os.path.join(root, fname)
                if fname.endswith((".sh", ".py")):
                    raw = read_bytes(fpath)
                    self.assertNotIn(b"\r", raw, f"CR byte found in {fpath}")
                    st_mode = os.stat(fpath).st_mode & 0o777
                    self.assertEqual(st_mode, 0o755, f"Expected 0755 on {fpath}, got {oct(st_mode)}")

                if fname.endswith(".sh"):
                    sh_count += 1
                    first_line = read_text(fpath).splitlines()[0].strip()
                    shell_bin = "/bin/sh" if first_line == "#!/bin/sh" else "bash"
                    chk = subprocess.run([shell_bin, "-n", fpath], capture_output=True, text=True)
                    self.assertEqual(chk.returncode, 0, f"{shell_bin} -n failed on {fpath}: {chk.stderr}")
                elif fname.endswith(".py"):
                    py_count += 1
                    ast.parse(read_text(fpath), filename=fpath)

        self.assertGreaterEqual(sh_count, 5)
        self.assertGreaterEqual(py_count, 3)

    # ------------------------------------------------------------------
    # 2. Batocera Byte-Exact Transfer (`tools/install.sh`) + Live High-Score Celebration
    # ------------------------------------------------------------------
    def test_02_batocera_install_sh_byte_exact_and_live_hiscore_e2e(self) -> None:
        self.mqtt.clear()
        console_root = os.path.join(self.work_dir, "bato_sh")
        os.makedirs(os.path.join(console_root, "userdata", "system"), exist_ok=True)
        bin_dir = os.path.join(self.work_dir, "bin_bato_sh")
        env = create_mock_ssh_bin(bin_dir, console_root, rewrite_stdin_and_b64=False, http_port=self.http.port)

        install_sh = os.path.join(self.tools_dir, "install.sh")
        res = subprocess.run(
            ["bash", install_sh],
            input="10.0.0.11\n1\n127.0.0.1:8080\n1\n\n\n",
            capture_output=True,
            text=True,
            env=env,
            timeout=20,
        )
        self.assertEqual(res.returncode, 0, f"tools/install.sh Batocera failed:\n{res.stdout}\n{res.stderr}")
        self.assertIn("Console identified as: batocera", res.stdout)
        self.assertIn("DMD /api/mqtt/broker-target ->", res.stdout)

        src_comp = os.path.join(self.tools_dir, "arcadematrix_companion_sync.py")
        expected_comp = (
            read_text(src_comp)
            .replace("{{DMD_HOST}}", "127.0.0.1")
            .replace("{{BROKER}}", "127.0.0.1")
            .replace("{{TOPIC}}", "system/playing/batocera")
            .encode("utf-8")
        )
        deployed_comp_path = os.path.join(
            console_root, "userdata", "system", "scripts", "arcadematrix_companion_sync.py"
        )
        deployed_comp = read_bytes(deployed_comp_path)
        self.assertEqual(
            hashlib.sha256(deployed_comp).hexdigest(),
            hashlib.sha256(expected_comp).hexdigest(),
            "Deployed Batocera arcadematrix_companion_sync.py is not byte-identical!",
        )

        remap_re = re.compile(
            r"(?<![A-Za-z0-9_])(/recalbox|/userdata|/opt/retropie|/tmp/arcadematrix_|/tmp/am_|/tmp/es_state\.inf|/tmp/retroarch)"
        )
        hook_path = os.path.join(console_root, "userdata", "system", "scripts", "arcadematrix_mqtt.sh")
        for p in (hook_path, deployed_comp_path):
            txt = remap_re.sub(lambda m: console_root + m.group(1), read_text(p))
            if self.http.port != 8080:
                txt = txt.replace("80 8080", f"80 {self.http.port}").replace("(80, 8080)", f"(80, {self.http.port})")
            write_text(p, txt)

        hi_dir = os.path.join(console_root, "userdata", "saves", "mame", "mame0278", "hiscore")
        rom_dir = os.path.join(console_root, "userdata", "roms", "mame")
        os.makedirs(hi_dir, exist_ok=True)
        os.makedirs(rom_dir, exist_ok=True)
        hi_file = os.path.join(hi_dir, "sf2.hi")
        rom_file = os.path.join(rom_dir, "sf2.zip")
        write_bytes(rom_file, b"PK\x05\x06" + b"\x00" * 18)
        write_bytes(hi_file, make_sf2_hi_bytes(50000, "RYU"))

        try:
            subprocess.run(["/bin/sh", hook_path, "system-selected", "mame"], env=env, check=True, timeout=5)
            self.assertIsNotNone(
                self.mqtt.wait_for(
                    lambda t, p: t == "system/playing/batocera"
                    and p.get("status") == "browsing"
                    and p.get("system") == "mame"
                )
            )

            subprocess.run(
                ["/bin/sh", hook_path, "gameStart", "mame", "libretro", "mame0278", rom_file],
                env=env,
                check=True,
                timeout=5,
            )
            meta_evt = self.mqtt.wait_for(
                lambda t, p: t == "system/playing/batocera" and p.get("status") == "sync_meta" and p.get("game") == "sf2"
            )
            self.assertIsNotNone(meta_evt)
            self.assertEqual(meta_evt["hiscores"][0]["score"], "50,000")
            self.assertEqual(meta_evt["hiscores"][0]["tag"], "RYU")

            time.sleep(0.05)
            write_bytes(hi_file, make_sf2_hi_bytes(950000, "KEN"))
            subprocess.run(["/bin/sh", hook_path, "gameStop"], env=env, check=True, timeout=5)

            rec_evt = self.mqtt.wait_for(
                lambda t, p: t == "system/playing/batocera"
                and p.get("status") == "celebrate"
                and p.get("kind") == "highscore"
            )
            self.assertIsNotNone(rec_evt, f"Missing celebrate highscore event; got {self.mqtt.snapshot()}")
            self.assertEqual(rec_evt["score"], "950,000")
            self.assertEqual(rec_evt["tag"], "KEN")
            self.assertEqual(rec_evt["rank"], 1)
        finally:
            stop_sandbox_daemons(console_root)

    # ------------------------------------------------------------------
    # 3. Recalbox & RetroPie Live Daemon + Watchdog (`tools/install.sh` + Action 2)
    # ------------------------------------------------------------------
    def test_03_recalbox_and_retropie_install_sh_live_daemon_and_logs(self) -> None:
        install_sh = os.path.join(self.tools_dir, "install.sh")

        rb_root = os.path.join(self.work_dir, "recalbox_sh")
        os.makedirs(os.path.join(rb_root, "recalbox", "share"), exist_ok=True)
        rb_bin = os.path.join(self.work_dir, "bin_rb_sh")
        rb_env = create_mock_ssh_bin(rb_bin, rb_root, rewrite_stdin_and_b64=True, http_port=self.http.port)

        try:
            self.mqtt.clear()
            res = subprocess.run(
                ["bash", install_sh],
                input="10.0.0.12\n1\n127.0.0.1:8080\n1\n\n\n",
                capture_output=True,
                text=True,
                env=rb_env,
                timeout=25,
            )
            self.assertEqual(res.returncode, 0, f"tools/install.sh Recalbox failed:\n{res.stdout}\n{res.stderr}")
            es_inf = os.path.join(rb_root, "tmp", "es_state.inf")
            write_text(es_inf, "State=browsing\nAction=systembrowsing\nSystemId=neogeo\nGamePath=\nGame=\n")
            rb_evt = self.mqtt.wait_for(
                lambda t, p: t == "system/playing/recalbox"
                and p.get("status") == "browsing"
                and p.get("system") == "neogeo"
            )
            self.assertIsNotNone(rb_evt, f"Recalbox daemon did not publish; got {self.mqtt.snapshot()}")

            log_res = subprocess.run(
                ["bash", install_sh],
                input="10.0.0.12\n2\n1\n\n\n",
                capture_output=True,
                text=True,
                env=rb_env,
                timeout=15,
            )
            self.assertEqual(log_res.returncode, 0)
            self.assertIn("PUB ok", log_res.stdout)
        finally:
            stop_sandbox_daemons(rb_root)

        rp_root = os.path.join(self.work_dir, "retropie_sh")
        os.makedirs(os.path.join(rp_root, "opt", "retropie", "configs", "all"), exist_ok=True)
        rp_bin = os.path.join(self.work_dir, "bin_rp_sh")
        rp_env = create_mock_ssh_bin(rp_bin, rp_root, rewrite_stdin_and_b64=True, http_port=self.http.port)
        rp_env["AM_MOCK_DENY_ROOT"] = "1"

        try:
            self.mqtt.clear()
            res = subprocess.run(
                ["bash", install_sh],
                input="10.0.0.13\n1\n127.0.0.1:8080\n1\n\n\n",
                capture_output=True,
                text=True,
                env=rp_env,
                timeout=25,
            )
            self.assertEqual(res.returncode, 0, f"tools/install.sh RetroPie failed:\n{res.stdout}\n{res.stderr}")
            onstart = os.path.join(rp_root, "opt", "retropie", "configs", "all", "runcommand-onstart.sh")
            onend = os.path.join(rp_root, "opt", "retropie", "configs", "all", "runcommand-onend.sh")
            subprocess.run(["/bin/sh", onstart, "arcade", "lr-fbneo", "/home/pi/RetroPie/roms/arcade/pacman.zip"], check=True)
            self.assertIsNotNone(
                self.mqtt.wait_for(
                    lambda t, p: t == "system/playing/retropie"
                    and p.get("status") == "playing"
                    and p.get("game") == "pacman"
                )
            )
            subprocess.run(["/bin/sh", onend], check=True)
            self.assertIsNotNone(
                self.mqtt.wait_for(lambda t, p: t == "system/playing/retropie" and p.get("status") == "stopped")
            )
        finally:
            stop_sandbox_daemons(rp_root)

    # ------------------------------------------------------------------
    # 4. PowerShell `tools/install.ps1` E2E (>600 KB Base64 Auto-Detect Payload)
    # ------------------------------------------------------------------
    def test_04_install_ps1_powershell_e2e_all_three_consoles(self) -> None:
        if not PWSH_BIN:
            self.skipTest("pwsh not installed on this host")

        install_ps1 = os.path.join(self.tools_dir, "install.ps1")

        bato_root = os.path.join(self.work_dir, "bato_ps1")
        os.makedirs(os.path.join(bato_root, "userdata", "system"), exist_ok=True)
        bato_bin = os.path.join(self.work_dir, "bin_bato_ps1")
        bato_env = create_mock_ssh_bin(bato_bin, bato_root, rewrite_stdin_and_b64=False, http_port=self.http.port)

        res = subprocess.run(
            [
                PWSH_BIN,
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-File",
                install_ps1,
                "-TargetIp",
                "10.0.0.21",
                "-Action",
                "1",
                "-DmdIp",
                "http://127.0.0.1:8080/",
                "-OsChoice",
                "1",
            ],
            cwd=ROOT_DIR,
            capture_output=True,
            text=True,
            env=bato_env,
            timeout=30,
        )
        self.assertEqual(res.returncode, 0, f"tools/install.ps1 Batocera failed:\n{res.stdout}\n{res.stderr}")

        src_comp = os.path.join(self.tools_dir, "arcadematrix_companion_sync.py")
        expected_comp = (
            read_text(src_comp)
            .replace("{{DMD_HOST}}", "127.0.0.1")
            .replace("{{BROKER}}", "127.0.0.1")
            .replace("{{TOPIC}}", "system/playing/batocera")
            .encode("utf-8")
        )
        deployed_comp = read_bytes(
            os.path.join(bato_root, "userdata", "system", "scripts", "arcadematrix_companion_sync.py")
        )
        self.assertEqual(hashlib.sha256(deployed_comp).hexdigest(), hashlib.sha256(expected_comp).hexdigest())

        for sys_name, rel_marker, deny_root in (
            ("recalbox", os.path.join("recalbox", "share"), False),
            ("retropie", os.path.join("opt", "retropie", "configs", "all"), True),
        ):
            c_root = os.path.join(self.work_dir, f"{sys_name}_ps1")
            os.makedirs(os.path.join(c_root, rel_marker), exist_ok=True)
            c_bin = os.path.join(self.work_dir, f"bin_{sys_name}_ps1")
            c_env = create_mock_ssh_bin(c_bin, c_root, rewrite_stdin_and_b64=True, http_port=self.http.port)
            if deny_root:
                c_env["AM_MOCK_DENY_ROOT"] = "1"

            try:
                r_ins = subprocess.run(
                    [
                        PWSH_BIN,
                        "-NoLogo",
                        "-NoProfile",
                        "-NonInteractive",
                        "-File",
                        install_ps1,
                        "-TargetIp",
                        "10.0.0.22",
                        "-Action",
                        "1",
                        "-DmdIp",
                        "127.0.0.1:8080",
                        "-OsChoice",
                        "1",
                    ],
                    cwd=ROOT_DIR,
                    capture_output=True,
                    text=True,
                    env=c_env,
                    timeout=35,
                )
                self.assertEqual(r_ins.returncode, 0, f"tools/install.ps1 {sys_name} failed:\n{r_ins.stdout}\n{r_ins.stderr}")
                self.assertIn(f"Done! {sys_name} is configured", r_ins.stdout)
            finally:
                stop_sandbox_daemons(c_root)

    # ------------------------------------------------------------------
    # 5. Standalone MUGEN Character Extractor (`mugen_extractor.py`)
    # ------------------------------------------------------------------
    def test_05_standalone_mugen_extractor(self) -> None:
        try:
            from PIL import Image
        except ImportError:
            self.skipTest("Pillow not installed in test Python environment")

        char_dir = os.path.join(self.work_dir, "chars", "ken_e2e")
        os.makedirs(char_dir, exist_ok=True)

        spr = Image.new("P", (16, 24), 0)
        pal = [0, 0, 0] * 256
        pal[3:6] = [230, 40, 40]
        pal[6:9] = [245, 220, 60]
        spr.putpalette(pal)
        for y in range(4, 22):
            for x in range(4, 12):
                spr.putpixel((x, y), 1)
        for x in range(4, 12):
            spr.putpixel((x, 4), 2)
        pcx_buf = io.BytesIO()
        spr.save(pcx_buf, format="PCX")
        pcx_bytes = pcx_buf.getvalue()

        sff_hdr = bytearray(512)
        sff_hdr[0:12] = b"ElecbyteSpr\x00"
        struct.pack_into("<I", sff_hdr, 20, 1)
        struct.pack_into("<I", sff_hdr, 24, 512)
        sub_hdr = struct.pack("<IIhhHHHxB", 0, len(pcx_bytes), 8, 24, 0, 0, 0, 0) + b"\x00" * 12
        write_bytes(os.path.join(char_dir, "ken_e2e.sff"), bytes(sff_hdr) + sub_hdr + pcx_bytes)

        write_text(
            os.path.join(char_dir, "ken_e2e.air"),
            "[Begin Action 0]\n0,0, 0,0, 6\n\n[Begin Action 20]\n0,0, 0,0, 6\n\n[Begin Action 200]\n0,0, 0,0, 5\n",
        )
        write_text(
            os.path.join(char_dir, "ken_e2e.def"),
            "[Info]\nname = \"Ken E2E\"\ndisplayname = \"Ken\"\n[Files]\nsprite = ken_e2e.sff\nanim = ken_e2e.air\n",
        )

        out_fgt_dir = os.path.join(self.work_dir, "fighters_64")
        mugen_py = os.path.join(self.tools_dir, "mugen_extractor", "mugen_extractor.py")
        r_mugen = subprocess.run(
            [
                sys.executable,
                mugen_py,
                "--src",
                os.path.dirname(char_dir),
                "--dest",
                out_fgt_dir,
                "--mode",
                "FULLSIZE",
                "--compress",
                "--workers",
                "1",
            ],
            capture_output=True,
            text=True,
            timeout=30,
        )
        self.assertEqual(r_mugen.returncode, 0, f"mugen_extractor.py failed:\n{r_mugen.stdout}\n{r_mugen.stderr}")
        idx_txt = read_text(os.path.join(out_fgt_dir, "index.txt")).strip()
        self.assertTrue(idx_txt.startswith("ken_e2e,"), f"Unexpected fighters index.txt: {idx_txt}")


if __name__ == "__main__":
    print(f"🔧 Running ArcadeMatrix_RPI Companion Tools E2E Suite (pwsh={PWSH_BIN or 'SKIPPED'})...")
    unittest.main(verbosity=2)

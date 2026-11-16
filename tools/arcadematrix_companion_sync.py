#!/usr/bin/env python3
"""
ArcadeMatrix Console Sync Companion (High Scores & RetroAchievements).

Zero external dependencies (Python 3 standard library only).
Runs on Batocera, Recalbox, and RetroPie alongside the ArcadeMatrix MQTT hook/daemon.

Responsibilities:
1. Resolves active MQTT broker and sync preferences from `GET http://<DMD_HOST>/api/mqtt/broker-target`.
2. Extracts local arcade high scores (`MAME` / `FBNeo` `.hi` files, `.hi.json` sidecars, or `hi2txt`)
   using a built-in binary decoder table for ~60 classic arcade ROMs plus a safe generic BCD/ASCII
   heuristic.
3. Reads RetroAchievements configuration (`batocera.conf`, `recalbox.conf`, `retroarch.cfg`) and
   extracts session summary (`unlocked`, `total`, `points`, `hardcore`, `recent_title`).
4. On `start`:
   - Captures a baseline snapshot of the Top 5 high scores in `/tmp/am_hiscore_baseline.json`.
   - After a 0.35s non-blocking delay (preserving the 0ms Marquee guarantee), publishes an
     opportunistic `{"status": "sync_meta", ...}` payload if high scores or RA data exist.
   - Spawns a lightweight background log watcher tailing RetroArch logs for live `[RCHEEVOS]`
     achievement unlocks and immediately publishes `{"status": "celebrate", "kind": "achievement", ...}`.
5. On `stop`:
   - Terminates the live achievement watcher (`Viewing /tmp/am_companion_watcher.pid`).
   - Waits 0.25s for MAME/FBNeo to flush `.hi` files to disk on emulator exit.
   - Re-extracts high scores and compares the Top 3 against `/tmp/am_hiscore_baseline.json`.
   - If any Top 3 score increased, publishes `{"status": "celebrate", "kind": "highscore", ...}`.
"""

import argparse
import json
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import time
import urllib.parse
import urllib.request

DMD_HOST_DEFAULT = "{{DMD_HOST}}"
TOPIC_DEFAULT = "{{TOPIC}}"

BASELINE_FILE_DEFAULT = "/tmp/am_hiscore_baseline.json"
WATCHER_PID_FILE_DEFAULT = "/tmp/am_companion_watcher.pid"
RA_CACHE_FILE_DEFAULT = "/tmp/am_ra_cache.json"
LIVE_TEST_LOG_DEFAULT = "/tmp/am_rcheevos_live.log"


# ---------------------------------------------------------------------------
# Built-in classic arcade .hi format table (~60 classic ROMs + clones)
# ---------------------------------------------------------------------------
# Format fields:
#   mode: "bcd_be" | "bcd_le" | "ascii"
#   score_bytes: number of bytes per score
#   tag_bytes: number of ASCII bytes for initials
#   tag_first: True if tag precedes score in each record
#   stride: bytes per table entry
#   offset: starting byte offset in .hi file
#   entries: max entries in table
#   mult: score multiplier (e.g. 10 when trailing zero is implied)
_CLASSIC_ROM_FORMATS = {
    # Namco / Midway / Taito / Nintendo classics
    "pacman": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "puckman": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "mspacman": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "galaga": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "dkong": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "dkongjr": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "dkong3": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "frogger": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "digdug": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "xevious": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "rallyx": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "defender": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "robotron": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "joust": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "qbert": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "zaxxon": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "gyruss": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "trackfld": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "timeplt": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "scramble": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "bombjack": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "bubblebobble": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "bublbobl": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "pbobble": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "pbobble2": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    # Capcom CPS1 / CPS2 / Arcade classics
    "1942": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "1943": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "1941": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "19xx": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "commando": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "gng": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "ghouls": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "strider": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "finalfight": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "ffight": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "captcomm": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "knights": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "dino": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "punisher": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "avsp": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "sf2": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "sf2ce": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "sf2hf": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "ssf2": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "ssf2t": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "sfa2": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "sfa3": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    # SNK Neo-Geo MVS/AES classics
    "mslug": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "mslug2": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "mslug3": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "mslug4": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "mslug5": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "mslugx": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "kof98": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "kof2002": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "garou": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "samsho2": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "blazstar": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "pulstar": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "shocktro": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "turfmast": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "windjamm": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    # Sega / Konami / Irem / Seibu / Midway classics
    "outrun": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "shinobi": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "goldnaxe": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "raiden": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "tmnt": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "simpsons": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "xmen": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "ssriders": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "contra": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "rtype": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "ddragon": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "mk": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "mk2": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "nbajam": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "snowbros": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "pang": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
}


def format_score_commas(raw_score):
    """Formats an integer score with thousands separators (e.g. 1245800 -> '1,245,800')."""
    try:
        val = int(raw_score)
        if val < 0:
            val = 0
        return f"{val:,}"
    except Exception:
        return "0"


def _parse_bool(val, default=True):
    if val is None:
        return default
    if isinstance(val, bool):
        return val
    if isinstance(val, (int, float)):
        return bool(val)
    s = str(val).strip().lower()
    if s in ("1", "true", "yes", "on"):
        return True
    if s in ("0", "false", "no", "off"):
        return False
    return default


def resolve_broker_and_prefs(dmd_host=None):
    """
    Queries GET http://<DMD_HOST>/api/mqtt/broker-target on ports 80 and 8080 (timeout 0.8s).
    Returns dict with broker host, port, and sync preferences.
    """
    host_clean = (dmd_host or DMD_HOST_DEFAULT or "").strip()
    if not host_clean or host_clean.startswith("{{"):
        host_clean = os.environ.get("AM_DMD_HOST", "127.0.0.1")

    prefs = {
        "host": host_clean,
        "port": 1883,
        "sync_show_hiscores": _parse_bool(os.environ.get("AM_SYNC_SHOW_HISCORES"), True),
        "sync_show_ra": _parse_bool(os.environ.get("AM_SYNC_SHOW_RA"), True),
        "sync_celebrate_events": _parse_bool(os.environ.get("AM_SYNC_CELEBRATE_EVENTS"), True),
        "ra_username": os.environ.get("AM_RA_USERNAME", "").strip(),
        "ra_web_api_key": os.environ.get("AM_RA_API_KEY", "").strip(),
    }

    if os.environ.get("AM_SKIP_HTTP_RESOLVE") == "1":
        return prefs

    for http_port in (80, 8080):
        url = f"http://{host_clean}:{http_port}/api/mqtt/broker-target"
        try:
            req = urllib.request.Request(url, headers={"Accept": "application/json"})
            with urllib.request.urlopen(req, timeout=0.8) as resp:
                if 200 <= resp.status < 300:
                    data = json.loads(resp.read().decode("utf-8", errors="ignore"))
                    b_host = str(data.get("host") or "").strip()
                    b_port = int(data.get("port") or 1883)
                    if b_host and 0 < b_port <= 65535:
                        prefs["host"] = b_host
                        prefs["port"] = b_port
                    if "sync_show_hiscores" in data:
                        prefs["sync_show_hiscores"] = _parse_bool(data.get("sync_show_hiscores"), True)
                    if "sync_show_ra" in data:
                        prefs["sync_show_ra"] = _parse_bool(data.get("sync_show_ra"), True)
                    if "sync_celebrate_events" in data:
                        prefs["sync_celebrate_events"] = _parse_bool(data.get("sync_celebrate_events"), True)
                    if data.get("ra_username"):
                        prefs["ra_username"] = str(data.get("ra_username")).strip()
                    if data.get("ra_web_api_key"):
                        prefs["ra_web_api_key"] = str(data.get("ra_web_api_key")).strip()
                    return prefs
        except Exception:
            continue

    return prefs


def publish_mqtt_raw(host, port, topic, payload):
    """Publishes an MQTT 3.1.1 QoS 0 packet over a raw TCP socket (zero external dependencies)."""
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

    cid = f"am_comp_{os.getpid()}_{int(time.time() * 1000) % 10000}".encode("utf-8")
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


def publish_mqtt_event(dmd_host, topic, payload_obj, prefs=None):
    """Publishes a dictionary or JSON string payload to the resolved MQTT broker."""
    payload_str = payload_obj if isinstance(payload_obj, str) else json.dumps(payload_obj)

    capture_file = os.environ.get("AM_MQTT_CAPTURE_FILE")
    if capture_file:
        try:
            with open(capture_file, "a", encoding="utf-8") as cf:
                cf.write(payload_str + "\n")
        except Exception:
            pass
        if os.environ.get("AM_MQTT_CAPTURE_ONLY") == "1":
            return True

    if prefs is None:
        prefs = resolve_broker_and_prefs(dmd_host)
    broker_host = prefs.get("host") or dmd_host or "127.0.0.1"
    broker_port = int(prefs.get("port") or 1883)

    mosq_candidates = ["mosquitto_pub", "/usr/bin/mosquitto_pub", "/usr/local/bin/mosquitto_pub", "/bin/mosquitto_pub"]
    for bin_path in mosq_candidates:
        try:
            res = subprocess.run(
                [bin_path, "-h", broker_host, "-p", str(broker_port), "-t", topic, "-m", payload_str],
                timeout=2,
                check=False,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            if res.returncode == 0:
                return True
        except FileNotFoundError:
            continue
        except Exception:
            break

    try:
        publish_mqtt_raw(broker_host, broker_port, topic, payload_str)
        return True
    except Exception:
        return False


# ---------------------------------------------------------------------------
# High-Score Extraction (.hi binary, .hi.json sidecar, hi2txt CLI)
# ---------------------------------------------------------------------------

def decode_bcd_bytes(raw_bytes, little_endian=False, multiplier=1):
    """
    Decodes packed BCD bytes into an integer.
    Returns None if any nibble is invalid (> 9).
    """
    if not raw_bytes:
        return None
    seq = reversed(raw_bytes) if little_endian else raw_bytes
    val = 0
    for b in seq:
        hi = (b >> 4) & 0x0F
        lo = b & 0x0F
        if hi > 9 or lo > 9:
            return None
        val = (val * 100) + (hi * 10) + lo
    return val * multiplier


def decode_unpacked_bcd_bytes(raw_bytes, multiplier=1):
    """
    Decodes unpacked BCD bytes (each byte 0x00..0x09, or 0x30..0x39 ASCII digits,
    with 0x20/0x40/0xFF treated as leading space padding in classic Namco/Nintendo RAM).
    """
    if not raw_bytes:
        return None
    val = 0
    seen_digit = False
    for b in raw_bytes:
        if 0 <= b <= 9:
            val = (val * 10) + b
            seen_digit = True
        elif 0x30 <= b <= 0x39:
            val = (val * 10) + (b - 0x30)
            seen_digit = True
        elif b in (0x20, 0x40, 0xFF) and not seen_digit:
            continue
        else:
            return None
    return (val * multiplier) if seen_digit else None


def _is_valid_tag_bytes(b_slice):
    if not b_slice or len(b_slice) != 3:
        return False
    has_alpha = False
    for b in b_slice:
        if 0x41 <= b <= 0x5A or 0x61 <= b <= 0x7A:
            has_alpha = True
        elif 0x30 <= b <= 0x39 or b in (0x20, 0x2E, 0x21, 0x2D, 0x5F):
            pass
        else:
            return False
    return has_alpha


def _normalize_entries(entries, max_entries=5):
    """Sorts, deduplicates, and normalizes high score entries to top N canonical dicts."""
    valid = []
    seen = set()
    for item in entries:
        try:
            raw = int(item.get("raw", 0))
        except Exception:
            continue
        if raw <= 0 or raw > 999999999:
            continue
        tag = str(item.get("tag") or "1UP").strip().upper()[:3]
        if not tag:
            tag = "1UP"
        key = (raw, tag)
        if key in seen:
            continue
        seen.add(key)
        valid.append({"tag": tag, "raw": raw})

    valid.sort(key=lambda x: x["raw"], reverse=True)
    out = []
    for idx, item in enumerate(valid[:max_entries]):
        out.append({
            "rank": idx + 1,
            "tag": item["tag"],
            "score": format_score_commas(item["raw"]),
            "raw": item["raw"],
        })
    return out


def parse_hi2txt_output(text):
    """
    Parses standard hi2txt tabular output (pipe-delimited or whitespace-delimited)
    into normalized top 5 high score dicts.
    """
    if not text:
        return []
    entries = []
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or line.startswith("-"):
            continue
        upper = line.upper()
        if "RANK" in upper and "SCORE" in upper:
            continue
        parts = [p.strip() for p in re.split(r"[|\t]+|\s{2,}", line) if p.strip()]
        if len(parts) < 2:
            parts = line.split()
        if len(parts) < 2:
            continue

        score_val = None
        tag_val = None
        for idx, tok in enumerate(parts):
            clean_num = tok.replace(",", "").replace(".", "")
            if clean_num.isdigit():
                val = int(clean_num)
                if idx == 0 and val <= 20 and len(parts) >= 2:
                    continue
                if val > 0 and score_val is None:
                    score_val = val
                    continue
            if re.match(r"^[A-Za-z0-9._!-]{1,3}$", tok) and not tok.isdigit():
                if tag_val is None:
                    tag_val = tok.upper()

        if score_val is not None and score_val > 0:
            entries.append({"tag": tag_val or "1UP", "raw": score_val})

    return _normalize_entries(entries)


def parse_hi_binary(data, rom_name=""):
    """
    Decodes a binary .hi buffer using the built-in classic arcade descriptor table
    and a structured BCD/ASCII record scanner fallback.
    """
    if not data:
        return []

    # 0. Check if data is actually JSON or plain text lines
    try:
        text_probe = data.decode("utf-8").strip()
        if text_probe.startswith("{") or text_probe.startswith("["):
            parsed = json.loads(text_probe)
            if isinstance(parsed, dict):
                parsed = parsed.get("hiscores") or parsed.get("top") or []
            if isinstance(parsed, list):
                entries = []
                for row in parsed:
                    if isinstance(row, dict):
                        raw = row.get("raw")
                        if raw is None and row.get("score"):
                            raw = int(str(row["score"]).replace(",", ""))
                        entries.append({"tag": row.get("tag", "1UP"), "raw": raw or 0})
                norm = _normalize_entries(entries)
                if norm:
                    return norm
    except Exception:
        pass

    rom_key = re.sub(r"[^a-z0-9]", "", (rom_name or "").lower())
    fmt = _CLASSIC_ROM_FORMATS.get(rom_key)

    entries = []
    # 1. Try descriptor-guided fixed-stride decoding if file size matches expected layout
    if fmt:
        stride = fmt["stride"]
        offset = fmt["offset"]
        s_len = fmt["score_bytes"]
        t_len = fmt["tag_bytes"]
        mult = fmt.get("mult", 1)
        le = fmt["mode"] == "bcd_le"

        for i in range(fmt["entries"]):
            pos = offset + (i * stride)
            if pos + s_len + t_len > len(data):
                break
            if fmt.get("tag_first"):
                t_slice = data[pos : pos + t_len]
                s_slice = data[pos + t_len : pos + t_len + s_len]
            else:
                s_slice = data[pos : pos + s_len]
                t_slice = data[pos + s_len : pos + s_len + t_len]

            if _is_valid_tag_bytes(t_slice):
                score = decode_bcd_bytes(s_slice, little_endian=le, multiplier=mult)
                if score and score > 0:
                    tag = t_slice.decode("ascii", errors="ignore").strip().upper() or "1UP"
                    entries.append({"tag": tag, "raw": score})

        norm = _normalize_entries(entries)
        if norm:
            return norm

    # 2. Structured scanner for (BCD/ASCII score + 3-char ASCII tag) or (3-char tag + BCD/ASCII score)
    # Prevents ASCII tag bytes from overlapping with BCD score bytes.
    n = len(data)
    i = 0
    consumed = [False] * n

    while i <= n - 3:
        t_slice = data[i : i + 3]
        if _is_valid_tag_bytes(t_slice):
            tag = t_slice.decode("ascii", errors="ignore").strip().upper()
            matched_score = None
            consume_end = i + 3

            # Check 3 or 4 bytes immediately BEFORE the tag (with optional 1 pad byte)
            for pad in (0, 1):
                for s_len in (4, 3):
                    s_start = i - pad - s_len
                    s_end = i - pad
                    if s_start >= 0 and not any(consumed[s_start:i]):
                        s_slice = data[s_start:s_end]
                        # Ensure score slice is not all printable uppercase ASCII letters
                        if not _is_valid_tag_bytes(s_slice[:3]):
                            val = decode_bcd_bytes(s_slice, little_endian=False)
                            if val is not None and 100 <= val <= 999999999:
                                matched_score = val
                                for k in range(s_start, i + 3):
                                    consumed[k] = True
                                break
                if matched_score is not None:
                    break

            # If not found before tag, check 3 or 4 bytes immediately AFTER the tag (with optional 1 pad byte)
            if matched_score is None:
                for pad in (0, 1):
                    for s_len in (4, 3):
                        s_start = i + 3 + pad
                        s_end = s_start + s_len
                        if s_end <= n and not any(consumed[i:s_end]):
                            s_slice = data[s_start:s_end]
                            if not _is_valid_tag_bytes(s_slice[:3]):
                                val = decode_bcd_bytes(s_slice, little_endian=False)
                                if val is not None and 100 <= val <= 999999999:
                                    matched_score = val
                                    for k in range(i, s_end):
                                        consumed[k] = True
                                    consume_end = s_end
                                    break
                    if matched_score is not None:
                        break

            if matched_score is not None:
                entries.append({"tag": tag, "raw": matched_score})
                i = consume_end
                continue
        i += 1

    norm = _normalize_entries(entries)
    if norm:
        return norm

    # 3. ASCII text fallback (e.g. "1245800 RED" or "RED 1245800")
    try:
        ascii_text = data.decode("latin-1", errors="ignore")
        for m in re.finditer(r"(?:([A-Z]{3})\s+(\d{3,9})|(\d{3,9})\s+([A-Z]{3}))", ascii_text):
            if m.group(1) and m.group(2):
                entries.append({"tag": m.group(1), "raw": int(m.group(2))})
            elif m.group(3) and m.group(4):
                entries.append({"tag": m.group(4), "raw": int(m.group(3))})
        norm = _normalize_entries(entries)
        if norm:
            return norm
    except Exception:
        pass

    # 4. Single top-score BCD fallback (e.g. 4-byte packed BCD or 6-byte unpacked BCD with no tag, like pacman.hi)
    if 3 <= len(data) <= 16:
        for s_len in (4, 3):
            if len(data) >= s_len:
                val = decode_bcd_bytes(data[:s_len], little_endian=False)
                if val is not None and 100 <= val <= 99999999:
                    return _normalize_entries([{"tag": "1UP", "raw": val}])
        u_val = decode_unpacked_bcd_bytes(data[: min(len(data), 8)])
        if u_val is not None and 100 <= u_val <= 99999999:
            return _normalize_entries([{"tag": "1UP", "raw": u_val}])

    return []


def get_hiscore_candidate_paths(rom_name, rom_path=None):
    """Returns ordered candidate paths for <rom>.hi and <rom>.hi.json across Batocera, Recalbox, and RetroPie."""
    if not rom_name:
        return []
    candidates = []

    env_dir = os.environ.get("AM_HISCORE_DIR")
    if env_dir:
        candidates.extend([
            os.path.join(env_dir, f"{rom_name}.hi.json"),
            os.path.join(env_dir, f"{rom_name}.hi"),
            os.path.join(env_dir, "hi", f"{rom_name}.hi.json"),
            os.path.join(env_dir, "hi", f"{rom_name}.hi"),
            os.path.join(env_dir, "nvram", rom_name, f"{rom_name}.hi"),
        ])

    if rom_path:
        rom_dir = os.path.dirname(str(rom_path))
        if rom_dir:
            candidates.extend([
                os.path.join(rom_dir, "hi", f"{rom_name}.hi"),
                os.path.join(rom_dir, f"{rom_name}.hi"),
            ])

    # Batocera, Recalbox, and RetroPie standard locations
    base_dirs = [
        # Batocera
        "/userdata/saves/mame/hi",
        "/userdata/saves/fbneo",
        "/userdata/saves/fbneo/hi",
        "/userdata/saves/mame/MAME 0.78 Plus/hi",
        "/userdata/saves/mame/mame2003-plus/hi",
        "/userdata/bios/mame2003-plus/hi",
        # Recalbox
        "/recalbox/share/saves/mame/hi",
        "/recalbox/share/saves/fbneo",
        "/recalbox/share/saves/fbneo/hi",
        "/recalbox/share/saves/mame/mame2003-plus/hi",
        "/recalbox/share/saves/mame/MAME 0.78 Plus/hi",
        "/recalbox/share/bios/mame2003-plus/hi",
        # RetroPie
        "/home/pi/RetroPie/roms/arcade/hi",
        "/home/pi/RetroPie/roms/mame-libretro/hi",
        "/home/pi/RetroPie/roms/fba/hi",
        "/home/pi/RetroPie/BIOS/mame2003-plus/hi",
        "/opt/retropie/configs/all/retroarch/saves/mame2003-plus/hi",
        "/opt/retropie/configs/all/retroarch/saves/MAME 0.78 Plus/hi",
        "/opt/retropie/configs/all/retroarch/saves/FinalBurn Neo/fbneo/hi",
    ]
    for d in base_dirs:
        candidates.append(os.path.join(d, f"{rom_name}.hi.json"))
        candidates.append(os.path.join(d, f"{rom_name}.hi"))

    nvram_roots = []
    if env_dir:
        nvram_roots.append(os.path.join(env_dir, "nvram", rom_name))
    nvram_roots.extend([
        f"/userdata/saves/mame/nvram/{rom_name}",
        f"/userdata/saves/fbneo/nvram/{rom_name}",
        f"/recalbox/share/saves/mame/nvram/{rom_name}",
        f"/recalbox/share/saves/fbneo/nvram/{rom_name}",
        f"/home/pi/RetroPie/roms/arcade/nvram/{rom_name}",
    ])
    for nv_dir in nvram_roots:
        if os.path.isdir(nv_dir):
            try:
                for fn in sorted(os.listdir(nv_dir)):
                    full_nv = os.path.join(nv_dir, fn)
                    if os.path.isfile(full_nv):
                        candidates.append(full_nv)
            except Exception:
                pass

    return candidates


def extract_hiscores(rom_name, rom_path=None):
    """
    Searches candidate paths for <rom>.hi / <rom>.hi.json.
    Tries hi2txt CLI first if available, then falls back to built-in binary decoder.
    Returns top 5 list: [{"rank": 1, "tag": "RED", "score": "1,245,800", "raw": 1245800}, ...]
    """
    if not rom_name:
        return []

    hi2txt_bin = shutil.which("hi2txt")
    for path in get_hiscore_candidate_paths(rom_name, rom_path):
        if not os.path.isfile(path):
            continue

        if hi2txt_bin and path.endswith(".hi"):
            try:
                res = subprocess.run(
                    [hi2txt_bin, "-r", path],
                    timeout=1.5,
                    check=False,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.DEVNULL,
                )
                if res.returncode == 0 and res.stdout:
                    parsed = parse_hi2txt_output(res.stdout.decode("utf-8", errors="ignore"))
                    if parsed:
                        return parsed
            except Exception:
                pass

        try:
            with open(path, "rb") as f:
                data = f.read(65536)
            parsed = parse_hi_binary(data, rom_name=rom_name)
            if parsed:
                return parsed
        except Exception:
            continue

    return []


def save_hiscore_baseline(system, game, hiscores, rom_path=None, baseline_file=None):
    """Saves the baseline top scores snapshot at gameStart to /tmp/am_hiscore_baseline.json."""
    target = baseline_file or os.environ.get("AM_BASELINE_FILE") or BASELINE_FILE_DEFAULT
    payload = {
        "system": system or "",
        "game": game or "",
        "rom_path": rom_path or "",
        "ts": time.time(),
        "top": hiscores[:5] if hiscores else [],
    }
    try:
        with open(target, "w", encoding="utf-8") as f:
            json.dump(payload, f)
    except Exception:
        pass


def load_hiscore_baseline(baseline_file=None):
    """Loads the baseline high score snapshot from /tmp/am_hiscore_baseline.json."""
    target = baseline_file or os.environ.get("AM_BASELINE_FILE") or BASELINE_FILE_DEFAULT
    try:
        with open(target, "r", encoding="utf-8") as f:
            return json.load(f)
    except Exception:
        return None


def detect_new_highscore(baseline_top, current_top, system, game):
    """
    Compares the new Top 3 scores against the baseline Top 3.
    If any rank 1..3 has a strictly higher raw score than the baseline at that rank,
    returns the celebration payload dict for the highest beaten rank; otherwise None.
    """
    if not current_top:
        return None

    old_list = baseline_top if isinstance(baseline_top, list) else []
    for idx in range(min(3, len(current_top))):
        entry = current_top[idx]
        new_raw = int(entry.get("raw", 0))
        if new_raw <= 0:
            continue
        old_raw = 0
        if idx < len(old_list):
            try:
                old_raw = int(old_list[idx].get("raw", 0))
            except Exception:
                old_raw = 0

        if new_raw > old_raw:
            return {
                "status": "celebrate",
                "kind": "highscore",
                "system": str(system or ""),
                "game": str(game or ""),
                "rank": idx + 1,
                "tag": str(entry.get("tag") or "1UP")[:3],
                "score": str(entry.get("score") or format_score_commas(new_raw)),
            }

    return None


# ---------------------------------------------------------------------------
# RetroAchievements Config, Summary Extractor & Live Log Watcher
# ---------------------------------------------------------------------------

def read_ra_config(prefs=None):
    """
    Reads RetroAchievements settings from Batocera, Recalbox, or RetroPie config files,
    merged with optional DMD preferences.
    """
    cfg = {
        "enabled": False,
        "username": "",
        "token": "",
        "hardcore": False,
        "web_api_key": "",
    }
    if prefs:
        if prefs.get("ra_username"):
            cfg["username"] = str(prefs["ra_username"]).strip()
        if prefs.get("ra_web_api_key"):
            cfg["web_api_key"] = str(prefs["ra_web_api_key"]).strip()

    conf_candidates = []
    if os.environ.get("AM_RA_CONF_FILE"):
        conf_candidates.append(os.environ["AM_RA_CONF_FILE"])
    conf_candidates.extend([
        "/userdata/system/batocera.conf",
        "/recalbox/share/system/recalbox.conf",
        "/opt/retropie/configs/all/retroarch.cfg",
    ])

    for path in conf_candidates:
        if not os.path.isfile(path):
            continue
        try:
            with open(path, "r", encoding="utf-8", errors="ignore") as f:
                for raw_line in f:
                    line = raw_line.strip()
                    if not line or line.startswith("#") or line.startswith(";") or "=" not in line:
                        continue
                    k, v = line.split("=", 1)
                    k = k.strip().lower()
                    v = v.strip().strip('"').strip("'")
                    if k in ("global.retroachievements", "cheevos_enable"):
                        cfg["enabled"] = _parse_bool(v, False)
                    elif k in ("global.retroachievements.username", "cheevos_username") and v:
                        if not cfg["username"]:
                            cfg["username"] = v
                    elif k in (
                        "global.retroachievements.token",
                        "global.retroachievements.password",
                        "cheevos_token",
                        "cheevos_password",
                    ) and v:
                        cfg["token"] = v
                    elif k in ("global.retroachievements.hardcore", "cheevos_hardcore_mode_enable"):
                        cfg["hardcore"] = _parse_bool(v, False)
        except Exception:
            continue

    if cfg["username"] and cfg["web_api_key"]:
        cfg["enabled"] = True

    return cfg


def get_rcheevos_log_paths():
    """Returns candidate RetroArch / EmulationStation launcher log files."""
    paths = []
    if os.environ.get("AM_RCHEEVOS_LOG_FILE"):
        paths.append(os.environ["AM_RCHEEVOS_LOG_FILE"])
    paths.extend([
        LIVE_TEST_LOG_DEFAULT,
        "/userdata/system/logs/es_launch_stdout.log",
        "/userdata/system/logs/es_launch_stderr.log",
        "/tmp/retroarch.log",
        "/recalbox/share/system/logs/es_launch_stdout.log",
        "/recalbox/share/system/logs/retroarch.log",
        "/dev/shm/runcommand.log",
    ])
    return paths


def parse_rcheevos_summary_from_logs(log_text, default_hardcore=False):
    """
    Extracts RA progress summary from RetroArch [RCHEEVOS] log lines if present.
    Returns {"unlocked": int, "total": int, "points": int, "hardcore": bool, ...} or None.
    """
    if not log_text:
        return None
    unlocked = None
    total = None
    points = 0
    hardcore = bool(default_hardcore)
    recent_title = ""

    for raw_line in log_text.splitlines():
        if "RCHEEVOS" not in raw_line and "Achievement" not in raw_line:
            continue
        if re.search(r"hardcore\s*(?:mode\s*enabled|=\s*1|=\s*true)", raw_line, re.IGNORECASE):
            hardcore = True
        elif re.search(r"hardcore\s*(?:mode\s*disabled|=\s*0|=\s*false)", raw_line, re.IGNORECASE):
            hardcore = False

        # e.g. "[RCHEEVOS]: You have 18 of 45 achievements unlocked (240 points)"
        # or "[RCHEEVOS]: Unlocked 18/45 achievements (240 pts)"
        m = re.search(
            r"(?:have\s+|unlocked\s+)(\d+)\s*(?:of|/)\s*(\d+)\s+achievements?(?:.*?(\d+)\s*(?:points|pts))?",
            raw_line,
            re.IGNORECASE,
        )
        if m:
            unlocked = int(m.group(1))
            total = int(m.group(2))
            if m.group(3):
                points = int(m.group(3))

        unlock_evt = parse_rcheevos_unlock_line(raw_line, default_hardcore=hardcore)
        if unlock_evt:
            recent_title = unlock_evt.get("title", "")

    if total is not None and total > 0:
        out = {
            "unlocked": unlocked or 0,
            "total": total,
            "points": points,
            "hardcore": hardcore,
        }
        if recent_title:
            out["recent_title"] = recent_title
        return out
    return None


def parse_rcheevos_unlock_line(line, system="", game="", default_hardcore=False):
    """
    Parses a single RetroArch log line for a live achievement unlock event.
    Matches patterns like:
      - [RCHEEVOS]: Awarded achievement 12345: "Dragon Coin Collector" (10 points, hardcore)
      - [RCHEEVOS]: Awarded achievement: Dragon Coin Collector (10 points)
      - [RCHEEVOS]: Achievement Unlocked: Dragon Coin Collector (10 pts, hardcore)
    Returns a celebration dict or None.
    """
    if not line:
        return None
    if "Awarded achievement" not in line and "Achievement Unlocked" not in line and "Unlocking achievement" not in line:
        return None

    hardcore = bool(default_hardcore)
    lower = line.lower()
    if "hardcore" in lower:
        if any(neg in lower for neg in ("hardcore=0", "hardcore: false", "hardcore=false", "softcore")):
            hardcore = False
        else:
            hardcore = True
    elif "softcore" in lower:
        hardcore = False

    points = 0
    pts_match = re.search(r"\((\d+)\s*(?:points?|pts?)", line, re.IGNORECASE) or re.search(
        r"\[(\d+)\s*(?:points?|pts?)\]", line, re.IGNORECASE
    )
    if pts_match:
        points = int(pts_match.group(1))

    # Extract title after "Awarded achievement [id]:" or "Achievement Unlocked:"
    title = ""
    m = re.search(
        r"(?:Awarded achievement(?:\s+\d+)?|Achievement Unlocked|Unlocking achievement(?:\s+\d+)?)\s*:\s*(.+)$",
        line,
        re.IGNORECASE,
    )
    if m:
        raw_tail = m.group(1).strip()
        # Strip trailing "(10 points...)" or "[10 pts...]"
        raw_tail = re.sub(r"\s*[\(\[]\d+\s*(?:points?|pts?).*[\)\]]\s*$", "", raw_tail, flags=re.IGNORECASE)
        raw_tail = re.sub(r"\s*\((?:hardcore|softcore)\)\s*$", "", raw_tail, flags=re.IGNORECASE)
        title = raw_tail.strip().strip('"').strip("'").strip()

    if not title:
        return None

    return {
        "status": "celebrate",
        "kind": "achievement",
        "system": str(system or ""),
        "game": str(game or ""),
        "title": title,
        "points": points,
        "hardcore": hardcore,
    }


def extract_ra_summary(system, game, prefs=None):
    """
    Resolves RetroAchievements summary for the active game from cache or RetroArch logs.
    Returns dict {"unlocked": int, "total": int, "points": int, "hardcore": bool, ...} or None.
    """
    ra_cfg = read_ra_config(prefs)
    cache_path = os.environ.get("AM_RA_CACHE_FILE") or RA_CACHE_FILE_DEFAULT

    if os.path.isfile(cache_path):
        try:
            with open(cache_path, "r", encoding="utf-8") as f:
                cache_data = json.load(f)
            if isinstance(cache_data, dict):
                entry = (
                    cache_data.get(f"{system}/{game}")
                    or cache_data.get(str(game))
                    or (cache_data if "total" in cache_data else None)
                )
                if isinstance(entry, dict) and int(entry.get("total", 0)) > 0:
                    out = {
                        "unlocked": int(entry.get("unlocked", 0)),
                        "total": int(entry.get("total", 0)),
                        "points": int(entry.get("points", 0)),
                        "hardcore": bool(entry.get("hardcore", ra_cfg["hardcore"])),
                    }
                    if entry.get("recent_title"):
                        out["recent_title"] = str(entry["recent_title"])
                    return out
        except Exception:
            pass

    detected_game_id = None
    for log_path in get_rcheevos_log_paths():
        if not os.path.isfile(log_path):
            continue
        try:
            with open(log_path, "r", encoding="utf-8", errors="ignore") as f:
                content = f.read(65536)
            summary = parse_rcheevos_summary_from_logs(content, default_hardcore=ra_cfg["hardcore"])
            if summary and summary.get("total", 0) > 0:
                return summary
            gid_m = re.search(
                r"(?:Identified game|Fetching data for game|Game)\s*:?\s*(\d{2,9})",
                content,
                re.IGNORECASE,
            )
            if gid_m and not detected_game_id:
                detected_game_id = int(gid_m.group(1))
        except Exception:
            continue

    if ra_cfg.get("username") and ra_cfg.get("web_api_key") and detected_game_id:
        try:
            params = urllib.parse.urlencode({
                "z": ra_cfg["username"],
                "y": ra_cfg["web_api_key"],
                "u": ra_cfg["username"],
                "g": str(detected_game_id),
            })
            api_url = f"https://retroachievements.org/API/API_GetGameInfoAndUserProgress.php?{params}"
            req = urllib.request.Request(api_url, headers={"Accept": "application/json", "User-Agent": "ArcadeMatrix/1.0"})
            with urllib.request.urlopen(req, timeout=1.2) as resp:
                if 200 <= resp.status < 300:
                    data = json.loads(resp.read().decode("utf-8", errors="ignore"))
                    total = int(data.get("NumAchievements") or 0)
                    if total > 0:
                        hc = bool(ra_cfg.get("hardcore", False))
                        unlocked = int(
                            data.get("NumAwardedToUserHardcore" if hc else "NumAwardedToUser")
                            or data.get("NumAwardedToUser")
                            or 0
                        )
                        return {
                            "unlocked": unlocked,
                            "total": total,
                            "points": int(data.get("UserTotalPoints") or 0),
                            "hardcore": hc,
                        }
        except Exception:
            pass

    return None


def build_sync_meta_payload(system, game, hiscores=None, ra_summary=None):
    """
    Builds the opportunistic `sync_meta` payload.
    Returns None if neither hiscores nor ra_summary (with total > 0) is available.
    """
    has_hi = bool(hiscores and len(hiscores) > 0)
    has_ra = bool(ra_summary and int(ra_summary.get("total", 0)) > 0)
    if not has_hi and not has_ra:
        return None

    payload = {
        "status": "sync_meta",
        "system": str(system or ""),
        "game": str(game or ""),
    }
    if has_hi:
        payload["hiscores"] = [
            {
                "rank": int(h["rank"]),
                "tag": str(h["tag"])[:3],
                "score": str(h["score"]),
            }
            for h in hiscores[:5]
        ]
    if has_ra:
        ra_obj = {
            "unlocked": int(ra_summary.get("unlocked", 0)),
            "total": int(ra_summary.get("total", 0)),
            "points": int(ra_summary.get("points", 0)),
            "hardcore": bool(ra_summary.get("hardcore", False)),
        }
        if ra_summary.get("recent_title"):
            ra_obj["recent_title"] = str(ra_summary["recent_title"])
        payload["ra"] = ra_obj

    return payload


# ---------------------------------------------------------------------------
# Watcher Process Control & Lifecycle (start / stop)
# ---------------------------------------------------------------------------

def stop_live_watcher(pid_file=None):
    """Stops any running live achievement watcher via its PID file."""
    target = pid_file or os.environ.get("AM_WATCHER_PID_FILE") or WATCHER_PID_FILE_DEFAULT
    if not os.path.isfile(target):
        return
    try:
        with open(target, "r", encoding="utf-8") as f:
            pid_str = f.read().strip()
        if pid_str.isdigit():
            pid = int(pid_str)
            if pid > 0 and pid != os.getpid():
                os.kill(pid, signal.SIGTERM)
    except Exception:
        pass
    try:
        os.remove(target)
    except Exception:
        pass


def run_live_watcher_loop(dmd_host, topic, system, game, prefs, max_duration_sec=43200):
    """
    Tails candidate RetroArch log files while the game is running and immediately publishes
    `{"status": "celebrate", "kind": "achievement", ...}` on `[RCHEEVOS]` unlock lines.
    """
    pid_file = os.environ.get("AM_WATCHER_PID_FILE") or WATCHER_PID_FILE_DEFAULT
    my_pid = os.getpid()
    try:
        with open(pid_file, "w", encoding="utf-8") as f:
            f.write(str(my_pid))
    except Exception:
        return

    ra_cfg = read_ra_config(prefs)
    default_hc = ra_cfg.get("hardcore", False)
    log_paths = get_rcheevos_log_paths()
    offsets = {}
    for p in log_paths:
        try:
            if os.path.isfile(p):
                offsets[p] = os.path.getsize(p)
        except Exception:
            offsets[p] = 0

    seen_titles = set()
    start_ts = time.time()

    running = True
    def _handle_sig(_signum, _frame):
        nonlocal running
        running = False

    signal.signal(signal.SIGTERM, _handle_sig)
    signal.signal(signal.SIGINT, _handle_sig)

    while running and (time.time() - start_ts) < max_duration_sec:
        try:
            if not os.path.isfile(pid_file):
                break
            with open(pid_file, "r", encoding="utf-8") as pf:
                if pf.read().strip() != str(my_pid):
                    break
        except Exception:
            break

        for p in log_paths:
            if not os.path.isfile(p):
                continue
            try:
                cur_size = os.path.getsize(p)
                prev_off = offsets.get(p, 0)
                if cur_size < prev_off:
                    prev_off = 0
                if cur_size > prev_off:
                    with open(p, "r", encoding="utf-8", errors="ignore") as lf:
                        lf.seek(prev_off)
                        chunk = lf.read(cur_size - prev_off)
                        offsets[p] = lf.tell()
                    for line in chunk.splitlines():
                        evt = parse_rcheevos_unlock_line(
                            line, system=system, game=game, default_hardcore=default_hc
                        )
                        if evt and evt["title"] not in seen_titles:
                            seen_titles.add(evt["title"])
                            publish_mqtt_event(dmd_host, topic, evt, prefs=prefs)
            except Exception:
                continue

        time.sleep(0.25)


def handle_start(dmd_host, topic, system, game, rom_path=None, title=None, watch=True, delay_sec=0.35):
    """
    Executes the `start` workflow:
    1. Stops any previous live watcher.
    2. Captures high score baseline to `/tmp/am_hiscore_baseline.json`.
    3. Waits `delay_sec` (0.35s default) so the initial `playing` message is never delayed.
    4. Publishes `sync_meta` if high scores or RA data are present.
    5. Runs the live achievement log watcher loop if `watch` and `sync_celebrate_events` are enabled.
    """
    stop_live_watcher()
    prefs = resolve_broker_and_prefs(dmd_host)

    hiscores = []
    if prefs.get("sync_show_hiscores", True) or prefs.get("sync_celebrate_events", True):
        hiscores = extract_hiscores(game, rom_path=rom_path)
        save_hiscore_baseline(system, game, hiscores, rom_path=rom_path)

    if delay_sec > 0:
        time.sleep(delay_sec)

    pub_hi = hiscores if prefs.get("sync_show_hiscores", True) else []
    pub_ra = extract_ra_summary(system, game, prefs=prefs) if prefs.get("sync_show_ra", True) else None

    meta_payload = build_sync_meta_payload(system, game, hiscores=pub_hi, ra_summary=pub_ra)
    if meta_payload:
        publish_mqtt_event(dmd_host, topic, meta_payload, prefs=prefs)

    if watch and prefs.get("sync_celebrate_events", True):
        run_live_watcher_loop(dmd_host, topic, system, game, prefs)


def handle_stop(dmd_host, topic, system=None, game=None, rom_path=None, delay_sec=0.25):
    """
    Executes the `stop` workflow:
    1. Immediately terminates any active live achievement watcher.
    2. Waits `delay_sec` (0.25s default) for MAME/FBNeo to finish flushing `.hi` on exit.
    3. Compares re-extracted Top 3 high scores against `/tmp/am_hiscore_baseline.json`.
    4. Publishes `{"status": "celebrate", "kind": "highscore", ...}` if beaten.
    """
    stop_live_watcher()
    baseline = load_hiscore_baseline()

    sys_name = system or (baseline.get("system") if baseline else "") or ""
    game_name = game or (baseline.get("game") if baseline else "") or ""
    r_path = rom_path or (baseline.get("rom_path") if baseline else "") or ""

    if not game_name:
        return None

    if delay_sec > 0:
        time.sleep(delay_sec)

    prefs = resolve_broker_and_prefs(dmd_host)
    if not prefs.get("sync_celebrate_events", True):
        return None

    current_hi = extract_hiscores(game_name, rom_path=r_path)
    old_top = baseline.get("top", []) if isinstance(baseline, dict) else []

    celebrate_payload = detect_new_highscore(old_top, current_hi, sys_name, game_name)
    if celebrate_payload:
        publish_mqtt_event(dmd_host, topic, celebrate_payload, prefs=prefs)

    # Clean up baseline file after evaluation
    target = os.environ.get("AM_BASELINE_FILE") or BASELINE_FILE_DEFAULT
    try:
        if os.path.isfile(target):
            os.remove(target)
    except Exception:
        pass

    return celebrate_payload


def main(argv=None):
    parser = argparse.ArgumentParser(description="ArcadeMatrix Console Sync Companion")
    parser.add_argument("action", choices=["start", "stop"], help="Lifecycle event action")
    parser.add_argument("--dmd-host", default=DMD_HOST_DEFAULT, help="ArcadeMatrix DMD IP or hostname")
    parser.add_argument("--topic", default=TOPIC_DEFAULT, help="MQTT topic (e.g. system/playing/batocera)")
    parser.add_argument("--system", default="", help="System identifier (e.g. neogeo, mame, snes)")
    parser.add_argument("--game", default="", help="ROM basename without extension (e.g. mslug)")
    parser.add_argument("--rom-path", default="", help="Full path to the ROM file")
    parser.add_argument("--title", default="", help="Optional human-readable game title")
    parser.add_argument("--no-watch", action="store_true", help="Exit immediately after publishing sync_meta")
    args = parser.parse_args(argv)

    if args.action == "start":
        handle_start(
            dmd_host=args.dmd_host,
            topic=args.topic,
            system=args.system,
            game=args.game,
            rom_path=args.rom_path,
            title=args.title,
            watch=not args.no_watch,
        )
    elif args.action == "stop":
        handle_stop(
            dmd_host=args.dmd_host,
            topic=args.topic,
            system=args.system,
            game=args.game,
            rom_path=args.rom_path,
        )


if __name__ == "__main__":
    main()

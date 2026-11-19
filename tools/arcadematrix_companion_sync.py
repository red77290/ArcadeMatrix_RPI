#!/usr/bin/env python3
"""
ArcadeMatrix Console Sync Companion (High Scores & RetroAchievements).

Zero external dependencies (Python 3 standard library only).
Runs on Batocera, Recalbox, and RetroPie alongside the ArcadeMatrix MQTT hook/daemon.

Responsibilities:
1. Resolves active MQTT broker and sync preferences from `GET http://<DMD_HOST>/api/mqtt/broker-target`.
2. Extracts local arcade high scores (`MAME` / `FBNeo` `.hi` files, `.hi.json` sidecars, `nvram`/`.nv`, or `hi2txt`)
   using a built-in per-ROM format specification table (~65 classic arcade ROMs + Namco/Nintendo tile formats)
   backed by multi-stage BCD/ASCII/word-swapped heuristics when no spec matches.
3. Reads RetroAchievements configuration (`batocera.conf`, `recalbox.conf`, `retroarchcustom.cfg`, `retroarch.cfg`)
   and extracts session summary (`unlocked`, `total`, `points`, `hardcore`, `recent_title`) from localized
   EN/FR/ES RetroArch `[RCHEEVOS]` logs or the RetroAchievements Web API (`API_GetGameInfoAndUserProgress` &
   `API_GetUserRecentlyPlayedGames`).
4. On `start`:
   - Captures a baseline snapshot of the Top 5 high scores in `/tmp/am_hiscore_baseline.json`.
   - After a 0.35s non-blocking delay (preserving the 0ms Marquee guarantee), publishes an
     opportunistic `{"status": "sync_meta", ...}` payload if high scores or RA data exist.
   - Spawns a lightweight background log watcher tailing RetroArch logs for delayed `[RCHEEVOS]`
     startup summaries (1.5s..12s after launch) and live `[RCHEEVOS]` achievement unlocks,
     immediately publishing `{"status": "celebrate", "kind": "achievement", ...}`.
5. On `stop`:
   - Terminates the live achievement watcher (`/tmp/am_companion_watcher.pid`).
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
# Built-in classic arcade .hi format specification table (~65 classic ROMs)
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
#   alt_encoding: optional "namco_tile" | "dkong_record" secondary MAME hiscore.dat decoder
_CLASSIC_ROM_FORMATS = {
    # Namco / Midway / Taito / Nintendo classics
    "pacman": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "puckman": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "mspacman": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "galaga": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1, "alt_encoding": "namco_tile"},
    "galagao": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1, "alt_encoding": "namco_tile"},
    "galagamw": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1, "alt_encoding": "namco_tile"},
    "dkong": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1, "alt_encoding": "dkong_record"},
    "dkongjr": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1, "alt_encoding": "dkong_record"},
    "dkong3": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1, "alt_encoding": "dkong_record"},
    "mario": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1, "alt_encoding": "dkong_record"},
    "frogger": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1},
    "digdug": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1, "alt_encoding": "namco_tile"},
    "digdug2": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1, "alt_encoding": "namco_tile"},
    "xevious": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1, "alt_encoding": "namco_tile"},
    "mappy": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1, "alt_encoding": "namco_tile"},
    "bosconian": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1, "alt_encoding": "namco_tile"},
    "polepos": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1, "alt_encoding": "namco_tile"},
    "pacland": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1, "alt_encoding": "namco_tile"},
    "rallyx": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 1, "alt_encoding": "namco_tile"},
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
# High-Score Extraction (.hi binary, .hi.json sidecar, NVRAM, hi2txt CLI)
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
    with 0x10/0x20/0x24/0x40/0xFF treated as leading space padding in Namco/Nintendo RAM).
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
        elif b in (0x10, 0x20, 0x24, 0x40, 0xFF) and not seen_digit:
            continue
        else:
            return None
    return (val * multiplier) if seen_digit else None


def _is_valid_tag_bytes(b_slice, strict_upper=False):
    if not b_slice or len(b_slice) != 3:
        return False
    has_alpha = False
    for b in b_slice:
        if 0x41 <= b <= 0x5A:
            has_alpha = True
        elif not strict_upper and (0x61 <= b <= 0x7A):
            has_alpha = True
        elif 0x30 <= b <= 0x39 or b in (0x20, 0x2E, 0x21, 0x2D, 0x5F):
            pass
        else:
            return False
    return has_alpha


def _decode_namco_tile_tag(b_slice):
    """
    Decodes a 3-byte Namco tile-coded player tag (0x0A..0x23 -> A..Z, 0x24 -> space, 0x2A -> '.').
    """
    if not b_slice or len(b_slice) != 3:
        return None
    chars = []
    has_alpha = False
    for b in b_slice:
        if 0x0A <= b <= 0x23:
            chars.append(chr(ord("A") + (b - 0x0A)))
            has_alpha = True
        elif 0 <= b <= 9:
            chars.append(chr(ord("0") + b))
        elif b in (0x24, 0x20, 0x40, 0xFF):
            chars.append(" ")
        elif b in (0x2A, 0x2B):
            chars.append(".")
        else:
            return None
    tag = "".join(chars).strip()
    return tag if (has_alpha and tag) else None


def _decode_nintendo_tile_tag(b_slice):
    """
    Decodes a 3-byte Nintendo (Donkey Kong / Mario Bros.) tile-coded tag
    (0x11..0x2A -> A..Z, 0x00..0x09 -> 0..9, 0x10 -> space, 0x2C -> '.').
    """
    if not b_slice or len(b_slice) != 3:
        return None
    chars = []
    has_alpha = False
    for b in b_slice:
        if 0x11 <= b <= 0x2A:
            chars.append(chr(ord("A") + (b - 0x11)))
            has_alpha = True
        elif 0 <= b <= 9:
            chars.append(chr(ord("0") + b))
        elif b in (0x10, 0x20, 0xFF):
            chars.append(" ")
        elif b in (0x2B, 0x2C):
            chars.append(".")
        else:
            return None
    tag = "".join(chars).strip()
    return tag if (has_alpha and tag) else None


def _decode_namco_tile_hi(data):
    """
    Decodes classic Namco tile-mapped .hi buffers (e.g. galaga.hi, digdug.hi, xevious.hi, mappy.hi):
    - Split layout: 5 x 6-byte unpacked scores followed by 5 x 3-byte Namco tile tags (45..64 bytes),
      or 5 x 6-byte unpacked scores alone (30..36 bytes).
    - Interleaved layout: 5 x (6-byte score + 3-byte tag) records.
    """
    n = len(data)
    if n < 15 or n > 256:
        return []

    entries = []
    # 1. Split layout: 5 x 6-byte scores at offset 0, 5 x 3-byte tags at offset 30
    if n >= 45:
        split_ok = True
        split_entries = []
        for idx in range(5):
            sc = decode_unpacked_bcd_bytes(data[idx * 6 : (idx + 1) * 6])
            tg = _decode_namco_tile_tag(data[30 + idx * 3 : 30 + (idx + 1) * 3])
            if sc is not None and 10 <= sc <= 99999999 and tg:
                split_entries.append({"tag": tg, "raw": sc})
            elif sc is None and tg is None:
                split_ok = False
                break
        if split_ok and len(split_entries) >= 2:
            return _normalize_entries(split_entries)

    # 2. Interleaved 9-byte or 10-byte records: (6 score + 3 tag) or (3 tag + 6 score)
    for stride in (9, 10):
        if n >= stride * 3:
            rec_entries = []
            for idx in range(min(5, n // stride)):
                base = idx * stride
                sc_a = decode_unpacked_bcd_bytes(data[base : base + 6])
                tg_a = _decode_namco_tile_tag(data[base + 6 : base + 9])
                if sc_a is not None and 10 <= sc_a <= 99999999 and tg_a:
                    rec_entries.append({"tag": tg_a, "raw": sc_a})
                    continue
                tg_b = _decode_namco_tile_tag(data[base : base + 3])
                sc_b = decode_unpacked_bcd_bytes(data[base + 3 : base + 9])
                if sc_b is not None and 10 <= sc_b <= 99999999 and tg_b:
                    rec_entries.append({"tag": tg_b, "raw": sc_b})
            if len(rec_entries) >= 2:
                return _normalize_entries(rec_entries)

    # 3. Score-only 5 x 6-byte Namco table (e.g. 30 or 36 bytes without initials)
    if 30 <= n <= 36:
        for idx in range(5):
            sc = decode_unpacked_bcd_bytes(data[idx * 6 : (idx + 1) * 6])
            if sc is not None and 100 <= sc <= 99999999:
                entries.append({"tag": f"{idx + 1}UP", "raw": sc})
        if len(entries) >= 2:
            return _normalize_entries(entries)

    return []


def _decode_dkong_hi(data):
    """
    Decodes Nintendo Donkey Kong / Mario Bros. MAME hiscore.dat .hi buffers (e.g. 5 x 34 = 170 bytes
    containing 6-byte unpacked/Nintendo-tile digits + 3-byte Nintendo-tile or ASCII initials).
    """
    n = len(data)
    if n < 30 or n > 256:
        return []

    entries = []
    # Standard MAME dkong.hi: 5 records x 34 bytes = 170 bytes
    if n >= 160 and (n % 34 in (0, 1, 2, 4, 6)):
        for idx in range(min(5, n // 34)):
            rec = data[idx * 34 : (idx + 1) * 34]
            # Scan within the 34-byte record for 6 unpacked/ASCII digits + 3 Nintendo-tile/ASCII tag bytes
            rec_score = None
            rec_tag = None
            for off in (7, 6, 5, 2, 0):
                if off + 6 <= len(rec):
                    sc = decode_unpacked_bcd_bytes(rec[off : off + 6])
                    if sc is not None and 100 <= sc <= 9999999:
                        rec_score = sc
                        break
            # Packed BCD score often stored in last 3-4 bytes of dkong 34-byte record as well
            if rec_score is None and len(rec) >= 34:
                sc_bcd = decode_bcd_bytes(rec[30:33], little_endian=False) or decode_bcd_bytes(
                    rec[30:34], little_endian=False
                )
                if sc_bcd is not None and 100 <= sc_bcd <= 9999999:
                    rec_score = sc_bcd
            for t_off in (15, 14, 16, 13, 17, 12):
                if t_off + 3 <= len(rec):
                    ts = rec[t_off : t_off + 3]
                    if _is_valid_tag_bytes(ts):
                        rec_tag = ts.decode("ascii", errors="ignore").strip().upper()
                        break
                    nt = _decode_nintendo_tile_tag(ts)
                    if nt:
                        rec_tag = nt
                        break
            if rec_score is not None and rec_tag:
                entries.append({"tag": rec_tag, "raw": rec_score})
        if entries:
            return _normalize_entries(entries)

    return []


def _swap16_bytes(data):
    """Swaps even/odd bytes in each 16-bit word (M68000 NVRAM byte order on little-endian hosts)."""
    n = len(data) - (len(data) % 2)
    if n < 4:
        return b""
    out = bytearray(n)
    out[0:n:2] = data[1:n:2]
    out[1:n:2] = data[0:n:2]
    return bytes(out)


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


def _scan_structured_records(data, little_endian=False, strict_nvram=False):
    """
    Scans binary data for non-overlapping (score + 3-char ASCII tag) or (3-char tag + score)
    records using packed BCD (BE/LE) and unpacked BCD / ASCII digits.
    """
    n = len(data)
    i = 0
    consumed = [False] * n
    raw_records = []
    max_score = 99999990 if strict_nvram else 999999999

    while i <= n - 3:
        t_slice = data[i : i + 3]
        if _is_valid_tag_bytes(t_slice, strict_upper=strict_nvram):
            tag = t_slice.decode("ascii", errors="ignore").strip().upper()
            matched_score = None
            consume_end = i + 3

            # 1. Packed BCD (3..4 bytes) immediately BEFORE tag (optional 0..2 pad bytes)
            for pad in (0, 1, 2):
                for s_len in (4, 3):
                    s_start = i - pad - s_len
                    s_end = i - pad
                    if s_start >= 0 and not any(consumed[s_start:i]):
                        s_slice = data[s_start:s_end]
                        if not _is_valid_tag_bytes(s_slice[:3]):
                            val = decode_bcd_bytes(s_slice, little_endian=little_endian)
                            if val is not None and 100 <= val <= max_score:
                                if not strict_nvram or (val % 10 == 0):
                                    matched_score = val
                                    for k in range(s_start, i + 3):
                                        consumed[k] = True
                                    break
                if matched_score is not None:
                    break

            # 2. Unpacked BCD / ASCII digits (5..8 bytes) immediately BEFORE tag
            if matched_score is None and not little_endian:
                for pad in (0, 1):
                    for s_len in (8, 7, 6, 5):
                        s_start = i - pad - s_len
                        s_end = i - pad
                        if s_start >= 0 and not any(consumed[s_start:i]):
                            s_slice = data[s_start:s_end]
                            if not _is_valid_tag_bytes(s_slice[:3]):
                                val = decode_unpacked_bcd_bytes(s_slice)
                                if val is not None and 100 <= val <= max_score:
                                    if not strict_nvram or (val % 10 == 0):
                                        matched_score = val
                                        for k in range(s_start, i + 3):
                                            consumed[k] = True
                                        break
                    if matched_score is not None:
                        break

            # 3. Packed BCD (3..4 bytes) immediately AFTER tag
            if matched_score is None:
                for pad in (0, 1, 2):
                    for s_len in (4, 3):
                        s_start = i + 3 + pad
                        s_end = s_start + s_len
                        if s_end <= n and not any(consumed[i:s_end]):
                            s_slice = data[s_start:s_end]
                            if not _is_valid_tag_bytes(s_slice[:3]):
                                val = decode_bcd_bytes(s_slice, little_endian=little_endian)
                                if val is not None and 100 <= val <= max_score:
                                    if not strict_nvram or (val % 10 == 0):
                                        matched_score = val
                                        for k in range(i, s_end):
                                            consumed[k] = True
                                        consume_end = s_end
                                        break
                    if matched_score is not None:
                        break

            # 4. Unpacked BCD / ASCII digits (5..8 bytes) immediately AFTER tag
            if matched_score is None and not little_endian:
                for pad in (0, 1):
                    for s_len in (8, 7, 6, 5):
                        s_start = i + 3 + pad
                        s_end = s_start + s_len
                        if s_end <= n and not any(consumed[i:s_end]):
                            s_slice = data[s_start:s_end]
                            if not _is_valid_tag_bytes(s_slice[:3]):
                                val = decode_unpacked_bcd_bytes(s_slice)
                                if val is not None and 100 <= val <= max_score:
                                    if not strict_nvram or (val % 10 == 0):
                                        matched_score = val
                                        for k in range(i, s_end):
                                            consumed[k] = True
                                        consume_end = s_end
                                        break
                    if matched_score is not None:
                        break

            if matched_score is not None:
                raw_records.append({"pos": i, "tag": tag, "raw": matched_score})
                i = consume_end
                continue
        i += 1

    if strict_nvram:
        # In large NVRAM / saveram buffers (> 512B), require a contiguous table cluster
        # of >= 2 entries separated by <= 32 bytes each to prevent random RAM false matches.
        best_cluster = []
        cur_cluster = []
        for rec in raw_records:
            if not cur_cluster or (rec["pos"] - cur_cluster[-1]["pos"]) <= 32:
                cur_cluster.append(rec)
            else:
                if len(cur_cluster) > len(best_cluster):
                    best_cluster = cur_cluster
                cur_cluster = [rec]
        if len(cur_cluster) > len(best_cluster):
            best_cluster = cur_cluster
        if len(best_cluster) < 2:
            return []
        raw_records = best_cluster

    return raw_records


def _score_candidate_quality(records):
    """Ranks candidate BE vs LE record lists by arcade plausibility (multiples of 10 + descending order)."""
    if not records:
        return (-1, -1, -1)
    mult10 = sum(1 for r in records if (r["raw"] % 10 == 0))
    descending = sum(
        1 for idx in range(len(records) - 1) if records[idx]["raw"] >= records[idx + 1]["raw"]
    )
    return (len(records), mult10, descending)


def _decode_with_spec(data, fmt):
    """Decodes a binary buffer using a single _CLASSIC_ROM_FORMATS specification."""
    if not fmt:
        return []
    stride = fmt["stride"]
    offset = fmt["offset"]
    s_len = fmt["score_bytes"]
    t_len = fmt["tag_bytes"]
    mult = fmt.get("mult", 1)
    le = fmt["mode"] == "bcd_le"

    entries = []
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

    alt = fmt.get("alt_encoding")
    if alt == "namco_tile":
        return _decode_namco_tile_hi(data)
    if alt == "dkong_record":
        return _decode_dkong_hi(data)
    return []


def parse_hi_binary(data, rom_name=""):
    """
    Decodes a binary .hi or NVRAM buffer using:
      0. JSON / .hi.json sidecar detection
      1. Per-ROM specification table (_CLASSIC_ROM_FORMATS + Namco/Nintendo tile formats)
      2. Structured BCD (BE/LE) & unpacked/ASCII record scanner heuristic
      3. Namco / Donkey Kong tile heuristic for unknown classic clones (< 256B)
      4. Plain ASCII text fallback
      5. Single top-score BCD / unpacked BCD fallback (3..16B)
      6. M68000 16-bit byte-swapped retry for Neo-Geo / CPS NVRAM & .nv files
    """
    if not data:
        return []

    # 0. Check if data is actually JSON or plain text lines
    if len(data) <= 16384:
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
    strict_nvram = len(data) > 512

    # 1. Per-ROM specification table
    if fmt and not strict_nvram:
        spec_res = _decode_with_spec(data, fmt)
        if spec_res:
            return spec_res

    # 2. Structured heuristic scanner (comparing Big-Endian vs Little-Endian quality)
    rec_be = _scan_structured_records(data, little_endian=False, strict_nvram=strict_nvram)
    rec_le = _scan_structured_records(data, little_endian=True, strict_nvram=strict_nvram)
    chosen = rec_be if _score_candidate_quality(rec_be) >= _score_candidate_quality(rec_le) else rec_le
    if chosen:
        norm = _normalize_entries(chosen)
        if norm:
            return norm

    if not strict_nvram:
        # 3. Namco / Nintendo tile-coded heuristic for unknown classic clones (< 256B)
        tile_res = _decode_dkong_hi(data) or _decode_namco_tile_hi(data)
        if tile_res:
            return tile_res

        # 4. ASCII text fallback (e.g. "1245800 RED" or "RED 1245800")
        try:
            entries = []
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

        # 5. Single top-score BCD fallback (5..16B unpacked/ASCII like pacman.hi first, then 3..4B packed BCD)
        if 3 <= len(data) <= 16:
            if len(data) >= 5:
                u_val = decode_unpacked_bcd_bytes(data[: min(len(data), 8)])
                if u_val is not None and 100 <= u_val <= 99999999:
                    return _normalize_entries([{"tag": "1UP", "raw": u_val}])
            for s_len in (4, 3):
                if len(data) >= s_len:
                    val = decode_bcd_bytes(data[:s_len], little_endian=False)
                    if val is not None and 100 <= val <= 99999999:
                        return _normalize_entries([{"tag": "1UP", "raw": val}])
            u_val = decode_unpacked_bcd_bytes(data[: min(len(data), 8)])
            if u_val is not None and 100 <= u_val <= 99999999:
                return _normalize_entries([{"tag": "1UP", "raw": u_val}])

    # 6. M68000 16-bit byte-swapped retry (Neo-Geo MVS saveram / .nv & CPS NVRAM)
    if 8 <= len(data) <= 65536:
        swapped = _swap16_bytes(data)
        if swapped:
            if fmt and not strict_nvram:
                spec_sw = _decode_with_spec(swapped, fmt)
                if spec_sw:
                    return spec_sw
            sw_be = _scan_structured_records(swapped, little_endian=False, strict_nvram=True)
            if sw_be:
                norm_sw = _normalize_entries(sw_be)
                if norm_sw:
                    return norm_sw

    return []


def _infer_rom_system_folder(rom_path):
    """Infers the system folder name from a ROM path like /recalbox/share/roms/fba_libretro/sf2.zip."""
    if not rom_path:
        return ""
    parts = [p for p in str(rom_path).replace("\\", "/").split("/") if p]
    for idx, tok in enumerate(parts):
        if tok.lower() == "roms" and idx + 1 < len(parts) - 1:
            return parts[idx + 1]
    if len(parts) >= 2:
        return parts[-2]
    return ""


def _append_unique(out_list, seen_set, path_str):
    if path_str and path_str not in seen_set:
        seen_set.add(path_str)
        out_list.append(path_str)


def get_hiscore_candidate_paths(rom_name, rom_path=None, system=None):
    """
    Returns ordered candidate paths for <rom>.hi, <rom>.hi.json, and NVRAM files across
    Batocera, Recalbox (including per-system saves/<sys>/<core>/hi and bios/<core>/hi), and RetroPie.
    """
    if not rom_name:
        return []

    hi_candidates = []
    nv_candidates = []
    seen = set()

    def add_hi_dir(d):
        _append_unique(hi_candidates, seen, os.path.join(d, f"{rom_name}.hi.json"))
        _append_unique(hi_candidates, seen, os.path.join(d, f"{rom_name}.hi"))

    def add_nv_path(p):
        _append_unique(nv_candidates, seen, p)

    env_dir = os.environ.get("AM_HISCORE_DIR")
    if env_dir:
        add_hi_dir(env_dir)
        add_hi_dir(os.path.join(env_dir, "hi"))
        add_nv_path(os.path.join(env_dir, f"{rom_name}.nv"))
        add_nv_path(os.path.join(env_dir, "nvram", rom_name, f"{rom_name}.hi"))
        add_nv_path(os.path.join(env_dir, "nvram", rom_name, "saveram"))

    if rom_path:
        rom_dir = os.path.dirname(str(rom_path))
        if rom_dir:
            add_hi_dir(os.path.join(rom_dir, "hi"))
            add_hi_dir(rom_dir)

    rom_sys = _infer_rom_system_folder(rom_path)
    sys_names = []
    for s_cand in (
        system,
        rom_sys,
        "mame",
        "fbneo",
        "fba_libretro",
        "fba",
        "neogeo",
        "arcade",
        "mame-libretro",
        "cps1",
        "cps2",
        "cps3",
    ):
        s_clean = str(s_cand or "").strip()
        if s_clean and s_clean not in sys_names:
            sys_names.append(s_clean)

    core_subdirs = (
        "",
        "hi",
        "fbneo",
        "fbneo/hi",
        "FinalBurn Neo",
        "FinalBurn Neo/fbneo",
        "FinalBurn Neo/fbneo/hi",
        "fba",
        "fba/hi",
        "mame",
        "mame/hi",
        "mame2003-plus",
        "mame2003-plus/hi",
        "MAME 0.78 Plus",
        "MAME 0.78 Plus/hi",
        "mame2003",
        "mame2003/hi",
        "MAME 2003 (0.78)",
        "MAME 2003 (0.78)/hi",
        "mame2010",
        "mame2010/hi",
        "mame2015",
        "mame2015/hi",
        "mame2016",
        "mame2016/hi",
    )

    save_roots = (
        "/recalbox/share/saves",
        "/userdata/saves",
        "/opt/retropie/configs/all/retroarch/saves",
    )

    for root in save_roots:
        for s_name in sys_names:
            sys_dir = os.path.join(root, s_name)
            for sub in core_subdirs:
                add_hi_dir(os.path.join(sys_dir, sub) if sub else sys_dir)
            # Dynamic 1-level core folder discovery for custom/new Libretro core names
            if os.path.isdir(sys_dir):
                try:
                    for entry in sorted(os.listdir(sys_dir)):
                        sub_full = os.path.join(sys_dir, entry)
                        if os.path.isdir(sub_full):
                            add_hi_dir(sub_full)
                            add_hi_dir(os.path.join(sub_full, "hi"))
                except Exception:
                    pass
        # Direct core folders under RetroArch saves root (e.g. RetroPie)
        for sub in ("mame2003-plus/hi", "MAME 0.78 Plus/hi", "FinalBurn Neo/fbneo/hi", "fbneo/hi", "fba/hi"):
            add_hi_dir(os.path.join(root, sub))

    # BIOS hi directories used by mame2003 / mame2003-plus / fbneo on Recalbox, Batocera, RetroPie
    bios_dirs = (
        "/recalbox/share/bios/fbneo/hi",
        "/recalbox/share/bios/fba/hi",
        "/recalbox/share/bios/mame2003-plus/hi",
        "/recalbox/share/bios/mame2003/hi",
        "/recalbox/share/bios/mame/hi",
        "/recalbox/share/bios/hi",
        "/userdata/bios/fbneo/hi",
        "/userdata/bios/fba/hi",
        "/userdata/bios/mame2003-plus/hi",
        "/userdata/bios/mame2003/hi",
        "/userdata/bios/mame/hi",
        "/userdata/bios/hi",
        "/home/pi/RetroPie/BIOS/fbneo/hi",
        "/home/pi/RetroPie/BIOS/fba/hi",
        "/home/pi/RetroPie/BIOS/mame2003-plus/hi",
        "/home/pi/RetroPie/BIOS/mame2003/hi",
        "/home/pi/RetroPie/roms/arcade/hi",
        "/home/pi/RetroPie/roms/mame-libretro/hi",
        "/home/pi/RetroPie/roms/fba/hi",
        "/home/pi/RetroPie/roms/neogeo/hi",
    )
    for bd in bios_dirs:
        add_hi_dir(bd)

    # NVRAM & FBNeo .nv / saveram fallback paths (evaluated only after all .hi / .hi.json paths)
    nvram_dirs = []
    if env_dir:
        nvram_dirs.append(os.path.join(env_dir, "nvram", rom_name))
    for root in save_roots:
        for s_name in sys_names:
            sys_dir = os.path.join(root, s_name)
            add_nv_path(os.path.join(sys_dir, f"{rom_name}.nv"))
            add_nv_path(os.path.join(sys_dir, "fbneo", f"{rom_name}.nv"))
            add_nv_path(os.path.join(sys_dir, "fba", f"{rom_name}.nv"))
            nvram_dirs.extend([
                os.path.join(sys_dir, "nvram", rom_name),
                os.path.join(sys_dir, "fbneo", "nvram", rom_name),
                os.path.join(sys_dir, "mame", "nvram", rom_name),
                os.path.join(sys_dir, "mame2003-plus", "nvram", rom_name),
            ])
    nvram_dirs.append(f"/home/pi/RetroPie/roms/arcade/nvram/{rom_name}")

    for nv_dir in nvram_dirs:
        if os.path.isdir(nv_dir):
            try:
                for fn in sorted(os.listdir(nv_dir)):
                    full_nv = os.path.join(nv_dir, fn)
                    if os.path.isfile(full_nv):
                        add_nv_path(full_nv)
            except Exception:
                pass

    return hi_candidates + nv_candidates


def extract_hiscores(rom_name, rom_path=None, system=None):
    """
    Searches candidate paths for <rom>.hi / <rom>.hi.json / NVRAM.
    Tries hi2txt CLI first if available, then falls back to built-in spec + heuristic decoder.
    Returns top 5 list: [{"rank": 1, "tag": "RED", "score": "1,245,800", "raw": 1245800}, ...]
    """
    if not rom_name:
        return []

    hi2txt_bin = shutil.which("hi2txt")
    for path in get_hiscore_candidate_paths(rom_name, rom_path=rom_path, system=system):
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

def read_ra_config(prefs=None, system=None):
    """
    Reads RetroAchievements settings from Batocera, Recalbox, RetroArch custom overrides,
    or RetroPie config files, merged with optional DMD preferences.
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

    sys_low = str(system or "").strip().lower()
    conf_candidates = []
    if os.environ.get("AM_RA_CONF_FILE"):
        conf_candidates.append(os.environ["AM_RA_CONF_FILE"])
    conf_candidates.extend([
        "/userdata/system/batocera.conf",
        "/recalbox/share/system/recalbox.conf",
        "/recalbox/share/system/configs/retroarch/retroarchcustom.cfg",
        "/userdata/system/configs/retroarch/retroarchcustom.cfg",
        "/opt/retropie/configs/all/retroarch.cfg",
    ])

    sys_ra_override = None
    sys_hc_override = None

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
                    elif sys_low and k == f"{sys_low}.retroachievements":
                        sys_ra_override = _parse_bool(v, False)
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
                    elif sys_low and k == f"{sys_low}.retroachievements.hardcore":
                        sys_hc_override = _parse_bool(v, False)
        except Exception:
            continue

    if sys_ra_override is not None:
        cfg["enabled"] = sys_ra_override
    if sys_hc_override is not None:
        cfg["hardcore"] = sys_hc_override
    if cfg["username"] and cfg["web_api_key"]:
        cfg["enabled"] = True

    return cfg


def get_rcheevos_log_paths():
    """Returns ordered candidate RetroArch / EmulationStation launcher log files across Recalbox, Batocera & RetroPie."""
    paths = []
    if os.environ.get("AM_RCHEEVOS_LOG_FILE"):
        paths.append(os.environ["AM_RCHEEVOS_LOG_FILE"])
    paths.extend([
        LIVE_TEST_LOG_DEFAULT,
        "/tmp/retroarch.log",
        "/dev/shm/retroarch.log",
        "/userdata/system/logs/es_launch_stdout.log",
        "/userdata/system/logs/es_launch_stderr.log",
        "/userdata/system/logs/retroarch.log",
        "/recalbox/share/system/logs/retroarch.log",
        "/recalbox/share/system/logs/es_launch_stdout.log",
        "/recalbox/share/system/logs/es_launch_stderr.log",
        "/recalbox/share/system/configs/retroarch/logs/retroarch.log",
        "/recalbox/share/system/.config/retroarch/logs/retroarch.log",
        "/recalbox/share/system/logs/recalbox.log",
        "/tmp/recalbox.log",
        "/dev/shm/runcommand.log",
    ])
    return paths


def _read_log_tail(log_path, max_bytes=65536):
    """Reads up to the last `max_bytes` of a log file and isolates the latest game session if present."""
    with open(log_path, "rb") as f:
        f.seek(0, os.SEEK_END)
        size = f.tell()
        start = max(0, size - max_bytes)
        f.seek(start, os.SEEK_SET)
        raw = f.read(size - start)
    text = raw.decode("utf-8", errors="ignore")
    # If the log spans multiple game launches (e.g. recalbox.log), slice from the last session marker
    last_idx = -1
    for marker in ("Identifying game:", "Load started, hardcore"):
        pos = text.rfind(marker)
        if pos > last_idx:
            last_idx = pos
    if last_idx > 0:
        line_start = text.rfind("\n", 0, last_idx)
        text = text[line_start + 1 :] if line_start >= 0 else text[last_idx:]
    return text


def parse_rcheevos_unlock_line(line, system="", game="", default_hardcore=False):
    """
    Parses a single RetroArch log line for a live achievement unlock event.
    Matches real rcheevos (`rc_client.c`) and RetroArch patterns:
      - [INFO] [RCHEEVOS]: Awarding achievement 12345: Dragon Coin Collector
      - [RCHEEVOS]: Awarded achievement 12345: "Dragon Coin Collector" (10 points, hardcore)
      - [RCHEEVOS]: Achievement Unlocked: Dragon Coin Collector (10 pts, hardcore)
      - [RCHEEVOS]: Unlocked unofficial achievement 12345: Dragon Coin Collector
    Returns a celebration dict or None.
    """
    if not line:
        return None
    if not any(
        kw in line
        for kw in (
            "Awarding achievement",
            "Awarded achievement",
            "Achievement Unlocked",
            "Unlocking achievement",
            "Unlocked unofficial achievement",
        )
    ):
        return None
    # Guard against error/retry lines like "Awarded achievement 1 instead of 2" or "Error awarding achievement"
    if "instead of" in line or "Error awarding" in line:
        return None

    hardcore = bool(default_hardcore)
    lower = line.lower()
    if "hardcore" in lower:
        if any(neg in lower for neg in ("hardcore=0", "hardcore: false", "hardcore=false", "softcore", "disabled", "not active")):
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

    title = ""
    m = re.search(
        r"(?:Awarding achievement(?:\s+\d+)?|Awarded achievement(?:\s+\d+)?|Achievement Unlocked|Unlocking achievement(?:\s+\d+)?|Unlocked unofficial achievement(?:\s+\d+)?)\s*:\s*(.+)$",
        line,
        re.IGNORECASE,
    )
    if m:
        raw_tail = m.group(1).strip()
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


def parse_rcheevos_summary_from_logs(log_text, default_hardcore=False):
    """
    Extracts RA progress summary from RetroArch [RCHEEVOS] log lines (supporting EN, FR, ES
    localized RetroArch notifications as well as `rc_client.c` English diagnostic lines).
    Returns {"unlocked": int, "total": int, "points": int, "hardcore": bool, ...} or None.
    """
    if not log_text:
        return None
    unlocked = None
    total = None
    fallback_unlocked = None
    fallback_total = None
    points = 0
    hardcore = bool(default_hardcore)
    recent_title = ""

    for raw_line in log_text.splitlines():
        if not any(
            tok in raw_line
            for tok in ("RCHEEVOS", "Achievement", "achievement", "succès", "logros")
        ):
            continue

        if re.search(
            r"hardcore\s*(?:mode\s*)?(?:enabled|active|=\s*1|=\s*true)", raw_line, re.IGNORECASE
        ) and not re.search(r"not\s+active", raw_line, re.IGNORECASE):
            hardcore = True
        elif re.search(
            r"hardcore\s*(?:mode\s*)?(?:disabled|not\s+active|=\s*0|=\s*false)|pausing\s+hardcore",
            raw_line,
            re.IGNORECASE,
        ):
            hardcore = False

        # 1. English summary: "You have [earned] 18 of 45 achievements unlocked (240 points)" / "Unlocked 18/45 achievements"
        m_en = re.search(
            r"(?:have(?:\s+earned)?\s+|unlocked\s+)(\d+)\s*(?:of|/)\s*(\d+)\s+achievements?(?:.*?(\d+)\s*(?:points?|pts?))?",
            raw_line,
            re.IGNORECASE,
        )
        if m_en:
            unlocked = int(m_en.group(1))
            total = int(m_en.group(2))
            if m_en.group(3):
                points = int(m_en.group(3))
        else:
            # 2. French localized RetroArch summary:
            #    "Vous avez débloqué 18 succès sur 45" / "Vous avez 18 sur 45 succès débloqués" / "18/45 succès"
            m_fr = re.search(
                r"(?:débloqué\s+(\d+)\s+(?:succès\s+)?sur\s+(\d+)|avez\s+(\d+)\s+sur\s+(\d+)\s+succès|(\d+)\s*/\s*(\d+)\s+succès)",
                raw_line,
                re.IGNORECASE,
            )
            if m_fr:
                if m_fr.group(1) is not None:
                    unlocked, total = int(m_fr.group(1)), int(m_fr.group(2))
                elif m_fr.group(3) is not None:
                    unlocked, total = int(m_fr.group(3)), int(m_fr.group(4))
                else:
                    unlocked, total = int(m_fr.group(5)), int(m_fr.group(6))
            else:
                # 3. Spanish localized RetroArch summary:
                #    "Has desbloqueado 18 de 45 logros" / "Tienes 18 de 45 logros" / "18/45 logros"
                m_es = re.search(
                    r"(?:desbloqueado\s+(\d+)\s+de\s+(\d+)\s+logros|tienes\s+(\d+)\s+de\s+(\d+)\s+logros|(\d+)\s*/\s*(\d+)\s+logros)",
                    raw_line,
                    re.IGNORECASE,
                )
                if m_es:
                    if m_es.group(1) is not None:
                        unlocked, total = int(m_es.group(1)), int(m_es.group(2))
                    elif m_es.group(3) is not None:
                        unlocked, total = int(m_es.group(3)), int(m_es.group(4))
                    else:
                        unlocked, total = int(m_es.group(5)), int(m_es.group(6))

        # 4. Encore / all achievements active line: "All 45 achievements activated" / "Tous les 45 succès activés"
        m_all = re.search(
            r"(?:All|Tous\s+les|Todos\s+los)\s+(\d+)\s+(?:achievements?|succès|logros)\s+(?:activated|activés|activados)",
            raw_line,
            re.IGNORECASE,
        )
        if m_all and total is None:
            fallback_unlocked = 0
            fallback_total = int(m_all.group(1))

        # 5. rc_client.c non-localized set line: "Set 1234: 30/45 achievements active"
        #    (active = still-locked achievements in normal mode, so unlocked = total - active when unsupported == 0)
        m_set = re.search(
            r"Set\s+\d+\s*:\s*(\d+)\s*/\s*(\d+)\s+achievements\s+active(?:\s*\((\d+)\s+unsupported\))?",
            raw_line,
            re.IGNORECASE,
        )
        if m_set:
            act_cnt = int(m_set.group(1))
            tot_cnt = int(m_set.group(2))
            unsup_cnt = int(m_set.group(3) or 0)
            if tot_cnt > 0 and act_cnt + unsup_cnt <= tot_cnt:
                fallback_total = tot_cnt
                fallback_unlocked = max(0, tot_cnt - act_cnt - unsup_cnt)

        # 6. rc_client.c score update line: "Achievement 12345 awarded, new score: 250"
        m_new_score = re.search(r"new\s+score\s*:\s*(\d+)", raw_line, re.IGNORECASE)
        if m_new_score:
            points = int(m_new_score.group(1))

        unlock_evt = parse_rcheevos_unlock_line(raw_line, default_hardcore=hardcore)
        if unlock_evt:
            recent_title = unlock_evt.get("title", "")

    if total is None and fallback_total is not None:
        total = fallback_total
        unlocked = fallback_unlocked or 0

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


def _norm_title_tokens(s):
    if not s:
        return ""
    # Strip region/parenthetical tags like "(USA)" or "[!]"
    cleaned = re.sub(r"[\(\[][^\)\]]*[\)\]]", " ", str(s).lower())
    return re.sub(r"[^a-z0-9]+", "", cleaned)


def _titles_match(ra_title, game_slug="", human_title=""):
    ra_norm = _norm_title_tokens(ra_title)
    if not ra_norm:
        return False
    for cand in (human_title, game_slug):
        c_norm = _norm_title_tokens(cand)
        if not c_norm or len(c_norm) < 2:
            continue
        if c_norm == ra_norm or c_norm in ra_norm or ra_norm in c_norm:
            return True
    return False


def _fetch_ra_game_progress_api(username, web_api_key, game_id, hardcore=False):
    """Queries RetroAchievements API_GetGameInfoAndUserProgress.php for exact progress + latest unlocked trophy."""
    params = urllib.parse.urlencode({
        "z": username,
        "y": web_api_key,
        "u": username,
        "g": str(game_id),
    })
    api_url = f"https://retroachievements.org/API/API_GetGameInfoAndUserProgress.php?{params}"
    req = urllib.request.Request(
        api_url, headers={"Accept": "application/json", "User-Agent": "ArcadeMatrix/1.0"}
    )
    with urllib.request.urlopen(req, timeout=1.5) as resp:
        if not (200 <= resp.status < 300):
            return None
        data = json.loads(resp.read().decode("utf-8", errors="ignore"))

    total = int(data.get("NumAchievements") or 0)
    if total <= 0:
        return None

    hc = bool(hardcore)
    unlocked = int(
        data.get("NumAwardedToUserHardcore" if hc else "NumAwardedToUser")
        or data.get("NumAwardedToUser")
        or 0
    )

    points_earned = int(data.get("UserTotalPoints") or 0)
    recent_title = ""
    recent_date = ""
    ach_map = data.get("Achievements")
    if isinstance(ach_map, dict):
        calc_pts = 0
        for ach in ach_map.values():
            if not isinstance(ach, dict):
                continue
            dt = str(ach.get("DateEarnedHardcore") or ach.get("DateEarned") or "").strip()
            if dt:
                pts = int(ach.get("Points") or 0)
                calc_pts += pts
                if dt >= recent_date:
                    recent_date = dt
                    recent_title = str(ach.get("Title") or "").strip()
        if points_earned <= 0 and calc_pts > 0:
            points_earned = calc_pts

    out = {
        "unlocked": unlocked,
        "total": total,
        "points": points_earned,
        "hardcore": hc,
    }
    if recent_title:
        out["recent_title"] = recent_title
    return out


def _fetch_ra_recently_played_api(username, web_api_key, game="", title="", hardcore=False):
    """
    Queries API_GetUserRecentlyPlayedGames.php when no RetroArch log GameID is available
    (e.g. default Recalbox config without verbose logs). Matches the active game by title/slug.
    """
    params = urllib.parse.urlencode({
        "z": username,
        "y": web_api_key,
        "u": username,
        "c": "5",
    })
    api_url = f"https://retroachievements.org/API/API_GetUserRecentlyPlayedGames.php?{params}"
    req = urllib.request.Request(
        api_url, headers={"Accept": "application/json", "User-Agent": "ArcadeMatrix/1.0"}
    )
    with urllib.request.urlopen(req, timeout=1.5) as resp:
        if not (200 <= resp.status < 300):
            return None
        rows = json.loads(resp.read().decode("utf-8", errors="ignore"))

    if not isinstance(rows, list) or not rows:
        return None

    matched_row = None
    for row in rows:
        if not isinstance(row, dict):
            continue
        if _titles_match(row.get("Title", ""), game_slug=game, human_title=title):
            matched_row = row
            break

    if not matched_row:
        return None

    total = int(matched_row.get("NumPossibleAchievements") or 0)
    if total <= 0:
        return None

    game_id = int(matched_row.get("GameID") or 0)
    if game_id > 0:
        try:
            detailed = _fetch_ra_game_progress_api(username, web_api_key, game_id, hardcore=hardcore)
            if detailed and detailed.get("total", 0) > 0:
                return detailed
        except Exception:
            pass

    hc = bool(hardcore)
    unlocked = int(
        matched_row.get("NumAchievedHardcore" if hc else "NumAchieved")
        or matched_row.get("NumAchieved")
        or 0
    )
    points = int(
        matched_row.get("ScoreAchievedHardcore" if hc else "ScoreAchieved")
        or matched_row.get("ScoreAchieved")
        or 0
    )
    return {
        "unlocked": unlocked,
        "total": total,
        "points": points,
        "hardcore": hc,
    }


def extract_ra_summary(system, game, prefs=None, title=None, min_mtime=None):
    """
    Resolves RetroAchievements summary for the active game from cache, RetroArch logs,
    or the RetroAchievements Official Web API.
    Returns dict {"unlocked": int, "total": int, "points": int, "hardcore": bool, ...} or None.
    """
    ra_cfg = read_ra_config(prefs, system=system)
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

    explicit_env_log = os.environ.get("AM_RCHEEVOS_LOG_FILE")
    detected_game_id = None
    log_summary = None

    for log_path in get_rcheevos_log_paths():
        if not os.path.isfile(log_path):
            continue
        try:
            if (
                min_mtime is not None
                and log_path not in (explicit_env_log, LIVE_TEST_LOG_DEFAULT)
                and os.path.getmtime(log_path) < min_mtime
            ):
                continue
            content = _read_log_tail(log_path, max_bytes=65536)
            gid_matches = re.findall(
                r"(?:Identified game\s*:\s*|Starting session for game\s+|Game\s+)(\d{2,9})(?:\s+loaded|\b)",
                content,
                re.IGNORECASE,
            )
            if gid_matches and not detected_game_id:
                detected_game_id = int(gid_matches[-1])

            summary = parse_rcheevos_summary_from_logs(content, default_hardcore=ra_cfg["hardcore"])
            if summary and summary.get("total", 0) > 0:
                log_summary = summary
                break
        except Exception:
            continue

    # Enrich or fallback via RetroAchievements Web API when username + web_api_key are configured
    if ra_cfg.get("username") and ra_cfg.get("web_api_key"):
        if detected_game_id:
            try:
                api_summary = _fetch_ra_game_progress_api(
                    ra_cfg["username"],
                    ra_cfg["web_api_key"],
                    detected_game_id,
                    hardcore=log_summary["hardcore"] if log_summary else ra_cfg.get("hardcore", False),
                )
                if api_summary and api_summary.get("total", 0) > 0:
                    if log_summary and log_summary.get("recent_title") and not api_summary.get("recent_title"):
                        api_summary["recent_title"] = log_summary["recent_title"]
                    return api_summary
            except Exception:
                pass
        elif not log_summary:
            try:
                recent_summary = _fetch_ra_recently_played_api(
                    ra_cfg["username"],
                    ra_cfg["web_api_key"],
                    game=game,
                    title=title or "",
                    hardcore=ra_cfg.get("hardcore", False),
                )
                if recent_summary and recent_summary.get("total", 0) > 0:
                    return recent_summary
            except Exception:
                pass

    return log_summary


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


def run_live_watcher_loop(
    dmd_host,
    topic,
    system,
    game,
    prefs,
    max_duration_sec=43200,
    title="",
    initial_hiscores=None,
    initial_ra_summary=None,
    session_start_ts=None,
):
    """
    Tails candidate RetroArch log files while the game is running:
      1. Retries delayed RetroAchievements startup summary discovery during the first ~12s
         (RetroArch login + ROM hash takes 1.5s..4s after gameStart) and publishes an updated `sync_meta`.
      2. Immediately publishes `{"status": "celebrate", "kind": "achievement", ...}` on live `[RCHEEVOS]`
         unlock lines and refreshes `sync_meta` progress count.
    """
    pid_file = os.environ.get("AM_WATCHER_PID_FILE") or WATCHER_PID_FILE_DEFAULT
    my_pid = os.getpid()
    try:
        with open(pid_file, "w", encoding="utf-8") as f:
            f.write(str(my_pid))
    except Exception:
        return

    ra_cfg = read_ra_config(prefs, system=system)
    default_hc = ra_cfg.get("hardcore", False)
    want_celebrate = bool(prefs.get("sync_celebrate_events", True))
    want_ra_card = bool(prefs.get("sync_show_ra", True))
    pub_hi = list(initial_hiscores) if (initial_hiscores and prefs.get("sync_show_hiscores", True)) else []
    current_ra = dict(initial_ra_summary) if initial_ra_summary else None

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
    min_log_mtime = (session_start_ts - 5.0) if session_start_ts else (start_ts - 10.0)
    # Retry schedule (seconds elapsed since watcher start) when RA summary wasn't ready at +0.35s
    ra_retry_schedule = [1.2, 2.8, 5.0, 8.0, 12.0]
    ra_retry_idx = 0

    running = True

    def _handle_sig(_signum, _frame):
        nonlocal running
        running = False

    try:
        signal.signal(signal.SIGTERM, _handle_sig)
        signal.signal(signal.SIGINT, _handle_sig)
    except ValueError:
        pass

    while running and (time.time() - start_ts) < max_duration_sec:
        try:
            if not os.path.isfile(pid_file):
                break
            with open(pid_file, "r", encoding="utf-8") as pf:
                if pf.read().strip() != str(my_pid):
                    break
        except Exception:
            break

        elapsed = time.time() - start_ts

        # Delayed RA startup summary check (RetroArch finishes [RCHEEVOS] login ~1.5s..5s after launch)
        if want_ra_card and current_ra is None and ra_retry_idx < len(ra_retry_schedule):
            if elapsed >= ra_retry_schedule[ra_retry_idx]:
                ra_retry_idx += 1
                found_ra = extract_ra_summary(
                    system, game, prefs=prefs, title=title, min_mtime=min_log_mtime
                )
                if found_ra and found_ra.get("total", 0) > 0:
                    current_ra = found_ra
                    meta_payload = build_sync_meta_payload(
                        system, game, hiscores=pub_hi, ra_summary=current_ra
                    )
                    if meta_payload:
                        publish_mqtt_event(dmd_host, topic, meta_payload, prefs=prefs)
            if not want_celebrate and (current_ra is not None or ra_retry_idx >= len(ra_retry_schedule)):
                break

        if want_celebrate:
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
                                if want_ra_card and current_ra and current_ra.get("total", 0) > 0:
                                    current_ra["unlocked"] = min(
                                        int(current_ra["total"]),
                                        int(current_ra.get("unlocked", 0)) + 1,
                                    )
                                    current_ra["recent_title"] = evt["title"]
                                    if evt.get("points"):
                                        current_ra["points"] = int(current_ra.get("points", 0)) + int(
                                            evt["points"]
                                        )
                                    meta_upd = build_sync_meta_payload(
                                        system, game, hiscores=pub_hi, ra_summary=current_ra
                                    )
                                    if meta_upd:
                                        publish_mqtt_event(dmd_host, topic, meta_upd, prefs=prefs)
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
    5. Runs the live achievement log watcher loop for delayed RA startup summaries & live unlocks.
    """
    stop_live_watcher()
    session_start_ts = time.time()
    prefs = resolve_broker_and_prefs(dmd_host)

    hiscores = []
    if prefs.get("sync_show_hiscores", True) or prefs.get("sync_celebrate_events", True):
        hiscores = extract_hiscores(game, rom_path=rom_path, system=system)
        save_hiscore_baseline(system, game, hiscores, rom_path=rom_path)

    if delay_sec > 0:
        time.sleep(delay_sec)

    pub_hi = hiscores if prefs.get("sync_show_hiscores", True) else []
    pub_ra = (
        extract_ra_summary(
            system,
            game,
            prefs=prefs,
            title=title,
            min_mtime=session_start_ts - 5.0,
        )
        if prefs.get("sync_show_ra", True)
        else None
    )

    meta_payload = build_sync_meta_payload(system, game, hiscores=pub_hi, ra_summary=pub_ra)
    if meta_payload:
        publish_mqtt_event(dmd_host, topic, meta_payload, prefs=prefs)

    ra_cfg = read_ra_config(prefs, system=system)
    need_delayed_ra = bool(prefs.get("sync_show_ra", True) and pub_ra is None and ra_cfg.get("enabled"))
    if watch and (prefs.get("sync_celebrate_events", True) or need_delayed_ra):
        run_live_watcher_loop(
            dmd_host,
            topic,
            system,
            game,
            prefs,
            title=title or "",
            initial_hiscores=pub_hi,
            initial_ra_summary=pub_ra,
            session_start_ts=session_start_ts,
        )


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

    current_hi = extract_hiscores(game_name, rom_path=r_path, system=sys_name)
    old_top = baseline.get("top", []) if isinstance(baseline, dict) else []

    celebrate_payload = detect_new_highscore(old_top, current_hi, sys_name, game_name)
    if celebrate_payload:
        publish_mqtt_event(dmd_host, topic, celebrate_payload, prefs=prefs)

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

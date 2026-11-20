#!/usr/bin/env python3
"""
ArcadeMatrix Console Sync Companion (High Scores & RetroAchievements).

Zero external dependencies (Python 3 standard library only).
Runs on Batocera, Recalbox, and RetroPie alongside the ArcadeMatrix MQTT hook/daemon.

Responsibilities:
1. Resolves active MQTT broker and sync preferences from `GET http://<DMD_HOST>/api/mqtt/broker-target`.
2. Extracts local arcade high scores (`MAME` / `FBNeo` `.hi` files in `hi/` and MAME Lua `hiscore/`,
   `.hi.json` sidecars, `nvram`/`.nv`/`saveram`, or `hi2txt` CLI) using an embedded compressed
   `hi2txt-xml` catalog (~3,020 ROMs, exact + inter-core ±1..8B tolerant passes), a per-ROM fallback
   specification table, Namco/Nintendo tile decoders, structured BCD/ASCII heuristics, and M68000
   16-bit byte-swapped NVRAM scanning (never showing static factory defaults or fake placeholders).
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
import base64
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
import zlib

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
#   tag_pad: optional padding bytes between score and tag
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
    "bubblebobble": {"mode": "bcd_le", "score_bytes": 3, "tag_bytes": 3, "tag_pad": 1, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 10},
    "bublbobl": {"mode": "bcd_le", "score_bytes": 3, "tag_bytes": 3, "tag_pad": 1, "tag_first": False, "stride": 7, "offset": 0, "entries": 5, "mult": 10},
    "pbobble": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "pbobble2": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    # Capcom CPS1 / CPS2 / Arcade classics
    "1942": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "1943": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "1941": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 40, "entries": 10, "mult": 1},
    "19xx": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "commando": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "gng": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 7, "offset": 20, "entries": 10, "mult": 1},
    "ghouls": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "strider": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 40, "entries": 10, "mult": 1},
    "finalfight": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 20, "entries": 5, "mult": 1},
    "ffight": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 20, "entries": 5, "mult": 1},
    "captcomm": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "knights": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 12, "offset": 0, "entries": 10, "mult": 1},
    "dino": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 16, "offset": 0, "entries": 10, "mult": 1},
    "punisher": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "avsp": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 16, "offset": 4, "entries": 10, "mult": 1},
    "sf2": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 6, "mult": 1},
    "sf2ce": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 6, "mult": 1},
    "sf2hf": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 6, "mult": 1},
    "ssf2": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "ssf2t": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "sfa2": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "sfa3": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    # SNK Neo-Geo MVS/AES classics
    "mslug": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "mslug2": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 10, "offset": 0, "entries": 9, "mult": 1},
    "mslug3": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "mslug4": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "mslug5": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "mslugx": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 8, "mult": 1},
    "kof98": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "kof2002": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "garou": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "samsho2": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "blazstar": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "pulstar": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 4, "entries": 5, "mult": 1},
    "shocktro": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "turfmast": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "windjamm": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    # Sega / Konami / Irem / Seibu / Midway classics
    "outrun": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 14, "offset": 0, "entries": 10, "mult": 1},
    "shinobi": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_pad": 1, "tag_first": False, "stride": 8, "offset": 0, "entries": 10, "mult": 1},
    "goldnaxe": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "raiden": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "tmnt": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "simpsons": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "xmen": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "ssriders": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "contra": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "rtype": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "ddragon": {"mode": "bcd_be", "score_bytes": 3, "tag_bytes": 3, "tag_first": False, "stride": 6, "offset": 0, "entries": 5, "mult": 1},
    "mk": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "mk2": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "nbajam": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "snowbros": {"mode": "bcd_be", "score_bytes": 4, "tag_bytes": 3, "tag_first": False, "stride": 8, "offset": 0, "entries": 5, "mult": 1},
    "pang": {"mode": "bcd_be", "score_bytes": 3, "tag_bytes": 3, "tag_first": False, "stride": 16, "offset": 0, "entries": 9, "mult": 10},
}


# ---------------------------------------------------------------------------
# Embedded precompiled hi2txt-xml catalog (~3,020 ROMs, zlib-9 + Base85)
# Generated offline via `tools/hiscore_catalog/build_hi2txt_catalog.py`.
# ---------------------------------------------------------------------------
_HI2TXT_CATALOG_B85 = (
    "c-ri}376xzk@)#ls%mC>Y>zTc0z7-{fBL@NzRwzumz0!}sDo;X%*#9b*%ycaKmrJnQdYL#&U<USoYW5>Na74c1b}~hn1A^E<3B#+",
    ";fK#Z+<usS$eFl!_u+qL|M<WY#ds<%ZsbKKE<TeNxwyC!7f~QC-pdP_z`w~0nZVcN1s4}zmlu&rCyMojPC3fpX|oS868YFL2vleo",
    "FqJyI$~&FttxoiFd8dA-JTpne`o1_)fw6ufME{PsNc2+@rZ4n+COUGGnap641%fe|8KJz8flOY=%;54ZWs;G+keLz73z>EzFXVft",
    "@<Ki&lNT~Ga{X;LI-B3?Z2niB&Ht^l`7@o(w>q2uQ|HK?&XGHv&HtgZS*9H60)+hjw!Fw8^F)5=Y=~vb-;@_jr4!4Pzom2W|HzA2",
    "JruGNa$lzWf9rgObaI_}L7#bW<2#-Bd!6{d=*0h3C;o3b@y~SPU(<<yT_-*pmR=f~r-WgkK7pHB5}G;@49hRj<rnJmW4ipfF26{Z",
    "U#wG1bor&a{4!mBxp~IB!PV?TCLi)$d_AViE@aGHadQ=!tJqv6<|;K;nYq$A9rD2X%y+sS%Tmla*|JP~4^<w+4gaEVkeQw9XXg5m",
    "dETc5_3j3)q2avKiJ4wx)<0cbMCMkz4ZpWu8ye${l~kl^OddnEfSyU`70-vCVV-<@=E;LDA{f5%U4P>+EL2YpP0^&f|BDBi3MDK8",
    "W%Tbs!KfM*%2)WlPT&XfLf-d7d6B5f2TA@&UdVU*vAmGq^b=Vx1Kq*Mlz%EOWXeC27c%9a%Zo@=S*SF>kQXxLU+Rkfg|6t4uIRtg",
    "75&$`qW{LL=)ct!{df9Xf3GWgqC2WT=#J`-x*YzbJE~N-yg%zg&UD|C>((suH<dwukr(n2f5jql1|n6E?{z`mStWF)*SQ^5OSt5^",
    "e)VinFLd7!>O$#Ts%c`wx^Sn9@>Un6sU5evC~tL9$_u7TQolG?#TKceiB<ONEKXH6W-9Y?)5kE~m6|Sr`L&v7b^~#1HGFKK%7dw|",
    "=DHg+wcJYOPQ5PI#eAdQJ5ui*tM^XSd#CEXv%X;8c`fuayM@9_CAtVwT?Cmff?U1lJM}VwCxguN;CQD8$N&1EtKm1i(ec~f=y<C~",
    "$M2}m3)SZ_^?6)<UZg%RwsKbwb>?Gc7gVDgU5(xi`e}FQgkqUtb=F^;=-#UrVz@2M_-#sMbYjbSwr~}jzTI@pp`DyP%)v{3TMyYr",
    "&KkV!O`nE(+xLBQRNe5u_mb9ouY%8X2X><d`(YEkGvnrs88<&0=662K-N+hCk;d5|f{grM{{?NtH02;CGSQ6Qv2IjWzs_`Lz;$O3",
    "=}sWl{d%JN^;GxkneNwRWzV>m*<Y0eqpH~Fy5)aMxBUNu!?vy_!{!=Ael?2yku>p07ORrUv?!USs+2RoKVwj4H*gi}$^gZ52Un@x",
    "fxPoNymvag-s|kT>+AB~i~e2RKmJU2ML*Y7?H5M$`mU~Ezcex!4F8b#FziF-!?+K*4+B4BKMeSg|1jc1*8K#oQZ>|vPz3OJD1)Et",
    "PU8!>2=tpkp9e1hogRjAHO^!4{2H!eHPVMrj=zPgR1NVV^nAaEt6YukP^BRem?HcUu0l1;hfv0UhAXZH@em~Wzrt0lI@=Ir`M<-J",
    "86C|O3>wR?bz}LBZY;mmjRjr}MhVDjsCSUVAPPbbL%W6?25A*?7^FPNVUSmUtcJMn>8AI4xCo4tWu&XXh*yD;ub|@n!c@Fpnu_;@",
    "sd&GFt4w_hgO1`ix_SQrE&}yg4C>PF;EL&2fjWfoB9+q&a{o_om8hI%kV}7ot4!rIgM9ji?n2;J_{P7(mF^=MgnmbNFn`h=%%8R3",
    "_!liW{#Exc-_Q-_@4CU<s-gG$x}W)u8iRkJ2Ff3*hW;ZpQ2tmAls{1e<xh1#{|()X{@s()ztfKxpLrvO-6y1xJvpCwLq?!qJ=Cwv",
    "bl1pr*BI%pG1gsUqPxaaca53u8gtz>zV~z>H(LI1`X2b;FG(p=3x!%1G#x0{UjfhhTtDlxfxKwTJa^~yF8|RF=dQ?e>xn$K14-0Y",
    "4BTEMn?`y#&w4GAJ2@N(AF}dsQn7PmM{@)1@!Sl;mQ3}&m#I_&v%ap}>bhdag?GBHywi2%ovtgYj-|dN4JGys({F#r^xOY!`fdA;",
    "$e*aQxUa{Q?j#HbW1H4>p)>-Y2;uP6(6@m6(@$9YSxP@mfj$E&B6JlXtw2}7jBbJ(-2^wf32tO8Ze%QObQ9djQ@I%+U3s{dWxrx=",
    "+O5}!EM-9y*y<tGcs|n=<-Mm(GmYmRS$0OXIgqx^<=<_sYOJg4AY%f1N;g{fu_9PH%XeBka`TFbp2?ycD~pB`z4up5^q5v-+G6{y",
    "F4F(dMS7!)^fO(g&g8zA8o3tr%q!=i-;%ysx!5ScXF8+)-K$IQbzQpcOAd8^x;=zR_|MWQU)2hph&LZm-9zp6`#1Y>CZoRn&rzY9",
    "hgcI5d(JoPZn+j4cuH5M8**yz)`x*11It=>!`8s`&Yt5lr*gmVXZUV={130NKC^Tjem@c>T3!fKGd!e5otYYSL~33Uq^TONWO^@f",
    "C10%Sd$P|w?F~LQvL^kMI?k8mXiY_*#r6j~x(H&@88KJyr}~JS^Ge}6-EY2E1!t?<eZ9&z)uh+tpOP!|OzSf<)4DS=t@pj!&lU`g",
    "3>4@-mJMdktZ!VlLgcR3_!)_1^s7gz*sWK^ZZ552OxtFdw#_hYo8dYkuFD+SurbVpx(zekhK=+a=~s;PD<=9CQ~ipWezRQnRG;gf",
    "s((c@froiuU_|dgkAc`DfW#9RNhZ*GJ(Q|qJ?H<i5pEJQ)lba?KQq(%TsL-@%bU6rCW&c!ra9)?IEVAq&#YOhuKdgl^f>86-M?YR",
    "4#G@_&-#K@*#Oahs`LD3I;(%KbNCl(xc|P+;9u$t{zB*LuXM-%Yb62vK)1r*XmjarwYgMIZc{zYhMwjRda@>`r_iU!322(B323IK",
    "j*uaL(dMSV(u#S)f`uh{H#VU%vnDOCLyz?@!`Y6`QzM?~*$!bI)?;C!`@KYWcY11MdRWuJruyzwmybPt8k(?lS&kau{H)O})6E5X",
    "Hu?Tl)(10@u_Wi4UUKdgsJ}Mzl;3J4+F$jk_-CUhduKFZKQS|^pW2o(t#=6%tpLH139qyUCWMTIt1QrJD#+8{8a2}Ij2h|3Mve3b",
    "M~##j?Tyi%z`Ok1=vICJ%}iVSusQz1G`;VFawXK-m`Fc^nW;8*^S?I|#Se#y6>ScL6u!_k^SfHL{(C*j{!-V-pJ^lb7rNfb?8{JR",
    "X^l8qv<jK&+qA(PXML#hHe!3IwILw3q*^_fsX7+ufh|`x$ug8!)%`}*B`}@n)|%-~5%j%af(~FP>8HB3;ynya<A<{O@1Zr>w{zUt",
    "LdsVxeU1#S-_{-Log<{#GrYlMUo{tFI{K*F&5bUWzv%>?jPmiFH+^+v^Tan`xXO?b`So)`u`(3dpaA-mrzF7lLv6j%Hhqo>YP%lO",
    ");FeWmC-cVChb@kU80LF)kT--qRYKE_&1b{M&`g~&XZDVnO?^-nA}^AN4I(st1~vWi(*VEIGAh<N3+jdYpkyfv|jzXp8Zz6$gS=!",
    "ZqFOTZ!O1&YmMQzy7Rczit<~n#J<&P>RYX*zSWiC)@00`t`@h1CEQV18_mP*P$rEAIdp4GRG340c)nFRbUU2p__f<uVn07WzZpp8",
    "wt7d8#n1x4R1FH(f6-n4ziOf6-z<N&Sm__2etk_34`0{A!#DIK>_7BN{$KhfZ_Z3>O#jdu`o@}bKG1j#ykYbc1|1NN*6++{9hlJ?",
    "4W+rUlxi(am}xaln3=&U(~6ofi_CbWRW)Izbv0;C`HnHC+?f&ly)mcU7<0<4F{jK1ruW<kn0J;NVi<5s=rn~I=-dkRpu_Yelk2>S",
    "v}hM=(Js-VU8+U9jI`I&&P=wB#|%5))IG$_c}?egJyg8whfufB_^{8vbsQ9kp{qA&4g3S_YBRHEkfDB+foK%vz8W*mJq1i0PWfMb",
    "U-wf#(EZd8^~e0kJjke`f2@0vpXgrXr@9yUneIJ)u6vJP=xqF@&H;O(6f5Bu+R<<H`0-oaG=Ha?=I?dB{6RO(KkBCWC*3svtPSIT",
    "(bed$ghKSa=bP&*_y$tYo&H**rM=N#`%SN}4D?VM>Y<crDTr$+DAL1ItlucnZ<Ojc%JduM#2cAjeAd5_8BYV<@rAm-V>&On&caCN",
    "T&z3XM9)l9Ju}Vp%rw`M@=QyrcY0>}nHE{Ka6>5g&kTr0Hv1;gYHTTw^wOG9`bW#Vi(8ILmdIrp0GR3H=z&pvTMx}}$xPX^f$0lg",
    "PfQLhh1)=9s=ZhtJoCRd&PXS;8!csIy7lIRsfE!Lk+un@Ytu~ErvKEf@^k(0fA`Ed?{%yEnr@X}_ne~tu4DqP$K=mE=V+KG#d?^5",
    "D)Mhif{2s^5i1EIQ4&O|L|g6D$c&_po!@7sd%LlvF{}DGeKeHCH_uNzzHz1#2+nvA&OEK-KfyTR>$~6g^Ck7#-#l-e@LherP?hM5",
    "f5vBo`ZMg7;wxWoJohTcJ+pTW4WQ^eC*}9v#P+5a2vqxjciC;0CMWj2wI=+z|5HC#h^uy=ke#vEy(fA*Rmh+Fuk0wVUC&YP?Qgs#",
    "C4p|up>D-Yw_>haaikk@tQ&En1?yA~G?^Bx^WL=iI;D4@>rklc5Yu(YDCr`dOR>(S#B-^>Nc(LIOJ8vln2`oCuB8ch{jcfQ|GGWx",
    "cSlvD@$mHzcW3ku#-UKZOJ=mD!O*q;vtFQ}bRTz~gW!$r|Ix2G&{Hq?=x^$e{+j;iuY0zXTe|y7jAaycZr?SI6W=pdhVL6i=?{#(",
    ";fKbd@JGh-@Z)o8RCvjs8OM~L8^@Gi7(c>a8b87>j342zjCJeR#=7+zW8M0#v2Oj&_!0iz_!0iW_!0imxDWoxxDWo>xDWotxDWo-",
    "xDUQF?t_0HTUv6XyP`W{g@~GFeaF+F|0@<G^V`ZlX>pg4$_K7J-_gR)d%DWgejv75$`iHJ4C9}x`MmSm|BWZ5`Ay+7{Yl@p4UMr@",
    "L)&x6uj#qc*KJMWnCY33i-d0M#Zg9t$aG&8W?GXHX1X^Eb0eAMy?wxl&yJyY)Z5SMPoBn|HjJi^yJdfwf=_jZm#ma>t;yWBt80}`",
    "uZ|0}$kUtK0?H=juj=eAcX>2yxi`AO&U#b$jmm}pw8e$qXB5Y~+si#Ir_A9)<?5(e{2DE*JzYyx)^Och2U>}MR*&Bs$Gz_w$Gz_x",
    "bIs3<Yu?X|-R9T26ZwPb{QqKfD8Do2wC`FPTF}aU&uG^EXlneQO^yG;)cC&`XX{@W|LE@<9ouiU7WYS{IsD$tCjVqC)o4P~?&l0@",
    "^Dm7*=ud~rUFd#(rmOsKwDk5vEu#HWi)cUAjsFWh;g@-kX+Q5wXDxi$Uv&fcp)QNx664M6{4k@W+*q%ZNc2TITsiSA-MM{Bf55l&",
    "TmHL#$hY*Xd`qYOEj3xRjlG0RLui^4Gqr~smxjo=G{nYwmiR6WsS!3ZBVy!6un3Ge@!kj#cb1z2G?0HCx-azZv;T`-4^Z>`-e{ih",
    "j1vasbFLIGmM8J8H!b*%QUBN$mcXnX3C!A&z^ok!%-WH_tQ`r=+L6Gl9SO|Zk-)4S3C!A&(6XZb!dOv%X-rtq#zV7qBs6PBLbG-x",
    "G;2pfvvwpjYezz}cBEh5Y}Sr2vv!1;wIj@|9bsne2s1Li(N&&b{rs6FGeKtmn~|7rO?Cc%jKq9nD)z0Bm~TxLzcroat*Pv{Mq&;u",
    "i&tbCU}Su!BjY;_BVS}h=h%qOi4mPsBRXeBbk2?FY+KcgGk$2i@k8UX62|>9wB8`s-)p|iS%PzGDc5_;T@qNns-GEOCcNV2%DB8G",
    "vAOC~F%L`3!;GKIVC*qpmYOd+>z{aQ{1b1D7f3&X-5R^nt+6ZF7RbyzEHe+w%)>JCu*^IxGY`wmn!>C%ZT2bWu9cIL6hotk3C*w)",
    "8dpU0N9moz?#xJgXFOBx%(A9C(vlhkrmHf`-U4Qr>Bz3KxPFqI)p)C!O&M&-_tukqebxB8fxY&ONo&e&d3?dkExF*ucM9vx`ti`d",
    "f9tJ$wPzx>`+&P__fC&!ciOq^P7iH&+RN-tk8yX}-Ry2)t4^)mFz)o|cc&fD?kqb;JT!Q~N-F(?R6ilrPcY7Ex?W}a37LL^86A7i",
    "o_OfubEkDOciPwIPD^2T#=bt()>u0-yz@jb<F$LINA5d4(|@Oj@ORpv`rWY+_l_|0-kA~i&YGd%h<j&7+&eSk-kA~i&WyNs{)pQz",
    "(dnH=Zq0~$d%?0D)9v&fTHo8)BkrvkaldOu-0ztY_xonV{ec;Ae`rSBADI!iml1zrM%<tJBks@4i2HLh;{L*nxW6<b?iXgn{goMU",
    "e{DwG-<T2iw`Rosoj>CK-i)|^FeC0C&4~LaGvfZ)jJSU>Bko_#i2L4*xOZbC?q_Di4a|renh}?o5to}0H!>q`Y)0I~jJT;8aWj9!",
    "%?GPP^oaY;jJWU2i2KfrxbMt}`_7EG@63qXF9dk!kGSvtw~V;AZ!zNDni2QbjJUUE#J#oM@juNw>Hi-q|2K@4cdr^N@BTkNR^DAR",
    "R^I8c@=lMHcZ6BwoOO86i{!l{0N1|CP=9}->+i30{r#h^zu(jK_our4{@U9`==-|f{!G`~-{^X4_7(bpuD3teYJ%VD3j7ycfq$qg",
    "@Go@5{hhA3f7KQDN4nzvQdivH>-zd*U0=V@_4N<RW;3=;SFh)@9dU2XeiW!<NQV1_4ZJ+=yp`s@JJ+{$zWj&Im+$nZKuee2Z_ObC",
    "3*9D5{f%?|jSKyaOWh{#bz5BNwz$@9aidKSm2Q+<-6(guQSNo4Jm^MQ>qdFhjqyv}7#rOfPr5O-x-oXTF+S+V_^2D>lWvU9x-q`!",
    "MyT}G;Xli%!8e6`?%BHsYuc<;Yqqi8*UR5{O_^FE^3VLp&uH%)y{5n(bM9=H!!u^BZ+o6Px7LnGuKD8qm~Z4e!us?-f18PF_PJ<`",
    "Ql9@9%);<*QfPlJnuYis&V+1ffb5$cR1HZk6aC0$LY7uPE9ZCP{&sKZ?BC*>KJr-<2I^<w{w{Y8DV^U(=l7%iTjbTpNPM(<e_lGj",
    "EBANdfwe=n#l|7~jY~TFw<xHO@k|JP>gU4wUD?0m?FLdzW_dm!2Orf#^z<Ztr86NvtDlAY8;WB{O6T{&`Mq|2SMF~w#j0Tr=?1^~",
    "sjchP@+43;i-J5EF!{E*hSB1&Xd3vwFi<xqvQd1DgBh3klm)?k(XJKYJt>B$EOxtM-pXXcSr!Zs^O%E<`@zpV-<{gWVk^^*f>|E-",
    "tf5#z!IFYRlJ?zyZ<qU{c)O*19np3Y`94KiwCxTMpRu$j%tG^8wvRB9IS922Q021Iynv5Q6YL*~vTF`Y`C(x^V_DE6MzNIz=k;E^",
    "%J4Oia39qcYHEStJ;ZB739$CQILzTiQY=x8LV_`ZRW{qyhqde?LRtMJTCgC4H_K;X0Fjo>eOcEst&dDpy;x)jT!?({gWL1L_5Erh",
    "v>Xqfj|a=f!@`+(??A!brmZFYn1!=gd;|QA;FkN%YTZIfeq?Nx#WF#zN`4StOhqWCBFv{EET$qXry|@>MOaNmSdT<#tj4qw9l}x;",
    "Ba3D^>q$YNHNh%^Z3#v_qVpN?2;vFE1BmqP<eTLQDpWj+&F^vmHcf#MWSa~WY1=tugy_~;*0U|e+=yj|>1VSfHUz)MMh$_8I~(WL",
    "MYohP0~76;2R+(LLEEB_^-k6(uDV%8iXo~BIizwi21M#-HT=CT+Ts9BhqD=r16iv$7lBCzm1?^!4i5|Y9Z4WM?htZ(+wF_0mW9AY",
    ")5gwRv4aJ7^IC#9Q$tX(TRv?!ix8d{L8Om79@JqBFDAklQ{f{I*py<Fu?7l6rNfp7*`;y;SYSxyqs~JxnsQNH0uT`u>wLI5JgD|M",
    "AV*ooagz2>R$ZRW;=};v5$=(~BTL!8@qCu{sPZCJe%hT!o*{q5;GT+a`51*({>pGR5zZ&V<B9OYRQS{j9H!pjF!>7URD^0WLVZ5Q",
    "ab4`?M?_5Y3NcR%+{o%6>nVx@l0Ci1h`dLegB%DV(LtKu#SpZjqmaLayrW9BEtf@wrK;-k$0!}X4@BS-5l(C>LNpa2o{CUSMQA4@",
    "Oul71`I_zId$!4B444>>N16WG$x;(hM#^n3gz{Y>>r|A@(iqVT^x8IPEh0uOm{aC6o*Uqk$|J2u9xNg{|7I}xK@fVR^hoWIlSew6",
    "tZMf+8-l~4sTVMy#bMtF53A*FyMVOCfSF;;&A!-G@ZMYvAcuBYpwJpeYA{r2MX+w`K|k869HJVgjiLodAe0;-P!w?-<Im9f&79xd",
    "`5igGW9N6`{JwX7uk7FTp@un1sNeT^tZR7Zk8wH^uPlUu8RIx9__7mn35Z^Ct`sYpRTgZDQq4>f(QAqk@?#Rp-m?#@#sa&uftvy}",
    "6QZ*Xds0|P%&(Wwxg@fuh1%N8iv`~KF%f;Moa!X;OicbEa9KzV@yA3wFv|>S9CVs@OF$y)@*Y(L>uk2_V8SFsg%qBg%I|^@e5Zbp",
    "Xttr$4#?ZH8OH|;xo$Q033E}sSrDkASV6&(gp8y+nv#)g0(KO0BQ`J=CiyJM6={p-)``^CU1OtiSG$=b5%<y7Myt8|U6MF?wV!ux",
    "tixu(KvxyaBG#kMLE;k`fl7i^1S67j%2psZ51VD%Kn)93x2;e`u+C;OukCEyFBh^eNyU_w8{m){X7?$dMdtU>1&ssd1snF4h#CSp",
    "(NG(fMzc823<(!LD+q+TL>p49Baj{KwC+%)k;bUi!3%8R5LlfJ-7J<(0W-)<WHX@J>JUz+qT9(jpNhUX>`CRLqlMbVa@Re<gR@x#",
    "&lL>`yHOPwj)t;xq043?)6D`kQ$$h}3m-0vf`W4&RxZNUNhq0A;KNAhCL9Z7V}a#Z;9@jz^r4H<r!LMuR?IUYW4RcMqZpYSHBMHY",
    "X`32T5Q0u9hwMyDRipH8CNLZgW#gg6c<}g>S7V{~LoFBp<`M1@*<Lc){1ox4S>so-XCWm(wUs&sOsfNRQ6}12kZ4fZ<lzX00x@8#",
    "w*GKjxM1yq$_1ST+Vv6ymP|<ZVPr^Yqn!{XVFoD*IT|9FLs@P!d2b#JSq)>9m`Shz#ZI7jvq`YtmlawoGOg*CtbUfsiAiq8dQ`Y<",
    "GnXAs&eVV*$gw*>n`K<7)YTP-@|Nnk?q2q8IiEr9ik@*JUo($X8UR9%6dtK8A|4H@O))k|JA+h=;T1Nbhhle_ub22>c>9Ot{qg{v",
    "SR4eZW+?`G-ys#V2pD(rc$N*&i9lYoW(CobS(*({WwF!IsvP3?rrF4^$uk^Ov4Q2enb?#?`OJnoFbTK~7K?5HhOaOPAR{DP?LpcM",
    "1Eu3ba_^(cN1)<x+1zI*0=c&ylsVWz1;I8p+wHJ&#-bIc_@%r0fD;WdreY6SiuvGXQL7B$NVZ^_3b<<grH}v?-F7+9vz5{Wh-Roj",
    "R33a_E8d?CYlJ|J9JC>tc!?|;z&S}PnieG9lC&jh=h9QRI$*N0BBTa(2NKdzPe&KGyLs~rFh%iGvpEHi9)yaRh(0R|4SIrDO0)!Y",
    "XfdPmnf@HfP82IB=-u7aFmi^{q^nA}6&d7yUs-HLmx3|HzzceF4J(=!4sL2x%g7lYq&Z*L$0HiTLTN>1avF<Q&>5D?X5TFeOoyv>",
    "j;tbBO|X_?RD!LW;tSR?obiY7VlisF`|xloLNOJgoQiNi6=5|M;dDO2N=zPCE6fft2J(0SVfrQtU&PKMix+gj{6MC&-jWzi1s+!R",
    "eh)v|cvFi~1kqQF=TKl(`?xt+gEXRoU`L8M5x4DT)qp@4vJjOUiZP4rKsJSYb8QS!aJy+qUVHTA)WRe!l!hife(mJrFzNnj$O;NN",
    "FRLjdDYvPZDGVpE0O}52pJ~@14H-!}Nh_Dui&DKkm)5x+)+D5(mL#ocN+;1dR7%X)yD=d$4;iB%r{Ic$YX?5APc5`87OSpCvBHMk",
    "@`(C}kR`K(^{l0s9TD&2LVm<-KU8}MycDW84y745<^xy})P;4qlm8%5F_lgRjJa?w1Uj4T%jHldE<shyF`s4oOM18^Asw~#XoHd*",
    "Eoql^y>Cz_8Zsd^COunG%!zoKqoYg!Gk*m81%do%1YtIM+8jVR9`bMo^O6#k38@eTP@t2VIW!F=LVAOVieT*;r5(<cT!E;vSyy5K",
    "3@Svs*<&H8*#ZFO5$=(_O?F!?PVg8-cg?mwM7hDRu1vBNJR<wLB3>2cqEHr`>Ms!<N!s|dF3~Y*kf^Mw6vHphlo>?99T9Oy-Mu?1",
    "m=5o5sF>s%c`?1FqnKRFMNl`*)ARngF5OQ8x+G~u($1mtLb;)tc~rOB6;FqOs-^69J2l-9^M(YGkO{X-7^0;Y9hT0^3r=#l5GPWQ",
    "2_MCr1O|I)K8_<q2bdv-5rf975wyris!DoR+w7$V%W)VfbtnMlk-{VMA(<B?XdlBUM!!w~g-6OAftbg-ZRN8Qft*`6jcD`GxTNV5",
    "vTI)KHb?mA*u3kpl~ZVlmMUpR!-Awsl2#<`Xi6vCxv`sK-r}2O*!L^M9Cp||s0>$D4b4gl&RzJR27{iR=EY*5+7Li`B0?lF3`Ky1",
    "WOf8ai>cUirLur%A$l{-)#w^C^|sRAQb}1h>q?4|5$zP`%9k;eWf~j8Y_u*;(D_OWE%@pn3LUoFsNr^SE3$%ul@Cc)ADu*(hjxQT",
    "Spnmjbm$TniPDE-@M0pkoCuz{eK8umoC*#n!r4SPp9rr;!qxao2Ct=uAj%}Ml)#Xkmg_G8=^2Y+6$v|vx=>7(Rybjf^BKqI=n1-r",
    "gZ;Qd1;I+2wQD6wndY=^_oo6pB5igoY}82;Cg`-JO`8u`MZ$ujodq||ygNZhkuX$=E5r!K2^JBIN^fbgZH-wVP3-t0k`x5OUd)E2",
    "j}|bUcOX6@ve1*yP!Vaf+E49>J&gwol9n{3lc;FA?0kN*-*vaFk41|$G1i^it}akTBb^AqH8M<6P&et&`(`|RH4zR{=9%z%*X<7s",
    "TeX-thf)X^S6TyYqMW8<u<0n{&)JKa&Y~R3R*F>wt0`8Jkdc&AlzdW0LMl}+=4pxTBn`j}BG7TSgyl408fy7|UW@daq2?LM0X0K8",
    "0<7h!R6`h2xTwWGEfsU3%#hGWC4nl65%(^AMBGZ<SvikA?dQ0TL_pO@@G9hgQxP`c53clM3WOeE9^o!Q4PzR^m;e`1|BTgUEEH67",
    "TId3@X9J6aLbZk37CTLz<)8=_tSP`dPk9!@&%*ls@_YnKW<Q8X<F_*8*bFT>1NLPH21G6`gV3!sr*)(B5vg{PS8z!}FY4oh;anii",
    "j6QGPpo?Lc=1R_Mo{#Mo=ZD$?J8xbZx$Fs8sI8KUu1NY!Q#y%fH;tBUIl5NMx}{$g=+UF=1q%Z1;pVv8HAgE~pM@wXW+qTIG6vr!",
    "F8H9!dVW|!D-E+~mW8nJ0a<0kMX}izxV-Np%M;Wjs;l^Kg9y!73~<y7!KfW+Qn=`}Jb(>45n`5N%?5qaLTR+fIJ7djlzok+(ces5",
    "s0kUU#`7L7Xj)TvdClXnG_x6)QxvKQ))DN<XVo>&uCIyQT^zYx<LV66wgpjoebF1R7|6@ZP8p44Hw88$agkzKm(5a1`qB)~VXVv<",
    "T$x~8cQO&9Iv-_mT-<{cn@jK5ILr-1&5VstYHZ9ST!mub)}>9cI^bH4JjOl(*<<K2H8k7oKp-j_ILW!Nq!@KK@rLz*O0gp&QqOs$",
    "81d0wiW{l)1D2?@JQ^Myda7ZzoTtjy6-YrKRL$A2^wHW!6@jSKa5=66sfc7e?z4_S3xaviXlsm?%_2}I0VxQi?zA9r>zY8+t!wpV",
    "7OV(F-CR*OQxQ7?x$&08V)?vOa;75f`n2C+>(8Zgat6&GE^}D?8T6N8B$bm9xbc3zKgdjB0aNQx7SdY=ga(I!tnM=C)O<)sU6J&W",
    "pj1pRo|q}XENlpOt&(^m>5IuQclNb;C9b6iP3M3k2k-sJ2h4L8#3(NSd)#5>fiu<sh9qSq<s^+9dTiHoSiKoYnXN~)&7`dsR(W#S",
    "p+k=&SBI+cNSo>w%ZJVG9)@DA{#Ix`5Nj9rOCj7q56zNr#zASe*c9Rg@vAc>JuWt@MTd)K10g7;P|LJiDF%k4p^!15nEM1hDhO8l",
    "tQb$ZXeWZlZ!gb=?u&JeuM?sYTcOTp(60^H0q>I{rUr#a)*YyNZCH7vbI8lf^H8W=ROnU3glGaDQ~(N(R6g+|bvD{=96b?`3T0Zk",
    "^j=B`>MDdx+lt_WS^`c-z*!WNn+l}4CeT3Sz5uX$2%?f;mCx?|)Yi?yhZTViphO;d)2b%!szU-EshG_~>J6KUP)<cyPDPmdLg)y`",
    "Bh05_94B6r$eZKRX1n6B7#a!}hgH)yxLAqhGw6eAX&~9*jVaDdHDeU49XOnM^B$E1o7-%KW<wTcI<b{n1m&R4LG`HyXDZ{urP$J8",
    "&mFk=u~4tUQmmk0>%hJ0Z&+YPvwFXO9`^H6{j>$Kim?zC3Yt|E>>PM3=IaeGhPp7#z>mtK6$Q5qL7kXQcW7|K046#i?C}70a|#|v",
    "$Vf^j;61EW!y)Sh24EO3wjtC-mh`YYZ0t(+xIy{B5SUHvUT}MfbjCoAV=M#t1|qBun}c#N)$YF<mM(q7Z_Q_Epn)n8gT&kcHpgK+",
    "P|F4vO)D)UuyTcm=v$>(MZwO3r#-HyWpXP6j_=QTFL4?tXvQO06QNN^Fh;O~V70{_*@MW2C+kC>(Im=v3`04B!X}5J+?~p^&NnJT",
    "I~id*3A751N8zJUR?7mvN39)Mylhr4#W@*lS*)OE{1~Y{C-5RRVQegrpA8hEChni0_exl<zzD*)vWqX}eks_oGT!^SFL%p_qOHq8",
    "C!|nApon1ht(!e&XcU0Ynz#t%g$^sm0_P)v(OB5=Y+wt$*H8{y9yY5z=&hODOrR%)kIErZ^-*ptkiup&a`i)iSF!FDmZ$k1ms%UC",
    "e|b8t3T%iGDivqy3g&>)zk!DVu2sdjDhK6)V5mG%uB}EQe1j|5;y{_P5Op>a6Npob{bQ_z-R0A~UE?y&C{RlYmru)u_=mg2F!bgW",
    "zePWUw@OEtpl2loTL&I@b%PB&j#SNiIyOxKiUAhK*u5gdwO%6Y;B|E9Aeg*oL&5HBO0DQwQ3Q~yyLitI6eA;c1hm<+%ESyE{N!)u",
    "g3<*qP%~X*MY}h9#%K!_k{25t+TsM0i&$2b9<3<G2v`xU^VrL#IMV^$3qj5`*aO4P(PBb`gz;Fe`sRwcR(#2h%B^Y0PHkgtYj&@V",
    "J4~)jmxC=VFtT$@&Ye&7$(lUh!~Rj$jX(;GxIr+AI?>RLorwt-EQi;y^kY^83jKRwqZyZR;@*gexAx<8ezY$1zpW!aqbmtEx7fOP",
    "1nDJ-R1r#0S!B~J<zk=VK2ykA%+471{FCgoZZNe(w5A~}NV=fu+QY3%R`jqy+T{v2h+>J7{59(wh(7ts1dP9-78vrhRMkh^mg6$w",
    "k1Q4aFURJfA{5gxrV~+8+*p+Lbd2feRMRo06WLD3m`>z^7>^f3nNFsgjxmzRZZqFPubT>igBeozsP@q*BpCBqceL5Iz+Ks*SS?*t",
    "sIb^!`+UE_jxl9eXAh?}ZcUkpQIe;ST3s)23%-=2sY6^vT9$#(a6Fie2hT@>oB6WDm3Z2>T(DK$qKl_A`$hxSk+7gCjprm?TC`a%",
    "UzS6Y`h8QQ>Pjk`nF1>hDCN72iai1Ma{`_y7E+K=kb97NjQ^CAvRw3Fx$>SWnETK}yW0$WWCZCzE5uo@^HGwWg^210NNxV%($<0X",
    "e2$r%%DH`yN}IJsa}P^I8EA#V0jyl=xf9;E#ACYMu3a~{(pyyIAOm>VmdhnRTh03<dlrylZp6d$9`waJ13U4C8?gS0Xkx;agWR7Q",
    "xnD(qUACC0nqYpSA`}xb)L4><NdW_efT;NiPDHX^HK5~G7e+oEp&QJU6ZuNW<KRLbr_xm<2ed1|A&X@(3Y18o=1LQOI$#9_OA5{@",
    "xF=v1`Ok*!h!}WKumLPcS`kzpIp8J=`UHn9PCnb;6KqScN1xR$I)(%D5rD`APY!r0J#e}w5Odi^xrIeN@JZ>BxkoBL<pY5Vf|WZG",
    "67Py?K8NioRuiywSu<a)Ja+FBH`a20w85@eIfb|z@y8Rv`wd!M`y?cBUV2|2Kq;nN<^(y49Kb<Xt_%n5i1dpKg1-&I%aI7D!}4(=",
    "xS9yxPK37;;oV4hTPT%dp9yW(ln#0<^uVdj(*iy{qM}B-h-$<L)>6!lsAj9fjz^+`U<-n^4r@<pDCw(6Cx%_zz&Mu%1U7)MXTFa?",
    "Uq|4W#wm?+5}#~ZE*tf3A<i$6lom1`aV%$QUH_{0d}E^8TClWZieVJ8M6S8v1{5ACJ+iXMeuKS29?ig}g{1URWuwDM)HU2QQOPu`",
    "WqoRE&`9tw#LXaAB5TCx2@3>ykiurB1~LLx7F&xHu`6JCY_zljce~SB7ex&2#=@chm%clJdJG&TV3W!wn^p0MhGmwErUYJQ8Wtoi",
    "NxC5Enxqv?-6X0S+D4wM{T_i7KB8FVqs~U)DKK;tkbZT|2An=PSaqPF(aksr(I&RSOd0;hlP)ZPMw20%JHL+&tlk<T`**!t9EvC0",
    "6I%3VvO5zOgPqS+jOBEUa5@T`j>4y-lovdyyx>vQbd+u^N`2TYPXl|!s%~(@@Cdb0^7nDI4=J{?;Bm97p%3K2pqV0FySOP9$`d$}",
    "E*h}e8(t-FB?kHk8!E<x%@pMDj}1}5f}|xy!OL3iz7oU(Sdg@$>50PSMc<${0qr|Av#qBwimaN={e3-{Lke(MoZzg8j|pg7U{g|g",
    "73F^$L?}jLJijQbm0IY4B=nKBRzu8ims^<h@gP$jJEDqUoyFSaW?OH&p}3(?Nijx2K7`$&+~AsKG05QNIf}v9A-CB91Gt6-MJww5",
    "tqY|JhFL=wGAO7}X_GapDA>8ML3<FF9ux6^RSs0!g2r4IL3cRq>n(mvgoY@^mJ}-~SW&PeA@v|XW>fEN))t~OK3I!OR^6lA0~zAQ",
    "GtQ!(oot93X&PsemD)1X0;LNo7c?8F;yg?gdv59lZg6f^!E4q!ush)^;dYaXls-~5KrvWA`Yez1GaI}hnpYk=`D9KYBC!*d+T7Yh",
    "*TAC%538N!XorLm=*ef?N4Q4bX3m|>VqM_Q-D;)lT5c8w9XtnrA2?@0UWIUM1O`{<!;Qm~IV(aTift{nEEYAMGQn|mFOtwl(AJsU",
    "UC<yXe8yl}j%1@oxxsz=7eB2NfeME;ZP{2WsMgEfPMylarP~h61~8;)L1IQzIx=_Y6Gn?+jx2v#zc(i#{Jq#J$#q?0rpq0q5(O%c",
    "bPj1YFREzd!fQ;Zh1Jsl)floq*1P9DD5y9$&P?lLC-+lqhAPyxL?td-<<d(?9ZwR%o^N=;^aae2^&Q2vx4hzx#npGLuD;`R{T=Lz",
    "TOOWIdz9X|G6*PC5UjG;qjGjvI<EEeaoM2f3FjQ$^MIX>b8;7271%np;Q81r(AkUg7>%#M@I)MsF&B!eRoZ)$D!QlXnaHqcXTUcl",
    "4qlumYIY8!+-*tdMBNrmhm$3qiqSCz5n!`lX_GwXnQ~;&up((~(}R>d%eG(rrAgTg*=ad%;S3<2;}EJ)WwTbfee)=ou`uab=fH<z",
    "vDiU^a?cs2*%Fj5xqd2;lTTm^q#?ug^LARzzOiGS&_l-|1%a5)N*^^Z6dR`J`2=l*TsOxJW%%G!7L99%tKGPG#Bs@h-iXTr+y&t4",
    "#G?o$5hNwSDjzj2!c~7fj@8+Nl8M@hWgO$SnVK;Qo-7CkFfN(|CZ@j6_>WfA>B|niBzTmf3Q4n)gv_F~kl#T1j53uSwLF>c+4dh6",
    "5WOIIgkyyGzQAnI2%*dg4xQj89FNe9C;vDW{_HHFt{M;vAR}Xj+rL(gTD?H~rB|{SRhx60I|Ra%J$COytS@p;8)Ofnu{Tz<uELWr",
    "<OWf2jvQX$M~J|3JhT`I-XBX?j>F>!oskqOU50rOi@qCntx~A4nGi*n<<MUo(b^|v<FTzzij&VO<}s(;AUnCRzDGSdkJWY;2Cgi+",
    "EjD82yEs|h=(gC4VF66<W)pVJU@5uY?hoL+!2HXUYrQ<GqrmMR1qD~znLern{=#PHDpq+>topK8mpyX&nN886vpG+M0Fe(^MZuPW",
    "od=iL4JS;UTZ{1FWWy3Y=y;;EodB3cgwP5WtifG?+@us_B;_QHY`VnmOe&T@i_N8C32e?>?%ZNLWw&^6<7F|+3uA6@j7p50Bgx!H",
    "s`Ig7vw@Z8W(3%7cDs2!7~Ulq*+@!ugC!1SIW8tuWakqn(qg~D(n-~dtL+vHk=B~9EdmispVKQYbM*xuc^YU>0U4299Ft!hGrBlt",
    "d~wXh<uNb+r0VjRmw$bA;pZQuWDCtIm8~!CSxG`hQclr|gat)A2bv9iO+TzqNwE2l)%PrbQ=Y|C+DL<n9!MNITZAaOCNcAA4qZwb",
    "SuCblLBTZv6JW8;)&xv}B{o}Aa7)1&HZu0`ad#qc1}wE%LBTZzw<M(Q&;4S`^Cbn>6x@=qpy<(qhZVy^!uvIdTaimAJXXN$hO@1u",
    "3U{-!<OebEzTn}bndzvT4*FA`nsdp`KH0s#2N8A{nGjLrVH4^K{4uuO8bq@MHWLF0L<mLqASm)IRH|Xc7y%aq+f&SrDCSkq<w~W<",
    "a4#=}C}g^<B%yQr0+;7;>Cy$qWdp%T(GYP0RX&S+RM_b+7shHXt<B(&v0JWihRL%S=WIwS0+l|i9aP`rmOf@fL3!eGvBUNFxMd8$",
    "2NYzrbHHv>?i$>?o@c4LbMs}lS!{<3Efwl)_VTy^2`*D>H+J>BR(o;w_5um_wPQrs)vL|?h;3i*BekoWMGgDJn8S2-^{H)fI}x6%",
    "<2p23yYR8uErzbGn$<QG8G!53dCpMS+xgZyh4vI<7Sx;V@?2Xa5ix?51gj`kSa6>t!SH}r`MY#MWq~v=RIbIa6kuP>*E!ftW3)d>",
    "P+KmU`(gp-(!yO*r5g=kLDG_-epIzwu&?=wRSsNrcqR`$Eym3ZU|%aK_2D|xeNin>Z3x!mkl_+c%_<v;N!Y$?hJFQ_%_+EY;IeH{",
    "y43wIvZvB!hlQTQ1~!@#OdSfO;qi#IGDG(#0F_M|+`UJj99RBDL4duw31$=Fd?GxW2#+Vii?Q&{!P?1QF943@wlF<`-pFHx50^f=",
    "_fbWlqtDzIITaph<x*$M%Xbe)`D}{e8pFMq!8>Z8z32i<G@!8ncc@pWu-I}1^ZY2_GguD|pz;VV1ojviv2!6B&!Qkg6Szduv<<CA",
    "%@+;qJc=I{1!^&(LU2&E+%N_-g&GzlT~oC15<S2!aCXGw`UzO7jzE=6?N}bk4Pl~i01J|qB%PCVP0|fXD~C3Fb@-2Rm)wil`+beC",
    "pR3hjlEDJHC{%~6A*md+1Os@Y&#OWLoA3Bm6%5?8%aKr+Y>|Px`3{eD4nwtbynszT2h&F+4;tbFcp{+_bzMBvU;v6lJxZZZkQX6b",
    "M0ygq-)uKfjA5)gYzfvU4^<x7+C*v`aA)gC3?4kRG!FZLLV^t5*M}{roTE@`7Bs`dD_qvt?6AB?0Yo2pyx-uMAtGT9E-qkksSYd7",
    "2A)nE=;`~e0Z|Anv=}4e6?VLrt}JZf4Lt87dD0B{ib=qH(y)MloP^<$qRgke0>6SAOPFMgV4Pr)#}1qO_L-(hl6V2q-7q3)M$+7)",
    "P2_!3t9`I-{9HUPHhV7t#QqmQ_o?`J>2kR-4Qi^jFuLEh8|CAz-K*qY!Sf^Qto~IoFogJ&Ikd&IR3a9MXD2;dP_S~KX-_Q1Gf(#W",
    "W8Glo)8^5At?GJ;h9A6XVd?U~(I0215zD~htWZfHCtAD2gMTBIqcmI}L^k0D-KMeDa;HXcREW4*Af{R2!Z|Ebp_YY1=&_xT1vlWH",
    "eLi^gG#Y$j<H3A9I2sR*$Aio9;N5s|I}yB^2(Kr?yQ%Q)WQ2(%#)7|u({v)axHD5oyTA=9*&`k}?J}n_wzYDvNTh)@Q%s*qZrBlb",
    "Ac&;L1-93tI(o@Lm5;Fb+pKet+_?b6g@{L5eWWR5X2fHQd$5t4*+HKMrwj$id~lDQjKx}yJiDY3BYJ~nA~zw<pirdp3msf^jA}GW",
    "=cIbC22h@BL9<!aa&N@p+Kzp*UhXz$&tlv+JNF}v5x1WgG!GkP4344zm3W#}1oW93?-Z$JIZ@Q>PWPwy2y32PNJTB$EweoWfw@SY",
    "4(pT9Mqu$e+03ZI1`M{XvC)0#Y-59dN)a4$l;iKh1FmbNV5J)gkGHZAxb8XkX}kT9d2dj8z)!sMz``M$!@_y%&8dK|k0PeV;nS|e",
    "YQhI)wZrWz1&!d)BXJS;dxm3pI}w3<H9*2LA@Q1`5eYGBYSWmC8k4EwAQ)wEAc?rhhwrUcG!1f0lEOx!0wsG7sUtHTRuXLMvCW1+",
    "escSwq!<<PK(TT@r*Tc<6-^n97bM<LblwoO^;x+j*q-<X`z!%qXz8)plS|sIgPLxyHnQ%SnD@Ay)ME`R3nJ;G6K4ih1jBagz|Ll3",
    "eyrRAqBxw%CF_QT6f7uMQLrPSpD6Vp>N(V7sHgZ3X$t&4hJM}vm`AurB9D|lu@mY}Ythf`&+h%DBNUS{P%4!kPjYvDaTQrHT6jnl",
    "$9lq{VByfVIJA$rh0jOWuME@`X}_Lo|5)r$gp~Vkfdm-4!MNeTNQ7!4yc-FBiqW)e_C47@e_7%|-f?IQeEVm)Zxon_BI$yY8HTz)",
    "myr?c&5#}nP!UE+U?!jvagVk2VhKuEeUy|!h0VIvx`lmXr1=51N)dlyX@CkV$HGQ$9KCZs5_l%;K^_Lb><@X7q_N4e>~_xTWnfJK",
    "0;_cPf&E@ItQ@*uo;H`PmQyj+H0FZ17sh-%c5`dEP^k2o-4n~j>{950Nr~Eb@vwYq?{Ow=b|gE%83QmqN@Cm=Q=x=lDZw&=6$C2@",
    "Ruk-KvqM!p0+Hf6IOrj(DA>8MMUA37><XqA30hLcMb=9#GMeffCL(SZ^<p(#_^rrro%dm1<KZWfTn`u;Qd+2fkd7qqzETm*kQK`3",
    "#A9|uIbzq=h<boCah?WSA1hK0k=bT7O0^wdvC4)oqG;MBbk1qO&>mr6F{JhVy<Cm{Ffbm~yG>coQM#7rQUTlJAy|TNSOkVEd!Qjh",
    "*l1um76^TsTy+Y2=^#3J%sT*}-lL$ZO07OV;)ZOI%+fS5s32HLu!>@^<jkd=&6;wfHW1dXuC;XTM-5^m4`T|z&Z22`2CHXz0X2=g",
    "GuVyZwdj-~pgcJj2i^>9KPI@G2wyx+Mkpp@R1-0p(Im7rD$QXh6T=D$mK2;*a6!Q}1&<ayM$naJdYe@7TU1b}w%}r;Sm{QpJg~G$",
    "F~=QVP0t`_LdWIe+@YKzSnZ=8Wejy!3RM)VNyrGgr<fbjnP&Bl^Df%^3~jp^NB0rUY9Cfd|K{pQq)4D{<+BHmIdfixn9J-gTabg4",
    "^WlXfu*w5FLixFfIn3RWR3LXlQh_=zgBJ_5OX)?c$D$H4({gB;5Kk-iShJ-YQ_`c#W~d&ME=+LrmINSn%Dt(gK$DU{j9|pAm5*>X",
    "XtC0J3zfcw3Ln+Gg$rSaqDW)}oOuO+d4ziez4mOjcF4<eu=eX%9JRNTh-AEP<)xY*9Sgb7fjUJE2L}8L^C&6bro(8(h4CiSD~oxH",
    "?r%|UdLjiGNjXU)lEx%$9a;-Pkl~<{Ga;wO@QQ;&_b>`{eR+gSQWQe1TONzMK$w<S=%=J3RFg64$r#;ajHjs>m}|1?@d2zz+EElU",
    "SUw;hz>1_DMX`)~J)p*=%a?c@M<jQ>VHz->bROb9iaZi~r1S{(*EZYFLxTD5h3e`dL+73z$TOeWa1p&ism<g*&~w|h>8O0@gsch2",
    "YaxJ6sz+<s#2S`gDCGbkJjYG-2MCHGspa<3!?~J5j9{E#1;I*!%_&AbWKKN=&XacG0>&_xt~vs)sn@Isb|P47vCU!CZ4R)zc3-%g",
    "!}j!umNH}4*03OHNzx~W3YiLq+gN)4<UOhg))K69*rwc+_=ya+s?n^VV7a3yqwvv&O>tOT3VDUr1gl-vf~X*p<9N&$UWKshjS4Tv",
    "BOI6QV6JoAoKWu76EOs(McR8Ym&H$0c38~{3U(IUEjPuYwCf|V`O;@60+oz_8-h7EuQtu@g!d{ZD9uV6cKhS8#zIPP`{82^Yc3yc",
    "@uaTX0=JV-XE{gmOp%j~j(A929As#VM4~>+W}IL<pWXYYB2eu!S0Dx0Iq~kLv`OjwbXYMKSdIlA=)eZISvTi5N#Y#+q)=tCV^f!A",
    "!y6bZo8@|Q+AW8B^J%uFVCO)|pk%_JAAwKD%@$foEH_9>dxSBW8?z*$&Iz_9SZA~6`MSm@bG0d*VwDX$@04nV3WAjcyC>M%W6m6^",
    "+sS<kp~DFy@XHhz14AloU4lE^iGT$6?To|JtPF<;xSni0!gLbV`54a)<YAJg=(qkQHZ<j6_<^t#04fMpQS3;-FFVxz1r}{j?t@Uq",
    "VCE$HFO`I0k-{UTM{0+31?&%FcI*D~vB0z0@Dv;QQ|&OQQNtFZhuf+q8et0srwtME^!Eo2xyGcIYCeE}Z}5fX(gxtgqG(Pa9mXtD",
    "`<N<tB%vEsZ5l0@cy}yLd6mK_hQaP5fi0dWx+Gyi(iKTNnvzM}Q;<w{y`|`ef)xeH+YdX6o*pSmeO57%@8v`|>KbEtPMS;=uF}wo",
    "fTx3wM`4puV1hp$Wi=V2nvAiXi~%`59_83hM(HMEY$x)4J5@04M0ht9K2|e!yXSI)^-)Z*U@r}Tr+z|(zY7NpO(qBmkL-QYSZIGN",
    "z$z4TvmJ_p&`a>Jr^hvF<Uc0SEQg&J;u)^OQDMcIz>|{wI71yE66T}QM;(EPJF!1;n3vqCSQQitDM-e2l6=-XJ(@ijHLGm+vM<^K",
    "j+^2UIwUux5SLJWZT2S7Y)Qe|hHW9|`SwOO&9a!|k#j<ZP<wPY%SBnBkw(yTCS@&gktrp*4ld4z&v&@Bg`hd?^@114Fu0fq-%f;|",
    "Cc@jP@X;5Tc)!ut>+Xl!pd%=K!Ym|(J1E2`nB_4bwHnm8<2IXPeGda(6k=OMU-@e2DKFX-9xW3`Y{nAk-Q>nNDZZTyW#hqoG`Q&w",
    "&@v;gYII%JbKF8dmIw36Et=I8)&>kOry^8S5xU6;!BmXL@d#5XD8pqO#ailX@Gx9e0(y;@qE4~Q4^4f<5>95YtXQ)x0UfqK;(F;g",
    "Qi~H5s%>^uGtR-C-C+SI8L8#9&9M-l0aA-}ln8oM66`>*+G4%&HjZ*MvS=1iFr*-(VBy22w%If0r}T`22`UgsOm3zH>(&re6r207",
    "^pmRybo3b>rX0r%m!NA_d9btZ?N&_z`np)UjbWa4qVvJCri_=jWb+34)I=_d1BZ|1c?Gk<I2MyZScnXS`H1_da!|Jwa#3NeDpl}6",
    "LObdy%%d1mjFnvjNZ}(?u-FXzS>sr{s4Iq0h-i>`04x9A>fp1jkBC?YCp9rvZ}5dP@gg~VR<U{RR<JxhjFo0wGuT?)r_84N<q5Sr",
    "v0xF@TMeO{<hD^3ogb{@^fU(LfR#qqsMk$G<rsP@^zIxe@|}bLaX1x)M_Qi{kv8hUaShivU6Izv5MQnBn!^heQ^IgP->G;g4!HK-",
    "Y*8lIs$ANKou0~Kr4%T7J*h$)f}LzuD*XbS2mmiv*5$6m1>5F;bOQ@TK^I|YNzw&Lmn6M+=)S~$Q2Mq7Vb2O1&Nq4iWQ0Ep=Xd4&",
    "J~+Q|5R&ITrW(v0(%HAvWi#9i072;ytdSO4`DC^6*vThL8y#Ae4Kb?z6sl|{rV*w64w7(>>`EUk2~<+?%6kGf1Upg8f4!X_@92`b",
    "_jbqDC5`i5OFYX+A0vHQw9CT*+%se8^9QbLCp>?8*yh!V)Pn=5H7cxF$^lI7%rPL$x^Lb#FRHew@&2^j%th-vSxnuP<4K9)XW{;y",
    "&%t>+P6OO1MzGR}vvhyEi7dT4N*qfZ#0bV+ws3*>_`M4b4yfGUVd@h16Ds!;PzD)-dl#%-;C}O$sL!*An+FYia2}}+L4d6j;{<1a",
    "4oSN{EE<ZriFfX2cJ60(?q_yx{&#NvcWy?2Z;}Pvoar_$us?ELEb7NcTWmpAxbU!A!>aFCdUT?6pkPJP&ZW(^zzj{f8nxC-dGf}<",
    "7I+d(vW5jOajF+8D;83aQIJzGqF_wHgn&DWEhu<$;qj<8&NBNUp4RnlS7WBfTIZl3bLeLNFzBJguW<NtA$qYrJ~2@PykZpv8wz$L",
    "#5u_ji;-zQeD-eph-(E!v{apc5~msLWjTmYPR6JvV;sk0G<!A{;X$j1^!)6!*&k2rh!s3nRS{X`v51IwfD_JahNlpCjE>1v&+lPQ",
    "<ATN&g(Et4gww(vi(#`xja?S#sk@?#qzz5!BpN@BhvEdXt<>cVQ0X!;U0)3=+Tr14IoMzPv^iN#;`G4mKtBB0KG?rP`)55I+PBYA",
    "&D;z$Py+b}#cEi%R7(#^SEUH^5g(#maV(YjF4NLHFzl&^?0(fY9^0TU)?sUkl_5zPNx4P$sJqm;E`LJS#u+mUj1=WGj%XayIPoy9",
    "@jtV~|8yvhYSZ&LlU{<r=8vl#EdO93E-9CAwZ(M+gDK*neiYLu%!WwK*%T{DNJYiZj$!*8%}x|6C|FW(PQe8QD+)f^iPpQK!EKJ>",
    "%<L5m^h;oBr(htsH$oy0{o}|Mn}-cPIz!dDW?Ki=xE4Roa#X-6R5`3si}?o6IBe#RuxOvjRh`oKAukfx2|$HnM=zZ=pr4nTAZ_PY",
    "F;Gz}eMW;6qHrrpeUBSe8I7{+8}WtBo_>fbn;naJiOZ8s=YXhnkU9+`mb?3*zC*!)q@hQb_!!+wA!&UA##e{IENjT-Ll;9N(1pNp",
    "jJam_I7-WXBtXS0jVsu$Fmxh#IT6gp!@IHI{jqT}czN8xaok)T2?eYHPxRrJKZ1f!e0gNhOM#Mv+}Myq<;Z4urmYOV@0w@Gnj9=|",
    "h7}HkCk^y0;7GpnS>>XpEe=oC7OJP?e1ZA|vQxv*;(YK@ZBSvpTYPrWDCLg+a9}4J>C@KC#$}2v9VP}}*sM4S<eunk06Ukq>ZmyS",
    "RlPNGr=yrj7tOG80th|Q+*@e5xW@rG(C8laQAh%HHY$L^BMXnLJ<_@4sq%@F&_*4ll1L|)tVfJs++o}0DG!IvR+^PI+$rs3V#KMV",
    "bT~qPqeR+R<VYm0dWR#7fnH7HF-8+<HitHZx7H1?*%$40G2GYebnIS~qA>|_w2%q-tj?@SLStA}@aRz7F`%*Z_!=^ja)P>1!9mmP",
    "GAe!+_HTIvZo5-;E7fR3#yGXjMjg#)b~)9sAnDeoBKiB({@ngk5bW;us9{0UlB7$Iu8J06WGaxLZ39-eB#hP8hzOx@`@^Xfgeo2;",
    "vsj_W!3K)(J9GiI<wS167Y|@_uD8eF6LrstObHBqLn?bq7|W9|GhdIeaOiTmd)#7;%hf9{o3lM_E4o>%pQ6skagUhIb|(9U#2I6j",
    "Hp^mr(CfwW0V-^EU*cxUM%yWnI)chnP*JNE>OGkG6krtvK?&v4xlfzQNwq6g>t(V#V2GM}&Y|~XT*++RVNo2^7f0AZbaGh}?uVnc",
    "bOSUzgC2kwMFt~RXHF<fSd2z=&GrryH>wk-LUS{8N>-oLQ5}_pHTb<K%da9Qhv3$?2}@uZeh&+hV$Vllc7}6@Q$8rChkCbrc18zL",
    ">G#8Yd{pB?u8QMvv)`-wk*fNKs32JBu;UVk3nL~xEgv?RH;Fv&J{+*hf-l9>lUlyalvvpo^V7UQvmLHD{UH51mAtG9PLpzo{H#D$",
    "xUg9bZ);Zb;m`KeRp1(xNMknC=~j9V;hM}?IgcpBeN^}e7A*{zgewAB4*<i@`Mvv9cy~U65b=)17KSpGVKHmg+HfzICjrXgZo@(f",
    "G754Jwq3b&py;{ab$BoX%?FaoMom-T&a??v=lLpD5^PB^>SoXbd9b#jI{Y-@OqtFRg#=>+;{+=T0%EgwU`sLQ?$sWj%hkXo!O<q`",
    "B}>}%%W&U#!IlR+VFq_Uz-<svP{Y8}`OxDAB|$OVN?W<OklO(Efz@c}el!%eDH;i8=Y!jN_1q1+Flo56wy2TT6}72$e^dgThymie",
    "5Q#w!M^c6aV+7+qt2|W0mQ@K)l@36|lBAtOmF0WbWo`EE+*P51U~`)tv<6m)lLNvmbZqOR-3@O<AM0$^*Y4N&4x&k8;sP*s#2qb#",
    "ER$<?^rIaMkNKo@$f5NBvmbVlFVJ}`B3R*(W927*tUXqE#J{ieiGN$?d>@Q_OdTlK9^l}}L~bF7?N)%wAusx9>RA0`&hOIsUAeyx",
    "E~p*g{VrT^bU<g_Qp1=-pCl*)i_y?>G_)HJ9ZUKs&yTmx+(gzm2owd7>(a+08^F$?O|i$7`h&(>H-}<Tht@nuLFPc%_tq#`+lkFm",
    "RCqWQflWo=QxU4k2xBktjH?k7k)zn*9^pvnB#LozB7YYyC@mm{=I#A%ZXeaF=z^waA|GBnemLEOBrLbX$Pqm>-KiLEz^hql!)0O4",
    "(p6`>iZS}QTMqqG6e%oJ3)yaI?WQGIRaiDHB!!PkA5}hT90aXA%KHz;4Hz86eH8g9_K}*?4q^9%`_KuwS?&jcD-Z&-aj7emOEIP+",
    "VYm027MD?~*;EVa8YqF7Dc3i}7y*wI!{eS^S~;*O>wDCl#9S4%Vl@Rj2j0sgM1b{YtqFWYry=p#kpU;0w7pXl5;?0`!=zPiU&sdR",
    "WWmSHK}!FY6;Z>^qia0AH;Gd?w$rfAhIM~_m^o%l(>aAvNcX54_xZT4@kA`tj(q5LqJ7~UUQN1yp#NeL=StY^4(_h^iWC--rw&7>",
    "kOb%p+11B-3neGbbg-HSQDw8^VNpNgqKpW)Ym<Lruux$g8Te3$w}q`<66vNH0XnQ2IG8a>IPAy^s4E(LGa*)o@c_<AS~*m%8E)+3",
    "*)-%N?I`NPQ=!rnG97FViU?m`F5rrLRyeRzJC&Pt>I%JBY&kEbAPU7B;qd_Stm2U)L*XdnvEajE*`gE%=S~h7N}4`h(0Ey%N##s3",
    "-5GqK`6GoZ3X%yl6r`W^;y}@zEQi~$D#R#O5-_q^Q!dq5BxYAog7=#TFxDk<7eerrfvX67Wgg@)t*c_VQ|7~F-l};}D4IFQ0uP(T",
    "vc`^4yKOyemXF7Jg9As3hun#afpzv_v)b$moCwGx75RXL6f7uMl5j`SIRzIKTv4#1U`@e}gv_H<n%>=Nv3!!sMfH(EA;Btw9UN9m",
    "4RSR+TSl>#Vr0aQfHpgY!87)ep|T@rR1vK6nH|kbO9D0|-EOYFR9oi}6$C3?wnBxbK5Gw2;iGDZ9)%!U+mrdnVu!-5XabPi01BHl",
    "3p`ZKtom1|a@cNoQjGXr-=hXyNl}_~7COp*1B<Fg4aJxRg^YIEZUznb5i}y=okU**GN0V0Q15Ik0>jZzHX1sfax@wmkB1hc!R2@;",
    "?)Q2=d^C8m+;vZ&G0EfvAqEJWx!HjEr1KE>NaPd$w%8+yM{19po==Q~g-4x7E_XYvTh)GHnpHMDwJV&%n)<br8&`vWMaZO<GZ?_a",
    "p-;zV3EuP{S&S<M#YHkhg}4p|Ro`d?HWh)Niy%bZBRaq9Et4KMyXUrkgqe-#hotwBU}Eg4Eg!4L%@G|k`h{zcV#I9@k1)t&O!Umq",
    "df{x=BvZ>p#n0CLU09&57sut??4vSZTbnhzg+A9t4cCw0X`E<pHSu$0{f6c2Kw><BMWKRVrOjjkijM;*-+&KbXwzqz3-<xQR6IEt",
    "z`~-^!o(N&jv3DG6)Ff;xvaMARfGvkp@LxWdGaLtsAqGViT?tCJx(+!d{o&8cO6P{A)0}fPy!Ts9aIo(NwA7woyU?Rf&6IxLkl1I",
    "Z{fc~<=!UdgqtJCXJT{%eg4z_;dBoGPd!0U_yoNN9L5O7Jyv>T?UBkS#1kvxiA1!{C*i_p&Yh>-Qs=$$fmEm@SY@&2vc{7l6M33%",
    "0D^p;>WxF6i}||U9|jH?&&6T8TslWPIg-}%QZ7IMpAsHxPYDfaT2a)2o2Bw4&D61#K$wr(dz;l=_1rD6-Q?(D@lvR@S4!pZQaqsh",
    "b1LVpQEE`-FwZ`Xh!KnvtRz@vv*i|dj7WnJZJ;l>%SW#(mVkL=jgxWC7@UZwQ9-bc!`$uJ$=%sCEGamr;DUlD5;h*);I_e9v!fY2",
    "$%1=uU$t(2#IZA#y9EYA7?SkSqiTomR9Zk8z#6ya43tv(rCw~%OFRt{+)4kX-WBpx;Gu==rQVkfo)HVa#=(FcNm$vm7LDx)Pf4}I",
    "m4b|<+@UYe;(t#VH$%}vu_uZRBL2_pgA^tH@nHr3FFt|IK8RZKq5d%YAeult6IY*yvk#&`|M5Y5^@maf>;Q?h!$71e?)jhp5h)E4",
    "NidN)CL&}tnMy#WB5vpS7XJ|OWh9XkTyZDa%t}^1TqGO0>54leouWu9qSFeUcZ|t*JnDCh2Ja|{=zD}9D1&1N&L)zlQ$MJTh6MWb",
    "*)aW#czv1hsm}!J>!sg+Bz@E2k4~9G%85u>RBCYx+{@smM7{tPom5<i2H}2DK;{1M@tY5W92sU<WFC9$%iK+#*~y1kxPE!~$;aMT",
    "i3qX#fYdH;Og{|cbfo(9#c-w;(TPfB$0sD})Ey$H&o-ZAtoe-A$vJ$COuYRtY}?5o^$asQf`=q$9>OoE_&cgu<&b)m4YDS2Q@4Ak",
    "a3t}O-pQ$IDWt-Z?2>*Y5{Zeah)mT=GO{ZIU1a?`(Fwn{g$Gs<ZB+&x-KL#k(~g2OPm0gJE?3Xv-tz_#T8sd_M?Bhd$LG3^NIg^+",
    "oO%vb&*Dom9IJxK2-8wzc*EI;Gxa06u&1+Mmk%3wbWtyd8Id_D`>o@AUy)tXRk-6<IOs?(OEKg@oLrfz+f1&j(_t_ye5-cFf%D4I",
    "M6a;<Kr}JG)sA+wMBANXd78orgI7rbZjv}i-$dbH{LjWqDVykSxZR1HAtIXSy{B{JT>6+JxtD%Ek|AOuaud!d=_noMPc+CEKg%y_",
    "5QAQKkZ^1UAhKPMmE|TP^XAa^!T2CHY|EL>HTi(ENoPF#%F+&{Dfm+oza(#1_LaSV9HhxtmN=8UjlE&!5Yom}ACO$$Lt}zr%#&*q",
    "CFB^V_0R(~DZPd1S&??o@pCpl1v=q7$N@l3%guNy8|5IXGg*m4HY%5AlX-rzFdkje(}u&YFxGX>?g4SIN}^XKI66VQtfJ*(6^%^s",
    "5>X(3Q&Au{HDMqNL|F8B6p<B~+af_QS!~Xy=NC^jMub!AZpM<ckB>jK&v@I;Vyc&L<fe<n8WxO8O15B(8Y6zml$V8LbKZ;U!TEwD",
    "r3uD@?EGZF|8nN&JuaR<az9J0FXy9KN@?=W>bJsi72f%Yh4n(wXi+etQ@$Z`l0{y1%#;{7`iV-1XSFkHykZl@JF28J9~KY4_*~B!",
    "l8T|pw3Df{U#<FM$&Bbeljz#vZZn)#`$BYP3s~vYU`Hg!spP)Wx6EJhmeKTECKpacIQ7{|OL$cUu%*<#H*zGaf%=A}IOmD1%Av%n",
    "G#_ule6mlF^H`pmUg30BuXkz65tVW@mU1?wC;BvvOLo!NmtC$yjxQB3r-kjIKF=t1d#&yXOIp?lo(x_#r$p>vG#8@Ry!81V!kZ<0",
    "^5<`+^rk$RS$aFGmt^B#pI=f3xZTB#e0|EUOX)fd`R0S88=sxMjg8$luGb7MX~0Q5XugaPgva_iTh=;LsLGNS_DXTTW=FxRbYOIq",
    "<d-P|ql<ctp`9hw1?9dSJ%5}?iq7M7Ox&i`|D03JM(Jr;INH-)VHF-c=1S}ES%KF|KJraZ*N(cw#(}xTDC5le%;M)%Ca-=RM}_81",
    "PWNf)+kfCSPkh}h_gt^XFOcN5b$Fmi3XL#7u$oC#7K#!xN;tvB%Q%sGZOimZN1YJQP~v!UQ7zki@a$rP{7;y#9XT^<kE-9bm~h0W",
    "6DDA*A4-TE&*Ll88nxpDqzeFLwy<x+@i@8QK7N6g=Tj~8uG&&Jm1OAUkzUfJ#h1*%c{*m_Vb`d&*ehS>60NG;UXv@dsywp?ff+T|",
    "LP08fRf5ys%qgXXCkw=mt$|4BhFz7=xN-4PQ`hnAgq8NMy1sx%d<uMdEenEyNEnTYYMB(oC#@E2Vxk?5xfNVwv881c%YQ`)f*0qD",
    "(@PBIW`p513T(ZAB|z}tRh`rsE32|TrxzIpMrIkqA&bH7UR7tZOowxVt!rlU-GDMEgm^-J*qp2XVPdz<k@UhmdE*p$&fYjhxu5P;",
    "DbjT^yI7xik5h{CEtMx{o{^lLQ@Z{i*`P8qs={CD#&m8P!v_z*`327?xi!o_wVRxyhVA06+!*Pv5h$W~=w*4vZG<tZt7NpMWNHj&",
    "ukoH4c9{{%KsTHX`po=Q^SZN-8K~_))xt62SxLEd$<?3q1Y}rpnwjZ&a`oJ1<G*JZI-N&EXf3TN3{mbOvl9s^Jvj^GOH=u3K5OR<",
    "M^Sv`=e=g!blZ12BZe<Ei#s!l#Sx|ZOh$}E@+o~Cx5jJNK|+_A{S1yumne+RKjhWVu;*===b>}{85jBHXP#R4=-XRWr$?DsukpyY",
    "Z6_uJ(6524I9D3x22x#&QXMB%&%q_;44%Gf5*A<L=IV6Wv2xIBqF^XfvsZQ6w7ooLvsb!BpVw2y%BeCV-IX~TuYmbg{ui^ersXh-",
    "uJ_@X*31#YQ{s7qFDZ|3aI%%UlH~Qd%w7HFO+qR-&x(Z8Q$#t$n39Y}=0M}EC7JZ&9-XZuwhq*pw2al&<QfnD809#YlUm~FY(ZUB",
    "QZY(i3Edj_+ey}OdX0f#vX6F$@#(1|J;;XH_#o?yd~vV2A4UvD$J$7D&TMoXoPN~ESc_FU=NCNU6;|glp?X5^bMZWHSo2YXlJI`u",
    "?N_bj$!o-SdXd!VB11<tnZ8yv=^Fu4YU~c9OB_Oya6G%&w8)VRT4jQUQBKaQ`Rkt%o_~l_octBWC&&EZkMc=xCOlb+)G;ktaFv(N",
    "xbiT&M55B8v8kKGS$q<Ciy6&qe#&d+3UBRXxBxL^S7>iXWUP@f#aj@@;KgkF3lCnmo{zs`QQ+&=ExN7Jvf+@;{;JHpaDbjtNX6qT",
    "bNOf$eoZ!{pOOt1cp3ETPN!<tq}}}0&&fwEY5cNvQtWlQ2BRCgM17H7tG!Tt^!a&W@-_;%*S#sV%Is2Ky7Z#jJ8J&p(ZzDo(H7cu",
    "@|IaBjHw>?(&u<mU2^?{rnR=450r!J5vIY@RzntCzT%rjS1&~6dHm@qPfqX8?qraSZec(rn?!LGO<HFxCt^Z*m0U2y+0%s!@-oRT",
    "Uh-~tec=Vhd($Su#A_32U{%%d^^Ue=PNw1LLQcmw|Kgd$dAoUZmCt4LdimvoT6Z>|vg?iCs>y{(Hr_p?Y-am&PF7^woq`m$=_wX_",
    "?K=3Y+`I>3ME)u<g3xuOpSp7Y%1r>W;NrxlwdK0^W#?LX<SmU!xsj$!to#~_3(;#5QqVbZ-pvy>InIrE7fyONzuIaVUy{^>-JVa+",
    "%Lg-yH0q76uQ#fncSjsFgfw=lz~FgW_3Z23KXD6!k<ACll=&N^oY>M^t`@1M$)rnQX!WDGLOXF*sV9zm<AvUap*PT^r0mC&%U|+1",
    "m>%gR3}m8d{HCd9N>`SCVycvzAPXjz`+KQo<2j)0;uG5+ye?lE3xcmA*_;J$UtT<kg10ZPh=s}9msiAc_7!Ao#FOYNNHz|#H%(Ui",
    "kp<D)msi4r>@CZSsZ?K`1Am*8NGDVVX*wHdP59Mbb<T)6bOuN>d+su7-n?x8B475G&p~a$joK81(bGoP5hFjlrp7npXTfk%pp5`y",
    "tC*rBqEkMz$(Eb+C6k#n&nHwJ;n2q;a+f>QR%$olCA&<KFJg=5pZX%ftKWksSH8xmIIA4X@^SBGNPD2nI^+13P+O65K1{s(aMi8Z",
    "1-D+ZZNj)o;yRnzaN@{Dz27g{_AK<jFnX27z}xF+=m?eZtG)h5rpq`X;iSVun0(5{s%N&bWMj4Nq6wPcr(V0OXMEL!e<lj^^E?mW",
    "KlD~L*RCIz84+Kwq30)Sz_Wx<Oiw)~z5X%jX!<O_Zm$pKR~!2~9MQ*le?Hz3k{W#H0lDMrO05+Sy*=Vy^@P`Hrnujx_$O?lHRdUv",
    "W&Q%-uy;(3FIf|Pb^7dWc3#Qy+4RyTZzC2`CB~+A5BY>;gjaYX#@A#(9+*|ZIs?vaJI$i;R}Ze}-SY4f@%#c0$1n;8n~^eSfZ|E;",
    "2}#3ZptDKG?33O)AA9R_o4eFwtEtAs348B|X!6#pw(5vI{d(jm60cwW)}56QayA?ov+LTz<hIFYJ+HidMx13Es``gvp8Lh0`9eDj",
    "?70KGaJ_TtE*nudL^-|mom^=YP0lthdGS$at!reRoLk#$jk?00T}c=^j>4huC_I>}hQ>!&K0orO!&9cobaGScGv7KqnjpDkG>9&7",
    "8jIMNQ9ZdvO!Pe*^JF%<advXCjcZt+^3-1xckPChgcr{_gGZjt2L9lqt@~4+$S-|lFfLSFBG~1*@7y(Y3_^UN52f#NHSQtwX6e$3",
    "vk{;0#P6xh1IGL=*d-3_XSNm(0?QRt^f}=NZ@)9&!H0Zg#29<)r;b>S>rqd{3v?3d&hSztSul1u1EXg2VM^(Oh8Yq1$FL+515$Fy",
    "8q(+@UF!vk4QGLx`?A@@<{!bN!yOwRCX=hqT$n2>{P@`mmu}(|d)dma@gjS(1jn{jrJW(;Pj<2-$4_?Iz&W<DuJXk?v-fnshYPP>",
    "x$(Z&+g!8rR(y>vH4I{JMJ_Sl4CIrpTa3o8eu|iiDS<j7l*^n}CtOhN*ju;@sqQd)JI|xDZdd%16b$bCHa>emNN}k({_LuxXjsI(",
    "mp@U&Qe6z%`DoMx5su`{a1Q#K{57h+@a*PV*D0^~@as{6zDcvuO%(Z?sLC#VT6EzKhL@dSMEmPyqh6Z);=RHmN+gQn^V_?fYx>rc",
    "!-zgLh!7CY?Upt*f^n}-O*Q}ID)BadO`m_!K1pHts*RHBv(uLgrB+Uyd-7VL)p;p?xt&+LXWMt)gy(ZI3@=L~cuf*^-i#dp5{AB~",
    "eM*1h>lc59;YDiUc+|I~K~2^~C1e*@!jVmzdk6VgJt|B{!|`xT(iY?F5^H^OkxJCI73QyVgm>0!j8CRAa(Bu>#}-ex$e!Ecf{kx+",
    "k)7M&Vs<{s_$x>gW=n6L=&RrKEw<rskJuXTmvhpgE<d*~KfB0p`t1H)XVVR@-0DMRG<)L&S@6~gW^bGzPhOMYD>oq?sbtc*%=0&o",
    "uIP8qY*d+^nfUon$#P<&%6{{{i?;<y+3>R>Y9W2_CdY<`s%73>vse0rpObUyL^ga{LQa#D@rBJi92Zs3pBFmJm&=a@@QXKA2V=yS",
    "tYl@A$8e_CZsZ@jlI6pNJNXB%ypN6DcftF{JbmRWi(9kc^|y|{_~e0M@mrh|<$F_Gv$A1L$Sz($ciDaw-UISC+1md6eg&WvJ1_0>",
    "Pgd!{lBGC|v{@nyhi)VxbJQ`ODg9OJo+CE9VEt7XTyxS$$iks*DzZn8DEOoeiS4s#nPt1WcBhAl6_eS7<79Hl%C>Cw`dyd@=DG0P",
    "mW3hXp7QQ^UfmX5a$4N9c6Rjai8JbT77abnuGn4a+IAWb?d*;);cdv3U1QWICfh8TUA=>>r>P(pFoY}}W>{!#`7^35;L~O=`{2>@",
    "cJE|DC)j?tsGeP1&Em_R7hmtHG@_Fo^z0-0?<ji3K}6?|8ldgLldH+H!NzcbRdYV!1P7{WK*nUVYYmT=?SKh$$8odRrQ1-`CtdU4",
    "x}&q41vvD)%UkU`*gxKO)r+GGm0$O$M)b`j=ag=jzXSh-f%Z(MhC?APe4X`pL!LA_-KpuFyrb;Ae%0!k)!A6l*l8M8Kf`!1mb9%H",
    "sK*?=^?eURRo=+`V>;u}8<Xf0pPVP3n1H_s*h=Ao1d`yDA0OuIqL=@~?FpbRalgBoI-XkXj_Dn<>7^abuHCFcwS_o|E|F%PWfpW=",
    "S@w2mi2?Nu6N}j!CYD8SmlzA*F#kor_J)aNZ&(04e8U3Z(Hj;3Pv5WrqTme+Ad23w0HX8_3m^{OumEEIh6NBOZ&(0H@P-AD@HZ@g",
    "BzeOENb)x<fRw#q0i?+r7C@T6VF6_94GSQ9+cEf)3ZS+U%SQ*{(f7#Ta>&i!a>xyXw;XfB;4KH;FnG&RHw@o$*bT$C9CyR;EeBp9",
    "ExxJj`IbX($UbfCwVPzfM+e{WH{owN)`t8o2V1#(^et0+%i%VR-g3MRqqiJz#nk34OCi2?$bIc%El!($d~JIayjo>=@rKB*o|9c}",
    "Kgx(L=%<W6;f33!3@7-pzcA8YT|jBfvvZE~go+@)@+{26-yu4gy~*Nw_~zt_We?e;hgCH6t>7tTh&^*4bUv}(v~_ah&5fb-k&Dqq",
    "u3#4yU&!;Z@*_`F39dd*<r5BOnZ4aXHK9Ht)?Sw=^7Advs5$eL<ib43J~2;N^cHzCd2CPim1Ie9v5V~V@w>0wF*>>UZYsN|yOaO7",
    "x@*~$<Hn)?;&mAisb_tRH_7V4K_-9=Pd54Yao4TtmP!;wO1eg7-%Zs^5=EZJ)V~n~e;;q(-AzZaZO|-LvzP6?C_xj5rNk6~J|fi+",
    "$;*hWh*G(|XcZa=S5Lo^_UGQaaF)d}*3G}9<nbOuVbK`xxVTd7MOr^FAHuHVfSX@9maC8F<HKph8tUXIm?lpUc||?nHg>*q!>F$w",
    "BO$FA8K`}1taYB^YuePc>?ne_&IeTRVAsS(t>WG9r$=Uig>2(`$Yy0S4aHTN?mZpSO-SiezU=@D0s|6}L;E3+fw$+f{l^_&6o}U6",
    "Z=X02?UE}}yq4jRN+~UPhzqu)w7^A?r`*r=AhL<hbGt1QOW)pbbxNb|W|~Pzj5-O(L?|vPQb*oZ^dT4WXR+TgzdQdPaD|eg`H&|v",
    "SVwSiV|;obO&NFnezJ`J%Dk{83$WuaI_0t|o<2-ZiDAf*rZU3l>H2M_<Ul-?N3Rm!L^p))?@;xOhO0*kdK;O{2vZJ(Mmcc8q!m1{",
    "F0CLq9Ty^R?5S_xX?cymdsG&GfoQlt3O=1K3(4cA)C!S4XUoaOjhN3TnfE+URigunPWrIc!d|aIC=b?++~coSPkWFc7$n@9UF4nk",
    "UvVdK)t#*NIRx({deRCwZ|nO#DUlbCrquy4`i(nr)xFd)C*&G7FNVIC&u7<NB7$IvD7E$-eQe&^|ES=qy?>zVQYIQ*l($d!?T3(a",
    "EY#P(pXn4SN#C_$?lSVcvfFrLJY+1}YzIFHGR{$rpkxMs={n~;i6?oHa72lOC3?Zb{`ycPDjrjDr1C`$dXkZ&@*NPeNx2zzD&<T?",
    "`czgp5TI}%9Iy%?eNVyfZBNYsbLXdcSI2d^BZw?h)hhDK@JefMwuDeY)~Ux8I<k=g^4+P?bL_o`vA|5vxb+?|Cwhe`M?lG|mJC50",
    "T+b&D6;$QQH(m#eS`#z)Tkawn44seRBSsTdR5F!NfuzBJqdEk;wROlVgL#^}v4>giM)xMVyYz7{E?+_5u7U(+*l+hoj<p#*C(_46",
    "1#l*J!D0{0<at&iY_QDB2ZE>MqTgRgJl}^OY`HS^YcfR5of-&Y2Sqk-N4MuCRp^JWkHuyuyl0A(C4{ZCyWJ>+#S_0+rQggb!V7o5",
    "<ih>Lg`0uW8H6a%H^2MWa5|^ux#2lXq3{8y83!<Z_)xnF`n6G@EaKp&hmS>R;sTZWT`#E^IHyC{^Jf`EJ%J<=f5?}RsF6oV2J6r*",
    "?Xw^fk=X}UT*{4VA6zQQiJQR8HC>62F02okmW51<UPEvTUF0DwcaiT+x@hDG0JR!Y`~Indq3@Tv`@6<}`jSJ$KmVxjPH2h%5pH`f",
    "`EF?0Ld)TH&O*c2yEkM|%$Nb=v8KkNw?Q0k^!ggr;aKP=c#)q^N4&`w@lW9*%USs}s1Csyq9Rtc2&k5TvWWh@E>xgX?3EYrZ~!6L",
    "v~dBEJuyU=m&;Kzf>||!MthCHJVwuGuen|#WF#0(t3Y6(WchWXP*D2Tu+gYB4rD@0^WJW;3B5&}z#>p&pJ)Hbo+ubrBnpHExqfW}",
    "?Kx?CzT8pYn2Ukp3XmqChY0t0WK!j`M2GH-y!F<kE;;P}jT&u-q0y3)YjEShp53Jc^*KR>#paC-WJDE;Udt8NSj@OPg6AbY+43+I",
    "`EB1W*RW}YBp|H`84E+ancjI7InRXrN;R~&YdhL1<;7%bL|Vs*?%bR}VszsM>n}|1nSc`QY9LbFSWH3j1dSSFT*VkE8RSa~Cyw<#",
    "Z+)FFC8S`a``H=i#Un475@EWuEr>DL^|_kGsmXt^W2l3_i_RAfK7eFNVf4GeDDBPXgusK1iGfa!UT6hNE3RmlUs1ngN87f_{0|T(",
    "upZna2Q&1@McWNsIrS{{|Dp-#LJ3yBWP<d`6R$Feg_r4;ACDs+EMs+hAXa>o+M2)LmA{81Fxa}i^w7!xLVNFX&sxi}<Pc87oJYx7",
    ">SQLCDbMhgl>=l)n>s3zMUyys0R>`70!n_jpE8I!9@e)f1hb2sEHFU$U<A`I$Zb3ircpunY0nND=KKk0BaPM^Nx60F3vPjuw^y#0",
    "oD)LY<xU7eNi}}N-_6$Y;AQlW2{V}EwD*h(j0tx#hPEa4UG?+tEmoMxjlp)U!#j0++fjSS;>n9-%+VVBbHaGyjoSS9CS!_T8V{z&",
    "#|t$MFB|Xr;eW4jgD87|dNohijK~e&@rY8c`FTe0g>-@cDo~%eDkrD1vkzFe3m&Tr&rD2)mjdvliXZqk+TT_jhFs2uK{Jqj#%L0+",
    "+aSuIKtUDT-6k)VU%ih8xZ}qi^;J$SU#i=0#Rz4`F&Oa(Y#elJ;*?Fh@i=Ru0LtSGa9LjVbhRIPDrBHiLtx=v*GWQ?<>NC6Y%>n!",
    "_RZ@)$MFaLZ}|-VpC*Z7d0DoI&`Fh3SwTMzxs2Ti1jo{lfNWL2()h$TcTSUKzcbbYQ5eEVL&(t3tiumM0kLc<mw5cS<4Dz3=$2%s",
    "DRT@)PSOggvl*AQZmtw+Cl6jX5Kn5@Ixs`9#Mh`6Oia1$5z1|kB3)7G4|Ve2QG>%0b&U3y$9IjCb(cX<ip+E`cGQlTB(_Q{*iXuf",
    "bexB^qXdo|Si30^XS$(1A*4<RVWA*V>{Du8r6v4KP)Ybb7HxrapN+)U*_H0=1uxgPB+&&FFl1*?DBVWPW70JC1K$xzu{}n%BaXzy",
    "(&;3<Q(31IP0^cmB9z^gK%FD}_VZ`3nGL^ew|+2LJdCpF*sq}GA-4tg=xGu+g9Zw3sR5agTbfoed``%d@9<7Jd&GND+uUkM45r58",
    "LYT{2Yn?;$UI6-RaitRO;YU#OU|WK?*!vm?Trg&y)sNKrv9s~H3X%$LZY?cbWI|{HWkOilz9u(4DkS}|ds6SC_XqbhENtXKNuisO",
    "qx$mKAM(`EFZ+7sU#-g5we<UAdC+x=CvA8*VwOY^)g4uGN@&gst*D8F;*Fy4rd-DlMzb@J;tqo|Opq|jNNyq$Jq00XGMj7ndHDsm",
    "Rg4|i`kY2tJ}w4KRpf8$&2}fi_fO>tA#MJk@M1x|$~43mj|$3NeDNv+`S1a23<4HS2bdur<@U%eIQI>kx;0O!fUKqbzu^Kwae<1q",
    "^sO|3U`31#YdQzcn|?#C-`?QG=ssUdJ1G-jJS$wl#>qV!Lrbd`#v<qnZGcdScmCY6OGDmrQyJ3#E{}DuJOs@OL=QSvQrrm~O90ef",
    "@=c3iH`y{qaJdAClX%VdPch&rB!p`q<9B)gX@bK#R^4vUVVyKs7wZsJ7f0!!#nTzG9YRn-NPyFqkV4|Fq5CLA5S2p<^X{6{>RO!<",
    "okAhb3u-B>gcYs975!th@o|i7n4fvj)G$tXJ76a7#|m#~@6dksqRPkOG7>BJid5UK3&#}%F{&UME@)c}DFXr#-w4bSeR3+Y26Eks",
    "F|c}<a3P}9A$KRCAa}H&#<BcFPPG$d@Mu(n->E_#BpM~z6@IdU?rfqG=#NI^;jcpESpiI-jiUuQcK=>|hnou-Gkq#ywdsfu$3hNU",
    "5dqk??+Tcdg!(^q<d~<Mm?`kR`VVFS-XLgtO-8hUIBxxD*uUXN^hq!HYX~3ECp}%>FsR$4?)WPAvZ|1jT{172-(364@^yl&$(kq$",
    "(?8n>L|i}%Mubb0I!UU4*eO)LeHtkp)77(Y@Aq0M0zzI+cjbxDh7tcT0<EGv61whl#^yOnS^vR2suhXcQk8?=OPkW_$HGBEe!Dk;",
    "<!|rJ{zKV$&!cntB$aE+uBhMY#O1tz7M|-l=eH1^V%pcN`}WE)%872-lp|g4U}EylEmaEgUUd@HNWE<3iO+J;t7B~RCFi$h;|-HU",
    "VE#Oy6^^BEpX62(C+ER3oUEd$mS5n`5+2BL{Niym$HaCi8m+t(t*V#iitcDZlAR?Dik`?>Eo)$M2QwSl$*i-;w-4ZGZNCIf80Hu?",
    "5EQLz*EMuH{N`ROA<O8=j7a$iY1Tov>c`?i6n#1`VMGgE)n!1;a8U>tw;l?Nc~B{HFeX)Db*JcG)AjgA(O~03_%IzJrP&aGW;%q&",
    "=@3cwI5{Mv#zib`O65tTr>-gc>h>xAPna6w0wk5ATb~T8qd6#y44PET`y>?R_w-^h)Qyr0?fEVN_lyO_!WfFxT%bk~7kqpxzYsaX",
    "PuI&mX{DcckT-Q1-65h()n?@25KMzQ@*H=PYKdz1&l*rn%3gY!5R)Mls=?8Wfi$KDb3BWEO4jUqeyQ&Oh3i^nD(X>Vdsx)WjlF>*",
    "h7e;JETG@uMkDXO|HZLC56a{Fb9>g^tG9}7*=W!o&C5z_SW|CMXROo9rMacdqC^5Ubk5}tH6GlTg0#~{?d}{m+69{pMZ0~-KGI&v",
    "p;l!y!Wre8tY_HS4woI)v9TglvzJTxpgTzm2cmsgDMwUVxEap*MpaCx+vr+PKcQTz1dk%YNa()qOe0hL`d%t4ZA%((Q<t;n$9j%*",
    "MeX#<4vnM|dtBga??zH9EaQ-z?2)!s5js-saHLU3RRN@Zc|QWBuf_6hvj*!sr-D)NB)V8P)n6D&88F{#6z<U_i8Bsr8se^>Pfq)`",
    "9&SPR>n0Ma4;iQTMemwy)*A|Sy-SQ&gb@c^*PnkcN|7FF*Y1Y;=~GYQg96v}rDc)RM@6o!Y*$6LW-79vZ)8RJN8vKQukcVtZw1~=",
    "q8yc$!?Y;p;-82LT;pF(6?+0vH;PQ`IAdthSn*2d?Pd!QW>?l?Nlg=d4anP30du^2rzwTUC3-m)h0Pwq%1u&aPcdDJ#f~!R-UDZP",
    "$|}A9ssW`}ZqiW5Ip$+qJ_pjUCPNz&gTA1`_}fXzNGj98q&ekuRafv_Yd<H`w3zww&;I~XiI(;",
)
_HI2TXT_CACHE = None



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
    """Ranks candidate BE vs LE vs M68000-swapped record lists by arcade plausibility."""
    if not records:
        return (-1, -1, -1, -1)
    full3 = sum(1 for r in records if len(str(r.get("tag", "")).strip()) >= 3)
    mult10 = sum(1 for r in records if (r["raw"] % 10 == 0))
    descending = sum(
        1 for idx in range(len(records) - 1) if records[idx]["raw"] >= records[idx + 1]["raw"]
    )
    return (len(records), full3, mult10, descending)


def _get_hi2txt_catalog():
    """Lazily decompresses and caches the embedded hi2txt catalog on first .hi lookup."""
    global _HI2TXT_CACHE
    if _HI2TXT_CACHE is not None:
        return _HI2TXT_CACHE
    try:
        raw = zlib.decompress(base64.b85decode("".join(_HI2TXT_CATALOG_B85)))
        parsed = json.loads(raw.decode("utf-8"))
        if isinstance(parsed, dict) and "r" in parsed and "t" in parsed and "c" in parsed:
            _HI2TXT_CACHE = parsed
        else:
            _HI2TXT_CACHE = {}
    except Exception:
        _HI2TXT_CACHE = {}
    return _HI2TXT_CACHE


def _extract_hi2txt_slice(data, pos, sz, allow_partial=True):
    n = len(data)
    if pos >= n or pos + sz <= 0:
        return None
    if pos < 0:
        if not allow_partial:
            return None
        return (b"\x00" * (-pos)) + data[0 : min(n, pos + sz)]
    if pos + sz > n:
        if not allow_partial:
            return None
        return data[pos:n] + (b"\x00" * (pos + sz - n))
    return data[pos : pos + sz]


def _apply_hi2txt_byte_transform(raw_slice, spec):
    if not raw_slice:
        return b""
    b = bytes(raw_slice)
    perm = spec.get("b")
    if perm and len(b) >= max(perm) + 1:
        b = bytes(b[idx] for idx in perm)
    w = spec.get("w")
    if w == 2 and len(b) >= 2:
        out = bytearray(len(b))
        n2 = len(b) - (len(b) % 2)
        out[0:n2:2] = b[1:n2:2]
        out[1:n2:2] = b[0:n2:2]
        if len(b) % 2:
            out[-1] = b[-1]
        b = bytes(out)
    elif w == 4 and len(b) >= 4:
        out = bytearray(len(b))
        for off in range(0, len(b) - 3, 4):
            out[off : off + 4] = b[off : off + 4][::-1]
        b = bytes(out)
    k = spec.get("k")
    if k == "o":
        b = b[1::2]
    elif k == "e":
        b = b[0::2]
    elif isinstance(k, int):
        b = bytes(x for x in b if x != k)
    elif isinstance(k, str) and len(k) >= 2:
        mlen = len(k)
        b = bytes(x for idx, x in enumerate(b) if k[idx % mlen] == "0")
    if spec.get("l"):
        b = b[::-1]
    r = spec.get("r")
    if r is not None:
        idx = 0
        while idx < len(b) and b[idx] == r:
            idx += 1
        b = b[idx:]
    return b


def _decode_hi2txt_score_field(data, base_off, spec, charsets, allow_partial=True):
    raw_sl = _extract_hi2txt_slice(
        data, base_off + spec.get("p", 0), spec["z"], allow_partial=allow_partial
    )
    if raw_sl is None:
        return None
    b = _apply_hi2txt_byte_transform(raw_sl, spec)
    if not b:
        return 0

    enc = spec["e"]
    ns = spec.get("n")
    val = None
    if enc == "b":
        val = 0
        if ns == "o":
            for x in b:
                d = x & 0x0F
                if d > 9:
                    return None
                val = val * 10 + d
        elif ns == "e":
            for x in b:
                d = (x >> 4) & 0x0F
                if d > 9:
                    return None
                val = val * 10 + d
        else:
            for x in b:
                hi, lo = (x >> 4) & 0x0F, x & 0x0F
                if hi > 9 or lo > 9:
                    return None
                val = val * 100 + hi * 10 + lo
    elif enc == "u":
        val = 0
        for x in b:
            if ns == "o":
                x &= 0x0F
            elif ns == "e":
                x = (x >> 4) & 0x0F
            if x > 9:
                return None
            val = val * 10 + x
    elif enc == "t":
        cmap = charsets[spec["c"]] if "c" in spec else None
        a_off = spec.get("f", 0)
        digits = []
        for x in b:
            if ns == "o":
                x &= 0x0F
            elif ns == "e":
                x = (x >> 4) & 0x0F
            ks = str(x)
            ch = (
                cmap[ks]
                if (cmap and ks in cmap)
                else (chr(x + a_off) if 32 <= x + a_off < 127 else "")
            )
            if "0" <= ch <= "9":
                digits.append(ch)
            elif ch in ("", " ") or x == 0:
                if digits:
                    break
            else:
                return None
        val = int("".join(digits)) if digits else 0
    else:
        if ns in ("o", "e"):
            val = 0
            for x in b:
                nib = (x & 0x0F) if ns == "o" else ((x >> 4) & 0x0F)
                val = (val << 4) | nib
        else:
            val = int.from_bytes(b, "big")

    if val is None or val <= 0:
        return 0 if val == 0 else None
    val = (val * spec.get("x", 1)) // spec.get("v", 1) + spec.get("a", 0)
    return val if val >= 0 else None


def _decode_hi2txt_name_field(data, base_off, spec, charsets, allow_partial=True):
    pos = base_off + spec.get("p", 0)
    sz = spec["z"]
    if pos < 0 or pos >= len(data) or (not allow_partial and pos + sz > len(data)):
        return "", False
    b = _apply_hi2txt_byte_transform(data[pos : min(len(data), pos + sz)], spec)
    if not b:
        return "", False

    cmap = charsets[spec["c"]] if "c" in spec else None
    enc = spec.get("e")
    invalid_cnt = 0

    if enc in ("r40", "r32"):
        radix = 40 if enc == "r40" else 32
        chars = []
        for off in range(0, len(b) - 1, 2):
            w = (b[off] << 8) | b[off + 1]
            for c in ((w // (radix * radix)) % radix, (w // radix) % radix, w % radix):
                ks = str(c)
                if cmap and ks in cmap:
                    chars.append(cmap[ks])
                elif 1 <= c <= 26:
                    chars.append(chr(64 + c))
                elif c == 0:
                    chars.append(" ")
                else:
                    invalid_cnt += 1
        raw_tag = "".join(chars)
    else:
        ns, a_off, a_step = spec.get("n"), spec.get("f", 0), spec.get("s", 1)
        chars = []
        for x in b:
            if ns == "o":
                x &= 0x0F
            elif ns == "e":
                x = (x >> 4) & 0x0F
            ks = str(x)
            if cmap and ks in cmap:
                chars.append(cmap[ks])
            elif x in (0x00, 0xFF) and a_off == 0:
                chars.append(" ")
            else:
                code = (x // a_step) + a_off if a_step > 1 else (x + a_off)
                if 32 <= code < 127:
                    chars.append(chr(code))
                elif x == 0:
                    chars.append(" ")
                else:
                    invalid_cnt += 1
        raw_tag = "".join(chars)

    cleaned = re.sub(r"[^A-Za-z0-9._!-]+", "", raw_tag.strip().upper())[:3]
    has_alnum = bool(re.search(r"[A-Z0-9]", cleaned))
    return cleaned, (has_alnum and invalid_cnt == 0)


def _decode_hi2txt_template(raw_data, tpl, charsets, shift=0, exact_mode=True):
    if shift > 0:
        if shift >= len(raw_data):
            return []
        data = raw_data[shift:]
    else:
        data = raw_data

    w = tpl.get("w")
    if w == 2 and len(data) >= 2:
        out = bytearray(len(data))
        n2 = len(data) - (len(data) % 2)
        out[0:n2:2] = data[1:n2:2]
        out[1:n2:2] = data[0:n2:2]
        if len(data) % 2:
            out[-1] = data[-1]
        data = bytes(out)
    elif w == 4 and len(data) >= 4:
        out = bytearray(len(data))
        for off in range(0, len(data) - 3, 4):
            out[off : off + 4] = data[off : off + 4][::-1]
        data = bytes(out)

    rows = []
    for grp in tpl.get("g", ()):
        mode = grp["m"]
        cnt = grp["n"]
        limit = grp.get("l", cnt)
        max_sc = grp.get("u", 999999999)
        if mode in ("c", "o"):
            vstart, stride = grp["o"], grp["d"]
            s_spec, s2_list, n_spec = grp["s"], grp.get("s2"), grp.get("t")
            is_bcd = s_spec.get("e") in ("b", "u")
            for i in range(min(cnt, limit)):
                base = vstart + i * stride
                sc = _decode_hi2txt_score_field(
                    data, base, s_spec, charsets, allow_partial=exact_mode
                )
                if sc is not None and sc > 0 and s2_list:
                    for sp2 in s2_list:
                        sc2 = _decode_hi2txt_score_field(
                            data, base, sp2, charsets, allow_partial=exact_mode
                        )
                        if sc2 is None:
                            sc = None
                            break
                        sc += sc2
                if sc is None:
                    if not exact_mode:
                        return []
                    continue
                if 0 < sc <= max_sc:
                    if n_spec:
                        tg, clean_ok = _decode_hi2txt_name_field(
                            data, base, n_spec, charsets, allow_partial=exact_mode
                        )
                    else:
                        tg, clean_ok = "", True
                    rows.append({
                        "tag": tg or "1UP",
                        "raw": sc,
                        "clean_tag": clean_ok,
                        "has_name": bool(n_spec),
                        "is_bcd": is_bcd,
                    })
        elif mode == "s":
            so, sd, s_spec = grp["so"], grp["sd"], grp["s"]
            to, td, n_spec = grp["to"], grp["td"], grp["t"]
            is_bcd = s_spec.get("e") in ("b", "u")
            for i in range(min(cnt, limit)):
                sc = _decode_hi2txt_score_field(
                    data, so + i * sd, s_spec, charsets, allow_partial=exact_mode
                )
                if sc is None:
                    if not exact_mode:
                        return []
                    continue
                if 0 < sc <= max_sc:
                    tg, clean_ok = _decode_hi2txt_name_field(
                        data, to + i * td, n_spec, charsets, allow_partial=exact_mode
                    )
                    rows.append({
                        "tag": tg or "1UP",
                        "raw": sc,
                        "clean_tag": clean_ok,
                        "has_name": True,
                        "is_bcd": is_bcd,
                    })

    if not rows:
        return []

    if exact_mode:
        # Guard against random binary garbage in non-BCD integer tables when exact size matches
        if all(not r["is_bcd"] and r["has_name"] for r in rows):
            if not any(r["clean_tag"] for r in rows):
                return []
        return _normalize_entries(rows)

    # Strict coherence validation for inter-core tolerant pass (±1..8 bytes)
    if any(r["raw"] < 100 or r["raw"] > 999999999 for r in rows):
        return []
    has_names = any(r["has_name"] for r in rows)
    max_table_cnt = max((grp.get("n", 1) for grp in tpl.get("g", ())), default=1)
    if has_names:
        if not all(r["clean_tag"] for r in rows):
            return []
        if max_table_cnt >= 2 and len(rows) < 2:
            return []
    else:
        if not all((r["raw"] % 10) == 0 for r in rows):
            return []

    if len(rows) >= 2:
        if not all(rows[i]["raw"] >= rows[i + 1]["raw"] for i in range(len(rows) - 1)):
            return []
    if any(not r["is_bcd"] for r in rows):
        if not all((r["raw"] % 5) == 0 for r in rows):
            return []

    return _normalize_entries(rows)


def decode_with_hi2txt_catalog(data, rom_name):
    """
    Decodes a binary .hi buffer using the embedded precompiled hi2txt-xml catalog:
      - Pass 2A (Exact): `expected_size is None` or `len(data) == expected_size`
      - Pass 2B (Tolerant inter-core ±1..8B): handles FBNeo / mame2003-plus / MAME Lua
        trailing/leading check-byte differences with strict score & tag coherence checks.
    """
    if not data or not rom_name:
        return []
    cat = _get_hi2txt_catalog()
    roms_map = cat.get("r")
    if not roms_map:
        return []

    rom_clean = os.path.splitext(os.path.basename(str(rom_name).strip()))[0].lower()
    var = roms_map.get(rom_clean)
    if var is None:
        rom_alnum = re.sub(r"[^a-z0-9_]", "", rom_clean)
        var = roms_map.get(rom_alnum)
    if var is None:
        return []

    variants = var if isinstance(var[0], list) else [var]
    templates = cat["t"]
    charsets = cat["c"]
    n = len(data)

    # Pass 2A: Exact expected_size match
    best_exact = []
    for tidx, exp_sz in variants:
        if exp_sz is None or n == exp_sz:
            cand = _decode_hi2txt_template(data, templates[tidx], charsets, shift=0, exact_mode=True)
            if cand and (not best_exact or cand[0]["raw"] > best_exact[0]["raw"]):
                best_exact = cand
    if best_exact:
        return best_exact

    # Pass 2B: Tolerant inter-core check-byte pass (±1..8 bytes)
    for tidx, exp_sz in variants:
        if exp_sz is not None and 1 <= abs(n - exp_sz) <= 8:
            shifts = (0, 1) if n == exp_sz + 1 else (0,)
            for sh in shifts:
                cand = _decode_hi2txt_template(
                    data, templates[tidx], charsets, shift=sh, exact_mode=False
                )
                if cand:
                    return cand

    return []


def _decode_with_spec(data, fmt):
    """Decodes a binary buffer using a single _CLASSIC_ROM_FORMATS specification."""
    if not fmt:
        return []
    stride = fmt["stride"]
    offset = fmt["offset"]
    s_len = fmt["score_bytes"]
    t_len = fmt["tag_bytes"]
    t_pad = fmt.get("tag_pad", 0)
    mult = fmt.get("mult", 1)
    le = fmt["mode"] == "bcd_le"

    entries = []
    for i in range(fmt["entries"]):
        pos = offset + (i * stride)
        if pos + s_len + t_pad + t_len > len(data):
            break
        if fmt.get("tag_first"):
            t_slice = data[pos : pos + t_len]
            s_slice = data[pos + t_len + t_pad : pos + t_len + t_pad + s_len]
        else:
            s_slice = data[pos : pos + s_len]
            t_slice = data[pos + s_len + t_pad : pos + s_len + t_pad + t_len]

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
      1. Embedded hi2txt-xml universal catalog (~3,020 ROMs, exact + ±1..8B tolerant passes)
      2. Per-ROM specification table (_CLASSIC_ROM_FORMATS + Namco/Nintendo tile formats)
      3. Structured BCD (BE/LE) & unpacked/ASCII record scanner heuristic
      4. Namco / Donkey Kong tile heuristic for unknown classic clones (< 256B)
      5. Plain ASCII text fallback & single top-score BCD fallback (3..16B)
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

    # 1. Embedded hi2txt-xml universal catalog (for .hi and compact <= 4096B buffers)
    if len(data) <= 4096 and rom_name:
        cat_res = decode_with_hi2txt_catalog(data, rom_name)
        if cat_res:
            return cat_res

    rom_key = re.sub(r"[^a-z0-9]", "", (rom_name or "").lower())
    fmt = _CLASSIC_ROM_FORMATS.get(rom_key)
    strict_nvram = len(data) > 512

    # 2. Per-ROM fallback specification table
    if fmt and not strict_nvram:
        spec_res = _decode_with_spec(data, fmt)
        if spec_res:
            return spec_res

    # 3. Structured heuristic scanner (comparing Big-Endian, Little-Endian, and M68000 16-bit swapped NVRAM)
    rec_be = _scan_structured_records(data, little_endian=False, strict_nvram=strict_nvram)
    rec_le = _scan_structured_records(data, little_endian=True, strict_nvram=strict_nvram)
    chosen = rec_be if _score_candidate_quality(rec_be) >= _score_candidate_quality(rec_le) else rec_le
    if strict_nvram and len(data) <= 65536:
        swapped_nv = _swap16_bytes(data)
        if swapped_nv:
            sw_be = _scan_structured_records(swapped_nv, little_endian=False, strict_nvram=True)
            if _score_candidate_quality(sw_be) > _score_candidate_quality(chosen):
                chosen = sw_be
    if chosen:
        norm = _normalize_entries(chosen)
        if norm:
            return norm

    if not strict_nvram:
        # 4. Namco / Nintendo tile-coded heuristic for unknown classic clones (< 256B)
        tile_res = _decode_dkong_hi(data) or _decode_namco_tile_hi(data)
        if tile_res:
            return tile_res

        # 5a. ASCII text fallback (e.g. "1245800 RED" or "RED 1245800")
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

        # 5b. Single top-score BCD fallback (5..16B unpacked/ASCII like pacman.hi first, then 3..4B packed BCD)
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

    # 6. M68000 16-bit byte-swapped retry (compact <= 512B .nv / CPS fragments)
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


def _safe_mtime(path_str):
    try:
        return float(os.path.getmtime(path_str))
    except Exception:
        return 0.0


def _detect_configured_core(system=None, rom_name=None):
    """
    Lightweight reader for `<sys>.core=` / `mame.core=` (and per-ROM overrides) in
    `recalbox.conf` or `batocera.conf` so the active emulator core is prioritized.
    """
    sys_low = str(system or "").strip().lower()
    rom_low = os.path.splitext(os.path.basename(str(rom_name or "").strip()))[0].lower()

    conf_paths = []
    env_conf = os.environ.get("AM_SYSTEM_CONF_FILE", "").strip()
    if env_conf:
        conf_paths.append(env_conf)
    conf_paths.extend([
        "/recalbox/share/system/recalbox.conf",
        "/userdata/system/batocera.conf",
    ])

    rom_override = ""
    sys_core = ""
    mame_fallback = ""

    for cpath in conf_paths:
        if not os.path.isfile(cpath):
            continue
        try:
            with open(cpath, "r", encoding="utf-8", errors="ignore") as f:
                for raw_line in f:
                    line = raw_line.strip()
                    if not line or line.startswith("#") or line.startswith(";") or "=" not in line:
                        continue
                    k, v = line.split("=", 1)
                    k = k.strip().lower()
                    v = v.strip().strip('"').strip("'").lower()
                    if not v or v in ("auto", "default"):
                        continue
                    if v.startswith("libretro-") or v.startswith("libretro_"):
                        v = v[9:]
                    if rom_low and k.endswith(".core") and (
                        f'["{rom_low}.' in k
                        or f'["{rom_low}"]' in k
                        or k == f"{sys_low}.{rom_low}.core"
                    ):
                        rom_override = v
                    elif sys_low and k == f"{sys_low}.core":
                        sys_core = v
                    elif k == "mame.core":
                        mame_fallback = v
        except Exception:
            continue

    if rom_override:
        return rom_override
    if sys_core:
        return sys_core
    if (not sys_low or sys_low.startswith("mame")) and mame_fallback:
        return mame_fallback
    return ""


def _matches_configured_core(path_str, cfg_core):
    if not cfg_core or not path_str:
        return False
    norm_core = cfg_core.lower().replace("_", "-")
    for seg in str(path_str).replace("\\", "/").split("/"):
        s = seg.lower().replace("_", "-")
        if s == norm_core or (norm_core == "mame2003-plus" and s == "mame 0.78 plus"):
            return True
    return False


def _rank_hi_candidates(paths, cfg_core=""):
    existing = []
    missing = []
    for idx, p in enumerate(paths):
        if os.path.isfile(p):
            existing.append((
                1 if p.endswith(".hi.json") else 0,
                1 if _matches_configured_core(p, cfg_core) else 0,
                _safe_mtime(p),
                -idx,
                p,
            ))
        else:
            missing.append((
                1 if _matches_configured_core(p, cfg_core) else 0,
                -idx,
                p,
            ))
    existing.sort(reverse=True)
    missing.sort(reverse=True)
    return [item[-1] for item in existing] + [item[-1] for item in missing]


def _rank_nv_candidates(paths, cfg_core=""):
    existing = []
    missing = []
    for idx, p in enumerate(paths):
        if os.path.isfile(p):
            existing.append((
                1 if _matches_configured_core(p, cfg_core) else 0,
                _safe_mtime(p),
                -idx,
                p,
            ))
        else:
            missing.append((-idx, p))
    existing.sort(reverse=True)
    missing.sort(reverse=True)
    return [item[-1] for item in existing] + [item[-1] for item in missing]


def get_hiscore_candidate_paths(rom_name, rom_path=None, system=None):
    """
    Returns ordered candidate paths for <rom>.hi, <rom>.hi.json, and NVRAM files across
    Batocera, Recalbox, and RetroPie:
      1. Explicit `AM_HISCORE_DIR` and `rom_path` directories (`hi/`, `hiscore/`, root)
      2. Primary system family (`saves/<active_sys>/**` covering `hi/` and MAME Lua `hiscore/`,
         ranked among existing files by `(is_sidecar_json, is_configured_core, st_mtime)` desc)
      3. Secondary arcade systems & BIOS `hi/` + `hiscore/` fallback directories
      4. Primary then Secondary NVRAM (`.nv` / `saveram`) sorted by `st_mtime` desc.
    """
    if not rom_name:
        return []

    direct_hi = []
    primary_hi = []
    secondary_hi = []
    direct_nv = []
    primary_nv = []
    secondary_nv = []
    seen = set()

    def add_hi_to(bucket, d):
        _append_unique(bucket, seen, os.path.join(d, f"{rom_name}.hi.json"))
        _append_unique(bucket, seen, os.path.join(d, f"{rom_name}.hi"))

    def add_nv_to(bucket, p):
        _append_unique(bucket, seen, p)

    env_dir = os.environ.get("AM_HISCORE_DIR")
    if env_dir:
        add_hi_to(direct_hi, env_dir)
        add_hi_to(direct_hi, os.path.join(env_dir, "hi"))
        add_hi_to(direct_hi, os.path.join(env_dir, "hiscore"))
        add_nv_to(direct_nv, os.path.join(env_dir, f"{rom_name}.nv"))
        add_nv_to(direct_nv, os.path.join(env_dir, "nvram", rom_name, f"{rom_name}.hi"))
        add_nv_to(direct_nv, os.path.join(env_dir, "nvram", rom_name, "saveram"))

    if rom_path:
        rom_dir = os.path.dirname(str(rom_path))
        if rom_dir:
            add_hi_to(direct_hi, os.path.join(rom_dir, "hi"))
            add_hi_to(direct_hi, os.path.join(rom_dir, "hiscore"))
            add_hi_to(direct_hi, rom_dir)

    rom_sys = _infer_rom_system_folder(rom_path)
    all_arcade_systems = (
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
    )

    explicit_systems = []
    for s_cand in (system, rom_sys):
        s_clean = str(s_cand or "").strip()
        if s_clean and s_clean not in explicit_systems:
            explicit_systems.append(s_clean)

    primary_sys_names = []
    secondary_sys_names = []
    if explicit_systems:
        for s_exp in explicit_systems:
            if s_exp not in primary_sys_names:
                primary_sys_names.append(s_exp)
            sl = s_exp.lower()
            if sl.startswith("mame"):
                for fam in ("mame", "mame-libretro"):
                    if fam not in primary_sys_names:
                        primary_sys_names.append(fam)
            elif sl in ("fbneo", "fba", "fba_libretro"):
                for fam in ("fbneo", "fba_libretro", "fba"):
                    if fam not in primary_sys_names:
                        primary_sys_names.append(fam)
        for s_fb in all_arcade_systems:
            if s_fb not in primary_sys_names:
                secondary_sys_names.append(s_fb)
    else:
        primary_sys_names = list(all_arcade_systems)

    core_subdirs = (
        "",
        "hi",
        "hiscore",
        "fbneo",
        "fbneo/hi",
        "fbneo/hiscore",
        "FinalBurn Neo",
        "FinalBurn Neo/fbneo",
        "FinalBurn Neo/fbneo/hi",
        "FinalBurn Neo/fbneo/hiscore",
        "fba",
        "fba/hi",
        "mame",
        "mame/hi",
        "mame/hiscore",
        "mame2003-plus",
        "mame2003-plus/hi",
        "mame2003-plus/hiscore",
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

    save_roots = []
    env_save_roots = os.environ.get("AM_SAVE_ROOTS", "").strip()
    if env_save_roots:
        for sr in env_save_roots.split(os.pathsep):
            if sr.strip() and sr.strip() not in save_roots:
                save_roots.append(sr.strip())
    for sr in (
        "/recalbox/share/saves",
        "/userdata/saves",
        "/opt/retropie/configs/all/retroarch/saves",
    ):
        if sr not in save_roots:
            save_roots.append(sr)

    def collect_system_saves(sys_list, hi_bucket, nv_bucket):
        nvram_dirs = []
        for root in save_roots:
            for s_name in sys_list:
                sys_dir = os.path.join(root, s_name)
                for sub in core_subdirs:
                    add_hi_to(hi_bucket, os.path.join(sys_dir, sub) if sub else sys_dir)
                # Dynamic 1-level core folder discovery (e.g. mame0258, mame0278, mame0288 Lua hiscore/)
                if os.path.isdir(sys_dir):
                    try:
                        for entry in sorted(os.listdir(sys_dir)):
                            sub_full = os.path.join(sys_dir, entry)
                            if os.path.isdir(sub_full):
                                add_hi_to(hi_bucket, sub_full)
                                add_hi_to(hi_bucket, os.path.join(sub_full, "hi"))
                                add_hi_to(hi_bucket, os.path.join(sub_full, "hiscore"))
                    except Exception:
                        pass

                add_nv_to(nv_bucket, os.path.join(sys_dir, f"{rom_name}.nv"))
                add_nv_to(nv_bucket, os.path.join(sys_dir, "fbneo", f"{rom_name}.nv"))
                add_nv_to(nv_bucket, os.path.join(sys_dir, "fba", f"{rom_name}.nv"))
                nvram_dirs.extend([
                    os.path.join(sys_dir, "nvram", rom_name),
                    os.path.join(sys_dir, "fbneo", "nvram", rom_name),
                    os.path.join(sys_dir, "mame", "nvram", rom_name),
                    os.path.join(sys_dir, "mame2003-plus", "nvram", rom_name),
                ])

        for nv_dir in nvram_dirs:
            if os.path.isdir(nv_dir):
                try:
                    for fn in sorted(os.listdir(nv_dir)):
                        full_nv = os.path.join(nv_dir, fn)
                        if os.path.isfile(full_nv):
                            add_nv_to(nv_bucket, full_nv)
                except Exception:
                    pass

    collect_system_saves(primary_sys_names, primary_hi, primary_nv)
    if secondary_sys_names:
        collect_system_saves(secondary_sys_names, secondary_hi, secondary_nv)

    # Direct core folders under RetroArch saves root (e.g. RetroPie) -> secondary fallback
    direct_root_bucket = secondary_hi if explicit_systems else primary_hi
    for root in save_roots:
        for sub in (
            "mame2003-plus/hi",
            "mame/hi",
            "mame/hiscore",
            "MAME 0.78 Plus/hi",
            "FinalBurn Neo/fbneo/hi",
            "FinalBurn Neo/fbneo/hiscore",
            "fbneo/hi",
            "fbneo/hiscore",
            "fba/hi",
        ):
            add_hi_to(direct_root_bucket, os.path.join(root, sub))

    # BIOS hi & MAME Lua hiscore directories used on Recalbox, Batocera, RetroPie
    bios_dirs = (
        "/recalbox/share/bios/mame/hiscore",
        "/recalbox/share/bios/mame/hi",
        "/recalbox/share/bios/mame2003-plus/hi",
        "/recalbox/share/bios/mame2003/hi",
        "/recalbox/share/bios/fbneo/hi",
        "/recalbox/share/bios/fbneo/hiscore",
        "/recalbox/share/bios/fba/hi",
        "/recalbox/share/bios/hiscore",
        "/recalbox/share/bios/hi",
        "/userdata/bios/mame/hiscore",
        "/userdata/bios/mame/hi",
        "/userdata/bios/mame2003-plus/hi",
        "/userdata/bios/mame2003/hi",
        "/userdata/bios/fbneo/hi",
        "/userdata/bios/fbneo/hiscore",
        "/userdata/bios/fba/hi",
        "/userdata/bios/hiscore",
        "/userdata/bios/hi",
        "/home/pi/RetroPie/BIOS/fbneo/hi",
        "/home/pi/RetroPie/BIOS/fba/hi",
        "/home/pi/RetroPie/BIOS/mame2003-plus/hi",
        "/home/pi/RetroPie/BIOS/mame2003/hi",
        "/home/pi/RetroPie/roms/arcade/hi",
        "/home/pi/RetroPie/roms/arcade/hiscore",
        "/home/pi/RetroPie/roms/mame-libretro/hi",
        "/home/pi/RetroPie/roms/mame-libretro/hiscore",
        "/home/pi/RetroPie/roms/fba/hi",
        "/home/pi/RetroPie/roms/neogeo/hi",
    )
    for bd in bios_dirs:
        add_hi_to(secondary_hi, bd)

    if env_dir:
        env_nv_dir = os.path.join(env_dir, "nvram", rom_name)
        if os.path.isdir(env_nv_dir):
            try:
                for fn in sorted(os.listdir(env_nv_dir)):
                    full_nv = os.path.join(env_nv_dir, fn)
                    if os.path.isfile(full_nv):
                        add_nv_to(direct_nv, full_nv)
            except Exception:
                pass

    retropie_nv_dir = f"/home/pi/RetroPie/roms/arcade/nvram/{rom_name}"
    if os.path.isdir(retropie_nv_dir):
        try:
            for fn in sorted(os.listdir(retropie_nv_dir)):
                full_nv = os.path.join(retropie_nv_dir, fn)
                if os.path.isfile(full_nv):
                    add_nv_to(secondary_nv, full_nv)
        except Exception:
            pass

    cfg_core = _detect_configured_core(system=system or rom_sys, rom_name=rom_name)
    return (
        _rank_hi_candidates(direct_hi, cfg_core=cfg_core)
        + _rank_hi_candidates(primary_hi, cfg_core=cfg_core)
        + _rank_hi_candidates(secondary_hi)
        + _rank_nv_candidates(direct_nv, cfg_core=cfg_core)
        + _rank_nv_candidates(primary_nv, cfg_core=cfg_core)
        + _rank_nv_candidates(secondary_nv)
    )


def extract_hiscores(rom_name, rom_path=None, system=None):
    """
    Searches candidate paths for <rom>.hi / <rom>.hi.json / NVRAM.
    Tries hi2txt CLI first if available, then falls back to embedded hi2txt catalog +
    per-ROM spec + heuristic / M68000 NVRAM decoders.
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

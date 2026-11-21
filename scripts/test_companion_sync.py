#!/usr/bin/env python3
"""
Hermetic unit & integration test suite for `arcadematrix_companion_sync.py`.
Zero hardware, zero network, Python 3 standard library only.
"""

import base64
import hashlib
import importlib.util
import json
import os
import random
import sys
import tempfile
import time
import unittest
import zlib

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.abspath(os.path.join(SCRIPT_DIR, ".."))

CANDIDATE_COMPANION_PATHS = [
    os.path.join(REPO_ROOT, "tools", "recalbox_daemon", "arcadematrix_companion_sync.py"),
    os.path.join(REPO_ROOT, "tools", "arcadematrix_companion_sync.py"),
]
EXISTING_COMPANION_PATHS = [p for p in CANDIDATE_COMPANION_PATHS if os.path.isfile(p)]
if not EXISTING_COMPANION_PATHS:
    raise RuntimeError(f"arcadematrix_companion_sync.py not found under {REPO_ROOT}")

SPEC = importlib.util.spec_from_file_location("arcadematrix_companion_sync", EXISTING_COMPANION_PATHS[0])
cs = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(cs)


def _pack_bcd_be(val, num_bytes=4):
    s = f"{int(val):0{num_bytes * 2}d}"[-num_bytes * 2 :]
    return bytes(int(s[i : i + 2], 16) for i in range(0, len(s), 2))


def _pack_bcd_le(val, num_bytes=3):
    return _pack_bcd_be(val, num_bytes=num_bytes)[::-1]


class TestCompanionSyncUniversalDecoder(unittest.TestCase):
    def test_01_catalog_integrity_determinism_and_cold_decompression_speed(self):
        # Verify SHA-256 parity across all copies present in this repository
        hashes = {}
        for p in EXISTING_COMPANION_PATHS:
            with open(p, "rb") as f:
                hashes[p] = hashlib.sha256(f.read()).hexdigest()
        self.assertEqual(
            len(set(hashes.values())),
            1,
            f"SHA-256 drift detected between companion copies: {hashes}",
        )

        # Verify template placeholder safety inside Base85 constant
        b85_joined = "".join(cs._HI2TXT_CATALOG_B85)
        for token in ("{{DMD_HOST}}", "{{BROKER}}", "{{TOPIC}}"):
            self.assertNotIn(token, b85_joined)

        # Measure cold decompression latency (best of 3 cold runs to avoid VM scheduling jitter)
        best_ms = 9999.0
        cat = None
        for _ in range(3):
            cs._HI2TXT_CACHE = None
            t0 = time.perf_counter()
            cat = cs._get_hi2txt_catalog()
            elapsed_ms = (time.perf_counter() - t0) * 1000.0
            if elapsed_ms < best_ms:
                best_ms = elapsed_ms

        self.assertIsInstance(cat, dict)
        self.assertIn("r", cat)
        self.assertIn("t", cat)
        self.assertIn("c", cat)
        self.assertGreaterEqual(len(cat["r"]), 2500)
        self.assertLess(best_ms, 15.0, f"Cold catalog decompression took {best_ms:.2f} ms (>= 15 ms)")

        # Canonical round-trip determinism check
        raw_json = json.dumps(cat, sort_keys=True, separators=(",", ":")).encode("utf-8")
        reencoded = base64.b85encode(zlib.compress(raw_json, level=9)).decode("ascii")
        self.assertEqual(reencoded, b85_joined)

        # If offline compiler tool exists in this repo (ArcadeMatrix), run its verifier too
        compiler_path = os.path.join(REPO_ROOT, "tools", "hiscore_catalog", "build_hi2txt_catalog.py")
        if os.path.isfile(compiler_path):
            comp_spec = importlib.util.spec_from_file_location("build_hi2txt_catalog", compiler_path)
            comp_mod = importlib.util.module_from_spec(comp_spec)
            comp_spec.loader.exec_module(comp_mod)
            comp_mod.verify_embedded_catalog()

    def test_02_combined_bcd_and_partial_charset_preservation(self):
        # sf2.hi (52 bytes): 6 x (4B BCD BE score + 3B charset='sf2' name + 1B unused) + 4B top score
        # sf2 charset only declares 0x20=' ' and 0x2E='.'; normal ASCII letters must fall back cleanly!
        rows = [
            (245800, b"R.K"),
            (198400, b"KEN"),
            (154000, b"RYU"),
            (112500, b"GUI"),
            (89000, b"CHU"),
            (64000, b"ZAN"),
        ]
        buf = bytearray()
        for sc, tg in rows:
            buf.extend(_pack_bcd_be(sc, 4))
            buf.extend(tg)
            buf.append(0x20)
        buf.extend(_pack_bcd_be(245800, 4))
        self.assertEqual(len(buf), 52)

        res = cs.parse_hi_binary(bytes(buf), rom_name="sf2")
        self.assertEqual(len(res), 5)
        self.assertEqual(res[0]["raw"], 245800)
        self.assertEqual(res[0]["tag"], "R.K")
        self.assertEqual(res[0]["score"], "245,800")
        self.assertEqual(res[1]["tag"], "KEN")
        self.assertEqual(res[4]["raw"], 89000)

    def test_03_separate_two_loops_galaga_byte_trim_and_ghouls_offset72(self):
        # 1. galaga.hi (51 bytes):
        #    - 5 x 6B unpacked BCD-LE scores with trailing 0x24 space trim
        #    - 5 x 3B names (ascii-offset=55, charset: 0x24=' ', 0x2A='.')
        #    - 6B top score
        # Score 185000 in 6B BCD-LE with leading 0x24 trim (trailing in LE): [0, 0, 0, 5, 8, 1] -> 185000,
        # Score 42000 with 1 leading trim byte 0x24: [0, 0, 0, 2, 4, 0x24] -> reversed [0x24, 4, 2, 0, 0, 0] -> 42000
        galaga = bytearray()
        galaga_scores = [
            bytes([0x00, 0x00, 0x00, 0x05, 0x08, 0x01]),  # 185,000
            bytes([0x00, 0x00, 0x00, 0x02, 0x04, 0x24]),  # 42,000 (with 0x24 byte-trim!)
            bytes([0x00, 0x00, 0x00, 0x00, 0x03, 0x24]),  # 30,000
            bytes([0x00, 0x00, 0x05, 0x02, 0x02, 0x24]),  # 22,500
            bytes([0x00, 0x00, 0x00, 0x00, 0x02, 0x24]),  # 20,000
        ]
        for s_bytes in galaga_scores:
            galaga.extend(s_bytes)
        # Tags with ascii-offset=55 ('A'=10, 'R'=27, 'E'=14, 'D'=13, '.'=0x2A)
        galaga_tags = [
            bytes([27, 14, 13]),      # "RED"
            bytes([23, 0x2A, 23]),    # "N.N"
            bytes([10, 0x2A, 10]),    # "A.A"
            bytes([22, 0x2A, 22]),    # "M.M"
            bytes([12, 0x2A, 12]),    # "C.C"
        ]
        for t_bytes in galaga_tags:
            galaga.extend(t_bytes)
        galaga.extend(galaga_scores[0])
        self.assertEqual(len(galaga), 51)

        res_gal = cs.parse_hi_binary(bytes(galaga), rom_name="galaga")
        self.assertEqual(len(res_gal), 5)
        self.assertEqual(res_gal[0]["raw"], 185000)
        self.assertEqual(res_gal[0]["tag"], "RED")
        self.assertEqual(res_gal[1]["raw"], 42000)
        self.assertEqual(res_gal[1]["tag"], "N.N")

        # 2. ghouls.hi (124 bytes):
        #    - 6 x 8B NAME at offset 0 (lines-max=5)
        #    - 6 x 4B NAME ADDRESS at offset 48
        #    - 6 x 4B SCORE (BCD BE) at offset 72
        #    - 6 x 4B SCORE ADDRESS at offset 96
        #    - 4B TOP SCORE at offset 120
        ghouls = bytearray(124)
        names = [b"RED     ", b"H.K     ", b"M.S     ", b"S.U     ", b"ARC     ", b"UNUSED  "]
        scores = [91500, 64200, 45000, 31800, 18400, 999999]  # 6th slot ignored via lines-max=5
        for idx in range(6):
            ghouls[idx * 8 : (idx + 1) * 8] = names[idx]
            ghouls[72 + idx * 4 : 72 + (idx + 1) * 4] = _pack_bcd_be(scores[idx], 4)
        ghouls[120:124] = _pack_bcd_be(91500, 4)

        res_gh = cs.parse_hi_binary(bytes(ghouls), rom_name="ghouls")
        self.assertEqual(len(res_gh), 5)
        self.assertEqual(res_gh[0]["raw"], 91500)
        self.assertEqual(res_gh[0]["tag"], "RED")
        self.assertEqual(res_gh[1]["raw"], 64200)
        self.assertEqual(res_gh[1]["tag"], "H.K")

    def test_04_score_only_pacman_11b_and_galaxian_3b(self):
        # pacman.hi (11 bytes, sameas puckman): 3B BCD LE score at offset 0
        pacman_hi = bytes.fromhex("8022000000080202404048")
        res_pac = cs.parse_hi_binary(pacman_hi, rom_name="pacman")
        self.assertEqual(len(res_pac), 1)
        self.assertEqual(res_pac[0]["raw"], 2280)
        self.assertEqual(res_pac[0]["tag"], "1UP")
        self.assertEqual(res_pac[0]["score"], "2,280")

        # galaxian.hi (3 bytes): 3B truncated BCD LE score
        galaxian_hi = bytes.fromhex("202400")
        res_gx = cs.parse_hi_binary(galaxian_hi, rom_name="galaxian")
        self.assertEqual(len(res_gx), 1)
        self.assertEqual(res_gx[0]["raw"], 2420)
        self.assertEqual(res_gx[0]["tag"], "1UP")

    def test_05_non_bcd_binary_integer_and_scale_1000(self):
        # 1. Real catalog ROM `barrier` (10B): 2B binary BE int * 1000 + 2B binary BE int + 6B odd-skipped name (ascii-offset=65)
        #    Score = 124 * 1000 + 500 = 124,500; Name = 'R'(17), 'E'(4), 'D'(3) on odd indices 1,3,5
        barrier_hi = (
            (124).to_bytes(2, "big")
            + (500).to_bytes(2, "big")
            + bytes([0, 17, 0, 4, 0, 3])
        )
        res_bar = cs.parse_hi_binary(barrier_hi, rom_name="barrier")
        self.assertEqual(len(res_bar), 1)
        self.assertEqual(res_bar[0]["raw"], 124500)
        self.assertEqual(res_bar[0]["tag"], "RED")

        # 2. Direct ikari-style binary integer template with scale=1000 (2B uint16 BE * 1000 + 3B ASCII tag)
        ikari_tpl = {
            "g": [
                {
                    "m": "c",
                    "o": 0,
                    "n": 3,
                    "d": 5,
                    "s": {"z": 2, "e": "i", "x": 1000},
                    "t": {"z": 3, "p": 2},
                }
            ]
        }
        ikari_buf = (
            (542).to_bytes(2, "big") + b"RAL"
            + (310).to_bytes(2, "big") + b"CLK"
            + (125).to_bytes(2, "big") + b"SNK"
        )
        res_ik = cs._decode_hi2txt_template(ikari_buf, ikari_tpl, [], exact_mode=True)
        self.assertEqual(len(res_ik), 3)
        self.assertEqual(res_ik[0]["raw"], 542000)
        self.assertEqual(res_ik[0]["tag"], "RAL")

    def test_06_inter_core_size_tolerance_and_corrupt_buffer_rejection(self):
        # Build valid 52B sf2.hi, then append 1 extra check byte (53B, FBNeo / mame2003-plus style)
        sf2_52 = bytearray()
        for sc, tg in [(310000, b"RED"), (250000, b"KEN"), (180000, b"RYU"), (120000, b"GUI"), (90000, b"CHU"), (50000, b"ZAN")]:
            sf2_52.extend(_pack_bcd_be(sc, 4))
            sf2_52.extend(tg)
            sf2_52.append(0x20)
        sf2_52.extend(_pack_bcd_be(310000, 4))

        # +1 trailing check byte (53B)
        res_trailing = cs.parse_hi_binary(bytes(sf2_52) + b"\x5a", rom_name="sf2")
        self.assertEqual(len(res_trailing), 5)
        self.assertEqual(res_trailing[0]["raw"], 310000)
        self.assertEqual(res_trailing[0]["tag"], "RED")

        # +1 leading header byte (53B)
        res_leading = cs.parse_hi_binary(b"\x01" + bytes(sf2_52), rom_name="sf2")
        self.assertEqual(len(res_leading), 5)
        self.assertEqual(res_leading[0]["raw"], 310000)
        self.assertEqual(res_leading[0]["tag"], "RED")

        # All-zero unplayed .hi file must NEVER produce static/fake placeholder scores
        self.assertEqual(cs.parse_hi_binary(b"\x00" * 52, rom_name="sf2"), [])
        self.assertEqual(cs.parse_hi_binary(b"\x00" * 51, rom_name="galaga"), [])

        # Deterministic corrupt / random buffers (both BCD and non-BCD ROMs, exact and +1B) must be rejected
        rng = random.Random(0xA4CADE)
        corrupt_53 = bytes(rng.getrandbits(8) | 0x80 for _ in range(53))
        corrupt_100 = bytes(rng.getrandbits(8) | 0x80 for _ in range(100))
        self.assertEqual(cs.parse_hi_binary(corrupt_53, rom_name="sf2"), [])
        self.assertEqual(cs.parse_hi_binary(corrupt_100, rom_name="1945kiii"), [])

    def test_07_neogeo_m68000_byteswapped_nvram_non_regression(self):
        # Construct an 8192-byte Neo-Geo saveram / .nv image (> 1 KiB) with M68000 16-bit byte-swapped records
        logical_nvram = bytearray(b"\xff" * 8192)
        table_offset = 1024
        neo_entries = [
            (1450800, b"RED"),
            (982400, b"SNK"),
            (745000, b"MAR"),
            (512300, b"TAR"),
            (310000, b"FIO"),
        ]
        for idx, (sc, tg) in enumerate(neo_entries):
            pos = table_offset + idx * 8
            logical_nvram[pos : pos + 4] = _pack_bcd_be(sc, 4)
            logical_nvram[pos + 4 : pos + 7] = tg
            logical_nvram[pos + 7] = 0x20

        m68k_on_disk = cs._swap16_bytes(bytes(logical_nvram))
        for rom in ("mslugx", "kof98"):
            parsed = cs.parse_hi_binary(m68k_on_disk, rom_name=rom)
            self.assertEqual(len(parsed), 5, f"Failed M68000 NVRAM extraction for {rom}")
            self.assertEqual(parsed[0]["raw"], 1450800)
            self.assertEqual(parsed[0]["tag"], "RED")
            self.assertEqual(parsed[1]["raw"], 982400)
            self.assertEqual(parsed[1]["tag"], "SNK")

    def test_08_mame_lua_hiscore_discovery_system_isolation_and_mtime_priority(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            saves_root = os.path.join(tmpdir, "saves")
            mame0278_dir = os.path.join(saves_root, "mame", "mame0278", "hiscore")
            mame2003_dir = os.path.join(saves_root, "mame", "mame2003-plus", "hi")
            fbneo_dir = os.path.join(saves_root, "fbneo", "fbneo")
            os.makedirs(mame0278_dir, exist_ok=True)
            os.makedirs(mame2003_dir, exist_ok=True)
            os.makedirs(fbneo_dir, exist_ok=True)

            p_mame0278 = os.path.join(mame0278_dir, "galaxian.hi")
            p_mame2003 = os.path.join(mame2003_dir, "galaxian.hi")
            p_fbneo = os.path.join(fbneo_dir, "galaxian.hi")

            # Write scores: fbneo=9990, mame2003=3100, mame0278=7840
            with open(p_fbneo, "wb") as f:
                f.write(_pack_bcd_le(9990, 3))
            with open(p_mame2003, "wb") as f:
                f.write(_pack_bcd_le(3100, 3))
            with open(p_mame0278, "wb") as f:
                f.write(_pack_bcd_le(7840, 3))

            now = time.time()
            # Even if fbneo has the newest timestamp overall, system="mame" must isolate MAME primary group!
            os.utime(p_fbneo, (now + 100, now + 100))
            os.utime(p_mame2003, (now - 60, now - 60))
            os.utime(p_mame0278, (now, now))

            old_env = os.environ.get("AM_SAVE_ROOTS")
            old_conf = os.environ.get("AM_SYSTEM_CONF_FILE")
            old_hi_dir = os.environ.get("AM_HISCORE_DIR")
            try:
                os.environ["AM_SAVE_ROOTS"] = saves_root
                os.environ.pop("AM_HISCORE_DIR", None)
                os.environ["AM_SYSTEM_CONF_FILE"] = os.path.join(tmpdir, "nonexistent.conf")

                # 1. mame0278/hiscore wins over mame2003-plus/hi (newer mtime) AND is not masked by fbneo!
                got1 = cs.extract_hiscores("galaxian", system="mame")
                self.assertEqual(len(got1), 1)
                self.assertEqual(got1[0]["raw"], 7840)

                # 2. Touch mame2003-plus/hi with a newer mtime -> mame2003-plus/hi now wins!
                os.utime(p_mame2003, (now + 30, now + 30))
                got2 = cs.extract_hiscores("galaxian", system="mame")
                self.assertEqual(got2[0]["raw"], 3100)

                # 3. Configure mame.core=mame0278 in recalbox.conf -> configured core takes priority!
                conf_file = os.path.join(tmpdir, "recalbox.conf")
                with open(conf_file, "w", encoding="utf-8") as cf:
                    cf.write("mame.core=mame0278\n")
                os.environ["AM_SYSTEM_CONF_FILE"] = conf_file
                got3 = cs.extract_hiscores("galaxian", system="mame")
                self.assertEqual(got3[0]["raw"], 7840)

                # 4. If primary MAME files are absent, secondary fallback cleanly finds fbneo!
                os.remove(p_mame0278)
                os.remove(p_mame2003)
                got4 = cs.extract_hiscores("galaxian", system="mame")
                self.assertEqual(got4[0]["raw"], 9990)
            finally:
                if old_env is None:
                    os.environ.pop("AM_SAVE_ROOTS", None)
                else:
                    os.environ["AM_SAVE_ROOTS"] = old_env
                if old_conf is None:
                    os.environ.pop("AM_SYSTEM_CONF_FILE", None)
                else:
                    os.environ["AM_SYSTEM_CONF_FILE"] = old_conf
                if old_hi_dir is not None:
                    os.environ["AM_HISCORE_DIR"] = old_hi_dir

    def test_09_end_to_end_start_stop_celebration_workflow(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            saves_root = os.path.join(tmpdir, "saves")
            hiscore_dir = os.path.join(saves_root, "mame", "mame0278", "hiscore")
            os.makedirs(hiscore_dir, exist_ok=True)
            sf2_path = os.path.join(hiscore_dir, "sf2.hi")

            def write_sf2_top(top_score, top_tag):
                buf = bytearray()
                rows = [
                    (top_score, top_tag),
                    (50000, b"RYU"),
                    (40000, b"KEN"),
                    (30000, b"GUI"),
                    (25000, b"CHU"),
                    (20000, b"ZAN"),
                ]
                for sc, tg in rows:
                    buf.extend(_pack_bcd_be(sc, 4))
                    buf.extend(tg)
                    buf.append(0x20)
                buf.extend(_pack_bcd_be(top_score, 4))
                with open(sf2_path, "wb") as f:
                    f.write(buf)

            write_sf2_top(65000, b"CPU")

            capture_path = os.path.join(tmpdir, "mqtt_events.jsonl")
            baseline_path = os.path.join(tmpdir, "baseline.json")
            pid_path = os.path.join(tmpdir, "watcher.pid")

            env_keys = {
                "AM_SAVE_ROOTS": saves_root,
                "AM_SYSTEM_CONF_FILE": os.path.join(tmpdir, "empty.conf"),
                "AM_MQTT_CAPTURE_FILE": capture_path,
                "AM_MQTT_CAPTURE_ONLY": "1",
                "AM_SKIP_HTTP_RESOLVE": "1",
                "AM_BASELINE_FILE": baseline_path,
                "AM_WATCHER_PID_FILE": pid_path,
                "AM_RA_CONF_FILE": os.path.join(tmpdir, "empty_ra.conf"),
                "AM_RA_CACHE_FILE": os.path.join(tmpdir, "empty_ra_cache.json"),
                "AM_RCHEEVOS_LOG_FILE": os.path.join(tmpdir, "empty_retroarch.log"),
            }
            saved_env = {k: os.environ.get(k) for k in env_keys}
            try:
                os.environ.update(env_keys)

                # 1. Game start -> captures baseline (65,000) and publishes sync_meta
                cs.handle_start(
                    dmd_host="127.0.0.1",
                    topic="system/playing/recalbox",
                    system="mame",
                    game="sf2",
                    watch=False,
                    delay_sec=0.0,
                )
                self.assertTrue(os.path.isfile(baseline_path))

                # 2. Player beats Rank #1 before exiting emulator -> 184,200 ("RED")
                write_sf2_top(184200, b"RED")

                celebrate = cs.handle_stop(
                    dmd_host="127.0.0.1",
                    topic="system/playing/recalbox",
                    system="mame",
                    game="sf2",
                    delay_sec=0.0,
                )
                self.assertIsNotNone(celebrate)
                self.assertEqual(celebrate["status"], "celebrate")
                self.assertEqual(celebrate["kind"], "highscore")
                self.assertEqual(celebrate["rank"], 1)
                self.assertEqual(celebrate["tag"], "RED")
                self.assertEqual(celebrate["score"], "184,200")

                with open(capture_path, "r", encoding="utf-8") as cf:
                    events = [json.loads(line) for line in cf if line.strip()]
                self.assertEqual(len(events), 2)
                self.assertEqual(events[0]["status"], "sync_meta")
                self.assertEqual(events[0]["hiscores"][0]["score"], "65,000")
                self.assertEqual(events[1]["status"], "celebrate")
                self.assertEqual(events[1]["kind"], "highscore")
                self.assertEqual(events[1]["rank"], 1)
            finally:
                for k, v in saved_env.items():
                    if v is None:
                        os.environ.pop(k, None)
                    else:
                        os.environ[k] = v

    def test_10_classic_rom_formats_fallback_offsets(self):
        # Verify aligned _CLASSIC_ROM_FORMATS directly via _decode_with_spec (bypassing hi2txt catalog):
        # 1. ffight: offset=20, stride=8, 4B BCD BE + 3B tag + 1B pad
        ffight_buf = bytearray(64)
        for idx, (sc, tg) in enumerate([(210500, b"HAG"), (165000, b"COD"), (120000, b"GUY")]):
            pos = 20 + idx * 8
            ffight_buf[pos : pos + 4] = _pack_bcd_be(sc, 4)
            ffight_buf[pos + 4 : pos + 7] = tg
            ffight_buf[pos + 7] = 0x00
        res_ff = cs._decode_with_spec(bytes(ffight_buf), cs._CLASSIC_ROM_FORMATS["ffight"])
        self.assertEqual(len(res_ff), 3)
        self.assertEqual(res_ff[0]["raw"], 210500)
        self.assertEqual(res_ff[0]["tag"], "HAG")

        # 2. gng: offset=20, stride=7, 4B BCD BE + 3B tag
        gng_buf = bytearray(94)
        for idx, (sc, tg) in enumerate([(88400, b"ART"), (54200, b"RED")]):
            pos = 20 + idx * 7
            gng_buf[pos : pos + 4] = _pack_bcd_be(sc, 4)
            gng_buf[pos + 4 : pos + 7] = tg
        res_gng = cs._decode_with_spec(bytes(gng_buf), cs._CLASSIC_ROM_FORMATS["gng"])
        self.assertEqual(len(res_gng), 2)
        self.assertEqual(res_gng[0]["raw"], 88400)
        self.assertEqual(res_gng[0]["tag"], "ART")


if __name__ == "__main__":
    unittest.main(verbosity=2)

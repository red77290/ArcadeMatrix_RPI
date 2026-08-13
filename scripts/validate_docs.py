#!/usr/bin/env python3
"""
validate_docs.py
Validates documentation files & reference conf.ini in ArcadeMatrix_RPi to prevent doc drift.
"""
import os
import sys
import re
import configparser

ROOT_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))

REQUIRED_DOC_FILES = [
    "README.md", "README_FR.md", "README_ES.md",
    "CONTRIBUTING.md", "CONTRIBUTING_FR.md", "CONTRIBUTING_ES.md"
]

# Add any docs/ subfolder files if present
DOCS_DIR = os.path.join(ROOT_DIR, "docs")
if os.path.exists(DOCS_DIR):
    for root, _, files in os.walk(DOCS_DIR):
        for file in files:
            if file.endswith(".md"):
                rel_path = os.path.relpath(os.path.join(root, file), ROOT_DIR)
                REQUIRED_DOC_FILES.append(rel_path)

OBSOLETE_PATTERNS = [
    (re.compile(r'\bROTATION=.*sprites.*\b', re.IGNORECASE), "References obsolete 'sprites' in ROTATION string"),
]

EXPECTED_CONFIG_SCHEMA = {
    "WIFI": ["SSID", "PASS", "CONFIGURED", "DISABLE_INTERNAL_WIFI"],
    "MATRIX": [
        "ROWS", "COLS", "HARDWARE_MAPPING", "DRIVER_CHIP", "BRIGHTNESS",
        "CHAIN", "PARALLEL", "SLOWDOWN", "RGB_SEQUENCE", "PWM_BITS",
        "PWM_LSB_NANOSECONDS", "LIMIT_REFRESH_RATE_HZ", "DISABLE_HARDWARE_PULSING"
    ],
    "MQTT": ["ENABLED", "BROKER", "PORT", "USER", "PASS", "DEVICE_NAME", "TOPIC_BATOCERA", "TOPIC_RECALBOX"],
    "TIME": ["FORMAT_24H", "CLOCK_FONT", "CLOCK_SIZE", "CLOCK_THEME", "CLOCK_OFFSET_X", "CLOCK_OFFSET_Y", "CLOCK_COLOR_1", "CLOCK_COLOR_2"],
    "IDLE": ["ROTATION", "CLOCK_DURATION_SEC", "DATE_DURATION_SEC", "WEATHER_DURATION_SEC", "GIFS_COUNT", "FIGHTER_ENABLED", "FIGHTER_INTERVAL"],
    "DATE": ["THEME", "BACKGROUND_SPRITE", "FORMAT", "DATE_FONT", "DATE_SIZE", "DATE_OFFSET_X", "DATE_OFFSET_Y", "DATE_COLOR_1", "DATE_COLOR_2"],
    "WEATHER": ["API_KEY", "CITY", "WEATHER_OFFSET_X", "WEATHER_OFFSET_Y"],
    "STANDBY": ["NIGHT_MODE_ENABLED", "TURN_OFF_AT", "WAKE_UP_AT"],
    "CRYPTO": ["SYMBOLS", "CACHE_TTL_MIN"],
    "STOCK": ["SYMBOLS", "CACHE_TTL_MIN"],
}

def validate_sd_conf_ini(rel_path="data/conf.ini"):
    full_path = os.path.join(ROOT_DIR, rel_path)
    if not os.path.exists(full_path):
        print(f"❌ Missing reference configuration file: {rel_path}")
        return False

    config = configparser.ConfigParser(inline_comment_prefixes=('#', ';'))
    try:
        config.read(full_path, encoding='utf-8')
    except Exception as e:
        print(f"❌ Error parsing {rel_path}: {e}")
        return False

    errors = 0
    for section, expected_keys in EXPECTED_CONFIG_SCHEMA.items():
        if not config.has_section(section):
            print(f"❌ {rel_path}: Missing expected section [{section}]")
            errors += 1
            continue

        section_keys = [k.upper() for k in config.options(section)]
        for key in expected_keys:
            if key not in section_keys:
                print(f"❌ {rel_path}: Section [{section}] missing required key '{key}'")
                errors += 1

    # Check STANDBY section contract (WAKE_UP_AT present, TURN_OFF_AT present once)
    with open(full_path, "r", encoding="utf-8") as f:
        raw_text = f.read()

    standby_match = re.search(r'\[STANDBY\](.*?)(\n\[|\Z)', raw_text, re.DOTALL | re.IGNORECASE)
    if standby_match:
        standby_text = standby_match.group(1)
        turn_off_count = len(re.findall(r'^\s*TURN_OFF_AT\s*=', standby_text, re.MULTILINE | re.IGNORECASE))
        if turn_off_count > 1:
            print(f"❌ {rel_path}: Duplicate 'TURN_OFF_AT' key found in [STANDBY]")
            errors += 1
        if not re.search(r'^\s*WAKE_UP_AT\s*=', standby_text, re.MULTILINE | re.IGNORECASE):
            print(f"❌ {rel_path}: Missing 'WAKE_UP_AT' key in [STANDBY]")
            errors += 1

    if errors == 0:
        print(f"  ✓ {rel_path} structure, sections, and keys valid.")
        return True
    return False

def check_doc_file(rel_path):
    full_path = os.path.join(ROOT_DIR, rel_path)
    if not os.path.exists(full_path):
        print(f"❌ Missing required documentation file: {rel_path}")
        return False

    with open(full_path, "r", encoding="utf-8") as f:
        content = f.read()

    errors = 0
    for pattern, description in OBSOLETE_PATTERNS:
        if pattern.search(content):
            print(f"❌ {rel_path}: {description}")
            errors += 1

    if errors == 0:
        print(f"  ✓ {rel_path}")
        return True
    return False

def main():
    print("🔍 Validating ArcadeMatrix_RPi Documentation files & SD conf.ini...")
    all_ok = True
    for doc in REQUIRED_DOC_FILES:
        if not check_doc_file(doc):
            all_ok = False

    if not validate_sd_conf_ini():
        all_ok = False

    if all_ok:
        print("🎉 Documentation & SD Config validation PASSED.")
        sys.exit(0)
    else:
        print("❌ Documentation & SD Config validation FAILED.")
        sys.exit(1)

if __name__ == "__main__":
    main()

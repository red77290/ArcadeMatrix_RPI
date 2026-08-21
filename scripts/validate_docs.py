#!/usr/bin/env python3
import os
import sys
import re
import json

ROOT_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))

REQUIRED_DOC_FILES = [
    "README.md", "README_FR.md", "README_ES.md",
    "CONTRIBUTING.md", "CONTRIBUTING_FR.md", "CONTRIBUTING_ES.md"
]

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
    "matrix": ["width", "height", "chain_length", "pwm_bits"],
    "system": ["timezone", "format_24h", "night_mode_enabled"],
    "wifi": ["ssid", "password"],
}

def validate_sd_conf_json(rel_path="config.json"):
    full_path = os.path.join(ROOT_DIR, rel_path)
    if not os.path.exists(full_path):
        print(f"❌ Missing reference configuration file: {rel_path}")
        return False

    try:
        with open(full_path, "r", encoding='utf-8') as f:
            data = json.load(f)
    except Exception as e:
        print(f"❌ Error parsing {rel_path}: {e}")
        return False

    errors = 0
    for section, expected_keys in EXPECTED_CONFIG_SCHEMA.items():
        if section not in data:
            print(f"❌ {rel_path}: Missing expected section '{section}'")
            errors += 1
            continue

        for key in expected_keys:
            if key not in data[section]:
                print(f"❌ {rel_path}: Section '{section}' missing required key '{key}'")
                errors += 1

    if "instances" not in data or not isinstance(data["instances"], list):
        print(f"❌ {rel_path}: Missing or invalid 'instances' array.")
        errors += 1

    if "rotation" not in data or not isinstance(data["rotation"], list):
        print(f"❌ {rel_path}: Missing or invalid 'rotation' array.")
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
    print("🔍 Validating ArcadeMatrix_RPi Documentation files & SD config.json...")
    all_ok = True
    for doc in REQUIRED_DOC_FILES:
        if not check_doc_file(doc):
            all_ok = False

    if not validate_sd_conf_json():
        all_ok = False

    if all_ok:
        print("🎉 Documentation & SD Config validation PASSED.")
        sys.exit(0)
    else:
        print("❌ Documentation & SD Config validation FAILED.")
        sys.exit(1)

if __name__ == "__main__":
    main()

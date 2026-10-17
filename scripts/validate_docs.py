#!/usr/bin/env python3
"""
validate_docs.py
Validates trilingual documentation parity (EN/FR/ES), checks for obsolete
patterns or drift in Markdown files, and verifies config.json structure.
"""
import os
import sys
import re
import json

ROOT_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))

ROOT_TRILINGUAL_BASES = [
    "README",
    "CONTRIBUTING",
]

OBSOLETE_PATTERNS = [
    (re.compile(r'\bROTATION=.*sprites\b', re.IGNORECASE), "References obsolete 'sprites' in ROTATION string"),
    (re.compile(r'(?<![a-zA-Z0-9_/])install\.sh\b'), "References obsolete 'install.sh' instead of 'autoInstall.sh'"),
    (re.compile(r'\\n\\n'), "Contains literal escaped '\\n\\n' sequence"),
    (re.compile(r'\bDecibelEngine\b|\bnoise_level\b'), "References non-existent DecibelEngine / noise_level on RPi"),
]

HISTORICAL_KEYWORDS = re.compile(
    r'(legacy|historical|migrati|formerly|ancien|historique|auparavant|antiguo|históric|anteriormente|heredado|v1\.)',
    re.IGNORECASE,
)

EXPECTED_CONFIG_SCHEMA = {
    "matrix": ["width", "height", "chain_length", "pwm_bits", "rotation", "transition_effect"],
    "system": ["timezone", "format_24h", "lang", "temp_unit", "night_mode_enabled", "idle_fighter_enabled"],
    "wifi": ["ssid", "password", "hostname"],
    "mqtt": ["enabled", "broker", "port"],
    "data_mqtt": ["broker", "port"],
}


def collect_doc_files():
    doc_files = []
    parity_errors = 0

    for base in ROOT_TRILINGUAL_BASES:
        for suffix in (".md", "_FR.md", "_ES.md"):
            rel = f"{base}{suffix}"
            doc_files.append(rel)
            if not os.path.exists(os.path.join(ROOT_DIR, rel)):
                print(f"❌ Missing root trilingual file: {rel}")
                parity_errors += 1

    docs_dir = os.path.join(ROOT_DIR, "docs")
    if os.path.exists(docs_dir):
        for root, _, files in os.walk(docs_dir):
            md_files = sorted(f for f in files if f.endswith(".md"))
            for file in md_files:
                rel_path = os.path.relpath(os.path.join(root, file), ROOT_DIR)
                doc_files.append(rel_path)

                if not file.endswith("_FR.md") and not file.endswith("_ES.md"):
                    stem = file[:-3]
                    for lang in ("FR", "ES"):
                        counterpart = f"{stem}_{lang}.md"
                        if counterpart not in md_files:
                            rel_missing = os.path.relpath(os.path.join(root, counterpart), ROOT_DIR)
                            print(f"❌ Missing trilingual counterpart for {rel_path}: {rel_missing}")
                            parity_errors += 1

    tools_dir = os.path.join(ROOT_DIR, "tools")
    if os.path.exists(tools_dir):
        for rel in (
            "tools/README.md", "tools/README_FR.md", "tools/README_ES.md",
            "tools/mugen_extractor/README.md", "tools/mugen_extractor/README_FR.md", "tools/mugen_extractor/README_ES.md",
            "tools/mugen_extractor/ARCHITECTURE.md", "tools/mugen_extractor/ARCHITECTURE_FR.md", "tools/mugen_extractor/ARCHITECTURE_ES.md",
        ):
            if os.path.exists(os.path.join(ROOT_DIR, rel)):
                doc_files.append(rel)
            else:
                print(f"❌ Missing tools documentation file: {rel}")
                parity_errors += 1

    return sorted(set(doc_files)), parity_errors


def validate_sd_conf_json(rel_path="config.json"):
    full_path = os.path.join(ROOT_DIR, rel_path)
    if not os.path.exists(full_path):
        print(f"❌ Missing reference configuration file: {rel_path}")
        return False

    try:
        with open(full_path, "r", encoding="utf-8") as f:
            data = json.load(f)
    except Exception as e:
        print(f"❌ Error parsing {rel_path}: {e}")
        return False

    errors = 0
    for section, expected_keys in EXPECTED_CONFIG_SCHEMA.items():
        if section not in data or not isinstance(data[section], dict):
            print(f"❌ {rel_path}: Missing or invalid section '{section}'")
            errors += 1
            continue

        for key in expected_keys:
            if key not in data[section]:
                print(f"❌ {rel_path}: Section '{section}' missing required key '{key}'")
                errors += 1

    if "instances" not in data or not isinstance(data["instances"], (list, dict)):
        print(f"❌ {rel_path}: Missing or invalid 'instances' field.")
        errors += 1

    if "rotation" not in data or not isinstance(data["rotation"], (list, dict)):
        print(f"❌ {rel_path}: Missing or invalid 'rotation' field.")
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

    for lineno, line in enumerate(content.splitlines(), start=1):
        if "conf.ini" in line and not HISTORICAL_KEYWORDS.search(line):
            print(f"❌ {rel_path}:{lineno}: References obsolete 'conf.ini' outside a historical/legacy note")
            errors += 1

    if errors == 0:
        print(f"  ✓ {rel_path}")
        return True
    return False


def main():
    print("🔍 Validating ArcadeMatrix_RPi Documentation files & SD config.json...")
    doc_files, parity_errors = collect_doc_files()
    all_ok = (parity_errors == 0)

    for doc in doc_files:
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

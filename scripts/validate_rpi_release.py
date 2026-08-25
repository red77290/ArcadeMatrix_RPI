#!/usr/bin/env python3
"""
validate_rpi_release.py
Validates the ArcadeMatrix_RPi release artifacts and installation scripts.
"""
import os
import sys

ROOT_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))

REQUIRED_FILES = [
    "Cargo.toml",
    "autoInstall.sh",
    "config.json",
    "README.md", "README_FR.md", "README_ES.md"
]

def main():
    print("🔍 Validating ArcadeMatrix_RPi Release Artifacts...")
    all_ok = True

    for rel_path in REQUIRED_FILES:
        full_path = os.path.join(ROOT_DIR, rel_path)
        if not os.path.exists(full_path):
            print(f"❌ Missing required release file: {rel_path}")
            all_ok = False
        else:
            print(f"  ✓ Found {rel_path}")

    # Check autoInstall.sh permissions or content
    autoinstall_path = os.path.join(ROOT_DIR, "autoInstall.sh")
    if os.path.exists(autoinstall_path):
        with open(autoinstall_path, "r", encoding="utf-8") as f:
            content = f.read()
            if "cargo build" not in content and "systemctl" not in content:
                print("⚠️ Warning: autoInstall.sh might be missing expected build or service commands.")

    if all_ok:
        print("🎉 ArcadeMatrix_RPi Release Validation PASSED.")
        sys.exit(0)
    else:
        print("❌ ArcadeMatrix_RPi Release Validation FAILED.")
        sys.exit(1)

if __name__ == "__main__":
    main()

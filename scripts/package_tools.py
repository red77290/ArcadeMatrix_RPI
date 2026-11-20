#!/usr/bin/env python3
"""
scripts/package_tools.py
Automates validation and packaging of the ArcadeMatrix_RPI PC & Console Companion
Tools bundle (ArcadeMatrix-tools.zip) attached to GitHub Releases.

Packages end-user tools from tools/ (Recalbox/Batocera/RetroPie sync installers &
daemon, universal high-score companion sync, MUGEN sprite extractor, and Home
Assistant blueprints) while excluding Python caches and temporary files.
"""

import argparse
import os
import sys
import time
import zipfile

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TOOLS_DIR = os.path.join(ROOT_DIR, "tools")
DEFAULT_ZIP_PATH = os.path.join(ROOT_DIR, "ArcadeMatrix-tools.zip")
ARCHIVE_PREFIX = "ArcadeMatrix-tools"

INCLUDED_DIRS = [
    "mugen_extractor",
    "home_assistant",
]

INCLUDED_FILES = [
    "README.md",
    "README_FR.md",
    "README_ES.md",
    "install.sh",
    "install.ps1",
    "rpi_emulationstation_base_os_setup.sh",
    "recalbox_setup_mqtt.sh",
    "arcadematrix_daemon.py",
    "arcadematrix_companion_sync.py",
    "arcadematrix_launcher(permanent).sh",
    "arcadematrix_mqtt_batocera.sh",
    "arcadematrix_wakeup[start,systembrowsing,rungame,endgame,rundemo,enddemo,wakeup].sh",
]

FORBIDDEN_SUBSTRINGS = (
    "__pycache__",
)

EXCLUDE_NAMES = {
    ".DS_Store",
    "__pycache__",
    ".git",
    ".gitignore",
    "Thumbs.db",
}

EXCLUDE_SUFFIXES = (
    ".pyc",
    ".pyo",
    ".bak",
    ".swp",
)

EXECUTABLE_SUFFIXES = (
    ".sh",
    ".py",
)

REQUIRED_SOURCE_FILES = [
    *INCLUDED_FILES,
    "mugen_extractor/mugen_extractor.py",
    "mugen_extractor/README.md",
    "mugen_extractor/README_FR.md",
    "mugen_extractor/README_ES.md",
]


def _should_exclude(name: str) -> bool:
    if name in EXCLUDE_NAMES or name.startswith(".git"):
        return True
    return any(name.endswith(sfx) for sfx in EXCLUDE_SUFFIXES)


def validate_tools_sources() -> bool:
    print("🔍 Validating RPi companion tools sources...")
    errors = 0

    if not os.path.isdir(TOOLS_DIR):
        print(f"❌ Missing tools directory: {TOOLS_DIR}")
        return False

    for d in INCLUDED_DIRS:
        dp = os.path.join(TOOLS_DIR, d)
        if not os.path.isdir(dp):
            print(f"❌ Missing required tools subdirectory: {dp}")
            errors += 1

    for req_rel in REQUIRED_SOURCE_FILES:
        fp = os.path.join(TOOLS_DIR, req_rel)
        if not os.path.isfile(fp):
            print(f"❌ Missing required companion tool file: {fp}")
            errors += 1
        elif os.path.getsize(fp) == 0:
            print(f"❌ Companion tool file is empty (0 bytes): {fp}")
            errors += 1

    ha_dir = os.path.join(TOOLS_DIR, "home_assistant")
    yaml_found = False
    if os.path.isdir(ha_dir):
        for _r, _dirs, files in os.walk(ha_dir):
            if any(f.endswith((".yaml", ".yml")) for f in files):
                yaml_found = True
                break
    if not yaml_found:
        print(f"❌ No Home Assistant YAML blueprints found in: {ha_dir}")
        errors += 1

    if errors == 0:
        print("  ✓ All RPi companion tools source files are present and valid.")
        return True
    return False


def _add_file_to_zip(zf: zipfile.ZipFile, src_path: str, rel_inside_prefix: str) -> int:
    arcname = f"{ARCHIVE_PREFIX}/{rel_inside_prefix.replace(os.sep, '/')}"
    with open(src_path, "rb") as f:
        data = f.read()

    is_exec = src_path.endswith(EXECUTABLE_SUFFIXES)
    if is_exec:
        data = data.replace(b"\r\n", b"\n")

    dt = time.localtime(os.path.getmtime(src_path))[:6]
    zinfo = zipfile.ZipInfo(filename=arcname, date_time=dt)
    zinfo.compress_type = zipfile.ZIP_DEFLATED
    zinfo.create_system = 3
    mode = 0o100755 if is_exec else 0o100644
    zinfo.external_attr = mode << 16
    zf.writestr(zinfo, data)
    return len(data)


def package_tools_zip(output_zip_path: str = DEFAULT_ZIP_PATH) -> bool:
    print(f"📦 Packaging RPi companion tools bundle into: {output_zip_path}...")
    file_count = 0
    total_bytes = 0

    os.makedirs(os.path.dirname(os.path.abspath(output_zip_path)), exist_ok=True)

    with zipfile.ZipFile(output_zip_path, "w", zipfile.ZIP_DEFLATED) as zf:
        for rel_file in sorted(INCLUDED_FILES):
            src_path = os.path.join(TOOLS_DIR, rel_file)
            total_bytes += _add_file_to_zip(zf, src_path, rel_file)
            file_count += 1

        for subdir in sorted(INCLUDED_DIRS):
            subdir_path = os.path.join(TOOLS_DIR, subdir)
            for root, dirs, files in os.walk(subdir_path):
                dirs[:] = sorted(d for d in dirs if not _should_exclude(d))
                for fname in sorted(files):
                    if _should_exclude(fname):
                        continue
                    full_path = os.path.join(root, fname)
                    rel_path = os.path.relpath(full_path, TOOLS_DIR)
                    total_bytes += _add_file_to_zip(zf, full_path, rel_path)
                    file_count += 1

    zip_size_kb = os.path.getsize(output_zip_path) / 1024.0
    print(
        f"  ✓ Packaged {file_count} files "
        f"({total_bytes / 1024.0:.1f} KB uncompressed, {zip_size_kb:.1f} KB zip)."
    )

    print("🔍 Verifying RPi companion tools archive integrity...")
    with zipfile.ZipFile(output_zip_path, "r") as zf:
        namelist = set(zf.namelist())
        expected_entries = [f"{ARCHIVE_PREFIX}/{rel}" for rel in REQUIRED_SOURCE_FILES]
        for exp in expected_entries:
            if exp not in namelist:
                print(f"❌ Zip missing required entry: {exp}")
                return False

        for entry in namelist:
            if any(bad in entry for bad in FORBIDDEN_SUBSTRINGS) or entry.endswith(EXCLUDE_SUFFIXES):
                print(f"❌ Zip contains forbidden cache/temp entry: {entry}")
                return False

        bad_file = zf.testzip()
        if bad_file:
            print(f"❌ Corrupted file in zip: {bad_file}")
            return False

    print("🎉 RPi companion tools packaging & verification PASSED.")
    return True


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Package and validate ArcadeMatrix_RPI PC & Console Companion Tools bundle."
    )
    parser.add_argument("--output", default=DEFAULT_ZIP_PATH, help="Output zip file path")
    parser.add_argument(
        "--verify-only",
        action="store_true",
        help="Only validate source companion tools without creating the zip file",
    )
    args = parser.parse_args()

    if not validate_tools_sources():
        print("💥 RPi companion tools source validation failed.")
        sys.exit(1)

    if args.verify_only:
        print("🎉 Verification complete.")
        sys.exit(0)

    if not package_tools_zip(args.output):
        print("💥 RPi companion tools packaging failed.")
        sys.exit(1)


if __name__ == "__main__":
    main()

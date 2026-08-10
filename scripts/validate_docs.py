#!/usr/bin/env python3
"""
validate_docs.py
Validates documentation files in ArcadeMatrix_RPi to prevent doc drift.
"""
import os
import sys
import re

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
    print("🔍 Validating ArcadeMatrix_RPi Documentation files...")
    all_ok = True
    for doc in REQUIRED_DOC_FILES:
        if not check_doc_file(doc):
            all_ok = False

    if all_ok:
        print("🎉 Documentation validation PASSED.")
        sys.exit(0)
    else:
        print("❌ Documentation validation FAILED.")
        sys.exit(1)

if __name__ == "__main__":
    main()

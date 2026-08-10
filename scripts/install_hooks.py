#!/usr/bin/env python3
"""
install_hooks.py
Installs ArcadeMatrix_RPi git hooks into local git repository.
"""
import os
import sys
import subprocess
import shutil

ROOT_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
GIT_DIR = os.path.join(ROOT_DIR, ".git")
GITHOOKS_DIR = os.path.join(ROOT_DIR, ".githooks")
PRE_COMMIT_SRC = os.path.join(GITHOOKS_DIR, "pre-commit")

def main():
    if not os.path.exists(GIT_DIR):
        print("❌ Error: .git directory not found. Make sure you are in a Git repository.")
        sys.exit(1)

    # 1. Try configuring core.hooksPath .githooks via git config
    try:
        subprocess.run(["git", "config", "core.hooksPath", ".githooks"], cwd=ROOT_DIR, check=True)
        print("✅ Configured Git to use '.githooks' folder (git config core.hooksPath .githooks).")
    except Exception as e:
        print(f"⚠️ Could not set core.hooksPath via git config: {e}")

    # 2. Also copy pre-commit directly into .git/hooks/ as fallback
    target_hook = os.path.join(GIT_DIR, "hooks", "pre-commit")
    shutil.copyfile(PRE_COMMIT_SRC, target_hook)
    os.chmod(target_hook, 0o755)
    print(f"✅ Installed pre-commit hook into {target_hook}")

    print("\n🎉 Git hooks installation complete! Pre-commit checks will now run on every commit:")
    print("  - RPi Release Artifact Validation (scripts/validate_rpi_release.py)")
    print("  - Documentation Drift & Config Key Guarding (scripts/validate_docs.py)")
    print("  - Rust Unit Tests (cargo test)")

if __name__ == "__main__":
    main()

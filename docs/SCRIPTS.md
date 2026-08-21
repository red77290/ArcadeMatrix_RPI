# ArcadeMatrix Scripts Documentation

This document explains the purpose, prerequisites and usage of every script in
the `scripts/` directory. They help you **build**, **deploy**, **image**,
**diagnose** and **validate** ArcadeMatrix_RPi across platforms.

## Prerequisites

### 1. Docker
All cross-compilation and image building is handled via Docker, so you never
have to install an ARM toolchain locally.
- **macOS / Windows**: [Docker Desktop](https://www.docker.com/products/docker-desktop/) or Rancher Desktop.
- **Linux**: Docker Engine (`sudo apt install docker.io`).

### 2. SSH access to the Raspberry Pi (deploy only)
- The Pi must be reachable on your network with SSH enabled
  (`sudo raspi-config` > Interface Options > SSH).
- Note its IP address.

### 3. sshpass (Linux / macOS deploy only)
`deploy.sh` uses `sshpass` to automate password entry.
- **macOS**: `brew install hudochenkov/sshpass/sshpass`
- **Linux**: `sudo apt install sshpass`
- **Windows**: not needed — `deploy.ps1` uses native OpenSSH and prompts
  interactively (or uses key auth).

### 4. Shared defaults
`defaults.sh` centralises the credentials/paths used by the build, deploy and
image scripts (`AM_USER`, `AM_PASS`, ...). It is **sourced** by the other
scripts — edit it once instead of editing each script.

---

## 🛠 Build

### `build.sh [aarch64|armv7|all]`
Smart cross-compiler. Builds the Rust binary for 64-bit ARM (`aarch64`,
Pi 3/4/5 and Zero 2 W), 32-bit ARM (`armv7`, Pi 2/Zero) or both, inside Docker.
Defaults to `all`.
**Usage**: `bash scripts/build.sh aarch64`

The resulting binary lands in `target/<triple>/release/arcadematrix`.

---

## 🚀 Deploy

Deploy scripts auto-detect the Pi architecture, build via Docker, upload the
binary and restart the `arcadematrix` service.

### `deploy.sh` (macOS / Linux)
Flags (all optional; defaults come from `defaults.sh`):
- `--ip <IP>` — target Pi IP
- `--user <USER>` — SSH user (default `pi`)
- `--pass <PASS>` — SSH password (uses `sshpass`)
- `--skip-build` — reuse the already-compiled binary
- `--binary-path <PATH>` — deploy a specific prebuilt binary

**Usage**: `bash scripts/deploy.sh --ip 192.168.1.149 --user pi`

### `deploy.ps1` (Windows)
PowerShell equivalent using native OpenSSH. Same behaviour.
**Usage**: `./scripts/deploy.ps1 -PI_IP 192.168.1.149 -PI_USER pi [-SkipBuild] [-BinaryPath <path>]`

> 💡 For a running device you usually don't need to deploy at all — use the
> **OTA firmware upload** in the Web UI (System tab), which validates and swaps
> the binary over Wi-Fi. See `docs/QUICKSTART.md`.

---

## 💿 SD Image Building

### `build_image.sh`
Builds a ready-to-flash Raspberry Pi OS `.img` with ArcadeMatrix preinstalled,
via Docker. Produces a ~14 GB image sized to fit a 16 GB SD card.
**Usage**: `bash scripts/build_image.sh`

Internal helpers (not run directly):
- **`docker_builder.sh`** — runs inside the Docker container to assemble the image.
- **`chroot_setup.sh`** — runs inside the image chroot to install the OS,
  service and dependencies via `autoInstall.sh`.

---

## 🩺 Runtime / On-Pi

### `recovery.sh <PROJ_DIR>`
Runs on the Pi **before** the main service starts. Looks on the boot partition
for a recovery firmware; if present it installs it and backs it up to avoid
boot loops. Invoked automatically by the service, not by hand.

### `wifi_diag.sh`
On-Pi Wi-Fi / DMA-contention diagnostic logger. Because HW pulsing can drop
Wi-Fi/SSH, it runs detached (`setsid`), samples periodically and writes to a log
on disk you collect afterwards.
**Usage (on the Pi, as root)**: `sudo bash scripts/wifi_diag.sh`

### `run_ab_test.sh`
Automated A/B benchmark (memory-bus load vs hardware pulsing). Runs fully
detached (~6 min), survives SSH drops, and restores pulsing OFF at the end so
Wi-Fi returns. Writes `diag_*.log` files. Used for the benchmarks in
`docs/benchmarks/`.

---

## ✅ Tooling & Validation

These back the git pre-commit hook and are also runnable manually.

### `install_hooks.py`
Installs the ArcadeMatrix git hooks into `.git/hooks` (pre-commit runs the
validators + `cargo fmt --check` + tests).
**Usage**: `python3 scripts/install_hooks.py`

### `validate_docs.py`
Checks documentation drift and `config.json` key validity across the EN/FR/ES
doc set. Fails the commit if a required doc is missing or the config is invalid.

### `validate_rpi_release.py`
Validates that the release artifacts and installation scripts
(`Cargo.toml`, `autoInstall.sh`, `config.json`, the READMEs...) are present and
well-formed before a release.

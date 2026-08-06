# ArcadeMatrix Scripts Documentation

This document explains the purpose, prerequisites, and usage of all the scripts included in the `scripts/` directory. These scripts are provided to help you build, test, and deploy ArcadeMatrix across different platforms.

## Prerequisites

Before running the build or deploy scripts, ensure you have the following installed on your host machine:

### 1. Docker
All cross-compilation is handled via Docker. This ensures that you don't have to manually install complex ARM64 toolchains on your local machine.
- **macOS / Windows**: Install [Docker Desktop](https://www.docker.com/products/docker-desktop/).
- **Linux**: Install Docker Engine via your package manager (`sudo apt install docker.io`).

### 2. SSH Access to the Raspberry Pi
To use the deploy scripts, your Raspberry Pi must be accessible via SSH.
- Ensure the Raspberry Pi is connected to the same network as your host machine.
- Enable the SSH service on the Raspberry Pi (using `sudo raspi-config` > Interface Options > SSH).
- Note down the IP address of the Raspberry Pi.

### 3. SSHPass (Linux / macOS only)
The bash deployment scripts (`deploy_pi.sh`) use `sshpass` to automate password entry.
- **macOS**: `brew install hudochenkov/sshpass/sshpass` (or `brew install eugene`)
- **Linux**: `sudo apt install sshpass`
- **Windows**: Not required. The PowerShell `.ps1` scripts use native OpenSSH and will prompt you interactively for the password.

---

## Configuration

If your Raspberry Pi is not using the default IP (`192.168.1.169`) or default credentials, you must edit the deploy scripts before running them.

Open `scripts/deploy_pi.sh` (or `scripts/deploy_pi.ps1` for Windows) and modify the top variables:
```bash
PI_IP="192.168.1.169"  # Change to your Raspberry Pi's actual IP
PI_USER="pi"           # Default is 'pi' (or 'root' on Batocera)
PI_PASS="raspberry"    # Default is 'raspberry' (or 'linux' on Batocera)
```

---

## 🛠 Compilation Scripts

These scripts compile the Rust source code into a binary executable.

### `build_local_64.sh` (Linux / macOS)
Compiles the ArcadeMatrix binary for 64-bit ARM architectures (`aarch64-unknown-linux-gnu`). This is the target architecture for modern Raspberry Pi OS (64-bit) running on Pi 3, 4, or 5.
**Usage**: `bash scripts/build_local_64.sh`

### `build_local_64.ps1` (Windows)
The Windows PowerShell equivalent of `build_local_64.sh`. It performs the exact same Docker cross-compilation.
**Usage**: Right-click and select "Run with PowerShell", or run `.\scripts\build_local_64.ps1` in a terminal.

### `build_local.sh`
A legacy or generalized build script for multiple targets (often 32-bit `armv7`). Use this if you are running an older 32-bit Raspberry Pi OS.

---

## 🚀 Deployment Scripts

These scripts compile the code AND push the resulting binary directly to your live Raspberry Pi, restarting the systemd service automatically.

### `deploy_pi.sh` (Linux / macOS)
The primary deployment script for developers. It:
1. Calls `build_local_64.sh`.
2. Uses `sshpass` to stop the `arcadematrix.service` on the Pi.
3. Uses `scp` to upload the new binary to `/home/pi/arcadematrix_temp`.
4. Moves the binary to `/usr/local/bin/`, sets permissions, and restarts the service.
**Usage**: `bash scripts/deploy_pi.sh`

### `deploy_pi.ps1` (Windows)
The Windows PowerShell equivalent of the deployment script. It uses native Windows 10/11 OpenSSH (`ssh.exe` and `scp.exe`). Note that because Windows does not have `sshpass`, you will be prompted to type the Raspberry Pi password during the SSH phases (unless you have set up SSH public key authentication).
**Usage**: Right-click and select "Run with PowerShell", or run `.\scripts\deploy_pi.ps1` in a terminal.

### `deploy_to_pi.sh <IP_ADDRESS>`
A slightly different variant of the deploy script that takes the IP address as a command-line argument rather than hardcoding it in the script.
**Usage**: `bash scripts/deploy_to_pi.sh pi@192.168.1.200`

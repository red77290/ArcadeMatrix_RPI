# tools/

🇬🇧 English | 🇫🇷 [Français](README_FR.md) | 🇪🇸 [Español](README_ES.md)

PC/manual-side utilities for ArcadeMatrix_RPi. These are not part of the Flask app/API - you run
them yourself, separately, either on your own computer or by pasting them onto your Recalbox.

## `mugen_extractor/`

Converts MUGEN fighting-game character files (`.sff`/`.air`) into the `.fgt` binary sprite format
used by both this project's `engines/fighter.py` **and** the ESP32 sibling project's
`FighterEngine` (C++) - it's the same tool, kept identical in both repos since there's no shared
package mechanism between the two independent codebases. See `mugen_extractor/README.md` for full
usage.

## `rpi_emulationstation_base_os_setup.sh` (and `recalbox_setup_mqtt.sh`)

A standalone shell script you copy onto your retro gaming console (Recalbox, Batocera, or RetroPie) and run **directly on the device over SSH**
(not from your PC) to install the "now playing" MQTT daemon:

1. `ssh <user>@<console-ip>` (e.g., `root@<ip>` with password `recalboxroot` for Recalbox, `root`/`linux` for Batocera, `pi`/`raspberry` for RetroPie).
2. Run the script:
   ```bash
   sh /tmp/rpi_emulationstation_base_os_setup.sh
   ```
3. Enter your ArcadeMatrix device's IP (ESP32 or Raspberry Pi) when prompted.
4. Select your target system:
   - `1: Auto-Detect`
   - `2: Recalbox`
   - `3: Batocera`
   - `4: RetroPie`
5. The script automatically installs the daemon, hooks into the OS lifecycle (EmulationStation scripts, services, or runcommand hooks), and reboots when prompted.

What it installs:
- A small Python daemon (`arcadematrix_daemon.py`) that monitors game launch/stop events and publishes `{"status": "playing"|"browsing"|"stopped", "game": "<rom basename>", "system": "<SystemId>"}` over MQTT on `system/playing/<os>` (`system/playing/recalbox`, `system/playing/batocera`, or `system/playing/retropie`) via `mosquitto_pub`.
- Startup integration keeping the daemon active across reboots.

**Note for users who also have an ESP32 ArcadeMatrix device**:
This daemon and topic standard (`system/playing/#`) are 100% identical and compatible across both ESP32 and Raspberry Pi versions. A single installation on your console serves both devices simultaneously. You can also run the PC-side automated installer (`tools/install.sh` / `tools/install.ps1`).

**Normally this happens automatically via the web UI's SSH install feature** (`core/ssh_installer.py`,
triggered from the Settings page) - this script is the manual fallback for when you'd rather not
give the app your Recalbox's SSH credentials, or need to inspect/customize exactly what gets
installed before running it.

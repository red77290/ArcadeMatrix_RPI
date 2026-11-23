🇬🇧 English | 🇫🇷 [Français](QUICKSTART_FR.md) | 🇪🇸 [Español](QUICKSTART_ES.md)

# Quickstart Guide

This guide will help you install and configure ArcadeMatrix on your Raspberry Pi.

## 1. Installation (Recommended)

We provide a ready-to-use pre-compiled image (`ArcadeMatrix_Release_aarch64.img.xz` for Pi 3/4/5/Zero 2 W or `armhf.img.xz` for Pi 1/2/Zero).

> **Required MicroSD Card: true `16 GB` minimum (`>= 15.1 GB`) — `32 GB` strongly recommended.** The full image is `15.03 GB` (`bootfs` + Linux OS + ~`6 GB` exFAT `DATA` partition for your GIFs, MUGEN sprites, and fonts). Never use an `8 GB` card or an undersized `"16 GB"` card (`< 15.03 GB`), or verification will fail and the `DATA` partition will be truncated.

1. **Flash the image to your MicroSD card:**
   - **Direct flash without decompressing (recommended):** use **[Raspberry Pi Imager](https://www.raspberrypi.com/software/)** *(Operating System → **Use custom**, select the `.img.xz` file directly, and make sure to click **"No"** when asked to apply OS customisation settings)* or **[Balena Etcher](https://etcher.balena.io/)**.
   - **If your flashing tool requires decompressing to `.img` first** *(e.g. Rufus or Win32DiskImager — Pi Imager / Etcher remain preferred because Win32DiskImager does not refresh Windows partition tables after writing)*: extract the `.img.xz` **only** with **[7-Zip](https://www.7-zip.org/)** (Windows), **[Keka](https://www.keka.io/)** (macOS), or **`xz -d`** (Linux) with at least `16 GB` of free disk space *(avoid Windows 11's built-in extractor and WinRAR, which can truncate `.xz` archives over 4 GB)*.
2. **Open the `DATA` USB drive:** physically unplug and reinsert the SD card into your PC/Mac *(click **Cancel** if Windows prompts you to format the Linux partition)*. A USB drive named **DATA** will appear.
   - 💡 *On Windows, if only `bootfs` appears (`DATA` missing in File Explorer)*: right-click the **Start button → Disk Management** (`diskmgmt.msc`), right-click the **`DATA (exFAT)`** block → **Change Drive Letter and Paths... → Add... → Pick a letter (e.g. `E:`) → OK** *(use a USB MicroSD adapter if an older built-in laptop SD slot grays out that option)*.
3. Open the `config.json` file located on this **DATA** drive to insert your Wi-Fi credentials (`"ssid"` and `"password"`) and your matrix size, and drop your media into `gifs/`, `fighters_32/`, `fighters_64/`, or `fonts/`.
4. Insert the SD card into the Raspberry Pi and turn it on. The IP address will be displayed on the matrix!

## 2. Web Configuration & OTA Updates

Once the Pi powers on, open a web browser on your phone or PC and navigate to:
`http://<RASPBERRY_IP>:8080`

Here you can configure:
- Clock & Date themes, colors, fonts, and sizes.
- Active features in the idle rotation loop.
- Matrix brightness and Night Mode schedule.
- 🔄 **Firmware Update (OTA)**: Navigate to the **System** tab, drag and drop the compiled binary `arcadematrix_vX.Y.Z_aarch64`, and click **Upload & Update Firmware** to update the daemon directly over Wi-Fi without ever re-flashing your SD Card!

## 3. Adding Content (GIFs, Sprites, Fonts)

To add your own media, **simply plug your SD Card into your PC/Mac**.
The **DATA** partition will appear as a standard USB drive (exFAT format):

- **GIFs**: Place them inside the `gifs/` folder.
- **MUGEN Sprites**: Use our extractor tool to generate `.fgt` files and place them in `fighters_32/` or `fighters_64/`.
- **Fonts**: Place `.ttf` or `.bdf` font files inside the `fonts/` folder.

## 4. Hardware Connection

We recommend using an Adafruit RGB Matrix HAT or Bonnet connected to a Raspberry Pi Zero 2 W or Pi 4. Ensure the HUB75 connector is properly plugged into your LED panel.

## 5. Optional: Connect Recalbox / Batocera / RetroPie & In-Game Showcase

1. **1-Click Console Setup**: Open **System → MQTT & API → 🎮 FrontendSync**, enter your Recalbox, Batocera, or RetroPie IP and SSH credentials, and click **Install Sync Script** (your console stores only your DMD's IP and discovers the active MQTT broker automatically — you never have to reinstall the script later).
2. **Instant Marquees**: Set **Enable MQTT** to **Enabled**. Scrolling menus on your console displays Pixelcade marquees immediately (`0 ms`).
3. **In-Game Info Cards & Gameplay Videos** (with **[dmd_media_showcase](https://github.com/red77290/dmd_media_showcase)** on your LAN):
   - In **🎬 DMD Media Showcase**, toggle **Enable DMD Media Showcase** ON (`🟢 Communication OK`).
   - In **🎮 FrontendSync**, enable **Share Showcase MQTT Broker** (frees local RAM) and **In-Game Showcase (Card + Gameplay Video)**, pick your **Playback Cycle** (`Alternate Marquee & Showcase`, `Play Once`, or `Loop`) and **Card Style** (`Inherit from Showcase Studio` or `Custom Override on DMD`), and click **Save MQTT Settings**.

## 6. Appendix: PC & Console Companion Tools (`ArcadeMatrix-tools.zip`)

All optional PC, Mac, Linux, and retro-console helper scripts are available in a standalone archive — **no need to download or clone the source code**:

👉 **[⬇️ Download `ArcadeMatrix-tools.zip` (Latest Release)](https://github.com/red77290/ArcadeMatrix_RPI/releases/latest/download/ArcadeMatrix-tools.zip)** *(also available on the [Raspberry Pi Portal](https://red77290.github.io/ArcadeMatrix_RPI/)).*

Unzip `ArcadeMatrix-tools.zip` on your computer to access:
- **Console Sync & Universal High-Score Daemon (`ArcadeMatrix-tools/install.sh`, `ArcadeMatrix-tools/install.ps1`, `ArcadeMatrix-tools/rpi_emulationstation_base_os_setup.sh`)**: Manual alternative to the Web UI 1-click installer for Recalbox, Batocera, and RetroPie — streams menu marquees in `0 ms` and celebrates new personal High-Scores across ~3,100 MAME/FBNeo `.hi` ROMs + Neo-Geo/CPS NVRAM.
- **MUGEN Sprite Extractor (`ArcadeMatrix-tools/mugen_extractor/`)**: Run `start_extractor.bat` (Windows) or `./start_extractor.sh` (macOS / Linux) to convert MUGEN character folders (`chars/`) into `.fgt` sprites + `index.txt` ready to copy into `DATA:/fighters_32/` or `DATA:/fighters_64/`.
- **Home Assistant Blueprints (`ArcadeMatrix-tools/home_assistant/blueprints/`)**: Ready-to-import YAML blueprints for streaming Home Assistant sensors, 24h charts, tables, and weather to the `mqttdata` engine.


🇬🇧 English | 🇫🇷 [Français](README_FR.md) | 🇪🇸 [Español](README_ES.md)

# ArcadeMatrix RPi 🍓👾

📺 **Video Demo / Présentation :** https://youtu.be/vAK880Io8yo?si=KSieTymlE7fLyZQs

The original **Rust** implementation of the **ArcadeMatrix** project, specifically designed to run on a **Raspberry Pi** connected to an RGB LED Matrix (HUB75) via the Adafruit HAT or Joy-IT hardware.

Developed in parallel with the ESP32 C++ version, this implementation leverages the power of the Raspberry Pi to deliver a high-performance multi-threaded graphics pipeline.

📚 **Developer docs:** [Getting Started (dev workspace)](docs/GETTING_STARTED.md) · [Developer Guide](docs/DEVELOPER.md) · [Architecture](docs/ARCHITECTURE.md) · [Quickstart (end users)](docs/QUICKSTART.md) · [PC/manual tools](tools/README.md)

---

## 💾 Quickstart: Pre-compiled Image (Recommended)

We provide pre-compiled, fully automated `.img` files, built and published automatically on every release.

| Architecture | Recommended For | Compatible With | Download |
|--------------|-----------------|-----------------|----------|
| **64-bit (aarch64)** | Raspberry Pi 3, 4, 5, Zero 2 W | Pi 3, 4, 5, Zero 2 W | [⬇️ Download 64-bit Image](https://github.com/red77290/ArcadeMatrix_RPI/releases/latest/download/ArcadeMatrix_Release_aarch64.img.xz) |
| **32-bit (armhf)** | Raspberry Pi 1, 2, Zero (Original) | All Raspberry Pi models | [⬇️ Download 32-bit Image](https://github.com/red77290/ArcadeMatrix_RPI/releases/latest/download/ArcadeMatrix_Release_armhf.img.xz) |

*(Both are `.img.xz` files - decompress with 7-Zip/Keka/`xz -d` before flashing. See the [full release list](https://github.com/red77290/ArcadeMatrix_RPI/releases) for older versions.)*

1. Flash the `.img` to your SD card using **Raspberry Pi Imager**.
2. Once flashed, insert the SD card into your PC/Mac. You will see a large **DATA** USB drive appear!
3. Open the `conf.ini` file located on this DATA drive to configure your Matrix size and your **Wi-Fi** credentials (`SSID` and `PASS`).
4. Plug the SD card into your Raspberry Pi and power it on.
5. The Matrix will immediately turn on and **display the IP address** for 5 seconds. Use this IP to access the Web UI!

---

## 🌟 Features (RPi Exclusives vs ESP32)

* 🔄 **Over-The-Air (OTA) Firmware Update**: Update the single standalone Rust binary directly from the Web UI without re-flashing your SD Card image!
* 🚀 **Rust Native Performance**: High-performance multi-threaded engine using Actix-web and image processing in pure compiled Rust with 0% idle CPU overhead.
* **Dynamically Loadable Fonts (`.ttf`)**: No more hardcoded font files! Drop any `.ttf` or `.otf` font directly into the `fonts/` folder, and the Web UI will automatically list it for use on the Clock or Date.
* **Unlimited Clock/Date Sizes & Offsets**: You are no longer restricted to Size 1, 2, or 3. You can set the size to any number, and position the text freely on massive matrix panels (e.g. 256x64).
* **Massive Clock Selection**: Enjoy a variety of animated clocks including the classic Arcade, Binary, Cyberpunk, Flip, Word, and the brand new **Pac-Man**, **Tetris**, **SlotMachine**, and **Versus (Mugen)** clocks!
* 📈 **Real-Time Crypto & Stock Tickers**: Live price quotes & 24h % badges from CoinGecko, Binance, and Yahoo Finance with configurable TTL cache.
* **True Matrix Digital Rain (Katakana)**: A completely custom, buttery smooth, genuine Matrix digital rain effect (`DotGothic16`) with falling half-width Katakana and "unlit LED" negative space text punching through the rain.
* **Custom Smooth Gradients**: In addition to classic Publisher themes (Nintendo, Capcom, Sega...), you can now choose a **Custom Color / Gradient** theme and pick two colors to generate a dynamic gradient.
* **Dynamic Image Playlists (GIF/PNG/JPG)**: Read actual `.gif` and `.png` files dynamically straight from the filesystem without SD card fragmentation issues.

---

## 🚀 Hardware Requirements & Compatibility

Thanks to the ultra-lightweight native Rust implementation (~5 MB binary, ~10 MB RAM, 0% idle CPU overhead), **ArcadeMatrix now fully supports older legacy Raspberry Pi hardware without stutter or dropped frames**:

1. **Raspberry Pi**: 
   - **Legacy / Single-Core**: Pi 1 (B, B+, A+), Pi Zero, Pi Zero W *(Fully supported with zero lag thanks to Rust!)*
   - **Multi-Core**: Pi 2, Pi 3, Pi 4, Pi Zero 2 W *(Recommended)*
   - *(⚠️ **Pi 5 Warning**: The hzeller rgb-led-matrix library does NOT support the Pi 5 natively via GPIO due to the new RP1 chip. You must use an active adapter board for Pi 5!).*
2. **RGB LED Matrix**: HUB75 panels (e.g., 64x64, 128x32, 256x64).
3. **Adafruit RGB Matrix HAT** (or Joy-IT, or custom wiring).
4. **MicroSD Card** (16GB or larger recommended for the Pre-compiled Image).

---

## 🛠️ Advanced Installation\n\n### Option 2: Manual Installation
If you prefer to install it manually on a fresh **Raspberry Pi OS Lite (64-bit)**:
Once logged into your Raspberry Pi via SSH:

```bash
curl -sSL https://raw.githubusercontent.com/red77290/ArcadeMatrix_RPI/main/install.sh | bash
```
*(If the repository is private, you will need to `git clone` manually first and run `./install.sh` from inside the folder).*

The script will automatically:
1. Install Rust, Actix-web, image-rs, and `build-essential`.
2. Download and compile the `hzeller/rpi-rgb-led-matrix` driver.
3. Setup `systemd` to automatically start ArcadeMatrix on boot.

### Option 3: Smart Deploy via Computer
If you want to compile the application on your own computer (much faster) and automatically deploy it to the Raspberry Pi, you can use the smart Docker-based deployment scripts.

**On macOS / Linux:**
```bash
bash scripts/deploy.sh --ip <PI_IP> --user <PI_USER> --pass <PI_PASS>
# Example: bash scripts/deploy.sh --ip 192.168.1.177 --user pi --pass raspberry
```

**On Windows (PowerShell):**
```powershell
.\scripts\deploy.ps1 -PI_IP "192.168.1.177" -PI_USER "pi" -PI_PASS "raspberry"
```
*The script will auto-detect your Pi's architecture, compile the binary via Docker, stop the remote service, upload the file, and restart the application.*

---

## ⚠️ Hardware Warning: Wi-Fi & Interference (VHS Lines)

Raspberry Pis (especially Pi 3 and Zero W) share their internal Wi-Fi bus clock with the **PWM/PCM** hardware controller used by the LED matrix. 

When driving large matrices (like **256x64**), the DMA controller must push massive amounts of data to the GPIO pins. If hardware pulsing is enabled (`disable_hardware_pulsing = false`), this creates intense DMA bandwidth saturation that **starves the internal SDIO Wi-Fi chip**, causing severe packet loss, lag, and disconnections.

While an unstable Wi-Fi might be acceptable if you only use the Web UI occasionally to change the clock, it completely breaks features that rely on a stable internet connection or local network:
* **Recalbox/Batocera/Pixelcade MQTT Sync**: Will drop messages or disconnect.
* **Crypto & Stock Tickers**: API calls will timeout and fail.
* **Weather**: API calls will timeout.

**Solutions for 256x64 users:**
1. **Use an Ethernet (RJ45) cable** (Pi 3B/4): Bypasses the SDIO Wi-Fi chip entirely.
2. **Use a USB Wi-Fi Dongle**: USB controllers use a different internal bus and are immune to the PWM DMA starvation.
3. **Set `disable_hardware_pulsing = true`**: Forces the matrix to use CPU bit-banging instead of hardware DMA. Your Wi-Fi will work perfectly, but you will see a slight "VHS lines" flickering effect on the matrix.

*(Note: **128x32** matrices require 4x less DMA bandwidth, so they usually run perfectly fine with hardware pulsing enabled and internal Wi-Fi active).*

*(Note on **Raspberry Pi 4**: The Pi 4 uses a much faster PCIe architecture and DMA controller. It is highly likely that it is NOT impacted by this 256x64 Wi-Fi saturation bug. However, this has not been officially tested yet as we do not have a Pi 4 on hand to confirm.)*

### Raspberry Pi Matrix Compatibility Table

| Matrix Size | Network / Pulse Configuration | Image Quality | Wi-Fi / API / MQTT |
|-------------|-----------------------------|----------------|-------------------|
| **128x32**  | Internal Wi-Fi + `disable_hardware_pulsing = false` | ✅ Perfect | ✅ Stable |
| **256x64**  | **Ethernet (RJ45)** + `disable_hardware_pulsing = false` | ✅ Perfect | ✅ Stable |
| **256x64**  | **USB Wi-Fi Dongle** + `disable_hardware_pulsing = false` | ✅ Perfect | ✅ Stable |
| **256x64**  | Internal Wi-Fi + `disable_hardware_pulsing = true` | ⚠️ Slight Flicker | ✅ Stable |
| **256x64**  | Internal Wi-Fi + `disable_hardware_pulsing = false` | ✅ Perfect | ❌ Broken / Timeouts |

---

## 🎨 Media Management

The pre-compiled image features a dedicated **DATA partition** formatted as exFAT. This means you can plug your SD card directly into your Windows or Mac computer to drag-and-drop your files without needing SSH or FTP!

### Sprites & GIFs
* **`/fighters_32/`** or **`/fighters_64/`**: Put your `.fgt` sprites here (See MUGEN Sprites section below).
* **`/gifs/`**: Drop your `.gif` loops in folders inside here.
The Web UI will automatically scan these folders and let you check the ones you want to play!

### Fonts
* **`/fonts/`**: Drop your `.ttf`, `.otf`, or `.bdf` files here. 
By default, the project ships with `PressStart2P.ttf`, `VT323.ttf`, and `DotGothic16.ttf`.

---

## 🕸️ Web UI
Navigate to `http://<YOUR_PI_IP>:8080/` to access the Control Panel.

The interface is exactly the same as the ESP32 version, offering Dashboard controls, Playlist selection, Clock configuration, and MQTT settings, with added controls for **Gradients** and **Unlimited Sizes**.

---

## 🕹️ Recalbox & Batocera Integration (Pixelcade Marquees)

ArcadeMatrix supports dynamic **Pixelcade-style** marquees when you select or play a game on your Recalbox or Batocera!

As you browse your game lists, the Raspberry Pi will download official Pixelcade marquees from GitHub **in the background and in real-time**, cache them on your SD card, and display them on your LED matrix. If a game has no image, it will display an elegant animated text fallback.

### Automatic Installation (Recommended)
Go to the **MQTT** tab in the ArcadeMatrix Web UI, enter the IP of your Recalbox/Batocera along with its root password (default `recalboxroot` or `linux`), and click **Install Sync Script**. This will automatically inject the daemon via SSH.

### Manual Installation (Recalbox)
If you prefer manual installation, or if the network install fails:
1. Open the `tools/recalbox_setup_mqtt.sh` file included in the project.
2. Edit the line `MQTT_BROKER="192.168.1.xxx"` and set the IP of the Raspberry Pi running the LED Matrix.
3. Copy the `tools/recalbox_setup_mqtt.sh` file to your Recalbox (e.g., in `/recalbox/share/`).
4. SSH into your Recalbox and run: `bash /recalbox/share/recalbox_setup_mqtt.sh`.

### How does the Daemon architecture work?
Unlike native Recalbox scripts that run (and freeze the system) on every joystick movement, ArcadeMatrix installs an **ultra-lightweight Rust Daemon in the background**.
* **Zero Lag:** Consumes 0% CPU. EmulationStation suffers no stutter or lag, even when scrolling at max speed.
* **Anti-Spam (Debounce):** If you scroll quickly through 50 games, the daemon won't flood the network. It only sends the message to the matrix if you pause on a game for more than 150 milliseconds.
* **Thread Safety:** On the LED Matrix side, downloads and the drawing engine are separated by threads with robust locks, preventing freezing and image cache corruption.

---

## 🔧 Matrix Configuration
If you have a matrix larger than 64x64 or 128x32, or if you are using a non-Adafruit HAT, you may need to tweak the `hzeller` arguments in `src/core/matrix.rs`. By default, it's set to `--led-gpio-mapping=adafruit-hat` and `128x32`.

You can also change Matrix brightness dynamically via the Web UI Settings.
- Enable Standby/Night modes.

---

## 📂 Managing Media (GIFs and MUGEN Sprites)

### Adding GIFs
Simply drop any standard `.gif` files into the `gifs/` directory:
```text
ArcadeMatrix_RPi/
└── gifs/
    ├── mario_run.gif
    ├── sonic_wait.gif
    └── ...
```

### Adding MUGEN Sprites
To achieve perfect 60fps performance and exact "virtual ground" alignments across massive character rosters, the Fighter engine uses pre-processed `.fgt` files along with an `index.txt` manifest.

**You cannot simply drop raw images into the fighters folders!**
You MUST use the provided `mugen_extractor.py` tool located in the `tools/mugen_extractor/` folder to process your MUGEN characters. 

The extractor will read MUGEN `.sff` and `.air` files, calculate the perfect bounding boxes to prevent animation jittering, and export optimized `.fgt` files directly into your `fighters_32/` and `fighters_64/` folders.

Please refer to `tools/mugen_extractor/README.md` for full instructions on how to add more MUGEN characters!

---

## ⚙️ Advanced Configuration (conf.ini)

If you prefer to edit settings manually instead of using the Web UI, you can directly edit the `conf.ini` file located on the **DATA** partition of your SD Card. 
This is especially useful for setting up Wi-Fi before the first boot.

### 🌐 [WIFI]
| Parameter | Default | Description |
|---|---|---|
| `SSID` | `YourNetworkName` | The name of your Wi-Fi network. |
| `PASS` | `YourNetworkPassword` | The password for your Wi-Fi network. |
| `CONFIGURED` | `false` | Set to `false` to force the Raspberry Pi to attempt a Wi-Fi connection on its next boot. Automatically sets back to `true` on success. |

### 🎛️ [MATRIX]
| Parameter | Default | Description |
|---|---|---|
| `ROWS` / `COLS` | `32` / `64` | The pixel dimensions of a single LED panel. |
| `HARDWARE_MAPPING` | `adafruit-hat` | Type of HAT/wiring used. (`adafruit-hat`, `adafruit-hat-pwm`, `regular-pi1`, `regular`). |
| `CHAIN` / `PARALLEL` | `1` / `1` | `CHAIN` for horizontal daisy-chaining. `PARALLEL` for vertical stacking on multiple HUB75 ports. |
| `SLOWDOWN` | `2` | Hardware slowdown (1 to 4). Increase if your Matrix has flickering or visual artifacts (especially Pi 3/4). |
| `disable_hardware_pulsing`| `false` | **CRITICAL:** Set to `true` to prevent the internal Wi-Fi chip from crashing (at the cost of slight flickering). |
| `BRIGHTNESS` | `100` | Global matrix brightness (1 to 100). |
| `RGB_SEQUENCE` | `RGB` | Color order. Change to `RBG` or `BGR` if your colors look swapped. |

### ⏰ [TIME] & [DATE]
| Parameter | Default | Description |
|---|---|---|
| `FORMAT_24H` | `true` | `true` for 24-hour format, `false` for 12-hour AM/PM format. |
| `CLOCK_FONT` | `DotGothic16.ttf`| Name of the `.ttf` or `.bdf` file in the `/fonts/` folder to use for the clock. |
| `CLOCK_SIZE` | `16` | Font size (scaling factor) for the clock. |
| `THEME` | `0` | The numeric ID of the animated clock theme (e.g. 19 for Flip, 21 for True Matrix). |
| `CLOCK_COLOR_1` | `#ffffff` | Primary hex color. Used for gradients if theme is Custom (20). |
| `CLOCK_COLOR_2` | `#ffffff` | Secondary hex color. Used for gradients if theme is Custom (20). |

*(The `[DATE]` section contains identical parameters for configuring the date display).*

### 🔄 [IDLE]
| Parameter | Default | Description |
|---|---|---|
| `ROTATION` | `all` | Dictates rotation behavior (`clock`, `gifs`, `sprites`, or `all`). |
| `CLOCK_DURATION_SEC`| `10` | How long the clock/date stays on screen during the rotation loop. |
| `GIF_DURATION_SEC` | `10` | How long a single GIF stays on screen before advancing. |
| `SELECTED_GIFS` | *(empty)* | Comma-separated list of media to loop. Leave empty to play everything. |
| `SELECTED_SPRITES` | *(empty)* | Comma-separated list of sprites to loop. Leave empty to play everything. |

### 📈 [CRYPTO] & 📊 [STOCK]
| Parameter | Default | Description |
|---|---|---|
| `SYMBOLS` | `BTC,ETH,SOL,DOGE` / `AAPL,NVDA,TSLA,MSFT` | Comma-separated list of symbols to display. |
| `CACHE_TTL_MIN` | `1` | Refresh rate / Cache TTL in minutes (prevents API rate limiting). |

### 🌙 [STANDBY]
| Parameter | Default | Description |
|---|---|---|
| `NIGHT_MODE_ENABLED`| `false` | If `true`, the Matrix will automatically turn off and wake up. |
| `TURN_OFF_AT` | `23:00` | HH:MM formatted time for screen sleep. |
| `WAKE_UP_AT` | `07:00` | HH:MM formatted time for screen wake. |

### 🔒 [API]
| Parameter | Default | Description |
|---|---|---|
| `AUTH_ENABLED` | `false` | If `true`, requires the `X-API-Token` header to match `TOKEN` on the sensitive endpoints: `/api/wifi`, `/api/mqtt/install`, `/api/system/reboot`, `/api/system/shutdown`. Disabled by default so the bundled Web UI keeps working out of the box; enable it if the device is reachable beyond a trusted LAN. |
| `TOKEN` | *(auto-generated)* | A random token generated on first boot. Copy it here (or read it from `conf.ini` after first run) and send it as `X-API-Token` when calling protected endpoints. |

## 🙏 Acknowledgments

A huge thanks to the open-source community and the creators of the incredible libraries that power this project:
- **[rpi-rgb-led-matrix](https://github.com/hzeller/rpi-rgb-led-matrix)** by hzeller (and the Rust bindings by AidanWallace)
- **[Actix-web](https://github.com/actix/actix-web)** for the blazing fast web API
- **[image-rs](https://github.com/image-rs/image)** for image processing
- **[rumqttc](https://github.com/bytebeamio/rumqtt)** for MQTT support
- And the entire Rust community for creating such an amazing ecosystem (Tokio, Serde, reqwest, tracing, etc.)!

Special thanks to the **RPiTeam** for the awesome pack of 600 GIFs!

## 📜 License
This project is licensed under the **[PolyForm Noncommercial License 1.0.0](LICENSE)**.

**In short:** you're free to use, modify, and share this project for any noncommercial purpose (personal use, hobby builds, research, education, non-profit/public institutions) - see the full [LICENSE](LICENSE) file for the exact terms. **Any commercial use (selling assembled units, kits, or derived products/services) requires a separate license - contact [Red1L](https://github.com/red77290) to discuss commercial terms.**

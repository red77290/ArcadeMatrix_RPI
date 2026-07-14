# ArcadeMatrix RPi 🍓👾

A Python-based port of the **ArcadeMatrix** project, specifically designed to run on a **Raspberry Pi** connected to an RGB LED Matrix (HUB75) via the Adafruit HAT or Joy-IT hardware.

This project replicates the awesome features of the ESP32 version while completely removing its hardware limitations.

---

## 🌟 Features (RPi Exclusives vs ESP32)

* **Dynamically Loadable Fonts (`.ttf`)**: No more hardcoded font files! Drop any `.ttf` or `.otf` font directly into the `fonts/` folder, and the Web UI will automatically list it for use on the Clock or Date.
* **Unlimited Clock/Date Sizes & Offsets**: You are no longer restricted to Size 1, 2, or 3. You can set the size to any number, and position the text freely on massive matrix panels (e.g. 256x64).
* **Massive Clock Selection**: Enjoy a variety of animated clocks including the classic Arcade, Binary, Cyberpunk, Flip, Word, and the brand new **Pac-Man**, **Tetris**, **SlotMachine**, and **Versus (Mugen)** clocks!
* **True Matrix Digital Rain (Katakana)**: A completely custom, buttery smooth, genuine Matrix digital rain effect (`DotGothic16`) with falling half-width Katakana and "unlit LED" negative space text punching through the rain.
* **Custom Smooth Gradients**: In addition to classic Publisher themes (Nintendo, Capcom, Sega...), you can now choose a **Custom Color / Gradient** theme and pick two colors to generate a dynamic gradient.
* **Dynamic Image Playlists (GIF/PNG/JPG)**: Read actual `.gif` and `.png` files dynamically straight from the filesystem without SD card fragmentation issues.
* **Python Power**: The entire engine, API, and frontend are served by Python (`Pillow` for drawing, `Flask` for the API), allowing for much faster modification.

---

## 🚀 Hardware Requirements

1. **Raspberry Pi**: Any model up to Pi 4 (Zero 2 W, Pi 3, Pi 4). 
   *(⚠️ **Pi 5 Warning**: The hzeller rgb-led-matrix library does NOT support the Pi 5 natively via GPIO due to the new RP1 chip. You must use an active adapter board for Pi 5! Pi 4 or Zero 2W are highly recommended).*
2. **RGB LED Matrix**: HUB75 panels (e.g., 64x64, 128x32, 256x64).
3. **Adafruit RGB Matrix HAT** (or Joy-IT, or custom wiring).
4. **MicroSD Card** (16GB or larger recommended for the Pre-compiled Image).

---

## 💾 Installation & Setup

### Option 1: Pre-compiled Image (Recommended for Users)
We provide a pre-compiled, fully automated `.img` file (`ArcadeMatrix_Release.img`). 
1. Flash the `.img` to your SD card using **Raspberry Pi Imager**.
2. Once flashed, insert the SD card into your PC/Mac. You will see a large 8GB **DATA** USB drive appear!
3. Open the `conf.ini` file located on this DATA drive to configure your Matrix size and your **Wi-Fi** credentials (`SSID` and `PASS`).
4. Plug the SD card into your Raspberry Pi and power it on.
5. The Matrix will immediately turn on and **display the IP address** for 5 seconds. Use this IP to access the Web UI!

### Option 2: Manual Installation
If you prefer to install it manually on a fresh **Raspberry Pi OS Lite (64-bit)**:
Once logged into your Raspberry Pi via SSH:

```bash
curl -sSL https://raw.githubusercontent.com/red77290/ArcadeMatrix_RPI/main/install.sh | bash
```
*(If the repository is private, you will need to `git clone` manually first and run `./install.sh` from inside the folder).*

The script will automatically:
1. Install Python 3, Flask, Pillow, and `build-essential`.
2. Download and compile the `hzeller/rpi-rgb-led-matrix` driver.
3. Setup `systemd` to automatically start ArcadeMatrix on boot.

---

## 🎨 Media Management

The pre-compiled image features a dedicated **8GB DATA partition** formatted as exFAT. This means you can plug your SD card directly into your Windows or Mac computer to drag-and-drop your files without needing SSH or FTP!

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

## 🔧 Matrix Configuration
If you have a matrix larger than 64x64 or 128x32, or if you are using a non-Adafruit HAT, you may need to tweak the `hzeller` arguments in `core/matrix.py`. By default, it's set to `--led-gpio-mapping=adafruit-hat` and `128x32`.

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
* `SSID`: The name of your Wi-Fi network.
* `PASS`: The password for your Wi-Fi network.
* `CONFIGURED`: Set to `false` to force the Raspberry Pi to attempt a connection on its next boot. Once connected successfully, the system automatically sets this back to `true`.

### 🎛️ [MATRIX]
* `ROWS` & `COLS`: The pixel dimensions of a single LED panel (e.g., `ROWS=32`, `COLS=64`).
* `HARDWARE_MAPPING`: The type of HAT/wiring used. Use `adafruit-hat` or `adafruit-hat-pwm` for Adafruit HATs. Use `regular-pi1` or `regular` if wiring directly to the GPIO.
* `CHAIN` & `PARALLEL`: Use `CHAIN` to specify how many panels are daisy-chained horizontally. Use `PARALLEL` if you are using multiple HUB75 ports vertically.
* `SLOWDOWN`: Increase this value (1 to 4) if your Matrix has visual glitches, flickering, or artifacts (especially on Raspberry Pi 3 and 4).

### ⏰ [TIME] & [DATE]
* `FORMAT_24H`: Set to `true` for 24-hour format, or `false` for 12-hour AM/PM format.
* `CLOCK_FONT`: Name of the `.ttf` or `.bdf` file in the `/fonts/` folder to use for the clock.
* `THEME`: The numeric ID of the animated clock or date theme (as seen in the Web UI).

### 🔄 [IDLE]
* `ROTATION`: Dictates the rotation behavior (`clock`, `gifs`, `sprites`, or `all`).
* `CLOCK_DURATION_SEC`: How long the clock stays on screen during rotation.
* `SELECTED_GIFS` / `SELECTED_SPRITES`: A comma-separated list of media you want to loop. Leave empty to play everything.

### 🌙 [STANDBY]
* `NIGHT_MODE_ENABLED`: If `true`, the Matrix will automatically turn off and wake up at the specified times.
* `TURN_OFF_AT` & `WAKE_UP_AT`: HH:MM formatted times for the Night Mode schedule.

## 📜 License
This project is open-source. Enjoy your Ultimate Retro Arcade Clock!

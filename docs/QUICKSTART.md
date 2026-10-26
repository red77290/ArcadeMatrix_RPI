🇬🇧 English | 🇫🇷 [Français](QUICKSTART_FR.md) | 🇪🇸 [Español](QUICKSTART_ES.md)

# Quickstart Guide

This guide will help you install and configure ArcadeMatrix on your Raspberry Pi.

## 1. Installation (Recommended)

We provide a ready-to-use pre-compiled image.

1. Flash the `ArcadeMatrix_Release.img` file to your SD card using **Raspberry Pi Imager**.
2. Once finished, reinsert the SD card into your PC/Mac. An 8GB USB drive named **DATA** will appear.
3. Open the `config.json` file located on this **DATA** drive to insert your Wi-Fi credentials (`SSID` and `PASS`) and your matrix size.
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

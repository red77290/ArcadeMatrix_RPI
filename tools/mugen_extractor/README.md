# ArcadeMatrix MUGEN Sprite Extractor

🇬🇧 English | 🇫🇷 [Français](README_FR.md) | 🇪🇸 [Español](README_ES.md)

This Python script (`mugen_extractor.py`) is custom-designed to extract, optimize, and convert fighting game characters from the **MUGEN** engine to make them compatible with ArcadeMatrix's `FighterEngine` (both ESP32 C++ and Raspberry Pi Python versions).

## What is it for?

Fighting games (MUGEN in particular) handle sprites with complex color palettes (`.act`, `.sff`) and animation scripts (`.air`) that include variable delays between each frame, as well as collision boxes.

Furthermore, the size of an LED matrix is very limited (e.g., 64x32). Original MUGEN sprites are often too large and do not always have the same alignment from one animation to another (for example, a jumping character will have a larger image expanding upwards).

The goal of this tool is to:
1. **Read native MUGEN formats** (`.sff` v1 and `.air`).
2. **Decode the master palette** (so colors are correct).
3. **Select only the necessary animations** for ArcadeMatrix (`walk`, `attack`, `hit`, `win`, `special`, `super`, `fall`).
4. **Calculate a uniform scale** based on the character's standard height (in `stand` or `walk` position) so they fit within the height of your LED matrix (e.g., 32 pixels).
5. **Generate a perfect alignment (Virtual Ground)**: The tool calculates a global bounding box to ensure that the ground line (`ground_y`) and the center of the character (`origin_x`) remain perfectly fixed from one animation to another. This prevents the character from "jittering" or changing size when attacking!
6. **Convert to `.fgt` (Fighter Format)**: The `.fgt` format is an optimized binary format created specifically for ArcadeMatrix, storing pixels in RGB565 with a transparent color code, ready to be read ultra-fast by the ESP32 and Raspberry Pi.

## Prerequisites

Make sure you have Python 3 installed along with the PIL (Pillow) image library:

```bash
pip install Pillow
```

## MUGEN Directory Structure

The script expects you to provide a source folder containing multiple subfolders, one per character. Each character must contain at least their `.sff` and `.air` files.

Example:
```text
/path/to/mugen_chars/
    ├── Ryu/
    │   ├── ryu.sff
    │   ├── ryu.air
    │   └── ryu.def
    ├── Ken/
    │   ├── ken.sff
    │   └── ken.air
    └── ChunLi/
```

## How to use it

Run the script with command-line arguments - no need to edit any code:

```bash
python mugen_extractor.py --src /Path/To/Your/Mugen/chars --dest ./fighters_32
```

Options:
| Option | Short alias | Default | Description |
|---|---|---|---|
| `--src` | `-i` | *(required)* | Directory containing your MUGEN character subfolders. |
| `--dest` | `-o` | `./fighters_32` | Output directory for generated `.fgt` files + `index.json`/`index.txt`. |
| `--mode` | | `FULLSIZE` | `SCALED` resizes characters to fit panel height (standard ESP32, no PSRAM); `FULLSIZE` keeps 1:1 scale (RPi or ESP32-S3 with PSRAM). |
| `--scale` | `--scaling` | `None` | Custom scaling factor (e.g. `0.5` to scale sprites down by 50% saving 75% RAM, `0.8`, `2.0`). Overrides mode calculation. |
| `--compress` | | disabled | Compresses `.fgt` files with gzip (`.fgt.gz`) - useful for RPi disk space saving. |

To target both a 32px and a 64px matrix, just run it twice with different `--dest` folders:

```bash
python mugen_extractor.py --src /Path/To/Your/Mugen/chars --dest ./fighters_32
python mugen_extractor.py --src /Path/To/Your/Mugen/chars --dest ./fighters_64
```

### Alternative: interactive wrapper (no command-line flags needed)

If you'd rather not type flags yourself, `start_extractor.sh` (macOS/Linux) /
`start_extractor.bat` (Windows) create a local Python virtual environment, install `Pillow`
automatically, and prompt you for the input/output folders interactively (they call
`mugen_extractor.py -i <input> -o <output>` for you):

```bash
./start_extractor.sh     # macOS/Linux
start_extractor.bat      # Windows
```

### Extraction Process

The script creates (or empties) the single output folder given by `--dest`/`-o` (default
`./fighters_32`) - run it twice with different `--dest` values if you need both a 32px and a 64px
export (see the "target both" example above). For each character, it creates a subfolder (e.g.,
`fighters_32/Ryu/`) containing:
- `walk.fgt`
- `attack.fgt`
- `hit.fgt`
- `win.fgt`
- *(and optionally `special1.fgt`/`special2.fgt`/`special3.fgt`, `super1.fgt`/`super2.fgt`/`super3.fgt`, and `fall.fgt` - up to 3 special moves and 3 super/ultra moves are auto-detected per character from their MUGEN `.air` animation IDs; any that aren't found are simply skipped)*

It also generates two index files at the root of the export folder, read by different engines:
- `index.json` - full metadata including `has_special`/`has_super`/`special_count`/`super_count`. Read by the **Raspberry Pi** engine (`engines/fighter.py`), which uses these flags to pick among all loaded special/super variants at fight time.
- `index.txt` - a simpler flat CSV (`name,height,ground_y,origin_x,width,head_y`) with no special/super metadata. Read by the **ESP32** engine (`FighterEngine.cpp`), which doesn't need those flags: it just attempts to load one random `special1`-`special3`/`super1`-`super3` file per battle and gracefully skips if that specific file doesn't exist for a given character (memory-conscious - only one special/super animation set is kept loaded at a time on ESP32, vs. all three on RPi).

Both index files always contain the shared positioning metadata (`height`, `ground_y`, `origin_x`, `width`, `head_y`) needed by both engines to correctly align fighters on the matrix.

## Why did characters ignore the ground line before?

Previously, each animation (`walk`, `attack`) was scaled in isolation by cropping transparent pixels. As a result, a high attack made the attack image larger than the walk image, changing the scale and shifting the character downwards.

With this **v4** version, the script performs two passes:
1. It measures the character's global maximum proportions across all their animations combined.
2. It applies a strict scale ratio based solely on their walk/idle animation.
3. It draws all frames onto a global fixed-size "Canvas" (e.g., 48x48), so that the axis of the character's feet always falls on the exact `ground_y` pixel. The engines read this `ground_y` value to align them together!

---
*This script is open source and designed for the ArcadeMatrix ecosystem.*

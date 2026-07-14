# ArcadeMatrix MUGEN Sprite Extractor

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

In the `mugen_extractor.py` script, scroll down to the very bottom to the `if __name__ == "__main__":` section and modify the paths according to your setup:

```python
if __name__ == "__main__":
    # 1. Folder containing MUGEN characters
    src_dir = "/Path/To/Your/Mugen/chars"
    
    # 2. Destination folders and target heights (TARGET_HEIGHT)
    out_dirs = [
        ("./fighters_32", 32), # For P64x32 matrix
        ("./fighters_64", 64)  # For P128x64 or P64x64 matrix
    ]
```

Then run the script:

```bash
python mugen_extractor.py
```

### Extraction Process

The script will create (or empty) the `fighters_32` and `fighters_64` folders. For each character, it will create a subfolder (e.g., `fighters_32/Ryu/`) containing:
- `walk.fgt`
- `attack.fgt`
- `hit.fgt`
- `win.fgt`
- *(and optionally `special1.fgt`, `super1.fgt`, `fall.fgt` if found)*

It also generates two index files at the root of the export folder:
- `index.json`
- `index.txt`

These index files contain the metadata (Height, `ground_y`, `origin_x`, etc.) needed by the ArcadeMatrix rendering engines to correctly position the fighters on the matrix.

## Why did characters ignore the ground line before?

Previously, each animation (`walk`, `attack`) was scaled in isolation by cropping transparent pixels. As a result, a high attack made the attack image larger than the walk image, changing the scale and shifting the character downwards.

With this **v4** version, the script performs two passes:
1. It measures the character's global maximum proportions across all their animations combined.
2. It applies a strict scale ratio based solely on their walk/idle animation.
3. It draws all frames onto a global fixed-size "Canvas" (e.g., 48x48), so that the axis of the character's feet always falls on the exact `ground_y` pixel. The engines read this `ground_y` value to align them together!

---
*This script is open source and designed for the ArcadeMatrix ecosystem.*

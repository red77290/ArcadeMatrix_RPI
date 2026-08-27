# Architecture & Developer Guide — MUGEN Sprite Extractor

🇬🇧 English | 🇫🇷 [Français](ARCHITECTURE_FR.md) | 🇪🇸 [Español](ARCHITECTURE_ES.md) | 📖 [User Guide](README.md)

This document details the software architecture, parser internals, binary file formats, and developer guidelines for `tools/mugen_extractor/mugen_extractor.py`.

---

## 1. Purpose & Overview

The **`mugen_extractor.py`** script is designed to convert fighting game characters created for the **Elecbyte M.U.G.E.N** engine into optimized binary animations (`.fgt` / `.fgt.gz`) tailored for the **ArcadeMatrix** combat engines (ESP32 C++ and Raspberry Pi Rust/Python).

### Core Technical Challenges Solved:
1. **MUGEN Format Heterogeneity:** Proprietary binary formats (`.sff` v1), animation scripts (`.air`), state scripts (`.cns` / `.st`), and indexed palettes (`.act`).
2. **Palette Harmonization:** MUGEN authors frequently exported sprites with dummy or corrupted internal palettes, relying on dynamic engine remapping via `.act` files.
3. **Inter-Animation Spatial Alignment (Virtual Ground):** Preventing the character from "jittering" or shifting gravity center across idle, walk, hit, jump, or attack frames.
4. **Embedded Memory Constraints:** Generating pre-rendered, lightweight RGB565 binary streams with direct transparency channels.

---

## 2. Global Processing Pipeline

```
                     +----------------------------------+
                     | MUGEN Character Directory        |
                     +----------------------------------+
                                       |
                   +-------------------+-------------------+
                   |                   |                   |
                   v                   v                   v
            [ DefParser ]       [ CnsParser ]       [ SFFv1Parser ]
                   |                   |                   |
     - Finds sprite/anim/cns    - [Size] (head, scale) - Decodes subheaders
     - Resolves pal.defaults    - [Statedef] anims     - Caches PCX data
                   |                   |                   |
                   +-------------------+-------------------+
                                       |
                                       v
                                [ AirParser ]
                   - Actions & Frames ([Begin Action])
                   - Relative offsets (ox, oy)
                   - Graphic flips (H, V, HV)
                                       |
                                       v
                         [ resolve_master_palette() ]
                   - Heuristic scoring (score_palette)
                   - Modulo Bank Expansion (16/32/64c)
                   - Dynamic Offset Shifting
                   - Priority to pal.defaults
                                       |
                   +-------------------+-------------------+
                   |                                       |
                   v                                       v
            [ Pass 1: Geometry ]                    [ Pass 2: Rendering ]
     - Global Bounding Box (orig_w, orig_h)   - Apply master palette
     - Computes ground_y, origin_x, head_y    - Nearest Neighbor resize
     - Computes scale factor                  - Encode binary RGB565 (.fgt)
```

---

## 3. Decoded MUGEN Format Specifications

### 3.1. Character Definition File (`.def`) — `DefParser`
The `.def` file serves as the character entry point, mapping all component files:

* **`[Info]`:**
  * `pal.defaults = 1, 2, ...`: Author's official palette priority order.
* **`[Files]`:**
  * `sprite = <name>.sff`: Official sprite pack.
  * `anim = <name>.air`: Official animation script.
  * `cns = <name>.cns`, `st = <name>.cns`, `st1..st10 = ...`: Constants and state scripts.
  * `pal1` to `pal12 = <name>.act`: 12 color palette mappings.

### 3.2. State & Constants Script (`.cns` / `.st`) — `CnsParser`
The parser extracts two critical sections:
1. **`[Size]`:**
   * `head.pos = X, Y`: Head position relative to ground (negative Y value, e.g. `-90`).
   * `xscale`, `yscale`: Official scale factors (e.g. `0.5` for Hi-Res sprites, `2.0` for retro sprites).
2. **`[Statedef <ID>]`:**
   * Standard MUGEN state identifiers:
     * `0`: Stand
     * `20`, `21`: Walk Forward / Walk Back
     * `200..999`: Normal attacks
     * `5000..5020`: Hit reaction
     * `5030..5150`: Fall / K.O.
     * `180..199`: Win / Taunt
     * `1000..2999`: Special attacks
     * `3000..4999`: Super attacks
   * `CnsParser` resolves `anim = <ID>` or `[State ..., ...] type = ChangeAnim` $\rightarrow$ `value = <ID>`.

### 3.3. SFFv1 Sprite File (`.sff`) — `SFFv1Parser`
* **Global Header (512 bytes):**
  * `signature`: `ElecbyteSpr\0` (12 bytes)
  * `num_images` (uint32 at offset 20)
  * `first_offset` (uint32 at offset 24)
* **Image Subheader (32 bytes):**
  * `next_offset` (uint32, 4B)
  * `data_length` (uint32, 4B)
  * `x`, `y` (int16, 4B): Sprite axis alignment relative to origin
  * `group`, `image` (uint16, 4B): Identification key `(grp, img)`
  * `prev_copy` (uint16, 2B)
  * `same_pal` (uint8, 1B)
* **PCX Data:**
  * 8-bit RLE PCX encoded image.
  * Last 768 bytes contain VGA 256-color palette (if `data_length > 768`).

### 3.4. AIR Animation File (`.air`) — `AirParser`
Each action block starts with `[Begin Action <ID>]`. Frame rows adhere to standard Elecbyte syntax:
```text
grp, img, ox, oy, delay, [flip], [blend]
```
* `ox`, `oy`: Pixel offsets added to sprite axis (`total_ox = sff_x - air_ox`).
* `delay`: Frame display duration in ticks (1 tick = 1/60 sec, `-1` = infinite loop).
* `flip`: Mirroring flags (`H` for horizontal, `V` for vertical, `HV` for both).
* `blend`: Transparency mode (`A` = Additive, `S` = Subtractive).

---

## 4. Binary `.fgt` Format Specification (ArcadeMatrix Fighter Format)

The `.fgt` format is a compact, streamable binary animation format engineered for zero-allocation reading on microcontrollers:

### Binary Layout:

| Offset | Size | Type | Description |
|---|---|---|---|
| `0x00` | 3 bytes | ASCII | Magic Bytes: `FGT` |
| `0x03` | 1 byte | uint8 | Format Version (`1`) |
| `0x04` | 2 bytes | uint16 LE | Canvas Width (`canvas_w`) |
| `0x06` | 2 bytes | uint16 LE | Canvas Height (`canvas_h`) |
| `0x08` | 2 bytes | uint16 LE | Number of frames (`num_frames`) |
| `0x0A` | 2 bytes | uint16 LE | Transparent color RGB565 (`0x0000`) |
| `0x0C` | `2 * num_frames` | uint16 LE[] | Frame delays array (in ticks) |
| `0x0C + (2*N)` | `N * W * H * 2` | uint16 LE[] | Contiguous RGB565 pixel stream per frame |

> **Note on Compression:** The `--compress` flag produces standard `.fgt.gz` files, ideal for Raspberry Pi and SD cards.

---

## 5. Palette Resolution Algorithm (`resolve_master_palette`)

To ensure 100% authentic color rendering across arcade rips (Capcom, NeoGeo, Simpsons), digitized captures (Mortal Kombat, Midway), and original MUGEN creations:

1. **Body Reference Sprite Selection:**
   * Iterates through key body animation groups (`0`, `1`, `5`, `10`, `20`, `21`, `40`, `100`, `200`, `5000`).
   * Selects the frame with the maximum count of distinct pixel indices for optimal evaluation.
   * Excludes group `9000` (portraits / select icons) to avoid contamination.

2. **Multi-Candidate Collection & Expansion:**
   * **`.def` Candidates:** Palettes declared in `[Files]` (`pal1..pal12`), with highest priority to author's `pal.defaults`.
   * **Modulo Bank Expansion (16, 32, 64):** For partial bank rips (e.g. Krusty the Clown, Capcom CPS2), generates repeated `bank16`, `bank32`, `bank64` variants.
   * **Offset Shifting:** For digitized characters (Mortal Kombat) where palettes start at high slots (e.g. 176), shifts to slot 0 via `shift_min`.
   * **`SFFv1` Candidates:** Reference sprite palette, stance palette `(0,0)`, first SFF palette, and local sub-palettes.
   * **`.act` Candidates:** Extra `.act` files found in character folder.

3. **Evaluation & Scoring Function (`score_palette`):**
   * **Monochrome Rejection:** If palette renders a single color (`u_colors <= 1`) while sprite has multiple indices, reject (`score = -999.0`).
   * **Pure Binary Debug Mask Rejection:** If $\le 3$ colors with at least 2 saturated binary corners `(0/255, 0/255, 0/255)`, reject.
   * **Score Formula:**
     $$\text{Base Score} = \text{Unique Colors} \times 10$$
     $$\text{Natural Luminance Bonus} = +100 \quad \text{if } 20 \le \text{Mean Luminance} \le 210$$
     $$\text{Over/Under-exposure Penalty} = -30 \text{ (if } L < 15 \text{)}, \quad -80 \text{ (if } L > 225 \text{)}$$
   * **Source & Author Bonuses:**
     * `DEF(pal.defaults)`: **+150 pts** (bank/shift variants: **+140 pts**)
     * `DEF(pal1..12)`: **+100 pts** (bank/shift variants: **+90 pts**)
     * `SFF(body_sprite)`: **+40 pts**
     * `SFF(stand)`: **+35 pts**
     * `SFF(first)`: **+30 pts**
     * `SFF(local)`: **+20 pts**
     * `ACT(folder)`: **+10 pts**

---

## 6. Developer Guide: How to Contribute

### 6.1. Adding SFFv2 Support (MUGEN 1.0 / 1.1)
SFFv2 uses compressed sub-blocks (LZO, RLE8, PNG):
* Implement `SFFv2Parser`.
* Detect header signature: `ElecbyteSpr\x00` with version `0x02, 0x00, 0x00, 0x02`.
* Decompress sub-blocks into the in-memory cache `self.images[(grp, img)] = {'x': x, 'y': y, 'data': raw_rgba_or_indexed}`.

### 6.2. Supporting Alpha Blending (`A`, `S`, `ASxxxDxxx`)
Currently pixels with alpha < 128 are treated as transparent (`0x0000`).
* In `Pass 2` (Rendering), parse `fr.get('blend')`.
* For additive blending (`A`), apply transparency mask or pre-composite onto dark backdrop.

### 6.3. Adding Hybrid Palette Mode (Separate FX / Projectiles)
When a fireball or weapon uses a distinct palette from the fighter body:
* Evaluate `score_palette` for the local PCX palette on that specific frame.
* If local score is high (> 30.0) and group is FX (1000+), apply local palette instead of `master_palette`.

---

## 7. Validation & Test Commands

To test your changes against reference character rosters:

```bash
# Interactive guided test
./start_extractor.sh

# Direct CLI command
python3 mugen_extractor.py --src "/path/to/chars" --dest "./test_out" --mode SCALED --workers 4
```

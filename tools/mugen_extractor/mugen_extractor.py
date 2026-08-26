import os
import glob
import struct
import io
import time
import logging
import json
from PIL import Image

TRANSPARENT_COLOR_565 = 0x0000

def rgb888_to_rgb565(r, g, b):
    return ((r & 0xF8) << 8) | ((g & 0xFC) << 3) | (b >> 3)

def find_file_case_insensitive(directory, target_path):
    target_path = target_path.replace('\\', '/')
    parts = target_path.split('/')
    curr_dir = directory
    for part in parts:
        if not os.path.isdir(curr_dir): return None
        found = False
        for entry in os.listdir(curr_dir):
            if entry.lower() == part.lower():
                curr_dir = os.path.join(curr_dir, entry)
                found = True
                break
        if not found: return None
    return curr_dir

class SFFv1Parser:
    def __init__(self, filepath):
        self.filepath = filepath
        self.images = {}
        self.first_palette = None
        self.stand_palette = None  # palette from group 0, image 0
        self.candidate_palettes = []  # list of (source_name, 768_bytes)
        self.parse()

    def parse(self):
        seen_pals = set()
        with open(self.filepath, 'rb') as f:
            header = f.read(512)
            if not header.startswith(b'ElecbyteSpr'): return
            
            num_images = struct.unpack('<I', header[20:24])[0]
            first_offset = struct.unpack('<I', header[24:28])[0]
            
            next_offset = first_offset
            for _ in range(num_images):
                if next_offset == 0: break
                f.seek(next_offset)
                subheader = f.read(32)
                if len(subheader) < 32: break
                
                next_offset, data_length, x, y, grp, img, prev, same_pal = struct.unpack('<IIhhHHHxB', subheader[:20])
                
                if data_length > 0:
                    pcx_data = f.read(data_length)
                    if len(pcx_data) > 768:
                        pal = pcx_data[-768:]
                        if not self.first_palette:
                            self.first_palette = pal
                        if grp == 0 and img == 0 and not self.stand_palette:
                            self.stand_palette = pal
                        # Collect candidate palettes from key sprite groups
                        if (grp in (0, 20) or grp >= 7000) and pal not in seen_pals:
                            seen_pals.add(pal)
                            self.candidate_palettes.append((f"SFF({grp},{img})", pal))

                    self.images[(grp, img)] = {
                        'x': x,
                        'y': y,
                        'data': pcx_data
                    }

class AirParser:
    def __init__(self, filepath):
        self.filepath = filepath
        self.animations = {}
        self.parse()

    def parse(self):
        with open(self.filepath, 'r', errors='ignore') as f:
            lines = f.readlines()
            
        current_action = None
        for line in lines:
            line = line.split(';')[0].strip()
            if not line: continue
            
            if line.lower().startswith('[begin action'):
                try:
                    action_id = int(line.split()[2].replace(']', ''))
                    current_action = action_id
                    self.animations[current_action] = []
                except: pass
            elif current_action is not None and ',' in line:
                parts = line.split(',')
                if len(parts) >= 5:
                    try:
                        grp = int(parts[0].strip())
                        img = int(parts[1].strip())
                        delay = int(parts[4].strip())
                        if delay == -1: delay = 10
                        if delay > 0:
                            self.animations[current_action].append({'grp': grp, 'img': img, 'delay': delay})
                    except: pass

def score_palette(pal_bytes, indices, trans_idx):
    """
    Score how well a 768-byte palette renders a sprite given its pixel indices.
    
    Returns a float score. Higher = better palette.
    
    Scoring formula: unique_colors × (1 - penalty_black) × (1 - penalty_neon)
      - unique_colors: distinct RGB triples for visible (non-transparent) indices
      - penalty_black: max(0, black_ratio - 0.15) — tolerates ~15% black for outlines
      - penalty_neon:  tolerates up to 15% saturated accents (eyes, gems, bandanas)
                       and heavily penalizes dummy rainbow masking palettes (>15% neon).
    """
    if not pal_bytes or len(pal_bytes) < 768:
        return 0.0
    
    used_indices = set(idx for idx in indices if isinstance(idx, int) and idx != trans_idx)
    if not used_indices:
        return 0.0
    
    colors = set()
    black_count = 0
    neon_count = 0
    total = 0
    for idx in used_indices:
        if idx * 3 + 2 < len(pal_bytes):
            r, g, b = pal_bytes[idx * 3], pal_bytes[idx * 3 + 1], pal_bytes[idx * 3 + 2]
            colors.add((r, g, b))
            total += 1
            if r < 5 and g < 5 and b < 5:
                black_count += 1
            # Detect pure saturated primary/secondary masking colors
            max_c = max(r, g, b)
            min_c = min(r, g, b)
            if max_c > 160 and min_c < 20:
                if (r > 160 and g < 20 and b < 20) or (g > 160 and r < 20 and b < 20) or (b > 160 and r < 20 and b < 20):
                    neon_count += 1
                elif (r > 160 and b > 160 and g < 20) or (r > 160 and g > 160 and b < 20) or (g > 160 and b > 160 and r < 20):
                    neon_count += 1
    
    black_ratio = black_count / max(total, 1)
    neon_ratio = neon_count / max(total, 1)
    unique = len(colors)
    score = unique * (1.0 - max(0.0, black_ratio - 0.15)) * (1.0 - max(0.0, neon_ratio - 0.15) * 3)
    return max(0.0, score)

def load_all_act_palettes(char_dir):
    """
    Load all available .act palettes from the .def file and character directory.
    """
    pals = []
    seen = set()
    
    # 1. From .def file
    def_files = glob.glob(os.path.join(char_dir, "*.def"))
    if def_files:
        try:
            with open(def_files[0], "r", errors="ignore") as f:
                lines = f.readlines()
            for line in lines:
                lc = line.split(";")[0].strip()
                if "=" in lc:
                    k, v = lc.split("=", 1)
                    k = k.strip().lower()
                    v = v.strip().split(";")[0].strip().replace('"', '').replace("'", "")
                    if k.startswith("pal") and not k.startswith("pal.defaults"):
                        act_path = find_file_case_insensitive(char_dir, v)
                        if act_path and os.path.isfile(act_path):
                            with open(act_path, "rb") as af:
                                d = af.read(768)
                                if len(d) == 768 and d not in seen:
                                    seen.add(d)
                                    pals.append((f"ACT({k}:{v})", d))
        except:
            pass
            
    # 2. Any additional .act files in the directory
    for af in glob.glob(os.path.join(char_dir, "*.act")) + glob.glob(os.path.join(char_dir, "*.ACT")):
        try:
            with open(af, "rb") as f:
                d = f.read(768)
                if len(d) == 768 and d not in seen:
                    seen.add(d)
                    pals.append((f"ACT({os.path.basename(af)})", d))
        except:
            pass
            
    return pals

def resolve_master_palette(char_dir, sff):
    """
    Resolve the best palette for a character using an intelligent scoring system.
    
    Candidates:
      1. SFF first_palette & stand_palette
      2. SFF candidate palettes from key groups (0, 20, 7000..9999)
      3. All .act palettes from .def and character folder
    
    Each candidate is scored against the stand sprite's pixel indices.
    The palette with the highest coverage score wins.
    """
    char_name = os.path.basename(char_dir)
    
    # Find the stand sprite to use as scoring reference
    test_key = None
    for k in [(0, 0), (0, 1), (20, 0)]:
        if k in sff.images:
            test_key = k
            break
    if not test_key and sff.images:
        test_key = list(sff.images.keys())[0]
    
    if not test_key:
        return sff.first_palette or sff.stand_palette
    
    # Extract pixel indices from the test sprite
    try:
        img_obj = Image.open(io.BytesIO(sff.images[test_key]['data']))
        if hasattr(img_obj, 'get_flattened_data'):
            indices = list(img_obj.get_flattened_data())
        else:
            indices = list(img_obj.getdata())
    except:
        return sff.first_palette or sff.stand_palette
    
    # If the sprite is direct truecolor (RGB/RGBA tuples), no palette mapping needed
    if indices and isinstance(indices[0], (tuple, list)):
        return sff.first_palette or sff.stand_palette or (b'\x00' * 768)
    
    trans_idx = indices[0] if indices else 0
    
    # Collect candidates
    candidates = []
    seen = set()
    
    if sff.first_palette and sff.first_palette not in seen:
        seen.add(sff.first_palette)
        candidates.append(("SFF(first)", sff.first_palette))
        
    if sff.stand_palette and sff.stand_palette not in seen:
        seen.add(sff.stand_palette)
        candidates.append(("SFF(stand)", sff.stand_palette))
        
    for name, pal in sff.candidate_palettes:
        if pal not in seen:
            seen.add(pal)
            candidates.append((name, pal))
            
    for name, pal in load_all_act_palettes(char_dir):
        if pal not in seen:
            seen.add(pal)
            candidates.append((name, pal))
    
    if not candidates:
        return None
    
    # Score each candidate
    best_score = -1
    best_pal = None
    best_name = ""
    for name, pal in candidates:
        s = score_palette(pal, indices, trans_idx)
        if s > best_score:
            best_score = s
            best_pal = pal
            best_name = name
    
    logging.info(f"{char_name}: palette={best_name} (score={best_score:.1f})")
    return best_pal

def process_character(char_dir, out_dir, target_height=None, extract_mode=None, custom_scale=None, compress_fgt=None):
    if target_height is None:
        target_height = globals().get('TARGET_HEIGHT', 32)
    if extract_mode is None:
        extract_mode = globals().get('EXTRACT_MODE', 'FULLSIZE')
    if custom_scale is None:
        custom_scale = globals().get('CUSTOM_SCALE', None)
    if compress_fgt is None:
        compress_fgt = globals().get('COMPRESS_FGT', False)

    char_name = os.path.basename(char_dir)
    sff_files = glob.glob(os.path.join(char_dir, "*.sff"))
    air_files = glob.glob(os.path.join(char_dir, "*.air"))
    if not sff_files or not air_files: return False

    sff = SFFv1Parser(sff_files[0])
    air = AirParser(air_files[0])

    master_palette = resolve_master_palette(char_dir, sff)
    if not master_palette:
        return False

    required_anims = {}
    
    # 1. Stand (0, 5, 10, 11, 20 or first available animation)
    for act_id in [0, 5, 10, 11, 20]:
        if act_id in air.animations:
            required_anims[act_id] = 'stand'
            break
    if 'stand' not in required_anims.values() and air.animations:
        required_anims[list(air.animations.keys())[0]] = 'stand'
    
    # 2. Walk (20, 21, 100, 105, 106, 110, 115, 120, 40)
    for act_id in [20, 21, 100, 105, 106, 110, 115, 120, 40]:
        if act_id in air.animations and act_id not in required_anims:
            required_anims[act_id] = 'walk'
            break

    # 3. Attack (any normal, air, or command attack from 200 to 999)
    for act_id in range(200, 1000):
        if act_id in air.animations and act_id not in required_anims:
            required_anims[act_id] = 'attack'
            break
            
    # 4. Hit (any hit/damage reaction from 5000 to 5299)
    for act_id in range(5000, 5300):
        if act_id in air.animations and act_id not in required_anims:
            required_anims[act_id] = 'hit'
            break
            
    # 5. Fall (5030, 5050, 5070, 5080, 5100, 5110, 5120, 5150)
    for act_id in [5030, 5050, 5070, 5080, 5100, 5110, 5120, 5150]:
        if act_id in air.animations and act_id not in required_anims:
            required_anims[act_id] = 'fall'
            break

    # 6. Win (180-189 win poses, 190-199 taunts, 170-179)
    for act_id in list(range(180, 200)) + list(range(170, 180)):
        if act_id in air.animations and act_id not in required_anims:
            required_anims[act_id] = 'win'
            break

    # 7. Specials (1000-2999)
    special_count = 0
    for act_id in range(1000, 3000):
        if act_id in air.animations and act_id not in required_anims and len(air.animations[act_id]) > 0:
            special_count += 1
            required_anims[act_id] = f'special{special_count}'
            if special_count >= 3: break

    # 8. Supers (3000-4999)
    super_count = 0
    for act_id in range(3000, 5000):
        if act_id in air.animations and act_id not in required_anims and len(air.animations[act_id]) > 0:
            super_count += 1
            required_anims[act_id] = f'super{super_count}'
            if super_count >= 3: break

    # Pass 1: Collect valid frames, calculate global bounding box and base scale
    all_valid_frames = {}
    global_min_x, global_min_y, global_max_x, global_max_y = 9999, 9999, -9999, -9999
    walk_h = None
    stand_head_y_local = 0
    
    for anim_id, anim_name in required_anims.items():
        if anim_id not in air.animations: continue
        frames = air.animations[anim_id]
        if not frames: continue

        valid_frames = []
        for f in frames:
            if (f['grp'], f['img']) in sff.images:
                img_info = sff.images[(f['grp'], f['img'])]
                try:
                    img_obj = Image.open(io.BytesIO(img_info['data']))
                    
                    # Detect broken palettes (solid silhouettes with <= 2 colors including transparency)
                    colors = img_obj.convert('RGB').getcolors(256)
                    if colors is not None and len(colors) <= 2:
                        continue
                        
                except Exception as e:
                    continue
                
                ox, oy = img_info['x'], img_info['y']
                
                min_x = -ox
                min_y = -oy
                max_x = img_obj.width - ox
                max_y = img_obj.height - oy
                
                # Skip crazy frames (projectiles far away)
                if min_x < -1000 or min_y < -1000 or max_x > 1000 or max_y > 1000:
                    continue
                    
                global_min_x = min(global_min_x, min_x)
                global_min_y = min(global_min_y, min_y)
                global_max_x = max(global_max_x, max_x)
                global_max_y = max(global_max_y, max_y)
                
                valid_frames.append({
                    'img': img_obj,
                    'ox': ox,
                    'oy': oy,
                    'delay': f['delay'],
                    'min_y': min_y,
                    'max_y': max_y
                })

        if valid_frames:
            all_valid_frames[anim_name] = valid_frames
            if anim_name in ['stand', 'walk'] and walk_h is None:
                # Compute local max_y and min_y to determine the base scale
                l_min = min([fr['min_y'] for fr in valid_frames])
                l_max = max([fr['max_y'] for fr in valid_frames])
                walk_h = l_max - l_min
                stand_head_y_local = l_min

    if not all_valid_frames: return False
    
    # Universal Smart Fallbacks: allow characters missing specific animation states to use valid alternatives
    # 1. Fallback for 'stand' if missing
    if 'stand' not in all_valid_frames:
        for alt in ['walk', 'win', 'attack']:
            if alt in all_valid_frames:
                all_valid_frames['stand'] = all_valid_frames[alt]
                break
        if 'stand' not in all_valid_frames and all_valid_frames:
            all_valid_frames['stand'] = list(all_valid_frames.values())[0]

    if 'stand' not in all_valid_frames:
        logging.warning(f"Character {char_name} has no valid standing frames. Skipping.")
        return False

    # 2. Fallback for 'walk' -> use 'stand'
    if 'walk' not in all_valid_frames:
        all_valid_frames['walk'] = all_valid_frames['stand']

    # 3. Fallback for 'attack' -> use special1..3, super1..3, win, walk, or stand
    if 'attack' not in all_valid_frames:
        for alt in ['special1', 'super1', 'special2', 'super2', 'special3', 'super3', 'win', 'walk', 'stand']:
            if alt in all_valid_frames:
                all_valid_frames['attack'] = all_valid_frames[alt]
                break

    # 4. Fallback for 'hit' -> use 'fall' or last frame of 'stand'
    if 'hit' not in all_valid_frames:
        if 'fall' in all_valid_frames:
            all_valid_frames['hit'] = all_valid_frames['fall']
        elif 'stand' in all_valid_frames:
            all_valid_frames['hit'] = all_valid_frames['stand'][-1:]

    # 5. Fallback for 'fall' -> use 'hit'
    if 'fall' not in all_valid_frames and 'hit' in all_valid_frames:
        all_valid_frames['fall'] = all_valid_frames['hit']

    # 6. Fallback for 'win' -> use 'stand'
    if 'win' not in all_valid_frames:
        all_valid_frames['win'] = all_valid_frames.get('stand', all_valid_frames.get('walk'))

    # Ensure all mandatory animations are satisfied
    for req in ['stand', 'walk', 'attack', 'hit', 'win']:
        if req not in all_valid_frames:
            logging.warning(f"Character {char_name} missing mandatory animation '{req}'. Skipping.")
            return False

    if walk_h is None or walk_h <= 0: walk_h = global_max_y - global_min_y
    if walk_h <= 0: walk_h = target_height
    
    # Cap global bounds to prevent massive RAM usage
    global_min_x = max(-500, global_min_x)
    global_min_y = max(-500, global_min_y)
    global_max_x = min(500, global_max_x)
    global_max_y = min(500, global_max_y)
    
    orig_w = global_max_x - global_min_x
    orig_h = global_max_y - global_min_y
    
    if orig_w <= 0 or orig_h <= 0: return False
    
    if custom_scale is not None and float(custom_scale) > 0:
        scale = float(custom_scale)
    elif extract_mode == 'SCALED':
        scale = 1.0
        if walk_h > target_height:
            scale = target_height / walk_h
        canvas_w = max(1, int(orig_w * scale))
        canvas_h = max(1, int(orig_h * scale))
    else:
        # FULLSIZE Mode
        scale = 1.0
        if target_height == 32:
            scale = 0.5
            
    canvas_w = max(1, int(orig_w * scale))
    canvas_h = max(1, int(orig_h * scale))
        
    ground_y = int(-global_min_y * scale)
    origin_x = int(-global_min_x * scale)
    head_y = int((-global_min_y + stand_head_y_local) * scale)

    # Pass 2: Render and save frames
    char_out_dir = os.path.join(out_dir, char_name)
    os.makedirs(char_out_dir, exist_ok=True)
    
    for anim_name, valid_frames in all_valid_frames.items():
        COMPRESS = compress_fgt
        ext = ".fgt.gz" if COMPRESS else ".fgt"
        out_file = os.path.join(char_out_dir, f"{anim_name}{ext}")
        
        import gzip
        open_func = gzip.open if COMPRESS else open
        with open_func(out_file, 'wb') as f:
            f.write(b'FGT')
            f.write(struct.pack('<B', 1))
            f.write(struct.pack('<H', canvas_w))
            f.write(struct.pack('<H', canvas_h))
            f.write(struct.pack('<H', len(valid_frames)))
            f.write(struct.pack('<H', TRANSPARENT_COLOR_565))
            
            for fr in valid_frames:
                delay = fr['delay']
                if delay < 0: delay = 65535
                elif delay > 65535: delay = 65535
                f.write(struct.pack('<H', delay))

            for fr in valid_frames:
                img_obj = fr['img']
                paste_x = -global_min_x - fr['ox']
                paste_y = -global_min_y - fr['oy']
                
                try:
                    if hasattr(img_obj, 'get_flattened_data'):
                        indices = list(img_obj.get_flattened_data())
                    else:
                        indices = list(img_obj.getdata())
                except:
                    indices = [0] * (img_obj.width * img_obj.height)
                
                trans_idx = indices[0] if indices else 0
                
                rgba_canvas = Image.new('RGBA', (orig_w, orig_h), (0,0,0,0))
                rgba_pixels = rgba_canvas.load()
                
                for py in range(img_obj.height):
                    for px in range(img_obj.width):
                        cx = paste_x + px
                        cy = paste_y + py
                        if 0 <= cx < orig_w and 0 <= cy < orig_h:
                            val = indices[py * img_obj.width + px]
                            if isinstance(val, tuple):
                                if len(val) >= 4:
                                    if val[3] > 0:
                                        rgba_pixels[cx, cy] = val
                                elif val != (0, 255, 0) and val != (255, 0, 255):
                                    rgba_pixels[cx, cy] = (val[0], val[1], val[2], 255)
                            else:
                                idx = int(val)
                                if idx != trans_idx and idx * 3 + 2 < len(master_palette):
                                    r = master_palette[idx*3]
                                    g = master_palette[idx*3 + 1]
                                    b = master_palette[idx*3 + 2]
                                    rgba_pixels[cx, cy] = (r, g, b, 255)
                                        
                if orig_w != canvas_w or orig_h != canvas_h:
                    rgba_canvas = rgba_canvas.resize((canvas_w, canvas_h), Image.Resampling.NEAREST)
                
                cropped_data = rgba_canvas.getdata()
                for r, g, b, a in cropped_data:
                    if a < 128:
                        f.write(struct.pack('<H', TRANSPARENT_COLOR_565))
                    else:
                        c565 = rgb888_to_rgb565(r, g, b)
                        if c565 == TRANSPARENT_COLOR_565: c565 = 0x0001
                        f.write(struct.pack('<H', c565))
                        
    stand_h = int(walk_h * scale) if walk_h else canvas_h
    return {
        'height': stand_h if stand_h > 0 else canvas_h,
        'canvas_height': canvas_h,
        'ground_y': ground_y,
        'head_y': head_y,
        'origin_x': origin_x,
        'width': canvas_w,
        'has_special': special_count > 0,
        'has_super': super_count > 0,
        'special_count': special_count,
        'super_count': super_count
    }

def process_character_task(args):
    """
    Top-level task runner for multiprocessing / multithreading.
    args: (char_dir, out_dir, target_height, extract_mode, custom_scale, compress_fgt)
    """
    char_dir, out_dir, target_height, extract_mode, custom_scale, compress_fgt = args
    char_name = os.path.basename(char_dir)
    try:
        res = process_character(
            char_dir,
            out_dir,
            target_height=target_height,
            extract_mode=extract_mode,
            custom_scale=custom_scale,
            compress_fgt=compress_fgt
        )
        return char_name, res
    except Exception as e:
        logging.error(f"Error processing {char_name}: {e}")
        return char_name, False

TARGET_HEIGHT = 32

if __name__ == "__main__":
    import argparse
    import concurrent.futures
    
    default_workers = max(1, (os.cpu_count() or 2) // 2)
    
    parser = argparse.ArgumentParser(description="Extract Mugen Characters for ArcadeMatrix")
    parser.add_argument("--src", "-i", dest="src", type=str, required=True, help="Source directory containing Mugen characters")
    parser.add_argument("--dest", "-o", dest="dest", type=str, default="fighters_32", help="Output directory for the generated .fgt files and index (default: ./fighters_32)")
    parser.add_argument("--mode", type=str, choices=['SCALED', 'FULLSIZE'], default='SCALED', 
                        help="SCALED: Resize character to perfectly fit screen height (for standard ESP32). FULLSIZE: Extract at 1:1 original scale (for RPi or ESP32-S3 with PSRAM).")
    parser.add_argument("--scale", "--scaling", dest="scale", type=float, default=None,
                        help="Custom scaling factor (e.g. 0.5 for 50%%, 0.8, 2.0). Overrides default SCALED/FULLSIZE mode calculations when specified.")
    parser.add_argument("--compress", action="store_true", help="Compress the output .fgt files using gzip (.fgt.gz). Ideal for RPi to save space.")
    parser.add_argument("--workers", "-j", type=int, default=default_workers, 
                        help=f"Number of parallel worker processes (default: {default_workers} = CPU cores / 2)")
    args = parser.parse_args()

    # Set global options for compatibility
    global EXTRACT_MODE, CUSTOM_SCALE, COMPRESS_FGT
    EXTRACT_MODE = args.mode
    CUSTOM_SCALE = args.scale
    COMPRESS_FGT = args.compress

    src_dir = args.src
    target_h = 64 if "64" in args.dest else 32
    out_dirs = [
        (args.dest, target_h)
    ]
    
    workers = args.workers if args.workers and args.workers > 0 else default_workers
    total_cpus = os.cpu_count() or 2
    print(f"Parallel Processing: using {workers} worker processes ({total_cpus} CPUs / 2)")
    
    start_time = time.time()
    chars = [os.path.join(src_dir, d) for d in sorted(os.listdir(src_dir)) if os.path.isdir(os.path.join(src_dir, d))]
    
    if CUSTOM_SCALE:
        print(f"Starting extraction in mode: {EXTRACT_MODE} with custom scale: {CUSTOM_SCALE}")
    else:
        print(f"Starting extraction in mode: {EXTRACT_MODE}")
    
    for out_dir, target_h in out_dirs:
        TARGET_HEIGHT = target_h
        os.makedirs(out_dir, exist_ok=True)
        print(f"\n--- Extracting for TARGET_HEIGHT={TARGET_HEIGHT} (Output: {out_dir}) ---")
        
        success_count = 0
        index_data = {}
        
        tasks = [
            (char_dir, out_dir, TARGET_HEIGHT, EXTRACT_MODE, CUSTOM_SCALE, COMPRESS_FGT)
            for char_dir in chars
        ]
        
        with concurrent.futures.ProcessPoolExecutor(max_workers=workers) as executor:
            futures = {executor.submit(process_character_task, task): task[0] for task in tasks}
            for i, future in enumerate(concurrent.futures.as_completed(futures), 1):
                char_name, result = future.result()
                if result:
                    success_count += 1
                    index_data[char_name] = result
                    print(f"[{i}/{len(chars)}] Processed: {char_name} (H: {result['height']}, Ground: {result['ground_y']})")
                else:
                    print(f"[{i}/{len(chars)}] Skipped: {char_name}")
                
        # Sort index deterministically
        sorted_index = dict(sorted(index_data.items()))
        
        with open(os.path.join(out_dir, "index.json"), "w") as f:
            json.dump(sorted_index, f, indent=4)
            
        with open(os.path.join(out_dir, "index.txt"), "w") as f:
            for name, info in sorted_index.items():
                h = info['height']
                gy = info['ground_y']
                ox = info['origin_x']
                w = info['width']
                hy = info['head_y']
                f.write(f"{name},{h},{gy},{ox},{w},{hy}\n")
                
        elapsed = time.time() - start_time
        print(f"\nSuccessfully exported {success_count}/{len(chars)} characters for H={TARGET_HEIGHT} in {elapsed:.1f}s using {workers} parallel workers.")

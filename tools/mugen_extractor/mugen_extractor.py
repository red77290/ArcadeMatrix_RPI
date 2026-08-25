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

def get_def_palette(cdir):
    def_files = glob.glob(os.path.join(cdir, "*.def"))
    if not def_files: return None
    with open(def_files[0], 'r', errors='ignore') as f:
        for line in f:
            if line.strip().lower().startswith('pal1'):
                if '=' in line:
                    path = line.split('=')[-1].strip().split(';')[0].strip()
                    return find_file_case_insensitive(cdir, path)
    return None

class SFFv1Parser:
    def __init__(self, filepath):
        self.filepath = filepath
        self.images = {}
        self.first_palette = None
        self.parse()

    def parse(self):
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
                    if not self.first_palette and len(pcx_data) > 768:
                        self.first_palette = pcx_data[-768:]
                    self.images[(grp, img)] = {
                        'x': x, 'y': y,
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

def process_character(char_dir, out_dir):
    char_name = os.path.basename(char_dir)
    sff_files = glob.glob(os.path.join(char_dir, "*.sff"))
    air_files = glob.glob(os.path.join(char_dir, "*.air"))
    if not sff_files or not air_files: return False

    # We explicitly ignore .act files to fix the BGR vs RGB palette inversion issue.
    # The script will rely entirely on the main palette embedded within the .sff file.
    master_palette = None

    sff = SFFv1Parser(sff_files[0])
    air = AirParser(air_files[0])

    if not master_palette:
        master_palette = sff.first_palette
        if not master_palette: return False

    required_anims = {
        0: 'stand',
        20: 'walk'
    }
    attack_anims = [200, 210, 220, 230, 240, 250, 400, 410, 420]
    for act_id in attack_anims:
        if act_id in air.animations:
            required_anims[act_id] = 'attack'
            break
            
    hit_anims = [5000, 5001, 5002, 5010, 5011, 5012]
    for act_id in hit_anims:
        if act_id in air.animations:
            required_anims[act_id] = 'hit'
            break
            
    # Win (180-185 are win poses, 190-195 are taunts; do NOT match 170 which is lose/defeat pose)
    win_anims = [180, 181, 182, 183, 184, 185, 190, 191, 195]
    for act_id in win_anims:
        if act_id in air.animations:
            required_anims[act_id] = 'win'
            break

    special_count = 0
    for act_id in range(1000, 3000):
        if act_id in air.animations and len(air.animations[act_id]) > 3:
            special_count += 1
            required_anims[act_id] = f'special{special_count}'
            if special_count >= 3: break

    super_count = 0
    for act_id in range(3000, 5000):
        if act_id in air.animations and len(air.animations[act_id]) > 3:
            super_count += 1
            required_anims[act_id] = f'super{super_count}'
            if super_count >= 3: break

    for act_id in [5030, 5050]:
        if act_id in air.animations:
            required_anims[act_id] = 'fall'
            break

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
                
                valid_frames.append({'img': img_obj, 'ox': ox, 'oy': oy, 'delay': f['delay'], 'min_y': min_y, 'max_y': max_y})

        if valid_frames:
            all_valid_frames[anim_name] = valid_frames
            if anim_name in ['stand', 'walk'] and walk_h is None:
                # Compute local max_y and min_y to determine the base scale
                l_min = min([fr['min_y'] for fr in valid_frames])
                l_max = max([fr['max_y'] for fr in valid_frames])
                walk_h = l_max - l_min
                stand_head_y_local = l_min

    if not all_valid_frames: return False
    
    # Ensure all mandatory animations are present
    for req in ['stand', 'walk', 'attack', 'hit', 'win']:
        if req not in all_valid_frames:
            logging.warning(f"Character {char_name} missing mandatory animation '{req}'. Skipping.")
            return False

    if walk_h is None or walk_h <= 0: walk_h = global_max_y - global_min_y
    if walk_h <= 0: walk_h = TARGET_HEIGHT
    
    # Cap global bounds to prevent massive RAM usage
    global_min_x = max(-500, global_min_x)
    global_min_y = max(-500, global_min_y)
    global_max_x = min(500, global_max_x)
    global_max_y = min(500, global_max_y)
    
    orig_w = global_max_x - global_min_x
    orig_h = global_max_y - global_min_y
    
    if orig_w <= 0 or orig_h <= 0: return False
    
    EXTRACT_MODE = globals().get('EXTRACT_MODE', 'FULLSIZE')
    CUSTOM_SCALE = globals().get('CUSTOM_SCALE', None)
    
    if CUSTOM_SCALE is not None and float(CUSTOM_SCALE) > 0:
        scale = float(CUSTOM_SCALE)
    elif EXTRACT_MODE == 'SCALED':
        scale = 1.0
        if walk_h > TARGET_HEIGHT:
            scale = TARGET_HEIGHT / walk_h
    else:
        # FULLSIZE Mode
        scale = 1.0
        if TARGET_HEIGHT == 32:
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
        COMPRESS = globals().get('COMPRESS_FGT', False)
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
                # Paste coordinates so that the character's axis aligns with global origin
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
                            idx = indices[py * img_obj.width + px]
                            if idx != trans_idx:
                                if isinstance(idx, tuple):
                                    rgba_pixels[cx, cy] = (idx[0], idx[1], idx[2], 255)
                                else:
                                    if idx * 3 + 2 < len(master_palette):
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
                        
    return {
        'height': canvas_h,
        'ground_y': ground_y,
        'head_y': head_y,
        'origin_x': origin_x,
        'width': canvas_w,
        'has_special': special_count > 0,
        'has_super': super_count > 0,
        'special_count': special_count,
        'super_count': super_count
    }

TARGET_HEIGHT = 32

if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="Extract Mugen Characters for ArcadeMatrix")
    # -i/-o are short aliases for --src/--dest, kept for compatibility with the interactive
    # start_extractor.sh/.bat wrappers (which prompt the user and pass -i/-o).
    parser.add_argument("--src", "-i", dest="src", type=str, required=True, help="Source directory containing Mugen characters")
    parser.add_argument("--dest", "-o", dest="dest", type=str, default="fighters_32", help="Output directory for the generated .fgt files and index (default: ./fighters_32)")
    parser.add_argument("--mode", type=str, choices=['SCALED', 'FULLSIZE'], default='FULLSIZE', 
                        help="SCALED: Resize character to perfectly fit screen height (for standard ESP32). FULLSIZE: Extract at 1:1 original scale (for RPi or ESP32-S3 with PSRAM).")
    parser.add_argument("--scale", "--scaling", dest="scale", type=float, default=None,
                        help="Custom scaling factor (e.g. 0.5 for 50%%, 0.8, 2.0). Overrides default SCALED/FULLSIZE mode calculations when specified.")
    parser.add_argument("--compress", action="store_true", help="Compress the output .fgt files using gzip (.fgt.gz). Ideal for RPi to save space.")
    args = parser.parse_args()

    # Set global options so process_character can see them
    global EXTRACT_MODE
    EXTRACT_MODE = args.mode
    global CUSTOM_SCALE
    CUSTOM_SCALE = args.scale
    global COMPRESS_FGT
    COMPRESS_FGT = args.compress

    src_dir = args.src
    out_dirs = [
        (args.dest, TARGET_HEIGHT)
    ]
    
    start_time = time.time()
    chars = [os.path.join(src_dir, d) for d in os.listdir(src_dir) if os.path.isdir(os.path.join(src_dir, d))]
    
    print(f"Starting extraction in mode: {EXTRACT_MODE}")
    
    for out_dir, target_h in out_dirs:
        TARGET_HEIGHT = target_h
        os.makedirs(out_dir, exist_ok=True)
        print(f"\n--- Extracting for TARGET_HEIGHT={TARGET_HEIGHT} ---")
        
        success_count = 0
        index_data = {}
        
        for i, char_dir in enumerate(chars):
            result = process_character(char_dir, out_dir)
            if result:
                success_count += 1
                char_name = os.path.basename(char_dir)
                index_data[char_name] = result
                print(f"Processing: {char_name} (H: {result['height']}, Ground: {result['ground_y']})")
                
        with open(os.path.join(out_dir, "index.json"), "w") as f:
            json.dump(index_data, f, indent=4)
            
        with open(os.path.join(out_dir, "index.txt"), "w") as f:
            for name, info in index_data.items():
                f.write(f"{name},{info['height']},{info['ground_y']},{info['origin_x']},{info['width']},{info['head_y']}\n")
                
        print(f"Successfully exported {success_count} characters for H={TARGET_HEIGHT} in {time.time() - start_time:.1f}s.")

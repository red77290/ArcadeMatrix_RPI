import os
import glob
import struct
import io
import time
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

    act_file = get_def_palette(char_dir)
    master_palette = None
    if act_file:
        with open(act_file, 'rb') as f:
            master_palette = f.read(768)

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
            
    win_anims = [180, 181, 182, 183]
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

    if not all_valid_frames: return False
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
    
    scale = 1.0
    if walk_h > TARGET_HEIGHT:
        scale = TARGET_HEIGHT / walk_h
        
    canvas_w = max(1, int(orig_w * scale))
    canvas_h = max(1, int(orig_h * scale))
    ground_y = int(-global_min_y * scale)
    origin_x = int(-global_min_x * scale)

    # Pass 2: Render and save frames
    char_out_dir = os.path.join(out_dir, char_name)
    os.makedirs(char_out_dir, exist_ok=True)
    
    for anim_name, valid_frames in all_valid_frames.items():
        out_file = os.path.join(char_out_dir, f"{anim_name}.fgt")
        with open(out_file, 'wb') as f:
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
                                        
                if scale < 1.0:
                    rgba_canvas = rgba_canvas.resize((canvas_w, canvas_h), Image.Resampling.BILINEAR)
                
                resized_data = rgba_canvas.getdata()
                for r, g, b, a in resized_data:
                    if a < 128:
                        f.write(struct.pack('<H', TRANSPARENT_COLOR_565))
                    else:
                        c565 = rgb888_to_rgb565(r, g, b)
                        if c565 == TRANSPARENT_COLOR_565: c565 = 0x0001
                        f.write(struct.pack('<H', c565))
                        
    return {
        'height': canvas_h,
        'ground_y': ground_y,
        'origin_x': origin_x,
        'width': canvas_w,
        'has_special': special_count > 0,
        'has_super': super_count > 0,
        'special_count': special_count,
        'super_count': super_count
    }

TARGET_HEIGHT = 32

if __name__ == "__main__":
    src_dir = "/Users/red1l/Downloads/Mercury Mugen Roster 1.0  with over 1000+ Chars/chars"
    out_dirs = [
        ("/Users/red1l/Documents/work/git/perso/RetroPixelLED/ArcadeMatrix/scrap/fighters_32", 32),
        ("/Users/red1l/Documents/work/git/perso/RetroPixelLED/ArcadeMatrix/scrap/fighters_64", 64)
    ]
    
    start_time = time.time()
    chars = [os.path.join(src_dir, d) for d in os.listdir(src_dir) if os.path.isdir(os.path.join(src_dir, d))]
    
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
                f.write(f"{name},{info['height']},{info['ground_y']},{info['origin_x']},{info['width']}\n")
                
        print(f"Successfully exported {success_count} characters for H={TARGET_HEIGHT} in {time.time() - start_time:.1f}s.")

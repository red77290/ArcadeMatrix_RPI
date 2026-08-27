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

class DefParser:
    """
    Parser for MUGEN character .def definition files.
    Extracts the official .sff sprite file, .air animation file, .cns/.st state files,
    pal.defaults, and all pal1..pal12 color palettes.
    """
    def __init__(self, char_dir):
        self.char_dir = char_dir
        self.name = os.path.basename(char_dir)
        self.display_name = ""
        self.author = ""
        self.pal_defaults = []
        self.sff_file = None
        self.air_file = None
        self.cns_files = []
        self.palettes = {}  # slot_int (1..12) -> absolute_path
        self.def_path = None
        self.parse()

    def parse(self):
        char_name = os.path.basename(self.char_dir)
        candidate_defs = glob.glob(os.path.join(self.char_dir, "*.def")) + glob.glob(os.path.join(self.char_dir, "*.DEF"))
        if not candidate_defs:
            return

        def score_def(p):
            base = os.path.splitext(os.path.basename(p))[0].lower()
            if base == char_name.lower(): return 0
            if any(k in base for k in ['intro', 'select', 'menu', 'opt', 'cursor', 'stage']): return 99
            return 10

        candidate_defs.sort(key=score_def)
        self.def_path = candidate_defs[0]

        try:
            with open(self.def_path, 'r', errors='ignore') as f:
                lines = f.readlines()
        except:
            return

        current_section = None
        for line in lines:
            line = line.split(';')[0].strip()
            if not line: continue
            
            if line.startswith('[') and line.endswith(']'):
                current_section = line[1:-1].strip().lower()
                continue

            if '=' in line:
                k, v = [x.strip() for x in line.split('=', 1)]
                k = k.lower()
                v = v.split(';')[0].strip().replace('"', '').replace("'", "")
                
                if current_section == 'info':
                    if k == 'name': self.name = v
                    elif k == 'displayname': self.display_name = v
                    elif k == 'author': self.author = v
                    elif k == 'pal.defaults':
                        try:
                            self.pal_defaults = [int(p.strip()) for p in v.split(',') if p.strip().isdigit()]
                        except: pass
                elif current_section == 'files':
                    if k in ('sprite', 'sff'):
                        p = find_file_case_insensitive(self.char_dir, v)
                        if p and os.path.isfile(p): self.sff_file = p
                    elif k in ('anim', 'air'):
                        p = find_file_case_insensitive(self.char_dir, v)
                        if p and os.path.isfile(p): self.air_file = p
                    elif k == 'cns' or k.startswith('st'):
                        p = find_file_case_insensitive(self.char_dir, v)
                        if p and os.path.isfile(p) and p not in self.cns_files:
                            self.cns_files.append(p)
                    elif k.startswith('pal'):
                        slot_str = k[3:].strip()
                        if slot_str.isdigit():
                            p = find_file_case_insensitive(self.char_dir, v)
                            if p and os.path.isfile(p):
                                self.palettes[int(slot_str)] = p

class CnsParser:
    """
    Parser for MUGEN .cns and .st state files.
    Extracts physical dimensions ([Size] head.pos, xscale, yscale)
    and state animation mappings ([Statedef <id>] anim = ... and ChangeAnim).
    """
    def __init__(self, cns_paths):
        self.cns_paths = cns_paths if isinstance(cns_paths, list) else [cns_paths]
        self.xscale = 1.0
        self.yscale = 1.0
        self.head_pos = None  # (x, y) e.g. (-5, -90)
        self.mid_pos = None
        self.height = None
        self.state_anims = {}   # state_id -> list of primary character anim_ids
        self.state_explods = {} # state_id -> list of explod/FX anim_ids
        self.parse()

    def parse(self):
        for path in self.cns_paths:
            if not path or not os.path.isfile(path):
                continue
            try:
                with open(path, 'r', errors='ignore') as f:
                    lines = f.readlines()
            except:
                continue

            current_section = ""
            current_state = None
            current_state_type = None

            for line in lines:
                line = line.split(';')[0].strip()
                if not line:
                    continue

                if line.startswith('[') and line.endswith(']'):
                    sec = line[1:-1].strip()
                    current_section = sec.lower()
                    
                    if current_section.startswith('statedef'):
                        parts = current_section.split()
                        if len(parts) >= 2:
                            num_str = parts[1].split(',')[0].strip()
                            if num_str.lstrip('-').isdigit():
                                current_state = int(num_str)
                                current_state_type = 'statedef'
                                if current_state not in self.state_anims:
                                    self.state_anims[current_state] = []
                            else:
                                current_state = None
                        else:
                            current_state = None
                    elif current_section.startswith('state '):
                        current_state_type = 'state'
                    else:
                        current_state = None
                        current_state_type = None
                    continue

                if '=' in line:
                    k, v = [x.strip() for x in line.split('=', 1)]
                    k = k.lower()
                    v = v.split(';')[0].strip().replace('"', '').replace("'", "")

                    if current_section == 'size':
                        if k == 'xscale':
                            try: self.xscale = float(v)
                            except: pass
                        elif k == 'yscale':
                            try: self.yscale = float(v)
                            except: pass
                        elif k == 'height':
                            try: self.height = int(float(v))
                            except: pass
                        elif k == 'head.pos':
                            try:
                                coords = [int(float(c.strip())) for c in v.split(',') if c.strip().lstrip('-').isdigit()]
                                if len(coords) >= 2:
                                    self.head_pos = (coords[0], coords[1])
                            except: pass
                        elif k == 'mid.pos':
                            try:
                                coords = [int(float(c.strip())) for c in v.split(',') if c.strip().lstrip('-').isdigit()]
                                if len(coords) >= 2:
                                    self.mid_pos = (coords[0], coords[1])
                            except: pass

                    elif current_state is not None:
                        if k == 'type':
                            current_state_type = v.lower()
                        elif k == 'anim':
                            if v.lstrip('-').isdigit():
                                anim_id = int(v)
                                if current_state_type in ('explod', 'helper'):
                                    if current_state not in self.state_explods:
                                        self.state_explods[current_state] = []
                                    if anim_id not in self.state_explods[current_state]:
                                        self.state_explods[current_state].append(anim_id)
                                else:
                                    if anim_id not in self.state_anims[current_state]:
                                        self.state_anims[current_state].append(anim_id)
                        elif k == 'value' and current_state_type == 'changeanim':
                            if v.lstrip('-').isdigit():
                                anim_id = int(v)
                                if anim_id not in self.state_anims[current_state]:
                                    self.state_anims[current_state].append(anim_id)

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
                parts = [p.strip() for p in line.split(',')]
                if len(parts) >= 5:
                    try:
                        grp = int(parts[0])
                        img = int(parts[1])
                        air_ox = int(parts[2]) if parts[2] else 0
                        air_oy = int(parts[3]) if parts[3] else 0
                        delay = int(parts[4])
                        if delay == -1: delay = 10
                        
                        flip = parts[5].upper() if len(parts) > 5 and parts[5] else ""
                        blend = parts[6].upper() if len(parts) > 6 and parts[6] else ""
                        
                        if delay > 0:
                            self.animations[current_action].append({
                                'grp': grp,
                                'img': img,
                                'ox': air_ox,
                                'oy': air_oy,
                                'delay': delay,
                                'flip': flip,
                                'blend': blend
                            })
                    except: pass

def score_palette(pal_bytes, indices, trans_idx):
    """
    Score how well a 768-byte palette renders a sprite given its pixel indices.
    Returns a float score. Higher = better palette.
    """
    if not pal_bytes or len(pal_bytes) < 768:
        return -999.0
    
    used_indices = [idx for idx in indices if idx != trans_idx]
    if not used_indices:
        return -999.0
    
    colors = set()
    total_lum = 0.0
    neon_count = 0
    total = len(used_indices)
    
    for idx in used_indices:
        if idx * 3 + 2 < len(pal_bytes):
            r, g, b = pal_bytes[idx * 3], pal_bytes[idx * 3 + 1], pal_bytes[idx * 3 + 2]
            colors.add((r, g, b))
            total_lum += (0.299 * r + 0.587 * g + 0.114 * b)
            # Detect pure saturated primary/secondary masking colors
            if (r > 160 and g < 20 and b < 20) or (g > 160 and r < 20 and b < 20) or (b > 160 and r < 20 and b < 20):
                neon_count += 1
            elif (r > 160 and b > 160 and g < 20) or (r > 160 and g > 160 and b < 20) or (g > 160 and b > 160 and r < 20):
                neon_count += 1
                
    u_colors = len(colors)
    if u_colors <= 1 and len(set(used_indices)) > 1:
        return -999.0  # Solid monochrome / broken palette
        
    avg_lum = total_lum / max(total, 1)
    neon_ratio = neon_count / max(total, 1)
    if neon_ratio > 0.25:
        return -999.0  # Dummy rainbow/neon mask palette
        
    score = u_colors * 10.0
    if 25 <= avg_lum <= 200:
        score += 100.0
    elif avg_lum < 15:
        score -= 30.0
    elif avg_lum > 225:
        score -= 80.0
        
    return score

def resolve_master_palette(char_dir, sff, def_parser=None):
    """
    Resolve the best palette for a character using an intelligent scoring system.
    
    Candidates:
      1. Official .act palettes from .def (prioritizing pal.defaults)
      2. SFF embedded palettes from body sprites (0, 1, 5, 20, 21, 40, 100, 200, 5000)
      3. SFF stand_palette & first_palette
      4. Additional .act palettes in character folder
    """
    char_name = os.path.basename(char_dir)
    
    # Find the best body sprite to use as scoring reference (highest unique indices)
    best_indices = []
    best_key = None
    max_unique = 0
    
    for k, info in sff.images.items():
        if k[0] >= 9000:
            continue  # Skip portrait/select icons
        if k[0] in (0, 1, 5, 10, 20, 21, 40, 100, 200, 5000) or best_key is None:
            try:
                im = Image.open(io.BytesIO(info['data']))
                ind = list(im.get_flattened_data()) if hasattr(im, 'get_flattened_data') else list(im.getdata())
                u = len(set(ind) - {ind[0] if ind else 0})
                if u > max_unique:
                    max_unique = u
                    best_key = k
                    best_indices = ind
            except:
                pass
                
    if not best_indices and sff.images:
        for k, info in sff.images.items():
            try:
                im = Image.open(io.BytesIO(info['data']))
                best_indices = list(im.get_flattened_data()) if hasattr(im, 'get_flattened_data') else list(im.getdata())
                best_key = k
                break
            except:
                pass
                
    if not best_indices:
        return sff.first_palette or sff.stand_palette
        
    # If the sprite is direct truecolor (RGB/RGBA tuples), no palette mapping needed
    if best_indices and isinstance(best_indices[0], (tuple, list)):
        return sff.first_palette or sff.stand_palette or (b'\x00' * 768)
        
    trans_idx = best_indices[0] if best_indices else 0
    candidates = []
    seen = set()
    
    # 1. Official palettes from DEF file (prioritizing pal.defaults)
    if def_parser and def_parser.palettes:
        for slot in def_parser.pal_defaults:
            if slot in def_parser.palettes:
                p = def_parser.palettes[slot]
                try:
                    with open(p, 'rb') as f:
                        d = f.read(768)
                        if len(d) == 768 and d not in seen:
                            seen.add(d)
                            candidates.append((f"DEF(pal{slot}:default)", d, 50.0))
                except:
                    pass
        for slot in sorted(def_parser.palettes.keys()):
            p = def_parser.palettes[slot]
            try:
                with open(p, 'rb') as f:
                    d = f.read(768)
                    if len(d) == 768 and d not in seen:
                        seen.add(d)
                        candidates.append((f"DEF(pal{slot})", d, 30.0))
            except:
                pass

    # 2. SFF embedded palette in reference body sprite
    if best_key and best_key in sff.images and len(sff.images[best_key]['data']) > 768:
        d = sff.images[best_key]['data'][-768:]
        if d not in seen:
            seen.add(d)
            candidates.append((f"SFF({best_key[0]},{best_key[1]})", d, 40.0))

    if sff.stand_palette and sff.stand_palette not in seen:
        seen.add(sff.stand_palette)
        candidates.append(("SFF(stand)", sff.stand_palette, 35.0))

    if sff.first_palette and sff.first_palette not in seen:
        seen.add(sff.first_palette)
        candidates.append(("SFF(first)", sff.first_palette, 30.0))
        
    for (g, i), info in sff.images.items():
        if g < 9000 and len(info['data']) > 768:
            d = info['data'][-768:]
            if d not in seen:
                seen.add(d)
                candidates.append((f"SFF({g},{i})", d, 20.0))

    # 3. Any additional .act files in character folder
    for af in glob.glob(os.path.join(char_dir, "*.act")) + glob.glob(os.path.join(char_dir, "*.ACT")):
        try:
            with open(af, "rb") as f:
                d = f.read(768)
                if len(d) == 768 and d not in seen:
                    seen.add(d)
                    candidates.append((f"ACT({os.path.basename(af)})", d, 10.0))
        except:
            pass
            
    if not candidates:
        return sff.first_palette or sff.stand_palette
        
    best_cand = None
    best_score = -99999.0
    best_name = ""
    for name, pal, bonus in candidates:
        sc = score_palette(pal, best_indices, trans_idx)
        if sc > -500.0:  # Valid non-broken palette
            sc += bonus
            if sc > best_score:
                best_score = sc
                best_cand = pal
                best_name = name
                
    if best_cand:
        logging.info(f"{char_name}: palette={best_name} (score={best_score:.1f})")
        return best_cand
        
    return sff.first_palette or sff.stand_palette

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
    def_parser = DefParser(char_dir)

    # 1. Official SFF Sprite File
    sff_path = def_parser.sff_file
    if not sff_path or not os.path.isfile(sff_path):
        sff_files = glob.glob(os.path.join(char_dir, "*.sff")) + glob.glob(os.path.join(char_dir, "*.SFF"))
        if not sff_files: return False
        sff_path = sff_files[0]

    # 2. Official AIR Animation File
    air_path = def_parser.air_file
    if not air_path or not os.path.isfile(air_path):
        air_files = glob.glob(os.path.join(char_dir, "*.air")) + glob.glob(os.path.join(char_dir, "*.AIR"))
        if not air_files: return False
        air_path = air_files[0]

    # 3. Official CNS / ST State Files
    cns_paths = def_parser.cns_files
    if not cns_paths:
        cns_paths = glob.glob(os.path.join(char_dir, "*.cns")) + glob.glob(os.path.join(char_dir, "*.st"))
    cns = CnsParser(cns_paths)

    sff = SFFv1Parser(sff_path)
    air = AirParser(air_path)

    master_palette = resolve_master_palette(char_dir, sff, def_parser)
    if not master_palette:
        return False

    required_anims = {}
    
    # 1. Stand (Primary: CNS State 0 -> Heuristic: 0, 5, 10, 11, 20 -> First available)
    stand_anim_id = None
    for a_id in cns.state_anims.get(0, []):
        if a_id in air.animations and len(air.animations[a_id]) > 0:
            stand_anim_id = a_id
            break
    if stand_anim_id is None:
        for act_id in [0, 5, 10, 11, 20]:
            if act_id in air.animations and len(air.animations[act_id]) > 0:
                stand_anim_id = act_id
                break
    if stand_anim_id is None and air.animations:
        stand_anim_id = list(air.animations.keys())[0]
    if stand_anim_id is not None:
        required_anims[stand_anim_id] = 'stand'
    
    # 2. Walk (Primary: CNS State 20/21 -> Heuristic: 20, 21, 100, 105, 106, 110, 115, 120, 40)
    walk_anim_id = None
    for st_id in [20, 21]:
        for a_id in cns.state_anims.get(st_id, []):
            if a_id in air.animations and a_id not in required_anims and len(air.animations[a_id]) > 0:
                walk_anim_id = a_id
                break
        if walk_anim_id is not None: break
    if walk_anim_id is None:
        for act_id in [20, 21, 100, 105, 106, 110, 115, 120, 40]:
            if act_id in air.animations and act_id not in required_anims and len(air.animations[act_id]) > 0:
                walk_anim_id = act_id
                break
    if walk_anim_id is not None:
        required_anims[walk_anim_id] = 'walk'

    # 3. Attack (Primary: CNS States 200..999 -> Heuristic: range 200..999)
    attack_anim_id = None
    for st_id in range(200, 1000):
        for a_id in cns.state_anims.get(st_id, []):
            if a_id in air.animations and a_id not in required_anims and len(air.animations[a_id]) > 0:
                attack_anim_id = a_id
                break
        if attack_anim_id is not None: break
    if attack_anim_id is None:
        for act_id in range(200, 1000):
            if act_id in air.animations and act_id not in required_anims and len(air.animations[act_id]) > 0:
                attack_anim_id = act_id
                break
    if attack_anim_id is not None:
        required_anims[attack_anim_id] = 'attack'
            
    # 4. Hit (Primary: CNS States 5000, 5001, 5002, 5010, 5020 -> Heuristic: range 5000..5299)
    hit_anim_id = None
    for st_id in [5000, 5001, 5002, 5010, 5020]:
        for a_id in cns.state_anims.get(st_id, []):
            if a_id in air.animations and a_id not in required_anims and len(air.animations[a_id]) > 0:
                hit_anim_id = a_id
                break
        if hit_anim_id is not None: break
    if hit_anim_id is None:
        for act_id in range(5000, 5300):
            if act_id in air.animations and act_id not in required_anims and len(air.animations[act_id]) > 0:
                hit_anim_id = act_id
                break
    if hit_anim_id is not None:
        required_anims[hit_anim_id] = 'hit'
            
    # 5. Fall (Primary: CNS States 5030, 5050, 5070, 5080, 5100, 5110 -> Heuristic: 5030..5150)
    fall_anim_id = None
    for st_id in [5030, 5050, 5070, 5080, 5100, 5110, 5120, 5150]:
        for a_id in cns.state_anims.get(st_id, []):
            if a_id in air.animations and a_id not in required_anims and len(air.animations[a_id]) > 0:
                fall_anim_id = a_id
                break
        if fall_anim_id is not None: break
    if fall_anim_id is None:
        for act_id in [5030, 5050, 5070, 5080, 5100, 5110, 5120, 5150]:
            if act_id in air.animations and act_id not in required_anims and len(air.animations[act_id]) > 0:
                fall_anim_id = act_id
                break
    if fall_anim_id is not None:
        required_anims[fall_anim_id] = 'fall'

    # 6. Win (Primary: CNS States 180..189, 190..199, 170..179 -> Heuristic: 180..199, 170..179)
    win_anim_id = None
    for st_id in list(range(180, 200)) + list(range(170, 180)):
        for a_id in cns.state_anims.get(st_id, []):
            if a_id in air.animations and a_id not in required_anims and len(air.animations[a_id]) > 0:
                win_anim_id = a_id
                break
        if win_anim_id is not None: break
    if win_anim_id is None:
        for act_id in list(range(180, 200)) + list(range(170, 180)):
            if act_id in air.animations and act_id not in required_anims and len(air.animations[act_id]) > 0:
                win_anim_id = act_id
                break
    if win_anim_id is not None:
        required_anims[win_anim_id] = 'win'

    # 7. Specials (Primary: CNS States 1000..2999 -> Heuristic: 1000..2999)
    special_count = 0
    for st_id in range(1000, 3000):
        for a_id in cns.state_anims.get(st_id, []):
            if a_id in air.animations and a_id not in required_anims and len(air.animations[a_id]) > 0:
                special_count += 1
                required_anims[a_id] = f'special{special_count}'
                if special_count >= 3: break
        if special_count >= 3: break
    if special_count < 3:
        for act_id in range(1000, 3000):
            if act_id in air.animations and act_id not in required_anims and len(air.animations[act_id]) > 0:
                special_count += 1
                required_anims[act_id] = f'special{special_count}'
                if special_count >= 3: break

    # 8. Supers (Primary: CNS States 3000..4999 -> Heuristic: 3000..4999)
    super_count = 0
    for st_id in range(3000, 5000):
        for a_id in cns.state_anims.get(st_id, []):
            if a_id in air.animations and a_id not in required_anims and len(air.animations[a_id]) > 0:
                super_count += 1
                required_anims[a_id] = f'super{super_count}'
                if super_count >= 3: break
        if super_count >= 3: break
    if super_count < 3:
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
                    
                    # Apply flip if specified in .air
                    flip = f.get('flip', '')
                    if 'H' in flip and 'V' in flip:
                        img_obj = img_obj.transpose(Image.Transpose.ROTATE_180)
                    elif 'H' in flip:
                        img_obj = img_obj.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
                    elif 'V' in flip:
                        img_obj = img_obj.transpose(Image.Transpose.FLIP_TOP_BOTTOM)

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

import time
import os
import random
import struct
import logging
from PIL import Image

class FighterEngine:
    def __init__(self, config):
        self.config = config
        # Match ESP: use correct directory for matrix height
        if self.config.matrix_height >= 64:
            self.primary_dir = "fighters_64"
            self.fallback_dir = "fighters_32"
        else:
            self.primary_dir = "fighters_32"
            self.fallback_dir = "fighters_64"
        
        self._ensure_dir()
        self.active = False
        self.fights_done = 0
        self.p1 = self._init_player()
        self.p2 = self._init_player()
        self.last_move = 0
        self.fight_end = 0
        logging.info(f"FighterEngine: matrix={self.config.matrix_width}x{self.config.matrix_height}, primary={self.primary_dir}")

    def _init_player(self):
        return {'name': None, 'state': 'walk', 'x': 0, 'y': 0, 'dir': 1, 
                'frame': 0, 'last_f': 0, 'anims': {}, 'dead': False, 'src_dir': None, 
                'has_special': False, 'has_super': False, 'width': 32, 'origin_x': 16}

    def _ensure_dir(self):
        for d in [self.primary_dir, self.fallback_dir]:
            if not os.path.exists(d):
                os.makedirs(d)

    def reset(self):
        self.fights_done = 0
        self.active = False

    def _load_fgt(self, filepath):
        """Load a .fgt file and return (frames, delays) or (None, None)."""
        if not os.path.exists(filepath): 
            return None, None
        FORCE_SWAP_RB = False # Set to False since the folder has mixed encodings
        try:
            with open(filepath, 'rb') as f:
                magic = f.read(4)
                if magic != b'FGT': 
                    logging.warning(f"FGT bad magic in {filepath}: {magic}")
                    return None, None
                
                logging.debug(f"Decoding FGT with NEW bytearray decoder: {filepath}")
                
                w, h, count, trans = struct.unpack('<HHHH', f.read(8))
                delays = list(struct.unpack(f'<{count}H', f.read(count * 2)))
                frames = []
                
                for _ in range(count):
                    frame_bytes = w * h * 2
                    data = f.read(frame_bytes)
                    if not data or len(data) < frame_bytes:
                        break
                        
                    # Decode RGB565 LE -> RGBA using bytearray (much faster and memory safe)
                    rgba = bytearray(w * h * 4)
                    idx = 0
                    for pi in range(0, frame_bytes, 2):
                        c = data[pi] | (data[pi+1] << 8)
                        if c == trans:
                            idx += 4
                            continue
                            
                        r = (c >> 11) << 3
                        g = ((c >> 5) & 0x3F) << 2
                        b = (c & 0x1F) << 3
                        
                        if FORCE_SWAP_RB:
                            rgba[idx] = b
                            rgba[idx+1] = g
                            rgba[idx+2] = r
                        else:
                            rgba[idx] = r
                            rgba[idx+1] = g
                            rgba[idx+2] = b
                            
                        rgba[idx+3] = 255
                        idx += 4
                    
                    img = Image.frombytes('RGBA', (w, h), bytes(rgba))
                    frames.append(img)
                    
                if not frames:
                    return None, None
                logging.info(f"FGT loaded {filepath}: {w}x{h}, {len(frames)} frames, trans=0x{trans:04X}")
                return frames, delays
        except Exception as e:
            logging.error(f"FGT decode err {filepath}: {e}")
            return None, None

    def _load_fighter(self, name, dir_path):
        """Load all animations for a fighter. Returns dict or None."""
        anims = {}
        # Mandatory
        for action in ['walk', 'attack', 'hit', 'win']:
            fpath = os.path.join(dir_path, name, f"{action}.fgt")
            frames, delays = self._load_fgt(fpath)
            if not frames: 
                logging.warning(f"Fighter {name}: missing {action}.fgt in {dir_path}")
                return None
            anims[action] = {'f': frames, 'd': delays}
            
        # Optional Specials/Supers/Fall
        for action in ['special1', 'special2', 'special3', 'super1', 'super2', 'super3', 'fall']:
            fpath = os.path.join(dir_path, name, f"{action}.fgt")
            frames, delays = self._load_fgt(fpath)
            if frames:
                anims[action] = {'f': frames, 'd': delays}
                
        return anims

    def _load_index(self):
        import json
        index_path = os.path.join(self.primary_dir, "index.json")
        if not os.path.exists(index_path): 
            return {}
        try:
            with open(index_path, "r") as f:
                return json.load(f)
        except: 
            return {}

    def _get_fighters_list(self):
        index_data = self._load_index()
        fighters = [(self.primary_dir, name, data['height'], data.get('has_special', False), data.get('has_super', False), data.get('ground_y', 0), data.get('origin_x', 0), data.get('width', 32)) for name, data in index_data.items()]
        return fighters

    def _start_fight(self):
        self.hit_stop_until = 0
        self.shake_frames = 0
        
        fighters = self._get_fighters_list()
        if len(fighters) < 2: 
            return
        
        c1 = random.choice(fighters)
        valid_opponents = [f for f in fighters if f != c1 and 0.8 * c1[2] <= f[2] <= 1.2 * c1[2]]
        
        if not valid_opponents:
            # Fallback if no matching size
            valid_opponents = [f for f in fighters if f != c1]
            if not valid_opponents: return
            
        c2 = random.choice(valid_opponents)
        
        logging.info(f"Fight: {c1[1]} (H:{c1[2]}) vs {c2[1]} (H:{c2[2]})")
        
        a1 = self._load_fighter(c1[1], c1[0])
        a2 = self._load_fighter(c2[1], c2[0])
        if not a1 or not a2: 
            return
        
        # Virtual Ground
        matrix_ground = self.config.matrix_height - 1
        y1 = matrix_ground - c1[5]
        y2 = matrix_ground - c2[5]
        
        self.p1 = self._init_player()
        self.p1['name'] = c1[1]
        self.p1['src_dir'] = c1[0]
        self.p1['anims'] = a1
        self.p1['dir'] = 1
        self.p1['x'] = -c1[7]
        self.p1['y'] = y1
        self.p1['has_special'] = c1[3]
        self.p1['has_super'] = c1[4]
        self.p1['width'] = c1[7]
        self.p1['origin_x'] = c1[6]
        
        self.p2 = self._init_player()
        self.p2['name'] = c2[1]
        self.p2['src_dir'] = c2[0]
        self.p2['anims'] = a2
        self.p2['dir'] = -1
        self.p2['x'] = self.config.matrix_width
        self.p2['y'] = y2
        self.p2['has_special'] = c2[3]
        self.p2['has_super'] = c2[4]
        self.p2['width'] = c2[7]
        self.p2['origin_x'] = c2[6]
        
        self.active = True
        self.last_move = time.time() * 1000
        self.fight_end = 0

    def _update_anim(self, p, now):
        state = p['state']
        anim = p['anims'].get(state)
        if not anim: return
        
        delay = anim['d'][min(p['frame'], len(anim['d']) - 1)]
        if delay < 30: delay = 30
        
        if now - p['last_f'] > delay:
            p['frame'] += 1
            p['last_f'] = now
            if p['frame'] >= len(anim['f']):
                if state == 'walk': 
                    p['frame'] = 0
                elif state.startswith('attack') or state.startswith('special') or state.startswith('super'): 
                    p['state'] = 'win'
                    p['frame'] = 0
                elif state == 'hit' or state == 'fall':
                    p['frame'] = len(anim['f']) - 1
                    p['dead'] = True
                elif state == 'win':
                    p['frame'] = len(anim['f']) - 1
        
        # Move forward during special/super
        if state.startswith('special') or state.startswith('super'):
            p['x'] += p['dir'] * 2

    def _draw_player(self, bg, p):
        if not self.active or not p['anims']: return
        state = p['state']
        anim = p['anims'].get(state)
        if not anim: return
        
        frame_idx = min(p['frame'], len(anim['f']) - 1)
        img = anim['f'][frame_idx]
        
        if p['dir'] == -1:
            img = img.transpose(Image.FLIP_LEFT_RIGHT)
        
        # Clip paste coordinates to avoid issues
        px, py = int(p['x']), int(p['y'])
        bg.paste(img, (px, py), img)

    def tick(self, bg_img):
        if self.config.idle_sprite_count <= 0: return bg_img
        
        now = time.time() * 1000
        if not self.active:
            if self.fight_end == 0 or now - self.fight_end > 2000:
                self._start_fight()
            return bg_img
            
        self._update_anim(self.p1, now)
        self._update_anim(self.p2, now)
        
        # Movement
        if self.p1['state'] == 'walk' and self.p2['state'] == 'walk':
            elapsed = now - self.last_move
            if elapsed >= 35:
                px_move = int(elapsed / 35)
                self.p1['x'] += px_move
                self.p2['x'] -= px_move
                self.last_move += px_move * 35
                
                p1_world_origin = self.p1['x'] + self.p1['origin_x']
                p2_world_origin = self.p2['x'] + (self.p2['width'] - self.p2['origin_x'])
                dist = p2_world_origin - p1_world_origin
                engage_dist = int(self.config.matrix_width * 0.4)
                
                if dist <= engage_dist:
                    import random
                    attacker = self.p1 if random.randint(0, 1) == 0 else self.p2
                    target = self.p2 if attacker == self.p1 else self.p1
                    
                    atk_state = 'attack'
                    tgt_state = 'hit'
                    
                    r = random.random()
                    is_heavy = False
                    
                    if attacker['has_super'] and r < 0.2:
                        choices = [k for k in attacker['anims'] if k.startswith('super')]
                        if choices:
                            atk_state = random.choice(choices)
                            tgt_state = 'fall' if 'fall' in target['anims'] else 'hit'
                            is_heavy = True
                    elif attacker['has_special'] and r < 0.5:
                        choices = [k for k in attacker['anims'] if k.startswith('special')]
                        if choices:
                            atk_state = random.choice(choices)
                            tgt_state = 'fall' if 'fall' in target['anims'] else 'hit'
                            is_heavy = True
                            
                    attacker['state'] = atk_state
                    target['state'] = tgt_state
                    
                    if is_heavy:
                        self.hit_stop_until = now + 150
                        self.shake_frames = 10
                    
                    self.p1['frame'] = 0
                    self.p2['frame'] = 0
                    self.p1['last_f'] = now
                    self.p2['last_f'] = now
                    
        if now < self.hit_stop_until:
            return bg_img
            
        if self.p1['state'] == 'fall': self.p1['x'] += self.p1['dir'] * -2
        if self.p2['state'] == 'fall': self.p2['x'] += self.p2['dir'] * -2
        if self.fight_end == 0 and (self.p1['dead'] or self.p2['dead']):
            self.fight_end = now
            
        if self.fight_end > 0 and now - self.fight_end > 2000:
            self.active = False
            self.fights_done += 1
        
        # Draw (loser behind, winner in front)
        import random
        offset_y = 0
        if self.shake_frames > 0:
            offset_y = random.randint(-2, 2)
            self.shake_frames -= 1
            
        bg_offset = bg_img.copy()
        
        if self.p1['state'].startswith('super') and self.p1['frame'] < 2:
            import PIL.ImageOps
            bg_offset = PIL.ImageOps.invert(bg_offset.convert('RGB')).convert('RGBA')
        elif self.p2['state'].startswith('super') and self.p2['frame'] < 2:
            import PIL.ImageOps
            bg_offset = PIL.ImageOps.invert(bg_offset.convert('RGB')).convert('RGBA')

        def _draw(p):
            if not self.active or not p['anims']: return
            state = p['state']
            anim = p['anims'].get(state)
            if not anim: return
            
            frame_idx = min(p['frame'], len(anim['f']) - 1)
            img = anim['f'][frame_idx]
            
            if p['dir'] == -1:
                img = img.transpose(Image.FLIP_LEFT_RIGHT)
            
            px, py = int(p['x']), int(p['y']) + offset_y
            bg_offset.paste(img, (px, py), img)

        if self.p1['dead'] or self.p1['state'] in ['hit', 'fall']:
            _draw(self.p1)
            _draw(self.p2)
        else:
            _draw(self.p2)
            _draw(self.p1)
            
        return bg_offset

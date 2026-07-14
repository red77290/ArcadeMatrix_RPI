from PIL import Image, ImageDraw, ImageFont
import random

class TetrisClock:
    def __init__(self, width, height):
        self.w = width
        self.h = height
        self.blocks = []
        self.last_time_str = ""
        self.colors = [
            (255, 0, 0), (0, 255, 0), (0, 0, 255), 
            (255, 255, 0), (255, 165, 0), (0, 255, 255), (255, 0, 255)
        ]
        self.block_size = 2 # 2x2 pixels per block for high res look
        
    def _build_targets(self, time_str, font, offset_x, offset_y):
        targets_by_char = []
        try:
            bbox = font.getbbox(time_str)
            tw = bbox[2] - bbox[0]
            th = bbox[3] - bbox[1]
        except:
            try:
                tw, th = font.getsize(time_str)
            except:
                tw, th = 30, 10
                
        start_x = (self.w - tw) // 2 + offset_x
        y = (self.h - th) // 2 + offset_y
        
        current_x = start_x
        for char in time_str:
            mask = Image.new('1', (self.w, self.h), color=0)
            draw = ImageDraw.Draw(mask)
            draw.text((current_x, y), char, font=font, fill=1)
            
            char_targets = []
            for py in range(0, self.h, self.block_size):
                for px in range(0, self.w, self.block_size):
                    if mask.getpixel((px, py)):
                        char_targets.append((px, py))
            targets_by_char.append(char_targets)
            
            try:
                cw = font.getlength(char)
            except:
                try:
                    cw, _ = font.getsize(char)
                except:
                    cw = 6
            current_x += int(cw)
            
        return targets_by_char

    def tick(self, img, time_str, font, offset_x, offset_y):
        draw = ImageDraw.Draw(img)
        
        if self.last_time_str != time_str:
            if len(self.last_time_str) != len(time_str) or not self.blocks:
                # Major change (or init): Drop old blocks out
                for b in self.blocks:
                    b['state'] = 'out'
                    b['dy'] = random.uniform(1.0, 3.0)
                    
                # Build new targets for all characters
                targets_by_char = self._build_targets(time_str, font, offset_x, offset_y)
                for char_idx, targets in enumerate(targets_by_char):
                    for tx, ty in targets:
                        self.blocks.append({
                            'char_index': char_idx,
                            'x': tx,
                            'y': ty - self.h - random.randint(0, 50),
                            'tx': tx,
                            'ty': ty,
                            'dy': random.uniform(2.0, 5.0),
                            'color': random.choice(self.colors),
                            'state': 'in'
                        })
            else:
                # Only update changed characters
                changed_indices = [i for i in range(len(time_str)) if time_str[i] != self.last_time_str[i]]
                if changed_indices:
                    # Drop blocks belonging to changed characters
                    for b in self.blocks:
                        if b.get('char_index', -1) in changed_indices and b['state'] in ['in', 'fixed']:
                            b['state'] = 'out'
                            b['dy'] = random.uniform(1.0, 3.0)
                            
                    # Build new targets and add blocks ONLY for changed characters
                    targets_by_char = self._build_targets(time_str, font, offset_x, offset_y)
                    for char_idx in changed_indices:
                        for tx, ty in targets_by_char[char_idx]:
                            self.blocks.append({
                                'char_index': char_idx,
                                'x': tx,
                                'y': ty - self.h - random.randint(0, 50),
                                'tx': tx,
                                'ty': ty,
                                'dy': random.uniform(2.0, 5.0),
                                'color': random.choice(self.colors),
                                'state': 'in'
                            })
                            
            self.last_time_str = time_str
            
        # Physics and Drawing
        new_blocks = []
        for b in self.blocks:
            if b['state'] == 'in':
                b['y'] += b['dy']
                if b['y'] >= b['ty']:
                    b['y'] = b['ty']
                    b['state'] = 'fixed'
                new_blocks.append(b)
            elif b['state'] == 'out':
                b['y'] += b['dy']
                b['dy'] += 0.5 # Gravity acceleration
                if b['y'] < self.h:
                    new_blocks.append(b)
            elif b['state'] == 'fixed':
                new_blocks.append(b)
                
            # Draw the block (with a slight 1px inner border effect by drawing a smaller rect inside)
            draw.rectangle([int(b['x']), int(b['y']), int(b['x']) + self.block_size - 1, int(b['y']) + self.block_size - 1], fill=b['color'])
            
        self.blocks = new_blocks
        return img

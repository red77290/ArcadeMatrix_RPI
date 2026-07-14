import time
import random
from datetime import datetime
from PIL import Image, ImageDraw, ImageFont
import logging
import os
from core.theme import load_font, draw_styled_text, get_theme_colors

class DateEngine:
    def __init__(self, matrix_wrapper, config, fighter_engine=None):
        self.mw = matrix_wrapper
        self.config = config
        self.fighter_engine = fighter_engine
        self.drops = []
        self._init_drops()

    def _init_drops(self):
        # For Classic Cyberpunk theme
        num_drops = max(15, self.config.matrix_width // 3)
        min_len = max(5, self.config.matrix_height // 6)
        max_len = max(10, self.config.matrix_height // 3 + 5)
        
        for _ in range(num_drops):
            self.drops.append({
                'x': random.randint(0, self.config.matrix_width - 1),
                'y': random.randint(-self.config.matrix_height, 0),
                'speed': random.randint(1, max(3, self.config.matrix_height // 10)),
                'length': random.randint(min_len, max_len)
            })
            
        # For True Matrix Katakana
        self.matrix_cols = [random.randint(-self.config.matrix_height, 0) for _ in range(0, self.config.matrix_width, 10)]
        self.matrix_img = None
        try:
            self.matrix_font = load_font('DotGothic16.ttf', 12)
        except:
            self.matrix_font = ImageFont.load_default()

    def _draw_cyberpunk_bg(self, img):
        draw = ImageDraw.Draw(img)
        min_len = max(5, self.config.matrix_height // 6)
        max_len = max(10, self.config.matrix_height // 3 + 5)
        
        for d in self.drops:
            d['y'] += d['speed']
            if d['y'] - d['length'] > self.config.matrix_height:
                d['x'] = random.randint(0, self.config.matrix_width - 1)
                d['y'] = random.randint(-20, 0)
                d['speed'] = random.randint(1, max(3, self.config.matrix_height // 10))
                d['length'] = random.randint(min_len, max_len)
            
            for j in range(d['length']):
                py = d['y'] - j
                if 0 <= py < self.config.matrix_height:
                    if j == 0:
                        draw.point((d['x'], py), fill=(255, 255, 255))
                    else:
                        g = max(0, 255 - (j * (255 // d['length'])))
                        draw.point((d['x'], py), fill=(0, g, 0))

    def _draw_true_matrix_bg(self, img):
        if self.matrix_img is None:
            self.matrix_img = Image.new('RGBA', img.size, (0,0,0,255))
            
        # Fade existing image by pasting a semi-transparent black rectangle
        overlay = Image.new('RGBA', img.size, (0, 0, 0, 40))
        self.matrix_img = Image.alpha_composite(self.matrix_img, overlay)
        draw = ImageDraw.Draw(self.matrix_img)
        
        for i, y in enumerate(self.matrix_cols):
            if y >= 0 and y < self.config.matrix_height:
                char = chr(random.randint(0x30A0, 0x30FF))
                draw.text((i * 10, y), char, font=self.matrix_font, fill=(180, 255, 180, 255))
                # Draw a bright head occasionally
                if random.random() < 0.2:
                     draw.text((i * 10, y), char, font=self.matrix_font, fill=(255, 255, 255, 255))
            
            # Advance column
            self.matrix_cols[i] += random.randint(8, 12)
            
            # Reset
            if self.matrix_cols[i] > self.config.matrix_height:
                if random.random() < 0.1: # Don't always reset immediately to create gaps
                    self.matrix_cols[i] = 0
                    
        img.paste(self.matrix_img.convert('RGB'), (0,0))

    def _get_date_string(self):
        now = datetime.now()
        fmt = self.config.date_format.replace("DD", "%d").replace("MM", "%m").replace("YYYY", "%Y")
        return now.strftime(fmt)

    def run(self, duration_sec):
        logging.info(f"Starting DateEngine for {duration_sec}s")
        start_time = time.time()
        
        canvas = self.mw.get_canvas()
        if not canvas:
            return
            
        # Size and scale logic
        is_bdf = self.config.date_font.lower().endswith('.bdf')
        if is_bdf:
            font_size = 16
            scale_factor = self.config.date_size
        else:
            font_size = self.config.date_size
            scale_factor = 1

        font = load_font(self.config.date_font, font_size)
            
        while time.time() - start_time < duration_sec:
            if getattr(self.config, 'reload_flag', False):
                break
            img = Image.new('RGB', (self.config.matrix_width, self.config.matrix_height), color=(0, 0, 0))
            draw = ImageDraw.Draw(img)
            
            if self.config.date_theme == 18:
                self._draw_cyberpunk_bg(img)
            elif self.config.date_theme == 21:
                self._draw_true_matrix_bg(img)
            
            date_str = self._get_date_string()
            
            try:
                bbox = font.getbbox(date_str)
                tw = (bbox[2] - bbox[0]) * scale_factor
                th = (bbox[3] - bbox[1]) * scale_factor
            except:
                tw, th = 30, 10
            
            x = (self.config.matrix_width - tw) // 2 + self.config.date_offset_x
            y = (self.config.matrix_height - th) // 2 + self.config.date_offset_y
            
            if self.config.date_theme == 19:
                if not hasattr(self, 'prev_digits'):
                    self.prev_digits = [""] * len(date_str)
                    
                time_chars = list(date_str)
                changed = [False] * len(time_chars)
                is_flipping = False
                
                for i in range(len(time_chars)):
                    if i < len(self.prev_digits) and time_chars[i] != self.prev_digits[i]:
                        changed[i] = True
                        is_flipping = True
                        
                if is_flipping:
                    for flip_frame in range(1, 9):
                        anim_img = Image.new('RGB', (self.config.matrix_width, self.config.matrix_height), color=(0, 0, 0))
                        anim_draw = ImageDraw.Draw(anim_img)
                        
                        panel_w = max(4, tw // len(time_chars) + 1)
                        panel_h = max(8, th + 4)
                        spacing = 2
                        total_w = (panel_w * len(time_chars)) + (spacing * (len(time_chars)-1))
                        start_x = (self.config.matrix_width - total_w) // 2 + self.config.date_offset_x
                        y_pos = (self.config.matrix_height - panel_h) // 2 + self.config.date_offset_y
                        
                        cx = start_x
                        for i, char in enumerate(time_chars):
                            if char in ':/.-':
                                anim_draw.rectangle([cx, y_pos + panel_h//3, cx + 1, y_pos + panel_h//3 + 1], fill=(255, 255, 255))
                                anim_draw.rectangle([cx, y_pos + 2*panel_h//3, cx + 1, y_pos + 2*panel_h//3 + 1], fill=(255, 255, 255))
                                cx += 2 + spacing
                                continue
                                
                            is_flipping_panel = changed[i]
                            if is_flipping_panel:
                                shrink = flip_frame
                                if shrink > 4: shrink = 8 - flip_frame
                                shrink_px = int((shrink / 4.0) * (panel_h / 2))
                                
                                top_y = y_pos + shrink_px
                                bottom_y = max(top_y, y_pos + panel_h - shrink_px - 1)
                                anim_draw.rectangle([cx, top_y, cx + panel_w - 1, bottom_y], fill=(255, 255, 255))
                                mid_y = y_pos + panel_h // 2
                                anim_draw.line([(cx, mid_y), (cx + panel_w - 1, mid_y)], fill=(0, 0, 0), width=1)
                            else:
                                anim_draw.rectangle([cx, y_pos, cx + panel_w - 1, y_pos + panel_h - 1], fill=(255, 255, 255))
                                draw_styled_text(anim_img, char, (cx + 1, y_pos + 1), font, 19, self.config.date_color_1, self.config.date_color_2, scale=scale_factor)
                                mid_y = y_pos + panel_h // 2
                                anim_draw.line([(cx, mid_y), (cx + panel_w - 1, mid_y)], fill=(0, 0, 0), width=1)
                                
                            cx += panel_w + spacing
                            
                        canvas.SetImage(anim_img)
                        canvas = self.mw.swap_canvas(canvas)
                        time.sleep(0.02)
                        
                self.prev_digits = time_chars.copy()

                # Final static frame
                img = Image.new('RGB', (self.config.matrix_width, self.config.matrix_height), color=(0, 0, 0))
                draw = ImageDraw.Draw(img)
                panel_w = max(4, tw // len(time_chars) + 1)
                panel_h = max(8, th + 4)
                spacing = 2
                total_w = (panel_w * len(time_chars)) + (spacing * (len(time_chars)-1))
                start_x = (self.config.matrix_width - total_w) // 2 + self.config.date_offset_x
                y_pos = (self.config.matrix_height - panel_h) // 2 + self.config.date_offset_y
                
                cx = start_x
                for i, char in enumerate(time_chars):
                    if char in ':/.-':
                        draw.rectangle([cx, y_pos + panel_h//3, cx + 1, y_pos + panel_h//3 + 1], fill=(255, 255, 255))
                        draw.rectangle([cx, y_pos + 2*panel_h//3, cx + 1, y_pos + 2*panel_h//3 + 1], fill=(255, 255, 255))
                        cx += 2 + spacing
                        continue
                        
                    draw.rectangle([cx, y_pos, cx + panel_w - 1, y_pos + panel_h - 1], fill=(255, 255, 255))
                    draw_styled_text(img, char, (cx + 1, y_pos + 1), font, 19, self.config.date_color_1, self.config.date_color_2, scale=scale_factor)
                    mid_y = y_pos + panel_h // 2
                    draw.line([(cx, mid_y), (cx + panel_w - 1, mid_y)], fill=(0, 0, 0), width=1)
                    cx += panel_w + spacing
            else:
                draw_styled_text(img, date_str, (x, y), font, self.config.date_theme, self.config.date_color_1, self.config.date_color_2, scale=scale_factor)
                
            if self.fighter_engine:
                img = self.fighter_engine.tick(img)
                
            canvas.SetImage(img)
            canvas = self.mw.swap_canvas(canvas)
            
            # Update faster if cyberpunk, matrix theme, or fighter engine is enabled
            fast_update = self.config.date_theme in [18, 21] or (self.fighter_engine and self.config.idle_sprite_count > 0)
            time.sleep(0.04 if fast_update else 1)

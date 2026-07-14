from PIL import Image, ImageDraw, ImageFont
import math
import time

class PacManClock:
    def __init__(self, width, height):
        self.w = width
        self.h = height
        self.last_minute = -1
        self.transitioning = False
        self.pac_x = -30
        self.anim_frame = 0
        self.old_time_str = ""
        self.new_time_str = ""
        self.ghost_colors = [(255, 0, 0), (255, 184, 255), (0, 255, 255), (255, 184, 82)]

    def draw_pacman(self, draw, x, y, radius, mouth_angle, facing_right=True):
        if facing_right:
            start = mouth_angle
            end = 360 - mouth_angle
        else:
            start = 180 + mouth_angle
            end = 180 - mouth_angle + 360
        
        draw.pieslice([x - radius, y - radius, x + radius, y + radius], start, end, fill=(255, 255, 0))

    def draw_ghost(self, draw, x, y, radius, color, tick_count):
        # Body
        draw.pieslice([x - radius, y - radius, x + radius, y + radius], 180, 360, fill=color)
        draw.rectangle([x - radius, y, x + radius, y + radius], fill=color)
        # Tentacles
        tentacle_offset = (tick_count // 2) % 2
        for i in range(3):
            tx = x - radius + i * (radius * 2 / 3)
            ty = y + radius
            if (i + tentacle_offset) % 2 == 0:
                draw.rectangle([tx, ty - 2, tx + radius*2/3, ty], fill=(0,0,0))
        # Eyes
        draw.ellipse([x - radius/2 - 1, y - 2, x - radius/2 + 1, y + 2], fill=(255, 255, 255))
        draw.ellipse([x + radius/2 - 1, y - 2, x + radius/2 + 1, y + 2], fill=(255, 255, 255))
        # Pupils
        draw.point((x - radius/2 + 1, y), fill=(0, 0, 255))
        draw.point((x + radius/2 + 1, y), fill=(0, 0, 255))

    def tick(self, img, time_str, font, color1, color2):
        draw = ImageDraw.Draw(img)
        self.anim_frame += 1

        parts = time_str.split(':')
        now_min = int(parts[1]) if len(parts) >= 2 and parts[1].isdigit() else 0

        if self.last_minute == -1:
            self.last_minute = now_min
            self.old_time_str = time_str
            self.new_time_str = time_str
        elif self.last_minute != now_min and not self.transitioning:
            self.transitioning = True
            self.new_time_str = time_str
            self.pac_x = -40

        try:
            bbox = draw.textbbox((0, 0), time_str, font=font)
            tw = bbox[2] - bbox[0]
            th = bbox[3] - bbox[1]
        except:
            tw, th = 30, 10

        tx = (self.w - tw) // 2
        ty = (self.h - th) // 2 - 2

        if not self.transitioning:
            # Normal static display with pulsing dots
            draw.text((tx, ty), self.new_time_str, font=font, fill=color1)
            # Draw some pellets randomly around
            for i in range(5):
                px = (math.sin(self.anim_frame * 0.1 + i) * self.w/2) + self.w/2
                py = (math.cos(self.anim_frame * 0.15 + i*2) * self.h/2) + self.h/2
                draw.point((int(px), int(py)), fill=(255, 183, 174))
        else:
            # Transition: Pacman runs across eating the old time, ghosts follow
            self.pac_x += 3
            
            # Mouth animation
            mouth_angle = int(abs(math.sin(self.anim_frame * 0.5)) * 45)
            
            # Draw old time to the right of Pac-Man, new time to the left
            if self.pac_x < self.w + 60:
                # Clip area for old time (only visible right of pacman)
                old_clip = (self.pac_x, 0, self.w, self.h)
                draw.text((tx, ty), self.old_time_str, font=font, fill=(100, 100, 100))
                # Cover the eaten part
                draw.rectangle([0, 0, self.pac_x, self.h], fill=(0,0,0))
                
                # Draw new time trailing far behind ghosts
                draw.text((tx, ty), self.new_time_str, font=font, fill=color1)
                # Cover new time ahead of the reveal wave
                reveal_x = self.pac_x - 50
                if reveal_x < self.w:
                    draw.rectangle([reveal_x, 0, self.w, self.h], fill=(0,0,0))

                # Draw Pacman
                self.draw_pacman(draw, int(self.pac_x), self.h // 2, 6, mouth_angle, True)
                
                # Draw Ghosts following
                for i, g_col in enumerate(self.ghost_colors):
                    gx = self.pac_x - 15 - (i * 12)
                    gy = self.h // 2 + math.sin(self.anim_frame * 0.2 + i) * 2
                    self.draw_ghost(draw, int(gx), int(gy), 5, g_col, self.anim_frame)
            else:
                self.transitioning = False
                self.last_minute = now_min
                self.old_time_str = self.new_time_str

        return img

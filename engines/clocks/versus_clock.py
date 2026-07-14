from PIL import Image, ImageDraw, ImageFont
import math

class VersusClock:
    def __init__(self, width, height):
        self.w = width
        self.h = height
        self.anim_frame = 0

    def tick(self, img, time_str, font, color1, color2):
        draw = ImageDraw.Draw(img)
        self.anim_frame += 1

        parts = time_str.split(':')
        if len(parts) >= 2:
            h_str = parts[0]
            m_str = parts[1]
        else:
            h_str = "00"
            m_str = "00"
            
        h_val = int(h_str) if h_str.isdigit() else 0
        m_val = int(m_str) if m_str.isdigit() else 0
        
        # Health bar max width
        bar_w = (self.w // 2) - 10
        
        # Calculate health percentages (Hour max 23, Min max 59)
        p1_hp = 1.0 - (h_val / 23.0) if h_val <= 23 else 0.5
        p2_hp = 1.0 - (m_val / 59.0) if m_val <= 59 else 0.5
        
        # P1 Health Bar (Left)
        p1_bar_len = int(bar_w * p1_hp)
        draw.rectangle([5, 2, 5 + bar_w, 6], fill=(50, 0, 0), outline=(200, 200, 200))
        if p1_bar_len > 0:
            c1 = (255, 255, 0) if p1_hp > 0.3 else (255, 0, 0)
            draw.rectangle([5 + (bar_w - p1_bar_len), 3, 5 + bar_w - 1, 5], fill=c1)
            
        # P2 Health Bar (Right)
        p2_bar_len = int(bar_w * p2_hp)
        draw.rectangle([self.w - 5 - bar_w, 2, self.w - 5, 6], fill=(50, 0, 0), outline=(200, 200, 200))
        if p2_bar_len > 0:
            c2 = (255, 255, 0) if p2_hp > 0.3 else (255, 0, 0)
            draw.rectangle([self.w - 5 - bar_w + 1, 3, self.w - 5 - bar_w + p2_bar_len, 5], fill=c2)
            
        # KO in middle
        if (self.anim_frame // 10) % 2 == 0:
            draw.text(((self.w // 2) - 5, 0), "KO", font=font, fill=(255, 0, 0))
            
        # Player names (P1, P2) or just draw the time really big in the center
        try:
            bbox = draw.textbbox((0, 0), time_str, font=font)
            tw = bbox[2] - bbox[0]
            th = bbox[3] - bbox[1]
        except:
            tw, th = 30, 10
            
        tx = (self.w - tw) // 2
        ty = (self.h - th) // 2 + 4
        
        # Draw background shadow for time
        draw.text((tx + 1, ty + 1), time_str, font=font, fill=(0, 0, 0))
        draw.text((tx, ty), time_str, font=font, fill=color1)
        
        # Fighter idle bounce simulation at bottom corners
        bounce1 = math.sin(self.anim_frame * 0.2) * 2
        bounce2 = math.cos(self.anim_frame * 0.2) * 2
        
        # Draw fake 8-bit fighter blobs
        draw.rectangle([10, self.h - 8 + bounce1, 16, self.h - 2 + bounce1], fill=color1)
        draw.rectangle([self.w - 16, self.h - 8 + bounce2, self.w - 10, self.h - 2 + bounce2], fill=color2)

        return img

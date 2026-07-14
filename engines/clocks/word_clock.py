from PIL import ImageDraw
from datetime import datetime

class WordClock:
    def __init__(self, width, height):
        self.w = width
        self.h = height
        
    def _number_to_french(self, n):
        nums = ["MINUIT", "UNE", "DEUX", "TROIS", "QUATRE", "CINQ", "SIX", "SEPT", "HUIT", "NEUF", "DIX", "ONZE", "MIDI"]
        if 0 <= n <= 12:
            return nums[n]
        return str(n)

    def tick(self, img, time_str, font, color1, color2):
        draw = ImageDraw.Draw(img)
        
        parts = time_str.split(':')
        if len(parts) >= 2:
            h = int(parts[0])
            m = int(parts[1])
        else:
            h, m = 0, 0
            
        # French literal time logic
        is_past_half = m > 32
        display_h = h % 24
        
        if is_past_half:
            display_h = (display_h + 1) % 24
            
        # 12h format for reading
        read_h = display_h % 12
        if display_h == 0:
            str_h = "MINUIT"
        elif display_h == 12:
            str_h = "MIDI"
        else:
            str_h = self._number_to_french(read_h) + (" HEURE" if read_h == 1 else " HEURES")
            
        # Minutes
        str_m = ""
        rounded_m = 5 * round(m / 5)
        
        if rounded_m == 0 or rounded_m == 60:
            str_m = "PILE"
        elif is_past_half:
            diff = 60 - rounded_m
            if diff == 15:
                str_m = "MOINS LE QUART"
            else:
                str_m = f"MOINS {diff}"
        else:
            if rounded_m == 15:
                str_m = "ET QUART"
            elif rounded_m == 30:
                str_m = "ET DEMIE"
            else:
                str_m = str(rounded_m)
                
        lines = [
            "IL EST",
            str_h,
            str_m
        ]
        
        total_h = 0
        line_heights = []
        for line in lines:
            try:
                bbox = draw.textbbox((0, 0), line, font=font)
                lh = bbox[3] - bbox[1]
            except:
                lh = 8
            line_heights.append(lh)
            total_h += lh + 2
            
        y = (self.h - total_h) // 2
        for i, line in enumerate(lines):
            try:
                bbox = draw.textbbox((0, 0), line, font=font)
                lw = bbox[2] - bbox[0]
            except:
                lw = len(line) * 5
                
            x = (self.w - lw) // 2
            c = color1 if i % 2 == 0 else color2
            draw.text((x, y), line, font=font, fill=c)
            y += line_heights[i] + 2
            
        return img

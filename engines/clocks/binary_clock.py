from PIL import ImageDraw

class BinaryClock:
    def __init__(self, width, height):
        self.w = width
        self.h = height
        
    def tick(self, img, time_str, font, color1, color2):
        draw = ImageDraw.Draw(img)
        
        parts = time_str.split(':')
        if len(parts) >= 3:
            h, m, s = parts[0], parts[1], parts[2]
        else:
            h, m, s = "00", "00", "00"
            
        # Ensure 2 digits
        h = h.zfill(2)
        m = m.zfill(2)
        s = s.zfill(2)
        
        digits = [int(h[0]), int(h[1]), int(m[0]), int(m[1]), int(s[0]), int(s[1])]
        max_bits = [2, 4, 3, 4, 3, 4] # Max bits needed for each column
        
        dot_radius = max(2, min(self.w // 20, self.h // 12))
        spacing_x = self.w // 8
        spacing_y = self.h // 6
        
        start_x = (self.w - (5 * spacing_x)) // 2
        start_y = self.h - (self.h // 6) # Bottom aligned
        
        for col, val in enumerate(digits):
            x = start_x + col * spacing_x
            if col in [2, 4]: # Add extra gap between H/M and M/S
                x += spacing_x // 2
                start_x += spacing_x // 2
                
            for bit in range(max_bits[col]):
                y = start_y - bit * spacing_y
                is_on = (val >> bit) & 1
                
                # Draw dot
                if is_on:
                    c = color1 if col < 2 else (color2 if col < 4 else (255,255,255))
                    draw.ellipse([x - dot_radius, y - dot_radius, x + dot_radius, y + dot_radius], fill=c)
                else:
                    c = (30, 30, 30) # Dimmed
                    draw.ellipse([x - dot_radius, y - dot_radius, x + dot_radius, y + dot_radius], outline=c, width=1)
                    
        return img

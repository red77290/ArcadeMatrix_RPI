import os
from PIL import Image, ImageDraw, ImageFont

def hex_to_rgb(hex_str):
    hex_str = hex_str.lstrip('#')
    return tuple(int(hex_str[i:i+2], 16) for i in (0, 2, 4))

def get_theme_colors(theme_id, color1_hex="#ffffff", color2_hex="#ffffff"):
    # Returns (textColor, shadowColor, isGradient, color2)
    # theme_id 20 = Custom Gradient
    if theme_id == 20:
        c1 = hex_to_rgb(color1_hex)
        c2 = hex_to_rgb(color2_hex)
        return c1, (0, 0, 0), c1 != c2, c2
    elif theme_id == 0: return (228, 0, 15), (255, 255, 255), False, None # Nintendo
    elif theme_id == 1: return (255, 215, 0), (0, 75, 175), False, None # Capcom
    elif theme_id == 2: return (0, 155, 219), (255, 255, 255), False, None # Taito
    elif theme_id == 3: return (0, 85, 170), (255, 255, 255), False, None # Sega
    elif theme_id == 4: return (138, 43, 226), (255, 255, 0), False, None # Cave
    elif theme_id == 5: return (255, 69, 0), (255, 255, 255), False, None # Konami
    elif theme_id == 6: return (30, 144, 255), (255, 215, 0), False, None # SNK
    elif theme_id == 7: return (0, 0, 139), (255, 255, 255), False, None # Technos
    elif theme_id == 8: return (50, 205, 50), (255, 215, 0), False, None # IGS
    elif theme_id == 9: return (255, 255, 0), (0, 0, 0), False, None # Hudson
    elif theme_id == 10: return (255, 0, 0), (0, 0, 0), False, None # Banpresto
    elif theme_id == 11: return (255, 0, 0), (255, 215, 0), False, None # Namco
    elif theme_id == 12: return (255, 255, 0), (255, 0, 0), False, None # Ryu
    elif theme_id == 13: return (255, 50, 50), (255, 255, 255), False, None # Mario
    elif theme_id == 14: return (255, 140, 0), (0, 100, 0), False, None # Marco
    elif theme_id == 15: return (0, 255, 255), (0, 0, 255), False, None # Megaman
    elif theme_id == 16: return (0, 255, 0), (255, 0, 255), False, None # Bub
    elif theme_id == 17: return (0, 255, 0), (0, 0, 0), False, None # Space InvaderClock (Negative Space)
    elif theme_id == 18: return (200, 255, 200), (0, 0, 0), False, None # Cyberpunk
    elif theme_id == 19: return (0, 0, 0), (0, 0, 0), False, None # Flip Clock (Negative Space)
    elif theme_id == 21: return (0, 255, 0), (0, 0, 0), False, None # True Matrix
    elif theme_id == 22: return (255, 255, 255), (0, 0, 0), False, None # Pong Clock
    elif theme_id == 23: return (255, 255, 255), (0, 0, 0), False, None # Tetris Clock
    elif theme_id == 24: return (255, 255, 0), (0, 0, 0), False, None # Pac-Man Clock
    elif theme_id == 25: return (255, 255, 255), (0, 0, 0), False, None # Word Clock
    elif theme_id == 26: return (0, 255, 255), (0, 0, 0), False, None # Binary Clock
    elif theme_id == 27: return (255, 255, 255), (0, 0, 0), False, None # Versus Clock
    else: return (255, 255, 255), (0, 0, 0), False, None # Default

def draw_styled_text(img, text, position, font, theme_id, c1_hex="#ffffff", c2_hex="#ffffff", scale=1):
    x, y = position
    textColor, shadowColor, is_gradient, gradient_c2 = get_theme_colors(theme_id, c1_hex, c2_hex)
    
    draw = ImageDraw.Draw(img)
    try:
        bbox = draw.textbbox((0, 0), text, font=font)
        left, top, right, bottom = bbox
        tw = int(right - left)
        th = int(bottom - top)
    except AttributeError:
        try:
            tw, th = font.getsize(text)
            left, top = 0, 0
        except Exception:
            tw, th = 30, 10
            left, top = 0, 0
            
    if tw <= 0 or th <= 0: return
    
    padding = 2
    mask_w = tw + padding * 2
    mask_h = th + padding * 2
    
    # 1. Create a 1-bit crisp mask
    mask = Image.new("1", (mask_w, mask_h), 0)
    mask_draw = ImageDraw.Draw(mask)
    # Shift by -left, -top so the ink starts exactly at (padding, padding)
    mask_draw.text((padding - left, padding - top), text, font=font, fill=1)
    
    # 2. Scale blockily if needed
    if scale > 1:
        new_size = (mask_w * scale, mask_h * scale)
        try:
            resample = Image.Resampling.NEAREST
        except AttributeError:
            resample = Image.NEAREST
        mask = mask.resize(new_size, resample)
        mask_w, mask_h = new_size

    # 3. Create Colored Text Image
    text_img = Image.new("RGBA", (mask_w, mask_h), (0,0,0,0))
    if is_gradient and gradient_c2 is not None:
        gradient = Image.new('RGBA', (mask_w, mask_h), (0,0,0,0))
        for i in range(mask_h):
            r = int(textColor[0] + (gradient_c2[0] - textColor[0]) * (i / mask_h))
            g = int(textColor[1] + (gradient_c2[1] - textColor[1]) * (i / mask_h))
            b = int(textColor[2] + (gradient_c2[2] - textColor[2]) * (i / mask_h))
            ImageDraw.Draw(gradient).line([(0, i), (mask_w, i)], fill=(r,g,b,255))
        text_img.paste(gradient, mask=mask)
    else:
        solid = Image.new('RGBA', (mask_w, mask_h), textColor + (255,))
        text_img.paste(solid, mask=mask)

    # 4. Draw Shadows/Outlines
    shadow_img = Image.new('RGBA', (mask_w, mask_h), shadowColor + (255,))
    black_img = Image.new('RGBA', (mask_w, mask_h), (0, 0, 0, 255))
    offset = max(1, scale)
    
    paste_x = int(x + left * scale - padding * scale)
    paste_y = int(y + top * scale - padding * scale)
    
    if theme_id >= 4 and theme_id <= 17:
        # Arcade 3D Outline Effect (ESP32 style)
        # Thick colored drop shadow (always 2px / 1px physically)
        img.paste(shadow_img, (paste_x + 2, paste_y + 2), mask=mask)
        img.paste(shadow_img, (paste_x + 1, paste_y + 2), mask=mask)
        img.paste(shadow_img, (paste_x + 2, paste_y + 1), mask=mask)
        # Black outline around the text
        img.paste(black_img, (paste_x - 1, paste_y), mask=mask)
        img.paste(black_img, (paste_x + 1, paste_y), mask=mask)
        img.paste(black_img, (paste_x, paste_y - 1), mask=mask)
        img.paste(black_img, (paste_x, paste_y + 1), mask=mask)
    elif theme_id in [0, 1, 3]: # Normal Outline themes
        img.paste(shadow_img, (paste_x + offset, paste_y), mask=mask)
        img.paste(shadow_img, (paste_x - offset, paste_y), mask=mask)
        img.paste(shadow_img, (paste_x, paste_y + offset), mask=mask)
        img.paste(shadow_img, (paste_x, paste_y - offset), mask=mask)
    elif theme_id != 19: # 19 is Flip Clock (no shadow)
        img.paste(shadow_img, (paste_x + offset, paste_y + offset), mask=mask)
        
    # 5. Paste the final crisp text
    img.paste(text_img, (paste_x, paste_y), mask=mask)

def load_font(font_name, size):
    font_path = os.path.join("fonts", font_name)
    try:
        if font_name.lower().endswith('.bdf'):
            pil_path = font_path[:-4] + ".pil"
            if not os.path.exists(pil_path):
                from PIL import BdfFontFile
                with open(font_path, "rb") as f:
                    p = BdfFontFile.BdfFontFile(f)
                    p.save(pil_path)
            return ImageFont.load(pil_path)
        return ImageFont.truetype(font_path, size)
    except Exception as e:
        import logging
        logging.error(f"Failed to load font {font_path}: {e}")
        return ImageFont.load_default()

import time
import socket
import logging
import os
from PIL import Image, ImageDraw, ImageFont

class NetworkEngine:
    def __init__(self, matrix_wrapper, config, fighter_engine=None):
        self.mw = matrix_wrapper
        self.config = config
        self.fighter_engine = fighter_engine
        
        # Try to load a nice font, or fallback to default
        font_path = os.path.join("fonts", "04B_03.TTF")
        if os.path.exists(font_path):
            try:
                self.font = ImageFont.truetype(font_path, 8)
            except:
                self.font = ImageFont.load_default()
        else:
            self.font = ImageFont.load_default()

    def get_ip_address(self):
        try:
            s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            s.connect(("8.8.8.8", 80))
            ip = s.getsockname()[0]
            s.close()
            return ip
        except Exception:
            return "127.0.0.1"

    def run(self, duration_sec):
        logging.info(f"Starting NetworkEngine for {duration_sec}s")
        start_time = time.time()
        canvas = self.mw.get_canvas()
        if not canvas:
            return
            
        ip = self.get_ip_address()
        hostname = socket.gethostname()
        
        while time.time() - start_time < duration_sec:
            if getattr(self.config, 'reload_flag', False):
                break
                
            img = Image.new('RGB', (self.config.matrix_width, self.config.matrix_height), color=(0, 0, 0))
            draw = ImageDraw.Draw(img)
            
            # Draw IP address
            text = f"IP: {ip}"
            try:
                bbox = self.font.getbbox(text)
                text_width = bbox[2] - bbox[0]
            except AttributeError:
                try:
                    text_width, _ = draw.textsize(text, font=self.font)
                except:
                    text_width = 40
            text_x = (self.config.matrix_width - text_width) // 2
            text_y = (self.config.matrix_height // 2) - 6
            draw.text((text_x, text_y), text, font=self.font, fill=(0, 255, 0))
            
            # Draw Hostname
            try:
                bbox = self.font.getbbox(hostname)
                host_width = bbox[2] - bbox[0]
            except AttributeError:
                try:
                    host_width, _ = draw.textsize(hostname, font=self.font)
                except:
                    host_width = 40
            text_x = (self.config.matrix_width - host_width) // 2
            text_y = (self.config.matrix_height // 2) + 2
            draw.text((text_x, text_y), hostname, font=self.font, fill=(255, 255, 255))
            
            if self.fighter_engine:
                img = self.fighter_engine.tick(img)
            
            canvas.SetImage(img)
            canvas = self.mw.swap_canvas(canvas)
            time.sleep(0.02)

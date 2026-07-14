import time
import os
import random
import logging
from PIL import Image

class GifEngine:
    def __init__(self, matrix_wrapper, config):
        self.mw = matrix_wrapper
        self.config = config
        self.gifs_dir = "gifs"
        self._ensure_dir()

    def _ensure_dir(self):
        if not os.path.exists(self.gifs_dir):
            os.makedirs(self.gifs_dir)

    def run(self, count):
        logging.info(f"Starting GifEngine to play {count} GIFs")
        if not os.path.exists(self.gifs_dir):
            return
            
        all_gifs = []
        
        # Check subdirectories (playlists)
        folders = [d for d in os.listdir(self.gifs_dir) if os.path.isdir(os.path.join(self.gifs_dir, d))]
        if self.config.selected_gifs:
            # UI might send 'gifs/my_folder' or 'my_folder'. Normalize to basename.
            allowed = [os.path.basename(p.rstrip('/\\')) for p in self.config.selected_gifs]
            folders = [f for f in folders if f in allowed or f in self.config.selected_gifs]
            
        for folder in folders:
            folder_path = os.path.join(self.gifs_dir, folder)
            gifs_in_folder = [os.path.join(folder, f) for f in os.listdir(folder_path) if f.lower().endswith('.gif')]
            all_gifs.extend(gifs_in_folder)
            
        # Also check root directory for standalone GIFs if no folder is specifically selected
        if not self.config.selected_gifs:
            root_gifs = [f for f in os.listdir(self.gifs_dir) if os.path.isfile(os.path.join(self.gifs_dir, f)) and f.lower().endswith('.gif')]
            all_gifs.extend(root_gifs)
            
        if not all_gifs:
            logging.info("No GIFs found or selected.")
            time.sleep(2)
            return

        canvas = self.mw.get_canvas()
        if not canvas:
            return

        for _ in range(count):
            if getattr(self.config, 'reload_flag', False): break
            gif_name = random.choice(all_gifs)
            self._play_gif(os.path.join(self.gifs_dir, gif_name), canvas)
            
    def _play_gif(self, gif_path, canvas):
        try:
            gif = Image.open(gif_path)
            logging.info(f"Playing GIF: {gif_path}")
        except Exception as e:
            logging.error(f"Cannot open GIF {gif_path}: {e}")
            return
            
        frames = []
        try:
            while True:
                # Convert frame to RGB
                # RGBMatrix expects a raw RGB image buffer
                frame = gif.convert('RGB')
                
                # Resize if necessary to fit matrix
                if frame.size != (self.config.matrix_width, self.config.matrix_height):
                    frame = frame.resize((self.config.matrix_width, self.config.matrix_height), Image.Resampling.NEAREST)
                    
                # Extract frame duration (usually in ms, default to 100ms if not found)
                duration = gif.info.get('duration', 100) / 1000.0 
                if duration < 0.02: # Prevent insanely fast GIFs
                    duration = 0.05
                    
                frames.append((frame, duration))
                gif.seek(gif.tell() + 1)
        except EOFError:
            pass # End of GIF frames

        if not frames:
            return

        # Play the GIF fully once (or loop it for a minimum duration)
        # Let's play it at least once. If it's too short (< 3 seconds), loop it.
        total_played_time = 0
        min_play_time = 3.0 
        
        while total_played_time < min_play_time:
            for frame, duration in frames:
                start_frame = time.time()
                
                canvas.SetImage(frame)
                canvas = self.mw.swap_canvas(canvas)
                
                # Wait for the frame duration
                elapsed = time.time() - start_frame
                sleep_time = duration - elapsed
                if sleep_time > 0:
                    time.sleep(sleep_time)
                    
                total_played_time += duration
                
                # Break early if we exceeded min_play_time AND finished at least one loop
                if total_played_time >= min_play_time:
                    break

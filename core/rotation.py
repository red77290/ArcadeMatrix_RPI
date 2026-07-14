import time
import logging
from engines.clock import ClockEngine
from engines.date import DateEngine
from engines.weather import WeatherEngine
from engines.gif import GifEngine
from engines.fighter import FighterEngine
from engines.network import NetworkEngine

class RotationManager:
    def __init__(self, matrix_wrapper, config):
        self.mw = matrix_wrapper
        self.config = config
        self.fighter_engine = FighterEngine(config)
        self.engines = {
            'clock': ClockEngine(matrix_wrapper, config, self.fighter_engine),
            'date': DateEngine(matrix_wrapper, config, self.fighter_engine),
            'weather': WeatherEngine(matrix_wrapper, config, self.fighter_engine),
            'network': NetworkEngine(matrix_wrapper, config, self.fighter_engine),
            'gifs': GifEngine(matrix_wrapper, config)
        }

    def start_loop(self):
        logging.info("Starting idle rotation loop...")
        while True:
            rotation_list = self.config.idle_rotation
            if not rotation_list:
                logging.warning("No rotation configured. Defaulting to clock.")
                rotation_list = ['clock']
                
            for engine_name in rotation_list:
                # Intercept forced jump
                if getattr(self.config, 'force_engine', None):
                    engine_name = self.config.force_engine
                    self.config.force_engine = None
                    self.config.reload_flag = False
                    
                if getattr(self.config, 'reload_flag', False):
                    self.config.reload_flag = False
                    break
                    
                engine_name = engine_name.strip()
                if engine_name not in self.engines:
                    logging.warning(f"Unknown engine: {engine_name}")
                    continue
                    
                engine = self.engines[engine_name]
                
                # Run the engine for its configured duration or count
                is_single = (len(rotation_list) == 1)
                
                if engine_name == 'clock':
                    engine.run(86400 if is_single else self.config.idle_clock_dur)
                elif engine_name == 'date':
                    engine.run(86400 if is_single else self.config.idle_date_dur)
                elif engine_name == 'weather':
                    engine.run(86400 if is_single else self.config.idle_weather_dur)
                elif engine_name == 'network':
                    engine.run(86400 if is_single else 10)
                elif engine_name == 'gifs':
                    engine.run(self.config.idle_gifs_count)
                    
                self.fighter_engine.reset()
                
                # Small pause between engines for smooth transition
                if not is_single:
                    self.mw.clear()
                    time.sleep(0.5)

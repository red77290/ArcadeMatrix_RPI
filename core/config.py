import configparser
import os
import logging

class Config:
    def __init__(self, config_file="conf.ini"):
        self.config_file = config_file
        self.parser = configparser.ConfigParser()
        self.parser.optionxform = str  # Preserve case for keys
        self.load_defaults()
        self.load()

    def load_defaults(self):
        self.reload_flag = False
        
        # MATRIX
        self.matrix_width = 64
        self.matrix_height = 32
        self.matrix_mapping = "regular"
        self.matrix_brightness = 50
        self.matrix_slowdown = 2
        self.matrix_chain = 1
        self.matrix_parallel = 1
        self.matrix_rgb_sequence = "RGB"
        self.matrix_pwm_bits = 11
        self.matrix_pwm_lsb_nanoseconds = 130
        
        # MQTT
        self.mqtt_enabled = False
        self.mqtt_broker = ""
        self.mqtt_port = 1883
        self.mqtt_user = ""
        self.mqtt_pass = ""
        self.mqtt_device = "ArcadeMatrixRPi"
        self.mqtt_topic_bato = "batocera/system/playing"
        self.mqtt_topic_recal = "recalbox/system/playing"

        # WIFI
        self.wifi_ssid = ""
        self.wifi_pass = ""
        self.wifi_configured = False

        # TIME
        self.time_24h = True
        self.time_font = "PressStart2P.ttf"
        self.time_size = 2
        self.time_theme = 0
        self.time_offset_x = 0
        self.time_offset_y = 0
        self.clock_color_1 = "#FFFFFF"
        self.clock_color_2 = "#FFFFFF"

        # IDLE
        self.idle_rotation = ["clock", "date", "weather", "gifs", "sprites"]
        self.idle_clock_dur = 15
        self.idle_date_dur = 10
        self.idle_weather_dur = 10
        self.idle_gifs_count = 5
        self.idle_sprite_count = 3
        self.selected_gifs = []
        self.selected_sprites = []

        # DATE
        self.date_theme = 0
        self.date_bg_sprite = "stage1.png"
        self.date_format = "DD/MM"
        self.date_font = "PressStart2P.ttf"
        self.date_size = 2
        self.date_offset_x = 0
        self.date_offset_y = 0
        self.date_color_1 = "#FFFFFF"
        self.date_color_2 = "#FFFFFF"

        # WEATHER
        self.weather_api = ""
        self.weather_city = ""
        self.weather_offset_x = 0
        self.weather_offset_y = 0

        # STANDBY
        self.standby_enabled = False
        self.standby_turn_off = "23:00"
        self.standby_wake_up = "07:00"

    def load(self):
        if not os.path.exists(self.config_file):
            logging.warning(f"Config file {self.config_file} not found. Using defaults.")
            return

        self.parser.read(self.config_file)
        
        # Helper to read safe
        def get_int(section, key, default):
            try: return self.parser.getint(section, key)
            except: return default
            
        def get_bool(section, key, default):
            try: return self.parser.getboolean(section, key)
            except: return default

        def get_str(section, key, default):
            try: return self.parser.get(section, key)
            except: return default

        # Parse MATRIX
        self.matrix_rows = get_int('MATRIX', 'ROWS', 32)
        self.matrix_cols = get_int('MATRIX', 'COLS', 64)
        self.matrix_chain = get_int('MATRIX', 'CHAIN', 1)
        self.matrix_parallel = get_int('MATRIX', 'PARALLEL', 1)
        self.matrix_mapping = get_str('MATRIX', 'HARDWARE_MAPPING', self.matrix_mapping)
        self.matrix_brightness = get_int('MATRIX', 'BRIGHTNESS', self.matrix_brightness)
        self.matrix_slowdown = get_int('MATRIX', 'SLOWDOWN', self.matrix_slowdown)
        self.matrix_rgb_sequence = get_str('MATRIX', 'RGB_SEQUENCE', self.matrix_rgb_sequence)
        self.matrix_pwm_bits = get_int('MATRIX', 'PWM_BITS', self.matrix_pwm_bits)
        self.matrix_pwm_lsb_nanoseconds = get_int('MATRIX', 'PWM_LSB_NANOSECONDS', self.matrix_pwm_lsb_nanoseconds)

        # Compute total dimensions
        self.matrix_width = self.matrix_cols * self.matrix_chain
        self.matrix_height = self.matrix_rows * self.matrix_parallel

        # Parse MQTT
        self.mqtt_enabled = get_bool('MQTT', 'ENABLED', self.mqtt_enabled)
        self.mqtt_broker = get_str('MQTT', 'BROKER', self.mqtt_broker)
        self.mqtt_port = get_int('MQTT', 'PORT', self.mqtt_port)
        self.mqtt_user = get_str('MQTT', 'USER', self.mqtt_user)
        self.mqtt_pass = get_str('MQTT', 'PASS', self.mqtt_pass)
        self.mqtt_device = get_str('MQTT', 'DEVICE_NAME', self.mqtt_device)
        self.mqtt_topic_bato = get_str('MQTT', 'TOPIC_BATOCERA', self.mqtt_topic_bato)
        self.mqtt_topic_recal = get_str('MQTT', 'TOPIC_RECALBOX', self.mqtt_topic_recal)

        # Parse WIFI
        self.wifi_ssid = get_str('WIFI', 'SSID', self.wifi_ssid)
        self.wifi_pass = get_str('WIFI', 'PASS', self.wifi_pass)
        self.wifi_configured = get_bool('WIFI', 'CONFIGURED', self.wifi_configured)

        # Parse TIME
        self.time_24h = get_bool('TIME', 'FORMAT_24H', self.time_24h)
        self.time_font = get_str('TIME', 'CLOCK_FONT', self.time_font)
        self.time_size = get_int('TIME', 'CLOCK_SIZE', self.time_size)
        self.time_theme = get_int('TIME', 'CLOCK_THEME', self.time_theme)
        self.time_offset_x = get_int('TIME', 'CLOCK_OFFSET_X', self.time_offset_x)
        self.time_offset_y = get_int('TIME', 'CLOCK_OFFSET_Y', self.time_offset_y)
        self.clock_color_1 = get_str('TIME', 'CLOCK_COLOR_1', self.clock_color_1)
        self.clock_color_2 = get_str('TIME', 'CLOCK_COLOR_2', self.clock_color_2)

        # Parse IDLE
        rot = get_str('IDLE', 'ROTATION', "")
        if rot: self.idle_rotation = [x.strip() for x in rot.split(',')]
        self.idle_clock_dur = get_int('IDLE', 'CLOCK_DURATION_SEC', self.idle_clock_dur)
        self.idle_date_dur = get_int('IDLE', 'DATE_DURATION_SEC', self.idle_date_dur)
        self.idle_weather_dur = get_int('IDLE', 'WEATHER_DURATION_SEC', self.idle_weather_dur)
        self.idle_gifs_count = get_int('IDLE', 'GIFS_COUNT', self.idle_gifs_count)
        self.idle_sprite_count = get_int('IDLE', 'SPRITE_COUNT', self.idle_sprite_count)
        sg = get_str('IDLE', 'SELECTED_GIFS', "")
        if sg: self.selected_gifs = [x.strip() for x in sg.split(',') if x.strip()]
        sp = get_str('IDLE', 'SELECTED_SPRITES', "")
        if sp: self.selected_sprites = [x.strip() for x in sp.split(',') if x.strip()]

        # Parse DATE
        self.date_theme = get_int('DATE', 'THEME', self.date_theme)
        self.date_bg_sprite = get_str('DATE', 'BACKGROUND_SPRITE', self.date_bg_sprite)
        self.date_format = get_str('DATE', 'FORMAT', self.date_format)
        self.date_font = get_str('DATE', 'DATE_FONT', self.date_font)
        self.date_size = get_int('DATE', 'DATE_SIZE', self.date_size)
        self.date_offset_x = get_int('DATE', 'DATE_OFFSET_X', self.date_offset_x)
        self.date_offset_y = get_int('DATE', 'DATE_OFFSET_Y', self.date_offset_y)
        self.date_color_1 = get_str('DATE', 'DATE_COLOR_1', self.date_color_1)
        self.date_color_2 = get_str('DATE', 'DATE_COLOR_2', self.date_color_2)

        # Parse WEATHER
        self.weather_api = get_str('WEATHER', 'API_KEY', self.weather_api)
        self.weather_city = get_str('WEATHER', 'CITY', self.weather_city)
        self.weather_offset_x = get_int('WEATHER', 'WEATHER_OFFSET_X', self.weather_offset_x)
        self.weather_offset_y = get_int('WEATHER', 'WEATHER_OFFSET_Y', self.weather_offset_y)

        # Parse STANDBY
        self.standby_enabled = get_bool('STANDBY', 'NIGHT_MODE_ENABLED', self.standby_enabled)
        self.standby_turn_off = get_str('STANDBY', 'TURN_OFF_AT', self.standby_turn_off)
        self.standby_wake_up = get_str('STANDBY', 'WAKE_UP_AT', self.standby_wake_up)

    def save(self):
        # Update parser object
        if not self.parser.has_section('MATRIX'): self.parser.add_section('MATRIX')
        self.parser.set('MATRIX', 'ROWS', str(self.matrix_rows))
        self.parser.set('MATRIX', 'COLS', str(self.matrix_cols))
        self.parser.set('MATRIX', 'HARDWARE_MAPPING', str(self.matrix_mapping))
        self.parser.set('MATRIX', 'BRIGHTNESS', str(self.matrix_brightness))
        self.parser.set('MATRIX', 'SLOWDOWN', str(self.matrix_slowdown))
        self.parser.set('MATRIX', 'CHAIN', str(self.matrix_chain))
        self.parser.set('MATRIX', 'PARALLEL', str(self.matrix_parallel))
        self.parser.set('MATRIX', 'RGB_SEQUENCE', str(self.matrix_rgb_sequence))
        self.parser.set('MATRIX', 'PWM_BITS', str(self.matrix_pwm_bits))
        self.parser.set('MATRIX', 'PWM_LSB_NANOSECONDS', str(self.matrix_pwm_lsb_nanoseconds))

        if not self.parser.has_section('MQTT'): self.parser.add_section('MQTT')
        self.parser.set('MQTT', 'ENABLED', str(self.mqtt_enabled).lower())
        self.parser.set('MQTT', 'BROKER', str(self.mqtt_broker))
        self.parser.set('MQTT', 'PORT', str(self.mqtt_port))
        self.parser.set('MQTT', 'USER', str(self.mqtt_user))
        self.parser.set('MQTT', 'PASS', str(self.mqtt_pass))

        if not self.parser.has_section('WIFI'): self.parser.add_section('WIFI')
        self.parser.set('WIFI', 'SSID', str(self.wifi_ssid))
        self.parser.set('WIFI', 'PASS', str(self.wifi_pass))
        self.parser.set('WIFI', 'CONFIGURED', str(self.wifi_configured).lower())

        if not self.parser.has_section('TIME'): self.parser.add_section('TIME')
        self.parser.set('TIME', 'FORMAT_24H', str(self.time_24h).lower())
        self.parser.set('TIME', 'CLOCK_FONT', str(self.time_font))
        self.parser.set('TIME', 'CLOCK_SIZE', str(self.time_size))
        self.parser.set('TIME', 'CLOCK_THEME', str(self.time_theme))
        self.parser.set('TIME', 'CLOCK_OFFSET_X', str(self.time_offset_x))
        self.parser.set('TIME', 'CLOCK_OFFSET_Y', str(self.time_offset_y))
        self.parser.set('TIME', 'CLOCK_COLOR_1', self.clock_color_1)
        self.parser.set('TIME', 'CLOCK_COLOR_2', self.clock_color_2)

        if not self.parser.has_section('IDLE'): self.parser.add_section('IDLE')
        self.parser.set('IDLE', 'ROTATION', ",".join(self.idle_rotation))
        self.parser.set('IDLE', 'CLOCK_DURATION_SEC', str(self.idle_clock_dur))
        self.parser.set('IDLE', 'DATE_DURATION_SEC', str(self.idle_date_dur))
        self.parser.set('IDLE', 'WEATHER_DURATION_SEC', str(self.idle_weather_dur))
        self.parser.set('IDLE', 'GIFS_COUNT', str(self.idle_gifs_count))
        self.parser.set('IDLE', 'SPRITE_COUNT', str(self.idle_sprite_count))
        self.parser.set('IDLE', 'SELECTED_GIFS', ",".join(self.selected_gifs))
        self.parser.set('IDLE', 'SELECTED_SPRITES', ",".join(self.selected_sprites))

        if not self.parser.has_section('DATE'): self.parser.add_section('DATE')
        self.parser.set('DATE', 'THEME', str(self.date_theme))
        self.parser.set('DATE', 'BACKGROUND_SPRITE', self.date_bg_sprite)
        self.parser.set('DATE', 'FORMAT', self.date_format)
        self.parser.set('DATE', 'DATE_FONT', str(self.date_font))
        self.parser.set('DATE', 'DATE_SIZE', str(self.date_size))
        self.parser.set('DATE', 'DATE_OFFSET_X', str(self.date_offset_x))
        self.parser.set('DATE', 'DATE_OFFSET_Y', str(self.date_offset_y))
        self.parser.set('DATE', 'DATE_COLOR_1', self.date_color_1)
        self.parser.set('DATE', 'DATE_COLOR_2', self.date_color_2)

        if not self.parser.has_section('WEATHER'): self.parser.add_section('WEATHER')
        self.parser.set('WEATHER', 'API_KEY', str(self.weather_api))
        self.parser.set('WEATHER', 'CITY', str(self.weather_city))
        self.parser.set('WEATHER', 'WEATHER_OFFSET_X', str(self.weather_offset_x))
        self.parser.set('WEATHER', 'WEATHER_OFFSET_Y', str(self.weather_offset_y))

        if not self.parser.has_section('STANDBY'): self.parser.add_section('STANDBY')
        self.parser.set('STANDBY', 'NIGHT_MODE_ENABLED', str(self.standby_enabled).lower())
        self.parser.set('STANDBY', 'TURN_OFF_AT', str(self.standby_turn_off))
        self.parser.set('STANDBY', 'WAKE_UP_AT', str(self.standby_wake_up))

        with open(self.config_file, 'w') as f:
            self.parser.write(f)

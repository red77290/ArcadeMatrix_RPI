use configparser::ini::Ini;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSettings {
    // MATRIX
    pub matrix_rows: u32,
    pub matrix_cols: u32,
    pub matrix_chain: u32,
    pub matrix_parallel: u32,
    pub matrix_mapping: String,
    pub matrix_slowdown: u32,
    pub matrix_brightness: u32,
    pub matrix_rgb_sequence: String,
    pub matrix_pwm_bits: u32,
    pub matrix_pwm_lsb_nanoseconds: u32,
    pub matrix_disable_hardware_pulsing: bool,

    // TIME
    pub time_format: String,
    pub time_font: String,
    pub time_size: u32,
    pub time_theme: i32,
    pub clock_color_1: String,
    pub clock_color_2: String,
    pub time_offset_x: i32,
    pub time_offset_y: i32,
    pub ntp_server: String,
    pub timezone: String,

    // DATE
    pub date_format: String,
    pub date_font: String,
    pub date_size: u32,
    pub date_theme: i32,
    pub date_color_1: String,
    pub date_color_2: String,
    pub date_offset_x: i32,
    pub date_offset_y: i32,

    // WEATHER
    pub weather_api_key: String,
    pub weather_city: String,
    pub weather_lang: String,
    pub weather_offset_x: i32,
    pub weather_offset_y: i32,

    // IDLE ROTATION
    pub idle_rotation: Vec<String>,
    pub idle_clock_duration_sec: u32,
    pub idle_date_duration_sec: u32,
    pub idle_weather_duration_sec: u32,
    pub idle_gifs_count: u32,
    pub idle_sprite_count: u32,
    pub idle_fighter_interval: u32,
    pub selected_gifs: Vec<String>,

    // STANDBY / NIGHT
    pub standby_enabled: bool,
    pub standby_turn_off: String,
    pub standby_wake_up: String,
    pub standby_night_brightness: u32,

    // MQTT
    pub mqtt_enabled: bool,
    pub mqtt_broker: String,
    pub mqtt_port: u16,
    pub mqtt_user: String,
    pub mqtt_pass: String,

    // API
    pub api_auth_enabled: bool,
    pub api_token: String,

    // WIFI
    pub wifi_ssid: String,
    pub wifi_pass: String,
    pub wifi_configured: bool,
}

impl Default for ConfigSettings {
    fn default() -> Self {
        Self {
            matrix_rows: 32,
            matrix_cols: 64,
            matrix_chain: 1,
            matrix_parallel: 1,
            matrix_mapping: "regular".to_string(),
            matrix_slowdown: 2,
            matrix_brightness: 40,
            matrix_rgb_sequence: "RGB".to_string(),
            matrix_pwm_bits: 11,
            matrix_pwm_lsb_nanoseconds: 130,
            matrix_disable_hardware_pulsing: false,

            time_format: "%H:%M:%S".to_string(),
            time_font: "PressStart2P.ttf".to_string(),
            time_size: 2,
            time_theme: 0,
            clock_color_1: "#ffffff".to_string(),
            clock_color_2: "#ffffff".to_string(),
            time_offset_x: 0,
            time_offset_y: 0,
            ntp_server: "pool.ntp.org".to_string(),
            timezone: "CET-1CEST,M3.5.0,M10.5.0/3".to_string(),

            date_format: "%d/%m".to_string(),
            date_font: "PressStart2P.ttf".to_string(),
            date_size: 2,
            date_theme: 0,
            date_color_1: "#ffffff".to_string(),
            date_color_2: "#ffffff".to_string(),
            date_offset_x: 0,
            date_offset_y: 0,

            weather_api_key: "".to_string(),
            weather_city: "".to_string(),
            weather_lang: "en".to_string(),
            weather_offset_x: 0,
            weather_offset_y: 0,

            idle_rotation: vec![
                "clock".to_string(),
                "date".to_string(),
                "weather".to_string(),
                "gifs".to_string(),
            ],
            idle_clock_duration_sec: 60,
            idle_date_duration_sec: 10,
            idle_weather_duration_sec: 15,
            idle_gifs_count: 3,
            idle_sprite_count: 1,
            idle_fighter_interval: 10,
            selected_gifs: vec![],

            standby_enabled: false,
            standby_turn_off: "23:00".to_string(),
            standby_wake_up: "07:00".to_string(),
            standby_night_brightness: 10,

            mqtt_enabled: false,
            mqtt_broker: "127.0.0.1".to_string(),
            mqtt_port: 1883,
            mqtt_user: "".to_string(),
            mqtt_pass: "".to_string(),

            api_auth_enabled: false,
            api_token: "9101d2ff5928c93107e537aa3c07a282".to_string(),

            wifi_ssid: "".to_string(),
            wifi_pass: "".to_string(),
            wifi_configured: false,
        }
    }
}

pub struct Config {
    pub config_file: Mutex<PathBuf>,
    pub reload_flag: AtomicBool,
    pub matrix_power: AtomicBool,
    pub matrix_brightness: AtomicU32,
    pub force_engine: Mutex<Option<String>>,
    pub message_payload: Mutex<Option<serde_json::Value>>,
    pub image_obj: Mutex<Option<image::RgbImage>>,
    pub settings: RwLock<ConfigSettings>,
}

impl Config {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let path_buf = path.as_ref().to_path_buf();
        let mut settings = ConfigSettings::default();
        Self::load_from_ini(&path_buf, &mut settings);

        let initial_brightness = settings.matrix_brightness;

        Self {
            config_file: Mutex::new(path_buf),
            reload_flag: AtomicBool::new(false),
            matrix_power: AtomicBool::new(true),
            matrix_brightness: AtomicU32::new(initial_brightness),
            force_engine: Mutex::new(None),
            message_payload: Mutex::new(None),
            image_obj: Mutex::new(None),
            settings: RwLock::new(settings),
        }
    }

    pub fn load_from_ini(path: &Path, settings: &mut ConfigSettings) {
        if !path.exists() {
            return;
        }

        let mut ini = Ini::new();
        if ini.load(path).is_err() {
            return;
        }

        // MATRIX
        if let Ok(Some(val)) = ini.getuint("MATRIX", "ROWS") {
            settings.matrix_rows = val as u32;
        }
        if let Ok(Some(val)) = ini.getuint("MATRIX", "COLS") {
            settings.matrix_cols = val as u32;
        }
        if let Ok(Some(val)) = ini.getuint("MATRIX", "CHAIN") {
            settings.matrix_chain = val as u32;
        }
        if let Ok(Some(val)) = ini.getuint("MATRIX", "PARALLEL") {
            settings.matrix_parallel = val as u32;
        }
        if let Some(v) = ini.get("MATRIX", "HARDWARE_MAPPING") {
            settings.matrix_mapping = v;
        }
        if let Ok(Some(val)) = ini.getuint("MATRIX", "SLOWDOWN") {
            settings.matrix_slowdown = val as u32;
        }
        if let Ok(Some(val)) = ini.getuint("MATRIX", "BRIGHTNESS") {
            settings.matrix_brightness = val as u32;
        }
        if let Some(v) = ini.get("MATRIX", "RGB_SEQUENCE") {
            settings.matrix_rgb_sequence = v;
        }
        if let Ok(Some(val)) = ini.getuint("MATRIX", "PWM_BITS") {
            settings.matrix_pwm_bits = val as u32;
        }
        if let Ok(Some(val)) = ini.getint("MATRIX", "pwm_lsb_nanoseconds") {
            settings.matrix_pwm_lsb_nanoseconds = val as u32;
        }
        if let Ok(Some(val)) = ini.getbool("MATRIX", "disable_hardware_pulsing") {
            settings.matrix_disable_hardware_pulsing = val;
        }

        // TIME
        if let Some(v) = ini.get("TIME", "FORMAT") {
            settings.time_format = v;
        }
        if let Some(v) = ini.get("TIME", "CLOCK_FONT") {
            settings.time_font = v;
        }
        if let Ok(Some(val)) = ini.getuint("TIME", "CLOCK_SIZE") {
            settings.time_size = val as u32;
        }
        if let Ok(Some(val)) = ini.getint("TIME", "THEME") {
            settings.time_theme = val as i32;
        }
        if let Some(v) = ini.get("TIME", "CLOCK_COLOR_1") {
            settings.clock_color_1 = v;
        }
        if let Some(v) = ini.get("TIME", "CLOCK_COLOR_2") {
            settings.clock_color_2 = v;
        }
        if let Ok(Some(val)) = ini.getint("TIME", "CLOCK_OFFSET_X") {
            settings.time_offset_x = val as i32;
        }
        if let Ok(Some(val)) = ini.getint("TIME", "CLOCK_OFFSET_Y") {
            settings.time_offset_y = val as i32;
        }
        if let Some(v) = ini.get("TIME", "NTP_SERVER") {
            settings.ntp_server = v;
        }
        if let Some(v) = ini.get("TIME", "TIMEZONE") {
            settings.timezone = v;
        }

        // DATE
        if let Some(v) = ini.get("DATE", "FORMAT") {
            settings.date_format = v;
        }
        if let Some(v) = ini.get("DATE", "DATE_FONT") {
            settings.date_font = v;
        }
        if let Ok(Some(val)) = ini.getuint("DATE", "DATE_SIZE") {
            settings.date_size = val as u32;
        }
        if let Ok(Some(val)) = ini.getint("DATE", "THEME") {
            settings.date_theme = val as i32;
        }
        if let Some(v) = ini.get("DATE", "DATE_COLOR_1") {
            settings.date_color_1 = v;
        }
        if let Some(v) = ini.get("DATE", "DATE_COLOR_2") {
            settings.date_color_2 = v;
        }
        if let Ok(Some(val)) = ini.getint("DATE", "DATE_OFFSET_X") {
            settings.date_offset_x = val as i32;
        }
        if let Ok(Some(val)) = ini.getint("DATE", "DATE_OFFSET_Y") {
            settings.date_offset_y = val as i32;
        }

        // WEATHER
        if let Some(v) = ini.get("WEATHER", "API_KEY") {
            settings.weather_api_key = v;
        }
        if let Some(v) = ini.get("WEATHER", "CITY") {
            settings.weather_city = v;
        }
        if let Some(v) = ini.get("WEATHER", "LANG") {
            settings.weather_lang = v;
        }
        if let Ok(Some(val)) = ini.getint("WEATHER", "WEATHER_OFFSET_X") {
            settings.weather_offset_x = val as i32;
        }
        if let Ok(Some(val)) = ini.getint("WEATHER", "WEATHER_OFFSET_Y") {
            settings.weather_offset_y = val as i32;
        }

        // IDLE
        if let Some(v) = ini.get("IDLE", "ROTATION") {
            let items: Vec<String> = v
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if !items.is_empty() {
                settings.idle_rotation = items;
            }
        }
        if let Ok(Some(val)) = ini.getuint("IDLE", "CLOCK_DURATION_SEC") {
            settings.idle_clock_duration_sec = val as u32;
        }
        if let Ok(Some(val)) = ini.getuint("IDLE", "DATE_DURATION_SEC") {
            settings.idle_date_duration_sec = val as u32;
        }
        if let Ok(Some(val)) = ini.getuint("IDLE", "WEATHER_DURATION_SEC") {
            settings.idle_weather_duration_sec = val as u32;
        }
        if let Ok(Some(val)) = ini.getuint("IDLE", "GIF_DURATION_SEC") {
            settings.idle_gifs_count = val as u32;
        }
        if let Ok(Some(val)) = ini.getuint("IDLE", "SPRITE_COUNT") {
            settings.idle_sprite_count = val as u32;
        }
        if let Ok(Some(val)) = ini.getuint("IDLE", "FIGHTER_INTERVAL_SEC") {
            settings.idle_fighter_interval = val as u32;
        }
        if let Some(v) = ini.get("IDLE", "SELECTED_GIFS") {
            settings.selected_gifs = v
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }

        // STANDBY
        if let Ok(Some(val)) = ini.getbool("STANDBY", "NIGHT_MODE_ENABLED") {
            settings.standby_enabled = val;
        }
        if let Some(v) = ini.get("STANDBY", "TURN_OFF_AT") {
            settings.standby_turn_off = v;
        }
        if let Some(v) = ini.get("STANDBY", "WAKE_UP_AT") {
            settings.standby_wake_up = v;
        }
        if let Ok(Some(val)) = ini.getuint("STANDBY", "NIGHT_BRIGHTNESS") {
            settings.standby_night_brightness = val as u32;
        }

        // MQTT
        if let Ok(Some(val)) = ini.getbool("MQTT", "ENABLED") {
            settings.mqtt_enabled = val;
        }
        if let Some(v) = ini.get("MQTT", "BROKER") {
            settings.mqtt_broker = v;
        }
        if let Ok(Some(val)) = ini.getuint("MQTT", "PORT") {
            settings.mqtt_port = val as u16;
        }
        if let Some(v) = ini.get("MQTT", "USER") {
            settings.mqtt_user = v;
        }
        if let Some(v) = ini.get("MQTT", "PASS") {
            settings.mqtt_pass = v;
        }

        // API
        if let Ok(Some(val)) = ini.getbool("API", "AUTH_ENABLED") {
            settings.api_auth_enabled = val;
        }
        if let Some(v) = ini.get("API", "TOKEN") {
            settings.api_token = v;
        }

        // WIFI
        if let Some(v) = ini.get("WIFI", "SSID") {
            settings.wifi_ssid = v;
        }
        if let Some(v) = ini.get("WIFI", "PASS") {
            settings.wifi_pass = v;
        }
        if let Ok(Some(val)) = ini.getbool("WIFI", "CONFIGURED") {
            settings.wifi_configured = val;
        }
    }

    pub fn save(&self) -> bool {
        let file_path = self.config_file.lock().clone();
        let s = self.settings.read().clone();

        let mut ini = Ini::new();
        ini.set("MATRIX", "ROWS", Some(s.matrix_rows.to_string()));
        ini.set("MATRIX", "COLS", Some(s.matrix_cols.to_string()));
        ini.set("MATRIX", "CHAIN", Some(s.matrix_chain.to_string()));
        ini.set("MATRIX", "PARALLEL", Some(s.matrix_parallel.to_string()));
        ini.set("MATRIX", "HARDWARE_MAPPING", Some(s.matrix_mapping));
        ini.set("MATRIX", "SLOWDOWN", Some(s.matrix_slowdown.to_string()));
        ini.set(
            "MATRIX",
            "BRIGHTNESS",
            Some(s.matrix_brightness.to_string()),
        );
        ini.set("MATRIX", "RGB_SEQUENCE", Some(s.matrix_rgb_sequence));
        ini.set("MATRIX", "PWM_BITS", Some(s.matrix_pwm_bits.to_string()));
        ini.set(
            "MATRIX",
            "pwm_lsb_nanoseconds",
            Some(s.matrix_pwm_lsb_nanoseconds.to_string()),
        );
        ini.set(
            "MATRIX",
            "disable_hardware_pulsing",
            Some(s.matrix_disable_hardware_pulsing.to_string()),
        );

        ini.set("TIME", "FORMAT", Some(s.time_format));
        ini.set("TIME", "CLOCK_FONT", Some(s.time_font));
        ini.set("TIME", "CLOCK_SIZE", Some(s.time_size.to_string()));
        ini.set("TIME", "THEME", Some(s.time_theme.to_string()));
        ini.set("TIME", "CLOCK_COLOR_1", Some(s.clock_color_1));
        ini.set("TIME", "CLOCK_COLOR_2", Some(s.clock_color_2));
        ini.set("TIME", "CLOCK_OFFSET_X", Some(s.time_offset_x.to_string()));
        ini.set("TIME", "CLOCK_OFFSET_Y", Some(s.time_offset_y.to_string()));
        ini.set("TIME", "NTP_SERVER", Some(s.ntp_server));
        ini.set("TIME", "TIMEZONE", Some(s.timezone));

        ini.set("DATE", "FORMAT", Some(s.date_format));
        ini.set("DATE", "DATE_FONT", Some(s.date_font));
        ini.set("DATE", "DATE_SIZE", Some(s.date_size.to_string()));
        ini.set("DATE", "THEME", Some(s.date_theme.to_string()));
        ini.set("DATE", "DATE_COLOR_1", Some(s.date_color_1));
        ini.set("DATE", "DATE_COLOR_2", Some(s.date_color_2));
        ini.set("DATE", "DATE_OFFSET_X", Some(s.date_offset_x.to_string()));
        ini.set("DATE", "DATE_OFFSET_Y", Some(s.date_offset_y.to_string()));

        ini.set("WEATHER", "API_KEY", Some(s.weather_api_key));
        ini.set("WEATHER", "CITY", Some(s.weather_city));
        ini.set("WEATHER", "LANG", Some(s.weather_lang));
        ini.set(
            "WEATHER",
            "WEATHER_OFFSET_X",
            Some(s.weather_offset_x.to_string()),
        );
        ini.set(
            "WEATHER",
            "WEATHER_OFFSET_Y",
            Some(s.weather_offset_y.to_string()),
        );

        ini.set("IDLE", "ROTATION", Some(s.idle_rotation.join(",")));
        ini.set(
            "IDLE",
            "CLOCK_DURATION_SEC",
            Some(s.idle_clock_duration_sec.to_string()),
        );
        ini.set(
            "IDLE",
            "DATE_DURATION_SEC",
            Some(s.idle_date_duration_sec.to_string()),
        );
        ini.set(
            "IDLE",
            "WEATHER_DURATION_SEC",
            Some(s.idle_weather_duration_sec.to_string()),
        );
        ini.set(
            "IDLE",
            "GIF_DURATION_SEC",
            Some(s.idle_gifs_count.to_string()),
        );
        ini.set(
            "IDLE",
            "SPRITE_COUNT",
            Some(s.idle_sprite_count.to_string()),
        );
        ini.set(
            "IDLE",
            "FIGHTER_INTERVAL_SEC",
            Some(s.idle_fighter_interval.to_string()),
        );
        ini.set("IDLE", "SELECTED_GIFS", Some(s.selected_gifs.join(",")));

        ini.set(
            "STANDBY",
            "NIGHT_MODE_ENABLED",
            Some(s.standby_enabled.to_string()),
        );
        ini.set("STANDBY", "TURN_OFF_AT", Some(s.standby_turn_off));
        ini.set("STANDBY", "WAKE_UP_AT", Some(s.standby_wake_up));
        ini.set(
            "STANDBY",
            "NIGHT_BRIGHTNESS",
            Some(s.standby_night_brightness.to_string()),
        );

        ini.set("MQTT", "ENABLED", Some(s.mqtt_enabled.to_string()));
        ini.set("MQTT", "BROKER", Some(s.mqtt_broker));
        ini.set("MQTT", "PORT", Some(s.mqtt_port.to_string()));
        ini.set("MQTT", "USER", Some(s.mqtt_user));
        ini.set("MQTT", "PASS", Some(s.mqtt_pass));

        ini.set("API", "AUTH_ENABLED", Some(s.api_auth_enabled.to_string()));
        ini.set("API", "TOKEN", Some(s.api_token));

        ini.set("WIFI", "SSID", Some(s.wifi_ssid));
        ini.set("WIFI", "PASS", Some(s.wifi_pass));
        ini.set("WIFI", "CONFIGURED", Some(s.wifi_configured.to_string()));

        ini.write(&file_path).is_ok()
    }
}

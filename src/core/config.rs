use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32};

pub fn parse_symbols_string(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineInstance {
    pub instance_id: String,
    pub engine_id: String,
    pub config: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationEntry {
    pub instance_id: String,
    pub duration_sec: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixConfig {
    pub width: u32,
    pub height: u32,
    pub panel_type: String,
    pub chain_length: u32,
    pub power_limit_percent: u32,
    pub force_single_buffer: bool,
    pub color_depth: u32,
    pub rgb_sequence: String,
    pub limit_refresh_rate_hz: u32,
    pub driver_chip: String,
    pub clk_phase: bool,
    pub latch_blanking: u32,
    pub row_address_mode: u32,
    pub matrix_power: bool,
    pub multiplexing: u32,
    pub slowdown: u32,
    pub pwm_bits: u32,
    pub pwm_lsb_nanoseconds: u32,
    pub disable_hardware_pulsing: bool,
    pub mapping: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WifiConfig {
    pub ssid: String,
    pub password: String,
    pub hostname: String,
    pub configured: bool,
    pub disable_internal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttConfig {
    pub enabled: bool,
    pub broker: String,
    pub port: u16,
    pub user: String,
    pub pass: String,
    pub device_name: String,
    pub topic_batocera: String,
    pub topic_recalbox: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    pub timezone: String,
    pub format_24h: bool,
    pub lang: String,
    pub unit: String,
    pub temp_offset: f32,
    pub night_mode_enabled: bool,
    pub turn_off_at: String,
    pub wake_up_at: String,
    pub night_brightness: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSettings {
    pub matrix: MatrixConfig,
    pub wifi: WifiConfig,
    pub mqtt: MqttConfig,
    pub system: SystemConfig,

    #[serde(default)]
    pub instances: Vec<EngineInstance>,
    #[serde(default)]
    pub rotation: Vec<RotationEntry>,
    
    // API
    pub api_auth_enabled: bool,
    pub api_token: String,
}

impl Default for ConfigSettings {
    fn default() -> Self {
        Self {
            matrix: MatrixConfig {
                width: 64,
                height: 32,
                panel_type: "".to_string(),
                chain_length: 1,
                power_limit_percent: 40,
                force_single_buffer: false,
                color_depth: 24,
                rgb_sequence: "RGB".to_string(),
                limit_refresh_rate_hz: 0,
                driver_chip: "SHIFTREG".to_string(),
                clk_phase: false,
                latch_blanking: 0,
                row_address_mode: 0,
                matrix_power: true,
                multiplexing: 0,
                slowdown: 2,
                pwm_bits: 11,
                pwm_lsb_nanoseconds: 130,
                disable_hardware_pulsing: false,
                mapping: "regular".to_string(),
            },
            wifi: WifiConfig {
                ssid: "".to_string(),
                password: "".to_string(),
                hostname: "arcadematrix".to_string(),
                configured: false,
                disable_internal: false,
            },
            mqtt: MqttConfig {
                enabled: false,
                broker: "127.0.0.1".to_string(),
                port: 1883,
                user: "".to_string(),
                pass: "".to_string(),
                device_name: "arcadematrix".to_string(),
                topic_batocera: "batocera".to_string(),
                topic_recalbox: "recalbox".to_string(),
            },
            system: SystemConfig {
                timezone: "CET-1CEST,M3.5.0,M10.5.0/3".to_string(),
                format_24h: true,
                lang: "en".to_string(),
                unit: "c".to_string(),
                temp_offset: 0.0,
                night_mode_enabled: false,
                turn_off_at: "23:00".to_string(),
                wake_up_at: "07:00".to_string(),
                night_brightness: 10,
            },
            api_auth_enabled: false,
            api_token: "9101d2ff5928c93107e537aa3c07a282".to_string(),
            instances: vec![],
            rotation: vec![],
        }
    }
}

pub struct Config {
    pub config_file: Mutex<PathBuf>,
    pub json_file: PathBuf,
    pub reload_flag: AtomicBool,
    pub reset_rotation: AtomicBool,
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
        let json_file = path_buf.with_file_name("config.json");

        let mut settings = ConfigSettings::default();
        let mut needs_save = false;

        if json_file.exists() {
            if let Ok(json_str) = std::fs::read_to_string(&json_file) {
                if let Ok(s) = serde_json::from_str::<ConfigSettings>(&json_str) {
                    settings = s;
                }
            }
        } else {
            // Default setup if no JSON exists
            let mut clock_cfg = std::collections::HashMap::new();
            clock_cfg.insert("theme".to_string(), "0".to_string());
            clock_cfg.insert("format".to_string(), "%H:%M:%S".to_string());
            settings.instances.push(EngineInstance {
                instance_id: "default_clock".to_string(),
                engine_id: "clock".to_string(),
                config: clock_cfg,
            });

            let mut weather_cfg = std::collections::HashMap::new();
            weather_cfg.insert("city".to_string(), "".to_string());
            weather_cfg.insert("api_key".to_string(), "".to_string());
            settings.instances.push(EngineInstance {
                instance_id: "default_weather".to_string(),
                engine_id: "weather".to_string(),
                config: weather_cfg,
            });

            settings.rotation = vec![
                RotationEntry {
                    instance_id: "default_clock".to_string(),
                    duration_sec: 60,
                },
                RotationEntry {
                    instance_id: "default_weather".to_string(),
                    duration_sec: 15,
                },
            ];
            needs_save = true;
        }

        let initial_brightness = 100;

        let cfg = Self {
            config_file: Mutex::new(path_buf),
            json_file,
            reload_flag: AtomicBool::new(false),
            reset_rotation: AtomicBool::new(false),
            matrix_power: AtomicBool::new(true),
            matrix_brightness: AtomicU32::new(initial_brightness),
            force_engine: Mutex::new(None),
            message_payload: Mutex::new(None),
            image_obj: Mutex::new(None),
            settings: RwLock::new(settings),
        };

        if needs_save {
            cfg.save();
        }

        cfg
    }

    pub fn save(&self) -> bool {
        let s = self.settings.read().clone();
        if let Ok(json_str) = serde_json::to_string_pretty(&s) {
            std::fs::write(&self.json_file, json_str).is_ok()
        } else {
            false
        }
    }
}

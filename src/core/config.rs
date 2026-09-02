use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32};

use serde_json::Value;

pub fn parse_symbols_string(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn deserialize_string_map<'de, D>(
    deserializer: D,
) -> Result<std::collections::HashMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let map = std::collections::HashMap::<String, Value>::deserialize(deserializer)?;
    let mut out = std::collections::HashMap::with_capacity(map.len());
    for (k, v) in map {
        let str_val = match v {
            Value::String(s) => s,
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            Value::Null => String::new(),
            other => other.to_string(),
        };
        out.insert(k, str_val);
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineInstance {
    pub instance_id: String,
    pub engine_id: String,
    #[serde(deserialize_with = "deserialize_string_map", default)]
    pub config: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayConfig {
    #[serde(default)]
    pub fighter: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationEntry {
    pub instance_id: String,
    pub duration_sec: u32,
    /// Transverse overlays configuration for this rotation slot (e.g. fighter overlay).
    #[serde(default)]
    pub overlays: OverlayConfig,
    /// Backward-compatibility fallback for legacy configs with top-level `fighter_overlay`.
    #[serde(default, skip_serializing)]
    pub fighter_overlay: Option<bool>,
}

impl RotationEntry {
    pub fn new(instance_id: impl Into<String>, duration_sec: u32) -> Self {
        Self {
            instance_id: instance_id.into(),
            duration_sec,
            overlays: OverlayConfig::default(),
            fighter_overlay: None,
        }
    }

    /// Normalizes the rotation entry by applying legacy `fighter_overlay` if present
    pub fn normalize(&mut self) {
        if let Some(fo) = self.fighter_overlay.take() {
            self.overlays.fighter = fo;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
    pub rotation: u32,
    pub transition_effect: String,
    pub transition_duration_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WifiConfig {
    pub ssid: String,
    pub password: String,
    pub hostname: String,
    pub configured: bool,
    pub disable_internal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
#[serde(default)]
pub struct SystemConfig {
    pub timezone: String,
    pub format_24h: bool,
    pub lang: String,
    pub temp_unit: String,
    pub night_mode_enabled: bool,
    pub turn_off_at: String,
    pub wake_up_at: String,
    pub night_brightness: u32,
    pub day_brightness: u32,
    /// Fighter overlay (decorative sprites composited on top of idle rotation
    /// screens). Formerly the "media/gif" idle animation. Defaults preserve the
    /// historical behaviour from the pre-refactor `main` branch.
    #[serde(default = "default_fighter_enabled")]
    pub idle_fighter_enabled: bool,
    #[serde(default = "default_fighter_interval")]
    pub idle_fighter_interval: u32,
    #[serde(default = "default_fighter_speed")]
    pub idle_fighter_speed: u32,
}

fn default_fighter_enabled() -> bool {
    true
}

fn default_fighter_interval() -> u32 {
    10
}

fn default_fighter_speed() -> u32 {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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

impl Default for MatrixConfig {
    fn default() -> Self {
        Self {
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
            rotation: 0,
            transition_effect: "vortex".to_string(),
            transition_duration_ms: 400,
        }
    }
}

impl Default for WifiConfig {
    fn default() -> Self {
        Self {
            ssid: "".to_string(),
            password: "".to_string(),
            hostname: "arcadematrix".to_string(),
            configured: false,
            disable_internal: false,
        }
    }
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            broker: "127.0.0.1".to_string(),
            port: 1883,
            user: "".to_string(),
            pass: "".to_string(),
            device_name: "arcadematrix".to_string(),
            topic_batocera: "batocera".to_string(),
            topic_recalbox: "recalbox".to_string(),
        }
    }
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            timezone: "CET-1CEST,M3.5.0,M10.5.0/3".to_string(),
            format_24h: true,
            lang: "en".to_string(),
            temp_unit: "C".to_string(),
            night_mode_enabled: false,
            turn_off_at: "23:00".to_string(),
            wake_up_at: "07:00".to_string(),
            night_brightness: 10,
            day_brightness: 100,
            idle_fighter_enabled: default_fighter_enabled(),
            idle_fighter_interval: default_fighter_interval(),
            idle_fighter_speed: default_fighter_speed(),
        }
    }
}

impl Default for ConfigSettings {
    fn default() -> Self {
        Self {
            matrix: MatrixConfig::default(),
            wifi: WifiConfig::default(),
            mqtt: MqttConfig::default(),
            system: SystemConfig::default(),
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
    pub message_payload: Mutex<Option<crate::engines::message::MessagePayload>>,
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
                match serde_json::from_str::<ConfigSettings>(&json_str) {
                    Ok(s) => settings = s,
                    Err(e) => {
                        tracing::error!("Failed to parse config.json, resetting to default: {}", e);
                        needs_save = true;
                    }
                }
            } else {
                needs_save = true;
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
                    overlays: OverlayConfig { fighter: true },
                    fighter_overlay: None,
                },
                RotationEntry {
                    instance_id: "default_weather".to_string(),
                    duration_sec: 15,
                    overlays: OverlayConfig { fighter: true },
                    fighter_overlay: None,
                },
            ];
            needs_save = true;
        }

        let sanitize_res =
            crate::core::config_sanitizer::ConfigSanitizer::sanitize_instances(&mut settings);
        if sanitize_res.modified {
            needs_save = true;
        }

        // Seed the live daytime brightness from the persisted value so a user's
        // saved brightness survives restarts instead of resetting to full.
        let initial_brightness = settings.system.day_brightness;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rotation_entry_legacy_normalization() {
        let json = r#"{"instance_id":"clock_1","duration_sec":30,"fighter_overlay":true}"#;
        let mut entry: RotationEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.fighter_overlay, Some(true));
        entry.normalize();
        assert_eq!(entry.overlays.fighter, true);
        assert_eq!(entry.fighter_overlay, None);
    }

    #[test]
    fn test_rotation_entry_overlays_fighter_false() {
        let json = r#"{"instance_id":"gifs_1","duration_sec":3,"overlays":{"fighter":false}}"#;
        let mut entry: RotationEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.overlays.fighter, false);
        entry.normalize();
        assert_eq!(entry.overlays.fighter, false);
        assert_eq!(entry.fighter_overlay, None);
    }

    #[test]
    fn test_rotation_entry_default() {
        let entry = RotationEntry::new("clock_main", 45);
        assert_eq!(entry.instance_id, "clock_main");
        assert_eq!(entry.duration_sec, 45);
        assert_eq!(entry.overlays.fighter, false);
    }
}

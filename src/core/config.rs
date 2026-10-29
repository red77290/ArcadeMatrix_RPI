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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderPipeline {
    /// Auto (Recommended): Automatically resolves to CanvasSingle for Wi-Fi safety
    Auto,
    /// Canvas Buffered + Single Buffer + Dirty Draw (Recommended):
    /// Offscreen RgbImage canvas, dirty row tracking, no VSync swap churn, minimal memory bus contention.
    CanvasSingle,
    /// Canvas Buffered + Double Buffer:
    /// Offscreen RgbImage canvas pushed to inactive LedCanvas with SwapOnVSync.
    CanvasDouble,
    /// Direct Double Buffer:
    /// Direct rendering to inactive LedCanvas via C FFI + SwapOnVSync.
    DirectDouble,
    /// Direct Single Buffer:
    /// Direct rendering to active scanning LedCanvas.
    DirectSingle,
}

impl Default for RenderPipeline {
    fn default() -> Self {
        RenderPipeline::CanvasSingle
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MatrixConfig {
    pub width: u32,
    pub height: u32,
    pub panel_type: String,
    pub chain_length: u32,
    #[serde(default = "default_parallel")]
    pub parallel: u32,
    pub power_limit_percent: u32,
    pub force_single_buffer: bool,
    pub color_depth: u32,
    pub rgb_sequence: String,
    pub limit_refresh_rate_hz: u32,
    pub driver_chip: String,
    pub clk_phase: bool,
    pub latch_blanking: u32,
    pub row_address_mode: u32,
    #[serde(default)]
    pub scan_mode: u32,
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
    /// Effect played when the rotation moves to the next slot ("none" disables it).
    #[serde(default = "default_slot_transition")]
    pub slot_transition: String,
    #[serde(default = "default_slot_transition_ms")]
    pub slot_transition_duration_ms: u32,
    #[serde(default)]
    pub render_pipeline: RenderPipeline,
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
    pub allow_overlay: bool,
    /// When Showcase is enabled and reachable, share Showcase's MQTT broker instead of the local broker.
    pub use_showcase_broker: bool,
    /// Automatically prepare and present a Showcase sequence (Card + Panoramas + Gameplay Video) on `playing`.
    pub ingame_showcase_enabled: bool,
    /// Cycle behavior once the Showcase sequence ends while still playing: `"alternating"`, `"loop"`, or `"once"`.
    pub ingame_cycle_mode: String,
    /// Marquee display duration (seconds) between Showcase cycles when `ingame_cycle_mode == "alternating"`.
    pub ingame_marquee_interval_sec: u32,
    /// Presentation style source: `"showcase_profile"` (inherit DMD profile on Showcase) or `"custom"` (override from DMD UI).
    pub card_style_source: String,
    pub include_card_intro: bool,
    pub card_layout: String,
    pub card_theme: String,
    pub include_poster: bool,
    pub include_synopsis: bool,
    pub text_scroll_speed: String,
    pub jacket_panorama: bool,
    pub marquee_panorama: bool,
    pub snapshot_panorama_count: u32,
    pub include_video: bool,
    pub video_max_duration_s: u32,
    pub video_fps: u32,
    pub video_mode: String,
    pub color_boost: String,
}

/// Broker credentials for the MQTT Data engine. Separate
/// from `mqtt` (the marquee / control connection, which can take over the
/// display). Nothing runs in the background: the engine connects with these
/// only while its screen is active.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DataMqttConfig {
    pub broker: String,
    pub port: u16,
    pub user: String,
    pub pass: String,
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
    false
}

fn default_fighter_interval() -> u32 {
    10
}

fn default_fighter_speed() -> u32 {
    100
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowcaseConfig {
    pub enabled: bool,
    pub server_url: String,
    pub transport_mode: String,
    pub use_showcase_broker: bool,
    pub mqtt_broker_source: String,
    pub mqtt_custom_url: String,
}

impl Default for ShowcaseConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            server_url: "http://dmdmediashowcase.local:8890".to_string(),
            transport_mode: "mqtt_udp".to_string(),
            use_showcase_broker: true,
            mqtt_broker_source: "global".to_string(),
            mqtt_custom_url: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigSettings {
    pub matrix: MatrixConfig,
    pub wifi: WifiConfig,
    pub mqtt: MqttConfig,
    pub data_mqtt: DataMqttConfig,
    pub showcase: ShowcaseConfig,
    pub system: SystemConfig,

    #[serde(default)]
    pub instances: Vec<EngineInstance>,
    #[serde(default)]
    pub rotation: Vec<RotationEntry>,

    // API
    pub api_auth_enabled: bool,
    pub api_token: String,
}

fn default_parallel() -> u32 {
    1
}

fn default_slot_transition() -> String {
    "none".to_string()
}

fn default_slot_transition_ms() -> u32 {
    500
}

pub fn generate_random_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 16] = rng.gen();
    let mut out = String::with_capacity(32);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(&mut out, "{:02x}", b);
    }
    out
}

impl Default for MatrixConfig {
    fn default() -> Self {
        Self {
            width: 64,
            height: 32,
            panel_type: "".to_string(),
            chain_length: 1,
            parallel: 1,
            power_limit_percent: 40,
            force_single_buffer: false,
            color_depth: 24,
            rgb_sequence: "RGB".to_string(),
            limit_refresh_rate_hz: 0,
            driver_chip: "SHIFTREG".to_string(),
            clk_phase: false,
            latch_blanking: 0,
            row_address_mode: 0,
            scan_mode: 0,
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
            slot_transition: default_slot_transition(),
            slot_transition_duration_ms: default_slot_transition_ms(),
            render_pipeline: RenderPipeline::CanvasSingle,
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
            allow_overlay: false,
            use_showcase_broker: true,
            ingame_showcase_enabled: true,
            ingame_cycle_mode: "alternating".to_string(),
            ingame_marquee_interval_sec: 45,
            card_style_source: "showcase_profile".to_string(),
            include_card_intro: true,
            card_layout: "classic_split".to_string(),
            card_theme: "tinted".to_string(),
            include_poster: true,
            include_synopsis: true,
            text_scroll_speed: "normal".to_string(),
            jacket_panorama: true,
            marquee_panorama: false,
            snapshot_panorama_count: 0,
            include_video: true,
            video_max_duration_s: 12,
            video_fps: 15,
            video_mode: "auto_action".to_string(),
            color_boost: "auto".to_string(),
        }
    }
}

impl Default for DataMqttConfig {
    fn default() -> Self {
        Self {
            broker: "".to_string(),
            port: 1883,
            user: "".to_string(),
            pass: "".to_string(),
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
            data_mqtt: DataMqttConfig::default(),
            showcase: ShowcaseConfig::default(),
            system: SystemConfig::default(),
            api_auth_enabled: false,
            api_token: generate_random_token(),
            instances: vec![],
            rotation: vec![],
        }
    }
}

use crate::core::types::{ForcedEngineMode, InGameShowcasePayload, ProducerSnapshot};
use arc_swap::ArcSwap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct Config {
    pub config_file: Mutex<PathBuf>,
    pub json_file: PathBuf,
    save_lock: Mutex<()>,
    pub reload_flag: AtomicBool,
    pub reset_rotation: AtomicBool,
    pub matrix_power: AtomicBool,
    pub matrix_brightness: AtomicU32,
    pub producer_snapshot: ArcSwap<ProducerSnapshot>,
    pub image_obj: Mutex<Option<image::RgbImage>>,
    pub image_generation: AtomicU64,
    pub settings: RwLock<ConfigSettings>,
}

impl Config {
    /// Resolves `path` through any symlink (even if the symlink target does not exist yet)
    /// so atomic `.tmp` writes and `rename()` operate on the underlying target file (e.g.
    /// `data/config.json`) without ever replacing the `config.json` symlink itself.
    fn resolve_symlink_target(path: &Path) -> PathBuf {
        if let Ok(canon) = std::fs::canonicalize(path) {
            return canon;
        }
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            if meta.file_type().is_symlink() {
                if let Ok(target) = std::fs::read_link(path) {
                    if target.is_absolute() {
                        return target;
                    }
                    if let Some(parent) = path.parent() {
                        return parent.join(target);
                    }
                }
            }
        }
        path.to_path_buf()
    }

    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let raw_path = path.as_ref().to_path_buf();
        let path_buf = if raw_path.is_absolute() {
            raw_path
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(&raw_path))
                .unwrap_or(raw_path)
        };
        let link_file = path_buf.with_file_name("config.json");
        let json_file = Self::resolve_symlink_target(&link_file);
        let bak_file = json_file.with_extension("json.bak");
        let legacy_bak_file = link_file.with_extension("json.bak");

        let mut settings = ConfigSettings::default();
        let mut needs_save = false;
        let mut loaded = false;

        if json_file.exists() {
            if let Ok(json_str) = std::fs::read_to_string(&json_file) {
                match serde_json::from_str::<ConfigSettings>(&json_str) {
                    Ok(s) => {
                        settings = s;
                        loaded = true;
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to parse config.json ({}), attempting backup recovery...",
                            e
                        );
                        needs_save = true;
                    }
                }
            } else {
                needs_save = true;
            }
        }

        if !loaded {
            for candidate_bak in [&bak_file, &legacy_bak_file] {
                if candidate_bak.exists() {
                    if let Ok(bak_str) = std::fs::read_to_string(candidate_bak) {
                        if let Ok(s) = serde_json::from_str::<ConfigSettings>(&bak_str) {
                            tracing::warn!("Recovered configuration from {:?}", candidate_bak);
                            settings = s;
                            loaded = true;
                            needs_save = true;
                            break;
                        }
                    }
                }
            }
        }

        if !loaded && !json_file.exists() {
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
                    overlays: OverlayConfig { fighter: false },
                    fighter_overlay: None,
                },
                RotationEntry {
                    instance_id: "default_weather".to_string(),
                    duration_sec: 15,
                    overlays: OverlayConfig { fighter: false },
                    fighter_overlay: None,
                },
            ];
            needs_save = true;
        }

        // Normalize bare single-label LAN hostnames (e.g. `http://dmdmediashowcase:8890` from v1.0.0) to `.local`
        let norm_sc_url = crate::engines::renderers::media_showcase::normalize_mdns_endpoint_url(
            &settings.showcase.server_url,
            true,
        );
        if !norm_sc_url.is_empty() && norm_sc_url != settings.showcase.server_url {
            settings.showcase.server_url = norm_sc_url;
            needs_save = true;
        }
        let norm_custom_mqtt =
            crate::engines::renderers::media_showcase::normalize_mdns_endpoint_url(
                &settings.showcase.mqtt_custom_url,
                false,
            );
        if norm_custom_mqtt != settings.showcase.mqtt_custom_url {
            settings.showcase.mqtt_custom_url = norm_custom_mqtt;
            needs_save = true;
        }

        // Auto-migrate legacy per-engine showcase settings if system showcase settings are still default
        if settings.showcase.server_url == "http://192.168.1.50:8000"
            || settings.showcase.server_url == "http://127.0.0.1:8080"
            || settings.showcase.server_url == "http://dmdmediashowcase.local:8080"
            || settings.showcase.server_url == "http://dmdmediashowcase.local:8890"
        {
            for inst in &settings.instances {
                if [
                    "retro_games",
                    "movie_showcase",
                    "series_showcase",
                    "anime_showcase",
                ]
                .contains(&inst.engine_id.as_str())
                {
                    if let Some(url) = inst.config.get("server_url") {
                        let norm_inst_url =
                            crate::engines::renderers::media_showcase::normalize_mdns_endpoint_url(
                                url, true,
                            );
                        if !norm_inst_url.is_empty()
                            && norm_inst_url != "http://dmdmediashowcase.local:8890"
                        {
                            settings.showcase.server_url = norm_inst_url;
                            settings.showcase.enabled = true;
                            if let Some(tm) = inst.config.get("transport_mode") {
                                settings.showcase.transport_mode = tm.clone();
                            }
                            if let Some(ub) = inst.config.get("use_showcase_broker") {
                                settings.showcase.use_showcase_broker = ub != "false";
                            }
                            if let Some(bs) = inst.config.get("mqtt_broker_source") {
                                settings.showcase.mqtt_broker_source = bs.clone();
                            }
                            if let Some(cu) = inst.config.get("mqtt_custom_url") {
                                settings.showcase.mqtt_custom_url =
                                    crate::engines::renderers::media_showcase::normalize_mdns_endpoint_url(
                                        cu, false,
                                    );
                            }
                            needs_save = true;
                            break;
                        }
                    }
                }
            }
        }

        if settings.api_token.trim().is_empty() {
            settings.api_token = generate_random_token();
            needs_save = true;
        }

        let _ = crate::core::config_sanitizer::ConfigSanitizer::sanitize_instances(&mut settings);

        // Seed the live daytime brightness from the persisted value so a user's
        // saved brightness survives restarts instead of resetting to full.
        let initial_brightness = settings.system.day_brightness;

        let cfg = Self {
            config_file: Mutex::new(path_buf),
            json_file,
            save_lock: Mutex::new(()),
            reload_flag: AtomicBool::new(false),
            reset_rotation: AtomicBool::new(false),
            matrix_power: AtomicBool::new(true),
            matrix_brightness: AtomicU32::new(initial_brightness),
            producer_snapshot: ArcSwap::from_pointee(ProducerSnapshot::default()),
            image_obj: Mutex::new(None),
            image_generation: AtomicU64::new(0),
            settings: RwLock::new(settings),
        };

        if needs_save {
            cfg.save();
        }

        cfg
    }

    /// Updates the dynamic marquee image and bumps `image_generation` so `MarqueeEngine` re-renders immediately.
    pub fn set_marquee_image(&self, img: Option<image::RgbImage>) {
        *self.image_obj.lock() = img;
        self.image_generation.fetch_add(1, Ordering::Release);
    }

    /// Atomically updates and publishes a forced engine mode in a lock-free snapshot.
    pub fn set_forced_engine_mode(&self, mode: ForcedEngineMode) {
        self.producer_snapshot.rcu(|current| {
            let mut next = (**current).clone();
            next.forced_mode = mode;
            next.generation = next.generation.wrapping_add(1);
            next
        });
    }

    /// Atomically updates and publishes an in-game Showcase stream payload in the lock-free snapshot.
    pub fn set_ingame_showcase(&self, payload: Option<InGameShowcasePayload>) {
        let ig_arc = payload.map(Arc::new);
        self.producer_snapshot.rcu(|current| {
            let mut next = (**current).clone();
            next.ingame_showcase = ig_arc.clone();
            next.generation = next.generation.wrapping_add(1);
            next
        });
    }

    /// Atomically clears the active in-game Showcase stream if its `session_token` matches `token`.
    pub fn clear_ingame_showcase_if_token(&self, token: u64) {
        self.producer_snapshot.rcu(|current| {
            let matches = current
                .ingame_showcase
                .as_ref()
                .map_or(false, |ig| ig.session_token == token);
            if !matches {
                return (**current).clone();
            }
            let mut next = (**current).clone();
            next.ingame_showcase = None;
            next.generation = next.generation.wrapping_add(1);
            next
        });
    }

    /// Atomically updates and publishes a message payload in a lock-free snapshot.
    pub fn set_message_payload(&self, payload: Option<crate::engines::message::MessagePayload>) {
        let msg_arc = payload.map(Arc::new);
        self.producer_snapshot.rcu(|current| {
            let mut next = (**current).clone();
            next.forced_mode = if msg_arc.is_some() {
                ForcedEngineMode::Message
            } else if next.forced_mode == ForcedEngineMode::Message {
                ForcedEngineMode::None
            } else {
                next.forced_mode
            };
            if msg_arc.is_some() {
                next.ingame_showcase = None;
            }
            next.message_payload = msg_arc.clone();
            next.generation = next.generation.wrapping_add(1);
            next
        });
    }

    /// Publishes a temporary FrontendSync fallback text banner without clearing an in-progress Showcase preparation.
    pub fn set_frontend_sync_banner(&self, payload: crate::engines::message::MessagePayload) {
        let msg_arc = Some(Arc::new(payload));
        self.producer_snapshot.rcu(|current| {
            let mut next = (**current).clone();
            next.forced_mode = ForcedEngineMode::Message;
            next.message_payload = msg_arc.clone();
            next.generation = next.generation.wrapping_add(1);
            next
        });
    }

    /// Atomically clears any forced engine override.
    pub fn clear_forced_engine(&self) {
        self.producer_snapshot.rcu(|current| {
            let mut next = (**current).clone();
            next.forced_mode = ForcedEngineMode::None;
            next.message_payload = None;
            next.ingame_showcase = None;
            next.generation = next.generation.wrapping_add(1);
            next
        });
    }

    pub fn get_sys_string(&self, key: &str, default: &str) -> String {
        let s = self.settings.read();
        match key {
            "language" | "lang" => {
                if s.system.lang.is_empty() {
                    default.to_string()
                } else {
                    s.system.lang.clone()
                }
            }
            "timezone" => {
                if s.system.timezone.is_empty() {
                    default.to_string()
                } else {
                    s.system.timezone.clone()
                }
            }
            "temp_unit" => {
                if s.system.temp_unit.is_empty() {
                    default.to_string()
                } else {
                    s.system.temp_unit.clone()
                }
            }
            _ => default.to_string(),
        }
    }

    pub fn save(&self) -> bool {
        use std::io::Write;
        let _guard = self.save_lock.lock();
        let s = self.settings.read().clone();
        let Ok(json_str) = serde_json::to_string_pretty(&s) else {
            return false;
        };

        let target_file = Self::resolve_symlink_target(&self.json_file);
        if let Some(parent) = target_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let tmp_file = target_file.with_extension("json.tmp");
        let bak_tmp_file = target_file.with_extension("json.bak.tmp");
        let bak_file = target_file.with_extension("json.bak");

        // 1. Preserve previous valid config.json into config.json.bak before replacement if present
        if target_file.exists() {
            let _ = std::fs::copy(&target_file, &bak_file);
        }

        let persisted = match std::fs::File::create(&tmp_file) {
            Ok(mut file) => {
                if let Err(e) = file
                    .write_all(json_str.as_bytes())
                    .and_then(|_| file.sync_all())
                {
                    tracing::error!(
                        "Failed to write and sync temporary config file {:?}: {}",
                        tmp_file,
                        e
                    );
                    let _ = std::fs::remove_file(&tmp_file);
                    return false;
                }
                drop(file);

                if let Err(e) = std::fs::rename(&tmp_file, &target_file) {
                    tracing::error!(
                        "Failed to atomically rename {:?} to {:?}: {}",
                        tmp_file,
                        target_file,
                        e
                    );
                    return false;
                }
                true
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to create temporary config file {:?} ({}); falling back to direct write to {:?}",
                    tmp_file,
                    e,
                    target_file
                );
                let Ok(mut file) = std::fs::File::create(&target_file) else {
                    tracing::error!(
                        "Failed to open config file {:?} for direct write: {}",
                        target_file,
                        e
                    );
                    return false;
                };
                if let Err(write_err) = file
                    .write_all(json_str.as_bytes())
                    .and_then(|_| file.sync_all())
                {
                    tracing::error!(
                        "Failed to write config file {:?}: {}",
                        target_file,
                        write_err
                    );
                    return false;
                }
                true
            }
        };

        if persisted {
            // 2. Atomically update config.json.bak with the newly committed configuration
            if let Ok(mut bfile) = std::fs::File::create(&bak_tmp_file) {
                if bfile
                    .write_all(json_str.as_bytes())
                    .and_then(|_| bfile.sync_all())
                    .is_ok()
                {
                    drop(bfile);
                    let _ = std::fs::rename(&bak_tmp_file, &bak_file);
                } else {
                    let _ = std::fs::remove_file(&bak_tmp_file);
                }
            } else {
                let _ = std::fs::copy(&target_file, &bak_file);
            }

            // 3. Flush parent directory metadata so the atomic rename survives an immediate reboot
            if let Some(parent) = target_file.parent() {
                if let Ok(dir) = std::fs::File::open(parent) {
                    let _ = dir.sync_all();
                }
            }
        }

        persisted
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

    #[test]
    fn test_atomic_save_and_backup_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("config.json");
        let cfg = Config::new(&cfg_path);
        {
            let mut s = cfg.settings.write();
            s.wifi.hostname = "custom-arcade".to_string();
        }
        assert!(cfg.save());
        assert!(dir.path().join("config.json.bak").exists());

        // Corrupt config.json and verify recovery from config.json.bak
        std::fs::write(&cfg_path, "{corrupted_json").unwrap();
        let recovered = Config::new(&cfg_path);
        assert_eq!(recovered.settings.read().wifi.hostname, "custom-arcade");
    }

    #[cfg(unix)]
    #[test]
    fn test_atomic_save_preserves_symlink_to_data_partition() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let data_cfg = data_dir.join("config.json");
        std::fs::write(&data_cfg, r#"{"wifi":{"hostname":"old-pacman-config"}}"#).unwrap();

        let symlink_cfg = dir.path().join("config.json");
        std::os::unix::fs::symlink("data/config.json", &symlink_cfg).unwrap();

        let cfg = Config::new(&symlink_cfg);
        assert_eq!(cfg.settings.read().wifi.hostname, "old-pacman-config");
        {
            let mut s = cfg.settings.write();
            s.wifi.hostname = "mario-castlevania-megaman".to_string();
        }
        assert!(cfg.save());

        // Symlink MUST remain a symlink and data/config.json MUST have the new content
        let meta = std::fs::symlink_metadata(&symlink_cfg).unwrap();
        assert!(meta.file_type().is_symlink());
        let reloaded = Config::new(&symlink_cfg);
        assert_eq!(
            reloaded.settings.read().wifi.hostname,
            "mario-castlevania-megaman"
        );
        let raw_data = std::fs::read_to_string(&data_cfg).unwrap();
        assert!(raw_data.contains("mario-castlevania-megaman"));
    }

    #[test]
    fn test_empty_api_token_regenerated() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("config.json");
        std::fs::write(&cfg_path, r#"{"api_token":""}"#).unwrap();
        let cfg = Config::new(&cfg_path);
        let token = cfg.settings.read().api_token.clone();
        assert_eq!(token.len(), 32);
    }
}

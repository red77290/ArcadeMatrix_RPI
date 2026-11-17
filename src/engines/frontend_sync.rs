use crate::core::config::{Config, MqttConfig};
use crate::core::engine_contract::{Engine, EngineConfig, EngineContext, EngineError};
use crate::core::i18n::{self, Lang};
use crate::core::matrix::IDrawingSurface;
use crate::core::types::{
    copy_str_to_pod, DisplayGeometry, ForcedEngineMode, InGameCycleMode, InGameShowcasePayload,
    SyncCelebrationPayload, SyncHiscoreEntryPod, SyncMetaPayload, SyncRaSummaryPod,
};
use crate::engines::clocks::sprite_bank::{self, CelebrationHeroFamily, SpriteId};
use crate::engines::dashboard::font::{draw_text_clipped, measure_text as measure_small_text};
use crate::engines::renderers::fireworks_lut::{FireworksLutRenderer, BOUNCE_LUT};
use crate::engines::renderers::media_showcase::{
    parse_host_and_port, probe_local_lan_ip, probe_showcase_health_details, resolve_showcase_host,
    resolve_showcase_server_url, rpi_hardware_short_id,
};
use crate::engines::renderers::primitives::{draw_fast_hline, draw_round_rect, fill_round_rect};
use crate::engines::renderers::tiny_font::{draw_tiny_text, tiny_text_width as measure_tiny_text};
use parking_lot::Mutex;
use rumqttc::{Client, Event, MqttOptions, Packet, QoS};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, OnceLock};
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

pub const MQTT_TOPIC_WILDCARD: &str = "system/playing/#";
pub const SHOWCASE_COMMAND_TOPIC_WILDCARD: &str = "dmd_showcase/clients/+/command";

static MQTT_REQUEST_ID: AtomicU64 = AtomicU64::new(0);
static CELEBRATION_EVENT_ID: AtomicU64 = AtomicU64::new(0);
static CONFIG_GENERATION: AtomicU64 = AtomicU64::new(1);
static LOCAL_MOSQUITTO_STOPPED_BY_SYNC: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedBrokerTarget {
    pub connect_host: String,
    pub lan_host: String,
    pub port: u16,
    pub source: &'static str,
    pub mqtt_enabled: bool,
}

#[derive(Clone)]
struct CachedBrokerProbe {
    showcase_url: String,
    use_showcase_broker: bool,
    showcase_enabled: bool,
    showcase_broker: Option<(String, u16)>,
    checked_at: Instant,
}

static BROKER_PROBE_CACHE: OnceLock<Mutex<Option<CachedBrokerProbe>>> = OnceLock::new();

#[derive(Debug, Clone, Default)]
struct RemoteSyncSessionState {
    req_id: u64,
    server_url: String,
    sync_client_id: String,
    /// Armed strictly when a `playing` event dispatches a Showcase sync job; consumed once on exit/preempt.
    /// Prevents HTTP `DELETE` traffic while scrolling frontend menus (`browsing` -> `browsing`).
    remote_sync_dirty: bool,
}

static ACTIVE_SYNC_QUEUE: OnceLock<Mutex<RemoteSyncSessionState>> = OnceLock::new();

#[derive(Debug, Clone, Default)]
pub struct E2eSyncTelemetry {
    pub mqtt_events_received: u64,
    pub last_mqtt_topic: String,
    pub last_status: String,
    pub last_system: String,
    pub last_game: String,
    pub active_marquee_path: String,
    pub showcase_enqueues: u64,
    pub showcase_streams_loaded: u64,
}

static E2E_TELEMETRY: OnceLock<Mutex<E2eSyncTelemetry>> = OnceLock::new();

fn e2e_telemetry() -> &'static Mutex<E2eSyncTelemetry> {
    E2E_TELEMETRY.get_or_init(|| Mutex::new(E2eSyncTelemetry::default()))
}

pub fn read_e2e_sync_telemetry() -> E2eSyncTelemetry {
    e2e_telemetry().lock().clone()
}

fn open_marquee_image(path: &std::path::Path) -> Option<image::RgbImage> {
    image::ImageReader::open(path)
        .ok()
        .and_then(|r| r.with_guessed_format().ok())
        .and_then(|r| r.decode().ok())
        .map(crate::engines::marquee::dynamic_image_to_rgb_on_black)
}

struct ReadySignalState {
    ready_events: Vec<(String, String, String)>, // (sync_client_id, queue_id, item_id)
}

static READY_SIGNAL: OnceLock<(std::sync::Mutex<ReadySignalState>, Condvar)> = OnceLock::new();

fn ready_signal() -> &'static (std::sync::Mutex<ReadySignalState>, Condvar) {
    READY_SIGNAL.get_or_init(|| {
        (
            std::sync::Mutex::new(ReadySignalState {
                ready_events: Vec::new(),
            }),
            Condvar::new(),
        )
    })
}

pub fn notify_mqtt_config_changed() {
    CONFIG_GENERATION.fetch_add(1, Ordering::SeqCst);
    if let Some(cache) = BROKER_PROBE_CACHE.get() {
        *cache.lock() = None;
    }
    let (_, cv) = ready_signal();
    cv.notify_all();
}

fn is_loopback_or_unspecified_host(host: &str) -> bool {
    let h = host.trim().to_ascii_lowercase();
    h.is_empty() || h == "127.0.0.1" || h == "localhost" || h == "0.0.0.0" || h == "::1"
}

fn detect_rpi_lan_ip() -> String {
    probe_local_lan_ip("8.8.8.8", 80)
        .or_else(|| probe_local_lan_ip("1.1.1.1", 80))
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

fn probe_showcase_mqtt_endpoint(raw_server_url: &str) -> Option<(String, u16)> {
    let resolved_url = resolve_showcase_server_url(raw_server_url);
    let (showcase_host, showcase_http_port) = parse_host_and_port(&resolved_url, 8890)?;
    let details = probe_showcase_health_details(&showcase_host, showcase_http_port)?;
    let broker_host = if !is_loopback_or_unspecified_host(&details.mqtt_host) {
        resolve_showcase_host(&details.mqtt_host)
    } else {
        showcase_host
    };
    Some((broker_host, details.mqtt_port))
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
fn manage_local_mosquitto_service(
    using_showcase_broker: bool,
    local_is_loopback: bool,
    mqtt_enabled: bool,
) {
    if crate::core::matrix::is_mock_matrix_mode() {
        return;
    }
    if using_showcase_broker && local_is_loopback {
        if !LOCAL_MOSQUITTO_STOPPED_BY_SYNC.swap(true, Ordering::SeqCst) {
            info!("[FrontendSync] Showcase MQTT broker active -> stopping local mosquitto.service to free RAM");
            let _ = std::process::Command::new("sudo")
                .args(["systemctl", "stop", "mosquitto"])
                .status();
        }
    } else if LOCAL_MOSQUITTO_STOPPED_BY_SYNC.swap(false, Ordering::SeqCst)
        && mqtt_enabled
        && local_is_loopback
    {
        info!("[FrontendSync] Falling back to local MQTT broker -> starting mosquitto.service");
        let _ = std::process::Command::new("sudo")
            .args(["systemctl", "start", "mosquitto"])
            .status();
    }
}

#[cfg(not(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64"))))]
fn manage_local_mosquitto_service(
    _using_showcase_broker: bool,
    _local_is_loopback: bool,
    _mqtt_enabled: bool,
) {
    let _ = &LOCAL_MOSQUITTO_STOPPED_BY_SYNC;
}

/// Resolves the active MQTT broker endpoint for both the local `FrontendSync` subscriber
/// and remote consoles querying `GET /api/mqtt/broker-target`.
pub fn resolve_broker_target(config: &Config, force_probe: bool) -> ResolvedBrokerTarget {
    let (
        mqtt_enabled,
        local_broker,
        local_port,
        use_showcase_broker,
        showcase_enabled,
        showcase_url,
    ) = {
        let s = config.settings.read();
        (
            s.mqtt.enabled,
            s.mqtt.broker.trim().to_string(),
            if s.mqtt.port > 0 { s.mqtt.port } else { 1883 },
            s.mqtt.use_showcase_broker,
            s.showcase.enabled,
            s.showcase.server_url.trim().to_string(),
        )
    };

    let local_is_loopback = is_loopback_or_unspecified_host(&local_broker);
    let connect_local = if local_broker.is_empty() {
        "127.0.0.1".to_string()
    } else {
        local_broker.clone()
    };
    let lan_local = if local_is_loopback {
        detect_rpi_lan_ip()
    } else {
        local_broker
    };

    if showcase_enabled && use_showcase_broker && !showcase_url.is_empty() {
        let cache_mutex = BROKER_PROBE_CACHE.get_or_init(|| Mutex::new(None));
        let cached = if !force_probe {
            let guard = cache_mutex.lock();
            guard.as_ref().and_then(|entry| {
                let ttl = if entry.showcase_broker.is_some() {
                    Duration::from_secs(10)
                } else {
                    Duration::from_secs(4)
                };
                if entry.showcase_enabled == showcase_enabled
                    && entry.use_showcase_broker == use_showcase_broker
                    && entry.showcase_url == showcase_url
                    && entry.checked_at.elapsed() < ttl
                {
                    Some(entry.showcase_broker.clone())
                } else {
                    None
                }
            })
        } else {
            None
        };

        let showcase_ep = match cached {
            Some(ep) => ep,
            None => {
                let ep = probe_showcase_mqtt_endpoint(&showcase_url);
                *cache_mutex.lock() = Some(CachedBrokerProbe {
                    showcase_url,
                    use_showcase_broker,
                    showcase_enabled,
                    showcase_broker: ep.clone(),
                    checked_at: Instant::now(),
                });
                ep
            }
        };

        if let Some((sc_host, sc_port)) = showcase_ep {
            let lan_sc_host = if is_loopback_or_unspecified_host(&sc_host) {
                detect_rpi_lan_ip()
            } else {
                sc_host.clone()
            };
            manage_local_mosquitto_service(true, local_is_loopback, mqtt_enabled);
            return ResolvedBrokerTarget {
                connect_host: sc_host,
                lan_host: lan_sc_host,
                port: sc_port,
                source: "showcase",
                mqtt_enabled,
            };
        }
    }

    manage_local_mosquitto_service(false, local_is_loopback, mqtt_enabled);
    ResolvedBrokerTarget {
        connect_host: connect_local,
        lan_host: lan_local,
        port: local_port,
        source: "local",
        mqtt_enabled,
    }
}

pub fn format_game_name(game: &str) -> String {
    let stripped = crate::core::dmd_cache::clean_system_name(game);
    let without_paren = match stripped.find(" (") {
        Some(idx) if idx > 0 => &stripped[..idx],
        _ => stripped.as_str(),
    };
    let without_tags = match without_paren.find(" [") {
        Some(idx) if idx > 0 => &without_paren[..idx],
        _ => without_paren,
    };
    let clean_name = without_tags.replace('-', " ").replace('_', " ");
    clean_name
        .split_whitespace()
        .map(|word| {
            let mut c = word.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn parse_cycle_mode(mqtt_cfg: &MqttConfig) -> InGameCycleMode {
    match mqtt_cfg
        .ingame_cycle_mode
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "once" => InGameCycleMode::Once,
        "loop" => InGameCycleMode::Loop,
        _ => InGameCycleMode::Alternating {
            marquee_interval_sec: mqtt_cfg.ingame_marquee_interval_sec.clamp(5, 3600),
        },
    }
}

pub fn build_ingame_queue_payload(
    mqtt_cfg: &MqttConfig,
    system: &str,
    game: &str,
    width: u32,
    height: u32,
    lang: &str,
) -> serde_json::Value {
    build_ingame_queue_payload_with_fallback(mqtt_cfg, system, game, None, width, height, lang)
}

pub fn build_ingame_queue_payload_with_fallback(
    mqtt_cfg: &MqttConfig,
    system: &str,
    game: &str,
    fallback_title: Option<&str>,
    width: u32,
    height: u32,
    lang: &str,
) -> serde_json::Value {
    let systems: Vec<&str> = if system.trim().is_empty() {
        Vec::new()
    } else {
        vec![system.trim()]
    };
    let clean_game = game.trim();
    let mut obj = serde_json::Map::new();
    obj.insert("domain".to_string(), json!("games"));
    obj.insert("systems".to_string(), json!(systems));
    obj.insert("query".to_string(), json!(clean_game));
    if let Some(raw_fb) = fallback_title {
        let fb = raw_fb.trim();
        if !fb.is_empty() && !fb.eq_ignore_ascii_case(clean_game) {
            obj.insert("fallback_query".to_string(), json!(fb));
        }
    }
    obj.insert("mode".to_string(), json!("play_now"));
    obj.insert("mark_seen".to_string(), json!(false));
    obj.insert("async_prepare".to_string(), json!(true));
    obj.insert("render_card_in_stream".to_string(), json!(true));
    obj.insert("width".to_string(), json!(width.max(32)));
    obj.insert("height".to_string(), json!(height.max(16)));
    obj.insert(
        "lang".to_string(),
        json!(if lang.trim().is_empty() {
            "en"
        } else {
            lang.trim()
        }),
    );

    if mqtt_cfg.card_style_source.eq_ignore_ascii_case("custom") {
        obj.insert(
            "include_card_intro".to_string(),
            json!(mqtt_cfg.include_card_intro),
        );
        obj.insert("card_layout".to_string(), json!(mqtt_cfg.card_layout));
        obj.insert("card_theme".to_string(), json!(mqtt_cfg.card_theme));
        obj.insert("include_poster".to_string(), json!(mqtt_cfg.include_poster));
        obj.insert(
            "include_synopsis".to_string(),
            json!(mqtt_cfg.include_synopsis),
        );
        obj.insert(
            "text_scroll_speed".to_string(),
            json!(mqtt_cfg.text_scroll_speed),
        );
        obj.insert(
            "jacket_panorama".to_string(),
            json!(mqtt_cfg.jacket_panorama),
        );
        obj.insert(
            "marquee_panorama".to_string(),
            json!(mqtt_cfg.marquee_panorama),
        );
        obj.insert(
            "snapshot_panorama_count".to_string(),
            json!(mqtt_cfg.snapshot_panorama_count.clamp(0, 5)),
        );
        obj.insert("include_video".to_string(), json!(mqtt_cfg.include_video));
        obj.insert(
            "video_max_duration_s".to_string(),
            json!(mqtt_cfg.video_max_duration_s.clamp(2, 120)),
        );
        obj.insert(
            "video_fps".to_string(),
            json!(mqtt_cfg.video_fps.clamp(6, 60)),
        );
        obj.insert("video_mode".to_string(), json!(mqtt_cfg.video_mode));
        obj.insert("color_boost".to_string(), json!(mqtt_cfg.color_boost));
    }

    serde_json::Value::Object(obj)
}

fn delete_showcase_client_queue_async(server_url: String, sync_client_id: String) {
    if server_url.trim().is_empty() || sync_client_id.trim().is_empty() {
        return;
    }
    std::thread::spawn(move || {
        let url = format!(
            "{}/api/v1/clients/{}/queue",
            server_url.trim_end_matches('/'),
            sync_client_id
        );
        let client = crate::core::memory::fast_http_client();
        let _ = client
            .delete(&url)
            .timeout(Duration::from_millis(1500))
            .send();
    });
}

fn take_dirty_remote_sync_target(matching_req_id: Option<u64>) -> Option<(String, String)> {
    let slot = ACTIVE_SYNC_QUEUE.get_or_init(|| Mutex::new(RemoteSyncSessionState::default()));
    let mut guard = slot.lock();
    if !guard.remote_sync_dirty {
        return None;
    }
    if let Some(expected) = matching_req_id {
        if guard.req_id != expected {
            return None;
        }
    }
    guard.remote_sync_dirty = false;
    if guard.server_url.is_empty() || guard.sync_client_id.is_empty() {
        return None;
    }
    Some((guard.server_url.clone(), guard.sync_client_id.clone()))
}

pub fn cancel_active_ingame_showcase(config: &Config) {
    config.set_ingame_showcase(None);
    config.set_sync_meta(None);
    if let Some((server_url, sync_client_id)) = take_dirty_remote_sync_target(None) {
        delete_showcase_client_queue_async(server_url, sync_client_id);
    }
    let (_, cv) = ready_signal();
    cv.notify_all();
}

/// Called from `app.rs` when `InGameCycleMode::Once` completes so the server-side temporary queue is also purged.
pub fn notify_ingame_once_completed(session_token: u64) {
    if let Some((server_url, sync_client_id)) = take_dirty_remote_sync_target(Some(session_token)) {
        delete_showcase_client_queue_async(server_url, sync_client_id);
    }
}

fn record_showcase_command_event(topic: &str, payload: &str) {
    // Topic format: dmd_showcase/clients/{sync_client_id}/command
    let parts: Vec<&str> = topic.split('/').collect();
    if parts.len() != 4
        || parts[0] != "dmd_showcase"
        || parts[1] != "clients"
        || parts[3] != "command"
    {
        return;
    }
    let sync_client_id = parts[2];
    let local_prefix = format!("rpi_{}_", rpi_hardware_short_id());
    if !sync_client_id.starts_with(&local_prefix) || !sync_client_id.ends_with("_sync") {
        return;
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) else {
        return;
    };
    if v.get("action").and_then(|s| s.as_str()) != Some("play_now") {
        return;
    }
    let Some(item_id) = v
        .get("item_id")
        .and_then(|s| s.as_str())
        .filter(|s| !s.is_empty())
    else {
        return;
    };
    let queue_id = v
        .get("queue_id")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();

    let (lock, cv) = ready_signal();
    if let Ok(mut guard) = lock.lock() {
        if guard.ready_events.len() >= 16 {
            guard.ready_events.remove(0);
        }
        guard
            .ready_events
            .push((sync_client_id.to_string(), queue_id, item_id.to_string()));
        cv.notify_all();
    }
}

fn take_ready_signal(sync_client_id: &str, queue_id: &str) -> Option<String> {
    let (lock, _) = ready_signal();
    let mut guard = lock.lock().ok()?;
    if let Some(pos) = guard.ready_events.iter().position(|(cid, qid, _)| {
        cid == sync_client_id && (qid.is_empty() || queue_id.is_empty() || qid == queue_id)
    }) {
        let (_, _, item_id) = guard.ready_events.remove(pos);
        return Some(item_id);
    }
    None
}

fn spawn_ingame_showcase_request(
    config: Arc<Config>,
    req_id: u64,
    system: String,
    game: String,
    fallback_title: String,
) {
    std::thread::spawn(move || {
        let (showcase_enabled, raw_server_url, mqtt_cfg, width, height, lang) = {
            let s = config.settings.read();
            let phys_w = s.matrix.width.saturating_mul(s.matrix.chain_length.max(1));
            let phys_h = s
                .matrix
                .height
                .saturating_mul(s.matrix.parallel.clamp(1, 3));
            let rot = crate::app::normalize_rotation(s.matrix.rotation);
            let geom = DisplayGeometry::new(phys_w, phys_h, rot, 0);
            (
                s.showcase.enabled,
                s.showcase.server_url.clone(),
                s.mqtt.clone(),
                geom.width(),
                geom.height(),
                if s.system.lang.is_empty() {
                    "en".to_string()
                } else {
                    s.system.lang.clone()
                },
            )
        };

        if !showcase_enabled
            || !mqtt_cfg.ingame_showcase_enabled
            || raw_server_url.trim().is_empty()
        {
            return;
        }
        if MQTT_REQUEST_ID.load(Ordering::Acquire) != req_id {
            return;
        }

        let resolved_url = resolve_showcase_server_url(&raw_server_url);
        if resolved_url.is_empty() {
            return;
        }
        let sync_client_id = format!(
            "rpi_{}_games_{}x{}_sync",
            rpi_hardware_short_id(),
            width,
            height
        );

        // Arm `remote_sync_dirty` BEFORE the HTTP POST so even an immediate game exit (< 100 ms)
        // purges the server queue and terminates any spawned converter worker process.
        {
            let slot =
                ACTIVE_SYNC_QUEUE.get_or_init(|| Mutex::new(RemoteSyncSessionState::default()));
            let mut guard = slot.lock();
            if MQTT_REQUEST_ID.load(Ordering::Acquire) != req_id {
                return;
            }
            guard.req_id = req_id;
            guard.server_url = resolved_url.clone();
            guard.sync_client_id = sync_client_id.clone();
            guard.remote_sync_dirty = true;
        }

        let enqueue_url = format!(
            "{}/api/v1/clients/{}/queue/async",
            resolved_url.trim_end_matches('/'),
            sync_client_id
        );
        let fb_opt = if fallback_title.trim().is_empty() {
            None
        } else {
            Some(fallback_title.as_str())
        };
        let body = build_ingame_queue_payload_with_fallback(
            &mqtt_cfg, &system, &game, fb_opt, width, height, &lang,
        );
        let cycle_mode = parse_cycle_mode(&mqtt_cfg);

        let api_client = crate::core::memory::api_http_client();
        let resp = match api_client
            .post(&enqueue_url)
            .timeout(Duration::from_secs(4))
            .json(&body)
            .send()
        {
            Ok(r) if r.status().is_success() => {
                e2e_telemetry().lock().showcase_enqueues += 1;
                r
            }
            Ok(r) => {
                let _ = take_dirty_remote_sync_target(Some(req_id));
                warn!(
                    "[FrontendSync] Showcase queue/async returned HTTP {} for {}/{}",
                    r.status(),
                    system,
                    game
                );
                return;
            }
            Err(e) => {
                let _ = take_dirty_remote_sync_target(Some(req_id));
                warn!("[FrontendSync] Showcase queue/async unreachable: {}", e);
                return;
            }
        };

        // If the game was exited while the HTTP POST was in flight, ensure the server queue is cleared!
        if MQTT_REQUEST_ID.load(Ordering::Acquire) != req_id {
            delete_showcase_client_queue_async(resolved_url, sync_client_id);
            return;
        }

        let Ok(resp_json) = resp.json::<serde_json::Value>() else {
            return;
        };
        let queue_item = resp_json.get("item").unwrap_or(&resp_json);
        let queue_id = queue_item
            .get("queue_id")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        if queue_id.is_empty() {
            return;
        }

        let mut ready_item_id: Option<String> = None;
        let initial_status = queue_item
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("");
        if initial_status == "ready" || initial_status == "playing" {
            ready_item_id = queue_item
                .get("item_id")
                .and_then(|s| s.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
        } else if initial_status == "error" {
            let _ = take_dirty_remote_sync_target(Some(req_id));
            return;
        }

        // Wait for async preparation (0 ms on MQTT `play_now` signal, or <= 750 ms via lightweight queue poll)
        let wait_deadline = Instant::now() + Duration::from_secs(120);
        let queue_poll_url = format!(
            "{}/api/v1/clients/{}/queue",
            resolved_url.trim_end_matches('/'),
            sync_client_id
        );
        while ready_item_id.is_none() && Instant::now() < wait_deadline {
            if MQTT_REQUEST_ID.load(Ordering::Acquire) != req_id {
                return;
            }
            if let Some(id) = take_ready_signal(&sync_client_id, &queue_id) {
                ready_item_id = Some(id);
                break;
            }
            {
                let (lock, cv) = ready_signal();
                if let Ok(guard) = lock.lock() {
                    let _ = cv.wait_timeout(guard, Duration::from_millis(750));
                }
            }
            if MQTT_REQUEST_ID.load(Ordering::Acquire) != req_id {
                return;
            }
            if let Some(id) = take_ready_signal(&sync_client_id, &queue_id) {
                ready_item_id = Some(id);
                break;
            }
            if let Ok(poll_resp) = api_client
                .get(&queue_poll_url)
                .timeout(Duration::from_millis(1200))
                .send()
            {
                if poll_resp.status().is_success() {
                    if let Ok(poll_json) = poll_resp.json::<serde_json::Value>() {
                        if let Some(items) = poll_json
                            .get("items")
                            .and_then(|v| v.as_array())
                            .or_else(|| poll_json.as_array())
                        {
                            let matched = items.iter().find(|it| {
                                it.get("queue_id").and_then(|s| s.as_str())
                                    == Some(queue_id.as_str())
                            });
                            match matched {
                                Some(it) => {
                                    let st =
                                        it.get("status").and_then(|s| s.as_str()).unwrap_or("");
                                    if st == "ready" || st == "playing" {
                                        if let Some(id) = it
                                            .get("item_id")
                                            .and_then(|s| s.as_str())
                                            .filter(|s| !s.is_empty())
                                        {
                                            ready_item_id = Some(id.to_string());
                                            break;
                                        }
                                    } else if st == "error" {
                                        let _ = take_dirty_remote_sync_target(Some(req_id));
                                        warn!(
                                            "[FrontendSync] Showcase failed to prepare {}/{}: {:?}",
                                            system,
                                            game,
                                            it.get("error_msg")
                                        );
                                        return;
                                    }
                                }
                                None => {
                                    let _ = take_dirty_remote_sync_target(Some(req_id));
                                    warn!(
                                        "[FrontendSync] Showcase found no scraper match for {}/{} (fallback='{}'); keeping marquee.",
                                        system, game, fallback_title
                                    );
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }

        let Some(item_id) = ready_item_id else {
            if MQTT_REQUEST_ID.load(Ordering::Acquire) == req_id {
                warn!(
                    "[FrontendSync] Showcase preparation timed out after 120s for {}/{}; aborting server conversion and keeping marquee.",
                    system, game
                );
                if let Some((srv, cid)) = take_dirty_remote_sync_target(Some(req_id)) {
                    delete_showcase_client_queue_async(srv, cid);
                }
            }
            return;
        };
        if MQTT_REQUEST_ID.load(Ordering::Acquire) != req_id {
            return;
        }

        // Download compact lossless `DMDZ` v2 stream (`purge=false&codec=dmdz2`, with automatic
        // `DMDS` fallback) so alternating/loop cycles replay locally from RAM with 0 network load.
        let stream_path = format!("/api/v1/games/stream/{}.rgb565.stream", item_id);
        let Some(stream) = crate::core::dmd_stream::fetch_showcase_stream(
            &resolved_url,
            &stream_path,
            Some(&sync_client_id),
            false,
            Some(Duration::from_secs(20)),
        ) else {
            warn!(
                "[FrontendSync] Invalid or unreachable DMDZ/DMDS stream for item {}",
                item_id
            );
            return;
        };

        if MQTT_REQUEST_ID.load(Ordering::Acquire) != req_id {
            return;
        }

        info!(
            "[FrontendSync] Late preemption ready for {}/{} ({} frames, {}, {} B, {:?})",
            system,
            game,
            stream.frame_count,
            if stream.is_compressed_dmdz() {
                "DMDZ"
            } else {
                "DMDS"
            },
            stream.payload_byte_len(),
            cycle_mode
        );
        crate::core::matrix::reset_mock_distinct_crcs();
        e2e_telemetry().lock().showcase_streams_loaded += 1;
        config.set_ingame_showcase(Some(InGameShowcasePayload {
            session_token: req_id,
            stream,
            cycle_mode,
        }));
    });
}

pub fn parse_sync_meta_from_json(
    json: &serde_json::Value,
    session_token: u64,
    mqtt_cfg: &MqttConfig,
) -> Option<SyncMetaPayload> {
    let raw_system = json["system"].as_str().unwrap_or("");
    let raw_game = json["title"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| json["game"].as_str())
        .unwrap_or("");
    let clean_sys = crate::core::dmd_cache::clean_system_name(raw_system);
    let game_title = format_game_name(raw_game);

    let mut hiscores = [SyncHiscoreEntryPod::EMPTY; 5];
    let mut hiscore_count = 0u8;
    if let Some(arr) = json.get("hiscores").and_then(|v| v.as_array()) {
        for (idx, item) in arr.iter().take(5).enumerate() {
            let rank = item
                .get("rank")
                .and_then(|r| r.as_u64())
                .map(|r| r.clamp(1, 99) as u8)
                .unwrap_or((idx + 1) as u8);
            let player = item
                .get("tag")
                .or_else(|| item.get("player"))
                .or_else(|| item.get("name"))
                .and_then(|s| s.as_str())
                .unwrap_or("1UP");
            let score_owned: String;
            let score_str = if let Some(s) = item.get("score").and_then(|v| v.as_str()) {
                s
            } else if let Some(n) = item
                .get("score")
                .or_else(|| item.get("raw"))
                .and_then(|v| v.as_u64())
            {
                score_owned = n.to_string();
                &score_owned
            } else {
                ""
            };
            if !score_str.trim().is_empty() {
                hiscores[hiscore_count as usize] =
                    SyncHiscoreEntryPod::from_strs(rank, player, score_str);
                hiscore_count += 1;
            }
        }
    }

    let mut ra = SyncRaSummaryPod::EMPTY;
    if let Some(ra_obj) = json.get("ra").and_then(|v| v.as_object()) {
        let total = ra_obj
            .get("total")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            .min(u16::MAX as u64) as u16;
        if total > 0 {
            let unlocked = ra_obj
                .get("unlocked")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                .min(total as u64) as u16;
            let points_earned = ra_obj
                .get("points")
                .or_else(|| ra_obj.get("points_earned"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                .min(u32::MAX as u64) as u32;
            let points_total = ra_obj
                .get("points_total")
                .and_then(|v| v.as_u64())
                .unwrap_or(points_earned as u64)
                .min(u32::MAX as u64) as u32;
            let hardcore = ra_obj
                .get("hardcore")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let recent = ra_obj
                .get("recent_title")
                .or_else(|| ra_obj.get("recent_badge"))
                .or_else(|| ra_obj.get("recent").and_then(|r| r.get("title")))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let ra_game_title = ra_obj
                .get("game_title")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(&game_title);
            ra = SyncRaSummaryPod {
                game_title: copy_str_to_pod(ra_game_title.trim()),
                unlocked,
                total,
                points_earned,
                points_total,
                hardcore,
                recent_badge: copy_str_to_pod(recent.trim()),
                valid: true,
            };
        }
    }

    let payload = SyncMetaPayload {
        session_token: session_token.max(1),
        system: copy_str_to_pod(clean_sys.trim()),
        game_title: copy_str_to_pod(game_title.trim()),
        hiscore_count,
        hiscores,
        ra,
        show_hiscores: mqtt_cfg.sync_show_hiscores,
        show_ra: mqtt_cfg.sync_show_ra,
        card_duration_sec: mqtt_cfg.sync_card_duration_sec.clamp(5, 20),
    };

    if payload.has_any_card() {
        Some(payload)
    } else {
        None
    }
}

pub fn parse_celebration_from_json(
    json: &serde_json::Value,
    event_token: u64,
) -> SyncCelebrationPayload {
    let kind_str = json
        .get("kind")
        .or_else(|| json.get("celebration_type"))
        .and_then(|v| v.as_str())
        .unwrap_or("highscore");
    let is_achievement =
        kind_str.eq_ignore_ascii_case("achievement") || kind_str.eq_ignore_ascii_case("ra");
    let raw_system = json["system"].as_str().unwrap_or("");
    let clean_sys = crate::core::dmd_cache::clean_system_name(raw_system);
    let raw_game = json["game"].as_str().unwrap_or("");
    let game_title = format_game_name(raw_game);

    if is_achievement {
        let title = json
            .get("title")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("ACHIEVEMENT");
        let pts = json.get("points").and_then(|v| v.as_u64()).unwrap_or(0);
        let pts_label = if pts > 0 {
            format!("+{} PTS", pts)
        } else {
            "UNLOCKED".to_string()
        };
        let hardcore = json
            .get("hardcore")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let duration_ms = json
            .get("duration_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(7500)
            .clamp(1000, 15000) as u32;
        SyncCelebrationPayload {
            event_token: event_token.max(1),
            kind: 1,
            system: copy_str_to_pod(clean_sys.trim()),
            title: copy_str_to_pod(title.trim()),
            subtitle: copy_str_to_pod(game_title.trim()),
            score_or_pts: copy_str_to_pod(&pts_label),
            hardcore,
            duration_ms,
        }
    } else {
        let rank = json
            .get("rank")
            .and_then(|v| v.as_u64())
            .unwrap_or(1)
            .clamp(1, 99);
        let tag = json
            .get("tag")
            .or_else(|| json.get("player"))
            .and_then(|v| v.as_str())
            .unwrap_or("1UP");
        let score = if let Some(s) = json.get("score").and_then(|v| v.as_str()) {
            s.to_string()
        } else if let Some(n) = json.get("score").and_then(|v| v.as_u64()) {
            n.to_string()
        } else {
            "999,999".to_string()
        };
        let rank_tag = format!("#{} {}", rank, tag.trim());
        let title_str = if !game_title.is_empty() {
            game_title
        } else {
            rank_tag.clone()
        };
        let duration_ms = json
            .get("duration_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(7500)
            .clamp(1000, 15000) as u32;
        SyncCelebrationPayload {
            event_token: event_token.max(1),
            kind: 0,
            system: copy_str_to_pod(clean_sys.trim()),
            title: copy_str_to_pod(&rank_tag),
            subtitle: copy_str_to_pod(title_str.trim()),
            score_or_pts: copy_str_to_pod(score.trim()),
            hardcore: false,
            duration_ms,
        }
    }
}

pub fn handle_frontend_sync_payload(
    config: &Arc<Config>,
    dmd_cache: &Arc<crate::core::dmd_cache::DmdCache>,
    payload: &str,
) {
    info!("MQTT game payload: {}", payload);
    let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) else {
        return;
    };
    let status = json["status"]
        .as_str()
        .or_else(|| json["event"].as_str())
        .unwrap_or("stopped");
    {
        let mut t = e2e_telemetry().lock();
        t.mqtt_events_received += 1;
        t.last_status = status.to_string();
    }

    if status == "celebrate" || json["type"].as_str() == Some("celebrate") {
        let celebrate_enabled = config.settings.read().mqtt.sync_celebrate_events;
        if celebrate_enabled {
            let evt_id = CELEBRATION_EVENT_ID.fetch_add(1, Ordering::AcqRel) + 1;
            let cel = parse_celebration_from_json(&json, evt_id);
            config.set_sync_celebration(Some(cel));
        }
        return;
    }

    if status == "sync_meta" {
        let mqtt_cfg = config.settings.read().mqtt.clone();
        let active_req_id = MQTT_REQUEST_ID.load(Ordering::Acquire).max(1);
        if let Some(meta) = parse_sync_meta_from_json(&json, active_req_id, &mqtt_cfg) {
            config.set_sync_meta(Some(meta));
        }
        return;
    }

    if status == "stopped" {
        MQTT_REQUEST_ID.fetch_add(1, Ordering::AcqRel);
        cancel_active_ingame_showcase(config);
        info!("Received stopped event, cancelled in-game Showcase and keeping last marquee displayed.");
        return;
    }

    let raw_system = json["system"].as_str().unwrap_or("").to_string();
    let raw_game = json["game"].as_str().unwrap_or("").to_string();
    let raw_title = json["title"].as_str().unwrap_or("").trim().to_string();
    let rom_path = json["rom_path"]
        .as_str()
        .or_else(|| json["path"].as_str())
        .unwrap_or("")
        .trim();

    let initial_clean_sys = crate::core::dmd_cache::clean_system_name(&raw_system);
    let clean_game = crate::core::dmd_cache::clean_system_name(&raw_game);

    let is_playing = status == "playing";
    let is_system_event = !is_playing
        && (json["type"].as_str() == Some("system")
            || clean_game.is_empty()
            || clean_game.eq_ignore_ascii_case(&initial_clean_sys));

    let effective_raw_sys = if !is_system_event {
        crate::core::dmd_cache::extract_system_from_rom_path(&raw_system, rom_path)
    } else {
        raw_system
    };
    let clean_sys = crate::core::dmd_cache::clean_system_name(&effective_raw_sys);

    let system = if !clean_sys.is_empty() {
        clean_sys
    } else {
        effective_raw_sys
    };
    let game_clean = if !clean_game.is_empty() {
        clean_game
    } else {
        raw_game.clone()
    };

    let req_id = MQTT_REQUEST_ID.fetch_add(1, Ordering::AcqRel) + 1;
    {
        let mut t = e2e_telemetry().lock();
        t.last_system = system.clone();
        t.last_game = game_clean.clone();
        t.active_marquee_path.clear();
    }
    // Any new browsing or playing event cancels any previously active in-game Showcase session
    cancel_active_ingame_showcase(config);

    if is_system_event {
        if let Some(path) = dmd_cache.get_cached_system_path(&system) {
            if let Some(img) = open_marquee_image(&path) {
                e2e_telemetry().lock().active_marquee_path = path.to_string_lossy().to_string();
                config.set_marquee_image(Some(img));
                config.set_forced_engine_mode(ForcedEngineMode::Marquee);
                return;
            }
        }

        config.set_marquee_image(None);
        let formatted_sys = format_game_name(&crate::core::dmd_cache::clean_system_name(&system));
        let clean_name = if formatted_sys.is_empty() {
            "Unknown".to_string()
        } else {
            formatted_sys
        };
        let msg_payload = build_cyan_sync_banner(config, &clean_name);
        config.set_frontend_sync_banner(msg_payload);

        if !dmd_cache.is_system_negative_cached(&system) {
            let config_clone = Arc::clone(config);
            let cache_clone = Arc::clone(dmd_cache);
            std::thread::spawn(move || {
                if let Some(path) = cache_clone.download_system_marquee(&system) {
                    if let Some(img) = open_marquee_image(&path) {
                        if MQTT_REQUEST_ID.load(Ordering::Acquire) == req_id {
                            e2e_telemetry().lock().active_marquee_path =
                                path.to_string_lossy().to_string();
                            config_clone.set_marquee_image(Some(img));
                            config_clone.set_forced_engine_mode(ForcedEngineMode::Marquee);
                        }
                    }
                }
            });
        }
    } else {
        let game = if !raw_game.is_empty() {
            raw_game
        } else {
            "Unknown".to_string()
        };
        let title_source = if !raw_title.is_empty() {
            raw_title.as_str()
        } else {
            game.as_str()
        };
        let formatted_title = format_game_name(title_source);
        let clean_name = if formatted_title.is_empty() {
            "Unknown".to_string()
        } else {
            formatted_title
        };

        // 1. Display local cached game marquee immediately (< 15 ms), or show the cyan game name
        //    banner and attempt Pixelcade game marquee download (never falling back to system art).
        let mut marquee_shown = false;
        if let Some(path) = dmd_cache.get_cached_path(&system, &game) {
            if let Some(img) = open_marquee_image(&path) {
                e2e_telemetry().lock().active_marquee_path = path.to_string_lossy().to_string();
                config.set_marquee_image(Some(img));
                config.set_forced_engine_mode(ForcedEngineMode::Marquee);
                marquee_shown = true;
            }
        }

        if !marquee_shown {
            config.set_marquee_image(None);
            let msg_payload = build_cyan_sync_banner(config, &clean_name);
            config.set_frontend_sync_banner(msg_payload);

            if !dmd_cache.is_negative_cached(&system, &game) {
                let config_clone = Arc::clone(config);
                let cache_clone = Arc::clone(dmd_cache);
                let sys_for_dl = system.clone();
                let game_for_dl = game.clone();
                let title_for_dl = if raw_title.is_empty() {
                    None
                } else {
                    Some(raw_title.clone())
                };
                std::thread::spawn(move || {
                    if let Some(path) = cache_clone.download_marquee_with_title(
                        &sys_for_dl,
                        &game_for_dl,
                        title_for_dl.as_deref(),
                    ) {
                        if let Some(img) = open_marquee_image(&path) {
                            if MQTT_REQUEST_ID.load(Ordering::Acquire) == req_id {
                                e2e_telemetry().lock().active_marquee_path =
                                    path.to_string_lossy().to_string();
                                config_clone.set_marquee_image(Some(img));
                                config_clone.set_forced_engine_mode(ForcedEngineMode::Marquee);
                            }
                        }
                    }
                });
            }
        }

        // 2. When launching a game (`status == "playing"`), request Showcase sequence in background for Late Preemption
        if is_playing && !game_clean.is_empty() {
            let mqtt_cfg = config.settings.read().mqtt.clone();
            if let Some(meta) = parse_sync_meta_from_json(&json, req_id, &mqtt_cfg) {
                config.set_sync_meta(Some(meta));
            }
            spawn_ingame_showcase_request(
                Arc::clone(config),
                req_id,
                system,
                game_clean,
                raw_title,
            );
        }
    }
}

pub fn build_cyan_sync_banner(
    config: &Config,
    clean_name: &str,
) -> crate::engines::message::MessagePayload {
    let (eff_w, eff_h) = {
        let s = config.settings.read();
        let phys_w = s.matrix.width.saturating_mul(s.matrix.chain_length.max(1));
        let phys_h = s
            .matrix
            .height
            .saturating_mul(s.matrix.parallel.clamp(1, 3));
        let rot = crate::app::normalize_rotation(s.matrix.rotation);
        let geom = DisplayGeometry::new(phys_w, phys_h, rot, 0);
        (geom.width(), geom.height())
    };
    let char_count = clean_name.chars().count();
    if eff_w >= 192 && eff_h >= 48 {
        let max_static_2x = (eff_w.saturating_sub(8) / 16) as usize;
        let max_static_1x = (eff_w.saturating_sub(8) / 8) as usize;
        if char_count <= max_static_2x {
            crate::engines::message::MessagePayload::new(
                clean_name.to_string(),
                "#00ffff",
                2,
                "none",
                0,
            )
        } else if char_count <= max_static_1x {
            crate::engines::message::MessagePayload::new(
                clean_name.to_string(),
                "#00ffff",
                1,
                "none",
                0,
            )
        } else {
            crate::engines::message::MessagePayload::new(
                format!(" {} ", clean_name),
                "#00ffff",
                2,
                "left",
                0,
            )
        }
    } else {
        let mut text_to_show = clean_name.to_string();
        if char_count > 10 {
            text_to_show = format!(" {} ", clean_name);
        }
        crate::engines::message::MessagePayload::new(
            text_to_show,
            "#00ffff",
            1,
            if char_count > 8 { "left" } else { "none" },
            0,
        )
    }
}

pub fn start_mqtt_client(config: Arc<Config>) {
    std::thread::Builder::new()
        .name("frontend-sync".to_string())
        .spawn(move || {
            let dmd_cache = Arc::new(crate::core::dmd_cache::DmdCache::new("data/marquees"));
            let mut shown_waiting_banner = false;
            let mut conn_seq: u32 = 0;

            loop {
                let target = resolve_broker_target(&config, false);
                if !target.mqtt_enabled {
                    shown_waiting_banner = false;
                    std::thread::sleep(Duration::from_secs(2));
                    continue;
                }

                let (user, pass, want_showcase_broker) = {
                    let s = config.settings.read();
                    (
                        s.mqtt.user.clone(),
                        s.mqtt.pass.clone(),
                        s.showcase.enabled
                            && s.mqtt.use_showcase_broker
                            && !s.showcase.server_url.trim().is_empty(),
                    )
                };
                let observed_cfg_gen = CONFIG_GENERATION.load(Ordering::SeqCst);

                if !shown_waiting_banner && MQTT_REQUEST_ID.load(Ordering::Acquire) == 0 {
                    shown_waiting_banner = true;
                    let msg_payload = crate::engines::message::MessagePayload::new(
                        "WAITING FOR MARQUEE".to_string(),
                        "#ffffff",
                        1,
                        "none",
                        0,
                    );
                    config.set_frontend_sync_banner(msg_payload);
                }

                conn_seq = conn_seq.wrapping_add(1);
                let client_id = format!(
                    "arcadematrix_rpi_{}_{}",
                    rpi_hardware_short_id(),
                    conn_seq % 10_000
                );
                info!(
                    "[FrontendSync] Connecting to MQTT broker {}:{} (source: {})",
                    target.connect_host, target.port, target.source
                );

                let mut mqttoptions =
                    MqttOptions::new(client_id, &target.connect_host, target.port);
                mqttoptions.set_keep_alive(Duration::from_secs(15));
                mqttoptions.set_max_packet_size(65536, 65536);
                if target.source == "local" && !user.is_empty() {
                    mqttoptions.set_credentials(&user, &pass);
                }

                let (client, mut connection) = Client::new(mqttoptions, 64);

                if let Err(e) = client.subscribe(MQTT_TOPIC_WILDCARD, QoS::AtMostOnce) {
                    error!(
                        "Failed to subscribe to MQTT topic {}: {}",
                        MQTT_TOPIC_WILDCARD, e
                    );
                    std::thread::sleep(Duration::from_secs(3));
                    continue;
                }
                let _ = client.subscribe(SHOWCASE_COMMAND_TOPIC_WILDCARD, QoS::AtMostOnce);

                // Off-thread config/recovery watcher:
                // - Disconnects the client immediately (< 50 ms) when Web UI settings change.
                // - Only probes Showcase HTTP `/api/v1/health` when currently running on local fallback
                //   while `want_showcase_broker` is true, so an active Showcase MQTT TCP session never
                //   stalls on HTTP health polling!
                let session_alive = Arc::new(AtomicBool::new(true));
                let session_alive_watch = Arc::clone(&session_alive);
                let client_watch = client.clone();
                let config_watch = Arc::clone(&config);
                let target_watch = target.clone();
                std::thread::spawn(move || {
                    let mut last_probe = Instant::now();
                    let (lock, cv) = ready_signal();
                    while session_alive_watch.load(Ordering::Acquire) {
                        if CONFIG_GENERATION.load(Ordering::SeqCst) != observed_cfg_gen {
                            info!("[FrontendSync] MQTT/Showcase configuration changed, reconnecting...");
                            let _ = client_watch.disconnect();
                            break;
                        }
                        if target_watch.source == "local"
                            && want_showcase_broker
                            && last_probe.elapsed() >= Duration::from_secs(12)
                        {
                            last_probe = Instant::now();
                            let refreshed = resolve_broker_target(&config_watch, false);
                            if !refreshed.mqtt_enabled
                                || refreshed.connect_host != target_watch.connect_host
                                || refreshed.port != target_watch.port
                                || refreshed.source != target_watch.source
                            {
                                info!(
                                    "[FrontendSync] Showcase broker became reachable ({}:{} -> {}:{}), switching...",
                                    target_watch.connect_host,
                                    target_watch.port,
                                    refreshed.connect_host,
                                    refreshed.port
                                );
                                let _ = client_watch.disconnect();
                                break;
                            }
                        }
                        if let Ok(guard) = lock.lock() {
                            let _ = cv.wait_timeout(guard, Duration::from_millis(500));
                        } else {
                            std::thread::sleep(Duration::from_millis(500));
                        }
                    }
                });

                for notification in connection.iter() {
                    if CONFIG_GENERATION.load(Ordering::SeqCst) != observed_cfg_gen {
                        let _ = client.disconnect();
                        break;
                    }

                    match notification {
                        Ok(Event::Incoming(Packet::Publish(publish))) => {
                            let payload = String::from_utf8_lossy(&publish.payload);
                            if publish.topic.starts_with("dmd_showcase/clients/") {
                                record_showcase_command_event(&publish.topic, &payload);
                            } else {
                                e2e_telemetry().lock().last_mqtt_topic = publish.topic.clone();
                                handle_frontend_sync_payload(&config, &dmd_cache, &payload);
                            }
                        }
                        Err(e) => {
                            error!(
                                "[FrontendSync] MQTT connection error on {}:{} ({}): {}",
                                target.connect_host, target.port, target.source, e
                            );
                            // Invalidate cached Showcase probe so fallback to local broker happens immediately
                            if let Some(cache) = BROKER_PROBE_CACHE.get() {
                                *cache.lock() = None;
                            }
                            break;
                        }
                        _ => {}
                    }
                }

                session_alive.store(false, Ordering::Release);
                ready_signal().1.notify_all();
                std::thread::sleep(Duration::from_secs(2));
            }
        })
        .expect("Failed to spawn frontend-sync thread");
}

// ----------------------------------------------------------------------------
// Zero-Allocation Stack Formatting & Clipped Text Helpers
// ----------------------------------------------------------------------------

struct StackStrBuf<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> StackStrBuf<N> {
    #[inline]
    const fn new() -> Self {
        Self {
            buf: [0u8; N],
            len: 0,
        }
    }

    #[inline]
    fn push_str(&mut self, s: &str) {
        let bytes = s.as_bytes();
        let avail = N.saturating_sub(self.len);
        let n = bytes.len().min(avail);
        if n > 0 {
            self.buf[self.len..self.len + n].copy_from_slice(&bytes[..n]);
            self.len += n;
        }
    }

    #[inline]
    fn push_u32(&mut self, mut val: u32) {
        if val == 0 {
            self.push_str("0");
            return;
        }
        let mut tmp = [0u8; 10];
        let mut idx = 10usize;
        while val > 0 && idx > 0 {
            idx -= 1;
            tmp[idx] = b'0' + (val % 10) as u8;
            val /= 10;
        }
        if let Ok(s) = std::str::from_utf8(&tmp[idx..]) {
            self.push_str(s);
        }
    }

    #[inline]
    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

#[inline]
fn rank_color_rgb(rank: u8) -> (u8, u8, u8) {
    match rank {
        1 => (255, 215, 0),  // Gold
        2 => (0, 229, 255),  // Cyan / Silver
        3 => (255, 140, 0),  // Bronze / Orange
        4 => (255, 80, 220), // Neon Magenta
        _ => (80, 255, 120), // Neon Green
    }
}

#[inline]
fn rank_label_short(rank: u8) -> &'static str {
    match rank {
        1 => "1ST",
        2 => "2ND",
        3 => "3RD",
        4 => "4TH",
        5 => "5TH",
        _ => "TOP",
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_tiny_text_clipped_bounds(
    surface: &mut dyn IDrawingSurface,
    x: i32,
    y: i32,
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
    text: &str,
    color: (u8, u8, u8),
) {
    if y < min_y || y + 4 > max_y {
        return;
    }
    draw_tiny_text(surface, text, x, y, min_x, max_x + 1, color);
}

#[allow(clippy::too_many_arguments)]
fn draw_small_text_clipped_bounds(
    surface: &mut dyn IDrawingSurface,
    x: i32,
    y: i32,
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
    text: &str,
    color: (u8, u8, u8),
) {
    draw_text_clipped(
        surface,
        text,
        x,
        y,
        min_x,
        max_x + 1,
        min_y,
        max_y + 1,
        color,
    );
}

// ----------------------------------------------------------------------------
// HallOfFameCard — Responsive Yoko (128x32, 256x64) & Tate (32x64, 64x128, 64x256)
// ----------------------------------------------------------------------------

pub struct HallOfFameCard;

impl HallOfFameCard {
    /// Renders the arcade Hall of Fame leaderboard card with zero heap allocations.
    pub fn render(
        surface: &mut dyn IDrawingSurface,
        meta: &SyncMetaPayload,
        lang: Lang,
        elapsed_ms: u64,
    ) {
        let w = surface.width() as i32;
        let h = surface.height() as i32;
        if w <= 0 || h <= 0 {
            return;
        }

        // Deep arcade attract-mode navy background
        surface.fill_rect(0, 0, w, h, 4, 6, 16);
        let pulse = ((elapsed_ms / 250) & 1) == 0;
        let title_col = if pulse {
            (255, 215, 0)
        } else {
            (255, 245, 150)
        };
        let count = (meta.hiscore_count as usize).min(meta.hiscores.len());

        // 1. Tate / Portrait layouts (32x64, 64x128, 64x256)
        if h > w {
            surface.fill_rect(0, 0, w, 12, 16, 12, 32);
            draw_fast_hline(surface, 0, 12, w, (255, 180, 0));

            if w >= 64 {
                let crown = sprite_bank::get_sprite_frame(SpriteId::CrownRecord, 0);
                sprite_bank::blit_sprite(surface, &crown, 2, 0, false);
                let title = i18n::sync_high_scores_title(lang);
                draw_tiny_text_clipped_bounds(surface, 16, 4, 16, w - 2, 0, 11, title, title_col);
            } else {
                draw_tiny_text_clipped_bounds(
                    surface,
                    2,
                    4,
                    0,
                    w - 2,
                    0,
                    11,
                    "HIGH SCORES",
                    title_col,
                );
            }

            let mut y = 15;
            let game = meta.game_title_str();
            if !game.is_empty() && h >= 64 {
                draw_tiny_text_clipped_bounds(
                    surface,
                    2,
                    y,
                    2,
                    w - 2,
                    13,
                    h - 2,
                    game,
                    (0, 229, 255),
                );
                y += 8;
                draw_fast_hline(surface, 2, y - 2, w - 4, (24, 34, 60));
            }

            let max_rows = if h >= 128 { 5 } else { 3 };
            let row_h = if w >= 64 && h >= 128 { 18 } else { 12 };
            for idx in 0..count.min(max_rows) {
                if y + 6 > h {
                    break;
                }
                let entry = &meta.hiscores[idx];
                let r_col = rank_color_rgb(entry.rank);
                let r_lbl = rank_label_short(entry.rank);

                if w >= 64 {
                    draw_tiny_text_clipped_bounds(surface, 2, y, 2, w - 2, 13, h - 1, r_lbl, r_col);
                    draw_tiny_text_clipped_bounds(
                        surface,
                        18,
                        y,
                        18,
                        w - 2,
                        13,
                        h - 1,
                        entry.player_str(),
                        (255, 255, 255),
                    );
                    let score_s = entry.score_str();
                    let sw = measure_tiny_text(score_s);
                    let sx = (w - 2 - sw).max(2);
                    draw_tiny_text_clipped_bounds(
                        surface,
                        sx,
                        y + 7,
                        2,
                        w - 2,
                        13,
                        h - 1,
                        score_s,
                        r_col,
                    );
                } else {
                    let mut top_line = StackStrBuf::<16>::new();
                    top_line.push_str("#");
                    top_line.push_u32(entry.rank as u32);
                    top_line.push_str(" ");
                    top_line.push_str(entry.player_str());
                    draw_tiny_text_clipped_bounds(
                        surface,
                        1,
                        y,
                        1,
                        w - 1,
                        13,
                        h - 1,
                        top_line.as_str(),
                        r_col,
                    );
                    draw_tiny_text_clipped_bounds(
                        surface,
                        1,
                        y + 6,
                        1,
                        w - 1,
                        13,
                        h - 1,
                        entry.score_str(),
                        (240, 240, 255),
                    );
                }
                y += row_h;
            }
            return;
        }

        // 2. Compact 32px height (64x32, 128x32) -> CPS1 smooth vertical scroll
        if h <= 32 {
            surface.fill_rect(0, 0, w, 8, 18, 12, 34);
            draw_fast_hline(surface, 0, 8, w, (255, 180, 0));
            let title = i18n::sync_high_scores_title(lang);
            let tw = measure_tiny_text(title);
            let tx = ((w - tw) / 2).max(2);
            draw_tiny_text_clipped_bounds(surface, tx, 2, 1, w - 2, 0, 7, title, title_col);

            let view_top = 10;
            let view_bot = h - 1;
            let row_pitch = 8;

            if count <= 2 {
                for idx in 0..count {
                    let ry = view_top + (idx as i32) * 10;
                    Self::draw_yoko_row_compact(
                        surface,
                        &meta.hiscores[idx],
                        w,
                        ry,
                        view_top,
                        view_bot,
                    );
                }
            } else {
                let cycle_px = (count as i32) * row_pitch;
                let scroll_px = ((elapsed_ms / 70) as i32) % cycle_px.max(1);
                for pass in 0..2 {
                    for idx in 0..count {
                        let ry = view_top + (idx as i32) * row_pitch + pass * cycle_px - scroll_px;
                        if ry + 5 >= view_top && ry <= view_bot {
                            Self::draw_yoko_row_compact(
                                surface,
                                &meta.hiscores[idx],
                                w,
                                ry,
                                view_top,
                                view_bot,
                            );
                        }
                    }
                }
            }
            return;
        }

        // 3. HD Yoko / Square (64x64, 128x64, 256x64) -> Full Arcade Podium
        surface.fill_rect(0, 0, w, 12, 18, 12, 36);
        draw_fast_hline(surface, 0, 12, w, (255, 190, 0));
        let crown = sprite_bank::get_sprite_frame(SpriteId::CrownRecord, 0);
        sprite_bank::blit_sprite(surface, &crown, 3, 0, false);

        let title = i18n::sync_high_scores_title(lang);
        if w >= 192 {
            draw_small_text_clipped_bounds(surface, 18, 3, 18, w - 4, 0, 11, title, title_col);
            let game = meta.game_title_str();
            if !game.is_empty() {
                let gw = measure_tiny_text(game);
                let gx = (w - 4 - gw).max(130);
                draw_tiny_text_clipped_bounds(
                    surface,
                    gx,
                    4,
                    130,
                    w - 4,
                    0,
                    11,
                    game,
                    (0, 229, 255),
                );
            }
        } else {
            draw_tiny_text_clipped_bounds(surface, 17, 4, 17, w - 2, 0, 11, title, title_col);
        }

        let start_y = 15;
        let row_h = 9;
        for idx in 0..count.min(5) {
            let ry = start_y + (idx as i32) * row_h;
            if ry + 6 > h {
                break;
            }
            let entry = &meta.hiscores[idx];
            let r_col = rank_color_rgb(entry.rank);
            let r_lbl = rank_label_short(entry.rank);

            if idx == 0 {
                surface.fill_rect(2, ry - 1, w - 4, 8, 28, 22, 10);
            }

            if w >= 128 {
                draw_small_text_clipped_bounds(surface, 4, ry, 2, w - 2, 13, h - 1, r_lbl, r_col);
                draw_small_text_clipped_bounds(
                    surface,
                    28,
                    ry,
                    28,
                    w / 2,
                    13,
                    h - 1,
                    entry.player_str(),
                    (255, 255, 255),
                );
                let score_s = entry.score_str();
                let sw = measure_small_text(score_s);
                let sx = (w - 4 - sw).max(w / 2 + 4);
                // Leader dots on wide 256x64 panels
                if w >= 192 {
                    let dot_start = 28 + measure_small_text(entry.player_str()) + 6;
                    let mut dx = dot_start;
                    while dx + 4 < sx {
                        surface.set_pixel(dx, ry + 5, 50, 60, 90);
                        dx += 4;
                    }
                }
                draw_small_text_clipped_bounds(
                    surface,
                    sx,
                    ry,
                    28,
                    w - 2,
                    13,
                    h - 1,
                    score_s,
                    r_col,
                );
            } else {
                draw_tiny_text_clipped_bounds(surface, 2, ry, 2, w - 2, 13, h - 1, r_lbl, r_col);
                draw_tiny_text_clipped_bounds(
                    surface,
                    16,
                    ry,
                    16,
                    w - 2,
                    13,
                    h - 1,
                    entry.player_str(),
                    (255, 255, 255),
                );
                let score_s = entry.score_str();
                let sw = measure_tiny_text(score_s);
                let sx = (w - 2 - sw).max(30);
                draw_tiny_text_clipped_bounds(
                    surface,
                    sx,
                    ry,
                    30,
                    w - 2,
                    13,
                    h - 1,
                    score_s,
                    r_col,
                );
            }
        }
    }

    fn draw_yoko_row_compact(
        surface: &mut dyn IDrawingSurface,
        entry: &SyncHiscoreEntryPod,
        w: i32,
        ry: i32,
        min_y: i32,
        max_y: i32,
    ) {
        let r_col = rank_color_rgb(entry.rank);
        let r_lbl = rank_label_short(entry.rank);
        draw_tiny_text_clipped_bounds(surface, 2, ry, 2, w - 2, min_y, max_y, r_lbl, r_col);
        draw_tiny_text_clipped_bounds(
            surface,
            18,
            ry,
            18,
            w / 2 + 8,
            min_y,
            max_y,
            entry.player_str(),
            (255, 255, 255),
        );
        let score_s = entry.score_str();
        let sw = measure_tiny_text(score_s);
        let sx = (w - 2 - sw).max(34);
        draw_tiny_text_clipped_bounds(surface, sx, ry, 34, w - 2, min_y, max_y, score_s, r_col);
    }
}

// ----------------------------------------------------------------------------
// RetroAchievementsCard — Responsive Yoko & Tate with Beveled RPG XP Bar
// ----------------------------------------------------------------------------

pub struct RetroAchievementsCard;

impl RetroAchievementsCard {
    /// Renders the RetroAchievements progress card with zero heap allocations.
    pub fn render(
        surface: &mut dyn IDrawingSurface,
        meta: &SyncMetaPayload,
        lang: Lang,
        elapsed_ms: u64,
    ) {
        let w = surface.width() as i32;
        let h = surface.height() as i32;
        if w <= 0 || h <= 0 {
            return;
        }

        let ra = &meta.ra;
        surface.fill_rect(0, 0, w, h, 6, 8, 18);

        // Hardcore gold border or softcore cyan border
        let border_col = if ra.hardcore {
            if ((elapsed_ms / 300) & 1) == 0 {
                (255, 205, 0)
            } else {
                (255, 235, 110)
            }
        } else {
            (0, 150, 210)
        };
        draw_round_rect(surface, 0, 0, w, h, 2, border_col);

        let mut progress_buf = StackStrBuf::<24>::new();
        progress_buf.push_u32(ra.unlocked as u32);
        progress_buf.push_str("/");
        progress_buf.push_u32(ra.total.max(1) as u32);

        let pct = ((ra.unlocked as u32).saturating_mul(100)) / (ra.total.max(1) as u32);
        let mut pts_buf = StackStrBuf::<24>::new();
        pts_buf.push_u32(ra.points_earned);
        pts_buf.push_str(" PTS");

        // 1. Tate / Portrait (32x64, 64x128, 64x256)
        let trophy = sprite_bank::get_sprite_frame(SpriteId::TrophyCup, 0);
        if h > w {
            sprite_bank::blit_sprite(surface, &trophy, (w - 12) / 2, 3, false);
            let hdr = if ra.hardcore {
                "RA HARDCORE"
            } else {
                "RETROACHIEV."
            };
            let hw = measure_tiny_text(hdr);
            draw_tiny_text_clipped_bounds(
                surface,
                ((w - hw) / 2).max(2),
                17,
                2,
                w - 3,
                2,
                h - 2,
                hdr,
                border_col,
            );

            // Beveled XP bar
            let bar_x = 3;
            let bar_w = (w - 6).max(8);
            let bar_y = 25;
            let bar_h = if h >= 128 { 8 } else { 6 };
            Self::draw_beveled_xp_bar(
                surface,
                bar_x,
                bar_y,
                bar_w,
                bar_h,
                ra.unlocked,
                ra.total,
                ra.hardcore,
            );

            let pw = measure_tiny_text(progress_buf.as_str());
            draw_tiny_text_clipped_bounds(
                surface,
                ((w - pw) / 2).max(2),
                bar_y + bar_h + 3,
                2,
                w - 3,
                2,
                h - 2,
                progress_buf.as_str(),
                (255, 255, 255),
            );

            let ptw = measure_tiny_text(pts_buf.as_str());
            draw_tiny_text_clipped_bounds(
                surface,
                ((w - ptw) / 2).max(2),
                bar_y + bar_h + 11,
                2,
                w - 3,
                2,
                h - 2,
                pts_buf.as_str(),
                (0, 229, 255),
            );

            let badge = ra.recent_badge_str();
            if !badge.is_empty() && h >= 64 {
                draw_tiny_text_clipped_bounds(
                    surface,
                    3,
                    bar_y + bar_h + 20,
                    3,
                    w - 3,
                    2,
                    h - 3,
                    badge,
                    (255, 220, 120),
                );
            }
            return;
        }

        // 2. Compact 32px height (64x32, 128x32)
        if h <= 32 {
            sprite_bank::blit_sprite(surface, &trophy, 3, 3, false);
            let title = if w >= 128 {
                if ra.hardcore {
                    "RETROACHIEVEMENTS [HC]"
                } else {
                    "RETROACHIEVEMENTS"
                }
            } else if ra.hardcore {
                "RA [HC]"
            } else {
                "RA"
            };
            draw_tiny_text_clipped_bounds(surface, 17, 4, 17, w - 3, 2, 10, title, border_col);

            let bar_x = 17;
            let bar_w = (w - 21).max(12);
            Self::draw_beveled_xp_bar(
                surface,
                bar_x,
                12,
                bar_w,
                6,
                ra.unlocked,
                ra.total,
                ra.hardcore,
            );

            draw_tiny_text_clipped_bounds(
                surface,
                4,
                22,
                2,
                w - 3,
                18,
                h - 2,
                progress_buf.as_str(),
                (255, 255, 255),
            );
            let ptw = measure_tiny_text(pts_buf.as_str());
            draw_tiny_text_clipped_bounds(
                surface,
                (w - 4 - ptw).max(32),
                22,
                32,
                w - 3,
                18,
                h - 2,
                pts_buf.as_str(),
                (0, 229, 255),
            );
            return;
        }

        // 3. HD Yoko / Square (64x64, 128x64, 256x64)
        sprite_bank::blit_sprite(surface, &trophy, 4, 4, false);
        let hdr = if w >= 192 {
            if ra.hardcore {
                "RETROACHIEVEMENTS • HARDCORE"
            } else {
                "RETROACHIEVEMENTS"
            }
        } else {
            i18n::sync_achievement_title(lang)
        };
        if w >= 192 {
            draw_small_text_clipped_bounds(surface, 20, 5, 20, w - 4, 2, 14, hdr, border_col);
        } else {
            draw_tiny_text_clipped_bounds(surface, 18, 6, 18, w - 4, 2, 14, hdr, border_col);
        }

        let game = if !ra.game_title_str().is_empty() {
            ra.game_title_str()
        } else {
            meta.game_title_str()
        };
        if !game.is_empty() {
            draw_tiny_text_clipped_bounds(
                surface,
                20,
                16,
                20,
                w - 4,
                14,
                24,
                game,
                (220, 235, 255),
            );
        }

        // Beveled RPG XP Bar
        let bar_x = 6;
        let bar_w = (w - 12).max(16);
        Self::draw_beveled_xp_bar(
            surface,
            bar_x,
            25,
            bar_w,
            10,
            ra.unlocked,
            ra.total,
            ra.hardcore,
        );

        let mut stats_line = StackStrBuf::<32>::new();
        stats_line.push_str(progress_buf.as_str());
        stats_line.push_str(" (");
        stats_line.push_u32(pct);
        stats_line.push_str("%)");
        draw_small_text_clipped_bounds(
            surface,
            6,
            39,
            4,
            w - 4,
            36,
            50,
            stats_line.as_str(),
            (255, 255, 255),
        );

        let ptw = measure_small_text(pts_buf.as_str());
        draw_small_text_clipped_bounds(
            surface,
            (w - 6 - ptw).max(w / 2),
            39,
            w / 2,
            w - 4,
            36,
            50,
            pts_buf.as_str(),
            (255, 215, 0),
        );

        let badge = ra.recent_badge_str();
        if !badge.is_empty() {
            draw_tiny_text_clipped_bounds(
                surface,
                6,
                51,
                4,
                w - 4,
                48,
                h - 3,
                badge,
                (0, 229, 255),
            );
        }
    }

    fn draw_beveled_xp_bar(
        surface: &mut dyn IDrawingSurface,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        unlocked: u16,
        total: u16,
        hardcore: bool,
    ) {
        if w <= 4 || h <= 3 {
            return;
        }
        draw_round_rect(surface, x, y, w, h, 1, (70, 85, 120));
        fill_round_rect(surface, x + 1, y + 1, w - 2, h - 2, 1, (14, 18, 30));

        let inner_w = (w - 2).max(0);
        let fill_w =
            ((inner_w as u32).saturating_mul(unlocked as u32) / (total.max(1) as u32)) as i32;
        let fill_w = fill_w.clamp(0, inner_w);
        if fill_w > 0 {
            let (top_col, bot_col) = if hardcore {
                ((255, 235, 90), (215, 145, 0))
            } else {
                ((80, 255, 170), (0, 185, 100))
            };
            let top_h = ((h - 2) / 2).max(1);
            let bot_h = (h - 2 - top_h).max(0);
            surface.fill_rect(x + 1, y + 1, fill_w, top_h, top_col.0, top_col.1, top_col.2);
            if bot_h > 0 {
                surface.fill_rect(
                    x + 1,
                    y + 1 + top_h,
                    fill_w,
                    bot_h,
                    bot_col.0,
                    bot_col.1,
                    bot_col.2,
                );
            }
        }
    }
}

// ----------------------------------------------------------------------------
// SyncCelebrationEngine — Priority 55 60 FPS Fireworks + Hero Cameo (0 Alloc)
// ----------------------------------------------------------------------------

pub struct SyncCelebrationEngine {
    fireworks: FireworksLutRenderer,
    active_payload: SyncCelebrationPayload,
    observed_event_token: u64,
    start_instant: Option<Instant>,
    paused_at: Option<Instant>,
    paused_accum_ms: u64,
    elapsed_ms: u64,
    last_frame_tick: u32,
    pending_draw: bool,
    lang: Lang,
}

impl SyncCelebrationEngine {
    pub fn new() -> Self {
        Self {
            fireworks: FireworksLutRenderer::new(),
            active_payload: SyncCelebrationPayload::EMPTY,
            observed_event_token: 0,
            start_instant: None,
            paused_at: None,
            paused_accum_ms: 0,
            elapsed_ms: 0,
            last_frame_tick: u32::MAX,
            pending_draw: true,
            lang: Lang::En,
        }
    }
}

impl Default for SyncCelebrationEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for SyncCelebrationEngine {
    fn initialize(
        &mut self,
        context: &mut EngineContext,
        _config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        let lang_code = context.config.get_sys_string("lang", "en");
        self.lang = Lang::from_code(&lang_code);
        Ok(())
    }

    fn activate(&mut self) {
        self.fireworks.reset();
        self.start_instant = Some(Instant::now());
        self.paused_at = None;
        self.paused_accum_ms = 0;
        self.elapsed_ms = 0;
        self.last_frame_tick = u32::MAX;
        self.pending_draw = true;
    }

    fn pause(&mut self) {
        if self.paused_at.is_none() {
            self.paused_at = Some(Instant::now());
        }
        self.pending_draw = false;
    }

    fn resume(&mut self) {
        if let Some(p) = self.paused_at.take() {
            self.paused_accum_ms = self
                .paused_accum_ms
                .saturating_add(p.elapsed().as_millis() as u64);
        }
        self.pending_draw = true;
    }

    fn deactivate(&mut self) {
        self.fireworks.reset();
        self.start_instant = None;
        self.paused_at = None;
        self.paused_accum_ms = 0;
        self.elapsed_ms = 0;
        self.pending_draw = false;
    }

    fn on_display_geometry_changed(&mut self, _geometry: &DisplayGeometry) {
        self.pending_draw = true;
    }

    fn update(&mut self, context: &mut EngineContext) {
        {
            let s = context.config.settings.read();
            let new_lang = Lang::from_code(&s.system.lang);
            if new_lang != self.lang {
                self.lang = new_lang;
                self.pending_draw = true;
            }
        }
        if self.paused_at.is_some() {
            self.pending_draw = false;
            return;
        }
        let now = Instant::now();
        let start = *self.start_instant.get_or_insert(now);

        let snap = context.config.producer_snapshot.load();
        if let Some(ref cel) = snap.sync_celebration {
            if cel.event_token != self.observed_event_token {
                self.observed_event_token = cel.event_token;
                self.active_payload = **cel;
                self.fireworks.reset();
                self.start_instant = Some(now);
                self.paused_accum_ms = 0;
                self.last_frame_tick = u32::MAX;
            }
        }

        let raw_elapsed = now.duration_since(start).as_millis() as u64;
        self.elapsed_ms = raw_elapsed.saturating_sub(self.paused_accum_ms);
        let tick = (self.elapsed_ms / 16) as u32;
        if tick != self.last_frame_tick {
            self.last_frame_tick = tick;
            let w = context.surface.width() as i32;
            let h = context.surface.height() as i32;
            self.fireworks.update(w, h);
            self.pending_draw = true;
        } else {
            self.pending_draw = false;
        }
    }

    fn render(&mut self, context: &mut EngineContext) {
        let surface = &mut *context.surface;
        let w = surface.width() as i32;
        let h = surface.height() as i32;
        if w <= 0 || h <= 0 {
            self.pending_draw = false;
            return;
        }

        // Layer 1: Celebratory night sky + Q8.8 integer fireworks
        surface.fill_rect(0, 0, w, h, 4, 4, 14);
        self.fireworks.render(surface);

        // Layer 2: Bouncing arcade header via BOUNCE_LUT
        let bounce_idx = ((self.last_frame_tick as usize) / 3) % BOUNCE_LUT.len();
        let bounce_dy = BOUNCE_LUT[bounce_idx] as i32;
        let header_y = (3 + bounce_dy / 2).max(1);
        let flash = ((self.last_frame_tick / 4) & 1) == 0;
        let hdr_col = if flash {
            (255, 230, 60)
        } else {
            (255, 255, 255)
        };

        let header_str = if self.active_payload.kind == 1 {
            i18n::sync_achievement_title(self.lang)
        } else {
            i18n::sync_new_record_title(self.lang)
        };
        let hw = measure_tiny_text(header_str);
        let hx = ((w - hw) / 2).max(2);
        draw_tiny_text_clipped_bounds(
            surface,
            hx,
            header_y,
            1,
            w - 2,
            0,
            h - 1,
            header_str,
            hdr_col,
        );

        // Layer 3: System-matched Hero Cameo from sprite_bank
        let hero_family = CelebrationHeroFamily::from_system(self.active_payload.system_str());
        let anim_step = (self.last_frame_tick / 4) as usize;
        let stage_y = if h <= 32 { 9 } else { (h / 2) - 8 };

        match hero_family {
            CelebrationHeroFamily::NintendoMario => {
                let is_jumping = (anim_step % 8) >= 5;
                let mario_frame = if is_jumping {
                    sprite_bank::mario_jump_frame()
                } else {
                    sprite_bank::mario_run_frame(anim_step)
                };
                let mario_y = if is_jumping { stage_y - 3 } else { stage_y };
                let mario_x = (w / 2) - 14;
                sprite_bank::blit_sprite(surface, &mario_frame, mario_x, mario_y, false);
                let coin_frame = sprite_bank::mario_gold_coin_frame();
                sprite_bank::blit_sprite(surface, &coin_frame, mario_x + 18, stage_y - 2, false);
            }
            CelebrationHeroFamily::SegaSonic => {
                let sonic_x = (w / 2) - 16;
                let sonic_frame = if h <= 32 {
                    sprite_bank::sonic_sms_run_frame(anim_step)
                } else if (anim_step % 8) >= 5 {
                    sprite_bank::sonic_ball_frame(anim_step)
                } else {
                    sprite_bank::sonic_md_run_frame(anim_step)
                };
                sprite_bank::blit_sprite(surface, &sonic_frame, sonic_x, stage_y - 4, false);
                let ring_frame = sprite_bank::sonic_ring_frame(anim_step);
                sprite_bank::blit_sprite(surface, &ring_frame, sonic_x + 24, stage_y + 2, false);
            }
            CelebrationHeroFamily::CapcomMegaMan => {
                let mm_x = (w / 2) - 18;
                let mm_frame = if (anim_step % 8) >= 5 {
                    sprite_bank::megaman_jump_shoot_frame()
                } else {
                    sprite_bank::megaman_run_frame(anim_step)
                };
                sprite_bank::blit_sprite(surface, &mm_frame, mm_x, stage_y - 4, false);
                let bullet_dx = ((self.last_frame_tick as i32) * 2) % 20;
                let bullet_frame = sprite_bank::megaman_bullet_frame();
                sprite_bank::blit_sprite(
                    surface,
                    &bullet_frame,
                    mm_x + 24 + bullet_dx,
                    stage_y + 4,
                    false,
                );
                if bullet_dx >= 14 {
                    let spark_frame = sprite_bank::megaman_spark_frame();
                    sprite_bank::blit_sprite(
                        surface,
                        &spark_frame,
                        mm_x + 26 + bullet_dx,
                        stage_y + 2,
                        false,
                    );
                }
            }
            CelebrationHeroFamily::SnkMetalSlug => {
                let marco_x = (w / 2) - 14;
                let marco_frame = if (anim_step % 8) >= 4 {
                    sprite_bank::marco_victory_frame()
                } else {
                    sprite_bank::marco_walk_frame(anim_step)
                };
                sprite_bank::blit_sprite(surface, &marco_frame, marco_x, stage_y - 6, false);
            }
            CelebrationHeroFamily::ArcadePacMan => {
                let pac_x = (w / 2) - 14;
                let pac_frame = sprite_bank::pacman_frame(anim_step);
                sprite_bank::blit_sprite(surface, &pac_frame, pac_x, stage_y, false);
                sprite_bank::draw_frightened_ghost(
                    surface,
                    pac_x + 16,
                    stage_y,
                    anim_step,
                    (anim_step & 2) != 0,
                );
            }
        }

        // Layer 4: Score / Achievement title & points footer
        let title_s = self.active_payload.title_str();
        let score_s = self.active_payload.score_or_pts_str();
        if h <= 32 {
            let bot_y = h - 6;
            let mut footer = StackStrBuf::<48>::new();
            if !title_s.is_empty() {
                footer.push_str(title_s);
                if !score_s.is_empty() {
                    footer.push_str(" ");
                    footer.push_str(score_s);
                }
            } else {
                footer.push_str(score_s);
            }
            let fw = measure_tiny_text(footer.as_str());
            let fx = ((w - fw) / 2).max(2);
            draw_tiny_text_clipped_bounds(
                surface,
                fx,
                bot_y,
                1,
                w - 2,
                0,
                h - 1,
                footer.as_str(),
                (0, 229, 255),
            );
        } else {
            if !title_s.is_empty() {
                let tw = measure_tiny_text(title_s);
                let tx = ((w - tw) / 2).max(2);
                draw_tiny_text_clipped_bounds(
                    surface,
                    tx,
                    h - 15,
                    2,
                    w - 2,
                    0,
                    h - 1,
                    title_s,
                    (255, 255, 255),
                );
            }
            if !score_s.is_empty() {
                let sw = measure_small_text(score_s);
                let sx = ((w - sw) / 2).max(2);
                draw_small_text_clipped_bounds(
                    surface,
                    sx,
                    h - 8,
                    2,
                    w - 2,
                    0,
                    h - 1,
                    score_s,
                    (0, 229, 255),
                );
            }
        }

        self.pending_draw = false;
    }

    fn wants_clear(&self) -> bool {
        true
    }

    fn has_new_frame(&self) -> bool {
        self.paused_at.is_none() && self.pending_draw
    }

    fn is_realtime(&self) -> bool {
        self.paused_at.is_none()
    }

    fn allows_overlay(&self) -> bool {
        false
    }

    fn allow_rotation(&self) -> bool {
        false
    }
}

// ----------------------------------------------------------------------------
// InGameShowcaseEngine — Opportunistic Card + Stream Cycle (Zero Alloc)
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InGameSubStage {
    HallOfFame,
    RetroAchievements,
    ShowcaseStream,
}

/// Zero-reallocation realtime playback engine for In-Game Showcase sequences (`DMDZ` v2 & `DMDS`)
/// and opportunistic `HallOfFameCard` / `RetroAchievementsCard` views.
///
/// Shares [`crate::core::dmd_stream::ShowcaseStreamPlayer`] 1:1 with [`MediaShowcaseRunner`] and
/// preempts `Marquee` only once the complete sequence or sync metadata is ready (`DisplaySourceId::InGameShowcase`).
pub struct InGameShowcaseEngine {
    session_token: u64,
    stream_width: u32,
    stream_height: u32,
    surface_width: u32,
    surface_height: u32,
    has_stream: bool,
    pending_draw: bool,
    finished: bool,
    player: crate::core::dmd_stream::ShowcaseStreamPlayer,
    sub_stage: InGameSubStage,
    stage_started_at: Option<Instant>,
    stage_paused_at: Option<Instant>,
    stage_paused_ms: u64,
    stage_elapsed_ms: u64,
    last_card_tick: u32,
    lang: Lang,
}

impl InGameShowcaseEngine {
    pub fn new() -> Self {
        Self {
            session_token: 0,
            stream_width: 0,
            stream_height: 0,
            surface_width: 0,
            surface_height: 0,
            has_stream: false,
            pending_draw: false,
            finished: false,
            player: crate::core::dmd_stream::ShowcaseStreamPlayer::new(),
            sub_stage: InGameSubStage::ShowcaseStream,
            stage_started_at: None,
            stage_paused_at: None,
            stage_paused_ms: 0,
            stage_elapsed_ms: 0,
            last_card_tick: u32::MAX,
            lang: Lang::En,
        }
    }

    fn select_initial_stage(
        meta: Option<&Arc<SyncMetaPayload>>,
        has_stream: bool,
    ) -> Option<InGameSubStage> {
        if let Some(m) = meta {
            if m.has_active_hiscores() {
                return Some(InGameSubStage::HallOfFame);
            }
            if m.has_active_ra() {
                return Some(InGameSubStage::RetroAchievements);
            }
        }
        if has_stream {
            Some(InGameSubStage::ShowcaseStream)
        } else {
            None
        }
    }
}

impl Default for InGameShowcaseEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for InGameShowcaseEngine {
    fn initialize(
        &mut self,
        context: &mut EngineContext,
        _config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        let lang_code = context.config.get_sys_string("lang", "en");
        self.lang = Lang::from_code(&lang_code);
        Ok(())
    }

    fn activate(&mut self) {
        let now = Instant::now();
        self.finished = false;
        self.pending_draw = true;
        self.session_token = 0;
        self.stage_started_at = Some(now);
        self.stage_paused_at = None;
        self.stage_paused_ms = 0;
        self.stage_elapsed_ms = 0;
        self.last_card_tick = u32::MAX;
        self.player.start(now);
    }

    fn pause(&mut self) {
        let now = Instant::now();
        self.pending_draw = false;
        if self.stage_paused_at.is_none() {
            self.stage_paused_at = Some(now);
        }
        self.player.pause(now);
    }

    fn resume(&mut self) {
        let now = Instant::now();
        if let Some(p) = self.stage_paused_at.take() {
            self.stage_paused_ms = self
                .stage_paused_ms
                .saturating_add(now.duration_since(p).as_millis() as u64);
        }
        self.player.resume(now);
        self.pending_draw = !self.finished;
    }

    fn deactivate(&mut self) {
        self.session_token = 0;
        self.stream_width = 0;
        self.stream_height = 0;
        self.has_stream = false;
        self.pending_draw = false;
        self.finished = false;
        self.stage_started_at = None;
        self.stage_paused_at = None;
        self.stage_paused_ms = 0;
        self.stage_elapsed_ms = 0;
        self.last_card_tick = u32::MAX;
        self.player.reset();
    }

    fn on_display_geometry_changed(&mut self, geometry: &DisplayGeometry) {
        self.surface_width = geometry.width();
        self.surface_height = geometry.height();
        self.player.invalidate_drawn_frame();
        if !self.finished && self.stage_paused_at.is_none() {
            self.pending_draw = true;
        }
    }

    fn update(&mut self, context: &mut EngineContext) {
        {
            let s = context.config.settings.read();
            let new_lang = Lang::from_code(&s.system.lang);
            if new_lang != self.lang {
                self.lang = new_lang;
                self.pending_draw = true;
            }
        }
        self.surface_width = context.surface.width();
        self.surface_height = context.surface.height();
        if self.stage_paused_at.is_some() || self.player.is_paused() {
            self.pending_draw = false;
            return;
        }

        let snap = context.config.producer_snapshot.load();
        let ig_opt = snap
            .ingame_showcase
            .as_ref()
            .filter(|ig| !ig.stream.is_empty());
        let meta_opt = snap.sync_meta.as_ref().filter(|m| m.has_any_card());

        if ig_opt.is_none() && meta_opt.is_none() {
            self.has_stream = false;
            self.pending_draw = false;
            self.finished = true;
            return;
        }

        let now = Instant::now();
        let active_token = ig_opt
            .map(|ig| ig.session_token)
            .or_else(|| meta_opt.map(|m| m.session_token))
            .unwrap_or(0);

        if self.session_token != active_token {
            let prev_token = self.session_token;
            self.session_token = active_token;
            self.finished = false;
            if let Some(first_stage) = Self::select_initial_stage(meta_opt, ig_opt.is_some()) {
                self.sub_stage = first_stage;
            }
            self.stage_started_at = Some(now);
            self.stage_paused_ms = 0;
            self.stage_elapsed_ms = 0;
            self.last_card_tick = u32::MAX;
            if prev_token != 0 || !self.player.is_started() {
                self.player.start(now);
            }
        }

        if self.finished {
            self.pending_draw = false;
            return;
        }

        let card_dur_ms = meta_opt
            .map(|m| (m.card_duration_sec.clamp(5, 20) as u64) * 1000)
            .unwrap_or(8000);

        match self.sub_stage {
            InGameSubStage::HallOfFame => {
                let has_hi = meta_opt.is_some_and(|m| m.has_active_hiscores());
                let start = *self.stage_started_at.get_or_insert(now);
                let elapsed = (now.duration_since(start).as_millis() as u64)
                    .saturating_sub(self.stage_paused_ms);
                self.stage_elapsed_ms = elapsed;
                if !has_hi || elapsed >= card_dur_ms {
                    if meta_opt.is_some_and(|m| m.has_active_ra()) {
                        self.sub_stage = InGameSubStage::RetroAchievements;
                        self.stage_started_at = Some(now);
                        self.stage_paused_ms = 0;
                        self.stage_elapsed_ms = 0;
                        self.last_card_tick = u32::MAX;
                        self.pending_draw = true;
                        return;
                    } else if ig_opt.is_some() {
                        self.sub_stage = InGameSubStage::ShowcaseStream;
                        self.player.start(now);
                    } else {
                        self.finished = true;
                        self.pending_draw = false;
                        return;
                    }
                } else {
                    let tick = (elapsed / 66) as u32;
                    if tick != self.last_card_tick {
                        self.last_card_tick = tick;
                        self.pending_draw = true;
                    } else {
                        self.pending_draw = false;
                    }
                    return;
                }
            }
            InGameSubStage::RetroAchievements => {
                let has_ra = meta_opt.is_some_and(|m| m.has_active_ra());
                let start = *self.stage_started_at.get_or_insert(now);
                let elapsed = (now.duration_since(start).as_millis() as u64)
                    .saturating_sub(self.stage_paused_ms);
                self.stage_elapsed_ms = elapsed;
                if !has_ra || elapsed >= card_dur_ms {
                    if ig_opt.is_some() {
                        self.sub_stage = InGameSubStage::ShowcaseStream;
                        self.player.start(now);
                    } else {
                        self.finished = true;
                        self.pending_draw = false;
                        return;
                    }
                } else {
                    let tick = (elapsed / 100) as u32;
                    if tick != self.last_card_tick {
                        self.last_card_tick = tick;
                        self.pending_draw = true;
                    } else {
                        self.pending_draw = false;
                    }
                    return;
                }
            }
            InGameSubStage::ShowcaseStream => {}
        }

        let Some(ig) = ig_opt else {
            self.has_stream = false;
            self.pending_draw = false;
            self.finished = true;
            return;
        };

        self.has_stream = true;
        self.stream_width = ig.stream.width;
        self.stream_height = ig.stream.height;

        let has_cards = meta_opt.is_some();
        let loop_playback = matches!(ig.cycle_mode, InGameCycleMode::Loop) && !has_cards;
        let stream_done = self.player.advance(&ig.stream, now, loop_playback, None);
        if stream_done && matches!(ig.cycle_mode, InGameCycleMode::Loop) && has_cards {
            if let Some(first_stage) = Self::select_initial_stage(meta_opt, true) {
                self.sub_stage = first_stage;
                self.stage_started_at = Some(now);
                self.stage_paused_ms = 0;
                self.stage_elapsed_ms = 0;
                self.last_card_tick = u32::MAX;
                self.pending_draw = true;
                return;
            }
        }
        self.finished = stream_done;
        self.pending_draw = self.player.has_new_frame(&ig.stream);
    }

    fn render(&mut self, context: &mut EngineContext) {
        self.surface_width = context.surface.width();
        self.surface_height = context.surface.height();
        let snap = context.config.producer_snapshot.load();
        match self.sub_stage {
            InGameSubStage::HallOfFame => {
                if let Some(ref meta) = snap.sync_meta {
                    HallOfFameCard::render(
                        &mut *context.surface,
                        meta,
                        self.lang,
                        self.stage_elapsed_ms,
                    );
                }
            }
            InGameSubStage::RetroAchievements => {
                if let Some(ref meta) = snap.sync_meta {
                    RetroAchievementsCard::render(
                        &mut *context.surface,
                        meta,
                        self.lang,
                        self.stage_elapsed_ms,
                    );
                }
            }
            InGameSubStage::ShowcaseStream => {
                if let Some(ref ig) = snap.ingame_showcase {
                    self.player
                        .render_to_surface(&ig.stream, &mut *context.surface);
                }
            }
        }
        self.pending_draw = false;
    }

    fn wants_clear(&self) -> bool {
        if self.sub_stage != InGameSubStage::ShowcaseStream {
            return true;
        }
        !self.has_stream
            || self.stream_width == 0
            || self.stream_height == 0
            || (self.surface_width > 0
                && (self.stream_width != self.surface_width
                    || self.stream_height != self.surface_height))
    }

    fn has_new_frame(&self) -> bool {
        !self.finished
            && self.stage_paused_at.is_none()
            && !self.player.is_paused()
            && self.pending_draw
    }

    fn is_realtime(&self) -> bool {
        !self.finished && self.stage_paused_at.is_none() && !self.player.is_paused()
    }

    fn is_finished(&self) -> bool {
        self.finished
    }

    fn self_paced(&self) -> bool {
        true
    }

    fn allows_overlay(&self) -> bool {
        false
    }

    fn allow_rotation(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::dmd_stream::{parse_dmd_stream_compact, DMDZ_MAGIC, FRAME_KEY, FRAME_RAW};
    use crate::core::matrix::{IDrawingSurface, MockSurface};
    use crate::core::registry::EngineRuntime;
    use std::collections::HashMap;

    fn build_sample_dmdz_v2_stream(
        frame_dur_ms: u16,
    ) -> Arc<crate::core::dmd_stream::PreloadedDmdStream> {
        let mut raw = Vec::new();
        raw.extend_from_slice(DMDZ_MAGIC);
        raw.push(2); // version 2
        raw.push(25); // fps
        raw.extend_from_slice(&2u16.to_le_bytes()); // width = 2
        raw.extend_from_slice(&2u16.to_le_bytes()); // height = 2
        raw.extend_from_slice(&2u16.to_le_bytes()); // total_frames = 2
        raw.extend_from_slice(&frame_dur_ms.to_le_bytes());
        raw.extend_from_slice(&1u16.to_le_bytes());

        // Frame 0: FRAME_RAW pure red (0xF800)
        let mut f0 = vec![FRAME_RAW];
        for _ in 0..4 {
            f0.extend_from_slice(&0xF800u16.to_le_bytes());
        }
        raw.extend_from_slice(&(f0.len() as u16).to_le_bytes());
        raw.extend_from_slice(&f0);

        // Frame 1: FRAME_KEY OP_LITERAL 1 (pure blue 0x001F) + OP_RUN 3 -> 4 blue pixels
        let mut f1 = vec![FRAME_KEY, 0xE0];
        f1.extend_from_slice(&0x001Fu16.to_le_bytes());
        f1.push(0x42);
        raw.extend_from_slice(&(f1.len() as u16).to_le_bytes());
        raw.extend_from_slice(&f1);

        parse_dmd_stream_compact(&raw).expect("valid DMDZ v2 test stream")
    }

    #[test]
    fn test_format_game_name() {
        assert_eq!(format_game_name("super-mario_bros"), "Super Mario Bros");
        assert_eq!(
            format_game_name("the_legend-of-zelda"),
            "The Legend Of Zelda"
        );
        assert_eq!(format_game_name("pacman"), "Pacman");
        assert_eq!(format_game_name("STREET_FIGHTER_II"), "STREET FIGHTER II");
        assert_eq!(format_game_name("donkey_kong"), "Donkey Kong");
        assert_eq!(
            format_game_name("Super Mario World (USA) [!]"),
            "Super Mario World"
        );
    }

    #[test]
    fn test_missing_game_marquee_displays_cyan_game_name_not_system_marquee() {
        let tmp_dir = std::env::temp_dir().join(format!(
            "arcadematrix_missing_game_marquee_test_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp_dir);
        let marquee_dir = tmp_dir.join("marquees");
        std::fs::create_dir_all(&marquee_dir).unwrap();

        // Populate a valid cached system marquee (`system_snes.png`), game marquee, AND negative-cache entries
        let mut sys_img = image::RgbImage::new(8, 8);
        sys_img.put_pixel(0, 0, image::Rgb([255, 0, 0]));
        sys_img.save(marquee_dir.join("system_snes.png")).unwrap();
        let mut game_img = image::RgbImage::new(8, 8);
        game_img.put_pixel(0, 0, image::Rgb([0, 255, 0]));
        game_img
            .save(marquee_dir.join("snes_Super Mario World (USA).png"))
            .unwrap();
        std::fs::write(
            marquee_dir.join(".negative_cache_v2"),
            "snes/homebrew_quest\nsnes/shortg\nsystem/custom_arcade\n",
        )
        .unwrap();

        let config = Arc::new(Config::new(tmp_dir.join("config.json")));
        let dmd_cache = Arc::new(crate::core::dmd_cache::DmdCache::new(&marquee_dir));

        // 1. System event with cached system marquee -> displays system marquee
        handle_frontend_sync_payload(
            &config,
            &dmd_cache,
            r#"{"status":"browsing","system":"snes","type":"system"}"#,
        );
        assert_eq!(
            config.producer_snapshot.load().forced_mode,
            ForcedEngineMode::Marquee
        );
        assert!(config.image_obj.lock().is_some());

        // 2. Game event with missing Pixelcade marquee -> MUST display cyan game title (NOT system_snes.png!)
        handle_frontend_sync_payload(
            &config,
            &dmd_cache,
            r#"{"status":"browsing","game":"homebrew_quest","title":"Secret Homebrew Quest","system":"snes"}"#,
        );
        let snap = config.producer_snapshot.load();
        assert_eq!(snap.forced_mode, ForcedEngineMode::Message);
        assert!(
            config.image_obj.lock().is_none(),
            "stale system marquee image must be cleared when falling back to cyan game banner"
        );
        let msg = snap
            .message_payload
            .as_ref()
            .expect("cyan game banner payload");
        assert_eq!(msg.color, "#00ffff");
        assert_eq!(msg.text, " Secret Homebrew Quest ");
        assert_eq!(msg.direction, "left");

        // 3. Short game slug without `title` field -> displays formatted game name in cyan
        handle_frontend_sync_payload(
            &config,
            &dmd_cache,
            r#"{"status":"browsing","game":"shortg","system":"snes"}"#,
        );
        let snap_short = config.producer_snapshot.load();
        assert_eq!(snap_short.forced_mode, ForcedEngineMode::Message);
        let msg_short = snap_short.message_payload.as_ref().unwrap();
        assert_eq!(msg_short.color, "#00ffff");
        assert_eq!(msg_short.text, "Shortg");
        assert_eq!(msg_short.direction, "none");

        // 4. System event without Pixelcade marquee -> displays cyan system name
        handle_frontend_sync_payload(
            &config,
            &dmd_cache,
            r#"{"status":"browsing","system":"custom_arcade","type":"system"}"#,
        );
        let snap_sys = config.producer_snapshot.load();
        assert_eq!(snap_sys.forced_mode, ForcedEngineMode::Message);
        let msg_sys = snap_sys.message_payload.as_ref().unwrap();
        assert_eq!(msg_sys.color, "#00ffff");
        assert_eq!(msg_sys.text, " Custom Arcade ");
        assert_eq!(msg_sys.direction, "left");

        // 5. Virtual collection ("favorites") with `rom_path` -> extracts real `snes` system and plays cached marquee!
        handle_frontend_sync_payload(
            &config,
            &dmd_cache,
            r#"{"status":"browsing","game":"Super Mario World (USA)","system":"favorites","rom_path":"/recalbox/share/roms/snes/Super Mario World (USA).sfc"}"#,
        );
        assert_eq!(
            config.producer_snapshot.load().forced_mode,
            ForcedEngineMode::Marquee
        );
        assert_eq!(e2e_telemetry().lock().last_system, "snes");
        assert!(config.image_obj.lock().is_some());

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_build_ingame_queue_payload_inherits_showcase_profile_by_default() {
        let cfg = MqttConfig::default();
        let payload = build_ingame_queue_payload(&cfg, "mame", "mslugx", 128, 32, "fr");
        assert_eq!(payload["domain"], "games");
        assert_eq!(payload["systems"], json!(["mame"]));
        assert_eq!(payload["query"], "mslugx");
        assert_eq!(payload["mode"], "play_now");
        assert_eq!(payload["mark_seen"], false);
        assert_eq!(payload["async_prepare"], true);
        assert_eq!(payload["render_card_in_stream"], true);
        assert_eq!(payload["width"], 128);
        assert_eq!(payload["height"], 32);
        assert_eq!(payload["lang"], "fr");
        assert!(payload.get("card_layout").is_none());
        assert!(payload.get("card_theme").is_none());
    }

    #[test]
    fn test_build_ingame_queue_payload_custom_card_overrides() {
        let mut cfg = MqttConfig::default();
        cfg.card_style_source = "custom".to_string();
        cfg.card_layout = "hero_banner".to_string();
        cfg.card_theme = "neon_border".to_string();
        cfg.video_max_duration_s = 15;
        let payload = build_ingame_queue_payload(&cfg, "snes", "Super Mario World", 128, 64, "en");
        assert_eq!(payload["card_layout"], "hero_banner");
        assert_eq!(payload["card_theme"], "neon_border");
        assert_eq!(payload["video_max_duration_s"], 15);
        assert_eq!(payload["render_card_in_stream"], true);
    }

    #[test]
    fn test_build_ingame_queue_payload_fallback_query_and_dirty_sync_guard() {
        let cfg = MqttConfig::default();
        let with_fb = build_ingame_queue_payload_with_fallback(
            &cfg,
            "neogeo",
            "mslugx",
            Some("Metal Slug X - Super Vehicle-001"),
            128,
            32,
            "fr",
        );
        assert_eq!(with_fb["query"], "mslugx");
        assert_eq!(
            with_fb["fallback_query"],
            "Metal Slug X - Super Vehicle-001"
        );

        let same_title = build_ingame_queue_payload_with_fallback(
            &cfg,
            "neogeo",
            "mslugx",
            Some("MSLUGX"),
            128,
            32,
            "fr",
        );
        assert!(same_title.get("fallback_query").is_none());

        // Idle browsing -> remote_sync_dirty is false -> take_dirty_remote_sync_target returns None (0 HTTP DELETE)
        {
            let slot =
                ACTIVE_SYNC_QUEUE.get_or_init(|| Mutex::new(RemoteSyncSessionState::default()));
            *slot.lock() = RemoteSyncSessionState::default();
        }
        assert!(take_dirty_remote_sync_target(None).is_none());

        // Armed during `playing` -> consumed strictly ONCE on exit, then idle again for subsequent browsing
        {
            let slot =
                ACTIVE_SYNC_QUEUE.get_or_init(|| Mutex::new(RemoteSyncSessionState::default()));
            *slot.lock() = RemoteSyncSessionState {
                req_id: 99,
                server_url: "http://127.0.0.1:8890".to_string(),
                sync_client_id: "rpi_abcdef_games_128x32_sync".to_string(),
                remote_sync_dirty: true,
            };
        }
        assert!(take_dirty_remote_sync_target(Some(42)).is_none());
        let consumed = take_dirty_remote_sync_target(None);
        assert_eq!(
            consumed,
            Some((
                "http://127.0.0.1:8890".to_string(),
                "rpi_abcdef_games_128x32_sync".to_string()
            ))
        );
        assert!(take_dirty_remote_sync_target(None).is_none());
    }

    #[test]
    fn test_parse_cycle_mode() {
        let mut cfg = MqttConfig::default();
        assert_eq!(
            parse_cycle_mode(&cfg),
            InGameCycleMode::Alternating {
                marquee_interval_sec: 45
            }
        );
        cfg.ingame_cycle_mode = "once".to_string();
        assert_eq!(parse_cycle_mode(&cfg), InGameCycleMode::Once);
        cfg.ingame_cycle_mode = "loop".to_string();
        assert_eq!(parse_cycle_mode(&cfg), InGameCycleMode::Loop);
    }

    #[test]
    fn test_ingame_showcase_engine_plays_dmdz_v2_and_clears_borders() {
        let tmp_dir = std::env::temp_dir().join(format!(
            "arcadematrix_ingame_dmdz_test_{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&tmp_dir);
        let config = Config::new(tmp_dir.join("config.json"));
        let stream = build_sample_dmdz_v2_stream(30);

        config.set_ingame_showcase(Some(InGameShowcasePayload {
            session_token: 42,
            stream: Arc::clone(&stream),
            cycle_mode: InGameCycleMode::Once,
        }));

        let mut engine = InGameShowcaseEngine::new();
        assert!(!engine.allows_overlay());
        assert!(!engine.allow_rotation());
        assert!(engine.self_paced());

        // Surface is 4x4 while stream is 2x2 -> wants_clear() must be true and borders cleared!
        let mut surface = MockSurface::new(4, 4);
        surface.fill_rect(0, 0, 4, 4, 255, 255, 255);

        engine.activate();
        {
            let mut ctx = EngineContext {
                surface: &mut surface,
                config: &config,
            };
            engine.update(&mut ctx);
            assert!(engine.wants_clear());
            assert!(engine.has_new_frame());
            engine.render(&mut ctx);
            assert!(!engine.has_new_frame());
        }
        assert_eq!(surface.canvas.get_pixel(0, 0).0, [255, 0, 0]);
        assert_eq!(surface.canvas.get_pixel(3, 3).0, [0, 0, 0]);

        // Advance past frame 0 (35ms -> frame 1 = FRAME_KEY blue)
        std::thread::sleep(Duration::from_millis(38));
        {
            let mut ctx = EngineContext {
                surface: &mut surface,
                config: &config,
            };
            engine.update(&mut ctx);
            assert!(!engine.is_finished());
            assert!(engine.has_new_frame());
            engine.render(&mut ctx);
        }
        assert_eq!(surface.canvas.get_pixel(0, 0).0, [0, 0, 255]);

        // Advance past frame 1 in `Once` mode -> marks finished
        std::thread::sleep(Duration::from_millis(38));
        {
            let mut ctx = EngineContext {
                surface: &mut surface,
                config: &config,
            };
            engine.update(&mut ctx);
            assert!(engine.is_finished());
            assert!(!engine.has_new_frame());
        }

        // Switch to `Loop` mode on a new session token -> wraps around cleanly without finishing
        config.set_ingame_showcase(Some(InGameShowcasePayload {
            session_token: 43,
            stream,
            cycle_mode: InGameCycleMode::Loop,
        }));
        {
            let mut ctx = EngineContext {
                surface: &mut surface,
                config: &config,
            };
            engine.update(&mut ctx);
            assert!(!engine.is_finished());
            assert!(engine.has_new_frame());
            engine.render(&mut ctx);
        }
        assert_eq!(surface.canvas.get_pixel(0, 0).0, [255, 0, 0]);

        engine.deactivate();
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_registry_recycle_preserves_frontend_sync_ingame_engine() {
        let tmp_dir = std::env::temp_dir().join(format!(
            "arcadematrix_recycle_sync_test_{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&tmp_dir);
        let config = Config::new(tmp_dir.join("config.json"));
        let mut surface = MockSurface::new(2, 2);
        let mut runtime = EngineRuntime::new();

        let h_sync = runtime.register_instance_handle("__ingame_showcase", "frontend_sync");
        {
            let mut ctx = EngineContext {
                surface: &mut surface,
                config: &config,
            };
            assert!(runtime.init_instance(h_sync, &mut ctx, &HashMap::new()));
        }
        assert!(runtime.recycle_instance(h_sync, false));
        let recycled = runtime
            .get_active_instance(h_sync)
            .expect("frontend_sync instance must remain initialized after recycle");
        // DummyEngine returns allows_overlay() == true (default), whereas InGameShowcaseEngine returns false!
        assert!(!recycled.allows_overlay());
        assert!(!recycled.allow_rotation());
        assert!(recycled.self_paced());

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_256x64_banner_transparent_png_backdrop_and_nested_ra_recent() {
        let tmp_dir = std::env::temp_dir().join(format!(
            "arcadematrix_256x64_sync_test_{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&tmp_dir);
        let config = Config::new(tmp_dir.join("config.json"));
        {
            let mut s = config.settings.write();
            s.matrix.width = 128;
            s.matrix.height = 64;
            s.matrix.chain_length = 2;
            s.matrix.parallel = 1;
        }

        // 1. 256x64 banner sizing: <= 15 chars -> 2x static, <= 31 chars -> 1x static, > 31 chars -> 2x left scroll
        let b_short = build_cyan_sync_banner(&config, "Metal Slug X");
        assert_eq!(b_short.size, 2);
        assert_eq!(b_short.direction, "none");
        assert_eq!(b_short.text, "Metal Slug X");

        let b_mid = build_cyan_sync_banner(&config, "Super Street Fighter II Turbo");
        assert_eq!(b_mid.size, 1);
        assert_eq!(b_mid.direction, "none");
        assert_eq!(b_mid.text, "Super Street Fighter II Turbo");

        let b_long =
            build_cyan_sync_banner(&config, "The King Of Fighters 98 Ultimate Match Hero");
        assert_eq!(b_long.size, 2);
        assert_eq!(b_long.direction, "left");
        assert_eq!(
            b_long.text,
            " The King Of Fighters 98 Ultimate Match Hero "
        );

        // 2. Transparent RGBA pixel with white RGB `(255, 255, 255, 0)` must composite to pure black `(0, 0, 0)`
        let png_path = tmp_dir.join("transparent_white.png");
        let mut rgba = image::RgbaImage::new(2, 1);
        rgba.put_pixel(0, 0, image::Rgba([255, 255, 255, 0]));
        rgba.put_pixel(1, 0, image::Rgba([200, 100, 0, 128]));
        rgba.save(&png_path).unwrap();
        let decoded = open_marquee_image(&png_path).expect("decoded RGBA marquee");
        assert_eq!(decoded.get_pixel(0, 0).0, [0, 0, 0]);
        assert!(decoded.get_pixel(1, 0).0[0] > 95 && decoded.get_pixel(1, 0).0[0] < 105);

        // 3. Nested `ra.recent.title` from companion sync is parsed into `recent_badge`
        let mqtt_cfg = MqttConfig::default();
        let meta_json = serde_json::json!({
            "status": "sync_meta",
            "system": "neogeo",
            "game": "mslugx",
            "ra": {
                "unlocked": 5,
                "total": 40,
                "points": 50,
                "points_total": 400,
                "hardcore": true,
                "recent": { "title": "Mission 1 Complete", "pts": 10 }
            }
        });
        let parsed = parse_sync_meta_from_json(&meta_json, 7, &mqtt_cfg).expect("valid RA meta");
        assert!(parsed.ra.valid);
        let badge_end = parsed
            .ra
            .recent_badge
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(parsed.ra.recent_badge.len());
        assert_eq!(
            std::str::from_utf8(&parsed.ra.recent_badge[..badge_end]).unwrap(),
            "Mission 1 Complete"
        );

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}

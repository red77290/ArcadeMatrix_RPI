use crate::core::config::{Config, MqttConfig};
use crate::core::engine_contract::{Engine, EngineConfig, EngineContext, EngineError};
use crate::core::i18n::Lang;
use crate::core::types::{
    copy_str_to_pod, DisplayGeometry, ForcedEngineMode, InGameCycleMode, InGameShowcasePayload,
    SyncCelebrationPayload, SyncHiscoreEntryPod, SyncMetaPayload, SyncRaSummaryPod,
};
use crate::engines::renderers::fireworks_lut::FireworksLutRenderer;
use crate::engines::renderers::media_showcase::{
    parse_host_and_port, probe_local_lan_ip, probe_showcase_health_details, resolve_showcase_host,
    resolve_showcase_server_url, rpi_hardware_short_id,
};
pub use crate::engines::renderers::sync_cards::{
    HallOfFameCard, RetroAchievementsCard, SyncCelebrationRenderer, SyncOfflineGifRenderer,
};
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
    dmd_cache: Arc<crate::core::dmd_cache::DmdCache>,
    req_id: u64,
    system: String,
    game: String,
    fallback_title: String,
    offline_dwell_ms: u64,
) {
    std::thread::spawn(move || {
        let launch_started = Instant::now();
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

        if !mqtt_cfg.ingame_showcase_enabled {
            return;
        }
        if MQTT_REQUEST_ID.load(Ordering::Acquire) != req_id {
            return;
        }

        // 1. Offline-First Fast Path: check local `showcase[_tate]/<system>/<rom>.gif` (0 network calls,
        //    works even when remote `showcase.enabled` is false or Wi-Fi is disconnected).
        if let Some(offline_gif_path) =
            dmd_cache.resolve_offline_showcase_gif_path(&system, &game, height > width)
        {
            if let Some(frames) = crate::engines::gif::GifEngine::decode_gif_frames(
                &offline_gif_path,
                width.max(1),
                height.max(1),
            ) {
                let cycle_mode = parse_cycle_mode(&mqtt_cfg);
                let target_dwell = Duration::from_millis(offline_dwell_ms);
                while launch_started.elapsed() < target_dwell {
                    if MQTT_REQUEST_ID.load(Ordering::Acquire) != req_id {
                        return;
                    }
                    let rem = target_dwell.saturating_sub(launch_started.elapsed());
                    std::thread::sleep(rem.min(Duration::from_millis(50)));
                }
                if MQTT_REQUEST_ID.load(Ordering::Acquire) != req_id {
                    return;
                }
                info!(
                    "[FrontendSync] Offline In-Game Showcase RGB888 GIF ready for {}/{} -> {} ({} frames, {:?})",
                    system,
                    game,
                    offline_gif_path.display(),
                    frames.len(),
                    cycle_mode
                );
                crate::core::matrix::reset_mock_distinct_crcs();
                e2e_telemetry().lock().showcase_streams_loaded += 1;
                config.set_ingame_showcase(Some(InGameShowcasePayload::from_offline_gif(
                    req_id,
                    Arc::new(frames),
                    width.max(1),
                    height.max(1),
                    cycle_mode,
                )));
                return;
            }
        }

        if !showcase_enabled || raw_server_url.trim().is_empty() {
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
        config.set_ingame_showcase(Some(InGameShowcasePayload::from_stream(
            req_id, stream, cycle_mode,
        )));
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
                Arc::clone(dmd_cache),
                req_id,
                system,
                game,
                raw_title,
                3500,
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
            let mut was_mqtt_active = false;
            let mut conn_seq: u32 = 0;

            loop {
                let target = resolve_broker_target(&config, false);
                if !target.mqtt_enabled {
                    let prev_req = MQTT_REQUEST_ID.swap(0, Ordering::AcqRel);
                    if was_mqtt_active || shown_waiting_banner || prev_req != 0 {
                        was_mqtt_active = false;
                        shown_waiting_banner = false;
                        cancel_active_ingame_showcase(&config);
                        config.set_sync_celebration(None);
                        config.set_marquee_image(None);
                        config.clear_forced_engine();
                        info!("[FrontendSync] Console Sync paused — resumed idle rotation.");
                    }
                    let (lock, cv) = ready_signal();
                    if let Ok(guard) = lock.lock() {
                        let _ = cv.wait_timeout(guard, Duration::from_secs(2));
                    } else {
                        std::thread::sleep(Duration::from_secs(2));
                    }
                    continue;
                }
                was_mqtt_active = true;

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
        let new_lang = context.config.system_lang_fast();
        if new_lang != self.lang {
            self.lang = new_lang;
            self.pending_draw = true;
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
        SyncCelebrationRenderer::render(
            &mut *context.surface,
            &self.fireworks,
            &self.active_payload,
            self.lang,
            self.last_frame_tick,
        );
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

/// Zero-reallocation realtime playback engine for In-Game Showcase sequences (`DMDZ` v2, `DMDS`, & Offline `RGB888` `.gif`)
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
    gif_frame_idx: usize,
    gif_frame_elapsed: Duration,
    gif_last_drawn_idx: Option<usize>,
    gif_last_tick: Option<Instant>,
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
            gif_frame_idx: 0,
            gif_frame_elapsed: Duration::ZERO,
            gif_last_drawn_idx: None,
            gif_last_tick: None,
            sub_stage: InGameSubStage::ShowcaseStream,
            stage_started_at: None,
            stage_paused_at: None,
            stage_paused_ms: 0,
            stage_elapsed_ms: 0,
            last_card_tick: u32::MAX,
            lang: Lang::En,
        }
    }

    #[inline]
    fn reset_gif_cursor(&mut self, now: Instant) {
        self.gif_frame_idx = 0;
        self.gif_frame_elapsed = Duration::ZERO;
        self.gif_last_drawn_idx = None;
        self.gif_last_tick = Some(now);
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
        self.reset_gif_cursor(now);
        self.player.start(now);
    }

    fn pause(&mut self) {
        let now = Instant::now();
        self.pending_draw = false;
        if self.stage_paused_at.is_none() {
            self.stage_paused_at = Some(now);
        }
        self.gif_last_tick = None;
        self.player.pause(now);
    }

    fn resume(&mut self) {
        let now = Instant::now();
        if let Some(p) = self.stage_paused_at.take() {
            self.stage_paused_ms = self
                .stage_paused_ms
                .saturating_add(now.duration_since(p).as_millis() as u64);
        }
        self.gif_last_tick = Some(now);
        self.gif_last_drawn_idx = None;
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
        self.gif_frame_idx = 0;
        self.gif_frame_elapsed = Duration::ZERO;
        self.gif_last_drawn_idx = None;
        self.gif_last_tick = None;
        self.player.reset();
    }

    fn on_display_geometry_changed(&mut self, geometry: &DisplayGeometry) {
        self.surface_width = geometry.width();
        self.surface_height = geometry.height();
        self.gif_last_drawn_idx = None;
        self.player.invalidate_drawn_frame();
        if !self.finished && self.stage_paused_at.is_none() {
            self.pending_draw = true;
        }
    }

    fn update(&mut self, context: &mut EngineContext) {
        let new_lang = context.config.system_lang_fast();
        if new_lang != self.lang {
            self.lang = new_lang;
            self.pending_draw = true;
        }
        self.surface_width = context.surface.width();
        self.surface_height = context.surface.height();
        if self.stage_paused_at.is_some() || self.player.is_paused() {
            self.pending_draw = false;
            return;
        }

        let snap = context.config.producer_snapshot.load();
        let ig_opt = snap.ingame_showcase.as_ref().filter(|ig| !ig.is_empty());
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
            self.reset_gif_cursor(now);
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
                        self.reset_gif_cursor(now);
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
                        self.reset_gif_cursor(now);
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
        let (dim_w, dim_h) = ig.dimensions();
        self.stream_width = dim_w;
        self.stream_height = dim_h;

        let has_cards = meta_opt.is_some();
        let loop_playback = matches!(ig.cycle_mode, InGameCycleMode::Loop) && !has_cards;

        if let Some(ref gif_data) = ig.offline_gif {
            let frames = gif_data.frames.as_slice();
            if frames.is_empty() {
                self.finished = true;
                self.pending_draw = false;
                return;
            }
            if let Some(prev) = self.gif_last_tick {
                self.gif_frame_elapsed = self
                    .gif_frame_elapsed
                    .saturating_add(now.duration_since(prev));
            }
            self.gif_last_tick = Some(now);

            let mut gif_done = false;
            while !frames.is_empty() && self.gif_frame_elapsed >= frames[self.gif_frame_idx].1 {
                let delay = frames[self.gif_frame_idx].1;
                self.gif_frame_elapsed = self.gif_frame_elapsed.saturating_sub(delay);
                self.gif_frame_idx += 1;
                if self.gif_frame_idx >= frames.len() {
                    if loop_playback {
                        self.gif_frame_idx = 0;
                    } else {
                        self.gif_frame_idx = frames.len() - 1;
                        gif_done = true;
                        break;
                    }
                }
            }

            if gif_done && matches!(ig.cycle_mode, InGameCycleMode::Loop) && has_cards {
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

            self.finished = gif_done;
            self.pending_draw = !gif_done && self.gif_last_drawn_idx != Some(self.gif_frame_idx);
            return;
        }

        let Some(ref stream) = ig.stream else {
            self.finished = true;
            self.pending_draw = false;
            return;
        };

        let stream_done = self.player.advance(stream, now, loop_playback, None);
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
        self.pending_draw = self.player.has_new_frame(stream);
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
                    if let Some(ref gif_data) = ig.offline_gif {
                        SyncOfflineGifRenderer::render(
                            &mut *context.surface,
                            gif_data.frames.as_slice(),
                            self.gif_frame_idx,
                        );
                        self.gif_last_drawn_idx = Some(self.gif_frame_idx);
                    } else if let Some(ref stream) = ig.stream {
                        self.player.render_to_surface(stream, &mut *context.surface);
                    }
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

        config.set_ingame_showcase(Some(InGameShowcasePayload::from_stream(
            42,
            Arc::clone(&stream),
            InGameCycleMode::Once,
        )));

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
        config.set_ingame_showcase(Some(InGameShowcasePayload::from_stream(
            43,
            stream,
            InGameCycleMode::Loop,
        )));
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
    fn test_offline_showcase_rgb888_gif_sync_without_remote_server_and_cycle_modes() {
        use image::codecs::gif::GifEncoder;
        use image::{Delay, Frame, Rgba, RgbaImage};

        let tmp_dir = std::env::temp_dir().join(format!(
            "arcadematrix_offline_rgb888_gif_test_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp_dir);
        let marquee_dir = tmp_dir.join("marquees");
        let showcase_snes_dir = tmp_dir.join("showcase").join("snes");
        let pixelcade_snes_dir = tmp_dir.join("pixelcade").join("snes");
        std::fs::create_dir_all(&marquee_dir).unwrap();
        std::fs::create_dir_all(&showcase_snes_dir).unwrap();
        std::fs::create_dir_all(&pixelcade_snes_dir).unwrap();

        // 1. Write companion non-interlaced marquee PNG (`pixelcade/snes/smw.png`)
        let mut mq_img = image::RgbImage::new(4, 2);
        mq_img.put_pixel(0, 0, image::Rgb([12, 234, 56]));
        mq_img.save(pixelcade_snes_dir.join("smw.png")).unwrap();

        // 2. Write 2-frame pure 24-bit RGB888 animated GIF (`showcase/snes/smw.gif`)
        //    using non-RGB565-quantized channels `(123, 45, 211)` and `(19, 207, 131)`!
        let gif_path = showcase_snes_dir.join("smw.gif");
        {
            let f_out = std::fs::File::create(&gif_path).unwrap();
            let mut enc = GifEncoder::new(f_out);
            let f0_img = RgbaImage::from_pixel(4, 2, Rgba([123, 45, 211, 255]));
            let f1_img = RgbaImage::from_pixel(4, 2, Rgba([19, 207, 131, 255]));
            enc.encode_frame(Frame::from_parts(
                f0_img,
                0,
                0,
                Delay::from_numer_denom_ms(30, 1),
            ))
            .unwrap();
            enc.encode_frame(Frame::from_parts(
                f1_img,
                0,
                0,
                Delay::from_numer_denom_ms(30, 1),
            ))
            .unwrap();
        }

        let config = Arc::new(Config::new(tmp_dir.join("config.json")));
        {
            let mut s = config.settings.write();
            s.matrix.width = 4;
            s.matrix.height = 2;
            s.matrix.chain_length = 1;
            s.matrix.parallel = 1;
            s.showcase.enabled = false; // Remote Showcase server explicitly DISABLED!
            s.showcase.server_url.clear();
            s.mqtt.ingame_showcase_enabled = true;
            s.mqtt.ingame_cycle_mode = "once".to_string();
        }
        let dmd_cache = Arc::new(crate::core::dmd_cache::DmdCache::new(&marquee_dir));

        // Verify companion `pixelcade/snes/smw.png` & offline `showcase/snes/smw.gif` both resolve
        assert!(dmd_cache.get_cached_path("snes", "./smw.sfc").is_some());
        assert_eq!(
            dmd_cache.resolve_offline_showcase_gif_path("snes", "./smw.sfc", false),
            Some(gif_path.clone())
        );

        let req_id = MQTT_REQUEST_ID.fetch_add(1, Ordering::AcqRel) + 1;
        spawn_ingame_showcase_request(
            Arc::clone(&config),
            Arc::clone(&dmd_cache),
            req_id,
            "snes".to_string(),
            "./smw.sfc".to_string(),
            "Super Mario World".to_string(),
            10,
        );

        // Wait briefly for off-thread GIF decode (`<= 250 ms`, 0 network calls)
        let wait_t0 = Instant::now();
        while config.producer_snapshot.load().ingame_showcase.is_none()
            && wait_t0.elapsed() < Duration::from_millis(500)
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        let published = config
            .producer_snapshot
            .load()
            .ingame_showcase
            .clone()
            .expect("offline RGB888 GIF payload published without remote server");
        assert!(published.offline_gif.is_some());

        // Render Frame 0 & Frame 1 on `InGameShowcaseEngine` -> exact 24-bit RGB888 channels preserved!
        let mut engine = InGameShowcaseEngine::new();
        let mut surface = MockSurface::new(4, 2);
        engine.activate();
        {
            let mut ctx = EngineContext {
                surface: &mut surface,
                config: &config,
            };
            engine.update(&mut ctx);
            assert!(engine.has_new_frame());
            engine.render(&mut ctx);
        }
        assert_eq!(surface.canvas.get_pixel(0, 0).0, [123, 45, 211]);

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
        assert_eq!(surface.canvas.get_pixel(0, 0).0, [19, 207, 131]);

        std::thread::sleep(Duration::from_millis(38));
        {
            let mut ctx = EngineContext {
                surface: &mut surface,
                config: &config,
            };
            engine.update(&mut ctx);
            assert!(engine.is_finished());
        }

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

        let b_long = build_cyan_sync_banner(&config, "The King Of Fighters 98 Ultimate Match Hero");
        assert_eq!(b_long.size, 2);
        assert_eq!(b_long.direction, "left");
        assert_eq!(b_long.text, " The King Of Fighters 98 Ultimate Match Hero ");

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

    #[test]
    fn test_handle_frontend_sync_payload_e2e_offline_tate_preemption_and_stopped() {
        let tmp_dir = std::env::temp_dir().join(format!(
            "arcadematrix_e2e_mqtt_offline_tate_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp_dir);
        let marquee_dir = tmp_dir.join("marquees");
        let tate_showcase_dir = tmp_dir.join("showcase_tate").join("neogeo");
        let px_dir = tmp_dir.join("pixelcade").join("neogeo");
        std::fs::create_dir_all(&marquee_dir).unwrap();
        std::fs::create_dir_all(&tate_showcase_dir).unwrap();
        std::fs::create_dir_all(&px_dir).unwrap();

        // Write valid 2-frame RGB888 TATE GIF (`showcase_tate/neogeo/mslugx.gif`) + companion marquee `.png`
        {
            use image::{Delay, Frame, RgbaImage};
            let gif_file = std::fs::File::create(tate_showcase_dir.join("mslugx.gif")).unwrap();
            let mut enc = image::codecs::gif::GifEncoder::new(gif_file);
            enc.set_repeat(image::codecs::gif::Repeat::Infinite)
                .unwrap();
            let f0 = Frame::from_parts(
                RgbaImage::from_pixel(4, 8, image::Rgba([250, 90, 20, 255])),
                0,
                0,
                Delay::from_numer_denom_ms(40, 1),
            );
            let f1 = Frame::from_parts(
                RgbaImage::from_pixel(4, 8, image::Rgba([30, 220, 140, 255])),
                0,
                0,
                Delay::from_numer_denom_ms(40, 1),
            );
            enc.encode_frames(vec![f0, f1].into_iter()).unwrap();

            let marquee_img = image::RgbImage::from_pixel(4, 8, image::Rgb([15, 80, 210]));
            marquee_img.save(px_dir.join("mslugx.png")).unwrap();
        }

        let config = Arc::new(Config::new(tmp_dir.join("config.json")));
        {
            let mut s = config.settings.write();
            // TATE 64x128 (`rotation = 90` on 128x64 panel -> effective width=64, height=128)
            s.matrix.width = 128;
            s.matrix.height = 64;
            s.matrix.chain_length = 1;
            s.matrix.parallel = 1;
            s.matrix.rotation = 90;
            s.mqtt.ingame_showcase_enabled = true;
            s.mqtt.ingame_cycle_mode = "once".to_string();
            s.mqtt.sync_celebrate_events = true;
            // Showcase server disabled -> proves 100% autonomous local offline resolution!
            s.showcase.enabled = false;
            s.showcase.server_url.clear();
        }
        let dmd_cache = Arc::new(crate::core::dmd_cache::DmdCache::new(&marquee_dir));
        assert!(dmd_cache
            .resolve_offline_showcase_gif_path("neogeo", "mslugx.zip", true)
            .is_some());

        // 1. Virtual collection `favorites` + `/recalbox/share/roms/neogeo/mslugx.zip` in `playing` state:
        //    must immediately display companion marquee `pixelcade/neogeo/mslugx.png` AND spawn TATE offline GIF!
        handle_frontend_sync_payload(
            &config,
            &dmd_cache,
            r#"{"status":"playing","system":"favorites","game":"mslugx.zip","title":"Metal Slug X","rom_path":"/recalbox/share/roms/neogeo/mslugx.zip"}"#,
        );
        assert_eq!(
            config.producer_snapshot.load().forced_mode,
            ForcedEngineMode::Marquee
        );
        assert!(e2e_telemetry()
            .lock()
            .active_marquee_path
            .contains("pixelcade"));

        // 2. Inject `sync_meta` & `celebrate` while in-game -> both atomic snapshots populated
        handle_frontend_sync_payload(
            &config,
            &dmd_cache,
            r#"{"status":"sync_meta","system":"neogeo","game":"mslugx","hiscores":[{"rank":1,"tag":"SNK","score":"1,500,000"}]}"#,
        );
        assert!(config.producer_snapshot.load().sync_meta.is_some());

        handle_frontend_sync_payload(
            &config,
            &dmd_cache,
            r#"{"status":"celebrate","kind":"highscore","system":"neogeo","game":"mslugx","rank":1,"tag":"SNK","score":"2,000,000"}"#,
        );
        assert!(config.producer_snapshot.load().sync_celebration.is_some());

        // 3. `stopped` event immediately cancels in-game Showcase & SyncMeta while keeping last marquee displayed!
        handle_frontend_sync_payload(&config, &dmd_cache, r#"{"status":"stopped"}"#);
        let snap_after_stop = config.producer_snapshot.load();
        assert!(snap_after_stop.ingame_showcase.is_none());
        assert!(snap_after_stop.sync_meta.is_none());
        assert_eq!(snap_after_stop.forced_mode, ForcedEngineMode::Marquee);

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}

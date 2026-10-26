use crate::core::config::{Config, MqttConfig};
use crate::core::engine_contract::{Engine, EngineConfig, EngineContext, EngineError};
use crate::core::types::{
    DisplayGeometry, ForcedEngineMode, InGameCycleMode, InGameShowcasePayload,
};
use crate::engines::renderers::media_showcase::{
    parse_host_and_port, parse_rgb565_stream_compact, probe_local_lan_ip,
    probe_showcase_health_details, resolve_showcase_host, resolve_showcase_server_url,
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

#[derive(Debug, Clone)]
struct ActiveSyncQueueItem {
    req_id: u64,
    server_url: String,
    sync_client_id: String,
    queue_id: String,
}

static ACTIVE_SYNC_QUEUE: OnceLock<Mutex<Option<ActiveSyncQueueItem>>> = OnceLock::new();

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
    let clean_name = stripped.replace('-', " ").replace('_', " ");
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
    let systems: Vec<&str> = if system.trim().is_empty() {
        Vec::new()
    } else {
        vec![system.trim()]
    };
    let mut obj = serde_json::Map::new();
    obj.insert("domain".to_string(), json!("games"));
    obj.insert("systems".to_string(), json!(systems));
    obj.insert("query".to_string(), json!(game.trim()));
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

fn delete_showcase_queue_item_async(item: ActiveSyncQueueItem) {
    std::thread::spawn(move || {
        let url = format!(
            "{}/api/v1/clients/{}/queue/{}",
            item.server_url.trim_end_matches('/'),
            item.sync_client_id,
            item.queue_id
        );
        let client = crate::core::memory::fast_http_client();
        let _ = client
            .delete(&url)
            .timeout(Duration::from_millis(1500))
            .send();
    });
}

pub fn cancel_active_ingame_showcase(config: &Config) {
    config.set_ingame_showcase(None);
    let slot = ACTIVE_SYNC_QUEUE.get_or_init(|| Mutex::new(None));
    if let Some(prev) = slot.lock().take() {
        delete_showcase_queue_item_async(prev);
    }
    let (_, cv) = ready_signal();
    cv.notify_all();
}

/// Called from `app.rs` when `InGameCycleMode::Once` completes so the server-side temporary queue item is also purged.
pub fn notify_ingame_once_completed(session_token: u64) {
    let slot = ACTIVE_SYNC_QUEUE.get_or_init(|| Mutex::new(None));
    let mut guard = slot.lock();
    if guard
        .as_ref()
        .map_or(false, |it| it.req_id == session_token)
    {
        if let Some(prev) = guard.take() {
            delete_showcase_queue_item_async(prev);
        }
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
    if !sync_client_id.ends_with("_sync") {
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

fn spawn_ingame_showcase_request(config: Arc<Config>, req_id: u64, system: String, game: String) {
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
        let sync_client_id = format!("rpi_games_{}x{}_sync", width, height);
        let enqueue_url = format!(
            "{}/api/v1/clients/{}/queue/async",
            resolved_url.trim_end_matches('/'),
            sync_client_id
        );
        let body = build_ingame_queue_payload(&mqtt_cfg, &system, &game, width, height, &lang);
        let cycle_mode = parse_cycle_mode(&mqtt_cfg);

        let api_client = crate::core::memory::api_http_client();
        let resp = match api_client
            .post(&enqueue_url)
            .timeout(Duration::from_secs(4))
            .json(&body)
            .send()
        {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                warn!(
                    "[FrontendSync] Showcase queue/async returned HTTP {} for {}/{}",
                    r.status(),
                    system,
                    game
                );
                return;
            }
            Err(e) => {
                warn!("[FrontendSync] Showcase queue/async unreachable: {}", e);
                return;
            }
        };

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

        // Register active queue item; if game already changed while HTTP POST was in flight, delete immediately!
        {
            let slot = ACTIVE_SYNC_QUEUE.get_or_init(|| Mutex::new(None));
            let active_record = ActiveSyncQueueItem {
                req_id,
                server_url: resolved_url.clone(),
                sync_client_id: sync_client_id.clone(),
                queue_id: queue_id.clone(),
            };
            if MQTT_REQUEST_ID.load(Ordering::Acquire) != req_id {
                delete_showcase_queue_item_async(active_record);
                return;
            }
            if let Some(prev) = slot.lock().replace(active_record) {
                delete_showcase_queue_item_async(prev);
            }
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
            return;
        }

        // Wait for async preparation (0 ms on MQTT `play_now` signal, or <= 750 ms via lightweight queue poll)
        let wait_deadline = Instant::now() + Duration::from_secs(60);
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
                                        warn!(
                                            "[FrontendSync] Showcase failed to prepare {}/{}: {:?}",
                                            system,
                                            game,
                                            it.get("error_msg")
                                        );
                                        return;
                                    }
                                }
                                None => return,
                            }
                        }
                    }
                }
            }
        }

        let Some(item_id) = ready_item_id else {
            return;
        };
        if MQTT_REQUEST_ID.load(Ordering::Acquire) != req_id {
            return;
        }

        // Download compact DMDS RGB565 stream (`purge=false` so alternating/loop cycles replay locally with 0 network load)
        let stream_url = format!(
            "{}/api/v1/games/stream/{}.rgb565.stream?purge=false",
            resolved_url.trim_end_matches('/'),
            item_id
        );
        let stream_client = crate::core::memory::stream_http_client();
        let Ok(stream_resp) = stream_client
            .get(&stream_url)
            .timeout(Duration::from_secs(12))
            .send()
        else {
            return;
        };
        if !stream_resp.status().is_success() {
            return;
        }
        let Ok(bytes) = stream_resp.bytes() else {
            return;
        };
        let Some(stream) = parse_rgb565_stream_compact(&bytes) else {
            warn!(
                "[FrontendSync] Invalid DMDS stream received for item {}",
                item_id
            );
            return;
        };

        if MQTT_REQUEST_ID.load(Ordering::Acquire) != req_id {
            return;
        }

        info!(
            "[FrontendSync] Late preemption ready for {}/{} ({} frames, {:?})",
            system, game, stream.frame_count, cycle_mode
        );
        config.set_ingame_showcase(Some(InGameShowcasePayload {
            session_token: req_id,
            stream,
            cycle_mode,
        }));
    });
}

fn handle_frontend_sync_payload(
    config: &Arc<Config>,
    dmd_cache: &Arc<crate::core::dmd_cache::DmdCache>,
    payload: &str,
) {
    info!("MQTT game payload: {}", payload);
    let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) else {
        return;
    };
    let status = json["status"].as_str().unwrap_or("stopped");
    if status == "stopped" {
        MQTT_REQUEST_ID.fetch_add(1, Ordering::AcqRel);
        cancel_active_ingame_showcase(config);
        info!("Received stopped event, cancelled in-game Showcase and keeping last marquee displayed.");
        return;
    }

    let raw_system = json["system"].as_str().unwrap_or("").to_string();
    let raw_game = json["game"].as_str().unwrap_or("").to_string();

    let clean_sys = crate::core::dmd_cache::clean_system_name(&raw_system);
    let clean_game = crate::core::dmd_cache::clean_system_name(&raw_game);

    let is_playing = status == "playing";
    let is_system_event = !is_playing
        && (json["type"].as_str() == Some("system")
            || clean_game.is_empty()
            || clean_game.eq_ignore_ascii_case(&clean_sys));

    let system = if !clean_sys.is_empty() {
        clean_sys
    } else {
        raw_system
    };
    let game_clean = if !clean_game.is_empty() {
        clean_game
    } else {
        raw_game.clone()
    };

    let req_id = MQTT_REQUEST_ID.fetch_add(1, Ordering::AcqRel) + 1;
    // Any new browsing or playing event cancels any previously active in-game Showcase session
    cancel_active_ingame_showcase(config);

    if is_system_event {
        if let Some(path) = dmd_cache.get_cached_system_path(&system) {
            if let Ok(img) = image::open(&path) {
                config.set_marquee_image(Some(img.to_rgb8()));
                config.set_forced_engine_mode(ForcedEngineMode::Marquee);
                return;
            }
        }

        let clean_name = format_game_name(&crate::core::dmd_cache::clean_system_name(&system));
        let mut text_to_show = clean_name.clone();
        if text_to_show.len() > 10 {
            text_to_show = format!(" {} ", text_to_show);
        }
        let msg_payload = crate::engines::message::MessagePayload::new(
            text_to_show,
            "#00ffff",
            1,
            if clean_name.len() > 8 { "left" } else { "none" },
            0,
        );
        config.set_frontend_sync_banner(msg_payload);

        let config_clone = Arc::clone(config);
        let cache_clone = Arc::clone(dmd_cache);
        std::thread::spawn(move || {
            if let Some(path) = cache_clone.download_system_marquee(&system) {
                if let Ok(img) = image::open(&path) {
                    if MQTT_REQUEST_ID.load(Ordering::Acquire) == req_id {
                        config_clone.set_marquee_image(Some(img.to_rgb8()));
                        config_clone.set_forced_engine_mode(ForcedEngineMode::Marquee);
                    }
                }
            }
        });
    } else {
        let game = if !raw_game.is_empty() {
            raw_game
        } else {
            "Unknown".to_string()
        };
        let clean_name = format_game_name(&game);

        // 1. Display local cached game/system marquee immediately (< 15 ms) or launch Pixelcade fetch
        let mut marquee_shown = false;
        if let Some(path) = dmd_cache.get_cached_path(&system, &game) {
            if let Ok(img) = image::open(&path) {
                config.set_marquee_image(Some(img.to_rgb8()));
                config.set_forced_engine_mode(ForcedEngineMode::Marquee);
                marquee_shown = true;
            }
        }

        if !marquee_shown && dmd_cache.is_negative_cached(&system, &game) {
            if let Some(path) = dmd_cache.get_cached_system_path(&system) {
                if let Ok(img) = image::open(&path) {
                    config.set_marquee_image(Some(img.to_rgb8()));
                    config.set_forced_engine_mode(ForcedEngineMode::Marquee);
                    marquee_shown = true;
                }
            }
        }

        if !marquee_shown {
            let mut text_to_show = clean_name.clone();
            if text_to_show.len() > 10 {
                text_to_show = format!(" {} ", text_to_show);
            }
            let msg_payload = crate::engines::message::MessagePayload::new(
                text_to_show,
                "#00ffff",
                1,
                if clean_name.len() > 8 { "left" } else { "none" },
                0,
            );
            config.set_frontend_sync_banner(msg_payload);

            let config_clone = Arc::clone(config);
            let cache_clone = Arc::clone(dmd_cache);
            let sys_for_dl = system.clone();
            let game_for_dl = game.clone();
            std::thread::spawn(move || {
                let path_opt = cache_clone
                    .download_marquee(&sys_for_dl, &game_for_dl)
                    .or_else(|| cache_clone.download_system_marquee(&sys_for_dl));
                if let Some(path) = path_opt {
                    if let Ok(img) = image::open(&path) {
                        if MQTT_REQUEST_ID.load(Ordering::Acquire) == req_id {
                            config_clone.set_marquee_image(Some(img.to_rgb8()));
                            config_clone.set_forced_engine_mode(ForcedEngineMode::Marquee);
                        }
                    }
                }
            });
        }

        // 2. When launching a game (`status == "playing"`), request Showcase sequence in background for Late Preemption
        if is_playing && !game_clean.is_empty() {
            spawn_ingame_showcase_request(Arc::clone(config), req_id, system, game_clean);
        }
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

                let (user, pass) = {
                    let s = config.settings.read();
                    (s.mqtt.user.clone(), s.mqtt.pass.clone())
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
                let client_id = format!("arcadematrix_rpi_{}", conn_seq % 1000);
                info!(
                    "[FrontendSync] Connecting to MQTT broker {}:{} (source: {})",
                    target.connect_host, target.port, target.source
                );

                let mut mqttoptions =
                    MqttOptions::new(client_id, &target.connect_host, target.port);
                mqttoptions.set_keep_alive(Duration::from_secs(15));
                if target.source == "local" && !user.is_empty() {
                    mqttoptions.set_credentials(&user, &pass);
                }

                let (client, mut connection) = Client::new(mqttoptions, 16);

                if let Err(e) = client.subscribe(MQTT_TOPIC_WILDCARD, QoS::AtMostOnce) {
                    error!(
                        "Failed to subscribe to MQTT topic {}: {}",
                        MQTT_TOPIC_WILDCARD, e
                    );
                    std::thread::sleep(Duration::from_secs(3));
                    continue;
                }
                let _ = client.subscribe(SHOWCASE_COMMAND_TOPIC_WILDCARD, QoS::AtMostOnce);

                let mut last_broker_recheck = Instant::now();

                for notification in connection.iter() {
                    if CONFIG_GENERATION.load(Ordering::SeqCst) != observed_cfg_gen {
                        info!("[FrontendSync] MQTT/Showcase configuration changed, reconnecting...");
                        let _ = client.disconnect();
                        break;
                    }
                    if last_broker_recheck.elapsed() >= Duration::from_secs(12) {
                        last_broker_recheck = Instant::now();
                        let refreshed = resolve_broker_target(&config, false);
                        if !refreshed.mqtt_enabled
                            || refreshed.connect_host != target.connect_host
                            || refreshed.port != target.port
                            || refreshed.source != target.source
                        {
                            info!(
                                "[FrontendSync] Broker target switched ({}:{} -> {}:{}), reconnecting...",
                                target.connect_host,
                                target.port,
                                refreshed.connect_host,
                                refreshed.port
                            );
                            let _ = client.disconnect();
                            break;
                        }
                    }

                    match notification {
                        Ok(Event::Incoming(Packet::Publish(publish))) => {
                            if let Ok(payload) = String::from_utf8(publish.payload.to_vec()) {
                                if publish.topic.starts_with("dmd_showcase/clients/") {
                                    record_showcase_command_event(&publish.topic, &payload);
                                } else {
                                    handle_frontend_sync_payload(&config, &dmd_cache, &payload);
                                }
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

                std::thread::sleep(Duration::from_secs(2));
            }
        })
        .expect("Failed to spawn frontend-sync thread");
}

/// Zero-allocation realtime playback engine for In-Game Showcase sequences (`DMDS` RGB565 streams).
/// Preempts `Marquee` only once the complete sequence is ready (`DisplaySourceId::InGameShowcase`).
pub struct InGameShowcaseEngine {
    session_token: u64,
    frame_idx: usize,
    frame_count: usize,
    stream_width: u32,
    stream_height: u32,
    frame_elapsed: Duration,
    last_tick: Option<Instant>,
    last_drawn_frame: Option<usize>,
    finished: bool,
    paused: bool,
}

impl InGameShowcaseEngine {
    pub fn new() -> Self {
        Self {
            session_token: 0,
            frame_idx: 0,
            frame_count: 0,
            stream_width: 0,
            stream_height: 0,
            frame_elapsed: Duration::ZERO,
            last_tick: None,
            last_drawn_frame: None,
            finished: false,
            paused: false,
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
        _context: &mut EngineContext,
        _config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        Ok(())
    }

    fn activate(&mut self) {
        self.frame_idx = 0;
        self.frame_elapsed = Duration::ZERO;
        self.last_tick = Some(Instant::now());
        self.last_drawn_frame = None;
        self.finished = false;
        self.paused = false;
    }

    fn pause(&mut self) {
        self.paused = true;
        self.last_tick = None;
    }

    fn resume(&mut self) {
        self.paused = false;
        self.last_tick = Some(Instant::now());
        self.last_drawn_frame = None;
    }

    fn deactivate(&mut self) {
        self.frame_idx = 0;
        self.frame_count = 0;
        self.frame_elapsed = Duration::ZERO;
        self.last_tick = None;
        self.last_drawn_frame = None;
        self.finished = false;
        self.paused = false;
    }

    fn update(&mut self, context: &mut EngineContext) {
        if self.paused {
            return;
        }
        let snap = context.config.producer_snapshot.load();
        let Some(ref ig) = snap.ingame_showcase else {
            self.finished = true;
            return;
        };
        if ig.stream.is_empty() {
            self.finished = true;
            return;
        }

        if self.session_token != ig.session_token {
            self.session_token = ig.session_token;
            self.frame_idx = 0;
            self.frame_elapsed = Duration::ZERO;
            self.last_drawn_frame = None;
            self.finished = false;
            self.last_tick = Some(Instant::now());
        }

        self.frame_count = ig.stream.frame_count;
        self.stream_width = ig.stream.width;
        self.stream_height = ig.stream.height;

        if self.finished {
            return;
        }

        let now = Instant::now();
        let dt = self
            .last_tick
            .map(|t| now.duration_since(t))
            .unwrap_or(Duration::ZERO);
        self.last_tick = Some(now);
        self.frame_elapsed += dt;

        let delay = ig.stream.frame_delay.max(Duration::from_millis(16));
        while self.frame_elapsed >= delay {
            self.frame_elapsed -= delay;
            if self.frame_idx + 1 < self.frame_count {
                self.frame_idx += 1;
            } else {
                match ig.cycle_mode {
                    InGameCycleMode::Loop => {
                        self.frame_idx = 0;
                    }
                    InGameCycleMode::Once | InGameCycleMode::Alternating { .. } => {
                        self.finished = true;
                        break;
                    }
                }
            }
        }
    }

    fn render(&mut self, context: &mut EngineContext) {
        let snap = context.config.producer_snapshot.load();
        if let Some(ref ig) = snap.ingame_showcase {
            ig.stream
                .draw_frame_to_surface(self.frame_idx, &mut *context.surface);
            self.last_drawn_frame = Some(self.frame_idx);
        }
    }

    fn wants_clear(&self) -> bool {
        self.stream_width == 0 || self.stream_height == 0
    }

    fn has_new_frame(&self) -> bool {
        !self.finished && self.last_drawn_frame != Some(self.frame_idx)
    }

    fn is_realtime(&self) -> bool {
        !self.finished && !self.paused
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
}

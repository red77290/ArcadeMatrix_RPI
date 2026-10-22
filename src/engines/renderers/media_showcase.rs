//! Shared V3.0 renderer and dual-transport (`mqtt_udp` + `http_poll`) Consume-Once runner
//! (`MediaShowcaseRunner`) for the 4 DMD Media Showcase engines:
//! `retro_games`, `movie_showcase`, `series_showcase`, and `anime_showcase`.
//!
//! V3.0 Architecture:
//! - Strict Consume-Once & Zero Media Cache on the DMD: never caches a list of items or
//!   replays old videos when the server is stopped. If the server goes offline (MQTT LWT
//!   `online: false`, UDP watchdog timeout, or HTTP error), active media is cleared immediately.
//! - Dedicated MQTT Broker Config per engine: `use_showcase_broker = true` automatically
//!   derives `<server_url_host>:1883` without manual configuration; `use_showcase_broker = false`
//!   allows selecting either ArcadeMatrix's `global` MQTT broker or a `custom` broker URL.
//! - Real-Time `mqtt_udp` + `http_poll`: Phase 1 (Info Card) stays in JSON so the DMD renders
//!   the mini-jacket and scrolls 100% of the localized synopsis at `text_scroll_speed`
//!   (`slow` = 16 px/s, `normal` = 24 px/s, `fast` = 36 px/s); Phases 2 & 3 (Panoramas + Video)
//!   stream in real time via `DMDF` UDP packets (`ffmpeg -re`) or HTTP `.rgb565.stream`.
//! - 4 Creative Procedural Pixel-Art "Waiting for Content" Animations (Games, Movies, Series, Anime)
//!   adapted to all DMD panel resolutions (`128x32`, `128x64`, `256x64`, `64x64`, and TATE).

use crate::core::engine_contract::{EngineConfig, EngineContext};
use crate::core::matrix::IDrawingSurface;
use crate::engines::dashboard::font::draw_text_scaled;
use crate::engines::renderers::primitives::{
    draw_fast_hline, draw_fast_vline, draw_round_rect, draw_scrolling_text, fill_round_rect,
};
use crate::engines::renderers::tiny_font::draw_tiny_text;
use image::RgbImage;
use rumqttc::{Client as MqttSyncClient, Event as MqttEvent, LastWill, MqttOptions, Packet, QoS};
use serde::{Deserialize, Serialize};
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{info, warn};

fn fill_rect(
    matrix: &mut dyn IDrawingSurface,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: (u8, u8, u8),
) {
    fill_round_rect(matrix, x, y, w, h, 0, color);
}

fn draw_line(
    matrix: &mut dyn IDrawingSurface,
    mut x0: i32,
    mut y0: i32,
    x1: i32,
    y1: i32,
    color: (u8, u8, u8),
) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        matrix.set_pixel(x0, y0, color.0, color.1, color.2);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn draw_circle(matrix: &mut dyn IDrawingSurface, cx: i32, cy: i32, r: i32, color: (u8, u8, u8)) {
    let mut x = r;
    let mut y = 0;
    let mut err = 0;
    while x >= y {
        matrix.set_pixel(cx + x, cy + y, color.0, color.1, color.2);
        matrix.set_pixel(cx + y, cy + x, color.0, color.1, color.2);
        matrix.set_pixel(cx - y, cy + x, color.0, color.1, color.2);
        matrix.set_pixel(cx - x, cy + y, color.0, color.1, color.2);
        matrix.set_pixel(cx - x, cy - y, color.0, color.1, color.2);
        matrix.set_pixel(cx - y, cy - x, color.0, color.1, color.2);
        matrix.set_pixel(cx + y, cy - x, color.0, color.1, color.2);
        matrix.set_pixel(cx + x, cy - y, color.0, color.1, color.2);
        y += 1;
        if err <= 0 {
            err += 2 * y + 1;
        }
        if err > 0 {
            x -= 1;
            err -= 2 * x + 1;
        }
    }
}

fn fill_circle(matrix: &mut dyn IDrawingSurface, cx: i32, cy: i32, r: i32, color: (u8, u8, u8)) {
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                matrix.set_pixel(cx + dx, cy + dy, color.0, color.1, color.2);
            }
        }
    }
}

pub const DMDS_MAGIC: &[u8; 4] = b"DMDS";
pub const DMDS_HEADER_SIZE: usize = 16;

pub const DMDF_MAGIC: &[u8; 4] = b"DMDF";
pub const DMDF_HEADER_SIZE: usize = 10;

/// Initial hold before the synopsis begins scrolling (ms).
const SYNOPSIS_START_HOLD_MS: u64 = 900;
/// End hold once the final word of the synopsis is on screen (ms).
const SYNOPSIS_END_HOLD_MS: u64 = 700;
/// Minimum card visibility duration when synopsis is disabled or very short (ms).
const MIN_CARD_DISPLAY_MS: u64 = 4500;
/// Watchdog timeout for real-time UDP stream once packets have started arriving.
const UDP_STREAM_WATCHDOG_MS: u64 = 1500;
/// Initial grace period waiting for the first UDP packet after sending `start_stream`.
const UDP_INITIAL_GRACE_MS: u64 = 3500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowcaseDomain {
    Games,
    Movies,
    Series,
    Anime,
}

impl ShowcaseDomain {
    pub const fn as_api_slug(self) -> &'static str {
        match self {
            ShowcaseDomain::Games => "games",
            ShowcaseDomain::Movies => "movies",
            ShowcaseDomain::Series => "series",
            ShowcaseDomain::Anime => "anime",
        }
    }

    pub const fn badge_label(self) -> &'static str {
        match self {
            ShowcaseDomain::Games => "RETRO GAMES",
            ShowcaseDomain::Movies => "MOVIES",
            ShowcaseDomain::Series => "TV SERIES",
            ShowcaseDomain::Anime => "ANIME",
        }
    }

    pub const fn default_accent_rgb(self) -> (u8, u8, u8) {
        match self {
            ShowcaseDomain::Games => (255, 109, 0),
            ShowcaseDomain::Movies => (0, 229, 255),
            ShowcaseDomain::Series => (229, 57, 53),
            ShowcaseDomain::Anime => (255, 64, 129),
        }
    }
}

/// Compute the adaptive mini-jacket `(poster_w, poster_h)` dimensions on the Info Card
/// for any DMD panel resolution or TATE orientation.
pub fn compute_card_poster_dims(width: u32, height: u32) -> (u32, u32) {
    if height > width {
        let side = if width >= 64 { 40 } else { 24 };
        (side, side)
    } else if width == height {
        let side = 32.min(width / 2).max(16);
        (side, side)
    } else if width >= 128 && height >= 64 {
        (48, 64)
    } else {
        let side = 32.min(height).min(width / 3).max(16);
        (side, side)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShowcaseConfig {
    pub server_url: String,
    pub transport_mode: String,
    pub use_showcase_broker: bool,
    pub mqtt_broker_source: String,
    pub mqtt_custom_url: String,
    pub lang: String,
    pub text_scroll_speed: String,
    pub show_poster: bool,
    pub show_synopsis: bool,
    pub jacket_panorama: bool,
    pub marquee_panorama: bool,
    pub snapshot_panorama_count: u32,
    pub show_video: bool,
    pub video_max_duration_s: u32,
    pub video_fps: u32,
    pub video_mode: String,
    pub color_boost: bool,
    pub systems: String,
    pub genres: String,
    pub decades: String,
    pub min_rating: f32,
    pub video_type: String,
}

impl Default for ShowcaseConfig {
    fn default() -> Self {
        Self {
            server_url: "http://127.0.0.1:8080".to_string(),
            transport_mode: "mqtt_udp".to_string(),
            use_showcase_broker: true,
            mqtt_broker_source: "global".to_string(),
            mqtt_custom_url: String::new(),
            lang: "system".to_string(),
            text_scroll_speed: "4".to_string(),
            show_poster: true,
            show_synopsis: true,
            jacket_panorama: true,
            marquee_panorama: false,
            snapshot_panorama_count: 0,
            show_video: true,
            video_max_duration_s: 8,
            video_fps: 15,
            video_mode: "auto_action".to_string(),
            color_boost: true,
            systems: "arcade,neogeo,cps1,cps2,snes,megadrive,psx,dreamcast".to_string(),
            genres: String::new(),
            decades: String::new(),
            min_rating: 6.0,
            video_type: "any".to_string(),
        }
    }
}

impl ShowcaseConfig {
    pub fn from_engine_config(cfg: &dyn EngineConfig, domain: ShowcaseDomain) -> Self {
        let mut out = Self::default();
        let raw_url = cfg.get_string("server_url", "http://127.0.0.1:8080");
        out.server_url = raw_url.trim().trim_end_matches('/').to_string();
        out.transport_mode = cfg.get_string("transport_mode", "mqtt_udp");
        out.use_showcase_broker = cfg.get_bool("use_showcase_broker", true);
        out.mqtt_broker_source = cfg.get_string("mqtt_broker_source", "global");
        out.mqtt_custom_url = cfg.get_string("mqtt_custom_url", "").trim().to_string();
        out.lang = cfg.get_string("lang", "system");
        out.text_scroll_speed = cfg.get_string("text_scroll_speed", "4");
        out.show_poster = cfg.get_bool("show_poster", true);
        out.show_synopsis = cfg.get_bool("show_synopsis", true);
        out.jacket_panorama = cfg.get_bool("jacket_panorama", true);
        out.marquee_panorama = cfg.get_bool("marquee_panorama", false);

        let snap_cnt = cfg.get_int("snapshot_panorama_count", -1);
        if snap_cnt >= 0 {
            out.snapshot_panorama_count = snap_cnt.clamp(0, 5) as u32;
        } else {
            out.snapshot_panorama_count = cfg.get_int("image_count", 0).clamp(0, 5) as u32;
        }

        out.show_video = cfg.get_bool("show_video", true);
        out.video_max_duration_s = cfg.get_int("video_max_duration_s", 8).clamp(2, 30) as u32;
        out.video_fps = cfg.get_int("video_fps", 15).clamp(6, 30) as u32;
        out.video_mode = cfg.get_string(
            "video_mode",
            match domain {
                ShowcaseDomain::Games => "pixel_art",
                ShowcaseDomain::Anime => "anime",
                _ => "auto_action",
            },
        );
        out.color_boost = cfg.get_bool("color_boost", true);
        out.systems = cfg.get_string(
            "systems",
            "arcade,neogeo,cps1,cps2,snes,megadrive,psx,dreamcast",
        );
        out.genres = cfg.get_string("genres", "");
        out.decades = cfg.get_string("decades", "");
        out.min_rating = cfg
            .get_string("min_rating", "6.0")
            .parse::<f32>()
            .unwrap_or(6.0)
            .clamp(0.0, 9.9);
        out.video_type = cfg.get_string("video_type", "any");
        out
    }

    pub fn effective_lang<'a>(&'a self, system_lang: &'a str) -> &'a str {
        if self.lang.is_empty() || self.lang.eq_ignore_ascii_case("system") {
            if system_lang.is_empty() {
                "fr"
            } else {
                system_lang
            }
        } else {
            &self.lang
        }
    }

    pub fn synopsis_px_per_sec(&self) -> u64 {
        match self.text_scroll_speed.trim().to_ascii_lowercase().as_str() {
            "1" => 14,
            "2" | "slow" => 18,
            "3" => 24,
            "4" | "normal" => 32,
            "5" => 42,
            "6" | "fast" => 56,
            "7" => 75,
            "8" => 100,
            "9" => 130,
            "10" => 160,
            _ => 32,
        }
    }

    pub fn any_panorama_requested(&self) -> bool {
        self.jacket_panorama || self.marquee_panorama || self.snapshot_panorama_count > 0
    }
}

/// Extract `(host, port)` from an HTTP or MQTT URL string.
pub fn parse_host_and_port(raw_url: &str, default_port: u16) -> Option<(String, u16)> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return None;
    }
    let without_scheme = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .or_else(|| trimmed.strip_prefix("mqtt://"))
        .or_else(|| trimmed.strip_prefix("tcp://"))
        .unwrap_or(trimmed);
    let authority = without_scheme.split('/').next().unwrap_or(without_scheme);
    if authority.is_empty() {
        return None;
    }
    if let Some((h, p_str)) = authority.rsplit_once(':') {
        if !h.is_empty() {
            let port = p_str.parse::<u16>().unwrap_or(default_port);
            return Some((h.to_string(), port));
        }
    }
    Some((authority.to_string(), default_port))
}

/// Resolve the target MQTT `(host, port)` according to the engine's 3-mode broker configuration:
/// 1. `use_showcase_broker == true` -> derive host directly from `server_url` and use port `1883`.
/// 2. `use_showcase_broker == false && mqtt_broker_source == "custom"` -> parse `mqtt_custom_url`.
/// 3. `use_showcase_broker == false && mqtt_broker_source == "global"` -> use ArcadeMatrix's global MQTT config.
pub fn resolve_mqtt_broker_endpoint(
    cfg: &ShowcaseConfig,
    global_mqtt: Option<(&str, u16)>,
) -> Option<(String, u16)> {
    if cfg.use_showcase_broker {
        let (host, _) = parse_host_and_port(&cfg.server_url, 8080)?;
        return Some((host, 1883));
    }
    if cfg.mqtt_broker_source.eq_ignore_ascii_case("custom") {
        if let Some(ep) = parse_host_and_port(&cfg.mqtt_custom_url, 1883) {
            return Some(ep);
        }
    }
    if let Some((g_host, g_port)) = global_mqtt {
        if !g_host.trim().is_empty() {
            return Some((
                g_host.trim().to_string(),
                if g_port > 0 { g_port } else { 1883 },
            ));
        }
    }
    let (host, _) = parse_host_and_port(&cfg.server_url, 8080)?;
    Some((host, 1883))
}

/// Probe the DMD's outbound LAN IPv4 address via a zero-packet UDP routing table lookup.
/// This allows `dmd-media-showcase` running inside a bridged Docker container to stream
/// UDP `DMDF` frames to the DMD's real LAN IP instead of the Docker gateway (`172.18.0.1`).
pub fn probe_local_lan_ip(target_host: &str, target_port: u16) -> Option<String> {
    if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
        if sock.connect((target_host, target_port)).is_ok() {
            if let Ok(addr) = sock.local_addr() {
                let ip = addr.ip().to_string();
                if !ip.is_empty() && ip != "0.0.0.0" && !ip.starts_with("127.") {
                    return Some(ip);
                }
            }
        }
    }
    if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
        if sock.connect(("1.1.1.1", 80)).is_ok() {
            if let Ok(addr) = sock.local_addr() {
                let ip = addr.ip().to_string();
                if !ip.is_empty() && ip != "0.0.0.0" && !ip.starts_with("127.") {
                    return Some(ip);
                }
            }
        }
    }
    None
}

/// Pure-Rust RFC 4648 Base64 decoder for inline `poster_rgb565_b64` payloads.
pub fn decode_base64_bytes(b64: &str) -> Option<Vec<u8>> {
    let trimmed = b64.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity((trimmed.len() * 3) / 4);
    let mut buf: u32 = 0;
    let mut bits: u8 = 0;
    for b in trimmed.bytes() {
        if b == b'=' {
            break;
        }
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b' ' | b'\r' | b'\n' | b'\t' => continue,
            _ => return None,
        };
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiMediaAssets {
    #[serde(default)]
    pub poster_png: String,
    #[serde(default)]
    pub poster_rgb565: String,
    #[serde(default)]
    pub unified_stream_rgb565: String,
    #[serde(default)]
    pub jacket_stream_rgb565: String,
    #[serde(default)]
    pub marquee_stream_rgb565: String,
    #[serde(default)]
    pub snapshot_streams_rgb565: Vec<String>,
    #[serde(default)]
    pub panorama_streams_rgb565: Vec<String>,
    #[serde(default)]
    pub video_stream_rgb565: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiShowcaseItem {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub item_id: String,
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub year: String,
    #[serde(default)]
    pub badge: String,
    #[serde(default = "default_badge_hex")]
    pub badge_color: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub rating: String,
    #[serde(default)]
    pub synopsis: String,
    #[serde(default)]
    pub poster_width: u32,
    #[serde(default)]
    pub poster_height: u32,
    #[serde(default)]
    pub poster_rgb565_url: String,
    #[serde(default)]
    pub poster_rgb565_b64: String,
    #[serde(default)]
    pub has_panoramas: bool,
    #[serde(default)]
    pub has_video: bool,
    #[serde(default)]
    pub video_ready: bool,
    #[serde(default)]
    pub stream_frames: u32,
    #[serde(default)]
    pub stream_duration_s: f32,
    #[serde(default)]
    pub total_stream_duration_s: f32,
    #[serde(default)]
    pub stream_fps: u32,
    #[serde(default = "default_fps_val")]
    pub video_fps: u32,
    #[serde(default = "default_fps_val")]
    pub fps: u32,
    #[serde(default)]
    pub assets: ApiMediaAssets,
}

impl ApiShowcaseItem {
    pub fn canonical_id(&self) -> &str {
        if !self.id.is_empty() {
            &self.id
        } else {
            &self.item_id
        }
    }

    pub fn effective_stream_duration_s(&self) -> f32 {
        if self.stream_duration_s > 0.0 {
            self.stream_duration_s
        } else if self.total_stream_duration_s > 0.0 {
            self.total_stream_duration_s
        } else {
            8.0
        }
    }

    pub fn effective_stream_fps(&self) -> u32 {
        if self.stream_fps > 0 {
            self.stream_fps
        } else if self.fps > 0 {
            self.fps
        } else if self.video_fps > 0 {
            self.video_fps
        } else {
            15
        }
    }

    pub fn canonical_poster_path(&self) -> &str {
        if !self.poster_rgb565_url.is_empty() {
            &self.poster_rgb565_url
        } else {
            &self.assets.poster_rgb565
        }
    }

    pub fn resolve_poster_image(
        &self,
        domain_slug: &str,
        base_url: &str,
        http_client: Option<&reqwest::blocking::Client>,
        default_pw: u32,
        default_ph: u32,
    ) -> Option<RgbImage> {
        let pw = if self.poster_width > 0 {
            self.poster_width
        } else {
            default_pw
        };
        let ph = if self.poster_height > 0 {
            self.poster_height
        } else {
            default_ph
        };
        if !self.poster_rgb565_b64.is_empty() {
            if let Some(raw) = decode_base64_bytes(&self.poster_rgb565_b64) {
                if let Some(img) = decode_rgb565_le_image(&raw, pw, ph) {
                    return Some(img);
                }
            }
        }
        let p_path = self.canonical_poster_path();
        let fallback_path = if !p_path.is_empty() {
            p_path.to_string()
        } else if !self.canonical_id().is_empty() {
            format!(
                "/api/v1/{}/poster/{}.rgb565",
                domain_slug,
                self.canonical_id()
            )
        } else {
            String::new()
        };
        if fallback_path.is_empty() {
            return None;
        }
        let url = format!("{}{}", base_url, fallback_path);
        http_client
            .and_then(|c| c.get(&url).send().ok())
            .and_then(|r| r.bytes().ok())
            .and_then(|b| decode_rgb565_le_image(&b, pw, ph))
    }
}

fn default_badge_hex() -> String {
    "#FF6D00".to_string()
}

fn default_fps_val() -> u32 {
    15
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiNextResponse {
    pub status: String,
    #[serde(default)]
    pub item: Option<ApiShowcaseItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttReadyPayload {
    pub ready: bool,
    #[serde(default)]
    pub item_id: String,
    #[serde(default)]
    pub card: Option<ApiShowcaseItem>,
}

/// Single Consume-Once item held only during the current rotation turn.
/// Never cached across turns or replayed when the server goes offline.
#[derive(Clone)]
pub struct PreparedShowcaseItem {
    pub id: String,
    pub title: String,
    pub year: String,
    pub badge: String,
    pub badge_rgb: (u8, u8, u8),
    pub subtitle: String,
    pub rating: String,
    pub synopsis: String,
    pub poster: Option<RgbImage>,
    pub has_stream: bool,
    pub stream_duration_s: f32,
    pub stream_fps: u32,
    /// Used only in `http_poll` mode for the single current item, then dropped immediately.
    pub http_stream_frames: Vec<RgbImage>,
    pub http_frame_delay: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchReadiness {
    Offline,
    Warming,
    Ready,
}

/// Shared state between the network worker thread (`mqtt_udp` or `http_poll`) and the render loop.
/// Holds at most 1 unconsumed ready item and 1 live UDP frame (`Option<RgbImage>`).
pub struct SharedShowcaseState {
    pub server_online: bool,
    pub mqtt_connected_once: bool,
    pub mqtt_connected_at: Option<Instant>,
    pub readiness: BatchReadiness,
    pub ready_item: Option<PreparedShowcaseItem>,
    pub live_udp_frame: Option<RgbImage>,
    pub live_udp_seq: u16,
    pub last_udp_packet_at: Option<Instant>,
    pub stream_eof: bool,
    pub requested_start_card_id: Option<String>,
    pub requested_start_stream_id: Option<String>,
    pub requested_stop_stream_id: Option<String>,
    pub generation: u64,
}

impl Default for SharedShowcaseState {
    fn default() -> Self {
        Self {
            server_online: false,
            mqtt_connected_once: false,
            mqtt_connected_at: None,
            readiness: BatchReadiness::Offline,
            ready_item: None,
            live_udp_frame: None,
            live_udp_seq: 0,
            last_udp_packet_at: None,
            stream_eof: false,
            requested_start_card_id: None,
            requested_start_stream_id: None,
            requested_stop_stream_id: None,
            generation: 0,
        }
    }
}

impl SharedShowcaseState {
    /// Immediately wipe all media state when the server goes offline or a turn ends.
    pub fn clear_media_immediately(&mut self) {
        self.ready_item = None;
        self.live_udp_frame = None;
        self.last_udp_packet_at = None;
        self.stream_eof = true;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackPhase {
    Card,
    LiveStream,
}

/// Convert little-endian RGB565 raw bytes (`width * height * 2` bytes) into an `RgbImage`.
pub fn decode_rgb565_le_image(bytes: &[u8], width: u32, height: u32) -> Option<RgbImage> {
    let expected = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(2)?;
    if bytes.len() < expected || width == 0 || height == 0 {
        return None;
    }
    let mut img = RgbImage::new(width, height);
    let mut offset = 0usize;
    for y in 0..height {
        for x in 0..width {
            let lo = bytes[offset] as u16;
            let hi = bytes[offset + 1] as u16;
            offset += 2;
            let px = lo | (hi << 8);
            let r5 = ((px >> 11) & 0x1F) as u8;
            let g6 = ((px >> 5) & 0x3F) as u8;
            let b5 = (px & 0x1F) as u8;
            let r = (r5 << 3) | (r5 >> 2);
            let g = (g6 << 2) | (g6 >> 4);
            let b = (b5 << 3) | (b5 >> 2);
            img.put_pixel(x, y, image::Rgb([r, g, b]));
        }
    }
    Some(img)
}

/// Parse a binary `.rgb565.stream` (`DMDS`) payload into a sequence of `RgbImage` frames and frame delay.
pub fn decode_rgb565_stream(bytes: &[u8]) -> Option<(Vec<RgbImage>, Duration)> {
    if bytes.len() < DMDS_HEADER_SIZE {
        return None;
    }
    if &bytes[0..4] != DMDS_MAGIC {
        return None;
    }
    let _version = bytes[4];
    let fps = bytes[5].max(1);
    let width = u16::from_le_bytes([bytes[6], bytes[7]]) as u32;
    let height = u16::from_le_bytes([bytes[8], bytes[9]]) as u32;
    let total_frames = u16::from_le_bytes([bytes[10], bytes[11]]) as usize;
    let frame_delay_ms = u16::from_le_bytes([bytes[12], bytes[13]]) as u64;

    if width == 0 || height == 0 || total_frames == 0 {
        return None;
    }
    let frame_bytes = (width as usize) * (height as usize) * 2;
    let mut frames = Vec::with_capacity(total_frames);
    let mut cursor = DMDS_HEADER_SIZE;
    for _ in 0..total_frames {
        if cursor + frame_bytes > bytes.len() {
            break;
        }
        if let Some(frame) =
            decode_rgb565_le_image(&bytes[cursor..cursor + frame_bytes], width, height)
        {
            frames.push(frame);
        }
        cursor += frame_bytes;
    }
    if frames.is_empty() {
        return None;
    }
    let delay_ms = if frame_delay_ms > 0 {
        frame_delay_ms.clamp(25, 500)
    } else {
        (1000 / (fps as u64)).clamp(25, 500)
    };
    Some((frames, Duration::from_millis(delay_ms)))
}

/// Decoded header of a real-time `DMDF` UDP chunk packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmdfPacketHeader {
    pub frame_seq: u16,
    pub chunk_idx: u8,
    pub total_chunks: u8,
    pub byte_offset: u16,
}

/// Parse a `DMDF` UDP datagram into its header and raw RGB565LE pixel slice.
pub fn decode_dmdf_packet(packet: &[u8]) -> Option<(DmdfPacketHeader, &[u8])> {
    if packet.len() <= DMDF_HEADER_SIZE || &packet[0..4] != DMDF_MAGIC {
        return None;
    }
    let frame_seq = u16::from_le_bytes([packet[4], packet[5]]);
    let chunk_idx = packet[6];
    let total_chunks = packet[7].max(1);
    let byte_offset = u16::from_le_bytes([packet[8], packet[9]]);
    if chunk_idx >= total_chunks {
        return None;
    }
    Some((
        DmdfPacketHeader {
            frame_seq,
            chunk_idx,
            total_chunks,
            byte_offset,
        },
        &packet[DMDF_HEADER_SIZE..],
    ))
}

/// Stateful reassembler for `DMDF` UDP chunks into a single RGB565 frame buffer.
pub struct DmdfFrameAssembler {
    width: u32,
    height: u32,
    current_seq: Option<u16>,
    received_mask: u64,
    buf: Vec<u8>,
}

impl DmdfFrameAssembler {
    pub fn new(width: u32, height: u32) -> Self {
        let cap = (width as usize) * (height as usize) * 2;
        Self {
            width,
            height,
            current_seq: None,
            received_mask: 0,
            buf: vec![0u8; cap],
        }
    }

    pub fn push_packet(&mut self, packet: &[u8]) -> Option<(u16, RgbImage)> {
        let (hdr, payload) = decode_dmdf_packet(packet)?;
        if self.current_seq != Some(hdr.frame_seq) {
            self.current_seq = Some(hdr.frame_seq);
            self.received_mask = 0;
        }
        let start = hdr.byte_offset as usize;
        let end = start.saturating_add(payload.len());
        if end > self.buf.len() || hdr.total_chunks > 64 {
            return None;
        }
        self.buf[start..end].copy_from_slice(payload);
        self.received_mask |= 1u64 << hdr.chunk_idx;
        let expected_mask = if hdr.total_chunks == 64 {
            u64::MAX
        } else {
            (1u64 << hdr.total_chunks) - 1
        };
        if self.received_mask == expected_mask {
            self.received_mask = 0;
            let img = decode_rgb565_le_image(&self.buf, self.width, self.height)?;
            return Some((hdr.frame_seq, img));
        }
        None
    }
}

pub fn parse_hex_rgb(hex: &str, fallback: (u8, u8, u8)) -> (u8, u8, u8) {
    let clean = hex.trim().trim_start_matches('#');
    if clean.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&clean[0..2], 16),
            u8::from_str_radix(&clean[2..4], 16),
            u8::from_str_radix(&clean[4..6], 16),
        ) {
            return (r, g, b);
        }
    }
    fallback
}

fn split_csv(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Draw a single-pass organic scrolling synopsis across `[min_x, max_x)` at `speed_px_per_sec`.
fn draw_single_pass_synopsis(
    matrix: &mut dyn IDrawingSurface,
    text: &str,
    min_x: i32,
    max_x: i32,
    y: i32,
    color: (u8, u8, u8),
    elapsed_ms: u128,
    speed_px_per_sec: u64,
) {
    let box_w = max_x - min_x;
    if box_w <= 0 || text.is_empty() {
        return;
    }
    let char_count = text.chars().count() as i32;
    let text_w = (char_count * 6 - 1).max(0);
    let max_y = matrix.height() as i32;

    if text_w <= box_w {
        draw_text_scaled(matrix, text, min_x, y, min_x, max_x, 0, max_y, 1, color);
        return;
    }

    let max_scroll = (text_w - (box_w / 3)).max(0);
    let scroll_ms = (elapsed_ms as u64).saturating_sub(SYNOPSIS_START_HOLD_MS);
    let raw_offset = ((scroll_ms * speed_px_per_sec.max(8)) / 1000) as i32;
    let dx = raw_offset.min(max_scroll);

    draw_text_scaled(
        matrix,
        text,
        min_x - dx,
        y,
        min_x,
        max_x,
        0,
        max_y,
        1,
        color,
    );
}

/// Render one of the 4 creative Pixel-Art "Waiting for Content" scenes on the DMD surface.
pub fn render_waiting_pixel_art(
    domain: ShowcaseDomain,
    matrix: &mut dyn IDrawingSurface,
    w: i32,
    h: i32,
    elapsed_ms: u128,
    server_url: &str,
) {
    let t_ms = elapsed_ms as u64;
    let frame_step = (t_ms / 180) as i32;
    let blink_on = (t_ms / 450) % 2 == 0;

    match domain {
        ShowcaseDomain::Games => {
            // 1. Arcade Attract-Mode: Parallax Starfield + Walking 11x8 Space Invader + Pac-Ghost
            for i in 0..14 {
                let sx = ((i * 23 + (t_ms as i32 / (18 + (i % 3) * 10))) % w.max(1)).abs();
                let sy = (4 + (i * 11) % (h - 8).max(1)).clamp(2, h - 3);
                let c = if i % 2 == 0 {
                    (90, 120, 180)
                } else {
                    (50, 70, 110)
                };
                matrix.set_pixel(sx, sy, c.0, c.1, c.2);
            }

            // Classic Space Invader 11x8 bitmap (2 animation frames)
            const INVADER_F0: [u16; 8] = [
                0b00100000100,
                0b00010001000,
                0b00111111100,
                0b01101110110,
                0b11111111111,
                0b10111111101,
                0b10100000101,
                0b00011011000,
            ];
            const INVADER_F1: [u16; 8] = [
                0b00100000100,
                0b10010001001,
                0b10111111101,
                0b11101110111,
                0b11111111111,
                0b01111111110,
                0b00100000100,
                0b01000000010,
            ];
            let sprite = if frame_step % 2 == 0 {
                &INVADER_F0
            } else {
                &INVADER_F1
            };
            let inv_y = if h >= 64 { (h / 2) - 12 } else { 10 };
            let travel_w = (w - 16).max(8);
            let inv_x = 2 + ((t_ms as i32 / 35) % travel_w);
            for (ry, row_bits) in sprite.iter().enumerate() {
                for rx in 0..11 {
                    if (row_bits >> (10 - rx)) & 1 == 1 {
                        matrix.set_pixel(inv_x + rx, inv_y + ry as i32, 0, 255, 128);
                    }
                }
            }
            // Energy pellets ahead of the invader
            for p in 1..=3 {
                let px = (inv_x + 14 + p * 8) % (w - 2).max(4);
                fill_rect(matrix, px, inv_y + 3, 2, 2, (255, 215, 0));
            }
        }
        ShowcaseDomain::Movies => {
            // 2. 35mm Cinema Projector: Perforated film strips + 2 spinning reels + volumetric light beam
            let hole_offset = ((t_ms / 60) as i32) % 6;
            draw_fast_hline(matrix, 0, 0, w, (70, 70, 90));
            draw_fast_hline(matrix, 0, 3, w, (70, 70, 90));
            draw_fast_hline(matrix, 0, h - 4, w, (70, 70, 90));
            draw_fast_hline(matrix, 0, h - 1, w, (70, 70, 90));
            let mut hx = -hole_offset;
            while hx < w {
                fill_rect(matrix, hx, 1, 3, 2, (220, 190, 110));
                fill_rect(matrix, hx, h - 3, 3, 2, (220, 190, 110));
                hx += 6;
            }

            let cy = h / 2;
            let r_rad = if h >= 64 { 6 } else { 4 };
            let r1_x = 8;
            let r2_x = 8 + r_rad * 2 + 2;
            draw_circle(matrix, r1_x, cy - 2, r_rad, (0, 229, 255));
            draw_circle(matrix, r2_x, cy - 2, r_rad, (0, 229, 255));
            // Rotating spokes
            let spoke = frame_step % 4;
            let (dx, dy) = match spoke {
                0 => (r_rad - 1, 0),
                1 => (r_rad - 2, r_rad - 2),
                2 => (0, r_rad - 1),
                _ => (-(r_rad - 2), r_rad - 2),
            };
            draw_line(
                matrix,
                r1_x - dx,
                cy - 2 - dy,
                r1_x + dx,
                cy - 2 + dy,
                (255, 255, 255),
            );
            draw_line(
                matrix,
                r2_x - dx,
                cy - 2 - dy,
                r2_x + dx,
                cy - 2 + dy,
                (255, 255, 255),
            );

            // Projector body & flickering golden light cone
            let lens_x = r2_x + r_rad + 2;
            fill_rect(matrix, r1_x - 2, cy + 1, r_rad * 2 + 6, 4, (120, 140, 165));
            let flicker = if (t_ms / 90) % 3 == 0 { 210 } else { 255 };
            draw_line(matrix, lens_x, cy + 2, w - 4, 5, (flicker, 200, 70));
            draw_line(matrix, lens_x, cy + 2, w - 4, h - 6, (flicker, 200, 70));
        }
        ShowcaseDomain::Series => {
            // 3. Retro CRT Television + Rabbit-Ear Antennae + Expanding RF Waves + Scanline
            let tv_w = if w >= 128 { 22 } else { 16 };
            let tv_h = if h >= 64 { 16 } else { 12 };
            let tv_x = 6;
            let tv_y = (h / 2) - (tv_h / 2) + 2;
            draw_round_rect(matrix, tv_x, tv_y, tv_w, tv_h, 2, (229, 57, 53));

            // Rabbit-ear V-antennae
            let ant_cx = tv_x + tv_w / 2;
            draw_line(matrix, ant_cx, tv_y, ant_cx - 5, tv_y - 5, (200, 210, 230));
            draw_line(matrix, ant_cx, tv_y, ant_cx + 5, tv_y - 5, (200, 210, 230));

            // Concentric RF signal waves
            let wave_r = 2 + ((t_ms / 140) as i32 % 6);
            draw_circle(matrix, ant_cx, tv_y - 5, wave_r, (0, 229, 255));

            // SMPTE mini color bars inside CRT + moving scanline
            let bar_colors = [
                (235, 235, 235),
                (240, 220, 0),
                (0, 220, 220),
                (0, 210, 80),
                (220, 0, 220),
                (220, 30, 30),
            ];
            let inner_w = (tv_w - 4).max(6);
            let inner_h = (tv_h - 4).max(4);
            for (idx, col) in bar_colors.iter().enumerate() {
                let bx0 = tv_x + 2 + (idx as i32 * inner_w) / 6;
                let bx1 = tv_x + 2 + ((idx as i32 + 1) * inner_w) / 6;
                fill_rect(matrix, bx0, tv_y + 2, (bx1 - bx0).max(1), inner_h, *col);
            }
            let scan_y = tv_y + 2 + ((t_ms / 70) as i32 % inner_h.max(1));
            draw_fast_hline(matrix, tv_x + 2, scan_y, inner_w, (255, 255, 255));
        }
        ShowcaseDomain::Anime => {
            // 4. Anime Shuriken / Mecha Energy Core + Speed-Lines + Floating Sakura Petals
            for i in 0..8 {
                let ly = 5 + (i * (h - 10).max(8)) / 8;
                let len = 8 + (i * 5) % 14;
                let lx = ((w - ((t_ms as i32 / 12 + i * 19) % (w + len))) - len).max(0);
                draw_fast_hline(matrix, lx, ly, len.min(w - lx), (70, 30, 90));
            }

            // Rotating 4-point Shuriken on the left
            let sx = 14;
            let sy = h / 2;
            let arm = if h >= 64 { 7 } else { 5 };
            if frame_step % 2 == 0 {
                draw_fast_hline(matrix, sx - arm, sy, arm * 2 + 1, (0, 229, 255));
                draw_fast_vline(matrix, sx, sy - arm, arm * 2 + 1, (0, 229, 255));
                draw_line(matrix, sx - 3, sy - 3, sx + 3, sy + 3, (255, 64, 129));
                draw_line(matrix, sx - 3, sy + 3, sx + 3, sy - 3, (255, 64, 129));
            } else {
                draw_line(
                    matrix,
                    sx - arm,
                    sy - arm,
                    sx + arm,
                    sy + arm,
                    (0, 229, 255),
                );
                draw_line(
                    matrix,
                    sx - arm,
                    sy + arm,
                    sx + arm,
                    sy - arm,
                    (0, 229, 255),
                );
                draw_fast_hline(matrix, sx - 3, sy, 7, (255, 64, 129));
                draw_fast_vline(matrix, sx, sy - 3, 7, (255, 64, 129));
            }
            fill_circle(matrix, sx, sy, 1, (255, 255, 255));

            // Floating sakura petals
            for p in 0..6 {
                let px = (28 + p * 17 + (t_ms as i32 / 40)) % w.max(1);
                let py = (4 + p * 9 + (t_ms as i32 / 80)) % (h - 6).max(1);
                fill_rect(matrix, px, py, 2, 2, (255, 130, 190));
            }
        }
    }

    // Header badge + blinking status prompt
    let accent = domain.default_accent_rgb();
    let badge = domain.badge_label();
    let badge_w = (badge.len() as i32) * 4 + 4;
    let bx = ((w - badge_w) / 2).max(1);
    fill_round_rect(matrix, bx, 2, badge_w, 7, 1, accent);
    draw_tiny_text(matrix, badge, bx + 2, 3, 0, w, (255, 255, 255));

    let msg = if server_url.is_empty() {
        "SET SERVER URL"
    } else {
        match domain {
            ShowcaseDomain::Games => "WAITING FOR GAME...",
            ShowcaseDomain::Movies => "WAITING FOR MOVIE...",
            ShowcaseDomain::Series => "TUNING SERIES...",
            ShowcaseDomain::Anime => "WAITING FOR ANIME...",
        }
    };
    let text_min_x = if w >= 128 { 30 } else { 2 };
    let text_y = if h >= 64 { (h / 2) - 3 } else { (h / 2) + 2 };
    if blink_on {
        draw_tiny_text(
            matrix,
            msg,
            text_min_x,
            text_y,
            text_min_x,
            w - 2,
            (235, 245, 255),
        );
    }
}

pub struct MediaShowcaseRunner {
    pub domain: ShowcaseDomain,
    pub cfg: ShowcaseConfig,
    pub system_lang: String,
    pub global_mqtt_host: String,
    pub global_mqtt_port: u16,
    pub width: u32,
    pub height: u32,
    pub shared: Arc<Mutex<SharedShowcaseState>>,
    pub worker_busy: Arc<AtomicBool>,
    pub config_gen: Arc<AtomicU64>,

    // Single-item Consume-Once playback state (never cached across turns)
    pub active_item: Option<PreparedShowcaseItem>,
    pub phase: PlaybackPhase,
    pub phase_started_at: Instant,
    pub last_retry_poll: Instant,
    pub http_frame_idx: usize,
    pub http_frame_elapsed: Duration,
    pub last_tick: Instant,
    pub turn_evaluated: bool,
    pub skip_turn_immediately: bool,
    pub turn_completed: bool,
    pub dirty: bool,
}

impl MediaShowcaseRunner {
    pub fn new(domain: ShowcaseDomain, width: u32, height: u32) -> Self {
        let now = Instant::now();
        Self {
            domain,
            cfg: ShowcaseConfig::default(),
            system_lang: "fr".to_string(),
            global_mqtt_host: String::new(),
            global_mqtt_port: 1883,
            width: width.max(32),
            height: height.max(32),
            shared: Arc::new(Mutex::new(SharedShowcaseState::default())),
            worker_busy: Arc::new(AtomicBool::new(false)),
            config_gen: Arc::new(AtomicU64::new(1)),
            active_item: None,
            phase: PlaybackPhase::Card,
            phase_started_at: now,
            last_retry_poll: now,
            http_frame_idx: 0,
            http_frame_elapsed: Duration::ZERO,
            last_tick: now,
            turn_evaluated: false,
            skip_turn_immediately: false,
            turn_completed: false,
            dirty: true,
        }
    }

    /// Compute the exact organic duration of `PlaybackPhase::Card` so 100% of `item.synopsis`
    /// scrolls across the DMD viewport at `cfg.synopsis_px_per_sec()` before advancing.
    pub fn organic_card_duration(&self, item: &PreparedShowcaseItem) -> Duration {
        let base_dur = Duration::from_millis(MIN_CARD_DISPLAY_MS);
        if !self.cfg.show_synopsis || item.synopsis.is_empty() {
            return base_dur;
        }
        let w = self.width as i32;
        let h = self.height as i32;
        let (pw, _) = compute_card_poster_dims(self.width, self.height);
        let box_w = if h > w || w == h {
            (w - 4).max(24)
        } else if self.cfg.show_poster && item.poster.is_some() {
            (w - (pw as i32) - 4).max(24)
        } else {
            (w - 4).max(24)
        };

        let char_count = item.synopsis.chars().count() as i32;
        let text_w = (char_count * 6 - 1).max(0);
        if text_w <= box_w {
            return base_dur;
        }
        let scroll_px = (text_w - (box_w / 3)).max(1) as u64;
        let scroll_ms = (scroll_px * 1000) / self.cfg.synopsis_px_per_sec().max(8);
        let total_ms = SYNOPSIS_START_HOLD_MS + scroll_ms + SYNOPSIS_END_HOLD_MS;
        Duration::from_millis(total_ms).max(base_dur)
    }

    pub fn dmd_client_id(&self) -> String {
        format!(
            "rpi_{}_{}x{}",
            self.domain.as_api_slug(),
            self.width,
            self.height
        )
    }

    /// Spawn the background worker thread (`mqtt_udp` real-time client or `http_poll` fallback).
    pub fn spawn_background_sync(&self) {
        if self.cfg.server_url.is_empty() {
            return;
        }
        if self
            .worker_busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        let domain = self.domain;
        let cfg = self.cfg.clone();
        let lang = cfg.effective_lang(&self.system_lang).to_string();
        let width = self.width;
        let height = self.height;
        let shared = Arc::clone(&self.shared);
        let busy = Arc::clone(&self.worker_busy);
        let config_gen_arc = Arc::clone(&self.config_gen);
        let my_gen = config_gen_arc.load(Ordering::SeqCst);
        let global_host = self.global_mqtt_host.clone();
        let global_port = self.global_mqtt_port;

        std::thread::spawn(move || {
            let _guard = BusyResetGuard(busy);
            if cfg.transport_mode.eq_ignore_ascii_case("mqtt_udp") {
                Self::run_mqtt_udp_worker(
                    domain,
                    cfg,
                    lang,
                    width,
                    height,
                    global_host,
                    global_port,
                    shared,
                    config_gen_arc,
                    my_gen,
                );
            } else {
                Self::run_http_poll_worker(domain, cfg, lang, width, height, shared, my_gen);
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn run_mqtt_udp_worker(
        domain: ShowcaseDomain,
        cfg: ShowcaseConfig,
        lang: String,
        width: u32,
        height: u32,
        global_host: String,
        global_port: u16,
        shared: Arc<Mutex<SharedShowcaseState>>,
        config_gen_arc: Arc<AtomicU64>,
        my_gen: u64,
    ) {
        let Some((mqtt_host, mqtt_port)) = resolve_mqtt_broker_endpoint(
            &cfg,
            if global_host.is_empty() {
                None
            } else {
                Some((global_host.as_str(), global_port))
            },
        ) else {
            return;
        };

        let slug = domain.as_api_slug();
        let dmd_id = format!("rpi_{}_{}x{}", slug, width, height);
        let local_ip = probe_local_lan_ip(&mqtt_host, mqtt_port).unwrap_or_default();
        let udp_socket = match UdpSocket::bind("0.0.0.0:0") {
            Ok(s) => {
                let _ = s.set_read_timeout(Some(Duration::from_millis(4)));
                s
            }
            Err(_) => return,
        };
        let local_udp_port = udp_socket.local_addr().map(|a| a.port()).unwrap_or(5568);

        let mut mqtt_opts = MqttOptions::new(
            format!("{}_{}", dmd_id, my_gen % 1000),
            mqtt_host,
            mqtt_port,
        );
        mqtt_opts.set_keep_alive(Duration::from_secs(15));
        mqtt_opts.set_last_will(LastWill::new(
            format!("dmd_showcase/clients/{}/status", dmd_id),
            r#"{"online":false}"#,
            QoS::AtLeastOnce,
            false,
        ));

        let (mqtt_client, mut connection) = MqttSyncClient::new(mqtt_opts, 64);

        let status_topic = "dmd_showcase/server/status".to_string();
        let ready_topic = format!("dmd_showcase/clients/{}/{}/ready", dmd_id, slug);
        let events_topic = format!("dmd_showcase/clients/{}/{}/events", dmd_id, slug);
        let stream_state_topic = format!("dmd_showcase/clients/{}/stream_state", dmd_id);
        let config_topic = format!("dmd_showcase/clients/{}/config", dmd_id);

        let mut domain_cfg = serde_json::Map::new();
        domain_cfg.insert(
            slug.to_string(),
            serde_json::json!({
                "lang": lang,
                "show_poster": cfg.show_poster,
                "show_synopsis": cfg.show_synopsis,
                "text_scroll_speed": cfg.text_scroll_speed,
                "jacket_panorama": cfg.jacket_panorama,
                "marquee_panorama": cfg.marquee_panorama,
                "snapshot_panorama_count": cfg.snapshot_panorama_count,
                "show_video": cfg.show_video,
                "video_max_duration_s": cfg.video_max_duration_s,
                "video_fps": cfg.video_fps,
                "video_mode": cfg.video_mode,
                "color_boost": cfg.color_boost,
                "systems": split_csv(&cfg.systems),
                "genres": split_csv(&cfg.genres),
                "decades": split_csv(&cfg.decades),
                "min_rating": cfg.min_rating,
                "video_type": cfg.video_type,
            }),
        );
        let client_cfg_payload = serde_json::json!({
            "dmd_id": dmd_id,
            "client_ip": local_ip,
            "udp_ip": local_ip,
            "width": width,
            "height": height,
            "udp_port": local_udp_port,
            "lang": lang,
            "domains": domain_cfg,
        });
        // Publish retained client config FIRST so the Showcase orchestrator can clean up
        // any mid-broadcast pre-reboot slot and re-publish `ready: true` immediately.
        if let Ok(bytes) = serde_json::to_vec(&client_cfg_payload) {
            let _ = mqtt_client.publish(&config_topic, QoS::AtLeastOnce, true, bytes);
        }

        let _ = mqtt_client.subscribe(&status_topic, QoS::AtLeastOnce);
        let _ = mqtt_client.subscribe(&ready_topic, QoS::AtLeastOnce);
        let _ = mqtt_client.subscribe(&events_topic, QoS::AtLeastOnce);
        let _ = mqtt_client.subscribe(&stream_state_topic, QoS::AtLeastOnce);

        let http_client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_millis(1500))
            .timeout(Duration::from_millis(6000))
            .build()
            .ok();
        let base_url = cfg.server_url.trim_end_matches('/').to_string();
        let (default_pw, default_ph) = compute_card_poster_dims(width, height);

        // Spawn a dedicated fast 4ms I/O pump thread for UDP `DMDF` packet reception and
        // immediate (< 4ms) outbound `start_card` / `start_stream` MQTT event dispatching.
        // This guarantees `connection.iter().next()` never blocks `start_stream` or UDP frames.
        let worker_alive = Arc::new(AtomicBool::new(true));
        {
            let pump_alive = Arc::clone(&worker_alive);
            let pump_gen_arc = Arc::clone(&config_gen_arc);
            let pump_shared = Arc::clone(&shared);
            let pump_mqtt = mqtt_client.clone();
            let pump_events_topic = events_topic.clone();
            let pump_local_ip = local_ip.clone();
            std::thread::spawn(move || {
                let mut assembler = DmdfFrameAssembler::new(width, height);
                let mut udp_buf = [0u8; 1500];
                while pump_alive.load(Ordering::SeqCst) {
                    if pump_gen_arc.load(Ordering::SeqCst) != my_gen {
                        let _ = pump_mqtt.disconnect();
                        break;
                    }
                    // 1. Drain any incoming UDP `DMDF` packets (up to 4ms read timeout on first call)
                    let mut got_pkt = false;
                    while let Ok((len, _)) = udp_socket.recv_from(&mut udp_buf) {
                        got_pkt = true;
                        if let Some((seq, img)) = assembler.push_packet(&udp_buf[..len]) {
                            let mut guard = pump_shared.lock().unwrap();
                            guard.live_udp_frame = Some(img);
                            guard.live_udp_seq = seq;
                            guard.last_udp_packet_at = Some(Instant::now());
                            guard.server_online = true;
                        }
                        let _ = udp_socket.set_nonblocking(true);
                    }
                    if got_pkt {
                        let _ = udp_socket.set_read_timeout(Some(Duration::from_millis(4)));
                    }

                    // 2. Immediately dispatch any queued `start_card`, `start_stream`, or `stop_stream` MQTT events
                    let (start_card_id, start_stream_id, stop_stream_id) = {
                        let mut guard = pump_shared.lock().unwrap();
                        (
                            guard.requested_start_card_id.take(),
                            guard.requested_start_stream_id.take(),
                            guard.requested_stop_stream_id.take(),
                        )
                    };
                    if let Some(item_id) = start_card_id {
                        info!(
                            "[Showcase:{}] --> MQTT 'start_card' item_id='{}' (client_ip={}, udp_port={})",
                            slug, item_id, pump_local_ip, local_udp_port
                        );
                        let ev = serde_json::json!({
                            "action": "start_card",
                            "item_id": item_id,
                            "client_ip": pump_local_ip,
                            "udp_ip": pump_local_ip,
                            "udp_port": local_udp_port,
                        });
                        if let Ok(b) = serde_json::to_vec(&ev) {
                            let _ =
                                pump_mqtt.publish(&pump_events_topic, QoS::AtLeastOnce, false, b);
                        }
                    }
                    if let Some(item_id) = start_stream_id {
                        info!(
                            "[Showcase:{}] --> MQTT 'start_stream' item_id='{}' (UDP {}:{})",
                            slug, item_id, pump_local_ip, local_udp_port
                        );
                        let ev = serde_json::json!({
                            "action": "start_stream",
                            "item_id": item_id,
                            "client_ip": pump_local_ip,
                            "udp_ip": pump_local_ip,
                            "udp_port": local_udp_port,
                        });
                        if let Ok(b) = serde_json::to_vec(&ev) {
                            let _ =
                                pump_mqtt.publish(&pump_events_topic, QoS::AtLeastOnce, false, b);
                        }
                    }
                    if let Some(item_id) = stop_stream_id {
                        let ev = serde_json::json!({
                            "action": "stop_stream",
                            "item_id": item_id,
                            "client_ip": pump_local_ip,
                            "udp_ip": pump_local_ip,
                            "udp_port": local_udp_port,
                        });
                        if let Ok(b) = serde_json::to_vec(&ev) {
                            let _ =
                                pump_mqtt.publish(&pump_events_topic, QoS::AtLeastOnce, false, b);
                        }
                    }
                }
            });
        }

        let mut iter = connection.iter();
        while config_gen_arc.load(Ordering::SeqCst) == my_gen {
            match iter.next() {
                Some(Ok(MqttEvent::Incoming(Packet::Publish(pub_pkt)))) => {
                    let topic = pub_pkt.topic.as_str();
                    let payload = &pub_pkt.payload;
                    if topic == status_topic {
                        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(payload) {
                            let online = v.get("online").and_then(|b| b.as_bool()).unwrap_or(false);
                            let mut guard = shared.lock().unwrap();
                            guard.server_online = online;
                            guard.mqtt_connected_once = true;
                            guard.mqtt_connected_at.get_or_insert_with(Instant::now);
                            if !online {
                                guard.readiness = BatchReadiness::Offline;
                                guard.clear_media_immediately();
                            }
                        }
                    } else if topic == ready_topic {
                        match serde_json::from_slice::<MqttReadyPayload>(payload) {
                            Ok(ready_msg) => {
                                if ready_msg.ready {
                                    if let Some(card) = ready_msg.card {
                                        let poster = if cfg.show_poster {
                                            card.resolve_poster_image(
                                                slug,
                                                &base_url,
                                                http_client.as_ref(),
                                                default_pw,
                                                default_ph,
                                            )
                                        } else {
                                            None
                                        };

                                        let item_id = if !ready_msg.item_id.is_empty() {
                                            ready_msg.item_id.clone()
                                        } else {
                                            card.canonical_id().to_string()
                                        };
                                        let has_stream = (cfg.any_panorama_requested()
                                            && card.has_panoramas)
                                            || (cfg.show_video && card.has_video);
                                        let stream_dur =
                                            card.effective_stream_duration_s().max(2.0);
                                        let stream_fps = card.effective_stream_fps().max(1);
                                        info!(
                                            "[Showcase:{}] <-- MQTT 'ready=true' card='{}' (id='{}', has_stream={}, dur={:.1}s, fps={})",
                                            slug, card.title, item_id, has_stream, stream_dur, stream_fps
                                        );
                                        let prepared = PreparedShowcaseItem {
                                            id: item_id,
                                            title: card.title,
                                            year: card.year,
                                            badge: card.badge,
                                            badge_rgb: parse_hex_rgb(
                                                &card.badge_color,
                                                domain.default_accent_rgb(),
                                            ),
                                            subtitle: card.subtitle,
                                            rating: card.rating,
                                            synopsis: card.synopsis,
                                            poster,
                                            has_stream,
                                            stream_duration_s: stream_dur,
                                            stream_fps,
                                            http_stream_frames: Vec::new(),
                                            http_frame_delay: Duration::from_millis(66),
                                        };
                                        let mut guard = shared.lock().unwrap();
                                        guard.server_online = true;
                                        guard.mqtt_connected_once = true;
                                        guard.mqtt_connected_at.get_or_insert_with(Instant::now);
                                        guard.readiness = BatchReadiness::Ready;
                                        guard.ready_item = Some(prepared);
                                        guard.generation = my_gen;
                                    }
                                } else {
                                    let mut guard = shared.lock().unwrap();
                                    guard.server_online = true;
                                    guard.mqtt_connected_once = true;
                                    guard.mqtt_connected_at.get_or_insert_with(Instant::now);
                                    if guard.ready_item.is_none() {
                                        guard.readiness = BatchReadiness::Warming;
                                    }
                                }
                            }
                            Err(err) => {
                                warn!(
                                    "[Showcase:{}] Failed to deserialize MQTT ready payload ({} bytes): {}",
                                    slug,
                                    payload.len(),
                                    err
                                );
                            }
                        }
                    } else if topic == events_topic || topic == stream_state_topic {
                        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(payload) {
                            let is_eof = v.get("event").and_then(|s| s.as_str())
                                == Some("stream_eof")
                                || v.get("state").and_then(|s| s.as_str()) == Some("stream_eof");
                            if is_eof {
                                let mut guard = shared.lock().unwrap();
                                guard.stream_eof = true;
                            }
                        }
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => {
                    worker_alive.store(false, Ordering::SeqCst);
                    if config_gen_arc.load(Ordering::SeqCst) != my_gen {
                        break;
                    }
                    // Broker unreachable or disconnected -> fallback to a single HTTP poll attempt
                    // before exiting so `mqtt_udp` degrades gracefully if only HTTP is reachable.
                    {
                        let mut guard = shared.lock().unwrap();
                        guard.server_online = false;
                        guard.readiness = BatchReadiness::Offline;
                        guard.clear_media_immediately();
                    }
                    Self::run_http_poll_worker(domain, cfg, lang, width, height, shared, my_gen);
                    break;
                }
            }
        }
        worker_alive.store(false, Ordering::SeqCst);
    }

    fn run_http_poll_worker(
        domain: ShowcaseDomain,
        cfg: ShowcaseConfig,
        lang: String,
        width: u32,
        height: u32,
        shared: Arc<Mutex<SharedShowcaseState>>,
        my_gen: u64,
    ) {
        let client = match reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_millis(1500))
            .timeout(Duration::from_millis(8000))
            .build()
        {
            Ok(c) => c,
            Err(_) => return,
        };

        let base = cfg.server_url.trim_end_matches('/');
        let slug = domain.as_api_slug();
        let dmd_id = format!("rpi_{}_{}x{}", slug, width, height);
        let (default_pw, default_ph) = compute_card_poster_dims(width, height);

        // 1. Register client config (`POST /api/v1/{domain}/config`)
        let cfg_url = format!("{}/api/v1/{}/config", base, slug);
        let cfg_body = serde_json::json!({
            "client_id": dmd_id,
            "lang": lang,
            "width": width,
            "height": height,
            "poster_width": default_pw,
            "poster_height": default_ph,
            "include_poster": cfg.show_poster,
            "include_synopsis": cfg.show_synopsis,
            "jacket_panorama": cfg.jacket_panorama,
            "marquee_panorama": cfg.marquee_panorama,
            "snapshot_panorama_count": cfg.snapshot_panorama_count,
            "show_video": cfg.show_video,
            "video_max_duration_s": cfg.video_max_duration_s,
            "video_fps": cfg.video_fps,
            "video_mode": cfg.video_mode,
            "color_boost": cfg.color_boost,
            "systems": split_csv(&cfg.systems),
            "genres": split_csv(&cfg.genres),
            "decades": split_csv(&cfg.decades),
            "min_rating": cfg.min_rating,
            "video_type": cfg.video_type,
        });

        let Ok(cfg_resp) = client.post(&cfg_url).json(&cfg_body).send() else {
            let mut guard = shared.lock().unwrap();
            guard.server_online = false;
            guard.readiness = BatchReadiness::Offline;
            guard.clear_media_immediately();
            return;
        };
        if !cfg_resp.status().is_success() {
            let mut guard = shared.lock().unwrap();
            guard.server_online = false;
            guard.readiness = BatchReadiness::Offline;
            guard.clear_media_immediately();
            return;
        }

        // 2. Poll next unconsumed item (`GET /api/v1/{domain}/next?client_id=...`)
        let next_url = format!("{}/api/v1/{}/next?client_id={}", base, slug, dmd_id);
        let Ok(next_resp) = client.get(&next_url).send() else {
            let mut guard = shared.lock().unwrap();
            guard.server_online = false;
            guard.readiness = BatchReadiness::Offline;
            guard.clear_media_immediately();
            return;
        };
        if !next_resp.status().is_success() {
            return;
        }
        let Ok(next_data) = next_resp.json::<ApiNextResponse>() else {
            return;
        };

        if !next_data.status.eq_ignore_ascii_case("ready") || next_data.item.is_none() {
            let mut guard = shared.lock().unwrap();
            guard.server_online = true;
            guard.readiness = BatchReadiness::Warming;
            return;
        }

        let api_item = next_data.item.unwrap();
        let item_id = api_item.canonical_id().to_string();

        let poster = if cfg.show_poster {
            api_item.resolve_poster_image(slug, base, Some(&client), default_pw, default_ph)
        } else {
            None
        };

        // Fetch unified `.rgb565.stream` (`[Jacket Panorama -> Marquee -> Snapshots -> Video]`)
        let mut http_stream_frames = Vec::new();
        let mut http_frame_delay = Duration::from_millis((1000 / api_item.video_fps.max(1)) as u64);
        let stream_path = if !api_item.assets.unified_stream_rgb565.is_empty() {
            api_item.assets.unified_stream_rgb565.clone()
        } else {
            format!("/api/v1/{}/stream/{}.rgb565.stream", slug, item_id)
        };
        if (cfg.any_panorama_requested() || cfg.show_video) && !stream_path.is_empty() {
            let url = format!("{}{}", base, stream_path);
            if let Ok(r) = client.get(&url).send() {
                if r.status().is_success() {
                    if let Ok(bytes) = r.bytes() {
                        if let Some((frames, delay)) = decode_rgb565_stream(&bytes) {
                            http_stream_frames = frames;
                            http_frame_delay = delay;
                        }
                    }
                }
            }
        }

        let has_stream = !http_stream_frames.is_empty();
        let stream_dur = api_item.effective_stream_duration_s().max(2.0);
        let stream_fps = api_item.effective_stream_fps().max(1);
        let prepared = PreparedShowcaseItem {
            id: item_id,
            title: api_item.title,
            year: api_item.year,
            badge: api_item.badge,
            badge_rgb: parse_hex_rgb(&api_item.badge_color, domain.default_accent_rgb()),
            subtitle: api_item.subtitle,
            rating: api_item.rating,
            synopsis: api_item.synopsis,
            poster,
            has_stream,
            stream_duration_s: stream_dur,
            stream_fps,
            http_stream_frames,
            http_frame_delay,
        };

        let mut guard = shared.lock().unwrap();
        guard.server_online = true;
        guard.mqtt_connected_once = true;
        guard.readiness = BatchReadiness::Ready;
        guard.ready_item = Some(prepared);
        guard.generation = my_gen;
    }

    pub fn initialize(&mut self, context: &mut EngineContext, config: &dyn EngineConfig) {
        self.width = context.surface.width().max(32);
        self.height = context.surface.height().max(32);
        {
            let settings = context.config.settings.read();
            self.system_lang = settings.system.lang.clone();
            self.global_mqtt_host = settings.mqtt.broker.clone();
            self.global_mqtt_port = settings.mqtt.port;
        }
        self.cfg = ShowcaseConfig::from_engine_config(config, self.domain);
        self.active_item = None;
        self.config_gen.fetch_add(1, Ordering::SeqCst);
        self.spawn_background_sync();
    }

    pub fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        let new_cfg = ShowcaseConfig::from_engine_config(config, self.domain);
        if new_cfg != self.cfg {
            self.cfg = new_cfg;
            self.active_item = None;
            {
                let mut guard = self.shared.lock().unwrap();
                guard.readiness = BatchReadiness::Offline;
                guard.clear_media_immediately();
            }
            self.config_gen.fetch_add(1, Ordering::SeqCst);
            self.spawn_background_sync();
        }
    }

    pub fn activate(&mut self) {
        let now = Instant::now();
        self.active_item = None;
        self.http_frame_idx = 0;
        self.http_frame_elapsed = Duration::ZERO;
        self.phase = PlaybackPhase::Card;
        self.phase_started_at = now;
        self.last_retry_poll = now;
        self.last_tick = now;
        self.turn_evaluated = false;
        self.skip_turn_immediately = false;
        self.turn_completed = false;
        self.dirty = true;
    }

    pub fn deactivate(&mut self) {
        // Strict Consume-Once & Zero-Cache: drop any active item or frame when leaving turn
        let stopped_id = self.active_item.take().map(|it| it.id);
        if let Ok(mut guard) = self.shared.lock() {
            guard.live_udp_frame = None;
            if let Some(id) = stopped_id {
                guard.requested_stop_stream_id = Some(id);
            }
        }
        self.skip_turn_immediately = false;
        self.turn_evaluated = false;
        self.turn_completed = false;
    }

    /// Evaluate turn start readiness: consume the prepared item (via `.take()`) so it can never be
    /// replayed on subsequent turns without the server preparing a brand-new item.
    pub fn evaluate_turn_start(&mut self, rotation_slot_count: usize) {
        self.turn_evaluated = true;
        let taken_item = {
            let mut guard = self.shared.lock().unwrap();
            let item = guard.ready_item.take();
            if let Some(ref it) = item {
                guard.readiness = BatchReadiness::Warming;
                guard.requested_start_card_id = Some(it.id.clone());
                guard.stream_eof = false;
                guard.live_udp_frame = None;
                guard.last_udp_packet_at = None;
            }
            item
        };

        if let Some(item) = taken_item {
            self.active_item = Some(item);
            self.skip_turn_immediately = false;
            self.turn_completed = false;
            self.phase = PlaybackPhase::Card;
            self.phase_started_at = Instant::now();
            self.spawn_background_sync();
        } else if rotation_slot_count > 1 {
            warn!(
                "[Showcase:{}] Server offline or warming with {} rotation slots -> skipping turn cleanly",
                self.domain.as_api_slug(),
                rotation_slot_count
            );
            self.active_item = None;
            self.skip_turn_immediately = true;
            self.turn_completed = true;
            self.spawn_background_sync();
        } else {
            self.active_item = None;
            self.skip_turn_immediately = false;
            self.turn_completed = false;
            self.phase = PlaybackPhase::Card;
            self.phase_started_at = Instant::now();
            self.spawn_background_sync();
        }
    }

    fn start_live_stream_phase(&mut self, now: Instant, item_id: &str) {
        self.phase = PlaybackPhase::LiveStream;
        self.phase_started_at = now;
        self.http_frame_idx = 0;
        self.http_frame_elapsed = Duration::ZERO;
        {
            let mut guard = self.shared.lock().unwrap();
            guard.stream_eof = false;
            guard.live_udp_frame = None;
            guard.last_udp_packet_at = None;
            guard.requested_start_stream_id = Some(item_id.to_string());
        }
        self.dirty = true;
    }

    fn finish_current_turn(&mut self) {
        self.active_item = None;
        {
            let mut guard = self.shared.lock().unwrap();
            guard.live_udp_frame = None;
            guard.last_udp_packet_at = None;
        }
        self.turn_completed = true;
        self.dirty = true;
        self.spawn_background_sync();
    }

    pub fn update(&mut self, context: &mut EngineContext) {
        let now = Instant::now();
        let dt = now.saturating_duration_since(self.last_tick);
        self.last_tick = now;

        let new_w = context.surface.width().max(32);
        let new_h = context.surface.height().max(32);
        if new_w != self.width || new_h != self.height {
            self.width = new_w;
            self.height = new_h;
            self.config_gen.fetch_add(1, Ordering::SeqCst);
            self.spawn_background_sync();
        }

        if !self.turn_evaluated {
            let (has_ready, connected_at) = {
                let guard = self.shared.lock().unwrap();
                (guard.ready_item.is_some(), guard.mqtt_connected_at)
            };
            // On initial boot or while the MQTT worker is connecting to receive the retained `ready`
            // message (plus a 450ms settle window after first MQTT packet in case a stale `ready:false`
            // arrived right before our `config` message triggered a `ready:true` re-publish), wait
            // before skipping the turn so a freshly rebooted DMD never misses its initial card.
            let in_boot_connect_grace = match connected_at {
                None => {
                    self.worker_busy.load(Ordering::SeqCst)
                        && now.saturating_duration_since(self.phase_started_at)
                            < Duration::from_millis(1800)
                }
                Some(t) => now.saturating_duration_since(t) < Duration::from_millis(450),
            };
            if !has_ready && !self.cfg.server_url.is_empty() && in_boot_connect_grace {
                self.spawn_background_sync();
                self.dirty = true;
                return;
            }
            let rot_len = context.config.settings.read().rotation.len();
            self.evaluate_turn_start(rot_len);
            self.dirty = true;
            return;
        }

        if self.skip_turn_immediately || self.turn_completed {
            return;
        }

        // Check if server went offline mid-turn: if so, immediately clear media
        {
            let mut guard = self.shared.lock().unwrap();
            if self.active_item.is_some()
                && !guard.server_online
                && guard.readiness == BatchReadiness::Offline
            {
                self.active_item = None;
                guard.clear_media_immediately();
                let rot_len = context.config.settings.read().rotation.len();
                if rot_len > 1 {
                    self.skip_turn_immediately = true;
                    self.turn_completed = true;
                }
                self.dirty = true;
                return;
            }

            // If we were waiting for an item (single-slot rotation), consume it as soon as ready
            if self.active_item.is_none() {
                if let Some(item) = guard.ready_item.take() {
                    guard.readiness = BatchReadiness::Warming;
                    guard.requested_start_card_id = Some(item.id.clone());
                    guard.stream_eof = false;
                    guard.live_udp_frame = None;
                    guard.last_udp_packet_at = None;
                    self.active_item = Some(item);
                    self.phase = PlaybackPhase::Card;
                    self.phase_started_at = now;
                    self.dirty = true;
                }
            }
        }

        let Some(item_snapshot) = self.active_item.clone() else {
            if now.saturating_duration_since(self.last_retry_poll) >= Duration::from_millis(2500) {
                self.last_retry_poll = now;
                self.spawn_background_sync();
            }
            self.dirty = true;
            return;
        };

        let phase_elapsed = now.saturating_duration_since(self.phase_started_at);

        match self.phase {
            PlaybackPhase::Card => {
                self.dirty = true;
                let card_target_dur = self.organic_card_duration(&item_snapshot);
                if phase_elapsed >= card_target_dur {
                    if item_snapshot.has_stream {
                        self.start_live_stream_phase(now, &item_snapshot.id);
                    } else {
                        self.finish_current_turn();
                    }
                }
            }
            PlaybackPhase::LiveStream => {
                self.dirty = true;
                // 1. If playing HTTP pre-decoded stream frames for this single item
                if !item_snapshot.http_stream_frames.is_empty() {
                    self.http_frame_elapsed += dt;
                    if self.http_frame_elapsed >= item_snapshot.http_frame_delay {
                        self.http_frame_elapsed = Duration::ZERO;
                        self.http_frame_idx += 1;
                        if self.http_frame_idx >= item_snapshot.http_stream_frames.len() {
                            self.finish_current_turn();
                        }
                    }
                    return;
                }

                // 2. Real-time UDP stream (`mqtt_udp`) with `stream_eof` + 1500ms packet watchdog
                let (stream_eof, last_pkt) = {
                    let guard = self.shared.lock().unwrap();
                    (guard.stream_eof, guard.last_udp_packet_at)
                };
                let max_stream_dur =
                    Duration::from_secs_f32((item_snapshot.stream_duration_s + 4.0).max(6.0));
                if stream_eof || phase_elapsed >= max_stream_dur {
                    self.finish_current_turn();
                    return;
                }
                if let Some(last_at) = last_pkt {
                    if now.saturating_duration_since(last_at)
                        >= Duration::from_millis(UDP_STREAM_WATCHDOG_MS)
                    {
                        self.finish_current_turn();
                    }
                } else if phase_elapsed >= Duration::from_millis(UDP_INITIAL_GRACE_MS) {
                    self.finish_current_turn();
                }
            }
        }
    }

    pub fn is_finished(&self) -> bool {
        if !self.turn_evaluated {
            return false;
        }
        if self.skip_turn_immediately || self.turn_completed {
            return true;
        }
        false
    }

    fn render_card_adaptive(
        &self,
        matrix: &mut dyn IDrawingSurface,
        item: &PreparedShowcaseItem,
        w: i32,
        h: i32,
        phase_ms: u128,
    ) {
        let bottom_text = if !item.synopsis.is_empty() && self.cfg.show_synopsis {
            item.synopsis.as_str()
        } else {
            item.subtitle.as_str()
        };
        let speed_px = self.cfg.synopsis_px_per_sec();

        // 1. TATE Vertical Portrait Mode (`h > w`, e.g. 64x128, 64x256, 32x128)
        if h > w {
            let badge_txt = if item.badge.len() > 8 {
                &item.badge[..8]
            } else {
                &item.badge
            };
            let badge_w = (badge_txt.chars().count() as i32) * 4 + 3;
            fill_round_rect(matrix, 2, 2, badge_w, 7, 1, item.badge_rgb);
            draw_tiny_text(matrix, badge_txt, 4, 3, 2, 2 + badge_w, (255, 255, 255));

            let meta_x = 2 + badge_w + 2;
            let meta_label = format!("{} *{}", item.year, item.rating);
            draw_tiny_text(matrix, &meta_label, meta_x, 3, meta_x, w - 1, (255, 215, 0));

            let mut cursor_y = 12;
            if self.cfg.show_poster {
                if let Some(poster) = &item.poster {
                    let pw = (poster.width() as i32).min(w - 4);
                    let ph = (poster.height() as i32).min(h / 2);
                    let jx = ((w - pw) / 2).max(0);
                    for py in 0..ph {
                        for px in 0..pw {
                            let p = poster.get_pixel(px as u32, py as u32).0;
                            matrix.set_pixel(jx + px, cursor_y + py, p[0], p[1], p[2]);
                        }
                    }
                    draw_round_rect(
                        matrix,
                        (jx - 1).max(0),
                        (cursor_y - 1).max(0),
                        pw + 2,
                        ph + 2,
                        1,
                        item.badge_rgb,
                    );
                    cursor_y += ph + 4;
                }
            }

            draw_scrolling_text(
                matrix,
                &item.title,
                2,
                w - 2,
                cursor_y,
                1,
                (255, 255, 255),
                phase_ms,
                true,
            );
            cursor_y += 10;

            if cursor_y + 16 < h {
                draw_tiny_text(
                    matrix,
                    &item.subtitle,
                    2,
                    cursor_y,
                    2,
                    w - 2,
                    (180, 195, 215),
                );
                cursor_y += 8;
            }

            draw_fast_hline(
                matrix,
                2,
                cursor_y,
                w - 4,
                (
                    item.badge_rgb.0 / 3,
                    item.badge_rgb.1 / 3,
                    item.badge_rgb.2 / 3,
                ),
            );
            let syn_y = (cursor_y + 4).min(h - 8);
            draw_single_pass_synopsis(
                matrix,
                bottom_text,
                2,
                w - 2,
                syn_y,
                (190, 225, 255),
                phase_ms,
                speed_px,
            );
            return;
        }

        // 2. Square 64x64 Layout (`w == h`)
        if w == h {
            let mut top_right_x = 2;
            if self.cfg.show_poster {
                if let Some(poster) = &item.poster {
                    let pw = (poster.width() as i32).min(w / 2);
                    let ph = (poster.height() as i32).min(h / 2);
                    for py in 0..ph {
                        for px in 0..pw {
                            let p = poster.get_pixel(px as u32, py as u32).0;
                            matrix.set_pixel(px, py, p[0], p[1], p[2]);
                        }
                    }
                    top_right_x = pw + 2;
                }
            }

            let badge_txt = if item.badge.len() > 6 {
                &item.badge[..6]
            } else {
                &item.badge
            };
            let badge_w = (badge_txt.chars().count() as i32) * 4 + 3;
            fill_round_rect(matrix, top_right_x, 2, badge_w, 7, 1, item.badge_rgb);
            draw_tiny_text(
                matrix,
                badge_txt,
                top_right_x + 2,
                3,
                top_right_x,
                w - 1,
                (255, 255, 255),
            );
            draw_tiny_text(
                matrix,
                &item.year,
                top_right_x,
                12,
                top_right_x,
                w - 1,
                (220, 220, 220),
            );
            let rate_lbl = format!("*{}", item.rating);
            draw_tiny_text(
                matrix,
                &rate_lbl,
                top_right_x,
                21,
                top_right_x,
                w - 1,
                (255, 215, 0),
            );

            let mid_y = h / 2;
            draw_fast_hline(
                matrix,
                0,
                mid_y,
                w,
                (
                    item.badge_rgb.0 / 3,
                    item.badge_rgb.1 / 3,
                    item.badge_rgb.2 / 3,
                ),
            );
            draw_scrolling_text(
                matrix,
                &item.title,
                2,
                w - 2,
                mid_y + 3,
                1,
                (255, 255, 255),
                phase_ms,
                false,
            );
            draw_tiny_text(
                matrix,
                &item.subtitle,
                2,
                mid_y + 13,
                2,
                w - 2,
                (175, 195, 215),
            );
            draw_single_pass_synopsis(
                matrix,
                bottom_text,
                2,
                w - 2,
                h - 9,
                (190, 225, 255),
                phase_ms,
                speed_px,
            );
            return;
        }

        // 3. Landscape Layouts (`128x32`, `128x64`, `256x64`)
        let text_min_x = if self.cfg.show_poster && item.poster.is_some() {
            let poster = item.poster.as_ref().unwrap();
            let pw = (poster.width() as i32).min(w / 2);
            let ph = (poster.height() as i32).min(h);
            let oy = ((h - ph) / 2).max(0);
            for py in 0..ph {
                for px in 0..pw {
                    let p = poster.get_pixel(px as u32, py as u32).0;
                    matrix.set_pixel(px, oy + py, p[0], p[1], p[2]);
                }
            }
            pw + 3
        } else {
            2
        };

        let badge_txt = if item.badge.len() > 8 {
            &item.badge[..8]
        } else {
            &item.badge
        };
        let badge_w = (badge_txt.chars().count() as i32) * 4 + 3;
        let top_y = if h >= 64 { 3 } else { 1 };
        fill_round_rect(matrix, text_min_x, top_y, badge_w, 7, 1, item.badge_rgb);
        draw_tiny_text(
            matrix,
            badge_txt,
            text_min_x + 2,
            top_y + 1,
            text_min_x,
            text_min_x + badge_w,
            (255, 255, 255),
        );

        let meta_x = text_min_x + badge_w + 3;
        let meta_label = format!("{} *{}", item.year, item.rating);
        draw_tiny_text(
            matrix,
            &meta_label,
            meta_x,
            top_y + 1,
            meta_x,
            w - 1,
            (255, 215, 0),
        );

        if h >= 64 {
            draw_scrolling_text(
                matrix,
                &item.title,
                text_min_x,
                w - 2,
                14,
                1,
                (255, 255, 255),
                phase_ms,
                false,
            );
            draw_tiny_text(
                matrix,
                &item.subtitle,
                text_min_x,
                26,
                text_min_x,
                w - 2,
                (180, 200, 220),
            );
            draw_fast_hline(
                matrix,
                text_min_x,
                36,
                w - text_min_x - 2,
                (
                    item.badge_rgb.0 / 3,
                    item.badge_rgb.1 / 3,
                    item.badge_rgb.2 / 3,
                ),
            );
            draw_single_pass_synopsis(
                matrix,
                bottom_text,
                text_min_x,
                w - 2,
                44,
                (190, 225, 255),
                phase_ms,
                speed_px,
            );
        } else {
            draw_scrolling_text(
                matrix,
                &item.title,
                text_min_x,
                w - 1,
                10,
                1,
                (255, 255, 255),
                phase_ms,
                false,
            );
            draw_fast_hline(
                matrix,
                text_min_x,
                19,
                w - text_min_x - 1,
                (
                    item.badge_rgb.0 / 3,
                    item.badge_rgb.1 / 3,
                    item.badge_rgb.2 / 3,
                ),
            );
            draw_single_pass_synopsis(
                matrix,
                bottom_text,
                text_min_x,
                w - 1,
                22,
                (190, 225, 255),
                phase_ms,
                speed_px,
            );
        }
    }

    pub fn render(&mut self, context: &mut EngineContext) {
        if self.skip_turn_immediately || self.turn_completed {
            return;
        }
        let matrix = &mut *context.surface;
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;
        let phase_ms = self.phase_started_at.elapsed().as_millis();

        let Some(ref item) = self.active_item else {
            render_waiting_pixel_art(self.domain, matrix, w, h, phase_ms, &self.cfg.server_url);
            self.dirty = false;
            return;
        };

        match self.phase {
            PlaybackPhase::Card => {
                self.render_card_adaptive(matrix, item, w, h, phase_ms);
            }
            PlaybackPhase::LiveStream => {
                if !item.http_stream_frames.is_empty() {
                    if let Some(frame) = item
                        .http_stream_frames
                        .get(self.http_frame_idx)
                        .or_else(|| item.http_stream_frames.last())
                    {
                        let fw = (frame.width() as i32).min(w);
                        let fh = (frame.height() as i32).min(h);
                        for py in 0..fh {
                            for px in 0..fw {
                                let p = frame.get_pixel(px as u32, py as u32).0;
                                matrix.set_pixel(px, py, p[0], p[1], p[2]);
                            }
                        }
                    }
                } else {
                    let live_frame = {
                        let guard = self.shared.lock().unwrap();
                        guard.live_udp_frame.clone()
                    };
                    if let Some(frame) = live_frame {
                        let fw = (frame.width() as i32).min(w);
                        let fh = (frame.height() as i32).min(h);
                        for py in 0..fh {
                            for px in 0..fw {
                                let p = frame.get_pixel(px as u32, py as u32).0;
                                matrix.set_pixel(px, py, p[0], p[1], p[2]);
                            }
                        }
                    } else {
                        // Stream is warming / awaiting first UDP packet -> show creative Pixel-Art scene
                        render_waiting_pixel_art(
                            self.domain,
                            matrix,
                            w,
                            h,
                            phase_ms,
                            &self.cfg.server_url,
                        );
                    }
                }
            }
        }
        self.dirty = false;
    }
}

struct BusyResetGuard(Arc<AtomicBool>);
impl Drop for BusyResetGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

pub fn build_common_showcase_fields(
    default_video_mode: &'static str,
) -> Vec<crate::core::engine_contract::ConfigField> {
    use crate::core::engine_contract::{ConfigField, ConfigOption, ConfigType, ValidationPolicy};
    vec![
        ConfigField {
            id: "server_url",
            field_type: ConfigType::String,
            label: "Showcase Server URL",
            description: "LAN URL of the dmd-media-showcase server (e.g. http://192.168.1.50:8080)",
            default_value: "http://127.0.0.1:8080",
            required: true,
            validation_policy: ValidationPolicy::Accept,
            ..Default::default()
        },
        ConfigField {
            id: "transport_mode",
            field_type: ConfigType::Options,
            label: "Transport Protocol",
            description: "Real-Time MQTT + UDP (ffmpeg -re) or HTTP Polling fallback",
            default_value: "mqtt_udp",
            options: Some(vec![
                ConfigOption { label: "MQTT + UDP Real-Time (Recommended)", value: "mqtt_udp" },
                ConfigOption { label: "HTTP Polling (Fallback / Testing)", value: "http_poll" },
            ]),
            validation_policy: ValidationPolicy::FallbackDefault,
            ..Default::default()
        },
        ConfigField {
            id: "use_showcase_broker",
            field_type: ConfigType::Boolean,
            label: "Use Showcase Embedded MQTT Broker",
            description: "Automatically connect to the Showcase server's MQTT broker (<server_ip>:1883) without extra setup",
            default_value: "true",
            validation_policy: ValidationPolicy::FallbackDefault,
            ..Default::default()
        },
        ConfigField {
            id: "mqtt_broker_source",
            field_type: ConfigType::Options,
            label: "External MQTT Broker Source",
            description: "When Embedded Broker is disabled, choose ArcadeMatrix Global MQTT or a Custom Broker URL",
            default_value: "global",
            options: Some(vec![
                ConfigOption { label: "Global ArcadeMatrix MQTT Broker", value: "global" },
                ConfigOption { label: "Custom Showcase MQTT Broker", value: "custom" },
            ]),
            validation_policy: ValidationPolicy::FallbackDefault,
            ..Default::default()
        },
        ConfigField {
            id: "mqtt_custom_url",
            field_type: ConfigType::String,
            label: "Custom MQTT Broker URL",
            description: "Used when External MQTT Broker Source is set to Custom (e.g. mqtt://192.168.1.200:1883)",
            default_value: "",
            validation_policy: ValidationPolicy::Accept,
            ..Default::default()
        },
        ConfigField {
            id: "lang",
            field_type: ConfigType::Options,
            label: "Language",
            description: "Language for localized titles, posters, and synopses",
            default_value: "system",
            options: Some(vec![
                ConfigOption { label: "System Default", value: "system" },
                ConfigOption { label: "French (FR)", value: "fr" },
                ConfigOption { label: "English (EN)", value: "en" },
                ConfigOption { label: "Spanish (ES)", value: "es" },
                ConfigOption { label: "German (DE)", value: "de" },
                ConfigOption { label: "Italian (IT)", value: "it" },
            ]),
            validation_policy: ValidationPolicy::FallbackDefault,
            ..Default::default()
        },
        ConfigField {
            id: "text_scroll_speed",
            field_type: ConfigType::Options,
            label: "Synopsis Scroll Speed",
            description: "Reading speed for the organic full-synopsis scroll on the Info Card (1 = Very Slow to 10 = Turbo)",
            default_value: "4",
            options: Some(vec![
                ConfigOption { label: "1 - Very Slow (14 px/s)", value: "1" },
                ConfigOption { label: "2 - Slow (18 px/s)", value: "2" },
                ConfigOption { label: "3 - Relaxed (24 px/s)", value: "3" },
                ConfigOption { label: "4 - Normal (32 px/s)", value: "4" },
                ConfigOption { label: "5 - Medium-Fast (42 px/s)", value: "5" },
                ConfigOption { label: "6 - Fast (56 px/s)", value: "6" },
                ConfigOption { label: "7 - Very Fast (75 px/s)", value: "7" },
                ConfigOption { label: "8 - Ultra Fast (100 px/s)", value: "8" },
                ConfigOption { label: "9 - Hyper (130 px/s)", value: "9" },
                ConfigOption { label: "10 - Turbo (160 px/s)", value: "10" },
                ConfigOption { label: "Legacy: Slow (18 px/s)", value: "slow" },
                ConfigOption { label: "Legacy: Normal (32 px/s)", value: "normal" },
                ConfigOption { label: "Legacy: Fast (56 px/s)", value: "fast" },
            ]),
            validation_policy: ValidationPolicy::FallbackDefault,
            ..Default::default()
        },
        ConfigField {
            id: "show_poster",
            field_type: ConfigType::Boolean,
            label: "Show Mini-Jacket on Card",
            description: "Display the adaptive mini-jacket (32x32 / 48x64 / TATE) on the metadata card",
            default_value: "true",
            validation_policy: ValidationPolicy::FallbackDefault,
            ..Default::default()
        },
        ConfigField {
            id: "show_synopsis",
            field_type: ConfigType::Boolean,
            label: "Show Full Scrolling Synopsis",
            description: "Scroll 100% of the localized synopsis on the metadata card before advancing",
            default_value: "true",
            validation_policy: ValidationPolicy::FallbackDefault,
            ..Default::default()
        },
        ConfigField {
            id: "jacket_panorama",
            field_type: ConfigType::Boolean,
            label: "Jacket Panorama (Top-to-Bottom Scroll)",
            description: "Scroll the full Jacket / Poster from top to bottom right after the info card",
            default_value: "true",
            validation_policy: ValidationPolicy::FallbackDefault,
            ..Default::default()
        },
        ConfigField {
            id: "marquee_panorama",
            field_type: ConfigType::Boolean,
            label: "Marquee / Banner Panorama",
            description: "Scroll the Marquee / Wheel / Wide Banner in panorama mode",
            default_value: "false",
            validation_policy: ValidationPolicy::FallbackDefault,
            ..Default::default()
        },
        ConfigField {
            id: "snapshot_panorama_count",
            field_type: ConfigType::Integer,
            label: "Snapshot Panoramas Count (0-5)",
            description: "Number of in-game screenshots or scene stills to scroll from top to bottom (0 to 5)",
            default_value: "0",
            min_val: Some("0"),
            max_val: Some("5"),
            step: Some("1"),
            validation_policy: ValidationPolicy::Clamp,
            ..Default::default()
        },
        ConfigField {
            id: "show_video",
            field_type: ConfigType::Boolean,
            label: "Show Video Stream (Skip if Unavailable)",
            description: "Play the video trailer/gameplay clip if available; automatically skipped when no video exists",
            default_value: "true",
            validation_policy: ValidationPolicy::FallbackDefault,
            ..Default::default()
        },
        ConfigField {
            id: "video_max_duration_s",
            field_type: ConfigType::Integer,
            label: "Max Video Duration (s)",
            description: "Maximum playback duration per video clip in seconds",
            default_value: "8",
            min_val: Some("2"),
            max_val: Some("30"),
            step: Some("1"),
            validation_policy: ValidationPolicy::Clamp,
            ..Default::default()
        },
        ConfigField {
            id: "video_fps",
            field_type: ConfigType::Integer,
            label: "Video FPS",
            description: "Target frame rate for video conversion and streaming",
            default_value: "15",
            min_val: Some("6"),
            max_val: Some("30"),
            step: Some("1"),
            validation_policy: ValidationPolicy::Clamp,
            ..Default::default()
        },
        ConfigField {
            id: "video_mode",
            field_type: ConfigType::Options,
            label: "Video Crop & Stabilization Preset",
            description: "Conversion preset applied by dmd-gif-converter on the server",
            default_value: default_video_mode,
            options: Some(vec![
                ConfigOption { label: "AI Auto-Action Crop (Tripod Stabilized)", value: "auto_action" },
                ConfigOption { label: "Pixel Art (Retro Crisp + Pan)", value: "pixel_art" },
                ConfigOption { label: "Vertical Panorama Scroll", value: "panorama" },
                ConfigOption { label: "Anime (Vibrant Cel)", value: "anime" },
                ConfigOption { label: "Cinema (Dark-Scene Lift)", value: "cinema" },
                ConfigOption { label: "Center Crop", value: "center_crop" },
            ]),
            validation_policy: ValidationPolicy::FallbackDefault,
            ..Default::default()
        },
        ConfigField {
            id: "color_boost",
            field_type: ConfigType::Boolean,
            label: "Smart LED Color Boost",
            description: "Apply automatic gamma, contrast, and saturation boost for HUB75 LEDs",
            default_value: "true",
            validation_policy: ValidationPolicy::FallbackDefault,
            ..Default::default()
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::matrix::MockSurface;

    #[test]
    fn test_decode_rgb565_le_image_and_stream() {
        let mut raw = Vec::new();
        for _ in 0..4 {
            raw.extend_from_slice(&0xF800u16.to_le_bytes());
        }
        let img = decode_rgb565_le_image(&raw, 2, 2).expect("valid 2x2 rgb565");
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 2);
        assert!(img.get_pixel(0, 0).0[0] >= 248);

        let mut stream = Vec::new();
        stream.extend_from_slice(DMDS_MAGIC);
        stream.push(1);
        stream.push(15);
        stream.extend_from_slice(&2u16.to_le_bytes());
        stream.extend_from_slice(&2u16.to_le_bytes());
        stream.extend_from_slice(&2u16.to_le_bytes());
        stream.extend_from_slice(&66u16.to_le_bytes());
        stream.extend_from_slice(&0u16.to_le_bytes());
        stream.extend_from_slice(&raw);
        stream.extend_from_slice(&raw);

        let (frames, delay) = decode_rgb565_stream(&stream).expect("valid DMDS stream");
        assert_eq!(frames.len(), 2);
        assert_eq!(delay, Duration::from_millis(66));
    }

    #[test]
    fn test_dmdf_udp_chunk_assembler() {
        let mut assembler = DmdfFrameAssembler::new(2, 2);
        let mut pkt0 = Vec::new();
        pkt0.extend_from_slice(DMDF_MAGIC);
        pkt0.extend_from_slice(&7u16.to_le_bytes());
        pkt0.push(0); // chunk_idx = 0
        pkt0.push(2); // total_chunks = 2
        pkt0.extend_from_slice(&0u16.to_le_bytes()); // offset 0
        pkt0.extend_from_slice(&0xF800u16.to_le_bytes());
        pkt0.extend_from_slice(&0xF800u16.to_le_bytes());

        assert!(assembler.push_packet(&pkt0).is_none());

        let mut pkt1 = Vec::new();
        pkt1.extend_from_slice(DMDF_MAGIC);
        pkt1.extend_from_slice(&7u16.to_le_bytes());
        pkt1.push(1); // chunk_idx = 1
        pkt1.push(2); // total_chunks = 2
        pkt1.extend_from_slice(&4u16.to_le_bytes()); // offset 4
        pkt1.extend_from_slice(&0x07E0u16.to_le_bytes());
        pkt1.extend_from_slice(&0x07E0u16.to_le_bytes());

        let (seq, img) = assembler.push_packet(&pkt1).expect("complete 2x2 frame");
        assert_eq!(seq, 7);
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 2);
        assert!(img.get_pixel(0, 0).0[0] >= 248);
        assert!(img.get_pixel(0, 1).0[1] >= 250);
    }

    #[test]
    fn test_resolve_mqtt_broker_endpoint_three_modes() {
        let mut cfg = ShowcaseConfig::default();
        cfg.server_url = "http://192.168.1.55:8080".to_string();
        cfg.use_showcase_broker = true;

        assert_eq!(
            resolve_mqtt_broker_endpoint(&cfg, Some(("10.0.0.1", 1884))),
            Some(("192.168.1.55".to_string(), 1883))
        );

        cfg.use_showcase_broker = false;
        cfg.mqtt_broker_source = "global".to_string();
        assert_eq!(
            resolve_mqtt_broker_endpoint(&cfg, Some(("10.0.0.1", 1884))),
            Some(("10.0.0.1".to_string(), 1884))
        );

        cfg.mqtt_broker_source = "custom".to_string();
        cfg.mqtt_custom_url = "mqtt://192.168.1.200:1888".to_string();
        assert_eq!(
            resolve_mqtt_broker_endpoint(&cfg, Some(("10.0.0.1", 1884))),
            Some(("192.168.1.200".to_string(), 1888))
        );
    }

    #[test]
    fn test_compute_card_poster_dims_multi_format_and_tate() {
        assert_eq!(compute_card_poster_dims(128, 32), (32, 32));
        assert_eq!(compute_card_poster_dims(128, 64), (48, 64));
        assert_eq!(compute_card_poster_dims(256, 64), (48, 64));
        assert_eq!(compute_card_poster_dims(64, 64), (32, 32));
        assert_eq!(compute_card_poster_dims(64, 128), (40, 40));
        assert_eq!(compute_card_poster_dims(32, 128), (24, 24));
    }

    #[test]
    fn test_consume_once_and_zero_cache_on_offline() {
        let mut runner = MediaShowcaseRunner::new(ShowcaseDomain::Games, 128, 32);
        {
            let mut guard = runner.shared.lock().unwrap();
            guard.readiness = BatchReadiness::Ready;
            guard.server_online = true;
            guard.ready_item = Some(PreparedShowcaseItem {
                id: "item-1".to_string(),
                title: "Metal Slug".to_string(),
                year: "1996".to_string(),
                badge: "NEOGEO".to_string(),
                badge_rgb: (255, 109, 0),
                subtitle: "SNK".to_string(),
                rating: "9.2".to_string(),
                synopsis: "Classic run-and-gun action.".to_string(),
                poster: None,
                has_stream: false,
                stream_duration_s: 6.0,
                stream_fps: 15,
                http_stream_frames: Vec::new(),
                http_frame_delay: Duration::from_millis(66),
            });
        }

        // First turn consumes the item via .take()
        runner.activate();
        runner.evaluate_turn_start(2);
        assert!(!runner.skip_turn_immediately);
        assert!(runner.active_item.is_some());
        assert!(runner.shared.lock().unwrap().ready_item.is_none());

        // Deactivating or starting a second turn without a new server item must NOT replay item-1!
        runner.deactivate();
        assert!(runner.active_item.is_none());
        runner.activate();
        runner.evaluate_turn_start(2);
        assert!(runner.skip_turn_immediately);
        assert!(runner.is_finished());
        assert!(runner.active_item.is_none());
    }

    #[test]
    fn test_waiting_pixel_art_all_domains_and_resolutions() {
        for domain in [
            ShowcaseDomain::Games,
            ShowcaseDomain::Movies,
            ShowcaseDomain::Series,
            ShowcaseDomain::Anime,
        ] {
            for (w, h) in [(128, 32), (128, 64), (64, 64), (64, 128)] {
                let mut surface = MockSurface::new(w, h);
                render_waiting_pixel_art(
                    domain,
                    &mut surface,
                    w as i32,
                    h as i32,
                    250,
                    "http://127.0.0.1:8080",
                );
            }
        }
    }

    #[test]
    fn test_base64_poster_and_scroll_speed_1_to_10() {
        let decoded = decode_base64_bytes("AAH/").expect("valid base64");
        assert_eq!(decoded, vec![0x00, 0x01, 0xFF]);

        let mut cfg = ShowcaseConfig::default();
        cfg.text_scroll_speed = "1".to_string();
        assert_eq!(cfg.synopsis_px_per_sec(), 14);
        cfg.text_scroll_speed = "4".to_string();
        assert_eq!(cfg.synopsis_px_per_sec(), 32);
        cfg.text_scroll_speed = "10".to_string();
        assert_eq!(cfg.synopsis_px_per_sec(), 160);
        cfg.text_scroll_speed = "fast".to_string();
        assert_eq!(cfg.synopsis_px_per_sec(), 56);

        let payload = br##"{"ready": true, "item_id": "pkg_games_f6dfa1b3_games_libretro_nes_defender_of_the_crown", "domain": "games", "card": {"id": "pkg_games_f6dfa1b3_games_libretro_nes_defender_of_the_crown", "item_id": "pkg_games_f6dfa1b3_games_libretro_nes_defender_of_the_crown", "domain": "games", "title": "Defender of the Crown", "year": "1980", "badge": "NES", "badge_color": "#D32F2F", "subtitle": "Plateforme \u2022 NES", "rating": "8.7", "synopsis": "Defender of the Crown est un jeu vid\u00e9o...", "lang": "fr", "source_used": "libretro_wikidata", "poster_width": 32, "poster_height": 32, "poster_rgb565_b64": "AAH/", "poster_rgb565_url": "/api/v1/games/poster/pkg.rgb565", "text_scroll_speed": "normal", "has_panoramas": true, "has_video": true, "stream_frames": 573, "stream_fps": 28, "fps": 28, "stream_duration_s": 20.46, "total_stream_duration_s": 20.46, "assets": {"poster_rgb565": "/api/v1/games/poster/pkg.rgb565", "unified_stream_rgb565": "/api/v1/games/stream/pkg.rgb565.stream"}}}"##;
        let parsed: MqttReadyPayload = serde_json::from_slice(payload)
            .expect("MQTT ready payload with both alias keys must deserialize");
        assert!(parsed.ready);
        let card = parsed.card.expect("card present");
        assert_eq!(card.effective_stream_fps(), 28);
        assert!((card.effective_stream_duration_s() - 20.46).abs() < 0.01);
    }
}

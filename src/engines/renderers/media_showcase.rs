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
use rumqttc::{
    Client as MqttSyncClient, Event as MqttEvent, LastWill, MqttOptions, Outgoing, Packet, QoS,
};
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
    pub enabled: bool,
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
    pub color_boost: String,
    pub systems: String,
    pub genres: String,
    pub decades: String,
    pub min_rating: f32,
    pub video_type: String,
}

impl Default for ShowcaseConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            server_url: "http://dmdmediashowcase.local:8890".to_string(),
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
            color_boost: "auto".to_string(),
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
        out.enabled = cfg.get_bool("enabled", true);
        let raw_url = cfg.get_string("server_url", "http://dmdmediashowcase.local:8890");
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
        out.video_max_duration_s = cfg.get_int("video_max_duration_s", 8).clamp(2, 120) as u32;
        out.video_fps = cfg.get_int("video_fps", 15).clamp(6, 60) as u32;
        let _ = domain;
        let raw_vm = cfg.get_string("video_mode", "auto_action");
        out.video_mode = match raw_vm.trim().to_ascii_lowercase().as_str() {
            "panorama" => "panorama".to_string(),
            "center_crop" => "center_crop".to_string(),
            _ => "auto_action".to_string(),
        };
        let raw_cb = cfg.get_string("color_boost", "auto");
        out.color_boost = match raw_cb.trim().to_ascii_lowercase().as_str() {
            "pixel_art" => "pixel_art".to_string(),
            "anime" => "anime".to_string(),
            "cinema" | "false" | "0" | "off" | "no" => "cinema".to_string(),
            _ => "auto".to_string(),
        };
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

    pub fn apply_unified_settings(&mut self, unified: &crate::core::config::ShowcaseConfig) {
        self.enabled = unified.enabled;
        if !unified.server_url.is_empty() {
            self.server_url = unified.server_url.clone();
            self.transport_mode = unified.transport_mode.clone();
            self.use_showcase_broker = unified.use_showcase_broker;
            self.mqtt_broker_source = unified.mqtt_broker_source.clone();
            self.mqtt_custom_url = unified.mqtt_custom_url.clone();
        }
    }

    pub fn effective_lang<'a>(&'a self, system_lang: &'a str) -> &'a str {
        if self.lang.is_empty() || self.lang.eq_ignore_ascii_case("system") {
            if system_lang.is_empty() {
                "en"
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

/// Normalize a bare single-label LAN hostname (e.g. `"dmdmediashowcase"` or `"mosquitto"`)
/// to its `.local` mDNS form while leaving IPv4/IPv6 literals, `localhost`, and FQDNs untouched.
pub fn normalize_mdns_hostname(host: &str) -> String {
    let trimmed = host.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.eq_ignore_ascii_case("auto") {
        return "dmdmediashowcase.local".to_string();
    }
    if !trimmed.contains('.')
        && !trimmed.contains(':')
        && !trimmed.starts_with('[')
        && !trimmed.eq_ignore_ascii_case("localhost")
    {
        return format!("{}.local", trimmed);
    }
    trimmed.to_string()
}

/// Normalize an HTTP or MQTT endpoint URL, migrating legacy v1.0.0 Showcase URLs
/// (e.g. `"http://dmdmediashowcase:8890"`) and appending `.local` to single-label LAN hosts.
pub fn normalize_mdns_endpoint_url(raw_url: &str, migrate_showcase_legacy: bool) -> String {
    let trimmed = raw_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    if migrate_showcase_legacy {
        let low = trimmed.to_ascii_lowercase();
        if matches!(
            low.as_str(),
            "http://192.168.1.50:8000"
                | "http://127.0.0.1:8080"
                | "http://dmdmediashowcase:8080"
                | "http://dmdmediashowcase.local:8080"
                | "http://dmdmediashowcase:8890"
                | "http://dmdmediashowcase"
                | "dmdmediashowcase:8890"
                | "dmdmediashowcase"
        ) {
            return "http://dmdmediashowcase.local:8890".to_string();
        }
    }

    let (scheme_prefix, rest) = if let Some(idx) = trimmed.find("://") {
        (&trimmed[..idx + 3], &trimmed[idx + 3..])
    } else {
        ("", trimmed)
    };
    let (auth, path_suffix) = if let Some(slash_idx) = rest.find('/') {
        (&rest[..slash_idx], &rest[slash_idx..])
    } else {
        (rest, "")
    };
    let close_bracket = auth.rfind(']');
    let (host, port_suffix) = if let Some(colon_idx) = auth.rfind(':') {
        if colon_idx > 0 && close_bracket.is_none_or(|b| colon_idx > b) {
            (&auth[..colon_idx], &auth[colon_idx..])
        } else {
            (auth, "")
        }
    } else {
        (auth, "")
    };

    if !host.is_empty()
        && !host.contains('.')
        && !host.contains(':')
        && !host.eq_ignore_ascii_case("localhost")
        && !host.eq_ignore_ascii_case("auto")
    {
        return format!(
            "{}{}.local{}{}",
            scheme_prefix, host, port_suffix, path_suffix
        );
    }
    trimmed.to_string()
}

/// Extract `(host, port)` from an HTTP or MQTT URL string, normalizing bare single-label
/// LAN hostnames (e.g. `http://dmdmediashowcase:8890`) to `.local` (`dmdmediashowcase.local`).
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
    let close_bracket = authority.rfind(']');
    if let Some((h, p_str)) = authority.rsplit_once(':') {
        let colon_pos = h.len();
        if !h.is_empty() && close_bracket.is_none_or(|b| colon_pos > b) {
            let port = p_str.parse::<u16>().unwrap_or(default_port);
            return Some((normalize_mdns_hostname(h), port));
        }
    }
    Some((normalize_mdns_hostname(authority), default_port))
}

fn skip_dns_name(packet: &[u8], mut offset: usize) -> Option<usize> {
    let mut jumps = 0;
    while offset < packet.len() {
        let len = packet[offset];
        if len == 0 {
            return Some(offset + 1);
        }
        if (len & 0xC0) == 0xC0 {
            if offset + 2 > packet.len() {
                return None;
            }
            return Some(offset + 2);
        }
        offset += 1 + (len as usize);
        jumps += 1;
        if jumps > 32 {
            return None;
        }
    }
    None
}

static CANDIDATE_SHOWCASE_IPS: std::sync::OnceLock<Mutex<Vec<String>>> = std::sync::OnceLock::new();
static MDNS_CACHE: std::sync::OnceLock<
    Mutex<std::collections::HashMap<String, (String, Instant)>>,
> = std::sync::OnceLock::new();
static RPI_HW_SHORT_ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static MQTT_CONN_SEQ: AtomicU64 = AtomicU64::new(1);

/// Extract the Broadcom SoC hardware serial from `/proc/cpuinfo` content if present.
/// Prioritizing silicon OTP serial over `/etc/machine-id` guarantees distinct `<hwid>` tags
/// even when multiple Raspberry Pi boards boot from cloned SD card images.
pub fn extract_cpuinfo_serial(cpuinfo: &str) -> Option<String> {
    for line in cpuinfo.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        if k.trim().eq_ignore_ascii_case("Serial") {
            let clean = v.trim().trim_matches('\0');
            if !clean.is_empty() && clean.chars().any(|c| c != '0') {
                return Some(clean.to_ascii_lowercase());
            }
        }
    }
    None
}

/// Deterministically fold an arbitrary hardware identity seed into a 6-lowercase-hex short ID.
pub fn derive_rpi_short_id_from_seed(seed: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in seed.trim().as_bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let mut folded = (((hash >> 40) ^ (hash >> 20) ^ hash) as u32) & 0x00FF_FFFF;
    if folded == 0 {
        folded = 0x0052_5049;
    }
    format!("{:06x}", folded)
}

fn detect_rpi_hardware_seed() -> String {
    if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
        if let Some(serial) = extract_cpuinfo_serial(&cpuinfo) {
            return format!("cpu-serial:{}", serial);
        }
    }
    if let Ok(dt_serial) = std::fs::read_to_string("/sys/firmware/devicetree/base/serial-number") {
        let clean = dt_serial.trim().trim_matches('\0');
        if !clean.is_empty() && clean.chars().any(|c| c != '0') {
            return format!("dt-serial:{}", clean);
        }
    }
    if let Ok(entries) = std::fs::read_dir("/sys/class/net") {
        let mut macs = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "lo"
                || name.starts_with("docker")
                || name.starts_with("veth")
                || name.starts_with("br-")
                || name.starts_with("virbr")
            {
                continue;
            }
            if let Ok(mac) = std::fs::read_to_string(entry.path().join("address")) {
                let clean = mac.trim().to_ascii_lowercase();
                if clean.len() == 17 && clean != "00:00:00:00:00:00" {
                    macs.push(format!("{}:{}", name, clean));
                }
            }
        }
        macs.sort();
        if let Some(primary_mac) = macs.first() {
            return format!("net-mac:{}", primary_mac);
        }
    }
    for path in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
        if let Ok(mid) = std::fs::read_to_string(path) {
            let clean = mid.trim();
            if !clean.is_empty() {
                return format!("machine-id:{}", clean);
            }
        }
    }
    if let Ok(host) = std::fs::read_to_string("/etc/hostname") {
        let clean = host.trim();
        if !clean.is_empty() {
            return format!("hostname:{}", clean);
        }
    }
    if let Ok(host) = std::env::var("HOSTNAME") {
        let clean = host.trim();
        if !clean.is_empty() {
            return format!("env-host:{}", clean);
        }
    }
    if let Some(ip) = probe_local_lan_ip("8.8.8.8", 53) {
        return format!("lan-ip:{}", ip);
    }
    "rpi-default-node".to_string()
}

/// Return this board's stable 6-hex hardware short ID (e.g. `"a1b2c3"`), cached for process lifetime.
pub fn rpi_hardware_short_id() -> &'static str {
    RPI_HW_SHORT_ID
        .get_or_init(|| derive_rpi_short_id_from_seed(&detect_rpi_hardware_seed()))
        .as_str()
}

/// Build the hardware-scoped DMD Showcase client/topic ID (`rpi_<hwid>_<domain>_<W>x<H>`).
pub fn format_rpi_dmd_client_id(domain_slug: &str, width: u32, height: u32) -> String {
    format!(
        "rpi_{}_{}_{}x{}",
        rpi_hardware_short_id(),
        domain_slug,
        width,
        height
    )
}

/// Register a candidate LAN IPv4 address (e.g., WebUI client `peer_addr` or configured MQTT broker)
/// to prioritize during automatic `dmdmediashowcase.local` discovery when running under Docker.
pub fn register_showcase_candidate_ip(raw_ip: &str) {
    let trimmed = raw_ip.trim().trim_start_matches("::ffff:");
    let Ok(std::net::IpAddr::V4(v4)) = trimmed.parse::<std::net::IpAddr>() else {
        return;
    };
    if v4.is_loopback() || v4.is_unspecified() || v4.is_broadcast() {
        return;
    }
    let ip_str = v4.to_string();
    let list = CANDIDATE_SHOWCASE_IPS.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut guard) = list.lock() {
        guard.retain(|x| x != &ip_str);
        guard.insert(0, ip_str);
        if guard.len() > 16 {
            guard.truncate(16);
        }
    }
}

/// Clear the `.local` hostname resolution cache so the next probe performs a fresh discovery.
pub fn invalidate_showcase_host_cache() {
    if let Some(cache) = MDNS_CACHE.get() {
        if let Ok(mut guard) = cache.lock() {
            guard.clear();
        }
    }
}

/// Detailed health and broker metadata returned by `GET /api/v1/health` on `dmd-media-showcase`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowcaseHealthDetails {
    pub version: String,
    pub mqtt_mode: String,
    pub mqtt_host: String,
    pub mqtt_port: u16,
}

/// Parse a raw HTTP response from `GET /api/v1/health` and extract full Showcase health details.
pub fn parse_showcase_health_details_http_response(raw: &[u8]) -> Option<ShowcaseHealthDetails> {
    let text = String::from_utf8_lossy(raw);
    let (headers, body) = text.split_once("\r\n\r\n")?;
    let status_line = headers.lines().next().unwrap_or("");
    if !status_line.contains(" 200 ") {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(body.trim()).ok()?;
    if v.get("status").and_then(|s| s.as_str()) != Some("ok") {
        return None;
    }
    // Verify Showcase-specific signature fields so we never match an unrelated HTTP server
    if v.get("mqtt_port").is_none()
        && v.get("converter_version").is_none()
        && v.get("providers_configured").is_none()
    {
        return None;
    }
    let version = v
        .get("version")
        .and_then(|s| s.as_str())
        .unwrap_or("3.0.0")
        .to_string();
    let mqtt_mode = v
        .get("mqtt_mode")
        .and_then(|s| s.as_str())
        .unwrap_or("embedded")
        .to_string();
    let mqtt_host = v
        .get("mqtt_host")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let mqtt_port = v
        .get("mqtt_port")
        .and_then(|p| p.as_u64())
        .unwrap_or(1883)
        .clamp(1, 65535) as u16;
    Some(ShowcaseHealthDetails {
        version,
        mqtt_mode,
        mqtt_host,
        mqtt_port,
    })
}

/// Parse a raw HTTP response from `GET /api/v1/health` and verify Showcase signature fields.
pub fn parse_showcase_health_http_response(raw: &[u8]) -> Option<(String, u64)> {
    let details = parse_showcase_health_details_http_response(raw)?;
    Some((details.version, details.mqtt_port as u64))
}

/// Fast single-socket TCP + HTTP `/api/v1/health` verification returning full broker metadata.
pub fn probe_showcase_health_details(ip: &str, port: u16) -> Option<ShowcaseHealthDetails> {
    use std::io::{Read, Write};
    let clean_ip = ip.trim().trim_start_matches("::ffff:");
    let ip_addr: std::net::IpAddr = clean_ip.parse().ok()?;
    let sock_addr = std::net::SocketAddr::new(ip_addr, port);
    let mut stream =
        std::net::TcpStream::connect_timeout(&sock_addr, Duration::from_millis(80)).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(350)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(200)));

    let req = format!(
        "GET /api/v1/health HTTP/1.0\r\nHost: {}:{}\r\nUser-Agent: ArcadeMatrix-RPi/3.0\r\nAccept: application/json\r\nConnection: close\r\n\r\n",
        clean_ip, port
    );
    stream.write_all(req.as_bytes()).ok()?;
    let _ = stream.flush();

    let mut raw = Vec::with_capacity(1024);
    let _ = stream.take(8192).read_to_end(&mut raw);
    parse_showcase_health_details_http_response(&raw)
}

/// Fast single-socket TCP + HTTP `/api/v1/health` verification confirming a `dmd-media-showcase`
/// instance is reachable at `(ip, port)`. Returns `Some((version, mqtt_port))` on success.
pub fn probe_showcase_health(ip: &str, port: u16) -> Option<(String, u64)> {
    let details = probe_showcase_health_details(ip, port)?;
    Some((details.version, details.mqtt_port as u64))
}

fn read_linux_arp_neighbors() -> Vec<String> {
    let mut out = Vec::new();
    let Ok(content) = std::fs::read_to_string("/proc/net/arp") else {
        return out;
    };
    for line in content.lines().skip(1) {
        let mut parts = line.split_whitespace();
        let Some(ip) = parts.next() else { continue };
        let _hw_type = parts.next();
        let flags = parts.next().unwrap_or("");
        if flags == "0x0" {
            continue;
        }
        if let Ok(std::net::IpAddr::V4(v4)) = ip.parse::<std::net::IpAddr>() {
            if !v4.is_loopback() && !v4.is_unspecified() {
                let s = v4.to_string();
                if !out.contains(&s) {
                    out.push(s);
                }
            }
        }
    }
    out
}

/// Auto-discover a `dmd-media-showcase` server on the local network when `.local` mDNS multicast
/// is isolated by Docker bridge / Docker Desktop / Rancher Desktop VM networking.
pub fn discover_showcase_on_lan(port: u16) -> Option<String> {
    let mut priority_candidates: Vec<String> = Vec::new();
    let mut push_unique = |ip: String| {
        if !ip.is_empty() && !priority_candidates.contains(&ip) {
            priority_candidates.push(ip);
        }
    };

    if let Some(list) = CANDIDATE_SHOWCASE_IPS.get() {
        if let Ok(guard) = list.lock() {
            for ip in guard.iter() {
                push_unique(ip.clone());
            }
        }
    }
    push_unique("127.0.0.1".to_string());
    if let Some(local_ip) = probe_local_lan_ip("1.1.1.1", 80) {
        push_unique(local_ip);
    }
    for arp_ip in read_linux_arp_neighbors() {
        push_unique(arp_ip);
    }

    for cand in &priority_candidates {
        if probe_showcase_health(cand, port).is_some() {
            info!(
                "[SHOWCASE] Auto-discovered Showcase server at {}:{} (priority candidate)",
                cand, port
            );
            return Some(cand.clone());
        }
    }

    // Determine local /24 subnet prefix to scan (e.g. "192.168.1")
    let subnet_base = probe_local_lan_ip("1.1.1.1", 80)
        .or_else(|| {
            priority_candidates
                .iter()
                .find(|ip| ip.starts_with("192.168.") || ip.starts_with("10."))
                .cloned()
        })
        .and_then(|ip| {
            let parts: Vec<&str> = ip.split('.').collect();
            if parts.len() == 4 && parts[0] != "127" {
                Some(format!("{}.{}.{}", parts[0], parts[1], parts[2]))
            } else {
                None
            }
        })?;

    let found_ip: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let stop_flag = Arc::new(AtomicBool::new(false));
    let worker_count = 16usize;
    let mut handles = Vec::with_capacity(worker_count);

    for worker_idx in 0..worker_count {
        let prefix = subnet_base.clone();
        let found_clone = Arc::clone(&found_ip);
        let stop_clone = Arc::clone(&stop_flag);
        handles.push(std::thread::spawn(move || {
            let mut host_byte = (worker_idx as u16) + 1;
            while host_byte <= 254 {
                if stop_clone.load(Ordering::Relaxed) {
                    break;
                }
                let cand = format!("{}.{}", prefix, host_byte);
                if let Ok(ip_addr) = cand.parse::<std::net::IpAddr>() {
                    let sock_addr = std::net::SocketAddr::new(ip_addr, port);
                    if std::net::TcpStream::connect_timeout(&sock_addr, Duration::from_millis(35))
                        .is_ok()
                    {
                        if probe_showcase_health(&cand, port).is_some() {
                            if let Ok(mut guard) = found_clone.lock() {
                                if guard.is_none() {
                                    *guard = Some(cand);
                                }
                            }
                            stop_clone.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                }
                host_byte += worker_count as u16;
            }
        }));
    }

    for h in handles {
        let _ = h.join();
    }

    let res = found_ip.lock().ok().and_then(|mut g| g.take());
    if let Some(ref ip) = res {
        info!(
            "[SHOWCASE] Auto-discovered Showcase server on LAN subnet at {}:{}",
            ip, port
        );
        register_showcase_candidate_ip(ip);
    }
    res
}

/// Send a fast RFC 6762 UDP multicast DNS `A` query to `224.0.0.251:5353` for `.local` hostnames
/// so `dmdmediashowcase.local` resolves reliably in <120ms without blocking on OS DNS timeouts.
pub fn query_mdns_ipv4(hostname: &str, port: u16) -> Option<String> {
    let clean = normalize_mdns_hostname(hostname).to_ascii_lowercase();
    if !clean.ends_with(".local") {
        return None;
    }
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    let _ = sock.set_broadcast(true);
    let _ = sock.set_read_timeout(Some(Duration::from_millis(120)));
    let _ = sock.set_write_timeout(Some(Duration::from_millis(80)));

    let mut query = Vec::with_capacity(64);
    query.extend_from_slice(&[
        0x12, 0x34, // Transaction ID
        0x00, 0x00, // Standard query
        0x00, 0x01, // QDCOUNT = 1
        0x00, 0x00, // ANCOUNT = 0
        0x00, 0x00, // NSCOUNT = 0
        0x00, 0x00, // ARCOUNT = 0
    ]);
    for label in clean.split('.') {
        if label.is_empty() || label.len() > 63 {
            return None;
        }
        query.push(label.len() as u8);
        query.extend_from_slice(label.as_bytes());
    }
    query.push(0x00);
    query.extend_from_slice(&[0x00, 0x01]); // QTYPE = A (1)
    query.extend_from_slice(&[0x80, 0x01]); // QCLASS = IN | QU (unicast response preferred)

    let _ = sock.send_to(&query, ("224.0.0.251", 5353));
    let _ = sock.send_to(&query, ("255.255.255.255", 5353));

    let mut buf = [0u8; 512];
    let deadline = Instant::now() + Duration::from_millis(120);
    while Instant::now() < deadline {
        let Ok((n, src_addr)) = sock.recv_from(&mut buf) else {
            break;
        };
        let pkt = &buf[..n];
        if pkt.len() < 12 {
            continue;
        }
        let qdcount = u16::from_be_bytes([pkt[4], pkt[5]]);
        let ancount = u16::from_be_bytes([pkt[6], pkt[7]]);
        if ancount == 0 {
            continue;
        }
        let mut offset = 12usize;
        let mut valid = true;
        for _ in 0..qdcount {
            if let Some(next_off) = skip_dns_name(pkt, offset) {
                if next_off + 4 <= pkt.len() {
                    offset = next_off + 4;
                } else {
                    valid = false;
                    break;
                }
            } else {
                valid = false;
                break;
            }
        }
        if !valid {
            continue;
        }
        for _ in 0..ancount {
            let Some(next_off) = skip_dns_name(pkt, offset) else {
                break;
            };
            if next_off + 10 > pkt.len() {
                break;
            };
            let rtype = u16::from_be_bytes([pkt[next_off], pkt[next_off + 1]]);
            let rdlen = u16::from_be_bytes([pkt[next_off + 8], pkt[next_off + 9]]) as usize;
            let rdata_off = next_off + 10;
            if rdata_off + rdlen > pkt.len() {
                break;
            }
            if rtype == 1 && rdlen == 4 {
                let a_ip = format!(
                    "{}.{}.{}.{}",
                    pkt[rdata_off],
                    pkt[rdata_off + 1],
                    pkt[rdata_off + 2],
                    pkt[rdata_off + 3]
                );
                let is_docker_bridge =
                    pkt[rdata_off] == 172 && (16..=31).contains(&pkt[rdata_off + 1]);
                let sender_ip = src_addr.ip().to_string();
                if !is_docker_bridge && probe_showcase_health(&a_ip, port).is_some() {
                    return Some(a_ip);
                }
                if probe_showcase_health(&sender_ip, port).is_some() {
                    return Some(sender_ip);
                }
                if probe_showcase_health(&a_ip, port).is_some() {
                    return Some(a_ip);
                }
            }
            offset = rdata_off + rdlen;
        }
    }
    None
}

/// Resolve a hostname (especially `.local` mDNS hostnames such as `dmdmediashowcase.local`)
/// via direct UDP multicast mDNS (`224.0.0.251:5353`) followed by Docker-aware LAN auto-discovery.
pub fn resolve_showcase_host(host: &str) -> String {
    resolve_showcase_host_with_port(host, 8890)
}

pub fn resolve_showcase_host_with_port(host: &str, port: u16) -> String {
    let normalized = normalize_mdns_hostname(host);
    let trimmed = normalized.as_str();
    if trimmed.is_empty() || !trimmed.to_ascii_lowercase().ends_with(".local") {
        return trimmed.to_string();
    }

    let cache = MDNS_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let key = format!("{}:{}", trimmed.to_ascii_lowercase(), port);
    if let Ok(guard) = cache.lock() {
        if let Some((ip, ts)) = guard.get(&key) {
            let ttl = if ip.ends_with(".local") {
                Duration::from_secs(5)
            } else {
                Duration::from_secs(120)
            };
            if ts.elapsed() < ttl {
                return ip.clone();
            }
        }
    }

    if let Some(ip) = query_mdns_ipv4(trimmed, port)
        .or_else(|| discover_showcase_on_lan(port))
        .or_else(|| {
            if port != 8890 {
                discover_showcase_on_lan(8890)
            } else {
                None
            }
        })
    {
        if let Ok(mut guard) = cache.lock() {
            guard.insert(key, (ip.clone(), Instant::now()));
        }
        return ip;
    }

    if let Ok(mut guard) = cache.lock() {
        guard.insert(key, (trimmed.to_string(), Instant::now()));
    }
    trimmed.to_string()
}

/// Return a normalized Showcase server URL where any `.local` hostname is resolved to its LAN IPv4
/// if reachable, preserving scheme and port (defaulting to `8890`, with automatic fallback to `8890`
/// if a mistyped port like `8889` or `8080` is unreachable while `8890` is live on the same host).
pub fn resolve_showcase_server_url(raw_url: &str) -> String {
    let trimmed = raw_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    let scheme = if trimmed.starts_with("https://") {
        "https://"
    } else {
        "http://"
    };
    let Some((host, port)) = parse_host_and_port(trimmed, 8890) else {
        return trimmed.to_string();
    };
    let resolved_host = resolve_showcase_host_with_port(&host, port);
    let effective_port = if port != 8890
        && probe_showcase_health(&resolved_host, port).is_none()
        && probe_showcase_health(&resolved_host, 8890).is_some()
    {
        8890
    } else {
        port
    };
    format!("{}{}:{}", scheme, resolved_host, effective_port)
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
        let (host, _) = parse_host_and_port(&cfg.server_url, 8890)?;
        return Some((resolve_showcase_host(&host), 1883));
    }
    if cfg.mqtt_broker_source.eq_ignore_ascii_case("custom") {
        if let Some((h, p)) = parse_host_and_port(&cfg.mqtt_custom_url, 1883) {
            return Some((resolve_showcase_host(&h), p));
        }
    }
    if let Some((g_host, g_port)) = global_mqtt {
        if !g_host.trim().is_empty() {
            return Some((
                resolve_showcase_host(g_host.trim()),
                if g_port > 0 { g_port } else { 1883 },
            ));
        }
    }
    let (host, _) = parse_host_and_port(&cfg.server_url, 8890)?;
    Some((resolve_showcase_host(&host), 1883))
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

/// Re-export of the unified `PreloadedDmdStream` (`DMDZ` v1/v2 compressed or legacy `DMDS` RGB565LE).
pub type PreloadedRgb565Stream = crate::core::dmd_stream::PreloadedDmdStream;

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
    /// Single contiguous `Arc<PreloadedRgb565Stream>` for the current item, dropped immediately after playback.
    pub http_stream: Option<Arc<PreloadedRgb565Stream>>,
    pub http_frame_delay: Duration,
}

impl PreparedShowcaseItem {
    #[inline]
    pub fn http_stream_frame_count(&self) -> usize {
        self.http_stream.as_ref().map_or(0, |s| s.frame_count)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchReadiness {
    Offline,
    Warming,
    Ready,
}

/// Shared state between the network worker thread (`mqtt_udp` or `http_poll`) and the render loop.
/// Holds at most 1 unconsumed ready item, 1 live UDP frame, and at most 1 preloaded HTTP stream.
pub struct SharedShowcaseState {
    pub server_online: bool,
    pub mqtt_connected_once: bool,
    pub mqtt_connected_at: Option<Instant>,
    pub ready_state_received: bool,
    pub readiness: BatchReadiness,
    pub ready_item: Option<PreparedShowcaseItem>,
    pub preloaded_streams: std::collections::VecDeque<(String, Arc<PreloadedRgb565Stream>)>,
    pub live_udp_frame: Option<RgbImage>,
    pub live_udp_seq: u16,
    pub last_udp_packet_at: Option<Instant>,
    pub stream_eof: bool,
    pub stream_eof_item_id: Option<String>,
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
            ready_state_received: false,
            readiness: BatchReadiness::Offline,
            ready_item: None,
            preloaded_streams: std::collections::VecDeque::new(),
            live_udp_frame: None,
            live_udp_seq: 0,
            last_udp_packet_at: None,
            stream_eof: false,
            stream_eof_item_id: None,
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
        self.ready_state_received = false;
        self.preloaded_streams.clear();
        self.preloaded_streams.shrink_to_fit();
        self.live_udp_frame = None;
        self.last_udp_packet_at = None;
        self.stream_eof = true;
        self.stream_eof_item_id = None;
    }

    pub fn take_preloaded_stream(&mut self, item_id: &str) -> Option<Arc<PreloadedRgb565Stream>> {
        if let Some(pos) = self
            .preloaded_streams
            .iter()
            .position(|(id, _)| id == item_id)
        {
            let (_, stream) = self.preloaded_streams.remove(pos)?;
            return Some(stream);
        }
        None
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

/// Parse a binary Showcase stream (`DMDZ` v1/v2 or legacy `DMDS`) into a single contiguous
/// `Arc<PreloadedRgb565Stream>` (`PreloadedDmdStream`).
#[inline]
pub fn parse_rgb565_stream_compact(bytes: &[u8]) -> Option<Arc<PreloadedRgb565Stream>> {
    crate::core::dmd_stream::parse_dmd_stream_compact(bytes)
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
        let mut flushed_prev: Option<(u16, RgbImage)> = None;
        if self.current_seq != Some(hdr.frame_seq) {
            if let Some(prev_seq) = self.current_seq {
                if self.received_mask != 0 {
                    if let Some(img) = decode_rgb565_le_image(&self.buf, self.width, self.height) {
                        flushed_prev = Some((prev_seq, img));
                    }
                }
            }
            self.current_seq = Some(hdr.frame_seq);
            self.received_mask = 0;
        }
        let start = hdr.byte_offset as usize;
        let end = start.saturating_add(payload.len());
        if end > self.buf.len() || hdr.total_chunks > 64 {
            return flushed_prev;
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
        flushed_prev
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
    lang_code: &str,
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

    let msg = crate::core::i18n::showcase_waiting_label(
        lang_code,
        domain.as_api_slug(),
        server_url.is_empty(),
    );
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
    pub worker_gen: Arc<AtomicU64>,
    pub config_gen: Arc<AtomicU64>,

    // Single-item Consume-Once playback state (never cached across turns)
    pub active_item: Option<PreparedShowcaseItem>,
    pub phase: PlaybackPhase,
    pub phase_started_at: Instant,
    pub last_retry_poll: Instant,
    pub http_frame_idx: usize,
    pub http_frame_elapsed: Duration,
    pub stream_player: crate::core::dmd_stream::ShowcaseStreamPlayer,
    pub last_tick: Instant,
    pub turn_evaluated: bool,
    pub skip_turn_immediately: bool,
    pub turn_completed: bool,
    pub dirty: bool,
    pub rotation_budget: u32,
    pub items_played_in_turn: u32,
    pub rotation_slot_count: usize,
    pub consecutive_warm_skips: u32,
    pub unified_showcase: crate::core::config::ShowcaseConfig,
}

impl Drop for MediaShowcaseRunner {
    fn drop(&mut self) {
        let stopped_id = self.active_item.take().map(|it| it.id);
        if let Ok(mut guard) = self.shared.lock() {
            if let Some(id) = stopped_id {
                guard.requested_stop_stream_id = Some(id);
            }
            guard.clear_media_immediately();
        }
        self.config_gen.fetch_add(1, Ordering::SeqCst);
        crate::core::memory::trim_heap();
    }
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
            worker_gen: Arc::new(AtomicU64::new(0)),
            config_gen: Arc::new(AtomicU64::new(1)),
            active_item: None,
            phase: PlaybackPhase::Card,
            phase_started_at: now,
            last_retry_poll: now,
            http_frame_idx: 0,
            http_frame_elapsed: Duration::ZERO,
            stream_player: crate::core::dmd_stream::ShowcaseStreamPlayer::new(),
            last_tick: now,
            turn_evaluated: false,
            skip_turn_immediately: false,
            turn_completed: false,
            dirty: true,
            rotation_budget: 1,
            items_played_in_turn: 0,
            rotation_slot_count: 1,
            consecutive_warm_skips: 0,
            unified_showcase: crate::core::config::ShowcaseConfig::default(),
        }
    }

    pub fn set_rotation_budget(&mut self, budget: u32) {
        self.rotation_budget = budget.max(1);
    }

    /// Transverse overlays (e.g. M.U.G.E.N Fighter) are permitted during `Card` / waiting scenes
    /// and strictly forbidden during `LiveStream` video/panorama playback (1:1 parity with ESP32).
    #[inline]
    pub fn allows_overlay(&self) -> bool {
        self.phase == PlaybackPhase::Card
    }

    /// Strict DMD-side upper bound on `LiveStream` duration: configured `video_max_duration_s`
    /// (clamped to `2..=120s`) plus `4s` per enabled scrolling panorama.
    pub fn configured_max_stream_duration(&self) -> Duration {
        let mut pan_s = 0u64;
        if self.cfg.jacket_panorama {
            pan_s += 4;
        }
        if self.cfg.marquee_panorama {
            pan_s += 4;
        }
        pan_s += (self.cfg.snapshot_panorama_count.min(5) as u64) * 4;
        let vid_s = if self.cfg.show_video {
            (self.cfg.video_max_duration_s.clamp(2, 120)) as u64
        } else {
            0
        };
        Duration::from_secs((pan_s + vid_s).max(2))
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
        format_rpi_dmd_client_id(self.domain.as_api_slug(), self.width, self.height)
    }

    /// Spawn the background worker thread (`mqtt_udp` real-time client or `http_poll` fallback).
    pub fn spawn_background_sync(&self) {
        if !self.cfg.enabled || self.cfg.server_url.is_empty() {
            return;
        }
        let my_gen = self.config_gen.load(Ordering::SeqCst);
        let cur_worker_gen = self.worker_gen.load(Ordering::SeqCst);
        if cur_worker_gen == my_gen {
            return;
        }
        if self
            .worker_gen
            .compare_exchange(cur_worker_gen, my_gen, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        self.worker_busy.store(true, Ordering::SeqCst);

        let domain = self.domain;
        let cfg = self.cfg.clone();
        let lang = cfg.effective_lang(&self.system_lang).to_string();
        let width = self.width;
        let height = self.height;
        let shared = Arc::clone(&self.shared);
        let busy = Arc::clone(&self.worker_busy);
        let worker_gen_arc = Arc::clone(&self.worker_gen);
        let config_gen_arc = Arc::clone(&self.config_gen);
        let global_host = self.global_mqtt_host.clone();
        let global_port = self.global_mqtt_port;

        std::thread::spawn(move || {
            let _guard = BusyResetGuard(busy, worker_gen_arc, my_gen);
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
                Self::run_http_poll_worker(
                    domain,
                    cfg,
                    lang,
                    width,
                    height,
                    shared,
                    config_gen_arc,
                    my_gen,
                );
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
        let Some((mut mqtt_host, mut mqtt_port)) = resolve_mqtt_broker_endpoint(
            &cfg,
            if global_host.is_empty() {
                None
            } else {
                Some((global_host.as_str(), global_port))
            },
        ) else {
            return;
        };
        let base_url = resolve_showcase_server_url(&cfg.server_url);
        if cfg.use_showcase_broker {
            if let Some((h, p)) = parse_host_and_port(&base_url, 8890) {
                if let Some(details) = probe_showcase_health_details(&h, p) {
                    if details.mqtt_port > 0 {
                        mqtt_port = details.mqtt_port;
                    }
                    if !details.mqtt_host.is_empty()
                        && details.mqtt_host != "localhost"
                        && details.mqtt_host != "127.0.0.1"
                        && details.mqtt_host != "mosquitto"
                        && !details.mqtt_host.starts_with("172.")
                    {
                        mqtt_host = resolve_showcase_host_with_port(&details.mqtt_host, mqtt_port);
                    } else {
                        mqtt_host = h;
                    }
                }
            }
        }

        let slug = domain.as_api_slug();
        let dmd_id = format_rpi_dmd_client_id(slug, width, height);
        let local_ip = probe_local_lan_ip(&mqtt_host, mqtt_port).unwrap_or_default();
        let udp_socket = match UdpSocket::bind("0.0.0.0:0") {
            Ok(s) => {
                let _ = s.set_nonblocking(true);
                s
            }
            Err(_) => return,
        };
        let local_udp_port = udp_socket.local_addr().map(|a| a.port()).unwrap_or(5568);
        let hello_pkt = format!("DMDH|{}|{}", dmd_id, local_ip);
        let _ = udp_socket.send_to(hello_pkt.as_bytes(), (mqtt_host.as_str(), 5569));

        let conn_seq = MQTT_CONN_SEQ.fetch_add(1, Ordering::Relaxed) % 10_000;
        let pump_mqtt_host = mqtt_host.clone();
        let pump_dmd_id = dmd_id.clone();
        let mut mqtt_opts =
            MqttOptions::new(format!("{}_{}", dmd_id, conn_seq), mqtt_host, mqtt_port);
        mqtt_opts.set_keep_alive(Duration::from_secs(15));
        // Raise rumqttc's default 10 KB incoming packet limit to 64 KB so 256x64 / 128x64
        // `ready` payloads (`48x64` Base64 poster = 8,192 chars + full localized synopsis)
        // never trigger `IncomingPacketTooLarge` disconnects.
        mqtt_opts.set_max_packet_size(64 * 1024, 64 * 1024);
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

        let http_client = crate::core::memory::stream_http_client();
        let preload_in_flight = Arc::new(AtomicBool::new(false));
        let pending_preload: Arc<Mutex<Option<(String, String)>>> = Arc::new(Mutex::new(None));
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
                        if let Ok(mut guard) = pump_shared.lock() {
                            if let Some(item_id) = guard.requested_stop_stream_id.take() {
                                let ev = serde_json::json!({
                                    "action": "stop_stream",
                                    "item_id": item_id,
                                    "client_ip": pump_local_ip,
                                    "udp_ip": pump_local_ip,
                                    "udp_port": local_udp_port,
                                });
                                if let Ok(b) = serde_json::to_vec(&ev) {
                                    let _ = pump_mqtt.publish(
                                        &pump_events_topic,
                                        QoS::AtLeastOnce,
                                        false,
                                        b,
                                    );
                                }
                            }
                        }
                        // Send a clean MQTT 3.1.1 DISCONNECT (`0xE0 0x00`) without firing
                        // false LWT `online:false` on normal rotation switches.
                        let _ = pump_mqtt.disconnect();
                        break;
                    }
                    // 1. Drain any incoming UDP `DMDF` packets in non-blocking mode
                    let mut got_dmdf_pkt = false;
                    let mut latest_frame: Option<(u16, RgbImage)> = None;
                    while let Ok((len, _)) = udp_socket.recv_from(&mut udp_buf) {
                        if decode_dmdf_packet(&udp_buf[..len]).is_some() {
                            got_dmdf_pkt = true;
                        }
                        if let Some(frame_pair) = assembler.push_packet(&udp_buf[..len]) {
                            latest_frame = Some(frame_pair);
                        }
                    }
                    if got_dmdf_pkt {
                        let mut guard = pump_shared.lock().unwrap();
                        guard.last_udp_packet_at = Some(Instant::now());
                        guard.server_online = true;
                        if let Some((seq, img)) = latest_frame {
                            guard.live_udp_frame = Some(img);
                            guard.live_udp_seq = seq;
                        }
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
                        let hello_pkt = format!("DMDH|{}|{}", pump_dmd_id, pump_local_ip);
                        let _ = udp_socket
                            .send_to(hello_pkt.as_bytes(), (pump_mqtt_host.as_str(), 5569));
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
                    if !got_dmdf_pkt {
                        std::thread::sleep(Duration::from_millis(3));
                    }
                }
            });
        }

        let mut iter = connection.iter();
        let mut disconnect_drain_since: Option<Instant> = None;
        loop {
            if config_gen_arc.load(Ordering::SeqCst) != my_gen {
                let t0 = *disconnect_drain_since.get_or_insert_with(Instant::now);
                if t0.elapsed() > Duration::from_millis(350) {
                    break;
                }
            }
            match iter.next() {
                Some(Ok(MqttEvent::Outgoing(Outgoing::Disconnect))) => {
                    break;
                }
                Some(Ok(MqttEvent::Incoming(_)))
                    if config_gen_arc.load(Ordering::SeqCst) != my_gen => {}
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
                                guard.ready_state_received = true;
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
                                                Some(http_client),
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
                                        let stream_path =
                                            if !card.assets.unified_stream_rgb565.is_empty() {
                                                card.assets.unified_stream_rgb565.clone()
                                            } else {
                                                format!(
                                                    "/api/v1/{}/stream/{}.rgb565.stream",
                                                    slug, item_id
                                                )
                                            };
                                        let preload_item_id = item_id.clone();
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
                                            http_stream: None,
                                            http_frame_delay: Duration::from_millis(66),
                                        };
                                        {
                                            let mut guard = shared.lock().unwrap();
                                            guard.server_online = true;
                                            guard.mqtt_connected_once = true;
                                            guard
                                                .mqtt_connected_at
                                                .get_or_insert_with(Instant::now);
                                            guard.readiness = BatchReadiness::Ready;
                                            guard.ready_state_received = true;
                                            guard.ready_item = Some(prepared);
                                            guard.generation = my_gen;
                                        }

                                        // Preload the unified stream (`DMDZ` v2 lossless True-RGB565 with automatic `DMDS`
                                        // fallback) over HTTP TCP in a background thread while the Card is displaying on the DMD
                                        // (or while N+1 waits in `ready_item`).
                                        // Queues `pending_preload` so if N+1 `ready: true` arrives while N is still
                                        // downloading, N+1 is automatically preloaded immediately after N completes.
                                        if has_stream && !base_url.is_empty() {
                                            if let Ok(mut q) = pending_preload.lock() {
                                                *q = Some((
                                                    preload_item_id.clone(),
                                                    stream_path.clone(),
                                                ));
                                            }
                                            if preload_in_flight
                                                .compare_exchange(
                                                    false,
                                                    true,
                                                    Ordering::AcqRel,
                                                    Ordering::Acquire,
                                                )
                                                .is_ok()
                                            {
                                                let preload_shared = Arc::clone(&shared);
                                                let preload_gen_arc = Arc::clone(&config_gen_arc);
                                                let in_flight_arc = Arc::clone(&preload_in_flight);
                                                let pending_arc = Arc::clone(&pending_preload);
                                                let p_slug = slug.to_string();
                                                let p_base_url = base_url.clone();
                                                let p_dmd_id = dmd_id.clone();
                                                std::thread::spawn(move || {
                                                    loop {
                                                        let next_job = pending_arc
                                                            .lock()
                                                            .ok()
                                                            .and_then(|mut q| q.take());
                                                        let Some((p_item_id, p_stream_path)) =
                                                            next_job
                                                        else {
                                                            in_flight_arc
                                                                .store(false, Ordering::Release);
                                                            let has_late_job = pending_arc
                                                                .lock()
                                                                .ok()
                                                                .is_some_and(|q| q.is_some());
                                                            if has_late_job
                                                                && in_flight_arc
                                                                    .compare_exchange(
                                                                        false,
                                                                        true,
                                                                        Ordering::AcqRel,
                                                                        Ordering::Acquire,
                                                                    )
                                                                    .is_ok()
                                                            {
                                                                continue;
                                                            }
                                                            break;
                                                        };
                                                        if preload_gen_arc.load(Ordering::SeqCst)
                                                            != my_gen
                                                        {
                                                            in_flight_arc
                                                                .store(false, Ordering::Release);
                                                            break;
                                                        }
                                                        if let Some(stream_arc) =
                                                            crate::core::dmd_stream::fetch_showcase_stream(
                                                                &p_base_url,
                                                                &p_stream_path,
                                                                Some(&p_dmd_id),
                                                                false,
                                                                width,
                                                                height,
                                                                Some(Duration::from_secs(25)),
                                                            )
                                                        {
                                                            let f_cnt = stream_arc.frame_count;
                                                            let delay = stream_arc.frame_delay;
                                                            if preload_gen_arc
                                                                .load(Ordering::SeqCst)
                                                                == my_gen
                                                            {
                                                                let mut guard =
                                                                    preload_shared.lock().unwrap();
                                                                let mut attached_to_ready = false;
                                                                if let Some(ref mut rdy) =
                                                                    guard.ready_item
                                                                {
                                                                    if rdy.id == p_item_id {
                                                                        rdy.http_stream = Some(
                                                                            Arc::clone(&stream_arc),
                                                                        );
                                                                        rdy.http_frame_delay =
                                                                            delay;
                                                                        attached_to_ready = true;
                                                                    }
                                                                }
                                                                if !attached_to_ready {
                                                                    guard.preloaded_streams.clear();
                                                                    guard
                                                                        .preloaded_streams
                                                                        .push_back((
                                                                            p_item_id.clone(),
                                                                            stream_arc,
                                                                        ));
                                                                }
                                                                info!(
                                                                    "[Showcase:{}] Preloaded HTTP stream for '{}' ({} frames @ {:?}/frame)",
                                                                    p_slug, p_item_id, f_cnt, delay
                                                                );
                                                            }
                                                        }
                                                    }
                                                    crate::core::memory::trim_heap();
                                                });
                                            }
                                        }
                                    } else {
                                        let mut guard = shared.lock().unwrap();
                                        guard.server_online = true;
                                        guard.mqtt_connected_once = true;
                                        guard.mqtt_connected_at.get_or_insert_with(Instant::now);
                                        guard.ready_state_received = true;
                                        if guard.ready_item.is_none() {
                                            guard.readiness = BatchReadiness::Warming;
                                        }
                                    }
                                } else {
                                    let mut guard = shared.lock().unwrap();
                                    guard.server_online = true;
                                    guard.mqtt_connected_once = true;
                                    guard.mqtt_connected_at.get_or_insert_with(Instant::now);
                                    guard.ready_state_received = true;
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
                                let eof_id = v
                                    .get("item_id")
                                    .and_then(|s| s.as_str())
                                    .map(|s| s.to_string());
                                let mut guard = shared.lock().unwrap();
                                guard.stream_eof = true;
                                guard.stream_eof_item_id = eof_id;
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
                    Self::run_http_poll_worker(
                        domain,
                        cfg,
                        lang,
                        width,
                        height,
                        shared,
                        config_gen_arc,
                        my_gen,
                    );
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
        config_gen_arc: Arc<AtomicU64>,
        my_gen: u64,
    ) {
        let client = crate::core::memory::api_http_client();

        let resolved_base = resolve_showcase_server_url(&cfg.server_url);
        let base = resolved_base.trim_end_matches('/');
        let slug = domain.as_api_slug();
        let dmd_id = format_rpi_dmd_client_id(slug, width, height);
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
            if config_gen_arc.load(Ordering::SeqCst) == my_gen {
                let mut guard = shared.lock().unwrap();
                guard.server_online = false;
                guard.readiness = BatchReadiness::Offline;
                guard.clear_media_immediately();
                guard.ready_state_received = true;
            }
            return;
        };
        if !cfg_resp.status().is_success() {
            if config_gen_arc.load(Ordering::SeqCst) == my_gen {
                let mut guard = shared.lock().unwrap();
                guard.server_online = false;
                guard.readiness = BatchReadiness::Offline;
                guard.clear_media_immediately();
                guard.ready_state_received = true;
            }
            return;
        }

        // 2. Poll next unconsumed item (`GET /api/v1/{domain}/next?client_id=...`)
        let next_url = format!("{}/api/v1/{}/next?client_id={}", base, slug, dmd_id);
        let Ok(next_resp) = client.get(&next_url).send() else {
            if config_gen_arc.load(Ordering::SeqCst) == my_gen {
                let mut guard = shared.lock().unwrap();
                guard.server_online = false;
                guard.readiness = BatchReadiness::Offline;
                guard.clear_media_immediately();
                guard.ready_state_received = true;
            }
            return;
        };
        if !next_resp.status().is_success() {
            if config_gen_arc.load(Ordering::SeqCst) == my_gen {
                let mut guard = shared.lock().unwrap();
                guard.ready_state_received = true;
            }
            return;
        }
        let Ok(next_data) = next_resp.json::<ApiNextResponse>() else {
            if config_gen_arc.load(Ordering::SeqCst) == my_gen {
                let mut guard = shared.lock().unwrap();
                guard.ready_state_received = true;
            }
            return;
        };

        if !next_data.status.eq_ignore_ascii_case("ready") || next_data.item.is_none() {
            let mut guard = shared.lock().unwrap();
            guard.server_online = true;
            guard.readiness = BatchReadiness::Warming;
            guard.ready_state_received = true;
            return;
        }

        let api_item = next_data.item.unwrap();
        let item_id = api_item.canonical_id().to_string();

        let poster = if cfg.show_poster {
            api_item.resolve_poster_image(slug, base, Some(client), default_pw, default_ph)
        } else {
            None
        };

        // Fetch unified stream (`DMDZ` v2 lossless True-RGB565 with automatic `DMDS` fallback)
        let mut http_stream = None;
        let mut http_frame_delay = Duration::from_millis((1000 / api_item.video_fps.max(1)) as u64);
        let stream_path = if !api_item.assets.unified_stream_rgb565.is_empty() {
            api_item.assets.unified_stream_rgb565.clone()
        } else {
            format!("/api/v1/{}/stream/{}.rgb565.stream", slug, item_id)
        };
        if (cfg.any_panorama_requested() || cfg.show_video) && !stream_path.is_empty() {
            if let Some(stream_arc) = crate::core::dmd_stream::fetch_showcase_stream(
                base,
                &stream_path,
                Some(&dmd_id),
                true,
                width,
                height,
                Some(Duration::from_secs(25)),
            ) {
                http_frame_delay = stream_arc.frame_delay;
                http_stream = Some(stream_arc);
            }
        }

        let has_stream = http_stream.is_some();
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
            http_stream,
            http_frame_delay,
        };

        if config_gen_arc.load(Ordering::SeqCst) != my_gen {
            return;
        }
        let mut guard = shared.lock().unwrap();
        guard.server_online = true;
        guard.mqtt_connected_once = true;
        guard.readiness = BatchReadiness::Ready;
        guard.ready_state_received = true;
        guard.ready_item = Some(prepared);
        guard.generation = my_gen;
        drop(guard);
        crate::core::memory::trim_heap();
    }

    pub fn initialize(&mut self, context: &mut EngineContext, config: &dyn EngineConfig) {
        self.width = context.surface.width().max(32);
        self.height = context.surface.height().max(32);
        let showcase_settings = {
            let settings = context.config.settings.read();
            self.system_lang = settings.system.lang.clone();
            self.global_mqtt_host = settings.mqtt.broker.clone();
            self.global_mqtt_port = settings.mqtt.port;
            self.rotation_slot_count = settings.rotation.len();
            settings.showcase.clone()
        };
        self.unified_showcase = showcase_settings.clone();
        self.cfg = ShowcaseConfig::from_engine_config(config, self.domain);
        self.cfg.apply_unified_settings(&showcase_settings);
        self.active_item = None;
        self.items_played_in_turn = 0;
        self.config_gen.fetch_add(1, Ordering::SeqCst);
        self.spawn_background_sync();
    }

    pub fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        let mut new_cfg = ShowcaseConfig::from_engine_config(config, self.domain);
        new_cfg.apply_unified_settings(&self.unified_showcase);
        if new_cfg != self.cfg {
            let now = Instant::now();
            self.cfg = new_cfg;
            self.active_item = None;
            self.phase = PlaybackPhase::Card;
            self.phase_started_at = now;
            self.last_retry_poll = now;
            self.http_frame_idx = 0;
            self.http_frame_elapsed = Duration::ZERO;
            self.stream_player.reset();
            self.skip_turn_immediately = false;
            self.turn_completed = false;
            self.consecutive_warm_skips = 0;
            self.dirty = true;
            {
                let mut guard = self.shared.lock().unwrap();
                guard.readiness = BatchReadiness::Offline;
                guard.clear_media_immediately();
            }
            self.config_gen.fetch_add(1, Ordering::SeqCst);
            self.spawn_background_sync();
        }
    }

    pub fn on_display_geometry_changed(&mut self, width: u32, height: u32) {
        let new_w = width.max(32);
        let new_h = height.max(32);
        if new_w != self.width || new_h != self.height {
            let now = Instant::now();
            self.width = new_w;
            self.height = new_h;
            let stopped_id = self.active_item.take().map(|it| it.id);
            self.phase = PlaybackPhase::Card;
            self.phase_started_at = now;
            self.last_retry_poll = now;
            self.http_frame_idx = 0;
            self.http_frame_elapsed = Duration::ZERO;
            self.stream_player.reset();
            self.turn_evaluated = false;
            self.skip_turn_immediately = false;
            self.turn_completed = false;
            self.dirty = true;
            if let Ok(mut guard) = self.shared.lock() {
                if let Some(id) = stopped_id {
                    guard.requested_stop_stream_id = Some(id);
                }
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
        self.items_played_in_turn = 0;
        self.http_frame_idx = 0;
        self.http_frame_elapsed = Duration::ZERO;
        self.stream_player.start(now);
        self.phase = PlaybackPhase::Card;
        self.phase_started_at = now;
        self.last_retry_poll = now;
        self.last_tick = now;
        self.turn_evaluated = false;
        self.skip_turn_immediately = false;
        self.turn_completed = false;
        self.dirty = true;
        self.spawn_background_sync();
    }

    pub fn deactivate(&mut self) {
        // Strict Consume-Once & Zero-Cache: drop all active/ready/preloaded media and stop
        // background worker threads when leaving the rotation slot.
        let stopped_id = self.active_item.take().map(|it| it.id);
        if let Ok(mut guard) = self.shared.lock() {
            guard.clear_media_immediately();
            guard.mqtt_connected_at = None;
            if let Some(id) = stopped_id {
                guard.requested_stop_stream_id = Some(id);
            }
        }
        self.config_gen.fetch_add(1, Ordering::SeqCst);
        self.stream_player.reset();
        self.skip_turn_immediately = false;
        self.turn_evaluated = false;
        self.turn_completed = false;
        crate::core::memory::trim_heap();
    }

    /// Evaluate turn start readiness: consume the prepared item (via `.take()`) so it can never be
    /// replayed on subsequent turns without the server preparing a brand-new item.
    pub fn evaluate_turn_start(&mut self, rotation_slot_count: usize) {
        self.turn_evaluated = true;
        if !self.cfg.enabled {
            self.active_item = None;
            if rotation_slot_count > 1 {
                self.skip_turn_immediately = true;
                self.turn_completed = true;
            } else {
                self.skip_turn_immediately = false;
                self.turn_completed = false;
                self.phase = PlaybackPhase::Card;
                self.phase_started_at = Instant::now();
            }
            return;
        }
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
            self.consecutive_warm_skips = 0;
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
            if self.items_played_in_turn == 0 {
                self.consecutive_warm_skips = self.consecutive_warm_skips.saturating_add(1);
            } else {
                self.consecutive_warm_skips = 0;
            }
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
        self.stream_player.start(now);
        {
            let mut guard = self.shared.lock().unwrap();
            guard.stream_eof = false;
            guard.stream_eof_item_id = None;
            guard.live_udp_frame = None;
            guard.last_udp_packet_at = None;
            let preloaded = guard.take_preloaded_stream(item_id);
            if let Some(ref mut active) = self.active_item {
                if active.http_stream.is_none() {
                    if let Some(stream_arc) = preloaded {
                        active.http_frame_delay = stream_arc.frame_delay;
                        active.http_stream = Some(stream_arc);
                    }
                }
            }
            let has_preloaded = self
                .active_item
                .as_ref()
                .is_some_and(|it| it.http_stream_frame_count() > 0);
            if !has_preloaded {
                guard.requested_start_stream_id = Some(item_id.to_string());
            } else {
                info!(
                    "[Showcase:{}] Starting LiveStream phase for '{}' using preloaded stream ({} frames)",
                    self.domain.as_api_slug(),
                    item_id,
                    self.active_item.as_ref().map(|it| it.http_stream_frame_count()).unwrap_or(0)
                );
            }
        }
        self.dirty = true;
    }

    fn finish_current_turn(&mut self) {
        let stopped_id = self.active_item.take().map(|it| it.id);
        self.stream_player.reset();
        {
            let mut guard = self.shared.lock().unwrap();
            guard.live_udp_frame = None;
            guard.last_udp_packet_at = None;
            if let Some(id) = stopped_id {
                let _ = guard.take_preloaded_stream(&id);
                guard.requested_stop_stream_id = Some(id);
            }
        }
        crate::core::memory::trim_heap();
        self.items_played_in_turn += 1;
        self.spawn_background_sync();

        if self.rotation_slot_count > 1 && self.items_played_in_turn >= self.rotation_budget {
            self.turn_completed = true;
            self.dirty = true;
        } else {
            // Strict rule: if `items_played_in_turn < rotation_budget` (or solo mode), check if the
            // next item (`Slot N+1`) is ALREADY ready; if NOT ready and `rotation_slot_count > 1`,
            // `evaluate_turn_start` immediately yields the turn (`skip_turn_immediately = true`)
            // without ever waiting on "WAITING FOR ...".
            self.evaluate_turn_start(self.rotation_slot_count);
            self.dirty = true;
        }
    }

    pub fn update(&mut self, context: &mut EngineContext) {
        let now = Instant::now();
        let _dt = now.saturating_duration_since(self.last_tick);
        self.last_tick = now;

        let (unified_showcase, rot_len) = {
            let settings = context.config.settings.read();
            (settings.showcase.clone(), settings.rotation.len())
        };
        self.rotation_slot_count = rot_len;
        if unified_showcase != self.unified_showcase {
            self.unified_showcase = unified_showcase.clone();
            self.cfg.apply_unified_settings(&unified_showcase);
            self.config_gen.fetch_add(1, Ordering::SeqCst);
            self.spawn_background_sync();
        }

        let new_w = context.surface.width().max(32);
        let new_h = context.surface.height().max(32);
        if new_w != self.width || new_h != self.height {
            self.on_display_geometry_changed(new_w, new_h);
        }

        if !self.turn_evaluated {
            let (has_ready, ready_state_received, connected_at) = {
                let guard = self.shared.lock().unwrap();
                (
                    guard.ready_item.is_some(),
                    guard.ready_state_received,
                    guard.mqtt_connected_at,
                )
            };
            // Turn-start readiness gate:
            // - Once `ready_state_received` is true and `!has_ready` (server answered `ready: false`
            //   or `Offline`), NEVER linger on "WAITING FOR ...": yield immediately (`0ms`) during
            //   normal rotation, or show a brief `350ms` visual step only when all Showcase slots
            //   in rotation are still warming up at cold boot (`consecutive_warm_skips > 0`).
            // - While `!ready_state_received` (awaiting LAN TCP + retained MQTT `ready` packet),
            //   allow at most `250ms` after MQTT connect (or `800ms` TCP connect timeout).
            let in_boot_connect_grace = if has_ready {
                false
            } else if ready_state_received {
                rot_len > 1
                    && self.consecutive_warm_skips > 0
                    && now.saturating_duration_since(self.phase_started_at)
                        < Duration::from_millis(350)
            } else {
                match connected_at {
                    None => {
                        self.worker_busy.load(Ordering::SeqCst)
                            && now.saturating_duration_since(self.phase_started_at)
                                < Duration::from_millis(800)
                    }
                    Some(t) => now.saturating_duration_since(t) < Duration::from_millis(250),
                }
            };
            if !has_ready && !self.cfg.server_url.is_empty() && in_boot_connect_grace {
                self.spawn_background_sync();
                self.dirty = true;
                return;
            }
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
                if rot_len > 1 {
                    self.skip_turn_immediately = true;
                    self.turn_completed = true;
                }
                self.dirty = true;
                return;
            }

            // If we have no active item after `turn_evaluated`:
            // - Consume `ready_item` immediately if ready.
            // - Otherwise, if `rot_len > 1`, NEVER wait on "WAITING FOR ..." — yield immediately!
            if self.active_item.is_none() {
                if let Some(item) = guard.ready_item.take() {
                    guard.readiness = BatchReadiness::Warming;
                    guard.requested_start_card_id = Some(item.id.clone());
                    guard.stream_eof = false;
                    guard.live_udp_frame = None;
                    guard.last_udp_packet_at = None;
                    self.consecutive_warm_skips = 0;
                    self.active_item = Some(item);
                    self.phase = PlaybackPhase::Card;
                    self.phase_started_at = now;
                    self.dirty = true;
                } else if rot_len > 1 {
                    drop(guard);
                    self.skip_turn_immediately = true;
                    self.turn_completed = true;
                    self.dirty = true;
                    return;
                }
            }
        }

        let Some((item_id, has_stream, stream_duration_s, mut http_frames_len, card_target_dur)) =
            self.active_item.as_ref().map(|it| {
                (
                    it.id.clone(),
                    it.has_stream,
                    it.stream_duration_s,
                    it.http_stream_frame_count(),
                    self.organic_card_duration(it),
                )
            })
        else {
            if now.saturating_duration_since(self.last_retry_poll) >= Duration::from_millis(2500) {
                self.last_retry_poll = now;
                self.spawn_background_sync();
            }
            self.dirty = true;
            return;
        };

        let mut phase_elapsed = now.saturating_duration_since(self.phase_started_at);

        match self.phase {
            PlaybackPhase::Card => {
                self.dirty = true;
                if phase_elapsed >= card_target_dur {
                    if has_stream {
                        self.start_live_stream_phase(now, &item_id);
                    } else {
                        self.finish_current_turn();
                    }
                }
            }
            PlaybackPhase::LiveStream => {
                // Attach background-preloaded HTTP stream if it completed right after LiveStream started
                if http_frames_len == 0 {
                    let mut guard = self.shared.lock().unwrap();
                    if let Some(stream_arc) = guard.take_preloaded_stream(&item_id) {
                        if let Some(ref mut active) = self.active_item {
                            http_frames_len = stream_arc.frame_count;
                            active.http_frame_delay = stream_arc.frame_delay;
                            active.http_stream = Some(stream_arc);
                            self.http_frame_idx = 0;
                            self.http_frame_elapsed = Duration::ZERO;
                            self.stream_player.start(now);
                            self.phase_started_at = now;
                            phase_elapsed = Duration::ZERO;
                        }
                    }
                }

                let max_cfg_dur = self.configured_max_stream_duration();

                // 1. Shared `ShowcaseStreamPlayer` (`DMDZ` v1/v2 & `DMDS`) playback:
                //    Indexes frames strictly by monotonic wall-clock `phase_elapsed` (never by
                //    drifting `Duration::ZERO` resets) and enforces the DMD's `video_max_duration_s` cap.
                if http_frames_len > 0 {
                    let stream_opt = self
                        .active_item
                        .as_ref()
                        .and_then(|it| it.http_stream.as_ref())
                        .cloned();
                    if let Some(stream) = stream_opt {
                        let finished = self.stream_player.advance_at_elapsed(
                            &stream,
                            phase_elapsed,
                            false,
                            Some(max_cfg_dur),
                        );
                        self.http_frame_idx = self.stream_player.current_frame_index();
                        self.dirty = self.stream_player.has_new_frame(&stream);
                        if finished {
                            self.finish_current_turn();
                        }
                        return;
                    }
                }

                self.dirty = true;

                // 2. Real-time UDP stream (`mqtt_udp`) with `stream_eof` + 1500ms packet watchdog
                let (stream_eof, last_pkt) = {
                    let guard = self.shared.lock().unwrap();
                    let matching_eof = guard.stream_eof
                        && guard
                            .stream_eof_item_id
                            .as_deref()
                            .is_none_or(|eof_id| eof_id == item_id);
                    (matching_eof, guard.last_udp_packet_at)
                };
                let max_stream_dur = Duration::from_secs_f32((stream_duration_s + 2.0).max(4.0))
                    .min(max_cfg_dur + Duration::from_secs(2));
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

        let eff_lang = self.cfg.effective_lang(&self.system_lang).to_string();
        if self.active_item.is_none() {
            let effective_url = if self.cfg.enabled {
                self.cfg.server_url.as_str()
            } else {
                ""
            };
            render_waiting_pixel_art(
                self.domain,
                matrix,
                w,
                h,
                phase_ms,
                effective_url,
                &eff_lang,
            );
            self.dirty = false;
            return;
        }

        match self.phase {
            PlaybackPhase::Card => {
                if let Some(ref item) = self.active_item {
                    self.render_card_adaptive(matrix, item, w, h, phase_ms);
                }
            }
            PlaybackPhase::LiveStream => {
                let stream_opt = self
                    .active_item
                    .as_ref()
                    .and_then(|it| it.http_stream.as_ref())
                    .cloned();
                if let Some(stream) = stream_opt {
                    if !stream.is_empty() {
                        self.stream_player.render_to_surface(&stream, matrix);
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
                            &eff_lang,
                        );
                    }
                }
            }
        }
        self.dirty = false;
    }
}

struct BusyResetGuard(Arc<AtomicBool>, Arc<AtomicU64>, u64);
impl Drop for BusyResetGuard {
    fn drop(&mut self) {
        if self
            .1
            .compare_exchange(self.2, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.0.store(false, Ordering::SeqCst);
        }
    }
}

pub fn build_common_showcase_fields(
    default_video_mode: &'static str,
) -> Vec<crate::core::engine_contract::ConfigField> {
    use crate::core::engine_contract::{ConfigField, ConfigOption, ConfigType, ValidationPolicy};
    vec![
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
            max_val: Some("120"),
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
            max_val: Some("60"),
            step: Some("1"),
            validation_policy: ValidationPolicy::Clamp,
            ..Default::default()
        },
        ConfigField {
            id: "video_mode",
            field_type: ConfigType::Options,
            label: "Video Framing Mode",
            description: "Video framing strategy applied by dmd-gif-converter on the server",
            default_value: default_video_mode,
            options: Some(vec![
                ConfigOption { label: "Auto (Let Me Handle It AI)", value: "auto_action" },
                ConfigOption { label: "Panorama (Top-Bottom Scan)", value: "panorama" },
                ConfigOption { label: "Static Center Crop", value: "center_crop" },
            ]),
            validation_policy: ValidationPolicy::FallbackDefault,
            ..Default::default()
        },
        ConfigField {
            id: "color_boost",
            field_type: ConfigType::Options,
            label: "LED Color Boost",
            description: "Colorimetry profile for HUB75 LED panels (Auto AI or fixed domain preset)",
            default_value: "auto",
            options: Some(vec![
                ConfigOption { label: "Auto (Let Me Handle It AI)", value: "auto" },
                ConfigOption { label: "Pixel Art (Crisp Retro)", value: "pixel_art" },
                ConfigOption { label: "Anime (Vibrant Cel-Shaded)", value: "anime" },
                ConfigOption { label: "Cinema (Balanced Contrast & Shadows)", value: "cinema" },
            ]),
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

        let compact = parse_rgb565_stream_compact(&stream).expect("valid compact DMDS stream");
        assert_eq!(compact.frame_count, 2);
        assert_eq!(compact.frame_delay, Duration::from_millis(66));
        assert_eq!(compact.payload_byte_len(), 16);
        let mut surface = MockSurface::new(2, 2);
        compact.draw_frame_to_surface(0, &mut surface);
        assert!(surface.canvas.get_pixel(0, 0).0[0] >= 248);
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
                http_stream: None,
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
        assert!(runner.shared.lock().unwrap().ready_item.is_none());
        assert!(runner.shared.lock().unwrap().preloaded_streams.is_empty());
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
                    "en",
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

    #[test]
    fn test_deactivate_purges_all_cached_streams_and_stops_workers() {
        let mut runner = MediaShowcaseRunner::new(ShowcaseDomain::Movies, 128, 64);
        let stream = Arc::new(PreloadedRgb565Stream::from_raw_rgb565_le(
            128,
            64,
            10,
            Duration::from_millis(33),
            vec![0u8; 128 * 64 * 2 * 10],
        ));
        {
            let mut guard = runner.shared.lock().unwrap();
            let item = PreparedShowcaseItem {
                id: "m1".to_string(),
                title: "Blade Runner".to_string(),
                year: "1982".to_string(),
                badge: "SCIFI".to_string(),
                badge_rgb: (0, 210, 255),
                subtitle: "Sci-Fi".to_string(),
                rating: "8.1".to_string(),
                synopsis: "A blade runner must pursue and terminate four replicants.".to_string(),
                poster: None,
                has_stream: true,
                stream_duration_s: 0.33,
                stream_fps: 30,
                http_stream: Some(stream.clone()),
                http_frame_delay: Duration::from_millis(33),
            };
            guard.ready_item = Some(item.clone());
            guard
                .preloaded_streams
                .push_back(("m1".to_string(), stream.clone()));
            runner.active_item = Some(item);
        }

        let gen_before = runner.config_gen.load(Ordering::SeqCst);
        runner.deactivate();
        let gen_after = runner.config_gen.load(Ordering::SeqCst);

        assert!(gen_after > gen_before);
        assert!(runner.active_item.is_none());
        let guard = runner.shared.lock().unwrap();
        assert!(guard.ready_item.is_none());
        assert!(guard.preloaded_streams.is_empty());
        assert_eq!(Arc::strong_count(&stream), 1);
    }

    #[test]
    fn test_docker_lan_autodiscovery_fallback_for_local_hostname() {
        register_showcase_candidate_ip("::ffff:192.168.1.22");
        register_showcase_candidate_ip("127.0.0.1"); // loopback ignored in candidate list
        let list = CANDIDATE_SHOWCASE_IPS
            .get()
            .unwrap()
            .lock()
            .unwrap()
            .clone();
        assert_eq!(list.first().map(String::as_str), Some("192.168.1.22"));

        let body =
            r#"{"status":"ok","version":"3.0.0","converter_version":"8.0.0","mqtt_port":1883}"#;
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let parsed =
            parse_showcase_health_http_response(resp.as_bytes()).expect("valid showcase health");
        assert_eq!(parsed, ("3.0.0".to_string(), 1883));

        // Unrelated HTTP server without Showcase fields must be rejected
        let unrelated =
            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"status\":\"ok\"}";
        assert!(parse_showcase_health_http_response(unrelated).is_none());
    }

    #[test]
    fn test_multi_item_budget_immediately_yields_when_next_item_not_ready() {
        let mut runner = MediaShowcaseRunner::new(ShowcaseDomain::Movies, 128, 32);
        runner.cfg.server_url.clear();
        runner.rotation_slot_count = 4;
        runner.set_rotation_budget(30);
        {
            let mut guard = runner.shared.lock().unwrap();
            guard.readiness = BatchReadiness::Ready;
            guard.ready_state_received = true;
            guard.server_online = true;
            guard.ready_item = Some(PreparedShowcaseItem {
                id: "m1".to_string(),
                title: "The Matrix".to_string(),
                year: "1999".to_string(),
                badge: "SCIFI".to_string(),
                badge_rgb: (0, 229, 255),
                subtitle: "Sci-Fi".to_string(),
                rating: "8.7".to_string(),
                synopsis: "Short.".to_string(),
                poster: None,
                has_stream: false,
                stream_duration_s: 4.0,
                stream_fps: 30,
                http_stream: None,
                http_frame_delay: Duration::from_millis(33),
            });
        }

        runner.activate();
        runner.evaluate_turn_start(4);
        assert!(runner.active_item.is_some());
        assert!(!runner.is_finished());

        // Movie #1 finishes, Movie #2 is NOT ready (`ready_item == None`):
        // MUST immediately yield the turn to the next rotation slot without waiting!
        runner.finish_current_turn();
        assert_eq!(runner.items_played_in_turn, 1);
        assert!(runner.skip_turn_immediately);
        assert!(runner.turn_completed);
        assert!(runner.is_finished());
        assert_eq!(runner.consecutive_warm_skips, 0);
    }

    #[test]
    fn test_live_rotation_budget_update_to_1_stops_after_current_item() {
        let mut runner = MediaShowcaseRunner::new(ShowcaseDomain::Movies, 128, 32);
        runner.cfg.server_url.clear();
        runner.rotation_slot_count = 4;
        runner.set_rotation_budget(30);
        let mk_item = |id: &str| PreparedShowcaseItem {
            id: id.to_string(),
            title: id.to_string(),
            year: "1999".to_string(),
            badge: "SCIFI".to_string(),
            badge_rgb: (0, 229, 255),
            subtitle: "Sci-Fi".to_string(),
            rating: "8.7".to_string(),
            synopsis: "Short.".to_string(),
            poster: None,
            has_stream: false,
            stream_duration_s: 4.0,
            stream_fps: 30,
            http_stream: None,
            http_frame_delay: Duration::from_millis(33),
        };
        runner.shared.lock().unwrap().ready_item = Some(mk_item("m1"));
        runner.activate();
        runner.evaluate_turn_start(4);

        // Standby N+1 (`m2`) arrives while `m1` is playing, AND user saves Count=1 in WebUI:
        runner.shared.lock().unwrap().ready_item = Some(mk_item("m2"));
        runner.set_rotation_budget(1);

        // Finishing `m1` must immediately complete the turn (1/1) without starting `m2`!
        runner.finish_current_turn();
        assert!(runner.turn_completed);
        assert!(runner.is_finished());
        assert!(runner.shared.lock().unwrap().ready_item.is_some());
    }

    #[test]
    fn test_wall_clock_30fps_stream_timing_and_60s_cap() {
        let mut runner = MediaShowcaseRunner::new(ShowcaseDomain::Movies, 128, 32);
        runner.cfg.server_url.clear();
        runner.cfg.jacket_panorama = false;
        runner.cfg.marquee_panorama = false;
        runner.cfg.snapshot_panorama_count = 0;
        runner.cfg.show_video = true;
        runner.cfg.video_max_duration_s = 60;
        assert_eq!(
            runner.configured_max_stream_duration(),
            Duration::from_secs(60)
        );

        // 2400 frames @ 33ms (~79.2s) > 60s DMD cap
        let stream = Arc::new(PreloadedRgb565Stream::from_raw_rgb565_le(
            2,
            2,
            2400,
            Duration::from_millis(33),
            vec![0u8; 2 * 2 * 2 * 2400],
        ));
        let config = crate::core::config::Config::new("/tmp/nonexistent_showcase_test_cfg.json");
        {
            let mut s = config.settings.write();
            s.rotation = vec![
                crate::core::config::RotationEntry::new("retro_games_1", 1),
                crate::core::config::RotationEntry::new("movie_showcase_1", 1),
            ];
            s.showcase.enabled = true;
            s.showcase.server_url.clear();
        }
        runner.unified_showcase = config.settings.read().showcase.clone();
        {
            let mut guard = runner.shared.lock().unwrap();
            guard.server_online = true;
            guard.readiness = BatchReadiness::Ready;
        }
        let mut surface = MockSurface::new(128, 32);

        runner.activate();
        runner.turn_evaluated = true;
        runner.rotation_slot_count = 2;
        runner.set_rotation_budget(1);
        runner.active_item = Some(PreparedShowcaseItem {
            id: "m60".to_string(),
            title: "Heat".to_string(),
            year: "1995".to_string(),
            badge: "ACTION".to_string(),
            badge_rgb: (255, 80, 0),
            subtitle: "Crime".to_string(),
            rating: "8.3".to_string(),
            synopsis: "Short.".to_string(),
            poster: None,
            has_stream: true,
            stream_duration_s: 79.2,
            stream_fps: 30,
            http_stream: Some(stream),
            http_frame_delay: Duration::from_millis(33),
        });
        runner.phase = PlaybackPhase::LiveStream;

        // At wall-clock 30.0s, frame index must be ~909 (30s / 33ms), NOT ~600!
        runner.phase_started_at = Instant::now() - Duration::from_secs(30);
        {
            let mut ctx = crate::core::engine_contract::EngineContext {
                surface: &mut surface,
                config: &config,
            };
            runner.update(&mut ctx);
        }
        assert!(
            runner.http_frame_idx >= 905 && runner.http_frame_idx <= 915,
            "expected ~909 frames at 30.0s, got {}",
            runner.http_frame_idx
        );
        assert!(!runner.is_finished());

        // At wall-clock 60.1s, DMD's 60s `video_max_duration_s` cap must immediately end the turn!
        runner.phase_started_at = Instant::now() - Duration::from_millis(60_100);
        {
            let mut ctx = crate::core::engine_contract::EngineContext {
                surface: &mut surface,
                config: &config,
            };
            runner.update(&mut ctx);
        }
        assert!(runner.is_finished());
    }

    #[test]
    fn test_turn_start_skips_immediately_when_ready_false_received() {
        let mut runner = MediaShowcaseRunner::new(ShowcaseDomain::Movies, 128, 32);
        runner.cfg.server_url = "http://127.0.0.1:8890".to_string();
        let config = crate::core::config::Config::new("/tmp/nonexistent_showcase_test_cfg.json");
        {
            let mut s = config.settings.write();
            s.rotation = vec![
                crate::core::config::RotationEntry::new("retro_games_1", 1),
                crate::core::config::RotationEntry::new("movie_showcase_1", 1),
            ];
            s.showcase = crate::core::config::ShowcaseConfig {
                enabled: true,
                server_url: "http://127.0.0.1:8890".to_string(),
                ..Default::default()
            };
        }
        runner.unified_showcase = config.settings.read().showcase.clone();
        // Prevent spawning a real network thread during unit test
        let cur_gen = runner.config_gen.load(Ordering::SeqCst);
        runner.worker_gen.store(cur_gen, Ordering::SeqCst);
        {
            let mut guard = runner.shared.lock().unwrap();
            guard.server_online = true;
            guard.mqtt_connected_once = true;
            guard.mqtt_connected_at = Some(Instant::now());
            guard.ready_state_received = true;
            guard.readiness = BatchReadiness::Warming;
            guard.ready_item = None;
        }
        runner.turn_evaluated = false;
        let mut surface = MockSurface::new(128, 32);
        let mut ctx = crate::core::engine_contract::EngineContext {
            surface: &mut surface,
            config: &config,
        };
        runner.update(&mut ctx);
        assert!(runner.turn_evaluated);
        assert!(runner.skip_turn_immediately);
        assert!(runner.is_finished());
    }

    #[test]
    fn test_256x64_geometry_poster_dmdf_32_chunks_and_live_resolution_switch() {
        // 1. Adaptive poster size on 256x64 must be 48x64 (6,144 bytes = 8,192 B64 chars)
        assert_eq!(compute_card_poster_dims(256, 64), (48, 64));
        assert_eq!(compute_card_poster_dims(64, 256), (40, 40));

        // 2. 256x64 DMDF UDP frame reassembly (32,768 bytes = 32 chunks of 1,024 bytes)
        let mut assembler = DmdfFrameAssembler::new(256, 64);
        let frame_bytes = 256usize * 64usize * 2usize;
        let total_chunks = 32u8;
        let mut raw_frame = vec![0u8; frame_bytes];
        // Set last pixel (255, 63) to pure green RGB565 (0x07E0 -> LE [0xE0, 0x07])
        raw_frame[frame_bytes - 2] = 0xE0;
        raw_frame[frame_bytes - 1] = 0x07;

        let mut assembled = None;
        for chunk_idx in 0..total_chunks {
            let byte_offset = (chunk_idx as usize) * 1024;
            let mut pkt = Vec::with_capacity(10 + 1024);
            pkt.extend_from_slice(DMDF_MAGIC);
            pkt.extend_from_slice(&42u16.to_le_bytes());
            pkt.push(chunk_idx);
            pkt.push(total_chunks);
            pkt.extend_from_slice(&(byte_offset as u16).to_le_bytes());
            pkt.extend_from_slice(&raw_frame[byte_offset..byte_offset + 1024]);
            let out = assembler.push_packet(&pkt);
            if chunk_idx < total_chunks - 1 {
                assert!(out.is_none());
            } else {
                assembled = out;
            }
        }
        let (seq, img) = assembled.expect("32-chunk 256x64 DMDF frame must assemble");
        assert_eq!(seq, 42);
        assert_eq!((img.width(), img.height()), (256, 64));
        let last_px = img.get_pixel(255, 63).0;
        assert_eq!(last_px, [0, 255, 0]);

        // 3. Live hardware geometry transition (128x32 -> 256x64) clears stale 128x32 media
        //    and updates client ID to `rpi_<hwid>_games_256x64`
        let hwid = rpi_hardware_short_id();
        assert_eq!(hwid.len(), 6);
        assert!(hwid
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        let mut runner = MediaShowcaseRunner::new(ShowcaseDomain::Games, 128, 32);
        runner.cfg.enabled = false; // prevent spawning live network thread in unit test
        assert_eq!(runner.dmd_client_id(), format!("rpi_{}_games_128x32", hwid));
        let gen_before = runner.config_gen.load(Ordering::SeqCst);
        runner.active_item = Some(PreparedShowcaseItem {
            id: "old_128x32".to_string(),
            title: "Metal Slug".to_string(),
            year: "1996".to_string(),
            badge: "NEOGEO".to_string(),
            badge_rgb: (255, 109, 0),
            subtitle: "Run & Gun".to_string(),
            rating: "9.0".to_string(),
            synopsis: "Test".to_string(),
            poster: None,
            has_stream: false,
            stream_duration_s: 8.0,
            stream_fps: 15,
            http_stream: None,
            http_frame_delay: Duration::from_millis(66),
        });
        runner.on_display_geometry_changed(256, 64);
        assert_eq!((runner.width, runner.height), (256, 64));
        assert_eq!(runner.dmd_client_id(), format!("rpi_{}_games_256x64", hwid));
        assert!(runner.active_item.is_none());
        assert!(runner.config_gen.load(Ordering::SeqCst) > gen_before);
    }

    #[test]
    fn test_hardware_scoped_rpi_short_id_isolates_multiple_boards() {
        let cpu_a = "Hardware\t: BCM2835\nRevision\t: c03112\nSerial\t\t: 10000000aa11bb22\n";
        let cpu_b = "Hardware\t: BCM2835\nRevision\t: c03112\nSerial\t\t: 10000000cc33dd44\n";
        let ser_a = extract_cpuinfo_serial(cpu_a).expect("serial A");
        let ser_b = extract_cpuinfo_serial(cpu_b).expect("serial B");
        assert_eq!(ser_a, "10000000aa11bb22");
        assert_eq!(ser_b, "10000000cc33dd44");

        let id_a = derive_rpi_short_id_from_seed(&format!("cpu-serial:{}", ser_a));
        let id_b = derive_rpi_short_id_from_seed(&format!("cpu-serial:{}", ser_b));
        assert_eq!(id_a.len(), 6);
        assert_eq!(id_b.len(), 6);
        assert_ne!(id_a, id_b);
        assert!(extract_cpuinfo_serial("Serial : 0000000000000000\n").is_none());
    }

    #[test]
    fn test_normalize_mdns_hostname_and_endpoint_url() {
        assert_eq!(
            normalize_mdns_hostname("dmdmediashowcase"),
            "dmdmediashowcase.local"
        );
        assert_eq!(normalize_mdns_hostname("auto"), "dmdmediashowcase.local");
        assert_eq!(normalize_mdns_hostname("localhost"), "localhost");
        assert_eq!(normalize_mdns_hostname("192.168.1.22"), "192.168.1.22");
        assert_eq!(
            normalize_mdns_endpoint_url("http://dmdmediashowcase:8890", true),
            "http://dmdmediashowcase.local:8890"
        );
        assert_eq!(
            normalize_mdns_endpoint_url("http://dmdmediashowcase:8080", true),
            "http://dmdmediashowcase.local:8890"
        );
        assert_eq!(
            normalize_mdns_endpoint_url("http://mybox:8890", false),
            "http://mybox.local:8890"
        );
        assert_eq!(
            normalize_mdns_endpoint_url("mqtt://mosquitto:1883", false),
            "mqtt://mosquitto.local:1883"
        );
        assert_eq!(
            normalize_mdns_endpoint_url("http://192.168.1.22:8890", true),
            "http://192.168.1.22:8890"
        );
        assert_eq!(
            normalize_mdns_endpoint_url("http://localhost:8890", false),
            "http://localhost:8890"
        );
        assert_eq!(
            parse_host_and_port("http://dmdmediashowcase:8890", 8890),
            Some(("dmdmediashowcase.local".to_string(), 8890))
        );
    }

    #[test]
    fn test_dmdz_v2_stream_playback_and_allows_overlay_gate() {
        let mut dmdz = Vec::new();
        dmdz.extend_from_slice(crate::core::dmd_stream::DMDZ_MAGIC);
        dmdz.push(2); // v2
        dmdz.push(25); // 25 FPS
        dmdz.extend_from_slice(&2u16.to_le_bytes());
        dmdz.extend_from_slice(&2u16.to_le_bytes());
        dmdz.extend_from_slice(&2u16.to_le_bytes()); // 2 frames
        dmdz.extend_from_slice(&40u16.to_le_bytes());
        dmdz.extend_from_slice(&0u16.to_le_bytes());

        // Frame 0: FRAME_PAL8, 1 color (pure red 0xF800), PAL_INDEX 0 + PAL_RUN 3 -> 4 red pixels
        let mut f0 = vec![crate::core::dmd_stream::FRAME_PAL8, 0];
        f0.extend_from_slice(&0xF800u16.to_le_bytes());
        f0.push(0);
        f0.push(0xC1); // run of 3 -> total 4 pixels
        dmdz.extend_from_slice(&(f0.len() as u16).to_le_bytes());
        dmdz.extend_from_slice(&f0);

        // Frame 1: FRAME_PAL8, 1 color (pure green 0x07E0), PAL_INDEX 0 + PAL_RUN 3 -> 4 green pixels
        let mut f1 = vec![crate::core::dmd_stream::FRAME_PAL8, 0];
        f1.extend_from_slice(&0x07E0u16.to_le_bytes());
        f1.push(0);
        f1.push(0xC1);
        dmdz.extend_from_slice(&(f1.len() as u16).to_le_bytes());
        dmdz.extend_from_slice(&f1);

        let stream = parse_rgb565_stream_compact(&dmdz).expect("valid DMDZ v2 stream");
        assert!(stream.is_compressed_dmdz());

        let mut runner = MediaShowcaseRunner::new(ShowcaseDomain::Movies, 2, 2);
        runner.cfg.enabled = true;
        runner.cfg.server_url.clear();
        assert!(runner.allows_overlay());

        let tmp_dir = std::env::temp_dir().join(format!(
            "arcadematrix_showcase_dmdz_test_{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&tmp_dir);
        let config = crate::core::config::Config::new(tmp_dir.join("config.json"));
        {
            let mut s = config.settings.write();
            s.showcase.enabled = true;
            s.showcase.server_url.clear();
        }
        runner.unified_showcase = config.settings.read().showcase.clone();
        {
            let mut guard = runner.shared.lock().unwrap();
            guard.server_online = true;
            guard.readiness = BatchReadiness::Ready;
        }
        runner.activate();
        runner.turn_evaluated = true;
        runner.active_item = Some(PreparedShowcaseItem {
            id: "dmdz_item".to_string(),
            title: "Akira".to_string(),
            year: "1988".to_string(),
            badge: "ANIME".to_string(),
            badge_rgb: (255, 64, 129),
            subtitle: "Cyberpunk".to_string(),
            rating: "8.0".to_string(),
            synopsis: "Neo-Tokyo.".to_string(),
            poster: None,
            has_stream: true,
            stream_duration_s: 1.0,
            stream_fps: 25,
            http_stream: Some(stream),
            http_frame_delay: Duration::from_millis(40),
        });

        let now = Instant::now();
        runner.start_live_stream_phase(now - Duration::from_millis(50), "dmdz_item");
        assert!(!runner.allows_overlay());

        let mut surface = MockSurface::new(2, 2);
        let mut ctx = crate::core::engine_contract::EngineContext {
            surface: &mut surface,
            config: &config,
        };
        runner.update(&mut ctx);
        assert_eq!(runner.http_frame_idx, 1);
        runner.render(&mut ctx);
        assert_eq!(surface.canvas.get_pixel(0, 0).0, [0, 255, 0]);
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}

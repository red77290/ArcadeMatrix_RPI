//! Shared renderer and 2-step LAN protocol runner (`MediaShowcaseRunner`) for the 4
//! DMD Media Showcase engines (`retro_games`, `movie_showcase`, `series_showcase`, `anime_showcase`).
//!
//! V2.1 Architecture:
//! - Zero hardcoded demo/mock items: never displays fake titles (`Stranger Things`, `Metal Slug`, etc.)
//!   when the server is unconfigured or unreachable.
//! - Granular top-to-bottom panorama streams (`jacket_panorama`, `marquee_panorama`,
//!   `snapshot_panorama_count`) powered by `dmd_gif_converter`.
//! - Organic rotation timing: `PlaybackPhase::Card` dynamically computes the exact time
//!   needed for 100% of the localized synopsis to scroll from start to finish before advancing
//!   to `JacketPanorama -> MarqueePanorama -> SnapshotPanoramas -> VideoStream`.
//! - Adaptive multi-resolution (`256x64`, `128x64`, `128x32`, `64x64`) & TATE vertical
//!   (`64x128`, `64x256`, `32x128`) layouts derived automatically from `context.surface`.

use crate::core::engine_contract::{EngineConfig, EngineContext};
use crate::core::matrix::IDrawingSurface;
use crate::engines::dashboard::font::draw_text_scaled;
use crate::engines::renderers::primitives::{
    draw_fast_hline, draw_round_rect, draw_scrolling_text, fill_round_rect,
};
use crate::engines::renderers::tiny_font::draw_tiny_text;
use image::RgbImage;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, warn};

pub const DMDS_MAGIC: &[u8; 4] = b"DMDS";
pub const DMDS_HEADER_SIZE: usize = 16;

/// Speed of the single-pass organic synopsis scroll in pixels per second.
const SYNOPSIS_SPEED_PX_PER_SEC: u64 = 24;
/// Initial hold before the synopsis begins scrolling (ms).
const SYNOPSIS_START_HOLD_MS: u64 = 900;
/// End hold once the final word of the synopsis is on screen (ms).
const SYNOPSIS_END_HOLD_MS: u64 = 700;

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
    pub items_per_rotation: u32,
    pub lang: String,
    pub primary_source: String,
    pub scraper_user: String,
    pub scraper_pass: String,
    pub scraper_api_key: String,
    pub show_poster: bool,
    pub show_synopsis: bool,
    pub jacket_panorama: bool,
    pub marquee_panorama: bool,
    pub snapshot_panorama_count: u32,
    pub show_video: bool,
    pub card_duration_s: u32,
    pub snapshot_duration_s: u32,
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
            server_url: "http://127.0.0.1:8790".to_string(),
            items_per_rotation: 3,
            lang: "system".to_string(),
            primary_source: "auto".to_string(),
            scraper_user: String::new(),
            scraper_pass: String::new(),
            scraper_api_key: String::new(),
            show_poster: true,
            show_synopsis: true,
            jacket_panorama: true,
            marquee_panorama: false,
            snapshot_panorama_count: 0,
            show_video: true,
            card_duration_s: 7,
            snapshot_duration_s: 4,
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
        let raw_url = cfg.get_string("server_url", "http://127.0.0.1:8790");
        out.server_url = raw_url.trim().trim_end_matches('/').to_string();
        out.items_per_rotation = cfg.get_int("items_per_rotation", 3).clamp(1, 25) as u32;
        out.lang = cfg.get_string("lang", "system");
        out.primary_source = cfg.get_string("primary_source", "auto");
        out.scraper_user = cfg.get_string("scraper_user", "");
        out.scraper_pass = cfg.get_string("scraper_pass", "");
        out.scraper_api_key = cfg.get_string("scraper_api_key", "");
        out.show_poster = cfg.get_bool("show_poster", true);
        out.show_synopsis = cfg.get_bool("show_synopsis", true);
        out.jacket_panorama = cfg.get_bool("jacket_panorama", true);
        out.marquee_panorama = cfg.get_bool("marquee_panorama", false);

        // Support `snapshot_panorama_count` with backward-compatible fallback to `image_count`
        let snap_cnt = cfg.get_int("snapshot_panorama_count", -1);
        if snap_cnt >= 0 {
            out.snapshot_panorama_count = snap_cnt.clamp(0, 5) as u32;
        } else {
            out.snapshot_panorama_count = cfg.get_int("image_count", 0).clamp(0, 5) as u32;
        }

        out.show_video = cfg.get_bool("show_video", true);
        out.card_duration_s = cfg.get_int("card_duration_s", 7).clamp(3, 30) as u32;
        out.snapshot_duration_s = cfg.get_int("snapshot_duration_s", 4).clamp(2, 15) as u32;
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

    pub fn any_panorama_requested(&self) -> bool {
        self.jacket_panorama || self.marquee_panorama || self.snapshot_panorama_count > 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMediaAssets {
    pub poster_png: String,
    pub poster_rgb565: String,
    pub marquee_png: String,
    pub marquee_rgb565: String,
    pub snapshot_pan_png: String,
    pub snapshot_pan_rgb565: String,
    #[serde(default)]
    pub jacket_stream_rgb565: String,
    #[serde(default)]
    pub marquee_stream_rgb565: String,
    #[serde(default)]
    pub snapshot_streams_rgb565: Vec<String>,
    #[serde(default)]
    pub panorama_gifs: Vec<String>,
    #[serde(default)]
    pub panorama_streams_rgb565: Vec<String>,
    #[serde(default)]
    pub video_gif: String,
    #[serde(default)]
    pub video_stream_rgb565: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiShowcaseItem {
    pub id: String,
    pub domain: String,
    pub title: String,
    pub year: String,
    pub badge: String,
    #[serde(default = "default_badge_hex")]
    pub badge_color: String,
    pub subtitle: String,
    pub rating: String,
    pub synopsis: String,
    #[serde(default)]
    pub poster_width: u32,
    #[serde(default)]
    pub poster_height: u32,
    #[serde(default)]
    pub has_video: bool,
    #[serde(default)]
    pub video_ready: bool,
    #[serde(default)]
    pub video_duration_s: f32,
    #[serde(default = "default_fps_val")]
    pub video_fps: u32,
    pub assets: ApiMediaAssets,
}

fn default_badge_hex() -> String {
    "#FF6D00".to_string()
}

fn default_fps_val() -> u32 {
    15
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSessionRegisterResponse {
    pub session_id: String,
    pub status: String,
    pub batch_size: usize,
    pub ready_count: usize,
    pub queue_depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiBatchStatusResponse {
    pub session_id: String,
    pub status: String,
    pub ready_count: usize,
    pub total_count: usize,
    pub queue_position: usize,
    pub items: Vec<ApiShowcaseItem>,
}

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
    pub snapshot_pan: Option<RgbImage>,
    pub panorama_streams: Vec<(Vec<RgbImage>, Duration)>,
    pub video_frames: Vec<RgbImage>,
    pub video_frame_delay: Duration,
    pub has_video: bool,
    pub video_ready: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchReadiness {
    Offline,
    Warming,
    Ready,
}

pub struct SharedShowcaseBatch {
    pub session_id: Option<String>,
    pub readiness: BatchReadiness,
    pub items: Vec<PreparedShowcaseItem>,
    pub generation: u64,
}

impl Default for SharedShowcaseBatch {
    fn default() -> Self {
        Self {
            session_id: None,
            readiness: BatchReadiness::Offline,
            items: Vec::new(),
            generation: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackPhase {
    Card,
    SnapshotPan,
    VideoStream,
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

/// Draw a single-pass organic scrolling synopsis across `[min_x, max_x)` without premature cutoffs.
fn draw_single_pass_synopsis(
    matrix: &mut dyn IDrawingSurface,
    text: &str,
    min_x: i32,
    max_x: i32,
    y: i32,
    color: (u8, u8, u8),
    elapsed_ms: u128,
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
    let raw_offset = ((scroll_ms * SYNOPSIS_SPEED_PX_PER_SEC) / 1000) as i32;
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

pub struct MediaShowcaseRunner {
    pub domain: ShowcaseDomain,
    pub cfg: ShowcaseConfig,
    pub system_lang: String,
    pub width: u32,
    pub height: u32,
    pub shared: Arc<Mutex<SharedShowcaseBatch>>,
    pub worker_busy: Arc<AtomicBool>,
    pub config_gen: Arc<AtomicU64>,

    // Per-turn playback state
    pub active_items: Vec<PreparedShowcaseItem>,
    pub current_item_idx: usize,
    pub phase: PlaybackPhase,
    pub phase_started_at: Instant,
    pub last_retry_poll: Instant,
    pub pan_image_idx: usize,
    pub pan_frame_idx: usize,
    pub pan_frame_elapsed: Duration,
    pub video_frame_idx: usize,
    pub video_frame_elapsed: Duration,
    pub last_tick: Instant,
    pub turn_evaluated: bool,
    pub skip_turn_immediately: bool,
    pub forced_warming_card_fallback: bool,
    pub dirty: bool,
}

impl MediaShowcaseRunner {
    pub fn new(domain: ShowcaseDomain, width: u32, height: u32) -> Self {
        let now = Instant::now();
        Self {
            domain,
            cfg: ShowcaseConfig::default(),
            system_lang: "fr".to_string(),
            width: width.max(32),
            height: height.max(32),
            shared: Arc::new(Mutex::new(SharedShowcaseBatch::default())),
            worker_busy: Arc::new(AtomicBool::new(false)),
            config_gen: Arc::new(AtomicU64::new(1)),
            active_items: Vec::new(),
            current_item_idx: 0,
            phase: PlaybackPhase::Card,
            phase_started_at: now,
            last_retry_poll: now,
            pan_image_idx: 0,
            pan_frame_idx: 0,
            pan_frame_elapsed: Duration::ZERO,
            video_frame_idx: 0,
            video_frame_elapsed: Duration::ZERO,
            last_tick: now,
            turn_evaluated: false,
            skip_turn_immediately: false,
            forced_warming_card_fallback: false,
            dirty: true,
        }
    }

    /// Compute the exact organic duration of `PlaybackPhase::Card` so 100% of `item.synopsis`
    /// scrolls across the DMD viewport before advancing to Jacket/Marquee/Snapshot panoramas.
    pub fn organic_card_duration(&self, item: &PreparedShowcaseItem) -> Duration {
        let base_dur = Duration::from_secs(self.cfg.card_duration_s as u64);
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
        let scroll_ms = (scroll_px * 1000) / SYNOPSIS_SPEED_PX_PER_SEC;
        let total_ms = SYNOPSIS_START_HOLD_MS + scroll_ms + SYNOPSIS_END_HOLD_MS;
        Duration::from_millis(total_ms).max(base_dur)
    }

    /// Spawn background session registration (`POST /api/v1/{domain}/session`) and asset pre-fetching.
    pub fn spawn_background_sync(&self, register_new_session: bool) {
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
        let gen = self.config_gen.load(Ordering::SeqCst);

        std::thread::spawn(move || {
            let _guard = BusyResetGuard(busy);
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
            let (default_pw, default_ph) = compute_card_poster_dims(width, height);

            let existing_session_id = {
                let guard = shared.lock().unwrap();
                guard.session_id.clone()
            };

            let mut session_id = if !register_new_session {
                existing_session_id
            } else {
                None
            };

            let build_reg_body = || {
                serde_json::json!({
                    "client_id": format!("rpi_{}_{}x{}", slug, width, height),
                    "count": cfg.items_per_rotation,
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
                    "include_snapshot": cfg.any_panorama_requested(),
                    "image_count": cfg.snapshot_panorama_count,
                    "primary_source": cfg.primary_source,
                    "scraper_user": cfg.scraper_user,
                    "scraper_pass": cfg.scraper_pass,
                    "scraper_api_key": cfg.scraper_api_key,
                    "include_video": cfg.show_video,
                    "video_max_duration_s": cfg.video_max_duration_s as f32,
                    "video_fps": cfg.video_fps,
                    "video_mode": cfg.video_mode,
                    "color_boost": cfg.color_boost,
                    "systems": split_csv(&cfg.systems),
                    "genres": split_csv(&cfg.genres),
                    "decades": split_csv(&cfg.decades),
                    "min_rating": cfg.min_rating,
                    "video_type": cfg.video_type,
                })
            };

            let reg_url = format!("{}/api/v1/{}/session", base, slug);
            if session_id.is_none() {
                match client.post(&reg_url).json(&build_reg_body()).send() {
                    Ok(resp) if resp.status().is_success() => {
                        if let Ok(reg_data) = resp.json::<ApiSessionRegisterResponse>() {
                            session_id = Some(reg_data.session_id);
                        }
                    }
                    Ok(resp) => {
                        debug!("Showcase session POST returned HTTP {}", resp.status());
                    }
                    Err(e) => {
                        debug!("Showcase server unreachable at {}: {}", reg_url, e);
                    }
                }
            }

            let Some(mut ses_id) = session_id else {
                return;
            };

            let mut batch_url = format!("{}/api/v1/{}/batch?session_id={}", base, slug, ses_id);
            let mut batch_resp = client.get(&batch_url).send();

            // If session expired or server restarted (404), automatically re-register once
            if let Ok(ref r) = batch_resp {
                if !r.status().is_success() {
                    if let Ok(reg_r) = client.post(&reg_url).json(&build_reg_body()).send() {
                        if reg_r.status().is_success() {
                            if let Ok(reg_data) = reg_r.json::<ApiSessionRegisterResponse>() {
                                ses_id = reg_data.session_id;
                                batch_url =
                                    format!("{}/api/v1/{}/batch?session_id={}", base, slug, ses_id);
                                batch_resp = client.get(&batch_url).send();
                            }
                        }
                    }
                }
            }

            let Ok(resp) = batch_resp else {
                return;
            };
            if !resp.status().is_success() {
                return;
            }
            let Ok(batch_data) = resp.json::<ApiBatchStatusResponse>() else {
                return;
            };

            let is_ready = batch_data.status.eq_ignore_ascii_case("ready");
            let pan_h = height * 2 + 8;

            let mut prepared = Vec::with_capacity(batch_data.items.len());
            for api_item in batch_data.items {
                let badge_rgb = parse_hex_rgb(&api_item.badge_color, domain.default_accent_rgb());
                let item_pw = if api_item.poster_width > 0 {
                    api_item.poster_width
                } else {
                    default_pw
                };
                let item_ph = if api_item.poster_height > 0 {
                    api_item.poster_height
                } else {
                    default_ph
                };

                let poster = if cfg.show_poster {
                    let url = format!("{}{}", base, api_item.assets.poster_rgb565);
                    client
                        .get(&url)
                        .send()
                        .ok()
                        .and_then(|r| r.bytes().ok())
                        .and_then(|b| decode_rgb565_le_image(&b, item_pw, item_ph))
                } else {
                    None
                };

                // Download ordered panorama streams (`jacket_panorama` -> `marquee_panorama` -> `snapshot_panoramas`)
                let mut panorama_streams = Vec::new();
                if cfg.any_panorama_requested() {
                    let mut ordered_paths: Vec<String> = Vec::new();
                    if !api_item.assets.panorama_streams_rgb565.is_empty() {
                        ordered_paths
                            .extend(api_item.assets.panorama_streams_rgb565.iter().cloned());
                    } else {
                        if cfg.jacket_panorama && !api_item.assets.jacket_stream_rgb565.is_empty() {
                            ordered_paths.push(api_item.assets.jacket_stream_rgb565.clone());
                        }
                        if cfg.marquee_panorama && !api_item.assets.marquee_stream_rgb565.is_empty()
                        {
                            ordered_paths.push(api_item.assets.marquee_stream_rgb565.clone());
                        }
                        for s_path in api_item
                            .assets
                            .snapshot_streams_rgb565
                            .iter()
                            .take(cfg.snapshot_panorama_count as usize)
                        {
                            ordered_paths.push(s_path.clone());
                        }
                    }

                    for pan_path in ordered_paths {
                        let url = format!("{}{}", base, pan_path);
                        if let Ok(r) = client.get(&url).send() {
                            if r.status().is_success() {
                                if let Ok(bytes) = r.bytes() {
                                    if let Some(decoded) = decode_rgb565_stream(&bytes) {
                                        panorama_streams.push(decoded);
                                    }
                                }
                            }
                        }
                    }
                }

                let snapshot_pan = if cfg.any_panorama_requested() && panorama_streams.is_empty() {
                    let url = format!("{}{}", base, api_item.assets.snapshot_pan_rgb565);
                    client
                        .get(&url)
                        .send()
                        .ok()
                        .and_then(|r| r.bytes().ok())
                        .and_then(|b| decode_rgb565_le_image(&b, width, pan_h))
                } else {
                    None
                };

                let mut video_frames = Vec::new();
                let mut video_frame_delay =
                    Duration::from_millis((1000 / api_item.video_fps.max(1)) as u64);

                if cfg.show_video
                    && api_item.has_video
                    && !api_item.assets.video_stream_rgb565.is_empty()
                    && (is_ready || api_item.video_ready)
                {
                    let raw_path = api_item
                        .assets
                        .video_stream_rgb565
                        .split('?')
                        .next()
                        .unwrap_or(&api_item.assets.video_stream_rgb565);
                    let stream_url = format!(
                        "{}{}?max_duration_s={}",
                        base, raw_path, cfg.video_max_duration_s
                    );
                    if let Ok(r) = client.get(&stream_url).send() {
                        if r.status().is_success() {
                            if let Ok(bytes) = r.bytes() {
                                if let Some((frames, delay)) = decode_rgb565_stream(&bytes) {
                                    video_frames = frames;
                                    video_frame_delay = delay;
                                }
                            }
                        }
                    }
                }

                let item_has_video = api_item.has_video && !video_frames.is_empty();
                prepared.push(PreparedShowcaseItem {
                    id: api_item.id,
                    title: api_item.title,
                    year: api_item.year,
                    badge: api_item.badge,
                    badge_rgb,
                    subtitle: api_item.subtitle,
                    rating: api_item.rating,
                    synopsis: api_item.synopsis,
                    poster,
                    snapshot_pan,
                    panorama_streams,
                    video_frames,
                    video_frame_delay,
                    has_video: item_has_video,
                    video_ready: api_item.video_ready,
                });
            }

            let mut guard = shared.lock().unwrap();
            guard.session_id = Some(ses_id);
            guard.readiness = if is_ready {
                BatchReadiness::Ready
            } else {
                BatchReadiness::Warming
            };
            if !prepared.is_empty() {
                guard.items = prepared;
            }
            guard.generation = gen;
        });
    }

    pub fn initialize(&mut self, context: &mut EngineContext, config: &dyn EngineConfig) {
        self.width = context.surface.width().max(32);
        self.height = context.surface.height().max(32);
        self.system_lang = context.config.settings.read().system.lang.clone();
        self.cfg = ShowcaseConfig::from_engine_config(config, self.domain);
        self.active_items.clear();
        self.config_gen.fetch_add(1, Ordering::SeqCst);
        self.spawn_background_sync(true);
    }

    pub fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        let new_cfg = ShowcaseConfig::from_engine_config(config, self.domain);
        if new_cfg != self.cfg {
            self.cfg = new_cfg;
            self.active_items.clear();
            {
                let mut guard = self.shared.lock().unwrap();
                guard.session_id = None;
                guard.items.clear();
                guard.readiness = BatchReadiness::Offline;
            }
            self.config_gen.fetch_add(1, Ordering::SeqCst);
            self.spawn_background_sync(true);
        }
    }

    pub fn activate(&mut self) {
        let now = Instant::now();
        self.current_item_idx = 0;
        self.pan_image_idx = 0;
        self.pan_frame_idx = 0;
        self.pan_frame_elapsed = Duration::ZERO;
        self.video_frame_idx = 0;
        self.video_frame_elapsed = Duration::ZERO;
        self.phase_started_at = now;
        self.last_retry_poll = now;
        self.last_tick = now;
        self.turn_evaluated = false;
        self.skip_turn_immediately = false;
        self.forced_warming_card_fallback = false;
        self.dirty = true;
    }

    pub fn deactivate(&mut self) {
        self.skip_turn_immediately = false;
        self.turn_evaluated = false;
    }

    fn item_has_images(&self, item: &PreparedShowcaseItem) -> bool {
        self.cfg.any_panorama_requested()
            && (!item.panorama_streams.is_empty() || item.snapshot_pan.is_some())
    }

    fn initial_phase_for_item(&self, item: &PreparedShowcaseItem) -> PlaybackPhase {
        if self.forced_warming_card_fallback || self.cfg.show_poster || self.cfg.show_synopsis {
            PlaybackPhase::Card
        } else if self.item_has_images(item) {
            PlaybackPhase::SnapshotPan
        } else if self.cfg.show_video && item.has_video && !item.video_frames.is_empty() {
            PlaybackPhase::VideoStream
        } else {
            PlaybackPhase::Card
        }
    }

    /// Evaluate the 2-step rotation readiness policy at the start of the engine's rotation turn.
    pub fn evaluate_turn_start(&mut self, rotation_slot_count: usize) {
        self.turn_evaluated = true;
        let (readiness, items) = {
            let guard = self.shared.lock().unwrap();
            (guard.readiness, guard.items.clone())
        };

        match readiness {
            BatchReadiness::Ready if !items.is_empty() => {
                self.active_items = items
                    .into_iter()
                    .take(self.cfg.items_per_rotation as usize)
                    .collect();
                self.skip_turn_immediately = false;
                self.forced_warming_card_fallback = false;
                if let Some(first) = self.active_items.first() {
                    self.phase = self.initial_phase_for_item(first);
                }
                // Trigger background fetch of NextBatch one step ahead
                self.spawn_background_sync(false);
            }
            BatchReadiness::Warming | BatchReadiness::Offline | BatchReadiness::Ready => {
                if rotation_slot_count > 1 && items.is_empty() {
                    warn!(
                        "[Showcase:{}] Server offline or batch warming with {} rotation slots -> skipping turn cleanly",
                        self.domain.as_api_slug(),
                        rotation_slot_count
                    );
                    self.skip_turn_immediately = true;
                    self.spawn_background_sync(false);
                } else {
                    self.skip_turn_immediately = false;
                    self.forced_warming_card_fallback = true;
                    self.active_items = items
                        .into_iter()
                        .take(self.cfg.items_per_rotation as usize)
                        .collect();
                    self.phase = PlaybackPhase::Card;
                    self.spawn_background_sync(false);
                }
            }
        }
        self.phase_started_at = Instant::now();
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
            self.spawn_background_sync(true);
        }

        if !self.turn_evaluated {
            let rot_len = context.config.settings.read().rotation.len();
            self.evaluate_turn_start(rot_len);
            self.dirty = true;
            return;
        }

        if self.skip_turn_immediately {
            return;
        }

        // If waiting for server or warming, poll shared state and re-trigger background sync every 2.5s
        if self.forced_warming_card_fallback || self.active_items.is_empty() {
            let (readiness, ready_items) = {
                let guard = self.shared.lock().unwrap();
                (guard.readiness, guard.items.clone())
            };
            if !ready_items.is_empty() {
                self.active_items = ready_items
                    .into_iter()
                    .take(self.cfg.items_per_rotation as usize)
                    .collect();
                if readiness == BatchReadiness::Ready {
                    self.forced_warming_card_fallback = false;
                }
                self.phase_started_at = now;
                self.dirty = true;
            } else {
                if now.saturating_duration_since(self.last_retry_poll)
                    >= Duration::from_millis(2500)
                {
                    self.last_retry_poll = now;
                    self.spawn_background_sync(true);
                }
                self.dirty = true;
                return;
            }
        }

        if self.current_item_idx >= self.active_items.len() {
            return;
        }

        let phase_elapsed = now.saturating_duration_since(self.phase_started_at);
        let item = &self.active_items[self.current_item_idx];
        let has_images = self.item_has_images(item);
        let has_playable_video =
            self.cfg.show_video && item.has_video && !item.video_frames.is_empty();
        let card_target_dur = self.organic_card_duration(item);

        match self.phase {
            PlaybackPhase::Card => {
                self.dirty = true;
                if phase_elapsed >= card_target_dur {
                    if has_images {
                        self.phase = PlaybackPhase::SnapshotPan;
                        self.phase_started_at = now;
                        self.pan_image_idx = 0;
                        self.pan_frame_idx = 0;
                        self.pan_frame_elapsed = Duration::ZERO;
                    } else if has_playable_video {
                        self.phase = PlaybackPhase::VideoStream;
                        self.phase_started_at = now;
                        self.video_frame_idx = 0;
                        self.video_frame_elapsed = Duration::ZERO;
                    } else {
                        self.advance_to_next_item(now);
                    }
                }
            }
            PlaybackPhase::SnapshotPan => {
                if !item.panorama_streams.is_empty() {
                    if let Some((frames, delay)) = item.panorama_streams.get(self.pan_image_idx) {
                        self.pan_frame_elapsed += dt;
                        let max_img_dur =
                            Duration::from_secs(self.cfg.snapshot_duration_s.max(2) as u64 + 2);
                        if self.pan_frame_elapsed >= *delay {
                            self.pan_frame_elapsed = Duration::ZERO;
                            self.pan_frame_idx += 1;
                            self.dirty = true;
                        }
                        if self.pan_frame_idx >= frames.len() || phase_elapsed >= max_img_dur {
                            self.pan_image_idx += 1;
                            self.pan_frame_idx = 0;
                            self.pan_frame_elapsed = Duration::ZERO;
                            self.phase_started_at = now;
                            self.dirty = true;
                            if self.pan_image_idx >= item.panorama_streams.len() {
                                if has_playable_video {
                                    self.phase = PlaybackPhase::VideoStream;
                                    self.video_frame_idx = 0;
                                    self.video_frame_elapsed = Duration::ZERO;
                                } else {
                                    self.advance_to_next_item(now);
                                }
                            }
                        }
                    } else {
                        self.advance_to_next_item(now);
                    }
                } else {
                    self.dirty = true;
                    if phase_elapsed >= Duration::from_secs(self.cfg.snapshot_duration_s as u64) {
                        if has_playable_video {
                            self.phase = PlaybackPhase::VideoStream;
                            self.phase_started_at = now;
                            self.video_frame_idx = 0;
                            self.video_frame_elapsed = Duration::ZERO;
                        } else {
                            self.advance_to_next_item(now);
                        }
                    }
                }
            }
            PlaybackPhase::VideoStream => {
                let max_dur = Duration::from_secs(self.cfg.video_max_duration_s as u64);
                if !item.has_video || item.video_frames.is_empty() || phase_elapsed >= max_dur {
                    self.advance_to_next_item(now);
                    return;
                }
                self.video_frame_elapsed += dt;
                if self.video_frame_elapsed >= item.video_frame_delay {
                    self.video_frame_elapsed = Duration::ZERO;
                    self.video_frame_idx += 1;
                    self.dirty = true;
                    if self.video_frame_idx >= item.video_frames.len() {
                        self.advance_to_next_item(now);
                    }
                }
            }
        }
    }

    fn advance_to_next_item(&mut self, now: Instant) {
        self.current_item_idx += 1;
        self.phase_started_at = now;
        self.pan_image_idx = 0;
        self.pan_frame_idx = 0;
        self.pan_frame_elapsed = Duration::ZERO;
        self.video_frame_idx = 0;
        self.video_frame_elapsed = Duration::ZERO;
        self.dirty = true;
        if let Some(next_item) = self.active_items.get(self.current_item_idx) {
            self.phase = self.initial_phase_for_item(next_item);
        }
    }

    pub fn is_finished(&self) -> bool {
        if !self.turn_evaluated {
            return false;
        }
        if self.skip_turn_immediately {
            return true;
        }
        if self.active_items.is_empty() {
            return false;
        }
        self.current_item_idx >= self.active_items.len()
    }

    fn render_waiting_status(&self, matrix: &mut dyn IDrawingSurface, w: i32, h: i32) {
        let accent = self.domain.default_accent_rgb();
        let badge = self.domain.badge_label();
        let badge_w = (badge.len() as i32) * 4 + 4;
        let bx = ((w - badge_w) / 2).max(1);
        fill_round_rect(matrix, bx, 2, badge_w, 7, 1, accent);
        draw_tiny_text(matrix, badge, bx + 2, 3, 0, w, (255, 255, 255));

        let status_msg = if self.cfg.server_url.is_empty() {
            "SET SERVER URL IN WEBUI"
        } else {
            "CONNECTING TO SERVER..."
        };
        let elapsed_ms = self.phase_started_at.elapsed().as_millis();
        draw_scrolling_text(
            matrix,
            status_msg,
            2,
            w - 2,
            (h / 2) - 3,
            1,
            (220, 235, 255),
            elapsed_ms,
            true,
        );
        if h >= 32 && !self.cfg.server_url.is_empty() {
            draw_tiny_text(
                matrix,
                &self.cfg.server_url,
                2,
                h - 7,
                2,
                w - 2,
                (130, 150, 180),
            );
        }
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
            // Tall / XL Landscape (`128x64`, `256x64`)
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
            );
        } else {
            // Classic 128x32 Landscape
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
            );
        }
    }

    pub fn render(&mut self, context: &mut EngineContext) {
        if self.skip_turn_immediately {
            return;
        }
        let matrix = &mut *context.surface;
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;

        let Some(item) = self.active_items.get(self.current_item_idx) else {
            self.render_waiting_status(matrix, w, h);
            self.dirty = false;
            return;
        };

        let phase_ms = self.phase_started_at.elapsed().as_millis();

        match self.phase {
            PlaybackPhase::Card => {
                self.render_card_adaptive(matrix, item, w, h, phase_ms);
            }
            PlaybackPhase::SnapshotPan => {
                if !item.panorama_streams.is_empty() {
                    if let Some((frames, _)) = item.panorama_streams.get(self.pan_image_idx) {
                        if let Some(frame) =
                            frames.get(self.pan_frame_idx).or_else(|| frames.last())
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
                    }
                } else if let Some(snap) = &item.snapshot_pan {
                    let sw = snap.width() as i32;
                    let sh = snap.height() as i32;
                    let max_pan_y = (sh - h).max(0);
                    let total_ms = (self.cfg.snapshot_duration_s.max(1) as u128) * 1000;
                    let progress = (phase_ms as f32 / total_ms as f32).clamp(0.0, 1.0);
                    let pan_y = ((max_pan_y as f32) * progress).round() as i32;

                    for py in 0..h {
                        let sy = (py + pan_y).clamp(0, sh - 1);
                        for px in 0..w.min(sw) {
                            let p = snap.get_pixel(px as u32, sy as u32).0;
                            matrix.set_pixel(px, py, p[0], p[1], p[2]);
                        }
                    }
                }
            }
            PlaybackPhase::VideoStream => {
                if let Some(frame) = item
                    .video_frames
                    .get(self.video_frame_idx)
                    .or_else(|| item.video_frames.last())
                {
                    let fw = (frame.width() as i32).min(w);
                    let fh = (frame.height() as i32).min(h);
                    for py in 0..fh {
                        for px in 0..fw {
                            let p = frame.get_pixel(px as u32, py as u32).0;
                            matrix.set_pixel(px, py, p[0], p[1], p[2]);
                        }
                    }
                    let total_frames = item.video_frames.len().max(1);
                    let bar_w = (((self.video_frame_idx + 1) * (w as usize)) / total_frames) as i32;
                    draw_fast_hline(matrix, 0, h - 1, bar_w.clamp(1, w), item.badge_rgb);
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
            description: "LAN URL of the dmd-media-showcase Docker server (e.g. http://192.168.1.50:8790)",
            default_value: "http://127.0.0.1:8790",
            required: true,
            validation_policy: ValidationPolicy::Accept,
            ..Default::default()
        },
        ConfigField {
            id: "primary_source",
            field_type: ConfigType::Options,
            label: "Primary Scraper Source",
            description: "Preferred primary metadata/media source (other sources are used automatically as fallback)",
            default_value: "auto",
            options: Some(vec![
                ConfigOption { label: "Auto (Best Available + Fallbacks)", value: "auto" },
                ConfigOption { label: "ScreenScraper.fr (api.screenscraper.fr)", value: "screenscraper" },
                ConfigOption { label: "ArcadeItalia ADB (MAME / Neo-Geo / CPS)", value: "arcade_italia" },
                ConfigOption { label: "IGDB / Twitch API v4 (Games)", value: "igdb" },
                ConfigOption { label: "Libretro CDN + Wikipedia Live (Games)", value: "libretro" },
                ConfigOption { label: "TMDB API v3 (Movies / Series)", value: "tmdb" },
                ConfigOption { label: "IMDb + Wikipedia + Dailymotion Live (Movies)", value: "imdb" },
                ConfigOption { label: "TVMaze Public API (Series)", value: "tvmaze" },
                ConfigOption { label: "AnimeThemes.moe (Creditless OP/ED)", value: "animethemes" },
                ConfigOption { label: "Kitsu Edge API (Anime)", value: "kitsu" },
                ConfigOption { label: "Jikan v4 / MyAnimeList (Anime)", value: "jikan" },
            ]),
            validation_policy: ValidationPolicy::FallbackDefault,
            ..Default::default()
        },
        ConfigField {
            id: "scraper_user",
            field_type: ConfigType::String,
            label: "Scraper Username (Optional)",
            description: "Optional account username (e.g. ScreenScraper.fr login)",
            default_value: "",
            validation_policy: ValidationPolicy::Accept,
            ..Default::default()
        },
        ConfigField {
            id: "scraper_pass",
            field_type: ConfigType::String,
            label: "Scraper Password (Optional)",
            description: "Optional account password (e.g. ScreenScraper.fr password)",
            default_value: "",
            validation_policy: ValidationPolicy::Accept,
            ..Default::default()
        },
        ConfigField {
            id: "scraper_api_key",
            field_type: ConfigType::String,
            label: "Scraper API Key (Optional)",
            description: "Optional API key (e.g. TMDB API key, IGDB key, or ScreenScraper devid:devpassword)",
            default_value: "",
            validation_policy: ValidationPolicy::Accept,
            ..Default::default()
        },
        ConfigField {
            id: "items_per_rotation",
            field_type: ConfigType::Integer,
            label: "Items per Rotation",
            description: "Exact number of items preloaded and played per rotation turn",
            default_value: "3",
            min_val: Some("1"),
            max_val: Some("25"),
            step: Some("1"),
            validation_policy: ValidationPolicy::Clamp,
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
            id: "card_duration_s",
            field_type: ConfigType::Integer,
            label: "Min Card Duration (s)",
            description: "Minimum display duration of the card in seconds (automatically extended so full synopsis finishes)",
            default_value: "7",
            min_val: Some("3"),
            max_val: Some("30"),
            step: Some("1"),
            validation_policy: ValidationPolicy::Clamp,
            ..Default::default()
        },
        ConfigField {
            id: "snapshot_duration_s",
            field_type: ConfigType::Integer,
            label: "Duration per Panorama (s)",
            description: "Duration of each top-to-bottom scrolling panorama in seconds",
            default_value: "4",
            min_val: Some("2"),
            max_val: Some("15"),
            step: Some("1"),
            validation_policy: ValidationPolicy::Clamp,
            ..Default::default()
        },
        ConfigField {
            id: "video_max_duration_s",
            field_type: ConfigType::Integer,
            label: "Max Video Duration (s)",
            description: "Maximum playback duration per video clip in seconds (dynamically truncated in O(1))",
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
    fn test_compute_card_poster_dims_multi_format_and_tate() {
        assert_eq!(compute_card_poster_dims(128, 32), (32, 32));
        assert_eq!(compute_card_poster_dims(128, 64), (48, 64));
        assert_eq!(compute_card_poster_dims(256, 64), (48, 64));
        assert_eq!(compute_card_poster_dims(64, 64), (32, 32));
        assert_eq!(compute_card_poster_dims(64, 128), (40, 40));
        assert_eq!(compute_card_poster_dims(32, 128), (24, 24));
    }

    #[test]
    fn test_warming_skip_policy_and_zero_mock_items() {
        let mut runner = MediaShowcaseRunner::new(ShowcaseDomain::Games, 128, 32);
        {
            let mut guard = runner.shared.lock().unwrap();
            guard.readiness = BatchReadiness::Warming;
        }

        runner.activate();
        runner.evaluate_turn_start(3);
        assert!(runner.skip_turn_immediately);
        assert!(runner.is_finished());
        assert!(runner.active_items.is_empty());

        runner.activate();
        runner.evaluate_turn_start(1);
        assert!(!runner.skip_turn_immediately);
        assert!(runner.forced_warming_card_fallback);
        assert!(runner.active_items.is_empty());
        assert!(!runner.is_finished());
        assert_eq!(runner.phase, PlaybackPhase::Card);
    }
}

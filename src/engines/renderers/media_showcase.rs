//! Shared renderer and 2-step LAN protocol runner (`MediaShowcaseRunner`) for the 4
//! DMD Media Showcase engines (`retro_games`, `movie_showcase`, `series_showcase`, `anime_showcase`).
//!
//! Protocol architecture (Zero Webhook Leakage on DMD):
//! 1. Proactive Session Registration (`POST /api/v1/{domain}/session`):
//!    Sent asynchronously on boot (`initialize`) or config update (`on_config_changed`)
//!    with the exact `count = items_per_rotation`.
//! 2. Fast Pre-Rotation Check (`GET /api/v1/{domain}/batch?session_id=...`):
//!    Evaluated when the engine's rotation turn starts (`activate` / first `update`).
//!    - If `status == "ready"`: plays the `N` preloaded items (`Card` -> `SnapshotPan` -> `VideoStream`),
//!      signals `is_finished() = true`, and triggers background pre-fetch of `NextBatch`.
//!    - If `status == "warming"`:
//!      * When multiple engines are in rotation (`rotation.len() > 1`), immediately sets
//!        `skip_turn_immediately = true` (`is_finished() = true`) so the DMD skips this turn
//!        with zero freeze while the server finishes rendering in its bounded worker pool.
//!      * When this is the only engine in rotation (`rotation.len() <= 1`), temporarily displays
//!        the rich fallback card (`Card` + `SnapshotPan`) until the video stream becomes ready.

use crate::core::engine_contract::{EngineConfig, EngineContext};
use crate::core::i18n;
use crate::core::matrix::IDrawingSurface;
use crate::engines::dashboard::font::draw_text_scaled;
use crate::engines::renderers::primitives::{
    draw_fast_hline, draw_scrolling_text, fill_round_rect,
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

    pub const fn default_accent_rgb(self) -> (u8, u8, u8) {
        match self {
            ShowcaseDomain::Games => (255, 109, 0),
            ShowcaseDomain::Movies => (0, 229, 255),
            ShowcaseDomain::Series => (229, 57, 53),
            ShowcaseDomain::Anime => (255, 64, 129),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShowcaseConfig {
    pub server_url: String,
    pub items_per_rotation: u32,
    pub lang: String,
    pub show_poster: bool,
    pub show_synopsis: bool,
    pub show_snapshot: bool,
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
            server_url: "http://127.0.0.1:8080".to_string(),
            items_per_rotation: 3,
            lang: "system".to_string(),
            show_poster: true,
            show_synopsis: true,
            show_snapshot: true,
            show_video: true,
            card_duration_s: 7,
            snapshot_duration_s: 4,
            video_max_duration_s: 6,
            video_fps: 15,
            video_mode: "auto_action".to_string(),
            color_boost: true,
            systems: "mame,neogeo,snes,megadrive,psx".to_string(),
            genres: String::new(),
            decades: String::new(),
            min_rating: 6.5,
            video_type: "any".to_string(),
        }
    }
}

impl ShowcaseConfig {
    pub fn from_engine_config(cfg: &dyn EngineConfig, domain: ShowcaseDomain) -> Self {
        let mut out = Self::default();
        let raw_url = cfg.get_string("server_url", "http://127.0.0.1:8080");
        out.server_url = raw_url.trim().trim_end_matches('/').to_string();
        if out.server_url.is_empty() {
            out.server_url = "http://127.0.0.1:8080".to_string();
        }
        out.items_per_rotation = cfg.get_int("items_per_rotation", 3).clamp(1, 25) as u32;
        out.lang = cfg.get_string("lang", "system");
        out.show_poster = cfg.get_bool("show_poster", true);
        out.show_synopsis = cfg.get_bool("show_synopsis", true);
        out.show_snapshot = cfg.get_bool("show_snapshot", true);
        out.show_video = cfg.get_bool("show_video", true);
        out.card_duration_s = cfg.get_int("card_duration_s", 7).clamp(3, 30) as u32;
        out.snapshot_duration_s = cfg.get_int("snapshot_duration_s", 4).clamp(2, 15) as u32;
        out.video_max_duration_s = cfg.get_int("video_max_duration_s", 6).clamp(2, 30) as u32;
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
        out.systems = cfg.get_string("systems", "mame,neogeo,snes,megadrive,psx");
        out.genres = cfg.get_string("genres", "");
        out.decades = cfg.get_string("decades", "");
        out.min_rating = cfg
            .get_string("min_rating", "6.5")
            .parse::<f32>()
            .unwrap_or(6.5)
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMediaAssets {
    pub poster_png: String,
    pub poster_rgb565: String,
    pub marquee_png: String,
    pub marquee_rgb565: String,
    pub snapshot_pan_png: String,
    pub snapshot_pan_rgb565: String,
    pub video_gif: String,
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
    pub video_frames: Vec<RgbImage>,
    pub video_frame_delay: Duration,
    pub video_ready: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchReadiness {
    OfflineDemo,
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
            readiness: BatchReadiness::OfflineDemo,
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
            width: width.max(64),
            height: height.max(32),
            shared: Arc::new(Mutex::new(SharedShowcaseBatch::default())),
            worker_busy: Arc::new(AtomicBool::new(false)),
            config_gen: Arc::new(AtomicU64::new(1)),
            active_items: Vec::new(),
            current_item_idx: 0,
            phase: PlaybackPhase::Card,
            phase_started_at: now,
            video_frame_idx: 0,
            video_frame_elapsed: Duration::ZERO,
            last_tick: now,
            turn_evaluated: false,
            skip_turn_immediately: false,
            forced_warming_card_fallback: false,
            dirty: true,
        }
    }

    pub fn build_offline_demo_item(&self) -> PreparedShowcaseItem {
        let lang = self.cfg.effective_lang(&self.system_lang);
        let (title, year, badge, rating, syn_key) = match self.domain {
            ShowcaseDomain::Games => (
                "Metal Slug",
                "1996",
                "NEOGEO",
                "9.4",
                "showcase_demo_games_syn",
            ),
            ShowcaseDomain::Movies => (
                "Blade Runner",
                "1982",
                "SCIFI",
                "8.9",
                "showcase_demo_movies_syn",
            ),
            ShowcaseDomain::Series => (
                "Stranger Things",
                "2016",
                "SERIES",
                "8.7",
                "showcase_demo_series_syn",
            ),
            ShowcaseDomain::Anime => (
                "Dragon Ball Z",
                "1989",
                "SHONEN",
                "9.1",
                "showcase_demo_anime_syn",
            ),
        };
        let accent = self.domain.default_accent_rgb();
        let poster_dim = self.height.min(self.width);
        let mut poster = RgbImage::new(poster_dim, poster_dim);
        for y in 0..poster_dim {
            for x in 0..poster_dim {
                let border = x == 0 || y == 0 || x == poster_dim - 1 || y == poster_dim - 1;
                if border {
                    poster.put_pixel(x, y, image::Rgb([accent.0, accent.1, accent.2]));
                } else {
                    let shade = ((y * 160) / poster_dim.max(1)) as u8;
                    poster.put_pixel(
                        x,
                        y,
                        image::Rgb([
                            ((accent.0 as u16 * shade as u16) / 255) as u8,
                            ((accent.1 as u16 * shade as u16) / 255) as u8,
                            ((accent.2 as u16 * shade as u16) / 255) as u8,
                        ]),
                    );
                }
            }
        }

        let pan_h = self.height * 2 + 8;
        let mut snap = RgbImage::new(self.width, pan_h);
        for y in 0..pan_h {
            for x in 0..self.width {
                let grid = (x % 16 == 0) || (y % 12 == 0);
                if grid {
                    snap.put_pixel(x, y, image::Rgb([accent.0 / 2, accent.1 / 2, accent.2 / 2]));
                } else {
                    snap.put_pixel(x, y, image::Rgb([8, 10, 18]));
                }
            }
        }

        PreparedShowcaseItem {
            id: format!("demo_{}", self.domain.as_api_slug()),
            title: title.to_string(),
            year: year.to_string(),
            badge: badge.to_string(),
            badge_rgb: accent,
            subtitle: i18n::t(lang, "showcase_offline_sub").to_string(),
            rating: rating.to_string(),
            synopsis: i18n::t(lang, syn_key).to_string(),
            poster: Some(poster),
            snapshot_pan: Some(snap),
            video_frames: Vec::new(),
            video_frame_delay: Duration::from_millis(66),
            video_ready: false,
        }
    }

    /// Spawn background session registration (`POST /api/v1/{domain}/session`) and asset pre-fetching.
    pub fn spawn_background_sync(&self, register_new_session: bool) {
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
                .connect_timeout(Duration::from_millis(900))
                .timeout(Duration::from_millis(3500))
                .build()
            {
                Ok(c) => c,
                Err(_) => return,
            };

            let base = cfg.server_url.trim_end_matches('/');
            let slug = domain.as_api_slug();

            let existing_session_id = {
                let guard = shared.lock().unwrap();
                guard.session_id.clone()
            };

            let mut session_id = if !register_new_session {
                existing_session_id
            } else {
                None
            };

            if session_id.is_none() {
                let reg_url = format!("{}/api/v1/{}/session", base, slug);
                let body = serde_json::json!({
                    "client_id": format!("rpi_{}", slug),
                    "count": cfg.items_per_rotation,
                    "lang": lang,
                    "width": width,
                    "height": height,
                    "include_poster": cfg.show_poster,
                    "include_synopsis": cfg.show_synopsis,
                    "include_snapshot": cfg.show_snapshot,
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
                });

                match client.post(&reg_url).json(&body).send() {
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

            let Some(ses_id) = session_id else {
                return;
            };

            let batch_url = format!("{}/api/v1/{}/batch?session_id={}", base, slug, ses_id);
            let Ok(resp) = client.get(&batch_url).send() else {
                return;
            };
            if !resp.status().is_success() {
                return;
            }
            let Ok(batch_data) = resp.json::<ApiBatchStatusResponse>() else {
                return;
            };

            let is_ready = batch_data.status.eq_ignore_ascii_case("ready");
            let poster_dim = height.min(width);
            let pan_h = height * 2 + 8;

            let mut prepared = Vec::with_capacity(batch_data.items.len());
            for api_item in batch_data.items {
                let badge_rgb = parse_hex_rgb(&api_item.badge_color, domain.default_accent_rgb());

                let poster = if cfg.show_poster || !is_ready {
                    let url = format!("{}{}", base, api_item.assets.poster_rgb565);
                    client
                        .get(&url)
                        .send()
                        .ok()
                        .and_then(|r| r.bytes().ok())
                        .and_then(|b| decode_rgb565_le_image(&b, poster_dim, poster_dim))
                } else {
                    None
                };

                let snapshot_pan = if cfg.show_snapshot || !is_ready {
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

                if cfg.show_video && (is_ready || api_item.video_ready) {
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

                let item_video_ok = !video_frames.is_empty();
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
                    video_frames,
                    video_frame_delay,
                    video_ready: item_video_ok,
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
        self.width = context.surface.width().max(64);
        self.height = context.surface.height().max(32);
        self.system_lang = context.config.settings.read().system.lang.clone();
        self.cfg = ShowcaseConfig::from_engine_config(config, self.domain);
        self.active_items = vec![self.build_offline_demo_item()];
        self.config_gen.fetch_add(1, Ordering::SeqCst);
        self.spawn_background_sync(true);
    }

    pub fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        let new_cfg = ShowcaseConfig::from_engine_config(config, self.domain);
        if new_cfg != self.cfg {
            self.cfg = new_cfg;
            self.config_gen.fetch_add(1, Ordering::SeqCst);
            self.spawn_background_sync(true);
        }
    }

    pub fn activate(&mut self) {
        let now = Instant::now();
        self.current_item_idx = 0;
        self.video_frame_idx = 0;
        self.video_frame_elapsed = Duration::ZERO;
        self.phase_started_at = now;
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

    fn initial_phase_for_item(&self, item: &PreparedShowcaseItem) -> PlaybackPhase {
        if self.forced_warming_card_fallback || self.cfg.show_poster || self.cfg.show_synopsis {
            PlaybackPhase::Card
        } else if self.cfg.show_snapshot && item.snapshot_pan.is_some() {
            PlaybackPhase::SnapshotPan
        } else if self.cfg.show_video && item.video_ready && !item.video_frames.is_empty() {
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
            BatchReadiness::Warming => {
                if rotation_slot_count > 1 {
                    // Multiple engines in rotation: skip this turn cleanly so the DMD never freezes
                    // or shows an incomplete video while the server's worker pool finishes.
                    warn!(
                        "[Showcase:{}] Batch still warming on server and rotation has {} slots -> skipping turn cleanly",
                        self.domain.as_api_slug(),
                        rotation_slot_count
                    );
                    self.skip_turn_immediately = true;
                    self.spawn_background_sync(false);
                } else {
                    // Only engine in rotation: even if configured in Video-Only mode,
                    // temporarily display the fallback rich card + snapshot until video is ready.
                    self.skip_turn_immediately = false;
                    self.forced_warming_card_fallback = true;
                    if !items.is_empty() {
                        self.active_items = items
                            .into_iter()
                            .take(self.cfg.items_per_rotation as usize)
                            .collect();
                    } else {
                        self.active_items = vec![self.build_offline_demo_item()];
                    }
                    self.phase = PlaybackPhase::Card;
                    self.spawn_background_sync(false);
                }
            }
            BatchReadiness::OfflineDemo | BatchReadiness::Ready => {
                self.skip_turn_immediately = false;
                self.forced_warming_card_fallback = true;
                self.active_items = vec![self.build_offline_demo_item()];
                self.phase = PlaybackPhase::Card;
                self.spawn_background_sync(false);
            }
        }
        self.phase_started_at = Instant::now();
    }

    pub fn update(&mut self, context: &mut EngineContext) {
        let now = Instant::now();
        let dt = now.saturating_duration_since(self.last_tick);
        self.last_tick = now;

        let new_w = context.surface.width().max(64);
        let new_h = context.surface.height().max(32);
        if new_w != self.width || new_h != self.height {
            self.width = new_w;
            self.height = new_h;
        }

        if !self.turn_evaluated {
            let rot_len = context.config.settings.read().rotation.len();
            self.evaluate_turn_start(rot_len);
            self.dirty = true;
            return;
        }

        if self.skip_turn_immediately || self.current_item_idx >= self.active_items.len() {
            return;
        }

        // If we were in warming fallback (single engine in rotation), check if videos just finished
        if self.forced_warming_card_fallback {
            let (readiness, ready_items) = {
                let guard = self.shared.lock().unwrap();
                (guard.readiness, guard.items.clone())
            };
            if readiness == BatchReadiness::Ready && !ready_items.is_empty() {
                self.active_items = ready_items
                    .into_iter()
                    .take(self.cfg.items_per_rotation as usize)
                    .collect();
                self.forced_warming_card_fallback = false;
                if !self.cfg.show_poster && !self.cfg.show_synopsis && self.cfg.show_video {
                    self.phase = PlaybackPhase::VideoStream;
                    self.phase_started_at = now;
                    self.video_frame_idx = 0;
                    self.video_frame_elapsed = Duration::ZERO;
                }
            }
        }

        let phase_elapsed = now.saturating_duration_since(self.phase_started_at);
        let item = &self.active_items[self.current_item_idx];

        match self.phase {
            PlaybackPhase::Card => {
                self.dirty = true;
                if phase_elapsed >= Duration::from_secs(self.cfg.card_duration_s as u64) {
                    if (self.cfg.show_snapshot || self.forced_warming_card_fallback)
                        && item.snapshot_pan.is_some()
                    {
                        self.phase = PlaybackPhase::SnapshotPan;
                        self.phase_started_at = now;
                    } else if self.cfg.show_video
                        && item.video_ready
                        && !item.video_frames.is_empty()
                    {
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
                self.dirty = true;
                if phase_elapsed >= Duration::from_secs(self.cfg.snapshot_duration_s as u64) {
                    if self.cfg.show_video && item.video_ready && !item.video_frames.is_empty() {
                        self.phase = PlaybackPhase::VideoStream;
                        self.phase_started_at = now;
                        self.video_frame_idx = 0;
                        self.video_frame_elapsed = Duration::ZERO;
                    } else {
                        self.advance_to_next_item(now);
                    }
                }
            }
            PlaybackPhase::VideoStream => {
                let max_dur = Duration::from_secs(self.cfg.video_max_duration_s as u64);
                if item.video_frames.is_empty() || phase_elapsed >= max_dur {
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
        self.skip_turn_immediately || self.current_item_idx >= self.active_items.len()
    }

    pub fn render(&mut self, context: &mut EngineContext) {
        if self.skip_turn_immediately {
            return;
        }
        let Some(item) = self.active_items.get(self.current_item_idx) else {
            return;
        };

        let matrix = &mut *context.surface;
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;
        let phase_ms = self.phase_started_at.elapsed().as_millis();

        match self.phase {
            PlaybackPhase::Card => {
                // 1. Left Poster (h x h) if enabled or warming fallback
                let text_min_x = if (self.cfg.show_poster || self.forced_warming_card_fallback)
                    && item.poster.is_some()
                {
                    let poster = item.poster.as_ref().unwrap();
                    let pw = (poster.width() as i32).min(h);
                    let ph = (poster.height() as i32).min(h);
                    for py in 0..ph {
                        for px in 0..pw {
                            let p = poster.get_pixel(px as u32, py as u32).0;
                            matrix.set_pixel(px, py, p[0], p[1], p[2]);
                        }
                    }
                    pw + 2
                } else {
                    2
                };

                // 2. Top-Right: Colored Domain/System Pill Badge + Year + Rating
                let badge_txt = if item.badge.len() > 8 {
                    &item.badge[..8]
                } else {
                    &item.badge
                };
                let badge_w = (badge_txt.chars().count() as i32) * 4 + 3;
                fill_round_rect(matrix, text_min_x, 1, badge_w, 7, 1, item.badge_rgb);
                draw_tiny_text(
                    matrix,
                    badge_txt,
                    text_min_x + 2,
                    2,
                    text_min_x,
                    text_min_x + badge_w,
                    (255, 255, 255),
                );

                let meta_x = text_min_x + badge_w + 3;
                let meta_label = format!("{} *{}", item.year, item.rating);
                draw_tiny_text(matrix, &meta_label, meta_x, 2, meta_x, w - 1, (255, 215, 0));

                // 3. Middle: Marquee-scrolling Title in crisp 5x7 font
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

                // Subtle separator line
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

                // 4. Bottom: Localized Synopsis or Subtitle
                let bottom_text = if !item.synopsis.is_empty()
                    && (self.cfg.show_synopsis || self.forced_warming_card_fallback)
                {
                    item.synopsis.as_str()
                } else if self.forced_warming_card_fallback {
                    let lang = self.cfg.effective_lang(&self.system_lang);
                    i18n::t(lang, "showcase_warming")
                } else {
                    item.subtitle.as_str()
                };

                draw_scrolling_text(
                    matrix,
                    bottom_text,
                    text_min_x,
                    w - 1,
                    22,
                    1,
                    (190, 225, 255),
                    phase_ms,
                    false,
                );
            }
            PlaybackPhase::SnapshotPan => {
                if let Some(snap) = &item.snapshot_pan {
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
                            // Darken bottom 8 rows slightly so the title overlay pops
                            if py >= h - 8 {
                                matrix.set_pixel(px, py, p[0] / 3, p[1] / 3, p[2] / 3);
                            } else {
                                matrix.set_pixel(px, py, p[0], p[1], p[2]);
                            }
                        }
                    }
                    draw_text_scaled(
                        matrix,
                        &item.title,
                        2,
                        h - 7,
                        0,
                        w,
                        0,
                        h,
                        1,
                        (255, 255, 255),
                    );
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
                    // 1-pixel bottom progress bar
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
            description: "LAN URL of the dmd-media-showcase Docker server (e.g. http://192.168.1.50:8080)",
            default_value: "http://127.0.0.1:8080",
            required: true,
            validation_policy: ValidationPolicy::Accept,
            ..Default::default()
        },
        ConfigField {
            id: "items_per_rotation",
            field_type: ConfigType::Integer,
            label: "Items per Rotation",
            description: "Exact number of items (and videos) preloaded and played per rotation turn",
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
            label: "Show Poster / Cover",
            description: "Display the 32x32 poster/boxart on the metadata card",
            default_value: "true",
            validation_policy: ValidationPolicy::FallbackDefault,
            ..Default::default()
        },
        ConfigField {
            id: "show_synopsis",
            field_type: ConfigType::Boolean,
            label: "Show Synopsis",
            description: "Scroll the localized synopsis on the metadata card",
            default_value: "true",
            validation_policy: ValidationPolicy::FallbackDefault,
            ..Default::default()
        },
        ConfigField {
            id: "show_snapshot",
            field_type: ConfigType::Boolean,
            label: "Show Pan-Scan Snapshot",
            description: "Display the vertical Ken Burns pan-scan backdrop/snapshot phase",
            default_value: "true",
            validation_policy: ValidationPolicy::FallbackDefault,
            ..Default::default()
        },
        ConfigField {
            id: "show_video",
            field_type: ConfigType::Boolean,
            label: "Show Video Stream",
            description: "Play the full-screen animated video clip (.rgb565.stream)",
            default_value: "true",
            validation_policy: ValidationPolicy::FallbackDefault,
            ..Default::default()
        },
        ConfigField {
            id: "card_duration_s",
            field_type: ConfigType::Integer,
            label: "Card Duration (s)",
            description: "Display duration of the metadata card phase in seconds",
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
            label: "Snapshot Pan Duration (s)",
            description: "Duration of the pan-scan snapshot phase in seconds",
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
            default_value: "6",
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
            label: "Video Crop & Color Preset",
            description: "Conversion preset applied by dmd-gif-converter on the server",
            default_value: default_video_mode,
            options: Some(vec![
                ConfigOption { label: "AI Auto-Action Crop", value: "auto_action" },
                ConfigOption { label: "Pixel Art (Retro Crisp)", value: "pixel_art" },
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
        // 2x2 image in RGB565 LE (pure red 0xF800)
        let mut raw = Vec::new();
        for _ in 0..4 {
            raw.extend_from_slice(&0xF800u16.to_le_bytes());
        }
        let img = decode_rgb565_le_image(&raw, 2, 2).expect("valid 2x2 rgb565");
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 2);
        assert!(img.get_pixel(0, 0).0[0] >= 248);
        assert_eq!(img.get_pixel(0, 0).0[1], 0);
        assert_eq!(img.get_pixel(0, 0).0[2], 0);

        // Build a 2-frame DMDS stream
        let mut stream = Vec::new();
        stream.extend_from_slice(DMDS_MAGIC);
        stream.push(1); // version
        stream.push(15); // fps
        stream.extend_from_slice(&2u16.to_le_bytes()); // width = 2
        stream.extend_from_slice(&2u16.to_le_bytes()); // height = 2
        stream.extend_from_slice(&2u16.to_le_bytes()); // total_frames = 2
        stream.extend_from_slice(&66u16.to_le_bytes()); // frame_delay_ms = 66
        stream.extend_from_slice(&0u16.to_le_bytes()); // reserved
        stream.extend_from_slice(&raw);
        stream.extend_from_slice(&raw);

        let (frames, delay) = decode_rgb565_stream(&stream).expect("valid DMDS stream");
        assert_eq!(frames.len(), 2);
        assert_eq!(delay, Duration::from_millis(66));
    }

    #[test]
    fn test_warming_skip_policy_multi_vs_single_rotation() {
        let mut runner = MediaShowcaseRunner::new(ShowcaseDomain::Games, 128, 32);
        {
            let mut guard = runner.shared.lock().unwrap();
            guard.readiness = BatchReadiness::Warming;
        }

        // Case 1: Multiple engines in rotation (rotation_slot_count = 3) -> immediate skip
        runner.activate();
        runner.evaluate_turn_start(3);
        assert!(runner.skip_turn_immediately);
        assert!(runner.is_finished());

        // Case 2: Only engine in rotation (rotation_slot_count = 1) -> fallback to Card
        runner.activate();
        runner.evaluate_turn_start(1);
        assert!(!runner.skip_turn_immediately);
        assert!(runner.forced_warming_card_fallback);
        assert!(!runner.is_finished());
        assert_eq!(runner.phase, PlaybackPhase::Card);
    }
}

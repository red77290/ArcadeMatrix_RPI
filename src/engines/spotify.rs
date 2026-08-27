use crate::api::spotify::{SpotifyClient, SpotifyNowPlaying};
use crate::core::build_info::VERSION;
use crate::core::engine_contract::{
    Capabilities, ConfigField, ConfigSchema, ConfigType, Engine, EngineConfig, EngineContext,
    EngineDescriptor, EngineError, EngineMetadata, Requirements, ValidationPolicy,
};
use crate::core::matrix::MatrixBackend;
use crate::core::registry::ENGINES;
use crate::engines::renderers::base_renderer::ArcadeFont;
use crate::engines::renderers::BaseRenderer;
use image::{imageops, Rgb, RgbImage};
use linkme::distributed_slice;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub struct SpotifyEngine {
    base_renderer: BaseRenderer,
    client_id: String,
    client_secret: String,
    refresh_token: String,
    show_album_art: bool,
    show_progress: bool,
    show_volume: bool,
    show_visualizer: bool,

    // Live state shared with background poll thread
    now_playing: Arc<Mutex<SpotifyNowPlaying>>,
    is_running: Arc<AtomicBool>,
    last_image_url: String,
    cached_cover: Option<RgbImage>,

    // Animation & marquee timers
    anim_frame: u32,
    marquee_offset: i32,
    last_marquee_tick: Instant,
    last_anim_tick: Instant,
}

impl SpotifyEngine {
    pub fn new() -> Self {
        Self {
            base_renderer: BaseRenderer::new(),
            client_id: String::new(),
            client_secret: String::new(),
            refresh_token: String::new(),
            show_album_art: true,
            show_progress: true,
            show_volume: true,
            show_visualizer: true,
            now_playing: Arc::new(Mutex::new(SpotifyNowPlaying::default())),
            is_running: Arc::new(AtomicBool::new(false)),
            last_image_url: String::new(),
            cached_cover: None,
            anim_frame: 0,
            marquee_offset: 0,
            last_marquee_tick: Instant::now(),
            last_anim_tick: Instant::now(),
        }
    }

    fn apply_config(&mut self, config: &dyn EngineConfig) {
        self.client_id = config.get_string("client_id", "");
        self.client_secret = config.get_string("client_secret", "");
        self.refresh_token = config.get_string("refresh_token", "");
        self.show_album_art = config.get_bool("show_album_art", true);
        self.show_progress = config.get_bool("show_progress", true);
        self.show_volume = config.get_bool("show_volume", true);
        self.show_visualizer = config.get_bool("show_visualizer", true);
    }

    fn start_background_worker(&mut self) {
        if self.is_running.load(Ordering::Relaxed) {
            return;
        }

        self.is_running.store(true, Ordering::Relaxed);
        let status_arc = Arc::clone(&self.now_playing);
        let running_arc = Arc::clone(&self.is_running);
        let cid = self.client_id.clone();
        let sec = self.client_secret.clone();
        let ref_tok = self.refresh_token.clone();

        thread::spawn(move || {
            let mut client = SpotifyClient::new(&cid, &sec, &ref_tok);

            while running_arc.load(Ordering::Relaxed) {
                if let Ok(st) = client.get_currently_playing() {
                    if let Ok(mut guard) = status_arc.lock() {
                        *guard = st;
                    }
                }

                thread::sleep(Duration::from_millis(1500));
            }
        });
    }

    fn fetch_cover_art_if_needed(&mut self, url_opt: &Option<String>, target_size: u32) {
        let url = match url_opt {
            Some(u) if !u.is_empty() => u.clone(),
            _ => {
                self.last_image_url.clear();
                self.cached_cover = None;
                return;
            }
        };

        if url == self.last_image_url && self.cached_cover.is_some() {
            return;
        }

        self.last_image_url = url.clone();
        if let Ok(resp) = reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(2000))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new())
            .get(&url)
            .send()
        {
            if let Ok(bytes) = resp.bytes() {
                if let Ok(img) = image::load_from_memory(&bytes) {
                    let rgb = img.to_rgb8();
                    let resized = imageops::resize(
                        &rgb,
                        target_size,
                        target_size,
                        imageops::FilterType::Triangle,
                    );
                    self.cached_cover = Some(resized);
                    return;
                }
            }
        }

        self.cached_cover = None;
    }
}

impl Engine for SpotifyEngine {
    fn initialize(
        &mut self,
        _ctx: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        self.apply_config(config);
        self.start_background_worker();
        Ok(())
    }

    fn activate(&mut self) {
        self.start_background_worker();
    }

    fn update(&mut self, ctx: &mut EngineContext) {
        let (img_url, is_playing) = if let Ok(guard) = self.now_playing.lock() {
            (guard.image_url.clone(), guard.is_playing)
        } else {
            (None, false)
        };

        if self.show_album_art {
            let target_size = if ctx.matrix.height() >= 64 { 52 } else { 24 };
            self.fetch_cover_art_if_needed(&img_url, target_size);
        }

        // Marquee scrolling tick (~35ms per pixel)
        if self.last_marquee_tick.elapsed() >= Duration::from_millis(35) {
            self.marquee_offset = self.marquee_offset.wrapping_add(1);
            self.last_marquee_tick = Instant::now();
        }

        // Animation visualizer tick (~70ms)
        if is_playing && self.last_anim_tick.elapsed() >= Duration::from_millis(70) {
            self.anim_frame = (self.anim_frame + 1) % 1000;
            self.last_anim_tick = Instant::now();
        }
    }

    fn render(&mut self, ctx: &mut EngineContext) {
        let (status, has_data) = if let Ok(guard) = self.now_playing.lock() {
            (guard.clone(), guard.is_active && !guard.title.is_empty())
        } else {
            (SpotifyNowPlaying::default(), false)
        };

        let w = ctx.matrix.width() as i32;
        let h = ctx.matrix.height() as i32;

        let font = self.base_renderer.font();

        if !has_data {
            BaseRenderer::draw_text_at(
                ctx.matrix,
                "Spotify",
                &font,
                1.0,
                (w / 2) - 20,
                (h / 2) - 8,
                (30, 215, 96),
                (0, 0, 0),
            );
            BaseRenderer::draw_text_at(
                ctx.matrix,
                "No music playing",
                &font,
                1.0,
                (w / 2) - 34,
                (h / 2) + 2,
                (140, 140, 140),
                (0, 0, 0),
            );
            return;
        }

        let mut text_x = 2;

        // 1. Draw Cover Art if present
        if self.show_album_art {
            if let Some(ref cover) = self.cached_cover {
                let img_w = cover.width() as i32;
                let img_h = cover.height() as i32;
                let img_x = 1;
                let img_y = if h >= 64 {
                    (h - 4 - img_h) / 2
                } else {
                    ((h - 3 - img_h) / 2).max(1)
                };

                // Outer subtle border
                for bx in 0..img_w + 2 {
                    ctx.matrix.set_pixel(img_x - 1 + bx, img_y - 1, 40, 40, 50);
                    ctx.matrix
                        .set_pixel(img_x - 1 + bx, img_y + img_h, 40, 40, 50);
                }
                for by in 0..img_h + 2 {
                    ctx.matrix.set_pixel(img_x - 1, img_y - 1 + by, 40, 40, 50);
                    ctx.matrix
                        .set_pixel(img_x + img_w, img_y - 1 + by, 40, 40, 50);
                }

                // Render pixels
                for py in 0..cover.height() {
                    for px in 0..cover.width() {
                        let Rgb([r, g, b]) = *cover.get_pixel(px, py);
                        ctx.matrix
                            .set_pixel(img_x + px as i32, img_y + py as i32, r, g, b);
                    }
                }
                text_x = img_x + img_w + 3;
            }
        }

        // 2. Right Reserved Zone (Visualizer / Volume)
        let has_art = self.show_album_art && self.cached_cover.is_some();
        let is_compact_screen = w <= 64;

        let right_reserved = if self.show_visualizer && status.is_playing {
            if is_compact_screen && has_art {
                8 // Compact 3-bar visualizer (width 6px + 2px margin)
            } else {
                15 // Standard 4-bar visualizer (width 12px + 3px margin)
            }
        } else if self.show_volume && status.volume_percent > 0 {
            24
        } else {
            2
        };

        // Strict Viewport for Text
        let clip_min_x = text_x;
        let clip_max_x = w - right_reserved;
        let avail_w = (clip_max_x - clip_min_x).max(16);

        // 3. Continuous Circular (Seamless Looping) Marquee
        let render_marquee = |matrix: &mut dyn MatrixBackend,
                              text: &str,
                              font: &ArcadeFont<'_>,
                              y: i32,
                              color: (u8, u8, u8),
                              offset: i32| {
            let (_, text_w, _) = font.get_pixel_map(text, 1.0);
            if text_w <= avail_w {
                BaseRenderer::draw_text_clipped(
                    matrix,
                    text,
                    font,
                    1.0,
                    clip_min_x,
                    y,
                    clip_min_x,
                    clip_max_x,
                    color,
                    (0, 0, 0),
                );
            } else {
                let gap = 20; // 20px seamless spacing between loop repetitions
                let total_w = text_w + gap;
                let dx = offset.rem_euclid(total_w);

                // Draw primary text instance
                let draw_x1 = clip_min_x - dx;
                BaseRenderer::draw_text_clipped(
                    matrix,
                    text,
                    font,
                    1.0,
                    draw_x1,
                    y,
                    clip_min_x,
                    clip_max_x,
                    color,
                    (0, 0, 0),
                );

                // Draw trailing secondary instance for circular looping
                let draw_x2 = draw_x1 + total_w;
                if draw_x2 < clip_max_x {
                    BaseRenderer::draw_text_clipped(
                        matrix,
                        text,
                        font,
                        1.0,
                        draw_x2,
                        y,
                        clip_min_x,
                        clip_max_x,
                        color,
                        (0, 0, 0),
                    );
                }
            }
        };

        let y_title = if h >= 64 { 8 } else { 3 };
        let y_artist = if h >= 64 { 22 } else { 13 };

        // Draw Title
        render_marquee(
            ctx.matrix,
            &status.title,
            &font,
            y_title,
            (255, 255, 255),
            self.marquee_offset,
        );

        // Draw Artist / Album
        let artist_display = if !status.artist.is_empty() {
            status.artist.clone()
        } else if !status.album.is_empty() {
            status.album.clone()
        } else {
            "Spotify".to_string()
        };

        render_marquee(
            ctx.matrix,
            &artist_display,
            &font,
            y_artist,
            (30, 215, 96), // Spotify Green
            self.marquee_offset / 2,
        );

        // 4. Draw Beat Visualizer on the right with dynamic colors
        if self.show_visualizer && status.is_playing {
            let eq_base_y = if h >= 64 { 44 } else { 21 };

            if is_compact_screen && has_art {
                // 3 compact bars (2px wide each, 1px gap)
                let eq_x = w - 7;
                let bar_heights = [
                    ((self.anim_frame.wrapping_mul(4)) % 7 + 2) as i32,
                    ((self.anim_frame.wrapping_mul(6)) % 9 + 3) as i32,
                    ((self.anim_frame.wrapping_mul(3)) % 6 + 2) as i32,
                ];

                for (i, &bh) in bar_heights.iter().enumerate() {
                    let bx = eq_x + (i as i32 * 2);
                    for by in 0..bh {
                        let py = eq_base_y - by;
                        if py >= 0 {
                            let color = if by > 6 {
                                (255, 60, 60)
                            } else if by > 3 {
                                (255, 220, 0)
                            } else {
                                (30, 215, 96)
                            };
                            ctx.matrix.set_pixel(bx, py, color.0, color.1, color.2);
                        }
                    }
                }
            } else {
                // 4 wide bars (2px wide + 1px spacing)
                let eq_x = w - 13;
                let bar_heights = [
                    ((self.anim_frame.wrapping_mul(4)) % 8 + 2) as i32,
                    ((self.anim_frame.wrapping_mul(6)) % 11 + 3) as i32,
                    ((self.anim_frame.wrapping_mul(3)) % 9 + 2) as i32,
                    ((self.anim_frame.wrapping_mul(5)) % 7 + 2) as i32,
                ];

                for (i, &bh) in bar_heights.iter().enumerate() {
                    let bx = eq_x + (i as i32 * 3);
                    for by in 0..bh {
                        let py = eq_base_y - by;
                        if py >= 0 {
                            let color = if by > 7 {
                                (255, 60, 60)
                            } else if by > 4 {
                                (255, 220, 0)
                            } else {
                                (30, 215, 96)
                            };
                            ctx.matrix.set_pixel(bx, py, color.0, color.1, color.2);
                            ctx.matrix.set_pixel(bx + 1, py, color.0, color.1, color.2);
                        }
                    }
                }
            }
        } else if self.show_volume && status.volume_percent > 0 {
            // 5. Draw Volume (Top-Right, right-aligned)
            let vol_str = format!("{}%", status.volume_percent);
            let v_x = w - (vol_str.len() as i32 * 6) - 1;
            BaseRenderer::draw_text_at(
                ctx.matrix,
                &vol_str,
                &font,
                1.0,
                v_x,
                y_title,
                (180, 180, 180),
                (0, 0, 0),
            );
        }

        // 6. Draw Progress Bar (Bottom 2 pixels)
        if self.show_progress && status.duration_ms > 0 {
            let progress = (status.progress_ms as f32 / status.duration_ms as f32).clamp(0.0, 1.0);
            let bar_w = ((w - 2) as f32 * progress) as i32;

            // Background line
            for x in 1..w - 1 {
                ctx.matrix.set_pixel(x, h - 2, 30, 35, 30);
                ctx.matrix.set_pixel(x, h - 1, 18, 22, 18);
            }
            // Active progress fill (Spotify Green)
            for x in 1..=bar_w {
                ctx.matrix.set_pixel(x, h - 2, 30, 215, 96);
                ctx.matrix.set_pixel(x, h - 1, 20, 150, 65);
            }
        }
    }

    fn deactivate(&mut self) {
        self.is_running.store(false, Ordering::Relaxed);
    }

    fn is_realtime(&self) -> bool {
        true
    }

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        self.apply_config(config);
    }
}

#[distributed_slice(ENGINES)]
fn register_spotify_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "spotify",
            name: "Spotify Player",
            category: "media",
            version: VERSION,
        },
        capabilities: Capabilities {
            realtime: true,
            supports_128x32: true,
            supports_256x64: true,
            ..Default::default()
        },
        requirements: Requirements {
            needs_network: true,
            ..Default::default()
        },
        schema: ConfigSchema {
            fields: vec![
                ConfigField {
                    id: "client_id",
                    field_type: ConfigType::String,
                    label: "Spotify Client ID",
                    description: "Your Spotify Developer API Client ID.",
                    default_value: "",
                    required: true,
                    options: None,
                    min_val: None,
                    max_val: None,
                    step: None,
                    visible_when: None,
                    options_endpoint: None,
                    multiple: false,
                    validation_policy: ValidationPolicy::Accept,
                },
                ConfigField {
                    id: "client_secret",
                    field_type: ConfigType::String,
                    label: "Spotify Client Secret",
                    description: "Your Spotify Developer API Client Secret.",
                    default_value: "",
                    required: false,
                    options: None,
                    min_val: None,
                    max_val: None,
                    step: None,
                    visible_when: None,
                    options_endpoint: None,
                    multiple: false,
                    validation_policy: ValidationPolicy::Accept,
                },
                ConfigField {
                    id: "refresh_token",
                    field_type: ConfigType::String,
                    label: "Spotify Refresh Token",
                    description: "Your OAuth2 Refresh Token (allows infinite background updates without re-logging in).",
                    default_value: "",
                    required: true,
                    options: None,
                    min_val: None,
                    max_val: None,
                    step: None,
                    visible_when: None,
                    options_endpoint: None,
                    multiple: false,
                    validation_policy: ValidationPolicy::Accept,
                },
                ConfigField {
                    id: "show_album_art",
                    field_type: ConfigType::Boolean,
                    label: "Show Album Cover",
                    description: "Download and display Spotify album cover art on the matrix.",
                    default_value: "true",
                    required: false,
                    options: None,
                    min_val: None,
                    max_val: None,
                    step: None,
                    visible_when: None,
                    options_endpoint: None,
                    multiple: false,
                    validation_policy: ValidationPolicy::FallbackDefault,
                },
                ConfigField {
                    id: "show_progress",
                    field_type: ConfigType::Boolean,
                    label: "Show Progress Bar",
                    description: "Render track playback elapsed progress bar at the bottom.",
                    default_value: "true",
                    required: false,
                    options: None,
                    min_val: None,
                    max_val: None,
                    step: None,
                    visible_when: None,
                    options_endpoint: None,
                    multiple: false,
                    validation_policy: ValidationPolicy::FallbackDefault,
                },
                ConfigField {
                    id: "show_visualizer",
                    field_type: ConfigType::Boolean,
                    label: "Animated Equalizer",
                    description: "Display dancing equalizer frequency bars while music is playing.",
                    default_value: "true",
                    required: false,
                    options: None,
                    min_val: None,
                    max_val: None,
                    step: None,
                    visible_when: None,
                    options_endpoint: None,
                    multiple: false,
                    validation_policy: ValidationPolicy::FallbackDefault,
                },
                ConfigField {
                    id: "show_volume",
                    field_type: ConfigType::Boolean,
                    label: "Show Volume Indicator",
                    description: "Display Spotify active playback volume percentage.",
                    default_value: "true",
                    required: false,
                    options: None,
                    min_val: None,
                    max_val: None,
                    step: None,
                    visible_when: None,
                    options_endpoint: None,
                    multiple: false,
                    validation_policy: ValidationPolicy::FallbackDefault,
                },
            ],
        },
        factory: || Box::new(SpotifyEngine::new()),
    }
}

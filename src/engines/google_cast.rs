use crate::api::cast::{discover_cast_device, CastMediaStatus, GoogleCastClient};
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

pub struct GoogleCastEngine {
    base_renderer: BaseRenderer,
    device_ip: String,
    device_name: String,
    show_album_art: bool,
    show_progress: bool,
    show_volume: bool,
    show_visualizer: bool,

    // Live state shared with background poll thread
    media_status: Arc<Mutex<CastMediaStatus>>,
    is_running: Arc<AtomicBool>,
    last_image_url: String,
    cached_cover: Option<RgbImage>,

    // Animation & marquee timers
    anim_frame: u32,
    marquee_offset: i32,
    last_marquee_tick: Instant,
    last_anim_tick: Instant,
}

impl GoogleCastEngine {
    pub fn new() -> Self {
        Self {
            base_renderer: BaseRenderer::new(),
            device_ip: String::new(),
            device_name: String::new(),
            show_album_art: true,
            show_progress: true,
            show_volume: true,
            show_visualizer: true,
            media_status: Arc::new(Mutex::new(CastMediaStatus::default())),
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
        self.device_ip = config.get_string("device_ip", "");
        self.device_name = config.get_string("device_name", "");
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
        let status_arc = Arc::clone(&self.media_status);
        let running_arc = Arc::clone(&self.is_running);
        let ip_configured = self.device_ip.clone();
        let name_filter = self.device_name.clone();

        thread::spawn(move || {
            let mut resolved_ip = ip_configured.clone();
            let mut client: Option<GoogleCastClient> = None;

            while running_arc.load(Ordering::Relaxed) {
                // If IP is not configured or connection lost, discover via mDNS
                if resolved_ip.is_empty() {
                    let filter = if name_filter.is_empty() {
                        None
                    } else {
                        Some(name_filter.as_str())
                    };
                    if let Some((ip, port)) =
                        discover_cast_device(filter, Duration::from_millis(1500))
                    {
                        resolved_ip = ip.clone();
                        client = Some(GoogleCastClient::new(&ip, port));
                    }
                } else if client.is_none() {
                    client = Some(GoogleCastClient::new(&resolved_ip, 8009));
                }

                if let Some(ref mut c) = client {
                    match c.poll_status() {
                        Ok(st) => {
                            if let Ok(mut guard) = status_arc.lock() {
                                *guard = st;
                            }
                        }
                        Err(_) => {
                            // If user didn't hardcode an IP, reset resolved_ip to re-discover next round
                            if ip_configured.is_empty() {
                                resolved_ip.clear();
                                client = None;
                            }
                        }
                    }
                }

                thread::sleep(Duration::from_millis(1000));
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
        // Fetch and decode image
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

impl Engine for GoogleCastEngine {
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
        let (img_url, is_playing) = if let Ok(guard) = self.media_status.lock() {
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
        let (status, has_data) = if let Ok(guard) = self.media_status.lock() {
            (guard.clone(), guard.is_active && !guard.title.is_empty())
        } else {
            (CastMediaStatus::default(), false)
        };

        let w = ctx.matrix.width() as i32;
        let h = ctx.matrix.height() as i32;

        let font = self.base_renderer.font();

        if !has_data {
            // Idle screen when no media is casting
            let title = "Google Cast";
            let subtitle = if !self.device_name.is_empty() {
                format!("Ready to stream • {}", self.device_name)
            } else {
                "Ready to stream".to_string()
            };

            let (_, title_w, _) = font.get_pixel_map(title, 1.0);
            let y_idle_title = if h >= 64 { (h / 2) - 10 } else { 4 };
            let y_idle_sub = if h >= 64 { (h / 2) + 4 } else { 16 };

            // Title: Center if fits, else marquee
            if title_w <= w - 4 {
                let x_title = (w - title_w) / 2;
                BaseRenderer::draw_text_clipped(
                    ctx.matrix,
                    title,
                    &font,
                    1.0,
                    x_title,
                    y_idle_title,
                    2,
                    w - 2,
                    (66, 133, 244),
                    (0, 0, 0),
                );
            } else {
                let avail = w - 4;
                let overflow = title_w - avail + 12;
                let pause_start = 30;
                let pause_end = 20;
                let cycle = (pause_start + overflow + pause_end).max(1);
                let phase = self.marquee_offset.rem_euclid(cycle);
                let dx = if phase < pause_start {
                    0
                } else if phase < pause_start + overflow {
                    phase - pause_start
                } else {
                    overflow
                };
                BaseRenderer::draw_text_clipped(
                    ctx.matrix,
                    title,
                    &font,
                    1.0,
                    2 - dx,
                    y_idle_title,
                    2,
                    w - 2,
                    (66, 133, 244),
                    (0, 0, 0),
                );
            }

            // Subtitle: Marquee scroll with pause or center if fits
            let (_, sub_w, _) = font.get_pixel_map(&subtitle, 1.0);
            let clip_min_x = 2;
            let clip_max_x = w - 2;
            let avail_w = clip_max_x - clip_min_x;

            let sub_draw_x = if sub_w <= avail_w {
                (w - sub_w) / 2
            } else {
                let overflow = sub_w - avail_w + 14;
                let pause_start = 35;
                let pause_end = 20;
                let cycle = (pause_start + overflow + pause_end).max(1);
                let phase = (self.marquee_offset / 2).rem_euclid(cycle);
                let dx = if phase < pause_start {
                    0
                } else if phase < pause_start + overflow {
                    phase - pause_start
                } else {
                    overflow
                };
                clip_min_x - dx
            };

            BaseRenderer::draw_text_clipped(
                ctx.matrix,
                &subtitle,
                &font,
                1.0,
                sub_draw_x,
                y_idle_sub,
                clip_min_x,
                clip_max_x,
                (160, 160, 175),
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
        } else if self.show_volume {
            24
        } else {
            2
        };

        // Strict Viewport for Text
        let clip_min_x = text_x;
        let clip_max_x = w - right_reserved;
        let avail_w = (clip_max_x - clip_min_x).max(16);

        // 3. Smooth Marquee with Pause at Start & End
        let render_marquee = |matrix: &mut dyn MatrixBackend,
                              text: &str,
                              font: &ArcadeFont<'_>,
                              y: i32,
                              color: (u8, u8, u8),
                              offset: i32| {
            let (_, text_w, _) = font.get_pixel_map(text, 1.0);
            let draw_x = if text_w <= avail_w {
                clip_min_x
            } else {
                let overflow = text_w - avail_w + 12;
                let pause_start = 35; // ~1.2s initial pause
                let pause_end = 20; // ~0.7s pause at end
                let cycle = (pause_start + overflow + pause_end).max(1);
                let phase = offset.rem_euclid(cycle);

                let dx = if phase < pause_start {
                    0
                } else if phase < pause_start + overflow {
                    phase - pause_start
                } else {
                    overflow
                };
                clip_min_x - dx
            };

            BaseRenderer::draw_text_clipped(
                matrix,
                text,
                font,
                1.0,
                draw_x,
                y,
                clip_min_x,
                clip_max_x,
                color,
                (0, 0, 0),
            );
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

        // Draw Artist / Subtitle
        let artist_display = if !status.artist.is_empty() {
            status.artist.clone()
        } else if !status.app_name.is_empty() {
            status.app_name.clone()
        } else {
            "Google Nest".to_string()
        };

        render_marquee(
            ctx.matrix,
            &artist_display,
            &font,
            y_artist,
            (66, 180, 255), // Google Blue / Cyan tint
            self.marquee_offset / 2,
        );

        // 4. Draw Beat Visualizer on the right with dynamic colors
        if self.show_visualizer && status.is_playing {
            let eq_base_y = if h >= 64 { 44 } else { 21 };

            if is_compact_screen && has_art {
                // 3 ultra-crisp compact bars (2px wide each, 1px gap)
                let eq_x = w - 7;
                let bar_heights = [
                    ((self.anim_frame.wrapping_mul(3)) % 7 + 2) as i32,
                    ((self.anim_frame.wrapping_mul(5)) % 9 + 3) as i32,
                    ((self.anim_frame.wrapping_mul(2)) % 6 + 2) as i32,
                ];

                for (i, &bh) in bar_heights.iter().enumerate() {
                    let bx = eq_x + (i as i32 * 2);
                    for by in 0..bh {
                        let py = eq_base_y - by;
                        if py >= 0 {
                            let color = if by > 6 {
                                (255, 60, 60)
                            } else if by > 3 {
                                (255, 200, 0)
                            } else {
                                (0, 255, 120)
                            };
                            ctx.matrix.set_pixel(bx, py, color.0, color.1, color.2);
                        }
                    }
                }
            } else {
                // 4 wide bars (2px wide + 1px spacing)
                let eq_x = w - 13;
                let bar_heights = [
                    ((self.anim_frame.wrapping_mul(3)) % 8 + 2) as i32,
                    ((self.anim_frame.wrapping_mul(5)) % 11 + 3) as i32,
                    ((self.anim_frame.wrapping_mul(2)) % 9 + 2) as i32,
                    ((self.anim_frame.wrapping_mul(7)) % 7 + 2) as i32,
                ];

                for (i, &bh) in bar_heights.iter().enumerate() {
                    let bx = eq_x + (i as i32 * 3);
                    for by in 0..bh {
                        let py = eq_base_y - by;
                        if py >= 0 {
                            let color = if by > 7 {
                                (255, 50, 50)
                            } else if by > 4 {
                                (255, 190, 0)
                            } else {
                                (0, 240, 110)
                            };
                            ctx.matrix.set_pixel(bx, py, color.0, color.1, color.2);
                            ctx.matrix.set_pixel(bx + 1, py, color.0, color.1, color.2);
                        }
                    }
                }
            }
        } else if self.show_volume {
            let vol_pct = (status.volume_level * 100.0) as u32;
            let vol_str = format!("{}%", vol_pct);
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

        // 5. Draw Progress Bar (Bottom 2 pixels)
        if self.show_progress && status.duration_sec > 0.0 {
            let progress = (status.current_time_sec / status.duration_sec).clamp(0.0, 1.0);
            let bar_w = ((w - 2) as f32 * progress) as i32;

            // Background line
            for x in 1..w - 1 {
                ctx.matrix.set_pixel(x, h - 2, 35, 35, 40);
                ctx.matrix.set_pixel(x, h - 1, 20, 20, 25);
            }
            // Active progress fill (Google Blue gradient)
            for x in 1..=bar_w {
                ctx.matrix.set_pixel(x, h - 2, 66, 133, 244);
                ctx.matrix.set_pixel(x, h - 1, 33, 90, 180);
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
fn register_google_cast_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "google_cast",
            name: "Google Cast (Nest Audio)",
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
                    id: "device_ip",
                    field_type: ConfigType::String,
                    label: "Device IP (Optional)",
                    description: "Static IP of your Google Home / Nest Audio. Leave empty for automatic mDNS LAN discovery.",
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
                    id: "device_name",
                    field_type: ConfigType::String,
                    label: "Device Name Filter",
                    description: "Filter by friendly name (e.g. 'Living Room Speaker') when auto-discovering on LAN.",
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
                    id: "show_album_art",
                    field_type: ConfigType::Boolean,
                    label: "Show Album Cover",
                    description: "Download and display album art cover on the left of the LED matrix.",
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
                    description: "Render track playback elapsed time bar at the bottom.",
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
                    description: "Display Google Nest current volume percentage.",
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
        factory: || Box::new(GoogleCastEngine::new()),
    }
}

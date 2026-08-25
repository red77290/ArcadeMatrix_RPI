use crate::api::{DayForecast, WeatherProvider};
use crate::core::engine_contract::{Engine, EngineConfig, EngineContext, EngineError};
use crate::engines::renderers::BaseRenderer;
use image::{imageops, RgbImage};
use linkme::distributed_slice;

use std::time::{Duration, Instant};

pub struct WeatherEngine {
    base_renderer: BaseRenderer,
    providers: Vec<Box<dyn WeatherProvider>>,
    forecasts: Vec<DayForecast>,
    last_fetch: Option<Instant>,
    /// Pre-rendered panorama (3 × matrix_width) as RgbImage
    panorama: Option<RgbImage>,
    panorama_w: u32,
    panorama_mw: u32,
    scroll_start: Instant,
    api_key: String,
    city: String,
    lang: String,
    last_lang: String,
    units: String,
    offset_x: i32,
    offset_y: i32,
}

impl WeatherEngine {
    pub fn new() -> Self {
        Self {
            base_renderer: BaseRenderer::new(),
            providers: Vec::new(),
            forecasts: Vec::new(),
            last_fetch: None,
            panorama: None,
            panorama_w: 0,
            panorama_mw: 0,
            scroll_start: Instant::now(),
            api_key: "".to_string(),
            city: "".to_string(),
            lang: "en".to_string(),
            last_lang: "en".to_string(),
            units: "metric".to_string(),
            offset_x: 0,
            offset_y: 0,
        }
    }

    pub fn add_provider(&mut self, provider: Box<dyn WeatherProvider>) {
        self.providers.push(provider);
    }

    /// Parse the instance config into engine state. Shared by `initialize()`
    /// and `on_config_changed()` so edits apply live without an app restart.
    fn apply_config(&mut self, config: &dyn EngineConfig) {
        self.api_key = config.get_string("api_key", "");
        self.city = config.get_string("city", "");
        self.lang = config.get_string("lang", "en");
        self.units = config.get_string("units", "metric");
        self.offset_x = config.get_int("offset_x", 0);
        self.offset_y = config.get_int("offset_y", 0);
    }
}

impl Engine for WeatherEngine {
    fn initialize(
        &mut self,
        _context: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        self.apply_config(config);
        Ok(())
    }

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        self.apply_config(config);
        // City/lang/offset edits must take effect now: drop cached forecast and
        // pre-rendered panorama so the next render refetches and rebuilds.
        self.last_fetch = None;
        self.forecasts.clear();
        self.panorama = None;
    }

    fn activate(&mut self) {
        self.last_fetch = None; // Force refresh on activation
    }

    fn update(&mut self, _context: &mut EngineContext) {
        // Handle logic that doesn't draw
    }

    fn render(&mut self, context: &mut EngineContext) {
        if self.api_key.is_empty() || self.api_key == "API_KEY" {
            self.base_renderer.render_text(
                context.matrix,
                "No API key",
                0,
                1,
                0,
                0,
                Some((180, 80, 80)),
                None,
            );
            return;
        }
        if self.city.is_empty() {
            self.base_renderer.render_text(
                context.matrix,
                "No city",
                0,
                1,
                0,
                0,
                Some((180, 80, 80)),
                None,
            );
            return;
        }

        let sys_lang = context.config.settings.read().system.lang.clone();
        let active_lang = if !sys_lang.is_empty() {
            sys_lang
        } else {
            "fr".to_string()
        };

        if self.last_lang != active_lang {
            self.last_lang = active_lang.clone();
            self.last_fetch = None;
            self.forecasts.clear();
            self.panorama = None;
        }

        let should_fetch = self
            .last_fetch
            .map(|t| t.elapsed() > Duration::from_secs(1800))
            .unwrap_or(true);

        if should_fetch {
            self.fetch_forecast(&self.api_key.clone(), &self.city.clone(), &active_lang);
        }

        if self.forecasts.is_empty() {
            let empty_text = if self.units.eq_ignore_ascii_case("imperial")
                || self.units.eq_ignore_ascii_case("fahrenheit")
                || self.units.eq_ignore_ascii_case("f")
            {
                "--°F"
            } else {
                "--°C"
            };
            self.base_renderer.render_text(
                context.matrix,
                empty_text,
                0,
                2,
                self.offset_x,
                self.offset_y,
                None,
                None,
            );
            return;
        }

        let mw = context.matrix.width();
        let mh = context.matrix.height();

        // (Re)build panorama if needed
        if self.panorama.is_none() || self.panorama_mw != mw {
            self.build_panorama(mw, mh, self.offset_x, self.offset_y);
        }

        // Animated horizontal scroll with ease-in/out
        let num_slides = self.forecasts.len() as u64;
        let slide_dur = 5u64;
        let trans_dur = 1u64;
        let cycle = (slide_dur + trans_dur) * num_slides;

        let t = self.scroll_start.elapsed().as_secs() % cycle;
        let slide_idx = t / (slide_dur + trans_dur);
        let local_t = t % (slide_dur + trans_dur);

        let x_scroll = if local_t < slide_dur {
            (slide_idx * mw as u64) as u32
        } else {
            let progress = (local_t - slide_dur) as f32 / trans_dur as f32;
            let ease = progress * progress * (3.0 - 2.0 * progress);
            ((slide_idx as f32 + ease) * mw as f32) as u32
        };

        // Crop view from panorama
        if let Some(ref pano) = self.panorama {
            let view_x = x_scroll.min(pano.width().saturating_sub(mw));
            let view = imageops::crop_imm(pano, view_x, 0, mw, mh);
            let view_img = view.to_image();
            context.matrix.draw_image(&view_img, 0, 0);
        }
    }

    fn deactivate(&mut self) {
        // Cleanup if necessary
    }
}

impl WeatherEngine {
    fn draw_arcade_text(
        &self,
        img: &mut image::RgbaImage,
        text: &str,
        start_x: i32,
        start_y: i32,
        color: (u8, u8, u8),
        scale: f32,
    ) {
        let font = self.base_renderer.font();
        let (pixels_by_char, _, _) = font.get_pixel_map(text, scale);

        for char_pixels in pixels_by_char {
            for (px, py) in char_pixels {
                let draw_x = start_x + px;
                let draw_y = start_y + py;
                if draw_x >= 0
                    && draw_x < img.width() as i32
                    && draw_y >= 0
                    && draw_y < img.height() as i32
                {
                    img.put_pixel(
                        draw_x as u32,
                        draw_y as u32,
                        image::Rgba([color.0, color.1, color.2, 255]),
                    );
                }
            }
        }
    }

    fn build_panorama(&mut self, mw: u32, mh: u32, offset_x: i32, offset_y: i32) {
        let num_slides = self.forecasts.len() as u32 + 1; // +1 for wrap-around
        let pano_w = mw * num_slides;
        let mut panorama = image::RgbaImage::new(pano_w, mh);

        let slides: Vec<DayForecast> = self
            .forecasts
            .iter()
            .cloned()
            .chain(std::iter::once(self.forecasts[0].clone()))
            .collect();

        let is_wide = mw >= 128;
        let is_tall = mh >= 64;

        for (i, slide) in slides.iter().enumerate() {
            let base_x = i as u32 * mw;
            let font = self.base_renderer.font();

            let color_morning = (120, 200, 255);
            let color_afternoon = (255, 150, 50);
            let color_label = (180, 180, 255);
            let color_desc = (210, 210, 210);

            if mw >= 256 && mh >= 64 {
                // --- 256x64 Ultra-Widescreen HD Layout ---
                let icon_x = base_x as i32 + offset_x + 20;
                let icon_y = (mh as i32 - 24) / 2 + offset_y;
                self.draw_icon(&mut panorama, &slide.icon, icon_x, icon_y);

                let temp_x = icon_x + 36;
                self.draw_arcade_text(
                    &mut panorama,
                    &slide.temp_min,
                    temp_x,
                    offset_y + 10,
                    color_morning,
                    2.0,
                );
                self.draw_arcade_text(
                    &mut panorama,
                    &slide.temp_max,
                    temp_x,
                    offset_y + 38,
                    color_afternoon,
                    2.0,
                );

                let (_, min_w, _) = font.get_pixel_map(&slide.temp_min, 2.0);
                let (_, max_w, _) = font.get_pixel_map(&slide.temp_max, 2.0);
                let right_x = temp_x + min_w.max(max_w) + 18;

                self.draw_arcade_text(
                    &mut panorama,
                    &slide.label,
                    right_x,
                    offset_y + 10,
                    color_label,
                    2.0,
                );

                if !slide.condition.is_empty() {
                    self.draw_arcade_text(
                        &mut panorama,
                        &slide.condition,
                        right_x,
                        offset_y + 38,
                        color_desc,
                        2.0,
                    );
                }
            } else if is_wide && !is_tall {
                // --- 128x32 Optimized Spacious Layout ---
                let icon_x = base_x as i32 + offset_x + 4;
                let icon_y = (mh as i32 - 24) / 2 + offset_y;
                self.draw_icon(&mut panorama, &slide.icon, icon_x, icon_y);

                let temp_x = icon_x + 28;
                self.draw_arcade_text(
                    &mut panorama,
                    &slide.temp_min,
                    temp_x,
                    offset_y + 4,
                    color_morning,
                    1.0,
                );
                self.draw_arcade_text(
                    &mut panorama,
                    &slide.temp_max,
                    temp_x,
                    offset_y + 18,
                    color_afternoon,
                    1.0,
                );

                let (_, min_w, _) = font.get_pixel_map(&slide.temp_min, 1.0);
                let (_, max_w, _) = font.get_pixel_map(&slide.temp_max, 1.0);
                let mut right_x = temp_x + min_w.max(max_w) + 6;
                if right_x < base_x as i32 + 60 + offset_x {
                    right_x = base_x as i32 + 60 + offset_x;
                }
                let max_right = (base_x + mw) as i32 - 42;
                if right_x > max_right {
                    right_x = max_right;
                }

                self.draw_arcade_text(
                    &mut panorama,
                    &slide.label,
                    right_x,
                    offset_y + 4,
                    color_label,
                    1.0,
                );

                if !slide.condition.is_empty() {
                    let avail_w = ((base_x + mw) as i32 - right_x - 2).max(6);
                    let max_chars = (avail_w / 6).max(1) as usize;
                    let mut cond = slide.condition.clone();
                    if cond.chars().count() > max_chars {
                        if max_chars > 3 {
                            cond = format!(
                                "{}.",
                                &cond[..cond
                                    .char_indices()
                                    .nth(max_chars - 1)
                                    .map(|(i, _)| i)
                                    .unwrap_or(cond.len())]
                            );
                        } else {
                            cond = cond[..cond
                                .char_indices()
                                .nth(max_chars)
                                .map(|(i, _)| i)
                                .unwrap_or(cond.len())]
                                .to_string();
                        }
                    }
                    self.draw_arcade_text(
                        &mut panorama,
                        &cond,
                        right_x,
                        offset_y + 18,
                        color_desc,
                        1.0,
                    );
                }
            } else if is_tall && is_wide {
                // --- 128x64 or larger Spacious Layout ---
                let icon_x = base_x as i32 + offset_x + 6;
                let icon_y = (mh as i32 - 24) / 2 + offset_y;
                self.draw_icon(&mut panorama, &slide.icon, icon_x, icon_y);

                let temp_x = icon_x + 30;
                self.draw_arcade_text(
                    &mut panorama,
                    &slide.temp_min,
                    temp_x,
                    offset_y + 12,
                    color_morning,
                    2.0,
                );
                self.draw_arcade_text(
                    &mut panorama,
                    &slide.temp_max,
                    temp_x,
                    offset_y + 38,
                    color_afternoon,
                    2.0,
                );

                let (_, min_w, _) = font.get_pixel_map(&slide.temp_min, 2.0);
                let (_, max_w, _) = font.get_pixel_map(&slide.temp_max, 2.0);
                let mut right_x = temp_x + min_w.max(max_w) + 8;
                if right_x < base_x as i32 + 72 + offset_x {
                    right_x = base_x as i32 + 72 + offset_x;
                }
                let max_right = (base_x + mw) as i32 - 46;
                if right_x > max_right {
                    right_x = max_right;
                }

                self.draw_arcade_text(
                    &mut panorama,
                    &slide.label,
                    right_x,
                    offset_y + 14,
                    color_label,
                    1.0,
                );

                if !slide.condition.is_empty() {
                    let avail_w = ((base_x + mw) as i32 - right_x - 2).max(6);
                    let max_chars = (avail_w / 6).max(1) as usize;
                    let mut cond = slide.condition.clone();
                    if cond.chars().count() > max_chars {
                        if max_chars > 3 {
                            cond = format!(
                                "{}.",
                                &cond[..cond
                                    .char_indices()
                                    .nth(max_chars - 1)
                                    .map(|(i, _)| i)
                                    .unwrap_or(cond.len())]
                            );
                        } else {
                            cond = cond[..cond
                                .char_indices()
                                .nth(max_chars)
                                .map(|(i, _)| i)
                                .unwrap_or(cond.len())]
                                .to_string();
                        }
                    }
                    self.draw_arcade_text(
                        &mut panorama,
                        &cond,
                        right_x,
                        offset_y + 38,
                        color_desc,
                        1.0,
                    );
                }
            } else if mw <= 64 && mh <= 32 {
                // --- 64x32 Compact Horizontal Layout ---
                let icon_x = base_x as i32 + offset_x + 2;
                let icon_y = (mh as i32 - 24) / 2 + offset_y;
                self.draw_icon(&mut panorama, &slide.icon, icon_x, icon_y);

                let right_x = icon_x + 26;
                let avail_w = ((base_x + mw) as i32 - right_x - 1).max(6);
                let max_chars = (avail_w / 6).max(1) as usize;

                // Line 1: Day Label
                self.draw_arcade_text(
                    &mut panorama,
                    &slide.label,
                    right_x,
                    offset_y + 2,
                    color_label,
                    1.0,
                );

                // Line 2: Temperatures (Min + Max)
                self.draw_arcade_text(
                    &mut panorama,
                    &slide.temp_min,
                    right_x,
                    offset_y + 11,
                    color_morning,
                    1.0,
                );
                let (_, min_w, _) = font.get_pixel_map(&slide.temp_min, 1.0);
                self.draw_arcade_text(
                    &mut panorama,
                    &slide.temp_max,
                    right_x + min_w + 2,
                    offset_y + 11,
                    color_afternoon,
                    1.0,
                );

                // Line 3: Condition
                if !slide.condition.is_empty() {
                    let mut cond = slide.condition.clone();
                    if cond.chars().count() > max_chars {
                        if max_chars > 3 {
                            cond = format!(
                                "{}.",
                                &cond[..cond
                                    .char_indices()
                                    .nth(max_chars - 1)
                                    .map(|(i, _)| i)
                                    .unwrap_or(cond.len())]
                            );
                        } else {
                            cond = cond[..cond
                                .char_indices()
                                .nth(max_chars)
                                .map(|(i, _)| i)
                                .unwrap_or(cond.len())]
                                .to_string();
                        }
                    }
                    self.draw_arcade_text(
                        &mut panorama,
                        &cond,
                        right_x,
                        offset_y + 21,
                        color_desc,
                        1.0,
                    );
                }
            } else {
                // --- 64x64 or Vertical Layout ---
                let icon_x = base_x as i32 + (mw as i32 - 24) / 2 + offset_x;
                let icon_y = offset_y + 14;
                self.draw_icon(&mut panorama, &slide.icon, icon_x, icon_y);

                let (_, label_w, _) = font.get_pixel_map(&slide.label, 1.0);
                let label_x = base_x as i32 + (mw as i32 - label_w) / 2 + offset_x;
                self.draw_arcade_text(
                    &mut panorama,
                    &slide.label,
                    label_x,
                    offset_y + 3,
                    color_label,
                    1.0,
                );

                if !slide.condition.is_empty() {
                    let (_, desc_w, _) = font.get_pixel_map(&slide.condition, 1.0);
                    let desc_x = base_x as i32 + (mw as i32 - desc_w) / 2 + offset_x;
                    self.draw_arcade_text(
                        &mut panorama,
                        &slide.condition,
                        desc_x,
                        offset_y + 40,
                        color_desc,
                        1.0,
                    );
                }

                let (_, min_w, _) = font.get_pixel_map(&slide.temp_min, 1.0);
                let min_x = base_x as i32 + (mw as i32 - min_w) / 2 + offset_x;
                self.draw_arcade_text(
                    &mut panorama,
                    &slide.temp_min,
                    min_x,
                    offset_y + 49,
                    color_morning,
                    1.0,
                );

                let (_, max_w, _) = font.get_pixel_map(&slide.temp_max, 1.0);
                let max_x = base_x as i32 + (mw as i32 - max_w) / 2 + offset_x;
                self.draw_arcade_text(
                    &mut panorama,
                    &slide.temp_max,
                    max_x,
                    offset_y + 57,
                    color_afternoon,
                    1.0,
                );
            }
        }

        let mut rgb_panorama = RgbImage::new(pano_w, mh);
        for y in 0..mh {
            for x in 0..pano_w {
                let px = panorama.get_pixel(x, y);
                rgb_panorama.put_pixel(x, y, image::Rgb([px[0], px[1], px[2]]));
            }
        }

        self.panorama = Some(rgb_panorama);
        self.panorama_w = pano_w;
        self.panorama_mw = mw;
        self.scroll_start = Instant::now();
    }

    fn draw_icon(&self, img: &mut image::RgbaImage, icon: &str, x: i32, y: i32) {
        use image::Rgba;
        use imageproc::drawing::{
            draw_filled_circle_mut, draw_filled_rect_mut, draw_line_segment_mut,
        };
        use imageproc::rect::Rect;

        // 24x24 pixel area for icons
        let yellow = Rgba([255, 255, 0, 255]);
        let dark_yellow = Rgba([255, 200, 0, 255]);
        let light_grey = Rgba([200, 200, 200, 255]);
        let dark_grey = Rgba([150, 150, 150, 255]);
        let thunder_grey = Rgba([100, 100, 100, 255]);
        let blue = Rgba([0, 150, 255, 255]);
        let white = Rgba([255, 255, 255, 255]);
        let green = Rgba([0, 255, 0, 255]);

        if icon.contains("01") {
            // Sun
            draw_filled_circle_mut(img, (x + 12, y + 12), 6, yellow);
            draw_line_segment_mut(
                img,
                ((x + 12) as f32, (y + 2) as f32),
                ((x + 12) as f32, (y + 4) as f32),
                dark_yellow,
            );
            draw_line_segment_mut(
                img,
                ((x + 12) as f32, (y + 20) as f32),
                ((x + 12) as f32, (y + 22) as f32),
                dark_yellow,
            );
            draw_line_segment_mut(
                img,
                ((x + 2) as f32, (y + 12) as f32),
                ((x + 4) as f32, (y + 12) as f32),
                dark_yellow,
            );
            draw_line_segment_mut(
                img,
                ((x + 20) as f32, (y + 12) as f32),
                ((x + 22) as f32, (y + 12) as f32),
                dark_yellow,
            );
            draw_line_segment_mut(
                img,
                ((x + 5) as f32, (y + 5) as f32),
                ((x + 7) as f32, (y + 7) as f32),
                dark_yellow,
            );
            draw_line_segment_mut(
                img,
                ((x + 19) as f32, (y + 19) as f32),
                ((x + 17) as f32, (y + 17) as f32),
                dark_yellow,
            );
            draw_line_segment_mut(
                img,
                ((x + 19) as f32, (y + 5) as f32),
                ((x + 17) as f32, (y + 7) as f32),
                dark_yellow,
            );
            draw_line_segment_mut(
                img,
                ((x + 5) as f32, (y + 19) as f32),
                ((x + 7) as f32, (y + 17) as f32),
                dark_yellow,
            );
        } else if icon.contains("02") || icon.contains("03") || icon.contains("04") {
            // Clouds
            if icon.contains("02") {
                // Sun behind cloud
                draw_filled_circle_mut(img, (x + 8, y + 8), 4, yellow);
            }
            draw_filled_circle_mut(img, (x + 8, y + 14), 5, light_grey);
            draw_filled_circle_mut(img, (x + 14, y + 11), 6, white);
            draw_filled_circle_mut(img, (x + 20, y + 14), 5, light_grey);
            draw_filled_rect_mut(img, Rect::at(x + 8, y + 14).of_size(12, 6), light_grey);
        } else if icon.contains("09") || icon.contains("10") {
            // Rain
            draw_filled_circle_mut(img, (x + 8, y + 10), 5, dark_grey);
            draw_filled_circle_mut(img, (x + 14, y + 8), 6, light_grey);
            draw_filled_circle_mut(img, (x + 20, y + 10), 5, dark_grey);
            draw_filled_rect_mut(img, Rect::at(x + 8, y + 10).of_size(12, 6), dark_grey);
            draw_line_segment_mut(
                img,
                ((x + 8) as f32, (y + 18) as f32),
                ((x + 6) as f32, (y + 22) as f32),
                blue,
            );
            draw_line_segment_mut(
                img,
                ((x + 14) as f32, (y + 18) as f32),
                ((x + 12) as f32, (y + 22) as f32),
                blue,
            );
            draw_line_segment_mut(
                img,
                ((x + 20) as f32, (y + 18) as f32),
                ((x + 18) as f32, (y + 22) as f32),
                blue,
            );
        } else if icon.contains("11") {
            // Thunder
            draw_filled_circle_mut(img, (x + 8, y + 10), 5, thunder_grey);
            draw_filled_circle_mut(img, (x + 14, y + 8), 6, dark_grey);
            draw_filled_circle_mut(img, (x + 20, y + 10), 5, thunder_grey);
            draw_filled_rect_mut(img, Rect::at(x + 8, y + 10).of_size(12, 6), thunder_grey);
            draw_line_segment_mut(
                img,
                ((x + 14) as f32, (y + 16) as f32),
                ((x + 10) as f32, (y + 20) as f32),
                yellow,
            );
            draw_line_segment_mut(
                img,
                ((x + 10) as f32, (y + 20) as f32),
                ((x + 16) as f32, (y + 20) as f32),
                yellow,
            );
            draw_line_segment_mut(
                img,
                ((x + 16) as f32, (y + 20) as f32),
                ((x + 12) as f32, (y + 24) as f32),
                yellow,
            );
        } else if icon.contains("13") {
            // Snow
            draw_filled_circle_mut(img, (x + 14, y + 14), 2, white);
            draw_line_segment_mut(
                img,
                ((x + 14) as f32, (y + 8) as f32),
                ((x + 14) as f32, (y + 20) as f32),
                white,
            );
            draw_line_segment_mut(
                img,
                ((x + 8) as f32, (y + 14) as f32),
                ((x + 20) as f32, (y + 14) as f32),
                white,
            );
            draw_line_segment_mut(
                img,
                ((x + 10) as f32, (y + 10) as f32),
                ((x + 18) as f32, (y + 18) as f32),
                white,
            );
            draw_line_segment_mut(
                img,
                ((x + 18) as f32, (y + 10) as f32),
                ((x + 10) as f32, (y + 18) as f32),
                white,
            );
        } else {
            // Unknown
            draw_filled_circle_mut(img, (x + 12, y + 12), 6, green);
        }
    }

    fn fetch_forecast(&mut self, api_key: &str, city: &str, lang: &str) {
        self.last_fetch = Some(Instant::now());
        self.panorama = None; // Invalidate panorama cache

        for provider in &self.providers {
            if let Some(forecasts) = provider.fetch_forecast(api_key, city, lang, &self.units) {
                self.forecasts = forecasts;
                return;
            }
        }

        tracing::warn!("Failed to fetch weather forecast from all providers");
        self.forecasts.clear();
    }
}

use crate::core::engine_contract::{
    Capabilities, ConfigSchema, EngineDescriptor, EngineMetadata, Requirements,
};
#[distributed_slice(crate::core::registry::ENGINES)]
fn register_weather_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "weather",
            name: "WeatherEngine",
            category: "info",
            version: crate::core::build_info::VERSION,
        },
        capabilities: Capabilities::default(),
        requirements: Requirements::default(),
        schema: ConfigSchema {
            fields: vec![
                crate::core::engine_contract::ConfigField {
                    id: "api_key",
                    field_type: crate::core::engine_contract::ConfigType::String,
                    label: "API Key",
                    description: "OpenWeatherMap API Key",
                    default_value: "",
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Accept,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "city",
                    field_type: crate::core::engine_contract::ConfigType::String,
                    label: "City",
                    description: "City name for forecast (e.g. Paris,FR or for US: Tucson,AZ,US)",
                    default_value: "",
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Accept,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "units",
                    field_type: crate::core::engine_contract::ConfigType::Options,
                    label: "Units",
                    description: "Temperature unit: metric (°C) or imperial (°F)",
                    default_value: "metric",
                    options: Some(vec![
                        crate::core::engine_contract::ConfigOption {
                            label: "Celsius (°C)",
                            value: "metric",
                        },
                        crate::core::engine_contract::ConfigOption {
                            label: "Fahrenheit (°F)",
                            value: "imperial",
                        },
                    ]),
                    validation_policy:
                        crate::core::engine_contract::ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "offset_x",
                    field_type: crate::core::engine_contract::ConfigType::Integer,
                    label: "X Offset",
                    description: "Horizontal shift",
                    default_value: "0",
                    min_val: Some("-64"),
                    max_val: Some("64"),
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Clamp,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "offset_y",
                    field_type: crate::core::engine_contract::ConfigType::Integer,
                    label: "Y Offset",
                    description: "Vertical shift",
                    default_value: "0",
                    min_val: Some("-32"),
                    max_val: Some("32"),
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Clamp,
                    ..Default::default()
                },
            ],
        },
        factory: || -> Box<dyn crate::core::engine_contract::Engine> {
            let mut engine = WeatherEngine::new();
            // Wire the HTTP provider (see crypto.rs factory for why this is required).
            engine.add_provider(Box::new(crate::api::openweathermap::OpenWeatherMapProvider));
            Box::new(engine)
        },
    }
}

use crate::api::{DayForecast, WeatherProvider};
use crate::core::config::Config;
use crate::core::engine_contract::{Engine, EngineConfig, EngineContext, EngineError};
use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::BaseRenderer;
use image::{imageops, RgbImage};

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
    lang: String,
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
            lang: "fr".to_string(),
        }
    }

    pub fn add_provider(&mut self, provider: Box<dyn WeatherProvider>) {
        self.providers.push(provider);
    }
}

impl Engine for WeatherEngine {
    fn initialize(
        &mut self,
        _context: &mut EngineContext,
        _config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        // Initialization can be expanded later
        Ok(())
    }

    fn activate(&mut self) {
        self.last_fetch = None; // Force refresh on activation
    }

    fn update(&mut self, _context: &mut EngineContext) {
        // Handle logic that doesn't draw
    }

    fn render(&mut self, context: &mut EngineContext) {
        // Temporarily fetching from global config is wrong for the new architecture,
        // but since we are doing progressive migration we will mock this until rotation is migrated
        // In full architecture: offset_x = config.get_int("offset_x", 0), etc.
        let (api_key, city, lang, offset_x, offset_y) = {
            // Hardcoded for now during progressive migration since we can't easily
            // read the global Arc<RwLock<Config>> here without changing the struct
            (
                "API_KEY".to_string(), // TODO: use engine config
                "Paris".to_string(),
                "fr".to_string(),
                0,
                0,
            )
        };

        if self.lang != lang {
            self.lang = lang.clone();
            self.last_fetch = None;
            self.panorama = None;
        }

        if api_key.is_empty() || city.is_empty() || api_key == "API_KEY" {
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

        let should_fetch = self
            .last_fetch
            .map(|t| t.elapsed() > Duration::from_secs(1800))
            .unwrap_or(true);

        if should_fetch {
            self.fetch_forecast(&api_key, &city);
        }

        if self.forecasts.is_empty() {
            self.base_renderer.render_text(
                context.matrix,
                "--°C",
                0,
                2,
                offset_x,
                offset_y,
                None,
                None,
            );
            return;
        }

        let mw = context.matrix.width();
        let mh = context.matrix.height();

        // (Re)build panorama if needed
        if self.panorama.is_none() || self.panorama_mw != mw {
            self.build_panorama(mw, mh, offset_x, offset_y);
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

        let scale = if mh >= 64 { 2.0 } else { 1.0 };
        let icon_size = if mh >= 64 { mh - 8 } else { mh - 4 }.max(8);

        for (i, slide) in slides.iter().enumerate() {
            let base_x = i as u32 * mw;

            self.draw_icon(
                &mut panorama,
                &slide.icon,
                base_x as i32 + offset_x,
                (mh as i32 - 24) / 2 + offset_y,
            );

            // Draw label and temp as pixel text directly onto panorama
            let text_x =
                (base_x as i32 + offset_x + icon_size as i32 + (8.0 * scale) as i32).max(0);

            let (label_y, temp_y) = if scale >= 2.0 {
                (offset_y + 8, offset_y + 36)
            } else {
                (offset_y + 4, offset_y + 16)
            };

            self.draw_arcade_text(
                &mut panorama,
                &slide.label,
                text_x,
                label_y,
                (180, 180, 255),
                scale,
            );
            self.draw_arcade_text(
                &mut panorama,
                &slide.temp,
                text_x,
                temp_y,
                (255, 255, 255),
                scale,
            );
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

    fn fetch_forecast(&mut self, api_key: &str, city: &str) {
        self.last_fetch = Some(Instant::now());
        self.panorama = None; // Invalidate panorama cache

        for provider in &self.providers {
            if let Some(forecasts) = provider.fetch_forecast(api_key, city, &self.lang) {
                self.forecasts = forecasts;
                return;
            }
        }

        tracing::warn!("Failed to fetch weather forecast from all providers");
        self.forecasts.clear();
    }
}

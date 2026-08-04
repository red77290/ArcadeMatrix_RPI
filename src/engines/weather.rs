use crate::core::config::Config;
use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::BaseRenderer;
use image::{imageops, RgbImage};
use serde::Deserialize;
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Deserialize, Clone)]
struct ForecastMain {
    temp: f32,
    temp_min: f32,
    temp_max: f32,
}

#[derive(Deserialize, Clone)]
struct ForecastWeather {
    icon: String,
}

#[derive(Deserialize, Clone)]
struct ForecastEntry {
    main: ForecastMain,
    weather: Vec<ForecastWeather>,
}

#[derive(Deserialize)]
struct ForecastApiResponse {
    list: Vec<ForecastEntry>,
}

#[derive(Clone)]
struct DayForecast {
    label: String,
    temp: String,
    icon: String,
}

pub struct WeatherEngine {
    base_renderer: BaseRenderer,
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
        // Clean up corrupt weather icons on startup
        if let Ok(entries) = std::fs::read_dir("weather_icons") {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if meta.len() == 0 || image::open(entry.path()).is_err() {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }

        Self {
            base_renderer: BaseRenderer::new(),
            forecasts: Vec::new(),
            last_fetch: None,
            panorama: None,
            panorama_w: 0,
            panorama_mw: 0,
            scroll_start: Instant::now(),
            lang: "fr".to_string(),
        }
    }

    pub fn render(&mut self, matrix: &mut dyn MatrixBackend, config: &Config) {
        let (api_key, city, lang, offset_x, offset_y) = {
            let s = config.settings.read();
            (
                s.weather_api_key.clone(),
                s.weather_city.clone(),
                s.weather_lang.clone().to_lowercase(),
                s.weather_offset_x,
                s.weather_offset_y,
            )
        };
        if self.lang != lang {
            self.lang = lang.clone();
            self.last_fetch = None;
            self.panorama = None;
        }

        if api_key.is_empty() || city.is_empty() {
            self.base_renderer.render_text(
                matrix,
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
            self.base_renderer
                .render_text(matrix, "--°C", 0, 2, offset_x, offset_y, None, None);
            return;
        }

        let mw = matrix.width();
        let mh = matrix.height();

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
            matrix.draw_image(&view_img, 0, 0);
        }
    }

    fn draw_arcade_text(
        &self,
        img: &mut RgbImage,
        text: &str,
        x: i32,
        y: i32,
        color: (u8, u8, u8),
        scale: f32,
    ) {
        let font = self.base_renderer.font();
        let (pixels_by_char, _, _) = font.get_pixel_map(text, scale);

        for char_pixels in pixels_by_char {
            for (px, py) in char_pixels {
                let draw_x = x + px;
                let draw_y = y + py;
                if draw_x >= 0
                    && draw_x < img.width() as i32
                    && draw_y >= 0
                    && draw_y < img.height() as i32
                {
                    img.put_pixel(
                        draw_x as u32,
                        draw_y as u32,
                        image::Rgb([color.0, color.1, color.2]),
                    );
                }
            }
        }
    }

    fn build_panorama(&mut self, mw: u32, mh: u32, offset_x: i32, offset_y: i32) {
        let num_slides = self.forecasts.len() as u32 + 1; // +1 for wrap-around
        let pano_w = mw * num_slides;
        let mut panorama = RgbImage::new(pano_w, mh);

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

            // Try to load icon
            if let Some(icon) = self.load_icon(&slide.icon, icon_size) {
                let icon_x = (base_x as i32 + offset_x + 2).max(0) as u32;
                let icon_y = ((mh as i32 - icon.height() as i32) / 2 + offset_y).max(0) as u32;
                imageops::overlay(&mut panorama, &icon, icon_x as i64, icon_y as i64);
            }

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

        self.panorama = Some(panorama);
        self.panorama_w = pano_w;
        self.panorama_mw = mw;
        self.scroll_start = Instant::now();
    }

    fn load_icon(&self, icon_name: &str, size: u32) -> Option<RgbImage> {
        let icon_path = format!("weather_icons/{}.png", icon_name);
        if !Path::new(&icon_path).exists() {
            // Try to download from OpenWeatherMap
            let url = format!("http://openweathermap.org/img/wn/{}@2x.png", icon_name);
            if let Ok(resp) = reqwest::blocking::get(&url) {
                if resp.status().is_success() {
                    if let Ok(bytes) = resp.bytes() {
                        let _ = std::fs::create_dir_all("weather_icons");
                        let _ = std::fs::write(&icon_path, &bytes);
                    }
                }
            }
        }

        if let Ok(img) = image::open(&icon_path) {
            let mut rgba = img.into_rgba8();
            // Crop to bounding box to remove transparent padding
            let mut min_x = rgba.width();
            let mut min_y = rgba.height();
            let mut max_x = 0;
            let mut max_y = 0;

            for (x, y, pixel) in rgba.enumerate_pixels() {
                if pixel[3] > 0 {
                    // Check alpha channel
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }

            if min_x <= max_x && min_y <= max_y {
                let crop = imageops::crop(
                    &mut rgba,
                    min_x,
                    min_y,
                    max_x - min_x + 1,
                    max_y - min_y + 1,
                )
                .to_image();
                // Resize using high quality Lanczos3 filter
                let resized = imageops::resize(&crop, size, size, imageops::FilterType::Lanczos3);

                // Blend over black background to convert to RGB correctly
                let mut rgb = RgbImage::new(size, size);
                for (x, y, p) in resized.enumerate_pixels() {
                    let alpha = p[3] as f32 / 255.0;
                    let r = (p[0] as f32 * alpha) as u8;
                    let g = (p[1] as f32 * alpha) as u8;
                    let b = (p[2] as f32 * alpha) as u8;
                    rgb.put_pixel(x, y, image::Rgb([r, g, b]));
                }
                return Some(rgb);
            }
            None
        } else {
            None
        }
    }

    fn fetch_forecast(&mut self, api_key: &str, city: &str) {
        self.last_fetch = Some(Instant::now());
        self.panorama = None; // Invalidate panorama cache

        let url = format!(
            "https://api.openweathermap.org/data/2.5/forecast?q={}&appid={}&units=metric&lang={}",
            city, api_key, self.lang
        );

        let resp = match reqwest::blocking::get(&url) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Weather fetch error: {}", e);
                return;
            }
        };

        let data: ForecastApiResponse = match resp.json() {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("Weather JSON parse error: {}", e);
                return;
            }
        };

        // Day 0 = list[0], Day 1 = list[8] (~24h), Day 2 = list[16] (~48h)
        let indices = [0usize, 8, 16];
        let labels = match self.lang.as_str() {
            "fr" => ["AUJ", "DEM", "J+2"],
            "es" => ["HOY", "MAÑ", "D+2"],
            _ => ["NOW", "TMW", "D+2"],
        };

        self.forecasts = indices
            .iter()
            .zip(labels.iter())
            .filter_map(|(&idx, &label)| {
                data.list.get(idx).map(|entry| DayForecast {
                    label: label.to_string(),
                    temp: format!(
                        "{:.0}°C ({:.0}/{:.0})",
                        entry.main.temp, entry.main.temp_min, entry.main.temp_max
                    ),
                    icon: entry
                        .weather
                        .get(0)
                        .map(|w| w.icon.clone())
                        .unwrap_or_default(),
                })
            })
            .collect();
    }
}

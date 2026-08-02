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

        self.lang = lang;

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

        for (i, slide) in slides.iter().enumerate() {
            let base_x = i as u32 * mw;

            // Try to load icon
            let icon_size = (mh - 4).max(8);
            if let Some(icon) = self.load_icon(&slide.icon, icon_size) {
                let icon_x = (base_x as i32 + offset_x + 2).max(0) as u32;
                let icon_y = ((mh as i32 - icon.height() as i32) / 2 + offset_y).max(0) as u32;
                imageops::overlay(&mut panorama, &icon, icon_x as i64, icon_y as i64);
            }

            // Draw label and temp as pixel text directly onto panorama
            let text_x = (base_x as i32 + offset_x + icon_size as i32 + 4).max(0);
            let label_y = offset_y + 2;
            let temp_y = label_y + 8;

            self.draw_text_to_image(
                &mut panorama,
                &slide.label,
                text_x,
                label_y,
                (180, 180, 255),
            );
            self.draw_text_to_image(&mut panorama, &slide.temp, text_x, temp_y, (255, 255, 255));
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
                if let Ok(bytes) = resp.bytes() {
                    let _ = std::fs::create_dir_all("weather_icons");
                    let _ = std::fs::write(&icon_path, &bytes);
                }
            }
        }

        if let Ok(img) = image::open(&icon_path) {
            // Crop to bounding box to remove transparent padding
            let rgb = img.to_rgb8();
            // Resize to icon_size
            let resized = imageops::resize(&rgb, size, size, imageops::FilterType::Nearest);
            Some(resized)
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

    /// Simple pixel-font renderer that draws directly onto an RgbImage buffer
    fn draw_text_to_image(
        &self,
        img: &mut RgbImage,
        text: &str,
        x: i32,
        y: i32,
        color: (u8, u8, u8),
    ) {
        let segments: [[u8; 15]; 10] = [
            [1, 1, 1, 1, 0, 1, 1, 0, 1, 1, 0, 1, 1, 1, 1],
            [0, 1, 0, 1, 1, 0, 0, 1, 0, 0, 1, 0, 1, 1, 1],
            [1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1],
            [1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1],
            [1, 0, 1, 1, 0, 1, 1, 1, 1, 0, 0, 1, 0, 0, 1],
            [1, 1, 1, 1, 0, 0, 1, 1, 1, 0, 0, 1, 1, 1, 1],
            [1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 1, 1, 1, 1],
            [1, 1, 1, 0, 0, 1, 0, 0, 1, 0, 1, 0, 0, 1, 0],
            [1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1],
            [1, 1, 1, 1, 0, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1],
        ];
        let letter_bitmaps: [[u8; 15]; 26] = [
            [0, 1, 0, 1, 0, 1, 1, 1, 1, 1, 0, 1, 1, 0, 1], // A
            [1, 1, 0, 1, 0, 1, 1, 1, 0, 1, 0, 1, 1, 1, 0], // B
            [0, 1, 1, 1, 0, 0, 1, 0, 0, 1, 0, 0, 0, 1, 1], // C
            [1, 1, 0, 1, 0, 1, 1, 0, 1, 1, 0, 1, 1, 1, 0], // D
            [1, 1, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1, 1, 1], // E
            [1, 1, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1, 0, 0], // F
            [0, 1, 1, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 1, 1], // G
            [1, 0, 1, 1, 0, 1, 1, 1, 1, 1, 0, 1, 1, 0, 1], // H
            [1, 1, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0, 1, 1, 1], // I
            [0, 0, 1, 0, 0, 1, 0, 0, 1, 1, 0, 1, 0, 1, 0], // J
            [1, 0, 1, 1, 1, 0, 1, 0, 0, 1, 1, 0, 1, 0, 1], // K
            [1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 1, 1], // L
            [1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 0, 1], // M
            [1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1], // N
            [0, 1, 0, 1, 0, 1, 1, 0, 1, 1, 0, 1, 0, 1, 0], // O
            [1, 1, 0, 1, 0, 1, 1, 1, 0, 1, 0, 0, 1, 0, 0], // P
            [0, 1, 0, 1, 0, 1, 1, 0, 1, 1, 1, 1, 0, 1, 1], // Q
            [1, 1, 0, 1, 0, 1, 1, 1, 0, 1, 1, 0, 1, 0, 1], // R
            [0, 1, 1, 1, 0, 0, 0, 1, 0, 0, 0, 1, 1, 1, 0], // S
            [1, 1, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0], // T
            [1, 0, 1, 1, 0, 1, 1, 0, 1, 1, 0, 1, 0, 1, 0], // U
            [1, 0, 1, 1, 0, 1, 1, 0, 1, 0, 1, 0, 0, 1, 0], // V
            [1, 0, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1], // W
            [1, 0, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0, 1, 0, 1], // X
            [1, 0, 1, 1, 0, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0], // Y
            [1, 1, 1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 1, 1, 1], // Z
        ];
        let (img_w, img_h) = img.dimensions();
        let mut cx = x;
        for ch in text.chars() {
            if ch == ':' || ch == '/' || ch == '-' {
                if cx >= 0 && cx < img_w as i32 && y + 2 >= 0 && y + 2 < img_h as i32 {
                    img.put_pixel(
                        cx as u32,
                        (y + 1) as u32,
                        image::Rgb([color.0, color.1, color.2]),
                    );
                    img.put_pixel(
                        cx as u32,
                        (y + 3) as u32,
                        image::Rgb([color.0, color.1, color.2]),
                    );
                }
                cx += 2;
                continue;
            }
            if ch == ' ' {
                cx += 3;
                continue;
            }
            if ch == '°' {
                if cx >= 0 && y >= 0 && cx < img_w as i32 && y < img_h as i32 {
                    img.put_pixel(cx as u32, y as u32, image::Rgb([color.0, color.1, color.2]));
                }
                cx += 2;
                continue;
            }
            if ch == '+' {
                for (row, col, set) in [(1, 1, 1), (2, 0, 1), (2, 1, 1), (2, 2, 1), (3, 1, 1)] {
                    let px = cx + col;
                    let py = y + row;
                    if px >= 0 && py >= 0 && px < img_w as i32 && py < img_h as i32 && set == 1 {
                        img.put_pixel(
                            px as u32,
                            py as u32,
                            image::Rgb([color.0, color.1, color.2]),
                        );
                    }
                }
                cx += 4;
                continue;
            }
            let bitmap_opt: Option<&[u8; 15]> = if let Some(d) = ch.to_digit(10) {
                Some(&segments[d as usize])
            } else {
                let idx = ch.to_ascii_uppercase() as usize;
                if idx >= 'A' as usize && idx <= 'Z' as usize {
                    Some(&letter_bitmaps[idx - 'A' as usize])
                } else {
                    None
                }
            };
            if let Some(bm) = bitmap_opt {
                for row in 0..5i32 {
                    for col in 0..3i32 {
                        if bm[(row * 3 + col) as usize] == 1 {
                            let px = cx + col;
                            let py = y + row;
                            if px >= 0 && py >= 0 && px < img_w as i32 && py < img_h as i32 {
                                img.put_pixel(
                                    px as u32,
                                    py as u32,
                                    image::Rgb([color.0, color.1, color.2]),
                                );
                            }
                        }
                    }
                }
            }
            cx += 4;
        }
    }
}

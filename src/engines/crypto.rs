use crate::api::CryptoProvider;
use crate::core::config::Config;
use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::BaseRenderer;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};
use tracing::warn;

#[derive(Clone, Debug)]
pub struct CachedQuote {
    price: f64,
    change_24h: f64,
    last_fetch: Instant,
    has_data: bool,
    image_url: Option<String>,
}

pub struct CryptoEngine {
    base_renderer: BaseRenderer,
    cache: HashMap<String, CachedQuote>,
    providers: Vec<Box<dyn CryptoProvider>>,
    current_index: usize,
    last_switch: Instant,
}

impl CryptoEngine {
    pub fn new(_w: u32, _h: u32) -> Self {
        Self {
            base_renderer: BaseRenderer::new(),
            cache: HashMap::new(),
            providers: Vec::new(),
            current_index: 0,
            last_switch: Instant::now(),
        }
    }

    pub fn add_provider(&mut self, provider: Box<dyn CryptoProvider>) {
        self.providers.push(provider);
    }

    fn fetch_quote(&mut self, symbol: &str, ttl_min: u64) -> (f64, f64, bool, Option<String>) {
        let now = Instant::now();
        let ttl_secs = (if ttl_min > 0 { ttl_min } else { 1 }) * 60;

        // 1. Check Cache First
        if let Some(c) = self.cache.get(symbol) {
            if c.has_data && now.duration_since(c.last_fetch).as_secs() < ttl_secs {
                return (c.price, c.change_24h, true, c.image_url.clone());
            }
        }

        let mut fetched = false;
        let mut new_price = 0.0;
        let mut new_change = 0.0;
        let mut new_image_url = None;

        for provider in &self.providers {
            if let Some((price, change, img)) = provider.fetch_quote(symbol) {
                new_price = price;
                new_change = change;
                new_image_url = img;
                fetched = true;
                break;
            }
        }

        if fetched && new_price > 0.0 {
            self.cache.insert(
                symbol.to_string(),
                CachedQuote {
                    price: new_price,
                    change_24h: new_change,
                    last_fetch: now,
                    has_data: true,
                    image_url: new_image_url.clone(),
                },
            );
            return (new_price, new_change, true, new_image_url);
        }

        // 3. Fallback to last known quote for THIS symbol if HTTP failed
        if let Some(c) = self.cache.get(symbol) {
            if c.has_data {
                warn!(
                    "[HTTP Failed] Reusing last known cached price for {}: ${:.4}",
                    symbol, c.price
                );
                return (c.price, c.change_24h, true, c.image_url.clone());
            }
        }

        (0.0, 0.0, false, None)
    }

    fn get_and_load_icon(
        &self,
        symbol: &str,
        url: Option<String>,
        size: u32,
    ) -> Option<image::RgbaImage> {
        let icon_path = format!("data/crypto_icons/{}.png", symbol.to_lowercase());
        if !Path::new(&icon_path).exists() {
            if let Some(u) = url {
                let proxy_url = format!("https://wsrv.nl/?url={}&w=16&h=16&output=png", u);
                if let Ok(resp) = reqwest::blocking::get(&proxy_url) {
                    if resp.status().is_success() {
                        if let Ok(bytes) = resp.bytes() {
                            let _ = std::fs::create_dir_all("data/crypto_icons");
                            let _ = std::fs::write(&icon_path, &bytes);
                        }
                    }
                }
            }
        }

        if let Ok(img) = image::open(&icon_path) {
            let rgba = img.into_rgba8();
            // Try to crop transparent padding
            let mut min_x = rgba.width();
            let mut min_y = rgba.height();
            let mut max_x = 0;
            let mut max_y = 0;

            for (x, y, pixel) in rgba.enumerate_pixels() {
                if pixel[3] > 0 {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }

            let mut final_img = rgba.clone();
            if min_x <= max_x && min_y <= max_y {
                let crop_w = max_x - min_x + 1;
                let crop_h = max_y - min_y + 1;
                let mut cropped = image::RgbaImage::new(crop_w, crop_h);
                for y in 0..crop_h {
                    for x in 0..crop_w {
                        cropped.put_pixel(x, y, *rgba.get_pixel(min_x + x, min_y + y));
                    }
                }
                final_img = cropped;
            }

            let resized = image::imageops::resize(
                &final_img,
                size,
                size,
                image::imageops::FilterType::Triangle,
            );
            return Some(resized);
        }
        None
    }

    fn draw_plain_text(
        &self,
        matrix: &mut dyn MatrixBackend,
        text: &str,
        start_x: i32,
        start_y: i32,
        color: (u8, u8, u8),
        scale: f32,
    ) -> i32 {
        let font = self.base_renderer.font();
        let (pixels_by_char, text_width, _) = font.get_pixel_map(text, scale);

        for char_pixels in pixels_by_char {
            for (gx, gy) in char_pixels {
                matrix.set_pixel(start_x + gx, start_y + gy, color.0, color.1, color.2);
            }
        }
        text_width
    }

    pub fn render(&mut self, matrix: &mut dyn MatrixBackend, config: &Config) {
        let (symbols, ttl_min) = {
            let s = config.settings.read();
            (s.crypto_symbols.clone(), s.crypto_cache_ttl_min)
        };

        if symbols.is_empty() {
            return;
        }

        // Cycle through symbols every 5 seconds
        if self.last_switch.elapsed() > Duration::from_secs(5) {
            self.current_index = (self.current_index + 1) % symbols.len();
            self.last_switch = Instant::now();
        }

        let symbol = &symbols[self.current_index % symbols.len()];
        let (price, change, success, image_url) = self.fetch_quote(symbol, ttl_min as u64);

        let height = matrix.height();

        let price_str = if !success || price <= 0.0 {
            "Loading...".to_string()
        } else if price >= 1000.0 {
            format!("${:.0}", price)
        } else if price >= 1.0 {
            format!("${:.2}", price)
        } else {
            format!("${:.4}", price)
        };

        let pct_str = if !success || price <= 0.0 {
            "--".to_string()
        } else {
            format!("{}{:.2}%", if change >= 0.0 { "+" } else { "" }, change)
        };

        let badge_color = if !success || price <= 0.0 {
            Some((150, 150, 150))
        } else if change >= 0.0 {
            Some((0, 255, 120)) // Green
        } else {
            Some((255, 60, 60)) // Red
        };

        let badge_color_tuple = badge_color.unwrap_or((150, 150, 150));

        if height >= 64 {
            let scale = 2;
            let icon_x = 6;
            let icon_y = 6;

            if let Some(img) = self.get_and_load_icon(symbol, image_url.clone(), 16) {
                // Draw resized image with alpha blending over black
                for y in 0..img.height() {
                    for x in 0..img.width() {
                        let p = img.get_pixel(x, y);
                        if p[3] > 128 {
                            // Simple alpha threshold
                            matrix.set_pixel(
                                icon_x + x as i32,
                                icon_y + y as i32,
                                p[0],
                                p[1],
                                p[2],
                            );
                        }
                    }
                }
            } else {
                let icon = match symbol.as_str() {
                    "BTC" => &crate::engines::icons::ICON_BTC,
                    "ETH" => &crate::engines::icons::ICON_ETH,
                    "SOL" => &crate::engines::icons::ICON_SOL,
                    _ => &crate::engines::icons::ICON_BTC,
                };
                let icon_color = crate::engines::icons::get_crypto_color(symbol);
                crate::engines::icons::draw_icon(matrix, icon, icon_x, icon_y, scale, icon_color);
            }

            let sym_w = self.draw_plain_text(matrix, symbol, 28, 6, (255, 255, 255), 2.0);

            let font = self.base_renderer.font();
            let (_, price_w, _) = font.get_pixel_map(&price_str, 2.0);

            let mut price_x = matrix.width() as i32 - price_w - 6;
            if price_x < 28 + sym_w + 8 {
                price_x = 28 + sym_w + 8;
            }

            self.draw_plain_text(matrix, &price_str, price_x, 6, (255, 215, 0), 2.0);

            // Draw divider line
            for x in 6..(matrix.width() as i32 - 6) {
                matrix.set_pixel(x, 28, 60, 60, 60);
            }

            // Bottom Row: 24h Change
            let full_pct = if !success || price <= 0.0 {
                pct_str.clone()
            } else {
                format!("{} {}", if change >= 0.0 { "^" } else { "v" }, pct_str)
            };
            self.draw_plain_text(matrix, &full_pct, 6, 36, badge_color_tuple, 2.0);
        } else {
            let icon_x = 2;
            let icon_y = ((height as i32 - 16) / 2).max(0);

            if let Some(img) = self.get_and_load_icon(symbol, image_url.clone(), 16) {
                // Draw resized image with alpha blending over black
                for y in 0..img.height() {
                    for x in 0..img.width() {
                        let p = img.get_pixel(x, y);
                        if p[3] > 128 {
                            // Simple alpha threshold
                            matrix.set_pixel(
                                icon_x + x as i32,
                                icon_y + y as i32,
                                p[0],
                                p[1],
                                p[2],
                            );
                        }
                    }
                }
            } else {
                let icon = match symbol.as_str() {
                    "BTC" => &crate::engines::icons::ICON_BTC,
                    "ETH" => &crate::engines::icons::ICON_ETH,
                    "SOL" => &crate::engines::icons::ICON_SOL,
                    _ => &crate::engines::icons::ICON_BTC,
                };

                let icon_color = crate::engines::icons::get_crypto_color(symbol);
                crate::engines::icons::draw_icon(matrix, icon, icon_x, icon_y, 2, icon_color);
            }

            let sym_w = self.draw_plain_text(matrix, symbol, 20, 4, (255, 255, 255), 1.0);

            let price_x = 20 + sym_w + 6;
            self.draw_plain_text(matrix, &price_str, price_x, 4, (255, 215, 0), 1.0);

            self.draw_plain_text(matrix, &pct_str, 20, 18, badge_color_tuple, 1.0);
        }
    }
}

use crate::api::{CryptoProvider, PriceHistory, Timeframe};
use crate::core::engine_contract::{
    Capabilities, ConfigSchema, Engine, EngineConfig, EngineContext, EngineDescriptor, EngineError,
    EngineMetadata, Requirements,
};
use crate::core::matrix::MatrixBackend;
use crate::engines::dashboard::font::{draw_text_clipped, draw_text_scaled};
use crate::engines::renderers::{
    draw_fast_hline, draw_fast_vline, draw_round_rect, draw_sparkline, fill_round_rect,
};
use linkme::distributed_slice;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};
use tracing::warn;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CryptoPage {
    Info,
    Chart,
}

#[derive(Clone, Debug)]
pub struct CachedQuote {
    price: f64,
    change_24h: f64,
    last_fetch: Instant,
    has_data: bool,
    image_url: Option<String>,
}

pub struct CryptoEngine {
    cache: HashMap<String, CachedQuote>,
    history_cache: HashMap<(String, Timeframe), (PriceHistory, Instant)>,
    providers: Vec<Box<dyn CryptoProvider>>,
    current_index: usize,
    current_page: CryptoPage,
    last_page_switch: Instant,
    symbols: Vec<String>,
    currency: String,
    cache_ttl_min: u32,
    show_chart: bool,
    chart_timeframe: Timeframe,
    page_seconds: u64,
}

impl CryptoEngine {
    pub fn new(_w: u32, _h: u32) -> Self {
        Self {
            cache: HashMap::new(),
            history_cache: HashMap::new(),
            providers: Vec::new(),
            current_index: 0,
            current_page: CryptoPage::Info,
            last_page_switch: Instant::now(),
            symbols: vec![],
            currency: "USD".to_string(),
            cache_ttl_min: 1,
            show_chart: true,
            chart_timeframe: Timeframe::Daily,
            page_seconds: 5,
        }
    }

    pub fn add_provider(&mut self, provider: Box<dyn CryptoProvider>) {
        self.providers.push(provider);
    }

    pub fn currency_symbol(&self) -> &'static str {
        match self.currency.as_str() {
            "EUR" => "E",
            "GBP" => "L",
            "JPY" => "Y",
            _ => "$",
        }
    }

    /// Parse the instance config into engine state. Shared by `initialize()`
    /// and `on_config_changed()` so edits apply live without an app restart.
    fn apply_config(&mut self, config: &dyn EngineConfig) {
        let sym_str = config.get_string("symbols", "BTC,ETH");
        self.symbols = sym_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let new_curr = config.get_string("currency", "USD").to_uppercase();
        if self.currency != new_curr {
            self.currency = new_curr;
            self.cache.clear();
            self.history_cache.clear();
        }

        self.cache_ttl_min = config.get_int("cache_ttl_min", 1) as u32;
        self.show_chart = config.get_bool("show_chart", true);
        self.chart_timeframe =
            Timeframe::from_str_opt(&config.get_string("chart_timeframe", "daily"));
        self.page_seconds = config.get_int("page_seconds", 5).clamp(3, 30) as u64;

        // Keep the cursor in range after the symbol list shrinks.
        if self.symbols.is_empty() {
            self.current_index = 0;
        } else {
            self.current_index %= self.symbols.len();
        }
    }

    fn fetch_history(&mut self, symbol: &str, tf: Timeframe) -> Option<PriceHistory> {
        let now = Instant::now();
        let ttl_secs = match tf {
            Timeframe::Hourly => 60,    // 1 min
            Timeframe::Daily => 300,    // 5 min
            Timeframe::Weekly => 1800,  // 30 min
            Timeframe::Monthly => 7200, // 2 hours
        };

        if let Some((hist, ts)) = self.history_cache.get(&(symbol.to_string(), tf)) {
            if now.duration_since(*ts).as_secs() < ttl_secs {
                return Some(hist.clone());
            }
        }

        for provider in &self.providers {
            if let Some(hist) = provider.fetch_history_currency(symbol, tf, &self.currency) {
                self.history_cache
                    .insert((symbol.to_string(), tf), (hist.clone(), now));
                return Some(hist);
            }
        }

        if let Some((hist, _)) = self.history_cache.get(&(symbol.to_string(), tf)) {
            return Some(hist.clone());
        }

        None
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
            if let Some((price, change, img)) =
                provider.fetch_quote_currency(symbol, &self.currency)
            {
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

    fn format_price(&self, success: bool, price: f64) -> String {
        let cur_sym = self.currency_symbol();
        if !success || price <= 0.0 {
            "Loading...".to_string()
        } else if price >= 1000.0 {
            format!("{}{:.0}", cur_sym, price)
        } else if price >= 1.0 {
            format!("{}{:.2}", cur_sym, price)
        } else if price >= 0.001 {
            format!("{}{:.4}", cur_sym, price)
        } else {
            format!("{}{:.6}", cur_sym, price)
        }
    }

    fn render_quote(
        &self,
        matrix: &mut dyn MatrixBackend,
        symbol: &str,
        price_str: &str,
        pct_str: &str,
        badge_color: (u8, u8, u8),
        cached_img: Option<&image::RgbaImage>,
    ) {
        matrix.clear();
        let m_w = matrix.width() as i32;
        let m_h = matrix.height() as i32;

        let icon_x = 2;
        let mut icon_y = (m_h - 16) / 2;
        if icon_y < 0 {
            icon_y = 0;
        }

        draw_crypto_icon(matrix, symbol, icon_x, icon_y, 16, cached_img);

        if m_w < 48 {
            draw_glcd_text(matrix, symbol, 18, 2, (255, 255, 255));
            draw_glcd_text(matrix, price_str, 2, 12, (255, 215, 0));
            draw_glcd_text(matrix, pct_str, 2, 22, badge_color);
        } else {
            draw_glcd_text(matrix, symbol, 20, 4, (255, 255, 255));
            let price_x = 20 + symbol.len() as i32 * 6 + 6;
            draw_glcd_text(matrix, price_str, price_x, 4, (255, 215, 0));
            draw_glcd_text(matrix, pct_str, 20, 18, badge_color);
        }
    }

    fn render_chart(
        &self,
        matrix: &mut dyn MatrixBackend,
        symbol: &str,
        price_str: &str,
        history_opt: Option<&PriceHistory>,
    ) {
        matrix.clear();
        let m_w = matrix.width() as i32;
        let tf_label = self.chart_timeframe.label();

        let header_str = format!("{} {}", symbol, tf_label);
        draw_glcd_text(matrix, &header_str, 2, 1, (255, 255, 255));

        let mut price_x = m_w - (price_str.len() as i32 * 6 + 2);
        let min_price_x = 2 + (symbol.len() as i32 + 4) * 6;
        if price_x < min_price_x {
            price_x = min_price_x;
        }
        draw_glcd_text(matrix, price_str, price_x, 1, (255, 215, 0));

        let spark_x = 2;
        let spark_y = 11;
        let spark_w = matrix.width().saturating_sub(4);
        let spark_h = matrix.height().saturating_sub(13);

        if let Some(hist) = history_opt {
            if hist.points.len() > 1 {
                let is_up =
                    hist.points.last().unwrap_or(&0.0) >= hist.points.first().unwrap_or(&0.0);
                let line_color = if is_up { (0, 255, 120) } else { (255, 60, 60) };
                let fill_color = if is_up {
                    Some((0, 35, 12))
                } else {
                    Some((40, 12, 12))
                };
                draw_sparkline(
                    matrix, hist, spark_x, spark_y, spark_w, spark_h, line_color, fill_color,
                );
                return;
            }
        }
        draw_glcd_text(matrix, "Loading...", 4, spark_y + 4, (120, 120, 120));
    }

    fn render_unified_vertical(
        &self,
        matrix: &mut dyn MatrixBackend,
        symbol: &str,
        price_str: &str,
        pct_str: &str,
        badge_color: (u8, u8, u8),
        success: bool,
        price: f64,
        change: f64,
        cached_img: Option<&image::RgbaImage>,
        history_opt: Option<&PriceHistory>,
    ) {
        matrix.clear();
        let m_w = matrix.width() as i32;
        let tf_label = self.chart_timeframe.label();

        if m_w >= 48 {
            draw_crypto_icon(matrix, symbol, 2, 2, 16, cached_img);

            draw_glcd_text(matrix, symbol, 20, 2, (255, 255, 255));

            let mut tf_x = m_w - (tf_label.len() as i32 * 6 + 2);
            let min_tf_x = 20 + symbol.len() as i32 * 6 + 4;
            if tf_x < min_tf_x {
                tf_x = min_tf_x;
            }
            draw_glcd_text(matrix, tf_label, tf_x, 2, (140, 140, 140));

            draw_glcd_text(matrix, price_str, 20, 10, (255, 215, 0));

            let pct_text = if success && price > 0.0 {
                format!("{}{}", if change >= 0.0 { "^" } else { "v" }, pct_str)
            } else {
                pct_str.to_string()
            };
            draw_glcd_text(matrix, &pct_text, 2, 19, badge_color);

            draw_fast_hline(matrix, 2, 28, m_w - 4, (50, 50, 50));

            let spark_x = 2;
            let spark_y = 30;
            let spark_w = matrix.width().saturating_sub(4);
            let spark_h = matrix.height().saturating_sub(32);

            if let Some(hist) = history_opt {
                if hist.points.len() > 1 {
                    let is_up =
                        hist.points.last().unwrap_or(&0.0) >= hist.points.first().unwrap_or(&0.0);
                    let line_color = if is_up { (0, 255, 120) } else { (255, 60, 60) };
                    let fill_color = if is_up {
                        Some((0, 35, 12))
                    } else {
                        Some((40, 12, 12))
                    };
                    draw_sparkline(
                        matrix, hist, spark_x, spark_y, spark_w, spark_h, line_color, fill_color,
                    );
                    return;
                }
            }
            draw_glcd_text(
                matrix,
                "Loading...",
                4,
                spark_y + (spark_h as i32 / 2) - 3,
                (120, 120, 120),
            );
        } else {
            // Narrow Tate 32px
            draw_crypto_icon(matrix, symbol, 1, 1, 8, cached_img);

            draw_glcd_text(matrix, symbol, 11, 2, (255, 255, 255));
            draw_glcd_text(matrix, price_str, 1, 11, (255, 215, 0));
            draw_glcd_text(matrix, pct_str, 1, 20, badge_color);

            draw_fast_hline(matrix, 1, 28, m_w - 2, (50, 50, 50));

            let spark_x = 1;
            let spark_y = 30;
            let spark_w = matrix.width().saturating_sub(2);
            let spark_h = matrix.height().saturating_sub(32);

            if let Some(hist) = history_opt {
                if hist.points.len() > 1 {
                    let is_up =
                        hist.points.last().unwrap_or(&0.0) >= hist.points.first().unwrap_or(&0.0);
                    let line_color = if is_up { (0, 255, 120) } else { (255, 60, 60) };
                    let fill_color = if is_up {
                        Some((0, 35, 12))
                    } else {
                        Some((40, 12, 12))
                    };
                    draw_sparkline(
                        matrix, hist, spark_x, spark_y, spark_w, spark_h, line_color, fill_color,
                    );
                    return;
                }
            }
            draw_glcd_text(
                matrix,
                "...",
                2,
                spark_y + (spark_h as i32 / 2) - 3,
                (120, 120, 120),
            );
        }
    }

    fn render_unified_wide(
        &self,
        matrix: &mut dyn MatrixBackend,
        symbol: &str,
        price_str: &str,
        pct_str: &str,
        badge_color: (u8, u8, u8),
        success: bool,
        price: f64,
        change: f64,
        cached_img: Option<&image::RgbaImage>,
        history_opt: Option<&PriceHistory>,
    ) {
        matrix.clear();
        let m_w = matrix.width() as i32;
        let m_h = matrix.height() as i32;
        let tf_label = self.chart_timeframe.label();

        draw_crypto_icon(matrix, symbol, 4, 4, 16, cached_img);

        draw_glcd_text(matrix, symbol, 24, 4, (255, 255, 255));
        draw_glcd_text(matrix, tf_label, 24, 13, (140, 140, 140));
        draw_glcd_text(matrix, price_str, 4, 24, (255, 215, 0));

        let pct_text = if success && price > 0.0 {
            format!("{} {}", if change >= 0.0 { "^" } else { "v" }, pct_str)
        } else {
            pct_str.to_string()
        };
        draw_glcd_text(matrix, &pct_text, 4, 35, badge_color);

        let div_x = 58;
        draw_fast_vline(matrix, div_x, 4, m_h - 8, (50, 50, 50));

        let spark_x = 62;
        let spark_y = 6;
        let spark_w = matrix.width().saturating_sub(66);
        let spark_h = matrix.height().saturating_sub(12);

        if let Some(hist) = history_opt {
            if hist.points.len() > 1 {
                let is_up =
                    hist.points.last().unwrap_or(&0.0) >= hist.points.first().unwrap_or(&0.0);
                let line_color = if is_up { (0, 255, 120) } else { (255, 60, 60) };
                let fill_color = if is_up {
                    Some((0, 35, 12))
                } else {
                    Some((40, 12, 12))
                };
                draw_sparkline(
                    matrix, hist, spark_x, spark_y, spark_w, spark_h, line_color, fill_color,
                );
                return;
            }
        }
        draw_glcd_text(
            matrix,
            "Loading chart...",
            spark_x + 4,
            spark_y + (spark_h as i32 / 2) - 3,
            (120, 120, 120),
        );
    }

    fn render_fullscreen_quote(
        &self,
        matrix: &mut dyn MatrixBackend,
        symbol: &str,
        price_str: &str,
        pct_str: &str,
        badge_color: (u8, u8, u8),
        success: bool,
        price: f64,
        change: f64,
        cached_img: Option<&image::RgbaImage>,
    ) {
        matrix.clear();
        let m_w = matrix.width() as i32;
        let m_h = matrix.height() as i32;

        if m_w <= 64 {
            let icon_size = if m_w >= 48 { 16 } else { 8 };
            let icon_x = (m_w - icon_size) / 2;
            let price_len = price_str.len() as i32;
            let use_big_price = m_w >= 64 && price_len * 12 <= m_w - 4;

            let total_h = if use_big_price {
                icon_size + 3 + 8 + 4 + 16 + 4 + 9
            } else {
                icon_size + 4 + 8 + 4 + 8 + 4 + 9
            };
            let start_y = if m_h > total_h {
                (m_h - total_h) / 2
            } else {
                2
            };
            let icon_y = start_y;

            draw_crypto_icon(matrix, symbol, icon_x, icon_y, icon_size, cached_img);

            let mut sym_x = (m_w - symbol.len() as i32 * 6) / 2;
            if sym_x < 0 {
                sym_x = 0;
            }
            let sym_y = icon_y + icon_size + if use_big_price { 3 } else { 4 };
            draw_glcd_text(matrix, symbol, sym_x, sym_y, (255, 255, 255));

            let mut price_y = sym_y + 8 + 4;
            if use_big_price {
                let px = (m_w - price_len * 12) / 2;
                draw_glcd_text_scaled(matrix, price_str, px, price_y, 2, (255, 215, 0));
                price_y += 16 + 4;
            } else {
                let px = ((m_w - price_len * 6) / 2).max(0);
                draw_glcd_text(matrix, price_str, px, price_y, (255, 215, 0));
                price_y += 8 + 4;
            }

            let arrow_len = if success && price > 0.0 { 2 } else { 0 };
            let pct_len = pct_str.len() as i32 + arrow_len;
            let pct_w = pct_len * 6;
            let mut pct_x = (m_w - pct_w) / 2;
            if pct_x < 2 {
                pct_x = 2;
            }
            let mut pct_y = price_y;
            if pct_y > m_h - 10 {
                pct_y = m_h - 10;
            }

            let pill_bg = if change >= 0.0 {
                (0, 35, 12)
            } else {
                (45, 10, 10)
            };
            let pill_border = if change >= 0.0 {
                (0, 80, 25)
            } else {
                (90, 20, 20)
            };
            fill_round_rect(matrix, pct_x - 3, pct_y - 1, pct_w + 6, 10, 2, pill_bg);
            draw_round_rect(matrix, pct_x - 3, pct_y - 1, pct_w + 6, 10, 2, pill_border);

            let badge_text = if success && price > 0.0 {
                format!("{} {}", if change >= 0.0 { "^" } else { "v" }, pct_str)
            } else {
                pct_str.to_string()
            };
            draw_glcd_text(matrix, &badge_text, pct_x, pct_y, badge_color);
        } else {
            // Widescreen display without chart (128x64, 256x64)
            let mut icon_x = (m_w / 4) - 16;
            if icon_x < 4 {
                icon_x = 4;
            }
            let mut icon_y = (m_h - 32) / 2;
            if icon_y < 2 {
                icon_y = 2;
            }

            draw_crypto_icon(matrix, symbol, icon_x, icon_y, 32, cached_img);

            draw_fast_vline(matrix, m_w / 2 - 2, 8, m_h - 16, (40, 40, 40));

            let text_x = m_w / 2 + 8;
            let total_h = 16 + 4 + 16 + 4 + 10;
            let start_y = if m_h > total_h {
                (m_h - total_h) / 2
            } else {
                4
            };

            draw_glcd_text_scaled(matrix, symbol, text_x, start_y, 2, (255, 255, 255));
            draw_glcd_text_scaled(matrix, price_str, text_x, start_y + 19, 2, (255, 215, 0));

            let pct_y = start_y + 38;
            let arrow_len = if success && price > 0.0 { 2 } else { 0 };
            let pct_len = pct_str.len() as i32 + arrow_len;
            let pct_w = pct_len * 6;

            let pill_bg = if change >= 0.0 {
                (0, 35, 12)
            } else {
                (45, 10, 10)
            };
            let pill_border = if change >= 0.0 {
                (0, 80, 25)
            } else {
                (90, 20, 20)
            };
            fill_round_rect(matrix, text_x - 2, pct_y - 1, pct_w + 4, 10, 2, pill_bg);
            draw_round_rect(matrix, text_x - 2, pct_y - 1, pct_w + 4, 10, 2, pill_border);

            let badge_text = if success && price > 0.0 {
                format!("{} {}", if change >= 0.0 { "^" } else { "v" }, pct_str)
            } else {
                pct_str.to_string()
            };
            draw_glcd_text(matrix, &badge_text, text_x, pct_y, badge_color);
            draw_glcd_text(matrix, "24h", text_x + pct_w + 6, pct_y, (140, 140, 140));
        }
    }
} // End of impl CryptoEngine

fn draw_crypto_icon(
    matrix: &mut dyn MatrixBackend,
    symbol: &str,
    icon_x: i32,
    icon_y: i32,
    size: i32,
    cached_img: Option<&image::RgbaImage>,
) {
    if let Some(img) = cached_img {
        let img_w = img.width() as i32;
        let img_h = img.height() as i32;
        for y in 0..size {
            for x in 0..size {
                let src_x = ((x * img_w) / size).min(img_w - 1) as u32;
                let src_y = ((y * img_h) / size).min(img_h - 1) as u32;
                let p = img.get_pixel(src_x, src_y);
                if p[3] > 64 {
                    matrix.set_pixel(icon_x + x, icon_y + y, p[0], p[1], p[2]);
                }
            }
        }
    } else {
        let icon = match symbol {
            "BTC" => &crate::engines::icons::ICON_BTC,
            "ETH" => &crate::engines::icons::ICON_ETH,
            "SOL" => &crate::engines::icons::ICON_SOL,
            _ => &crate::engines::icons::ICON_BTC,
        };
        let color = crate::engines::icons::get_crypto_color(symbol);
        let scale = (size / 8).max(1);
        crate::engines::icons::draw_icon(matrix, icon, icon_x, icon_y, scale, color);
    }
}

#[inline]
fn draw_glcd_text(matrix: &mut dyn MatrixBackend, text: &str, x: i32, y: i32, color: (u8, u8, u8)) {
    let w = matrix.width() as i32;
    let h = matrix.height() as i32;
    draw_text_clipped(matrix, text, x, y, 0, w, 0, h, color);
}

#[inline]
fn draw_glcd_text_scaled(
    matrix: &mut dyn MatrixBackend,
    text: &str,
    x: i32,
    y: i32,
    scale: i32,
    color: (u8, u8, u8),
) {
    let w = matrix.width() as i32;
    let h = matrix.height() as i32;
    draw_text_scaled(matrix, text, x, y, 0, w, 0, h, scale, color);
}

impl Engine for CryptoEngine {
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
    }

    fn activate(&mut self) {}
    fn deactivate(&mut self) {}
    fn update(&mut self, _context: &mut EngineContext) {}

    fn render(&mut self, context: &mut EngineContext) {
        if self.symbols.is_empty() {
            return;
        }

        let matrix = &mut *context.matrix;
        let width = matrix.width();
        let height = matrix.height();

        let page_duration = Duration::from_secs(self.page_seconds);
        if self.last_page_switch.elapsed() >= page_duration {
            self.last_page_switch = Instant::now();
            if height >= 64 || !self.show_chart {
                self.current_page = CryptoPage::Info;
                self.current_index = (self.current_index + 1) % self.symbols.len();
            } else {
                match self.current_page {
                    CryptoPage::Info => {
                        self.current_page = CryptoPage::Chart;
                    }
                    CryptoPage::Chart => {
                        self.current_page = CryptoPage::Info;
                        self.current_index = (self.current_index + 1) % self.symbols.len();
                    }
                }
            }
        }

        let symbol = self.symbols[self.current_index % self.symbols.len()].clone();
        let (price, change, success, image_url) =
            self.fetch_quote(&symbol, self.cache_ttl_min as u64);
        let history_opt = if self.show_chart {
            self.fetch_history(&symbol, self.chart_timeframe)
        } else {
            None
        };

        let price_str = self.format_price(success, price);
        let pct_str = if !success || price <= 0.0 {
            "--".to_string()
        } else {
            format!("{}{:.2}%", if change >= 0.0 { "+" } else { "" }, change)
        };
        let badge_color = if !success || price <= 0.0 {
            (150, 150, 150)
        } else if change >= 0.0 {
            (0, 255, 120)
        } else {
            (255, 60, 60)
        };

        let loaded_icon = self.get_and_load_icon(&symbol, image_url, 16);

        if !self.show_chart {
            if height >= 64 {
                self.render_fullscreen_quote(
                    matrix,
                    &symbol,
                    &price_str,
                    &pct_str,
                    badge_color,
                    success,
                    price,
                    change,
                    loaded_icon.as_ref(),
                );
            } else {
                self.render_quote(
                    matrix,
                    &symbol,
                    &price_str,
                    &pct_str,
                    badge_color,
                    loaded_icon.as_ref(),
                );
            }
        } else if height >= 64 {
            if width <= 64 {
                self.render_unified_vertical(
                    matrix,
                    &symbol,
                    &price_str,
                    &pct_str,
                    badge_color,
                    success,
                    price,
                    change,
                    loaded_icon.as_ref(),
                    history_opt.as_ref(),
                );
            } else {
                self.render_unified_wide(
                    matrix,
                    &symbol,
                    &price_str,
                    &pct_str,
                    badge_color,
                    success,
                    price,
                    change,
                    loaded_icon.as_ref(),
                    history_opt.as_ref(),
                );
            }
        } else if self.current_page == CryptoPage::Info {
            self.render_quote(
                matrix,
                &symbol,
                &price_str,
                &pct_str,
                badge_color,
                loaded_icon.as_ref(),
            );
        } else {
            self.render_chart(matrix, &symbol, &price_str, history_opt.as_ref());
        }
    }
}

#[distributed_slice(crate::core::registry::ENGINES)]
fn register_crypto_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "crypto",
            name: "CryptoEngine",
            category: "finance",
            version: crate::core::build_info::VERSION,
        },
        capabilities: Capabilities::default(),
        requirements: Requirements::default(),
        available: true,
        unavailable_reason: None,
        schema: ConfigSchema {
            fields: vec![
                crate::core::engine_contract::ConfigField {
                    id: "symbols",
                    field_type: crate::core::engine_contract::ConfigType::String,
                    label: "Symbols",
                    description: "Comma-separated crypto symbols (e.g. BTC,ETH)",
                    default_value: "BTC,ETH",
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Accept,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "show_chart",
                    field_type: crate::core::engine_contract::ConfigType::Boolean,
                    label: "Show Chart",
                    description: "Display historical price sparkline chart screen",
                    default_value: "true",
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Accept,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "chart_timeframe",
                    field_type: crate::core::engine_contract::ConfigType::Options,
                    label: "Chart Timeframe",
                    description: "Timeframe for historical price chart",
                    default_value: "daily",
                    options: Some(vec![
                        crate::core::engine_contract::ConfigOption {
                            label: "1 Hour",
                            value: "hourly",
                        },
                        crate::core::engine_contract::ConfigOption {
                            label: "1 Day",
                            value: "daily",
                        },
                        crate::core::engine_contract::ConfigOption {
                            label: "1 Week",
                            value: "weekly",
                        },
                        crate::core::engine_contract::ConfigOption {
                            label: "1 Month",
                            value: "monthly",
                        },
                    ]),
                    validation_policy:
                        crate::core::engine_contract::ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "page_seconds",
                    field_type: crate::core::engine_contract::ConfigType::Integer,
                    label: "Page Seconds",
                    description: "Seconds to dwell on each page before cycling",
                    default_value: "5",
                    min_val: Some("3"),
                    max_val: Some("30"),
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Clamp,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "currency",
                    field_type: crate::core::engine_contract::ConfigType::Options,
                    label: "Currency",
                    description: "Fiat currency for quotes (USD, EUR, GBP, JPY)",
                    default_value: "USD",
                    options: Some(vec![
                        crate::core::engine_contract::ConfigOption {
                            label: "USD ($)",
                            value: "USD",
                        },
                        crate::core::engine_contract::ConfigOption {
                            label: "EUR (€)",
                            value: "EUR",
                        },
                        crate::core::engine_contract::ConfigOption {
                            label: "GBP (£)",
                            value: "GBP",
                        },
                        crate::core::engine_contract::ConfigOption {
                            label: "JPY (¥)",
                            value: "JPY",
                        },
                    ]),
                    validation_policy:
                        crate::core::engine_contract::ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "cache_ttl_min",
                    field_type: crate::core::engine_contract::ConfigType::Integer,
                    label: "Cache TTL (min)",
                    description: "Minutes to cache quote price",
                    default_value: "1",
                    min_val: Some("1"),
                    max_val: Some("60"),
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Clamp,
                    ..Default::default()
                },
            ],
        },
        factory: || -> Box<dyn crate::core::engine_contract::Engine> {
            let mut engine = crate::engines::crypto::CryptoEngine::new(64, 32);
            engine.add_provider(Box::new(crate::api::coingecko::CoinGeckoProvider));
            engine.add_provider(Box::new(crate::api::binance::BinanceProvider));
            Box::new(engine)
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::matrix::MockMatrix;

    #[test]
    fn test_crypto_price_formatting() {
        let mut engine = CryptoEngine::new(64, 32);
        engine.currency = "USD".to_string();
        assert_eq!(engine.format_price(false, 0.0), "Loading...");
        assert_eq!(engine.format_price(true, 65432.1), "$65432");
        assert_eq!(engine.format_price(true, 123.456), "$123.46");
        assert_eq!(engine.format_price(true, 0.0456), "$0.0456");
        assert_eq!(engine.format_price(true, 0.000123), "$0.000123");

        engine.currency = "EUR".to_string();
        assert_eq!(engine.format_price(true, 2500.0), "E2500");
        engine.currency = "GBP".to_string();
        assert_eq!(engine.format_price(true, 2500.0), "L2500");
        engine.currency = "JPY".to_string();
        assert_eq!(engine.format_price(true, 2500.0), "Y2500");
    }

    #[test]
    fn test_crypto_render_horizontal_64x32() {
        let mut engine = CryptoEngine::new(64, 32);
        engine.symbols = vec!["BTC".to_string()];
        engine.cache.insert(
            "BTC".to_string(),
            CachedQuote {
                price: 90000.0,
                change_24h: 3.5,
                last_fetch: Instant::now(),
                has_data: true,
                image_url: None,
            },
        );

        let config = crate::core::config::Config::new("config.json");
        let mut matrix = MockMatrix::new(64, 32);
        let mut context = EngineContext {
            matrix: &mut matrix,
            config: &config,
        };

        // Quote mode
        engine.show_chart = false;
        engine.render(&mut context);

        // Chart mode
        engine.show_chart = true;
        engine.current_page = CryptoPage::Chart;
        engine.render(&mut context);
    }

    #[test]
    fn test_crypto_render_vertical_tate_32x64() {
        let mut engine = CryptoEngine::new(32, 64);
        engine.symbols = vec!["ETH".to_string()];
        engine.cache.insert(
            "ETH".to_string(),
            CachedQuote {
                price: 3200.0,
                change_24h: -1.25,
                last_fetch: Instant::now(),
                has_data: true,
                image_url: None,
            },
        );

        let config = crate::core::config::Config::new("config.json");
        let mut matrix = MockMatrix::new(32, 64);
        let mut context = EngineContext {
            matrix: &mut matrix,
            config: &config,
        };

        // Unified vertical with chart
        engine.show_chart = true;
        engine.render(&mut context);

        // Fullscreen quote without chart
        engine.show_chart = false;
        engine.render(&mut context);
    }

    #[test]
    fn test_crypto_render_square_and_wide() {
        let mut engine = CryptoEngine::new(64, 64);
        engine.symbols = vec!["SOL".to_string()];
        engine.cache.insert(
            "SOL".to_string(),
            CachedQuote {
                price: 185.5,
                change_24h: 8.2,
                last_fetch: Instant::now(),
                has_data: true,
                image_url: None,
            },
        );

        let config = crate::core::config::Config::new("config.json");

        // 64x64 Square
        let mut matrix_square = MockMatrix::new(64, 64);
        let mut ctx_square = EngineContext {
            matrix: &mut matrix_square,
            config: &config,
        };
        engine.show_chart = true;
        engine.render(&mut ctx_square);
        engine.show_chart = false;
        engine.render(&mut ctx_square);

        // 128x64 Wide
        let mut matrix_wide = MockMatrix::new(128, 64);
        let mut ctx_wide = EngineContext {
            matrix: &mut matrix_wide,
            config: &config,
        };
        engine.show_chart = true;
        engine.render(&mut ctx_wide);
        engine.show_chart = false;
        engine.render(&mut ctx_wide);
    }
}

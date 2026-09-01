use crate::api::{CryptoProvider, PriceHistory, Timeframe};
use crate::core::engine_contract::{
    Capabilities, ConfigSchema, Engine, EngineConfig, EngineContext, EngineDescriptor, EngineError,
    EngineMetadata, Requirements,
};
use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::{draw_sparkline, BaseRenderer};
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
    base_renderer: BaseRenderer,
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
            base_renderer: BaseRenderer::new(),
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
            "EUR" => "€",
            "GBP" => "£",
            "JPY" => "¥",
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
} // End of impl CryptoEngine

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
        let ttl_min = self.cache_ttl_min;

        if self.symbols.is_empty() {
            return;
        }

        // Cycle through pages and symbols based on page_seconds
        let page_duration = Duration::from_secs(self.page_seconds);
        if self.last_page_switch.elapsed() >= page_duration {
            if self.show_chart {
                match self.current_page {
                    CryptoPage::Info => {
                        self.current_page = CryptoPage::Chart;
                    }
                    CryptoPage::Chart => {
                        self.current_page = CryptoPage::Info;
                        self.current_index = (self.current_index + 1) % self.symbols.len();
                    }
                }
            } else {
                self.current_page = CryptoPage::Info;
                self.current_index = (self.current_index + 1) % self.symbols.len();
            }
            self.last_page_switch = Instant::now();
        }

        let symbol = self.symbols[self.current_index % self.symbols.len()].clone();
        let (price, change, success, image_url) = self.fetch_quote(&symbol, ttl_min as u64);

        let matrix = &mut *context.matrix;
        let width = matrix.width();
        let height = matrix.height();

        let sym_prefix = self.currency_symbol();
        let price_str = if !success || price <= 0.0 {
            "Loading...".to_string()
        } else if price >= 1000.0 {
            format!("{}{:.0}", sym_prefix, price)
        } else if price >= 1.0 {
            format!("{}{:.2}", sym_prefix, price)
        } else {
            format!("{}{:.4}", sym_prefix, price)
        };

        let pct_str = if !success || price <= 0.0 {
            "--".to_string()
        } else {
            format!("{}{:.2}%", if change >= 0.0 { "+" } else { "" }, change)
        };

        let badge_color_tuple = if !success || price <= 0.0 {
            (150, 150, 150)
        } else if change >= 0.0 {
            (0, 255, 120) // Green
        } else {
            (255, 60, 60) // Red
        };

        if !self.show_chart {
            if height >= 64 {
                let scale = 2;
                let icon_x = 6;
                let icon_y = 6;

                if let Some(img) = self.get_and_load_icon(&symbol, image_url.clone(), 16) {
                    for y in 0..img.height() {
                        for x in 0..img.width() {
                            let p = img.get_pixel(x, y);
                            if p[3] > 128 {
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
                    let icon_color = crate::engines::icons::get_crypto_color(&symbol);
                    crate::engines::icons::draw_icon(
                        matrix, icon, icon_x, icon_y, scale, icon_color,
                    );
                }

                let sym_w = self.draw_plain_text(matrix, &symbol, 28, 6, (255, 255, 255), 2.0);

                let font = self.base_renderer.font();
                let (_, price_w, _) = font.get_pixel_map(&price_str, 2.0);

                let mut price_x = width as i32 - price_w - 6;
                if price_x < 28 + sym_w + 8 {
                    price_x = 28 + sym_w + 8;
                }

                self.draw_plain_text(matrix, &price_str, price_x, 6, (255, 215, 0), 2.0);

                // Divider line
                for x in 6..(width as i32 - 6) {
                    matrix.set_pixel(x, 28, 60, 60, 60);
                }

                // 24h Change
                let full_pct = if !success || price <= 0.0 {
                    pct_str.clone()
                } else {
                    format!("{} {}", if change >= 0.0 { "^" } else { "v" }, pct_str)
                };
                self.draw_plain_text(matrix, &full_pct, 6, 36, badge_color_tuple, 2.0);
            } else {
                let icon_x = 2;
                let icon_y = ((height as i32 - 16) / 2).max(0);

                if let Some(img) = self.get_and_load_icon(&symbol, image_url.clone(), 16) {
                    for y in 0..img.height() {
                        for x in 0..img.width() {
                            let p = img.get_pixel(x, y);
                            if p[3] > 128 {
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

                    let icon_color = crate::engines::icons::get_crypto_color(&symbol);
                    crate::engines::icons::draw_icon(matrix, icon, icon_x, icon_y, 2, icon_color);
                }

                let sym_w = self.draw_plain_text(matrix, &symbol, 20, 4, (255, 255, 255), 1.0);

                let price_x = 20 + sym_w + 6;
                self.draw_plain_text(matrix, &price_str, price_x, 4, (255, 215, 0), 1.0);

                self.draw_plain_text(matrix, &pct_str, 20, 18, badge_color_tuple, 1.0);
            }
        } else if height >= 64 {
            let history_opt = self.fetch_history(&symbol, self.chart_timeframe);
            let tf_label = self.chart_timeframe.label();

            if width <= 64 {
                // --- Unified Vertical (64x64, 32x64 Tate) ---
                let icon_x = 2;
                let icon_y = 2;

                if let Some(img) = self.get_and_load_icon(&symbol, image_url.clone(), 16) {
                    for y in 0..img.height() {
                        for x in 0..img.width() {
                            let p = img.get_pixel(x, y);
                            if p[3] > 128 {
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
                    let icon_color = crate::engines::icons::get_crypto_color(&symbol);
                    crate::engines::icons::draw_icon(matrix, icon, icon_x, icon_y, 2, icon_color);
                }

                let sym_w = self.draw_plain_text(matrix, &symbol, 20, 2, (255, 255, 255), 1.0);

                let font = self.base_renderer.font();
                let (_, tf_w, _) = font.get_pixel_map(tf_label, 1.0);
                let tf_x = (width as i32 - tf_w - 2).max(20 + sym_w + 4);
                self.draw_plain_text(matrix, tf_label, tf_x, 2, (140, 140, 140), 1.0);

                self.draw_plain_text(matrix, &price_str, 20, 10, (255, 215, 0), 1.0);

                let full_pct = if !success || price <= 0.0 {
                    pct_str.clone()
                } else {
                    format!("{}{}", if change >= 0.0 { "^" } else { "v" }, pct_str)
                };
                self.draw_plain_text(matrix, &full_pct, 2, 19, badge_color_tuple, 1.0);

                for x in 2..(width as i32 - 2) {
                    matrix.set_pixel(x, 28, 50, 50, 50);
                }

                let spark_x = 2;
                let spark_y = 30;
                let spark_w = width.saturating_sub(4);
                let spark_h = (height as i32 - 32).max(10) as u32;

                if let Some(hist) = history_opt {
                    let is_up =
                        hist.points.last().unwrap_or(&0.0) >= hist.points.first().unwrap_or(&0.0);
                    let line_color = if is_up { (0, 255, 120) } else { (255, 60, 60) };
                    let fill_color = if is_up {
                        Some((0, 30, 10))
                    } else {
                        Some((35, 10, 10))
                    };
                    draw_sparkline(
                        matrix, &hist, spark_x, spark_y, spark_w, spark_h, line_color, fill_color,
                    );
                } else {
                    self.draw_plain_text(
                        matrix,
                        "Loading...",
                        4,
                        spark_y + 10,
                        (120, 120, 120),
                        1.0,
                    );
                }
            } else {
                // --- Unified Wide (128x64, 256x64) ---
                let icon_x = 4;
                let icon_y = 4;

                if let Some(img) = self.get_and_load_icon(&symbol, image_url.clone(), 16) {
                    for y in 0..img.height() {
                        for x in 0..img.width() {
                            let p = img.get_pixel(x, y);
                            if p[3] > 128 {
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
                    let icon_color = crate::engines::icons::get_crypto_color(&symbol);
                    crate::engines::icons::draw_icon(matrix, icon, icon_x, icon_y, 2, icon_color);
                }

                self.draw_plain_text(matrix, &symbol, 24, 4, (255, 255, 255), 1.0);
                self.draw_plain_text(matrix, tf_label, 24, 13, (140, 140, 140), 1.0);
                self.draw_plain_text(matrix, &price_str, 4, 24, (255, 215, 0), 1.0);

                let full_pct = if !success || price <= 0.0 {
                    pct_str.clone()
                } else {
                    format!("{} {}", if change >= 0.0 { "^" } else { "v" }, pct_str)
                };
                self.draw_plain_text(matrix, &full_pct, 4, 35, badge_color_tuple, 1.0);

                for y in 4..(height as i32 - 4) {
                    matrix.set_pixel(62, y, 40, 40, 40);
                }

                let spark_x = 66;
                let spark_y = 6;
                let spark_w = width.saturating_sub(70);
                let spark_h = (height as i32 - 12).max(10) as u32;

                if let Some(hist) = history_opt {
                    let is_up =
                        hist.points.last().unwrap_or(&0.0) >= hist.points.first().unwrap_or(&0.0);
                    let line_color = if is_up { (0, 255, 120) } else { (255, 60, 60) };
                    let fill_color = if is_up {
                        Some((0, 30, 10))
                    } else {
                        Some((35, 10, 10))
                    };
                    draw_sparkline(
                        matrix, &hist, spark_x, spark_y, spark_w, spark_h, line_color, fill_color,
                    );
                } else {
                    self.draw_plain_text(
                        matrix,
                        "Loading...",
                        spark_x + 4,
                        spark_y + 20,
                        (120, 120, 120),
                        1.0,
                    );
                }
            }
        } else {
            // --- 32px height panel: Alternate between Info and Chart ---
            match self.current_page {
                CryptoPage::Info => {
                    let icon_x = 2;
                    let icon_y = ((height as i32 - 16) / 2).max(0);

                    if let Some(img) = self.get_and_load_icon(&symbol, image_url.clone(), 16) {
                        for y in 0..img.height() {
                            for x in 0..img.width() {
                                let p = img.get_pixel(x, y);
                                if p[3] > 128 {
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

                        let icon_color = crate::engines::icons::get_crypto_color(&symbol);
                        crate::engines::icons::draw_icon(
                            matrix, icon, icon_x, icon_y, 2, icon_color,
                        );
                    }

                    let sym_w = self.draw_plain_text(matrix, &symbol, 20, 4, (255, 255, 255), 1.0);

                    let price_x = 20 + sym_w + 6;
                    self.draw_plain_text(matrix, &price_str, price_x, 4, (255, 215, 0), 1.0);

                    self.draw_plain_text(matrix, &pct_str, 20, 18, badge_color_tuple, 1.0);
                }
                CryptoPage::Chart => {
                    let history_opt = self.fetch_history(&symbol, self.chart_timeframe);
                    let tf_label = self.chart_timeframe.label();

                    let header_text = format!("{} {}", symbol, tf_label);
                    let sym_w =
                        self.draw_plain_text(matrix, &header_text, 2, 1, (255, 255, 255), 1.0);

                    let font = self.base_renderer.font();
                    let (_, price_w, _) = font.get_pixel_map(&price_str, 1.0);
                    let price_x = (width as i32 - price_w - 2).max(2 + sym_w + 4);
                    self.draw_plain_text(matrix, &price_str, price_x, 1, (255, 215, 0), 1.0);

                    let spark_x = 2;
                    let spark_y = 12;
                    let spark_w = width.saturating_sub(4);
                    let spark_h = 19;

                    if let Some(hist) = history_opt {
                        let is_up = hist.points.last().unwrap_or(&0.0)
                            >= hist.points.first().unwrap_or(&0.0);
                        let line_color = if is_up { (0, 255, 120) } else { (255, 60, 60) };
                        let fill_color = if is_up {
                            Some((0, 30, 10))
                        } else {
                            Some((35, 10, 10))
                        };
                        draw_sparkline(
                            matrix, &hist, spark_x, spark_y, spark_w, spark_h, line_color,
                            fill_color,
                        );
                    } else {
                        self.draw_plain_text(
                            matrix,
                            "Loading...",
                            4,
                            spark_y + 4,
                            (120, 120, 120),
                            1.0,
                        );
                    }
                }
            }
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

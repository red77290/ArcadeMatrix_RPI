use crate::core::config::Config;
use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::BaseRenderer;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{info, warn};

#[derive(Clone, Debug)]
struct CachedQuote {
    price: f64,
    change_24h: f64,
    last_fetch: Instant,
    has_data: bool,
}

pub struct StockEngine {
    base_renderer: BaseRenderer,
    cache: HashMap<String, CachedQuote>,
    current_index: usize,
    last_switch: Instant,
}

impl StockEngine {
    pub fn new(_w: u32, _h: u32) -> Self {
        Self {
            base_renderer: BaseRenderer::new(),
            cache: HashMap::new(),
            current_index: 0,
            last_switch: Instant::now(),
        }
    }

    fn fetch_quote(&mut self, symbol: &str, cache_ttl_min: u32) -> (f64, f64, bool) {
        let ttl =
            Duration::from_secs((if cache_ttl_min > 0 { cache_ttl_min } else { 1 } * 60) as u64);
        let now = Instant::now();

        // 1. Check if cache is fresh
        if let Some(c) = self.cache.get(symbol) {
            if c.has_data && now.duration_since(c.last_fetch) < ttl {
                return (c.price, c.change_24h, true);
            }
        }

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(3))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .build()
            .ok();

        if let Some(ref c) = client {
            // Yahoo Finance v8 Chart API
            let url1 = format!(
                "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d&range=1d",
                symbol
            );
            let url2 = format!(
                "https://query2.finance.yahoo.com/v8/finance/chart/{}?interval=1d&range=1d",
                symbol
            );

            let mut res = c.get(&url1).send();
            if res.is_err() {
                res = c.get(&url2).send();
            }

            if let Ok(resp) = res {
                if resp.status().is_success() {
                    if let Ok(json) = resp.json::<serde_json::Value>() {
                        if let Some(meta) = json["chart"]["result"][0]["meta"].as_object() {
                            let price = meta
                                .get("regularMarketPrice")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0);
                            let prev_close = meta
                                .get("previousClose")
                                .or_else(|| meta.get("chartPreviousClose"))
                                .and_then(|v| v.as_f64())
                                .unwrap_or(price);

                            let change = if prev_close > 0.0 && price > 0.0 {
                                ((price - prev_close) / prev_close) * 100.0
                            } else {
                                0.0
                            };

                            if price > 0.0 {
                                self.cache.insert(
                                    symbol.to_string(),
                                    CachedQuote {
                                        price,
                                        change_24h: change,
                                        last_fetch: now,
                                        has_data: true,
                                    },
                                );
                                info!(
                                    "[Yahoo Stock] Quote for {}: ${:.2} ({:.2}%)",
                                    symbol, price, change
                                );
                                return (price, change, true);
                            }
                        }
                    }
                }
            }
        }

        // Fallback to last known cached quote for THIS symbol if HTTP failed
        if let Some(c) = self.cache.get(symbol) {
            if c.has_data {
                warn!(
                    "[HTTP Failed] Reusing last known cached stock price for {}: ${:.2}",
                    symbol, c.price
                );
                return (c.price, c.change_24h, true);
            }
        }

        (0.0, 0.0, false)
    }

    pub fn render(&mut self, matrix: &mut dyn MatrixBackend, config: &Config) {
        let (symbols, ttl_min) = {
            let s = config.settings.read();
            (s.stock_symbols.clone(), s.stock_cache_ttl_min)
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
        let (price, change, success) = self.fetch_quote(symbol, ttl_min);

        let height = matrix.height();

        let price_str = if !success || price <= 0.0 {
            "Loading...".to_string()
        } else {
            format!("${:.2}", price)
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

        if height >= 64 {
            // Prominent 2-Row Layout (Size 2)
            // Top Row: Symbol & Price
            self.base_renderer
                .render_text(matrix, symbol, 0, 2, 6, 6, Some((255, 255, 255)), None);

            self.base_renderer.render_text(
                matrix,
                &price_str,
                0,
                2,
                70,
                6,
                Some((0, 220, 255)), // Cyan
                None,
            );

            // Bottom Row: 24h Change
            let full_pct = format!("{} {}", if change >= 0.0 { "^" } else { "v" }, pct_str);
            self.base_renderer
                .render_text(matrix, &full_pct, 0, 2, 6, 36, badge_color, None);
        } else {
            // Standard Resolution (32px high)
            let line1 = format!("{} {}", symbol, price_str);
            self.base_renderer
                .render_text(matrix, &line1, 0, 1, 2, 4, Some((0, 220, 255)), None);

            self.base_renderer
                .render_text(matrix, &pct_str, 0, 1, 2, 18, badge_color, None);
        }
    }
}

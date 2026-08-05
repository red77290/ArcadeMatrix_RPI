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

pub struct CryptoEngine {
    base_renderer: BaseRenderer,
    cache: HashMap<String, CachedQuote>,
    current_index: usize,
    last_switch: Instant,
}

impl CryptoEngine {
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

        let lower_symbol = symbol.to_lowercase();

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(3))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .build()
            .ok();

        if let Some(ref c) = client {
            // 2a. CoinGecko Markets API
            let cg_url = format!(
                "https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&symbols={}",
                lower_symbol
            );
            if let Ok(res) = c.get(&cg_url).send() {
                if res.status().is_success() {
                    if let Ok(json) = res.json::<serde_json::Value>() {
                        if let Some(arr) = json.as_array() {
                            if let Some(coin) = arr.first() {
                                let price = coin["current_price"].as_f64().unwrap_or(0.0);
                                let change =
                                    coin["price_change_percentage_24h"].as_f64().unwrap_or(0.0);
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
                                        "[CoinGecko] Quote for {}: ${:.4} ({:.2}%)",
                                        symbol, price, change
                                    );
                                    return (price, change, true);
                                }
                            }
                        }
                    }
                }
            }

            // 2b. CoinGecko Simple API by Coin ID (handles ERGO, FLUX, KASPA, etc.)
            let coin_id = if lower_symbol == "erg" {
                "ergo"
            } else {
                &lower_symbol
            };
            let cg_simple_url = format!(
                "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd&include_24hr_change=true",
                coin_id
            );
            if let Ok(res) = c.get(&cg_simple_url).send() {
                if res.status().is_success() {
                    if let Ok(json) = res.json::<serde_json::Value>() {
                        if let Some(coin) = json.get(coin_id) {
                            let price = coin["usd"].as_f64().unwrap_or(0.0);
                            let change = coin["usd_24h_change"].as_f64().unwrap_or(0.0);
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
                                    "[CoinGecko ID] Quote for {}: ${:.4} ({:.2}%)",
                                    symbol, price, change
                                );
                                return (price, change, true);
                            }
                        }
                    }
                }
            }

            // 2c. Binance Fallback API
            let mut binance_symbol = symbol.to_uppercase();
            if !binance_symbol.ends_with("USDT") && !binance_symbol.ends_with("USD") {
                binance_symbol.push_str("USDT");
            }
            let binance_url = format!(
                "https://api.binance.com/api/v3/ticker/24hr?symbol={}",
                binance_symbol
            );
            if let Ok(res) = c.get(&binance_url).send() {
                if res.status().is_success() {
                    if let Ok(json) = res.json::<serde_json::Value>() {
                        let price = json["lastPrice"]
                            .as_str()
                            .and_then(|s| s.parse::<f64>().ok())
                            .unwrap_or(0.0);
                        let change = json["priceChangePercent"]
                            .as_str()
                            .and_then(|s| s.parse::<f64>().ok())
                            .unwrap_or(0.0);
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
                                "[Binance] Quote for {}: ${:.4} ({:.2}%)",
                                symbol, price, change
                            );
                            return (price, change, true);
                        }
                    }
                }
            }
        }

        // 3. Fallback to last known quote for THIS symbol if HTTP failed
        if let Some(c) = self.cache.get(symbol) {
            if c.has_data {
                warn!(
                    "[HTTP Failed] Reusing last known cached price for {}: ${:.4}",
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
        let (price, change, success) = self.fetch_quote(symbol, ttl_min);

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
                Some((255, 215, 0)), // Gold
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
                .render_text(matrix, &line1, 0, 1, 2, 4, Some((255, 215, 0)), None);

            self.base_renderer
                .render_text(matrix, &pct_str, 0, 1, 2, 18, badge_color, None);
        }
    }
}

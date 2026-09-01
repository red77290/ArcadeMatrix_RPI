use crate::api::{CryptoProvider, PriceHistory, Timeframe};
use std::time::Duration;
use tracing::info;

pub struct BinanceProvider;

impl CryptoProvider for BinanceProvider {
    fn fetch_quote(&self, symbol: &str) -> Option<(f64, f64, Option<String>)> {
        self.fetch_quote_currency(symbol, "USD")
    }

    fn fetch_quote_currency(
        &self,
        symbol: &str,
        currency: &str,
    ) -> Option<(f64, f64, Option<String>)> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(3))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .build()
            .ok()?;

        let upper_sym = symbol.to_uppercase();
        let pair_suffix = match currency.to_uppercase().as_str() {
            "EUR" => "EUR",
            "GBP" => "GBP",
            "JPY" => "JPY",
            _ => "USDT",
        };

        let binance_symbol = if upper_sym.ends_with(pair_suffix) {
            upper_sym
        } else {
            format!("{}{}", upper_sym, pair_suffix)
        };

        let url = format!(
            "https://api.binance.com/api/v3/ticker/24hr?symbol={}",
            binance_symbol
        );

        if let Ok(res) = client.get(&url).send() {
            let status = res.status();
            if status.is_success() {
                if let Ok(json) = res.json::<serde_json::Value>() {
                    if let Some((price, change)) = Self::parse(&json) {
                        info!(
                            "[Binance] Quote for {} ({}): {:.4} ({:.2}%)",
                            symbol, currency, price, change
                        );
                        return Some((price, change, None));
                    }
                }
            } else {
                tracing::warn!(
                    "[Binance] HTTP {} for {} (451 = geo-blocked region)",
                    status.as_u16(),
                    symbol
                );
            }
        } else {
            tracing::warn!("[Binance] Request failed for {} (network/DNS/TLS?)", symbol);
        }

        None
    }

    fn fetch_history(&self, symbol: &str, tf: Timeframe) -> Option<PriceHistory> {
        self.fetch_history_currency(symbol, tf, "USD")
    }

    fn fetch_history_currency(
        &self,
        symbol: &str,
        tf: Timeframe,
        currency: &str,
    ) -> Option<PriceHistory> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(3))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .build()
            .ok()?;

        let upper_sym = symbol.to_uppercase();
        let pair_suffix = match currency.to_uppercase().as_str() {
            "EUR" => "EUR",
            "GBP" => "GBP",
            "JPY" => "JPY",
            _ => "USDT",
        };

        let binance_symbol = if upper_sym.ends_with(pair_suffix) {
            upper_sym
        } else {
            format!("{}{}", upper_sym, pair_suffix)
        };

        let (interval, limit) = match tf {
            Timeframe::Hourly => ("1m", 60),
            Timeframe::Daily => ("1h", 24),
            Timeframe::Weekly => ("4h", 42),
            Timeframe::Monthly => ("1d", 30),
        };

        let url = format!(
            "https://api.binance.com/api/v3/klines?symbol={}&interval={}&limit={}",
            binance_symbol, interval, limit
        );

        if let Ok(res) = client.get(&url).send() {
            if res.status().is_success() {
                if let Ok(json) = res.json::<serde_json::Value>() {
                    if let Some(hist) = Self::parse_klines(&json) {
                        info!(
                            "[Binance] Fetched {} history points for {} ({:?})",
                            hist.points.len(),
                            symbol,
                            tf
                        );
                        return Some(hist);
                    }
                }
            } else {
                tracing::warn!(
                    "[Binance] History HTTP {} for {}",
                    res.status().as_u16(),
                    symbol
                );
            }
        }

        None
    }
}

impl BinanceProvider {
    pub fn parse(json: &serde_json::Value) -> Option<(f64, f64)> {
        let price = json["lastPrice"]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let change = json["priceChangePercent"]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);

        if price > 0.0 {
            Some((price, change))
        } else {
            None
        }
    }

    pub fn parse_klines(json: &serde_json::Value) -> Option<PriceHistory> {
        if let Some(candles) = json.as_array() {
            let mut raw_points = Vec::with_capacity(candles.len());
            for candle in candles {
                if let Some(close_str) = candle.get(4).and_then(|v| v.as_str()) {
                    if let Ok(close_val) = close_str.parse::<f64>() {
                        if close_val > 0.0 {
                            raw_points.push(close_val);
                        }
                    }
                }
            }
            return PriceHistory::from_raw(&raw_points);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_binance() {
        let payload =
            json!({"symbol": "BTCUSDT", "lastPrice": "62000.00", "priceChangePercent": "1.5"});
        let (price, change) = BinanceProvider::parse(&payload).unwrap();
        assert_eq!(price, 62000.0);
        assert_eq!(change, 1.5);
    }

    #[test]
    fn test_parse_klines() {
        let payload = json!([
            [1610000000000u64, "100.0", "105.0", "99.0", "102.5", "1000"],
            [1610003600000u64, "102.5", "108.0", "101.0", "107.0", "1500"],
            [1610007200000u64, "107.0", "107.0", "95.0", "96.0", "2000"]
        ]);
        let hist = BinanceProvider::parse_klines(&payload).unwrap();
        assert_eq!(hist.points.len(), 3);
        assert_eq!(hist.min, 96.0);
        assert_eq!(hist.max, 107.0);
    }
}

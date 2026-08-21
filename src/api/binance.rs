use crate::api::CryptoProvider;
use std::time::Duration;
use tracing::info;

pub struct BinanceProvider;

impl CryptoProvider for BinanceProvider {
    fn fetch_quote(&self, symbol: &str) -> Option<(f64, f64, Option<String>)> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(3))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .build()
            .ok()?;

        let mut binance_symbol = symbol.to_uppercase();
        if !binance_symbol.ends_with("USDT") && !binance_symbol.ends_with("USD") {
            binance_symbol.push_str("USDT");
        }

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
                            "[Binance] Quote for {}: ${:.4} ({:.2}%)",
                            symbol, price, change
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
}

use crate::api::{CryptoProvider, PriceHistory, Timeframe};
use std::time::Duration;
use tracing::info;

pub struct CoinGeckoProvider;

impl CryptoProvider for CoinGeckoProvider {
    fn fetch_quote(&self, symbol: &str) -> Option<(f64, f64, Option<String>)> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(3))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .build()
            .ok()?;

        let lower_symbol = symbol.to_lowercase();

        // 1. Primary API
        let cg_url = format!(
            "https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&symbols={}",
            lower_symbol
        );

        if let Ok(res) = client.get(&cg_url).send() {
            let status = res.status();
            if status.is_success() {
                if let Ok(json) = res.json::<serde_json::Value>() {
                    if let Some((price, change, img_url)) = Self::parse_primary(&json) {
                        info!(
                            "[CoinGecko Primary] Quote for {}: ${:.4} ({:.2}%)",
                            symbol, price, change
                        );
                        return Some((price, change, img_url));
                    }
                }
            } else {
                tracing::warn!(
                    "[CoinGecko Primary] HTTP {} for {} (429 = rate limited)",
                    status.as_u16(),
                    symbol
                );
            }
        } else {
            tracing::warn!(
                "[CoinGecko Primary] Request failed for {} (network/DNS/TLS?)",
                symbol
            );
        }

        // 2. Simple API
        let coin_id = if lower_symbol == "erg" {
            "ergo"
        } else {
            &lower_symbol
        };
        let cg_simple_url = format!(
            "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd&include_24hr_change=true",
            coin_id
        );

        if let Ok(res) = client.get(&cg_simple_url).send() {
            if res.status().is_success() {
                if let Ok(json) = res.json::<serde_json::Value>() {
                    if let Some((price, change)) = Self::parse_simple(&json, coin_id) {
                        info!(
                            "[CoinGecko Simple] Quote for {}: ${:.4} ({:.2}%)",
                            symbol, price, change
                        );
                        return Some((price, change, None));
                    }
                }
            }
        }

        None
    }

    fn fetch_history(&self, symbol: &str, tf: Timeframe) -> Option<PriceHistory> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(3))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .build()
            .ok()?;

        let lower = symbol.to_lowercase();
        let coin_id = match lower.as_str() {
            "btc" => "bitcoin",
            "eth" => "ethereum",
            "sol" => "solana",
            "erg" => "ergo",
            "doge" => "dogecoin",
            "ada" => "cardano",
            "xrp" => "ripple",
            "dot" => "polkadot",
            "link" => "chainlink",
            "avax" => "avalanche-2",
            _ => lower.as_str(),
        };

        let days = match tf {
            Timeframe::Hourly => "1",
            Timeframe::Daily => "1",
            Timeframe::Weekly => "7",
            Timeframe::Monthly => "30",
        };

        let url = format!(
            "https://api.coingecko.com/api/v3/coins/{}/market_chart?vs_currency=usd&days={}",
            coin_id, days
        );

        if let Ok(res) = client.get(&url).send() {
            if res.status().is_success() {
                if let Ok(json) = res.json::<serde_json::Value>() {
                    if let Some(hist) = Self::parse_market_chart(&json) {
                        info!(
                            "[CoinGecko] Fetched {} history points for {} ({:?})",
                            hist.points.len(),
                            symbol,
                            tf
                        );
                        return Some(hist);
                    }
                }
            } else {
                tracing::warn!(
                    "[CoinGecko] History HTTP {} for {}",
                    res.status().as_u16(),
                    symbol
                );
            }
        }

        None
    }
}

impl CoinGeckoProvider {
    pub fn parse_primary(json: &serde_json::Value) -> Option<(f64, f64, Option<String>)> {
        if let Some(arr) = json.as_array() {
            if let Some(coin) = arr.first() {
                let price = coin["current_price"].as_f64().unwrap_or(0.0);
                let change = coin["price_change_percentage_24h"].as_f64().unwrap_or(0.0);
                let image = coin["image"].as_str().map(|s| s.to_string());
                if price > 0.0 {
                    return Some((price, change, image));
                }
            }
        }
        None
    }

    pub fn parse_simple(json: &serde_json::Value, coin_id: &str) -> Option<(f64, f64)> {
        if let Some(coin) = json.get(coin_id) {
            let price = coin["usd"].as_f64().unwrap_or(0.0);
            let change = coin["usd_24h_change"].as_f64().unwrap_or(0.0);
            if price > 0.0 {
                return Some((price, change));
            }
        }
        None
    }

    pub fn parse_market_chart(json: &serde_json::Value) -> Option<PriceHistory> {
        if let Some(prices_arr) = json["prices"].as_array() {
            let mut raw_points = Vec::with_capacity(prices_arr.len());
            for item in prices_arr {
                if let Some(p) = item.get(1).and_then(|v| v.as_f64()) {
                    if p > 0.0 {
                        raw_points.push(p);
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
    fn test_parse_primary() {
        let payload = json!([{"current_price": 61234.56, "price_change_percentage_24h": 2.45, "image": "http://img"}]);
        let (price, change, img) = CoinGeckoProvider::parse_primary(&payload).unwrap();
        assert_eq!(price, 61234.56);
        assert_eq!(change, 2.45);
        assert_eq!(img.unwrap(), "http://img");
    }

    #[test]
    fn test_parse_simple() {
        let payload = json!({"ergo": {"usd": 1.23, "usd_24h_change": -5.12}});
        let (price, change) = CoinGeckoProvider::parse_simple(&payload, "ergo").unwrap();
        assert_eq!(price, 1.23);
        assert_eq!(change, -5.12);
    }

    #[test]
    fn test_parse_market_chart() {
        let payload = json!({
            "prices": [
                [1610000000000u64, 100.0],
                [1610003600000u64, 105.5],
                [1610007200000u64, 98.2]
            ]
        });
        let hist = CoinGeckoProvider::parse_market_chart(&payload).unwrap();
        assert_eq!(hist.points.len(), 3);
        assert_eq!(hist.min, 98.2);
        assert_eq!(hist.max, 105.5);
    }
}

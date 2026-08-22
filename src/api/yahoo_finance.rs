use crate::api::StockProvider;
use std::time::Duration;
use tracing::info;

pub struct YahooFinanceProvider;

impl StockProvider for YahooFinanceProvider {
    fn fetch_quote(&self, symbol: &str) -> Option<(f64, f64, Option<String>)> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(3))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .build()
            .ok()?;

        let urls = [
            format!(
                "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d&range=1d",
                symbol
            ),
            format!(
                "https://query2.finance.yahoo.com/v8/finance/chart/{}?interval=1d&range=1d",
                symbol
            ),
        ];

        for url in urls {
            if let Ok(res) = client.get(&url).send() {
                let status = res.status();
                if status.is_success() {
                    if let Ok(json) = res.json::<serde_json::Value>() {
                        if let Some((price, change)) = Self::parse(&json) {
                            info!(
                                "[Yahoo] Quote for {}: ${:.2} ({:.2}%)",
                                symbol, price, change
                            );
                            let img_url = format!(
                                "https://financialmodelingprep.com/image-stock/{}.png",
                                symbol.to_uppercase()
                            );
                            return Some((price, change, Some(img_url)));
                        }
                    }
                } else {
                    tracing::warn!("[Yahoo] HTTP {} for {}", status.as_u16(), symbol);
                }
            } else {
                tracing::warn!("[Yahoo] Request failed for {} (network/DNS/TLS?)", symbol);
            }
        }

        None
    }
}

impl YahooFinanceProvider {
    pub fn parse(json: &serde_json::Value) -> Option<(f64, f64)> {
        if let Some(arr) = json["chart"]["result"].as_array() {
            if let Some(result) = arr.first() {
                let meta = &result["meta"];
                let price = meta["regularMarketPrice"].as_f64().unwrap_or(0.0);

                let prev_close = meta["previousClose"]
                    .as_f64()
                    .or_else(|| meta["chartPreviousClose"].as_f64())
                    .unwrap_or(price);

                if price > 0.0 {
                    let change = if prev_close > 0.0 {
                        ((price - prev_close) / prev_close) * 100.0
                    } else {
                        0.0
                    };
                    return Some((price, change));
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_yahoo() {
        let payload = json!({
            "chart": {
                "result": [{
                    "meta": {
                        "regularMarketPrice": 150.25,
                        "previousClose": 148.00
                    }
                }]
            }
        });
        let (price, change) = YahooFinanceProvider::parse(&payload).unwrap();
        assert_eq!(price, 150.25);
        assert!((change - 1.52).abs() < 0.01);
    }
}

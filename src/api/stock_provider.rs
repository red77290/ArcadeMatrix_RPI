use crate::api::history::{PriceHistory, Timeframe};

pub trait StockProvider: Send + Sync {
    /// Fetches the quote for the given symbol.
    /// Returns a tuple of (price, 24h_change, image_url) if successful.
    fn fetch_quote(&self, symbol: &str) -> Option<(f64, f64, Option<String>)>;

    /// Fetches historical price points for the given timeframe.
    fn fetch_history(&self, _symbol: &str, _tf: Timeframe) -> Option<PriceHistory> {
        None
    }
}

use crate::api::history::{PriceHistory, Timeframe};

pub trait CryptoProvider: Send + Sync {
    /// Fetches the quote for the given symbol (defaults to USD).
    /// Returns a tuple of (price, 24h_change, image_url) if successful.
    fn fetch_quote(&self, symbol: &str) -> Option<(f64, f64, Option<String>)> {
        self.fetch_quote_currency(symbol, "USD")
    }

    /// Fetches the quote for the given symbol and currency (e.g. USD, EUR, GBP, JPY).
    fn fetch_quote_currency(
        &self,
        symbol: &str,
        _currency: &str,
    ) -> Option<(f64, f64, Option<String>)> {
        self.fetch_quote(symbol)
    }

    /// Fetches historical price points for the given timeframe and currency.
    fn fetch_history(&self, _symbol: &str, _tf: Timeframe) -> Option<PriceHistory> {
        None
    }

    /// Fetches historical price points for the given timeframe and currency.
    fn fetch_history_currency(
        &self,
        symbol: &str,
        tf: Timeframe,
        _currency: &str,
    ) -> Option<PriceHistory> {
        self.fetch_history(symbol, tf)
    }
}

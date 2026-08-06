pub trait CryptoProvider: Send + Sync {
    /// Fetches the quote for the given symbol.
    /// Returns a tuple of (price, 24h_change, image_url) if successful.
    fn fetch_quote(&self, symbol: &str) -> Option<(f64, f64, Option<String>)>;
}

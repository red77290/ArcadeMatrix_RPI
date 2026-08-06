#[derive(Clone, Debug, PartialEq)]
pub struct DayForecast {
    pub label: String,
    pub temp: String,
    pub icon: String,
}

pub trait WeatherProvider: Send + Sync {
    /// Fetches the weather forecast for the given city and language.
    /// Returns a vector of DayForecast if successful.
    fn fetch_forecast(&self, api_key: &str, city: &str, lang: &str) -> Option<Vec<DayForecast>>;
}

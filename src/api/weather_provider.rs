#[derive(Clone, Debug, PartialEq)]
pub struct DayForecast {
    pub label: String,
    pub temp: String,
    pub temp_min: String,
    pub temp_max: String,
    pub condition: String,
    pub icon: String,
}

pub trait WeatherProvider: Send + Sync {
    /// Fetches the weather forecast for the given city, language, and units (metric/imperial).
    /// Returns a vector of DayForecast if successful.
    fn fetch_forecast(
        &self,
        api_key: &str,
        city: &str,
        lang: &str,
        units: &str,
    ) -> Option<Vec<DayForecast>>;
}

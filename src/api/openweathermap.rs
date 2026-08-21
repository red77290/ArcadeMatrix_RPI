use crate::api::{DayForecast, WeatherProvider};
use chrono::{Datelike, Local};
use serde::Deserialize;
use std::time::Duration;
use tracing::info;

pub struct OpenWeatherMapProvider;

#[derive(Deserialize)]
struct ForecastMain {
    temp: f32,
    temp_min: f32,
    temp_max: f32,
}

#[derive(Deserialize)]
struct ForecastWeather {
    icon: String,
}

#[derive(Deserialize)]
struct ForecastEntry {
    main: ForecastMain,
    weather: Vec<ForecastWeather>,
}

#[derive(Deserialize)]
struct ForecastApiResponse {
    list: Vec<ForecastEntry>,
}

impl WeatherProvider for OpenWeatherMapProvider {
    fn fetch_forecast(&self, api_key: &str, city: &str, lang: &str) -> Option<Vec<DayForecast>> {
        if city.trim().is_empty() {
            tracing::warn!("[OpenWeatherMap] No city configured; cannot fetch forecast.");
            return None;
        }

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .ok()?;

        let url = format!(
            "https://api.openweathermap.org/data/2.5/forecast?q={}&appid={}&units=metric&lang={}",
            city, api_key, lang
        );

        let res = match client.get(&url).send() {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("[OpenWeatherMap] Request failed (network/DNS?): {}", e);
                return None;
            }
        };

        let status = res.status();
        if !status.is_success() {
            // Surface the API's own error message (invalid/inactive key -> 401,
            // unknown city -> 404) instead of silently rendering "--°C".
            let body = res.text().unwrap_or_default();
            let hint = match status.as_u16() {
                401 => " (invalid or not-yet-activated API key)",
                404 => " (city not found — check spelling, e.g. \"Paris,FR\")",
                429 => " (rate limit exceeded)",
                _ => "",
            };
            tracing::warn!(
                "[OpenWeatherMap] HTTP {}{} for city '{}': {}",
                status.as_u16(),
                hint,
                city,
                body.trim()
            );
            return None;
        }

        let json = match res.json::<serde_json::Value>() {
            Ok(j) => j,
            Err(e) => {
                tracing::warn!("[OpenWeatherMap] Failed to decode JSON response: {}", e);
                return None;
            }
        };

        let wday = Local::now().weekday().number_from_sunday() - 1; // 0=Sunday
        match Self::parse(&json, lang, true, wday) {
            Some(forecasts) => {
                info!(
                    "[OpenWeatherMap] Parsed {} days for {}",
                    forecasts.len(),
                    city
                );
                Some(forecasts)
            }
            None => {
                tracing::warn!(
                    "[OpenWeatherMap] Response parsed but contained no usable forecast entries for '{}'.",
                    city
                );
                None
            }
        }
    }
}

impl OpenWeatherMapProvider {
    pub fn parse(
        json: &serde_json::Value,
        lang: &str,
        have_time: bool,
        current_wday: u32,
    ) -> Option<Vec<DayForecast>> {
        let data: Result<ForecastApiResponse, _> = serde_json::from_value(json.clone());
        if let Ok(data) = data {
            let indices = [0usize, 8, 16];

            let mut labels = vec![];
            if lang.eq_ignore_ascii_case("fr") {
                labels.push("AUJ.");
                labels.push("DEMN");
                let fr_days = ["DIM", "LUN", "MAR", "MER", "JEU", "VEN", "SAM"];
                labels.push(fr_days[((current_wday + 2) % 7) as usize]);
            } else if lang.eq_ignore_ascii_case("es") {
                labels.push("HOY");
                labels.push("MANA");
                let es_days = ["DOM", "LUN", "MAR", "MIE", "JUE", "VIE", "SAB"];
                labels.push(es_days[((current_wday + 2) % 7) as usize]);
            } else {
                labels.push("TODAY");
                labels.push("TMRW");
                let en_days = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
                labels.push(en_days[((current_wday + 2) % 7) as usize]);
            }

            let forecasts: Vec<DayForecast> = indices
                .iter()
                .zip(labels.iter())
                .filter_map(|(&idx, &label)| {
                    data.list.get(idx).map(|entry| DayForecast {
                        label: label.to_string(),
                        temp: format!(
                            "{:.0}°C ({:.0}/{:.0})",
                            entry.main.temp, entry.main.temp_min, entry.main.temp_max
                        ),
                        icon: entry
                            .weather
                            .get(0)
                            .map(|w| w.icon.clone())
                            .unwrap_or_default(),
                    })
                })
                .collect();

            if !forecasts.is_empty() {
                return Some(forecasts);
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
    fn test_parse_openweathermap() {
        let payload = json!({
            "list": [
                {
                    "main": {"temp": 22.5, "temp_min": 20.0, "temp_max": 25.0},
                    "weather": [{"icon": "01d"}]
                },
                {"main": {"temp": 23.0, "temp_min": 20.0, "temp_max": 25.0}, "weather": [{"icon": "01d"}]},
                {"main": {"temp": 24.0, "temp_min": 20.0, "temp_max": 25.0}, "weather": [{"icon": "01d"}]},
                {"main": {"temp": 25.0, "temp_min": 20.0, "temp_max": 25.0}, "weather": [{"icon": "01d"}]},
                {"main": {"temp": 26.0, "temp_min": 20.0, "temp_max": 25.0}, "weather": [{"icon": "01d"}]},
                {"main": {"temp": 27.0, "temp_min": 20.0, "temp_max": 25.0}, "weather": [{"icon": "01d"}]},
                {"main": {"temp": 28.0, "temp_min": 20.0, "temp_max": 25.0}, "weather": [{"icon": "01d"}]},
                {"main": {"temp": 29.0, "temp_min": 20.0, "temp_max": 25.0}, "weather": [{"icon": "01d"}]},
                {
                    "main": {"temp": 30.0, "temp_min": 25.0, "temp_max": 35.0},
                    "weather": [{"icon": "02d"}]
                }
            ]
        });

        // current_wday = 0 (Sunday), so Day 3 should be Tuesday (TUE in EN)
        let forecasts = OpenWeatherMapProvider::parse(&payload, "en", true, 0).unwrap();
        assert_eq!(forecasts.len(), 2); // Because list only has 9 items, index 16 is missing
        assert_eq!(forecasts[0].label, "TODAY");
        assert_eq!(forecasts[0].icon, "01d");
        assert_eq!(forecasts[1].label, "TMRW");
        assert_eq!(forecasts[1].icon, "02d");
    }
}

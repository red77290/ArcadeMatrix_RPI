use crate::api::{DayForecast, WeatherProvider};
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
    #[serde(default)]
    main: Option<String>,
    #[serde(default)]
    description: Option<String>,
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

fn translate_condition(main: &str, desc: &str, lang: &str) -> String {
    let m = main.to_lowercase();
    let d = desc.to_lowercase();
    let l = lang.to_lowercase();

    if l == "fr" {
        if m.contains("clear") {
            return "Soleil".to_string();
        }
        if m.contains("cloud") {
            if d.contains("couvert") || d.contains("overcast") {
                return "Couvert".to_string();
            }
            if d.contains("part") || d.contains("peu") || d.contains("scat") {
                return "Eclaircies".to_string();
            }
            return "Nuageux".to_string();
        }
        if m.contains("rain") || m.contains("drizzle") {
            return "Pluie".to_string();
        }
        if m.contains("thunder") {
            return "Orage".to_string();
        }
        if m.contains("snow") {
            return "Neige".to_string();
        }
        if m.contains("mist") || m.contains("fog") || m.contains("haze") {
            return "Brume".to_string();
        }
        if !desc.is_empty() {
            let mut c = desc.chars();
            return match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            };
        }
        return "Meteo".to_string();
    } else if l == "es" {
        if m.contains("clear") {
            return "Soleado".to_string();
        }
        if m.contains("cloud") {
            return "Nublado".to_string();
        }
        if m.contains("rain") || m.contains("drizzle") {
            return "Lluvia".to_string();
        }
        if m.contains("thunder") {
            return "Tormenta".to_string();
        }
        if m.contains("snow") {
            return "Nieve".to_string();
        }
        if m.contains("mist") || m.contains("fog") {
            return "Niebla".to_string();
        }
    }

    if m.contains("clear") {
        return "Clear".to_string();
    }
    if m.contains("cloud") {
        return "Clouds".to_string();
    }
    if m.contains("rain") {
        return "Rain".to_string();
    }
    if m.contains("drizzle") {
        return "Drizzle".to_string();
    }
    if m.contains("thunder") {
        return "Thunder".to_string();
    }
    if m.contains("snow") {
        return "Snow".to_string();
    }
    if m.contains("mist") || m.contains("fog") {
        return "Fog".to_string();
    }
    main.to_string()
}

impl WeatherProvider for OpenWeatherMapProvider {
    fn fetch_forecast(
        &self,
        api_key: &str,
        city: &str,
        lang: &str,
        units: &str,
    ) -> Option<Vec<DayForecast>> {
        if city.trim().is_empty() {
            tracing::warn!("[OpenWeatherMap] No city configured; cannot fetch forecast.");
            return None;
        }
        if api_key.trim().is_empty() {
            tracing::warn!("[OpenWeatherMap] No API key configured; cannot fetch forecast.");
            return None;
        }

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .ok()?;

        let req_lang = if lang.is_empty() { "fr" } else { lang };
        let req_units = if units.eq_ignore_ascii_case("imperial")
            || units.eq_ignore_ascii_case("fahrenheit")
            || units.eq_ignore_ascii_case("f")
        {
            "imperial"
        } else {
            "metric"
        };

        let encoded_city = city.trim().replace(' ', "%20");
        let url = format!(
            "https://api.openweathermap.org/data/2.5/forecast?q={}&appid={}&units={}&lang={}",
            encoded_city, api_key, req_units, req_lang
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
            let body = res.text().unwrap_or_default();
            let hint = match status.as_u16() {
                401 => " (invalid or not-yet-activated API key)",
                404 => " (city not found — check spelling, e.g. \"Paris,FR\" or \"Tucson,AZ,US\")",
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

        let local_now = chrono::Local::now();
        let current_wday = local_now
            .format("%w")
            .to_string()
            .parse::<u32>()
            .unwrap_or(0);
        let parsed = Self::parse(&json, req_lang, req_units, true, current_wday);
        if let Some(ref list) = parsed {
            info!(
                "[OpenWeatherMap] Parsed {} days for {} (units: {})",
                list.len(),
                city,
                req_units
            );
        } else {
            tracing::warn!(
                "[OpenWeatherMap] Response parsed but contained no usable forecast entries for '{}'.",
                city
            );
        }
        parsed
    }
}

impl OpenWeatherMapProvider {
    pub fn parse(
        json: &serde_json::Value,
        lang: &str,
        units: &str,
        _have_time: bool,
        current_wday: u32,
    ) -> Option<Vec<DayForecast>> {
        if let Ok(data) = serde_json::from_value::<ForecastApiResponse>(json.clone()) {
            let indices = [0, 8, 16];
            let mut labels = Vec::new();

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

            let unit_sym = if units.eq_ignore_ascii_case("imperial")
                || units.eq_ignore_ascii_case("fahrenheit")
                || units.eq_ignore_ascii_case("f")
            {
                "°F"
            } else {
                "°C"
            };

            let forecasts: Vec<DayForecast> = indices
                .iter()
                .enumerate()
                .filter_map(|(day_idx, &idx)| {
                    data.list.get(idx).map(|entry| {
                        let start_k = day_idx * 8;
                        let end_k = ((day_idx + 1) * 8).min(data.list.len());
                        let mut day_min = entry.main.temp_min.min(entry.main.temp);
                        let mut day_max = entry.main.temp_max.max(entry.main.temp);
                        for k in start_k..end_k {
                            if let Some(e) = data.list.get(k) {
                                day_min = day_min.min(e.main.temp_min).min(e.main.temp);
                                day_max = day_max.max(e.main.temp_max).max(e.main.temp);
                            }
                        }

                        let raw_main = entry
                            .weather
                            .get(0)
                            .and_then(|w| w.main.as_deref())
                            .unwrap_or("");
                        let raw_desc = entry
                            .weather
                            .get(0)
                            .and_then(|w| w.description.as_deref())
                            .unwrap_or("");
                        let cond = translate_condition(raw_main, raw_desc, lang);

                        DayForecast {
                            label: labels[day_idx].to_string(),
                            temp: format!("{:.0}{}", entry.main.temp, unit_sym),
                            temp_min: format!("{:.0}{}", day_min, unit_sym),
                            temp_max: format!("{:.0}{}", day_max, unit_sym),
                            condition: cond,
                            icon: entry
                                .weather
                                .get(0)
                                .map(|w| w.icon.clone())
                                .unwrap_or_default(),
                        }
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
                    "weather": [{"icon": "01d", "main": "Clear"}]
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
                    "weather": [{"icon": "02d", "main": "Clouds"}]
                }
            ]
        });

        // current_wday = 0 (Sunday), so Day 3 should be Tuesday (TUE in EN)
        let forecasts = OpenWeatherMapProvider::parse(&payload, "en", "metric", true, 0).unwrap();
        assert_eq!(forecasts.len(), 2);
        assert_eq!(forecasts[0].label, "TODAY");
        assert_eq!(forecasts[0].temp, "22°C");
        assert_eq!(forecasts[0].temp_min, "20°C");
        assert_eq!(forecasts[0].temp_max, "29°C");
        assert_eq!(forecasts[0].condition, "Clear");
        assert_eq!(forecasts[0].icon, "01d");
        assert_eq!(forecasts[1].label, "TMRW");
        assert_eq!(forecasts[1].temp, "30°C");
        assert_eq!(forecasts[1].temp_min, "25°C");
        assert_eq!(forecasts[1].temp_max, "35°C");
        assert_eq!(forecasts[1].condition, "Clouds");
        assert_eq!(forecasts[1].icon, "02d");
    }
}

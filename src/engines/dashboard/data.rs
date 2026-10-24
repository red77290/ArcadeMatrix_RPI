use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Default)]
pub struct MarketQuote {
    pub symbol: String,
    pub price: f32,
    pub change_24h: f32,
}

#[derive(Debug, Clone, Default)]
pub struct WorldTimeQuote {
    pub code: String,
    pub offset_hours: i32,
}

#[derive(Debug, Clone, Default)]
pub struct DashboardData {
    pub temp_c: f32,
    pub weather_code: i32,
    pub weather_desc: String,
    pub cpu_usage: f32,
    pub ram_usage: f32,
    pub wifi_rssi: i32,
    pub markets: Vec<MarketQuote>,
    pub world_times: Vec<WorldTimeQuote>,
}

pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

pub fn parse_world_clocks(raw_str: &str) -> Vec<WorldTimeQuote> {
    let mut list = Vec::new();
    for item in raw_str.split(',') {
        let code = item.trim().to_uppercase();
        if code.is_empty() {
            continue;
        }
        let offset = match code.as_str() {
            "NYC" | "EST" | "EDT" => -5,
            "LAX" | "SFO" | "PST" => -8,
            "CHI" | "CST" => -6,
            "TYO" | "JST" => 9,
            "LON" | "GMT" | "UTC" => 0,
            "PAR" | "BER" | "MAD" | "ROM" | "AMS" | "CET" => 1,
            "DXB" => 4,
            "SIN" | "HKG" => 8,
            "SYD" | "AEST" => 10,
            "YUL" => -5,
            _ => 0,
        };
        list.push(WorldTimeQuote {
            code,
            offset_hours: offset,
        });
    }
    list
}

pub fn format_market_price(price: f32) -> String {
    if price <= 0.0 {
        "--".to_string()
    } else if price >= 100000.0 {
        format!("${:.0}k", price / 1000.0)
    } else if price >= 1000.0 {
        format!("${:.1}k", price / 1000.0)
    } else if price >= 10.0 {
        format!("${:.1}", price)
    } else {
        format!("${:.2}", price)
    }
}

pub fn fetch_live_weather(city: &str, api_key: &str) -> Option<(f32, i32, String)> {
    let trimmed_city = city.trim();
    if trimmed_city.is_empty() {
        return None;
    }
    let client = crate::core::memory::fast_http_client();

    // 1. Try OpenWeatherMap if an API key is provided (either directly or inherited from Weather engine)
    let clean_key = api_key.trim();
    if clean_key.len() > 5 && clean_key != "API_KEY" {
        let encoded_city = trimmed_city.replace(' ', "%20");
        let owm_url = format!(
            "https://api.openweathermap.org/data/2.5/weather?q={}&appid={}&units=metric",
            encoded_city, clean_key
        );
        if let Ok(res) = client.get(&owm_url).send() {
            if res.status().is_success() {
                if let Ok(json) = res.json::<serde_json::Value>() {
                    if let Some(temp_c) = json
                        .get("main")
                        .and_then(|m| m.get("temp"))
                        .and_then(|t| t.as_f64())
                    {
                        let weather_obj = json.get("weather").and_then(|w| w.get(0));
                        let code = weather_obj
                            .and_then(|w| w.get("id"))
                            .and_then(|id| id.as_i64())
                            .unwrap_or(800) as i32;
                        let desc = weather_obj
                            .and_then(|w| w.get("main"))
                            .and_then(|m| m.as_str())
                            .unwrap_or("Clear")
                            .to_string();
                        return Some((temp_c as f32, code, desc));
                    }
                }
            }
        }
    }

    // 2. Free Open-Meteo fallback (No API key needed, aligned with ESP32 DashboardDataProvider)
    let base_city = trimmed_city
        .split(',')
        .next()
        .unwrap_or(trimmed_city)
        .trim();
    let mut lat = 48.8566_f64;
    let mut lon = 2.3522_f64;

    if !base_city.eq_ignore_ascii_case("Paris") && !base_city.is_empty() {
        let enc_city = base_city.replace(' ', "%20");
        let geo_url = format!(
            "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&format=json",
            enc_city
        );
        if let Ok(res) = client.get(&geo_url).send() {
            if res.status().is_success() {
                if let Ok(json) = res.json::<serde_json::Value>() {
                    if let Some(first) = json
                        .get("results")
                        .and_then(|r| r.as_array())
                        .and_then(|a| a.first())
                    {
                        if let (Some(la), Some(lo)) = (
                            first.get("latitude").and_then(|v| v.as_f64()),
                            first.get("longitude").and_then(|v| v.as_f64()),
                        ) {
                            lat = la;
                            lon = lo;
                        }
                    }
                }
            }
        }
    }

    let met_url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={:.4}&longitude={:.4}&current=temperature_2m,weather_code",
        lat, lon
    );
    if let Ok(res) = client.get(&met_url).send() {
        if res.status().is_success() {
            if let Ok(json) = res.json::<serde_json::Value>() {
                if let Some(curr) = json.get("current") {
                    if let Some(temp_c) = curr.get("temperature_2m").and_then(|t| t.as_f64()) {
                        let wmo_code = curr
                            .get("weather_code")
                            .and_then(|c| c.as_i64())
                            .unwrap_or(0) as i32;
                        let (owm_code, desc) = match wmo_code {
                            0 => (800, "Clear"),
                            1..=3 => (802, "Clouds"),
                            45..=48 => (741, "Fog"),
                            51..=67 | 80..=82 => (500, "Rain"),
                            71..=77 | 85..=86 => (600, "Snow"),
                            95..=99 => (500, "Storm"),
                            _ => (800, "Clear"),
                        };
                        return Some((temp_c as f32, owm_code, desc.to_string()));
                    }
                }
            }
        }
    }

    // 3. Tertiary fallback: wttr.in with WWO-to-OWM weatherCode normalization
    let encoded_city = trimmed_city.replace(' ', "+");
    let url = format!("https://wttr.in/{}?format=j1", encoded_city);

    if let Ok(res) = client.get(&url).send() {
        if res.status().is_success() {
            if let Ok(json) = res.json::<serde_json::Value>() {
                if let Some(curr) = json.get("current_condition").and_then(|c| c.get(0)) {
                    let temp_c = curr
                        .get("temp_C")
                        .and_then(|t| t.as_str())
                        .and_then(|s| s.parse::<f32>().ok())?;
                    let wwo_code = curr
                        .get("weatherCode")
                        .and_then(|c| c.as_str())
                        .and_then(|s| s.parse::<i32>().ok())
                        .unwrap_or(113);
                    let code = match wwo_code {
                        113 => 800,
                        176 | 263 | 266 | 293 | 296 | 299 | 302 | 305 | 308 | 353 | 356 | 359
                        | 386 | 389 => 500,
                        _ => 802,
                    };
                    let desc = curr
                        .get("weatherDesc")
                        .and_then(|d| d.get(0))
                        .and_then(|d| d.get("value"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("Clear")
                        .to_string();
                    return Some((temp_c, code, desc));
                }
            }
        }
    }
    None
}

pub fn read_system_metrics() -> (f32, f32) {
    let cpu = 15.0;
    let ram = 35.0;

    #[cfg(target_os = "linux")]
    {
        let mut real_cpu = cpu;
        let mut real_ram = ram;

        if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
            let mut total = 0.0;
            let mut avail = 0.0;
            for line in content.lines() {
                if line.starts_with("MemTotal:") {
                    if let Some(val) = line.split_whitespace().nth(1) {
                        total = val.parse::<f32>().unwrap_or(1.0);
                    }
                } else if line.starts_with("MemAvailable:") {
                    if let Some(val) = line.split_whitespace().nth(1) {
                        avail = val.parse::<f32>().unwrap_or(0.0);
                    }
                }
            }
            if total > 0.0 {
                real_ram = ((total - avail) / total * 100.0).clamp(0.0, 100.0);
            }
        }

        if let Ok(content) = std::fs::read_to_string("/proc/loadavg") {
            if let Some(first) = content.split_whitespace().next() {
                let load = first.parse::<f32>().unwrap_or(0.5);
                real_cpu = (load * 25.0).clamp(1.0, 99.0);
            }
        }
        return (real_cpu, real_ram);
    }

    #[allow(unreachable_code)]
    (cpu, ram)
}

pub fn read_wifi_rssi() -> i32 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/proc/net/wireless") {
            for line in content.lines().skip(2) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    if let Ok(val) = parts[3].trim_end_matches('.').parse::<i32>() {
                        if val < 0 {
                            return val;
                        } else {
                            return val - 100;
                        }
                    }
                }
            }
        }
    }
    -55
}

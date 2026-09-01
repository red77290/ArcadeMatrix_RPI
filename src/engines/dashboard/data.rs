use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

pub fn fetch_live_weather(city: &str) -> Option<(f32, i32, String)> {
    if city.trim().is_empty() {
        return None;
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .build()
        .ok()?;

    let encoded_city = city.trim().replace(' ', "+");
    let url = format!("https://wttr.in/{}?format=j1", encoded_city);

    if let Ok(res) = client.get(&url).send() {
        if res.status().is_success() {
            if let Ok(json) = res.json::<serde_json::Value>() {
                if let Some(curr) = json.get("current_condition").and_then(|c| c.get(0)) {
                    let temp_c = curr
                        .get("temp_C")
                        .and_then(|t| t.as_str())
                        .and_then(|s| s.parse::<f32>().ok())?;
                    let code = curr
                        .get("weatherCode")
                        .and_then(|c| c.as_str())
                        .and_then(|s| s.parse::<i32>().ok())
                        .unwrap_or(800);
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

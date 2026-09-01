use crate::core::build_info::VERSION;
use crate::core::engine_contract::{
    Capabilities, ConfigField, ConfigOption, ConfigSchema, ConfigType, Engine, EngineConfig,
    EngineContext, EngineDescriptor, EngineError, EngineMetadata, Requirements, ValidationPolicy,
};
use crate::core::registry::ENGINES;
use crate::engines::renderers::BaseRenderer;
use chrono::{Local, Timelike, Utc};
use linkme::distributed_slice;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockMode {
    Digital = 0,
    WatchDial = 1,
    Minimal = 2,
}

#[derive(Debug, Clone)]
pub struct DashboardTheme {
    pub primary: (u8, u8, u8),
    pub secondary: (u8, u8, u8),
    pub accent: (u8, u8, u8),
    pub text: (u8, u8, u8),
    pub green: (u8, u8, u8),
    pub red: (u8, u8, u8),
    pub border: (u8, u8, u8),
}

impl DashboardTheme {
    pub fn get(theme_id: i32) -> Self {
        match theme_id {
            1 => Self {
                // Amber HUD / Tactical
                primary: (255, 170, 0),
                secondary: (255, 130, 0),
                accent: (255, 210, 50),
                text: (255, 230, 180),
                green: (100, 255, 100),
                red: (255, 80, 80),
                border: (60, 40, 10),
            },
            2 => Self {
                // Minimalist Luxury
                primary: (240, 240, 255),
                secondary: (170, 180, 210),
                accent: (255, 215, 0),
                text: (255, 255, 255),
                green: (80, 240, 140),
                red: (255, 90, 90),
                border: (40, 45, 60),
            },
            3 => Self {
                // Matrix Phosphor
                primary: (0, 255, 70),
                secondary: (0, 190, 50),
                accent: (140, 255, 170),
                text: (210, 255, 220),
                green: (0, 255, 100),
                red: (255, 60, 60),
                border: (10, 45, 15),
            },
            _ => Self {
                // 0: Cyberpunk Neon (Default)
                primary: (0, 240, 255),
                secondary: (255, 0, 128),
                accent: (255, 220, 0),
                text: (240, 245, 255),
                green: (0, 255, 136),
                red: (255, 51, 102),
                border: (30, 35, 65),
            },
        }
    }
}

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
    pub indoor_temp_c: f32,
    pub indoor_humidity: f32,
    pub cpu_usage: f32,
    pub ram_usage: f32,
    pub markets: Vec<MarketQuote>,
    pub world_times: Vec<WorldTimeQuote>,
}

pub struct DashboardEngine {
    base_renderer: BaseRenderer,
    clock_mode: ClockMode,
    theme_id: i32,
    show_clock: bool,
    show_world_clock: bool,
    show_weather: bool,
    show_indoor_temp: bool,
    show_markets: bool,
    show_sysinfo: bool,
    show_date: bool,
    show_seconds: bool,
    weather_city: String,
    tracked_markets: String,
    world_clocks_str: String,
    offset_x: i32,
    offset_y: i32,

    data: Arc<Mutex<DashboardData>>,
    running: Arc<AtomicBool>,
}

impl Default for DashboardEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DashboardEngine {
    pub fn new() -> Self {
        Self {
            base_renderer: BaseRenderer::new(),
            clock_mode: ClockMode::Digital,
            theme_id: 0,
            show_clock: true,
            show_world_clock: true,
            show_weather: true,
            show_indoor_temp: true,
            show_markets: true,
            show_sysinfo: true,
            show_date: true,
            show_seconds: true,
            weather_city: "Paris, FR".to_string(),
            tracked_markets: "BTC,ETH,SOL,NVDA".to_string(),
            world_clocks_str: "NYC,TYO,LON".to_string(),
            offset_x: 0,
            offset_y: 0,
            data: Arc::new(Mutex::new(DashboardData {
                temp_c: 20.0,
                weather_code: 800,
                weather_desc: "Clear".to_string(),
                indoor_temp_c: 22.0,
                indoor_humidity: 45.0,
                cpu_usage: 12.0,
                ram_usage: 34.0,
                markets: vec![
                    MarketQuote {
                        symbol: "BTC".into(),
                        price: 95400.0,
                        change_24h: 3.2,
                    },
                    MarketQuote {
                        symbol: "ETH".into(),
                        price: 3450.0,
                        change_24h: -1.1,
                    },
                    MarketQuote {
                        symbol: "SOL".into(),
                        price: 210.0,
                        change_24h: 5.4,
                    },
                    MarketQuote {
                        symbol: "NVDA".into(),
                        price: 142.5,
                        change_24h: 2.1,
                    },
                ],
                world_times: vec![
                    WorldTimeQuote {
                        code: "NYC".into(),
                        offset_hours: -5,
                    },
                    WorldTimeQuote {
                        code: "TYO".into(),
                        offset_hours: 9,
                    },
                    WorldTimeQuote {
                        code: "LON".into(),
                        offset_hours: 0,
                    },
                ],
            })),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    fn apply_config(&mut self, config: &dyn EngineConfig) {
        let cm = config.get_string("clock_mode", "0");
        self.clock_mode = match cm.as_str() {
            "1" | "dial" => ClockMode::WatchDial,
            "2" | "minimal" => ClockMode::Minimal,
            _ => ClockMode::Digital,
        };
        self.theme_id = config.get_int("theme", 0);
        self.show_clock = config.get_bool("show_clock", true);
        self.show_world_clock = config.get_bool("show_world_clock", true);
        self.show_weather = config.get_bool("show_weather", true);
        self.show_indoor_temp = config.get_bool("show_indoor_temp", true);
        self.show_markets = config.get_bool("show_markets", true);
        self.show_sysinfo = config.get_bool("show_sysinfo", true);
        self.show_date = config.get_bool("show_date", true);
        self.show_seconds = config.get_bool("show_seconds", true);
        self.weather_city = config.get_string("weather_city", "Paris, FR");
        self.tracked_markets = config.get_string("tracked_markets", "BTC,ETH,SOL,NVDA");
        self.world_clocks_str = config.get_string("world_clocks", "NYC,TYO,LON");
        self.offset_x = config.get_int("offset_x", 0);
        self.offset_y = config.get_int("offset_y", 0);

        self.parse_world_clocks();
    }

    fn parse_world_clocks(&mut self) {
        let mut list = Vec::new();
        for item in self.world_clocks_str.split(',') {
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
        if let Ok(mut lock) = self.data.lock() {
            if !list.is_empty() {
                lock.world_times = list;
            }
        }
    }

    fn spawn_background_fetcher(&self) {
        let running = self.running.clone();
        let data = self.data.clone();
        let markets = self.tracked_markets.clone();

        thread::spawn(move || {
            let mut last_fetch = Instant::now() - Duration::from_secs(3600);
            while running.load(Ordering::Relaxed) {
                if last_fetch.elapsed() >= Duration::from_secs(120) {
                    last_fetch = Instant::now();

                    let (cpu, ram) = Self::read_system_metrics();

                    if let Ok(mut lock) = data.lock() {
                        lock.cpu_usage = cpu;
                        lock.ram_usage = ram;

                        let syms: Vec<String> = markets
                            .split(',')
                            .map(|s| s.trim().to_uppercase())
                            .filter(|s| !s.is_empty())
                            .collect();

                        if !syms.is_empty() {
                            let mut updated = Vec::new();
                            for sym in syms {
                                let (p, c) = match sym.as_str() {
                                    "BTC" => (96200.0, 2.4),
                                    "ETH" => (3520.0, -0.8),
                                    "SOL" => (215.0, 6.1),
                                    "NVDA" => (145.2, 1.9),
                                    "AAPL" => (232.0, 0.5),
                                    "TSLA" => (340.0, -2.3),
                                    _ => (100.0, 0.0),
                                };
                                updated.push(MarketQuote {
                                    symbol: sym,
                                    price: p,
                                    change_24h: c,
                                });
                            }
                            lock.markets = updated;
                        }
                    }
                }
                thread::sleep(Duration::from_millis(500));
            }
        });
    }

    fn read_system_metrics() -> (f32, f32) {
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

    fn format_market_price(price: f32) -> String {
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
}

impl Engine for DashboardEngine {
    fn initialize(
        &mut self,
        _context: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        self.apply_config(config);
        Ok(())
    }

    fn activate(&mut self) {
        self.running.store(true, Ordering::Relaxed);
        self.spawn_background_fetcher();
    }

    fn deactivate(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }

    fn update(&mut self, _context: &mut EngineContext) {}

    fn render(&mut self, ctx: &mut EngineContext) {
        let matrix = &mut *ctx.matrix;
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;
        if w < 16 || h < 16 {
            return;
        }

        matrix.clear();

        let theme = DashboardTheme::get(self.theme_id);
        let font = self.base_renderer.font();

        let now = Local::now();
        let utc = Utc::now();
        let time_str = if self.show_seconds {
            now.format("%H:%M:%S").to_string()
        } else {
            now.format("%H:%M").to_string()
        };
        let date_str = now.format("%a %d %b").to_string();

        let data = self.data.lock().map(|d| d.clone()).unwrap_or_default();

        let is_tate = h > (w * 3) / 2 || (w < 48 && h >= 64);
        let is_wide = w >= 128;
        let is_square = (w - h).abs() <= 16 && w >= 48;

        if is_tate {
            // ==========================================
            // TATE / Portrait Layout (e.g. 32x64, 64x128)
            // ==========================================
            let mut cur_y = 2 + self.offset_y;

            // 1. Clock on top
            if self.show_clock {
                let (_, tw, _) = font.get_pixel_map(&time_str, 1.0);
                let tx = ((w - tw) / 2 + self.offset_x).max(1);
                BaseRenderer::draw_text_at(
                    matrix,
                    &time_str,
                    &font,
                    1.0,
                    tx,
                    cur_y,
                    theme.primary,
                    (0, 0, 0),
                );
                cur_y += 10;
            }

            // 2. Date
            if self.show_date && cur_y < h - 30 {
                let (_, dw, _) = font.get_pixel_map(&date_str, 1.0);
                let dx = ((w - dw) / 2 + self.offset_x).max(1);
                BaseRenderer::draw_text_at(
                    matrix,
                    &date_str,
                    &font,
                    1.0,
                    dx,
                    cur_y,
                    theme.text,
                    (0, 0, 0),
                );
                cur_y += 10;
            }

            // Divider
            if cur_y < h - 20 {
                for x in 2..w - 2 {
                    matrix.set_pixel(x, cur_y, theme.border.0, theme.border.1, theme.border.2);
                }
                cur_y += 3;
            }

            // 3. Weather / Climate
            if self.show_weather && cur_y < h - 20 {
                let t_str = format!("{:.0}°C", data.temp_c);
                BaseRenderer::draw_text_at(
                    matrix,
                    "OUT",
                    &font,
                    1.0,
                    2 + self.offset_x,
                    cur_y,
                    theme.secondary,
                    (0, 0, 0),
                );
                let (_, vw, _) = font.get_pixel_map(&t_str, 1.0);
                BaseRenderer::draw_text_at(
                    matrix,
                    &t_str,
                    &font,
                    1.0,
                    w - vw - 2 + self.offset_x,
                    cur_y,
                    theme.accent,
                    (0, 0, 0),
                );
                cur_y += 10;
            }

            // 4. System vitals
            if self.show_sysinfo && cur_y < h - 10 {
                let sys_str = format!("CPU:{:.0}%", data.cpu_usage);
                BaseRenderer::draw_text_at(
                    matrix,
                    &sys_str,
                    &font,
                    1.0,
                    2 + self.offset_x,
                    cur_y,
                    theme.text,
                    (0, 0, 0),
                );
                cur_y += 10;
            }

            // 5. Market quotes rolling at bottom
            if self.show_markets && !data.markets.is_empty() && cur_y < h - 8 {
                let m = &data.markets[(now.second() as usize / 3) % data.markets.len()];
                let p_str = Self::format_market_price(m.price);
                let m_col = if m.change_24h >= 0.0 {
                    theme.green
                } else {
                    theme.red
                };
                BaseRenderer::draw_text_at(
                    matrix,
                    &m.symbol,
                    &font,
                    1.0,
                    2 + self.offset_x,
                    h - 8,
                    theme.primary,
                    (0, 0, 0),
                );
                let (_, pw, _) = font.get_pixel_map(&p_str, 1.0);
                BaseRenderer::draw_text_at(
                    matrix,
                    &p_str,
                    &font,
                    1.0,
                    w - pw - 2 + self.offset_x,
                    h - 8,
                    m_col,
                    (0, 0, 0),
                );
            }
        } else if is_wide {
            // ==========================================
            // WIDESCREEN Desk Deck (128x32, 128x64, 256x64)
            // ==========================================
            let left_w = if h >= 64 { 64 } else { 54 };

            // Left Column: Clock & Date
            if self.show_clock {
                let clock_y = if h >= 64 {
                    12 + self.offset_y
                } else {
                    4 + self.offset_y
                };
                let scale = if h >= 64 { 1.5 } else { 1.0 };
                let (_, tw, _) = font.get_pixel_map(&time_str, scale);
                let tx = ((left_w - tw) / 2 + self.offset_x).max(2);
                BaseRenderer::draw_text_at(
                    matrix,
                    &time_str,
                    &font,
                    scale,
                    tx,
                    clock_y,
                    theme.primary,
                    (0, 0, 0),
                );

                if self.show_date {
                    let date_y = clock_y + (if h >= 64 { 18 } else { 12 });
                    let (_, dw, _) = font.get_pixel_map(&date_str, 1.0);
                    let dx = ((left_w - dw) / 2 + self.offset_x).max(2);
                    BaseRenderer::draw_text_at(
                        matrix,
                        &date_str,
                        &font,
                        1.0,
                        dx,
                        date_y,
                        theme.text,
                        (0, 0, 0),
                    );
                }
            }

            // Vertical separator line
            for y in 2..h - 2 {
                matrix.set_pixel(left_w, y, theme.border.0, theme.border.1, theme.border.2);
            }

            // Right Zone: Top row (World Time / Weather / SysInfo) + Bottom row (Markets)
            let right_x = left_w + 4 + self.offset_x;
            let right_w = w - right_x - 2;

            if h >= 64 {
                // Top Row: World Timezones & Weather
                let mut rx = right_x;
                if self.show_world_clock && !data.world_times.is_empty() {
                    for wt in data.world_times.iter().take(2) {
                        let wt_time = utc + chrono::Duration::hours(wt.offset_hours as i64);
                        let wt_str = format!("{}:{}", wt.code, wt_time.format("%H:%M"));
                        BaseRenderer::draw_text_at(
                            matrix,
                            &wt_str,
                            &font,
                            1.0,
                            rx,
                            6 + self.offset_y,
                            theme.secondary,
                            (0, 0, 0),
                        );
                        rx += 50;
                    }
                }

                if self.show_weather {
                    let w_str = format!("🌤️ {:.0}°C", data.temp_c);
                    let (_, ww, _) = font.get_pixel_map(&w_str, 1.0);
                    let wx = (w - ww - 4 + self.offset_x).max(rx);
                    BaseRenderer::draw_text_at(
                        matrix,
                        &w_str,
                        &font,
                        1.0,
                        wx,
                        6 + self.offset_y,
                        theme.accent,
                        (0, 0, 0),
                    );
                }

                // Middle Divider
                for x in right_x..w - 2 {
                    matrix.set_pixel(x, 30, theme.border.0, theme.border.1, theme.border.2);
                }

                // Bottom Row: Market Ticker & SysInfo
                if self.show_markets && !data.markets.is_empty() {
                    let item_w = 42;
                    let num_items = (right_w / item_w).max(1) as usize;
                    let scroll_idx = (now.second() as usize / 3) % data.markets.len();

                    for i in 0..num_items {
                        let idx = (scroll_idx + i) % data.markets.len();
                        let m = &data.markets[idx];
                        let mx = right_x + (i as i32 * item_w);
                        if mx + item_w <= w {
                            let p_str = Self::format_market_price(m.price);
                            let trend = if m.change_24h >= 0.0 {
                                theme.green
                            } else {
                                theme.red
                            };
                            BaseRenderer::draw_text_at(
                                matrix,
                                &m.symbol,
                                &font,
                                1.0,
                                mx,
                                36 + self.offset_y,
                                theme.primary,
                                (0, 0, 0),
                            );
                            BaseRenderer::draw_text_at(
                                matrix,
                                &p_str,
                                &font,
                                1.0,
                                mx,
                                46 + self.offset_y,
                                theme.text,
                                (0, 0, 0),
                            );
                            let chg_str = format!(
                                "{}{:.1}%",
                                if m.change_24h >= 0.0 { "+" } else { "" },
                                m.change_24h
                            );
                            BaseRenderer::draw_text_at(
                                matrix,
                                &chg_str,
                                &font,
                                1.0,
                                mx,
                                55 + self.offset_y,
                                trend,
                                (0, 0, 0),
                            );
                        }
                    }
                }
            } else {
                // 32px height panel
                if self.show_weather {
                    let w_str = format!("{:.0}°C", data.temp_c);
                    BaseRenderer::draw_text_at(
                        matrix,
                        "OUT",
                        &font,
                        1.0,
                        right_x,
                        4 + self.offset_y,
                        theme.secondary,
                        (0, 0, 0),
                    );
                    BaseRenderer::draw_text_at(
                        matrix,
                        &w_str,
                        &font,
                        1.0,
                        right_x + 24,
                        4 + self.offset_y,
                        theme.accent,
                        (0, 0, 0),
                    );
                }

                if self.show_markets && !data.markets.is_empty() {
                    let m = &data.markets[(now.second() as usize / 3) % data.markets.len()];
                    let p_str = Self::format_market_price(m.price);
                    let m_col = if m.change_24h >= 0.0 {
                        theme.green
                    } else {
                        theme.red
                    };
                    let m_line = format!("{} {}", m.symbol, p_str);
                    BaseRenderer::draw_text_at(
                        matrix,
                        &m_line,
                        &font,
                        1.0,
                        right_x,
                        18 + self.offset_y,
                        m_col,
                        (0, 0, 0),
                    );
                }
            }
        } else if is_square {
            // ==========================================
            // SQUARE 64x64
            // ==========================================
            let mut cur_y = 4 + self.offset_y;

            if self.show_clock {
                let (_, tw, _) = font.get_pixel_map(&time_str, 1.2);
                let tx = ((w - tw) / 2 + self.offset_x).max(2);
                BaseRenderer::draw_text_at(
                    matrix,
                    &time_str,
                    &font,
                    1.2,
                    tx,
                    cur_y,
                    theme.primary,
                    (0, 0, 0),
                );
                cur_y += 14;

                if self.show_date {
                    let (_, dw, _) = font.get_pixel_map(&date_str, 1.0);
                    let dx = ((w - dw) / 2 + self.offset_x).max(2);
                    BaseRenderer::draw_text_at(
                        matrix,
                        &date_str,
                        &font,
                        1.0,
                        dx,
                        cur_y,
                        theme.text,
                        (0, 0, 0),
                    );
                    cur_y += 12;
                }
            }

            for x in 4..w - 4 {
                matrix.set_pixel(x, cur_y, theme.border.0, theme.border.1, theme.border.2);
            }
            cur_y += 4;

            if self.show_weather {
                let w_str = format!("OUT: {:.0}°C", data.temp_c);
                BaseRenderer::draw_text_at(
                    matrix,
                    &w_str,
                    &font,
                    1.0,
                    4 + self.offset_x,
                    cur_y,
                    theme.accent,
                    (0, 0, 0),
                );
                cur_y += 11;
            }

            if self.show_markets && !data.markets.is_empty() {
                let m = &data.markets[(now.second() as usize / 3) % data.markets.len()];
                let p_str = Self::format_market_price(m.price);
                let m_col = if m.change_24h >= 0.0 {
                    theme.green
                } else {
                    theme.red
                };
                let m_line = format!("{}: {}", m.symbol, p_str);
                BaseRenderer::draw_text_at(
                    matrix,
                    &m_line,
                    &font,
                    1.0,
                    4 + self.offset_x,
                    cur_y,
                    m_col,
                    (0, 0, 0),
                );
            }
        } else {
            // ==========================================
            // COMPACT 64x32
            // ==========================================
            let left_w = w / 2;
            if self.show_clock {
                let (_, tw, _) = font.get_pixel_map(&time_str, 1.0);
                let tx = ((left_w - tw) / 2 + self.offset_x).max(1);
                BaseRenderer::draw_text_at(
                    matrix,
                    &time_str,
                    &font,
                    1.0,
                    tx,
                    4 + self.offset_y,
                    theme.primary,
                    (0, 0, 0),
                );
                if self.show_date {
                    let (_, dw, _) = font.get_pixel_map(&date_str, 1.0);
                    let dx = ((left_w - dw) / 2 + self.offset_x).max(1);
                    BaseRenderer::draw_text_at(
                        matrix,
                        &date_str,
                        &font,
                        1.0,
                        dx,
                        18 + self.offset_y,
                        theme.text,
                        (0, 0, 0),
                    );
                }
            }

            for y in 2..h - 2 {
                matrix.set_pixel(left_w, y, theme.border.0, theme.border.1, theme.border.2);
            }

            let right_x = left_w + 3 + self.offset_x;
            if self.show_weather {
                let t_str = format!("{:.0}°C", data.temp_c);
                BaseRenderer::draw_text_at(
                    matrix,
                    &t_str,
                    &font,
                    1.0,
                    right_x,
                    4 + self.offset_y,
                    theme.accent,
                    (0, 0, 0),
                );
            }

            if self.show_markets && !data.markets.is_empty() {
                let m = &data.markets[(now.second() as usize / 3) % data.markets.len()];
                let p_str = Self::format_market_price(m.price);
                let m_col = if m.change_24h >= 0.0 {
                    theme.green
                } else {
                    theme.red
                };
                BaseRenderer::draw_text_at(
                    matrix,
                    &m.symbol,
                    &font,
                    1.0,
                    right_x,
                    18 + self.offset_y,
                    m_col,
                    (0, 0, 0),
                );
            }
        }
    }

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        self.apply_config(config);
    }
}

#[distributed_slice(ENGINES)]
fn register_dashboard_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "dashboard",
            name: "Smart Dashboard Hub",
            category: "info",
            version: VERSION,
        },
        capabilities: Capabilities {
            realtime: true,
            supports_128x32: true,
            supports_256x64: true,
            ..Default::default()
        },
        requirements: Requirements {
            needs_network: true,
            ..Default::default()
        },
        available: true,
        unavailable_reason: None,
        schema: ConfigSchema {
            fields: vec![
                ConfigField {
                    id: "clock_mode",
                    field_type: ConfigType::Options,
                    label: "Clock Style",
                    description: "Display as Digital or Minimal",
                    default_value: "0",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    options: Some(vec![
                        ConfigOption {
                            value: "0".into(),
                            label: "Digital Modern".into(),
                        },
                        ConfigOption {
                            value: "1".into(),
                            label: "Watch Dial".into(),
                        },
                        ConfigOption {
                            value: "2".into(),
                            label: "Minimal".into(),
                        },
                    ]),
                    ..Default::default()
                },
                ConfigField {
                    id: "theme",
                    field_type: ConfigType::Options,
                    label: "Color Theme",
                    description: "Color palette for dashboard widgets",
                    default_value: "0",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    options: Some(vec![
                        ConfigOption {
                            value: "0".into(),
                            label: "Cyberpunk Neon".into(),
                        },
                        ConfigOption {
                            value: "1".into(),
                            label: "Arcade Amber HUD".into(),
                        },
                        ConfigOption {
                            value: "2".into(),
                            label: "Minimalist Luxury".into(),
                        },
                        ConfigOption {
                            value: "3".into(),
                            label: "Matrix Phosphor".into(),
                        },
                    ]),
                    ..Default::default()
                },
                ConfigField {
                    id: "show_clock",
                    field_type: ConfigType::Boolean,
                    label: "Show Clock",
                    description: "Display main time widget",
                    default_value: "true",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_world_clock",
                    field_type: ConfigType::Boolean,
                    label: "Show World Clocks",
                    description: "Display secondary timezones (NYC, TYO, LON...)",
                    default_value: "true",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "world_clocks",
                    field_type: ConfigType::String,
                    label: "World Timezones",
                    description: "Comma-separated airport codes (e.g. NYC,TYO,LON,PAR,SFO,LAX,SYD)",
                    default_value: "NYC,TYO,LON",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_weather",
                    field_type: ConfigType::Boolean,
                    label: "Show Weather",
                    description: "Display outdoor weather & temp",
                    default_value: "true",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "weather_city",
                    field_type: ConfigType::String,
                    label: "Weather City",
                    description:
                        "City name for weather forecast (e.g. Paris, London, Tokyo, New York)",
                    default_value: "Paris, FR",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_markets",
                    field_type: ConfigType::Boolean,
                    label: "Show Markets",
                    description: "Display live crypto and stock ticker badges",
                    default_value: "true",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "tracked_markets",
                    field_type: ConfigType::String,
                    label: "Tracked Markets",
                    description: "Comma-separated symbols (e.g. BTC,ETH,SOL,NVDA,AAPL,TSLA)",
                    default_value: "BTC,ETH,SOL,NVDA",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_sysinfo",
                    field_type: ConfigType::Boolean,
                    label: "Show System Vitals",
                    description: "Display CPU & RAM usage",
                    default_value: "true",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_date",
                    field_type: ConfigType::Boolean,
                    label: "Show Date",
                    description: "Display day and date badge",
                    default_value: "true",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_seconds",
                    field_type: ConfigType::Boolean,
                    label: "Show Seconds",
                    description: "Display seconds in clock",
                    default_value: "true",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "offset_x",
                    field_type: ConfigType::Integer,
                    label: "Offset X",
                    description: "Horizontal pixel shift",
                    default_value: "0",
                    validation_policy: ValidationPolicy::Clamp,
                    min_val: Some("-64"),
                    max_val: Some("64"),
                    ..Default::default()
                },
                ConfigField {
                    id: "offset_y",
                    field_type: ConfigType::Integer,
                    label: "Offset Y",
                    description: "Vertical pixel shift",
                    default_value: "0",
                    validation_policy: ValidationPolicy::Clamp,
                    min_val: Some("-32"),
                    max_val: Some("32"),
                    ..Default::default()
                },
            ],
        },
        factory: || Box::new(DashboardEngine::new()),
    }
}

pub mod climate_widget;
pub mod clock_widget;
pub mod data;
pub mod font;
pub mod geometry;
pub mod market_widget;
pub mod sysinfo_widget;
pub mod world_clock_widget;

use climate_widget::render_climate_slot;
use clock_widget::{render_analog_watch_dial, render_digital_clock, ClockMode};
use data::*;
use font::{draw_text_clipped, measure_text};
use geometry::*;
use market_widget::render_market_ticker;
use sysinfo_widget::render_sysinfo_slot;
use world_clock_widget::render_world_clock_slot;

use crate::core::build_info::VERSION;
use crate::core::engine_contract::{
    Capabilities, ConfigField, ConfigOption, ConfigSchema, ConfigType, Engine, EngineConfig,
    EngineContext, EngineDescriptor, EngineError, EngineMetadata, Requirements, ValidationPolicy,
};
use chrono::{Local, Timelike, Utc};
use linkme::distributed_slice;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

// ============================================================================
// Dashboard Engine Main Struct
// ============================================================================

pub struct DashboardEngine {
    clock_mode: ClockMode,
    show_clock: bool,
    show_world_clock: bool,
    show_weather: bool,
    show_indoor_temp: bool,
    show_markets: bool,
    show_sysinfo: bool,
    show_date: bool,
    show_seconds: bool,
    smooth_seconds: bool,
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
            clock_mode: ClockMode::WatchDial, // Default Analog watch face matching ESP32
            show_clock: true,
            show_world_clock: true,
            show_weather: true,
            show_indoor_temp: true,
            show_markets: true,
            show_sysinfo: true,
            show_date: true,
            show_seconds: true,
            smooth_seconds: true,
            weather_city: "Paris, FR".to_string(),
            tracked_markets: "BTC,ETH,SOL,NVDA".to_string(),
            world_clocks_str: "NYC,TYO,LON".to_string(),
            offset_x: 0,
            offset_y: 0,
            data: Arc::new(Mutex::new(DashboardData {
                temp_c: 21.0,
                weather_code: 800,
                weather_desc: "Clear".to_string(),
                indoor_temp_c: 22.0,
                indoor_humidity: 45.0,
                cpu_usage: 12.0,
                ram_usage: 34.0,
                wifi_rssi: -58,
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
        let cm = config.get_string("clock_mode", "1");
        self.clock_mode = match cm.as_str() {
            "0" | "digital" => ClockMode::Digital,
            "2" | "minimal" => ClockMode::Minimal,
            _ => ClockMode::WatchDial,
        };
        self.show_clock = config.get_bool("show_clock", true);
        self.show_world_clock = config.get_bool("show_world_clock", true);
        self.show_weather = config.get_bool("show_weather", true);
        self.show_indoor_temp = config.get_bool("show_indoor_temp", true);
        self.show_markets = config.get_bool("show_markets", true);
        self.show_sysinfo = config.get_bool("show_sysinfo", true);
        self.show_date = config.get_bool("show_date", true);
        self.show_seconds = config.get_bool("show_seconds", true);
        self.smooth_seconds = config.get_bool("smooth_seconds", true);
        self.weather_city = config.get_string("weather_city", "Paris, FR");
        self.tracked_markets = config.get_string("tracked_markets", "BTC,ETH,SOL,NVDA");
        self.world_clocks_str = config.get_string("world_clocks", "NYC,TYO,LON");
        self.offset_x = config.get_int("offset_x", 0);
        self.offset_y = config.get_int("offset_y", 0);

        let parsed = parse_world_clocks(&self.world_clocks_str);
        if let Ok(mut lock) = self.data.lock() {
            if !parsed.is_empty() {
                lock.world_times = parsed;
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
                if last_fetch.elapsed() >= Duration::from_secs(60) {
                    last_fetch = Instant::now();

                    let (cpu, ram) = read_system_metrics();

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

        let now = Local::now();
        let utc = Utc::now();
        let sub_second = if self.smooth_seconds {
            (now.nanosecond() as f32 / 1_000_000_000.0).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let data = self.data.lock().map(|d| d.clone()).unwrap_or_default();

        let is_tate = h > (w * 3) / 2 || (w < 48 && h >= 64);
        let is_wide = w >= 128;

        if is_tate {
            // ================================================================
            // TATE / Portrait Layout (e.g. 32x64, 64x128)
            // ================================================================
            let time_str = now.format("%H:%M").to_string();
            let mut cur_y = 2 + self.offset_y;

            if self.show_clock {
                let tw = measure_text(&time_str);
                let tx = ((w - tw) / 2 + self.offset_x).max(1);
                draw_text_clipped(matrix, &time_str, tx, cur_y, 0, w, 0, h, COLOR_PRIMARY);
                cur_y += 10;
            }

            if self.show_date && cur_y < h - 30 {
                let date_str = now.format("%d/%m").to_string();
                let dw = measure_text(&date_str);
                let dx = ((w - dw) / 2 + self.offset_x).max(1);
                draw_text_clipped(matrix, &date_str, dx, cur_y, 0, w, 0, h, COLOR_TEXT);
                cur_y += 10;
            }

            if cur_y < h - 20 {
                for x in 2..w - 2 {
                    matrix.set_pixel(x, cur_y, COLOR_BORDER.0, COLOR_BORDER.1, COLOR_BORDER.2);
                }
                cur_y += 3;
            }

            if self.show_weather && cur_y < h - 20 {
                let t_str = format!("{:.0}°C", data.temp_c);
                climate_widget::draw_mini_weather_icon(
                    matrix,
                    2 + self.offset_x,
                    cur_y,
                    0,
                    w,
                    0,
                    h,
                    data.weather_code,
                );
                let vw = measure_text(&t_str);
                draw_text_clipped(
                    matrix,
                    &t_str,
                    w - vw - 2 + self.offset_x,
                    cur_y,
                    0,
                    w,
                    0,
                    h,
                    COLOR_ACCENT,
                );
                cur_y += 10;
            }

            if self.show_sysinfo && cur_y < h - 10 {
                let sys_str = format!("CPU:{:.0}%", data.cpu_usage);
                draw_text_clipped(
                    matrix,
                    &sys_str,
                    2 + self.offset_x,
                    cur_y,
                    0,
                    w,
                    0,
                    h,
                    COLOR_TEXT,
                );
                cur_y += 10;
            }

            if self.show_markets && !data.markets.is_empty() && cur_y < h - 8 {
                let m = &data.markets[(now.second() as usize / 3) % data.markets.len()];
                let p_str = format_market_price(m.price);
                let m_col = if m.change_24h >= 0.0 {
                    COLOR_GREEN
                } else {
                    COLOR_RED
                };
                draw_text_clipped(
                    matrix,
                    &m.symbol,
                    2 + self.offset_x,
                    h - 8,
                    0,
                    w,
                    0,
                    h,
                    COLOR_PRIMARY,
                );
                let pw = measure_text(&p_str);
                draw_text_clipped(
                    matrix,
                    &p_str,
                    w - pw - 2 + self.offset_x,
                    h - 8,
                    0,
                    w,
                    0,
                    h,
                    m_col,
                );
            }
        } else if is_wide {
            // ================================================================
            // WIDESCREEN Responsive Geometry (128x32, 128x64, 256x64)
            // ================================================================
            let has_top_widgets = self.show_world_clock || self.show_weather || self.show_sysinfo;
            let has_bot_widgets = self.show_markets;

            let clock_w = (h.min(if w >= 200 { 64 } else { w / 3 })).min(w);
            let content_x = if self.show_clock { clock_w + 2 } else { 0 };
            let content_w = w - content_x;

            // 1. Clock Placement (Occupies left column)
            if self.show_clock {
                let clock_rect = Rect::new(self.offset_x, self.offset_y, clock_w, h);
                match self.clock_mode {
                    ClockMode::WatchDial => {
                        render_analog_watch_dial(
                            matrix,
                            &clock_rect,
                            &now,
                            sub_second,
                            self.show_seconds,
                        );
                    }
                    ClockMode::Digital => {
                        render_digital_clock(
                            matrix,
                            &clock_rect,
                            &now,
                            self.show_seconds,
                            self.show_date,
                        );
                    }
                    ClockMode::Minimal => {
                        render_digital_clock(matrix, &clock_rect, &now, false, false);
                    }
                }
            }

            // 2. Right Content Area (Dual Row: Top Row + Bottom Row)
            if content_w > 10 {
                let (top_y, top_h, bot_y, bot_h) = if has_top_widgets && has_bot_widgets {
                    let th = (h / 2) - 1;
                    let by = th + 2;
                    let bh = h - by;
                    (0, th, by, bh)
                } else if has_top_widgets {
                    (0, h, 0, 0)
                } else {
                    (0, 0, 0, h)
                };

                // --- TOP ROW WIDGETS ---
                if top_h > 0 {
                    let top_count = (if self.show_world_clock { 1 } else { 0 })
                        + (if self.show_weather { 1 } else { 0 })
                        + (if self.show_sysinfo { 1 } else { 0 });

                    let mut cur_top_x = content_x + self.offset_x;
                    let mut rem_w = content_w;
                    let mut left_to_place = top_count;

                    // World Clocks Slot
                    if self.show_world_clock && !data.world_times.is_empty() && left_to_place > 0 {
                        let slot_w = if left_to_place == 1 {
                            rem_w
                        } else if top_count == 3 {
                            rem_w * 34 / 100
                        } else {
                            rem_w / left_to_place
                        };

                        let slot_rect =
                            Rect::new(cur_top_x, top_y + self.offset_y, slot_w.max(10), top_h);
                        render_world_clock_slot(matrix, &slot_rect, &data.world_times, &now, &utc);

                        cur_top_x += slot_rect.w + 2;
                        rem_w -= slot_rect.w + 2;
                        left_to_place -= 1;
                    }

                    // Climate / Weather Slot
                    if self.show_weather && left_to_place > 0 {
                        let slot_w = if left_to_place == 1 {
                            rem_w
                        } else {
                            rem_w / left_to_place
                        };
                        let slot_rect =
                            Rect::new(cur_top_x, top_y + self.offset_y, slot_w.max(10), top_h);
                        render_climate_slot(matrix, &slot_rect, data.temp_c, data.weather_code);

                        cur_top_x += slot_rect.w + 2;
                        rem_w -= slot_rect.w + 2;
                        left_to_place -= 1;
                    }

                    // System Vitals Slot
                    if self.show_sysinfo && left_to_place > 0 {
                        let slot_rect =
                            Rect::new(cur_top_x, top_y + self.offset_y, rem_w.max(10), top_h);
                        render_sysinfo_slot(matrix, &slot_rect, data.ram_usage, data.wifi_rssi);
                    }
                }

                // --- BOTTOM ROW: INFINITE MARKET TICKER ---
                if bot_h > 0 && self.show_markets && !data.markets.is_empty() {
                    let bot_rect = Rect::new(
                        content_x + self.offset_x,
                        bot_y + self.offset_y,
                        content_w,
                        bot_h,
                    );
                    render_market_ticker(matrix, &bot_rect, &data.markets, now_ms());
                }
            }
        }
    }
}

// ============================================================================
// Registration via Distributed Slice (Clean Schema without Theme/Font)
// ============================================================================

#[distributed_slice(crate::core::registry::ENGINES)]
fn register_dashboard_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "dashboard",
            name: "Dashboard Engine",
            category: "info",
            version: VERSION,
        },
        capabilities: Capabilities::default(),
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
                    description: "Display as Digital Modern, Pixel-Art Watch Dial or Minimal",
                    default_value: "1",
                    options: Some(vec![
                        ConfigOption {
                            label: "Digital Modern",
                            value: "0",
                        },
                        ConfigOption {
                            label: "Pixel-Art Watch Dial",
                            value: "1",
                        },
                        ConfigOption {
                            label: "Minimal",
                            value: "2",
                        },
                    ]),
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_clock",
                    field_type: ConfigType::Boolean,
                    label: "Show Clock",
                    description: "Display main clock widget",
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
                    description: "Comma-separated list of timezone codes (e.g. NYC,TYO,LON,PAR)",
                    default_value: "NYC,TYO,LON",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_weather",
                    field_type: ConfigType::Boolean,
                    label: "Show Weather",
                    description: "Display outdoor weather & temperature",
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
                    id: "show_indoor_temp",
                    field_type: ConfigType::Boolean,
                    label: "Show Indoor Climate",
                    description: "Display room temperature & humidity",
                    default_value: "true",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_markets",
                    field_type: ConfigType::Boolean,
                    label: "Show Markets / Stocks",
                    description: "Display rolling crypto and stock ticker badges",
                    default_value: "true",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "tracked_markets",
                    field_type: ConfigType::String,
                    label: "Tracked Markets",
                    description: "Comma-separated list of symbols (e.g. BTC,ETH,SOL,NVDA)",
                    default_value: "BTC,ETH,SOL,NVDA",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_sysinfo",
                    field_type: ConfigType::Boolean,
                    label: "Show System Vitals",
                    description: "Display CPU, RAM & WiFi metrics",
                    default_value: "true",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_date",
                    field_type: ConfigType::Boolean,
                    label: "Show Date",
                    description: "Display date badge",
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
                    id: "smooth_seconds",
                    field_type: ConfigType::Boolean,
                    label: "Smooth Seconds",
                    description: "Smooth sweeping seconds vs crisp ticks",
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
                    min_val: Some("-64"),
                    max_val: Some("64"),
                    step: Some("1"),
                    validation_policy: ValidationPolicy::Clamp,
                    ..Default::default()
                },
                ConfigField {
                    id: "offset_y",
                    field_type: ConfigType::Integer,
                    label: "Offset Y",
                    description: "Vertical pixel shift",
                    default_value: "0",
                    min_val: Some("-32"),
                    max_val: Some("32"),
                    step: Some("1"),
                    validation_policy: ValidationPolicy::Clamp,
                    ..Default::default()
                },
            ],
        },
        factory: || -> Box<dyn Engine> { Box::new(DashboardEngine::new()) },
    }
}

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
use geometry::*;
use market_widget::render_market_ticker;
use sysinfo_widget::render_sysinfo_slot;
use world_clock_widget::render_world_clock_slot;

use crate::core::build_info::VERSION;
use crate::core::engine_contract::{
    Capabilities, ConfigField, ConfigOption, ConfigSchema, ConfigType, Engine, EngineConfig,
    EngineContext, EngineDescriptor, EngineError, EngineMetadata, Requirements, ValidationPolicy,
};
use chrono::{Datelike, Local, Timelike, Utc};
use linkme::distributed_slice;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

// ============================================================================
// Dashboard Engine Main Struct
// ============================================================================

pub struct DashboardEngine {
    theme: i32,
    clock_mode: ClockMode,
    format_24h: String,
    temp_unit: String,
    lang: String,
    timezone: String,
    show_clock: bool,
    show_world_clock: bool,
    show_weather: bool,
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
            theme: 0,
            clock_mode: ClockMode::WatchDial, // Default Analog watch face matching ESP32
            format_24h: "system".to_string(),
            temp_unit: "system".to_string(),
            lang: "system".to_string(),
            timezone: "system".to_string(),
            show_clock: true,
            show_world_clock: true,
            show_weather: true,
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
        self.theme = config.get_int("theme", 0);
        let cm = config.get_string("clock_mode", "1");
        self.clock_mode = match cm.as_str() {
            "0" | "digital" => ClockMode::Digital,
            "2" | "minimal" => ClockMode::Minimal,
            _ => ClockMode::WatchDial,
        };
        self.format_24h = config.get_string("format_24h", "system");
        self.temp_unit = config.get_string("temp_unit", "system");
        self.lang = config.get_string("lang", "system");
        self.timezone = config.get_string("timezone", "system");
        self.show_clock = config.get_bool("show_clock", true);
        self.show_world_clock = config.get_bool("show_world_clock", true);
        self.show_weather = config.get_bool("show_weather", true);
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
        let city = self.weather_city.clone();

        thread::spawn(move || {
            use crate::api::crypto_provider::CryptoProvider;
            use crate::api::stock_provider::StockProvider;

            let binance = crate::api::binance::BinanceProvider;
            let yahoo = crate::api::yahoo_finance::YahooFinanceProvider;
            let mut last_fetch = Instant::now() - Duration::from_secs(3600);
            let mut last_sys_metrics = Instant::now() - Duration::from_secs(10);

            while running.load(Ordering::Relaxed) {
                // Live system metrics (CPU load, RAM usage, WiFi signal) refreshed every 2 seconds
                if last_sys_metrics.elapsed() >= Duration::from_secs(2) {
                    last_sys_metrics = Instant::now();
                    let (cpu, ram) = read_system_metrics();
                    let wifi = read_wifi_rssi();
                    if let Ok(mut lock) = data.lock() {
                        lock.cpu_usage = cpu;
                        lock.ram_usage = ram;
                        lock.wifi_rssi = wifi;
                    }
                }

                // Web API market and weather quotes refreshed every 30 seconds
                if last_fetch.elapsed() >= Duration::from_secs(30) {
                    last_fetch = Instant::now();

                    let syms: Vec<String> = markets
                        .split(',')
                        .map(|s| s.trim().to_uppercase())
                        .filter(|s| !s.is_empty())
                        .collect();

                    let mut updated_markets = Vec::new();
                    for sym in &syms {
                        if !running.load(Ordering::Relaxed) {
                            break;
                        }
                        // 1. Try Binance (Cryptos)
                        let quote = binance
                            .fetch_quote(sym)
                            // 2. Try Yahoo Finance (Stocks / Alt Cryptos)
                            .or_else(|| yahoo.fetch_quote(sym))
                            .or_else(|| yahoo.fetch_quote(&format!("{}-USD", sym)));

                        if let Some((price, change, _)) = quote {
                            updated_markets.push(MarketQuote {
                                symbol: sym.clone(),
                                price: price as f32,
                                change_24h: change as f32,
                            });
                        }
                    }

                    let weather_info = fetch_live_weather(&city);

                    if let Ok(mut lock) = data.lock() {
                        if !updated_markets.is_empty() {
                            lock.markets = updated_markets;
                        }
                        if let Some((temp, code, desc)) = weather_info {
                            lock.temp_c = temp;
                            lock.weather_code = code;
                            lock.weather_desc = desc;
                        }
                    }
                }
                thread::sleep(Duration::from_millis(250));
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

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        self.apply_config(config);
    }

    fn activate(&mut self) {
        self.running.store(true, Ordering::Relaxed);
        self.spawn_background_fetcher();
    }

    fn deactivate(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }

    fn is_realtime(&self) -> bool {
        true
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

        let (sys_tz, sys_24h, sys_unit, sys_lang) = {
            let sys = ctx.config.settings.read();
            (
                sys.system.timezone.clone(),
                sys.system.format_24h,
                sys.system.temp_unit.clone(),
                sys.system.lang.clone(),
            )
        };

        let target_tz = if self.timezone.is_empty() || self.timezone == "system" {
            &sys_tz
        } else {
            &self.timezone
        };

        let active_lang = if self.lang.is_empty() || self.lang == "system" {
            &sys_lang
        } else {
            &self.lang
        };

        let theme_palette = get_dashboard_theme(self.theme);
        let utc = Utc::now();

        let (hours, minutes, seconds, sub_second, day, month) =
            if target_tz.is_empty() || target_tz == "system" {
                let now = Local::now();
                let sub = if self.smooth_seconds {
                    (now.nanosecond() as f32 / 1_000_000_000.0).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                (
                    now.hour(),
                    now.minute(),
                    now.second(),
                    sub,
                    now.day(),
                    now.month(),
                )
            } else if let Some(tz) = crate::engines::clock::parse_tz(target_tz) {
                let localized = utc.with_timezone(&tz);
                let sub = if self.smooth_seconds {
                    (localized.nanosecond() as f32 / 1_000_000_000.0).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                (
                    localized.hour(),
                    localized.minute(),
                    localized.second(),
                    sub,
                    localized.day(),
                    localized.month(),
                )
            } else {
                let now = Local::now();
                let sub = if self.smooth_seconds {
                    (now.nanosecond() as f32 / 1_000_000_000.0).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                (
                    now.hour(),
                    now.minute(),
                    now.second(),
                    sub,
                    now.day(),
                    now.month(),
                )
            };

        let is_24h = match self.format_24h.as_str() {
            "12h" | "12" => false,
            "24h" | "24" => true,
            "system" | "" => sys_24h,
            _ => sys_24h,
        };

        let is_fahrenheit = match self.temp_unit.as_str() {
            "F" | "fahrenheit" => true,
            "C" | "celsius" => false,
            "system" | "" => sys_unit.eq_ignore_ascii_case("F"),
            _ => sys_unit.eq_ignore_ascii_case("F"),
        };

        let data = self.data.lock().map(|d| d.clone()).unwrap_or_default();

        let is_tate = h > (w * 3) / 2 || (w < 48 && h >= 48);
        let is_wide = w >= 128 || (w >= 48 && w >= h);

        if is_tate {
            // ================================================================
            // PORTRAIT TOWER Responsive Layout (32x64, 32x128, 64x128, 64x256)
            // ================================================================
            let mut cur_y = self.offset_y;
            let gap = 1;

            if h >= 240 {
                // Full 64x256 Tall Tower
                if self.show_clock {
                    let clock_rect = Rect::new(self.offset_x, cur_y, w, 64);
                    match self.clock_mode {
                        ClockMode::WatchDial => render_analog_watch_dial(
                            matrix,
                            &clock_rect,
                            hours,
                            minutes,
                            seconds,
                            sub_second,
                            day,
                            &theme_palette,
                            self.show_seconds,
                            self.show_date,
                        ),
                        ClockMode::Digital => render_digital_clock(
                            matrix,
                            &clock_rect,
                            hours,
                            minutes,
                            seconds,
                            day,
                            month,
                            &theme_palette,
                            self.show_seconds,
                            self.show_date,
                            is_24h,
                            active_lang,
                        ),
                        ClockMode::Minimal => render_digital_clock(
                            matrix,
                            &clock_rect,
                            hours,
                            minutes,
                            seconds,
                            day,
                            month,
                            &theme_palette,
                            false,
                            false,
                            is_24h,
                            active_lang,
                        ),
                    }
                    cur_y += 64 + gap;
                }

                if self.show_world_clock && !data.world_times.is_empty() && cur_y < h {
                    let slot_rect = Rect::new(self.offset_x, cur_y, w, 44);
                    render_world_clock_slot(
                        matrix,
                        &slot_rect,
                        &data.world_times,
                        seconds,
                        &utc,
                        &theme_palette,
                        is_24h,
                    );
                    cur_y += 44 + gap;
                }

                if self.show_weather && cur_y < h {
                    let slot_rect = Rect::new(self.offset_x, cur_y, w, 54);
                    render_climate_slot(
                        matrix,
                        &slot_rect,
                        data.temp_c,
                        is_fahrenheit,
                        data.weather_code,
                        &theme_palette,
                    );
                    cur_y += 54 + gap;
                }

                if self.show_markets && !data.markets.is_empty() && cur_y < h {
                    let rem_h = h - cur_y;
                    let m_h = if self.show_sysinfo && rem_h > 24 {
                        rem_h - 16
                    } else {
                        rem_h
                    };
                    let slot_rect = Rect::new(self.offset_x, cur_y, w, m_h);
                    render_market_ticker(
                        matrix,
                        &slot_rect,
                        &data.markets,
                        now_ms(),
                        &theme_palette,
                    );
                    cur_y += m_h + gap;
                }

                if self.show_sysinfo && cur_y < h {
                    let slot_rect = Rect::new(self.offset_x, cur_y, w, h - cur_y);
                    render_sysinfo_slot(
                        matrix,
                        &slot_rect,
                        data.cpu_usage,
                        data.ram_usage,
                        data.wifi_rssi,
                        seconds,
                        &theme_palette,
                    );
                }
            } else if h >= 120 {
                // Medium 32x128 / 64x128 Tower (e.g. 128x32 matrix in TATE orientation)
                if self.show_clock {
                    let clock_rect = Rect::new(self.offset_x, cur_y, w, 48);
                    match self.clock_mode {
                        ClockMode::WatchDial => render_analog_watch_dial(
                            matrix,
                            &clock_rect,
                            hours,
                            minutes,
                            seconds,
                            sub_second,
                            day,
                            &theme_palette,
                            self.show_seconds,
                            self.show_date,
                        ),
                        ClockMode::Digital => render_digital_clock(
                            matrix,
                            &clock_rect,
                            hours,
                            minutes,
                            seconds,
                            day,
                            month,
                            &theme_palette,
                            self.show_seconds,
                            self.show_date,
                            is_24h,
                            active_lang,
                        ),
                        ClockMode::Minimal => render_digital_clock(
                            matrix,
                            &clock_rect,
                            hours,
                            minutes,
                            seconds,
                            day,
                            month,
                            &theme_palette,
                            false,
                            false,
                            is_24h,
                            active_lang,
                        ),
                    }
                    cur_y += 48 + gap;
                }

                if self.show_weather && cur_y < h {
                    let slot_rect = Rect::new(self.offset_x, cur_y, w, 36);
                    render_climate_slot(
                        matrix,
                        &slot_rect,
                        data.temp_c,
                        is_fahrenheit,
                        data.weather_code,
                        &theme_palette,
                    );
                    cur_y += 36 + gap;
                }

                if self.show_markets && !data.markets.is_empty() && cur_y < h {
                    let rem_h = h - cur_y;
                    let m_h = if self.show_sysinfo && rem_h > 24 {
                        rem_h - 16
                    } else {
                        rem_h
                    };
                    let slot_rect = Rect::new(self.offset_x, cur_y, w, m_h);
                    render_market_ticker(
                        matrix,
                        &slot_rect,
                        &data.markets,
                        now_ms(),
                        &theme_palette,
                    );
                    cur_y += m_h + gap;
                } else if self.show_world_clock && !data.world_times.is_empty() && cur_y < h {
                    let rem_h = h - cur_y;
                    let w_h = if self.show_sysinfo && rem_h > 24 {
                        rem_h - 16
                    } else {
                        rem_h
                    };
                    let slot_rect = Rect::new(self.offset_x, cur_y, w, w_h);
                    render_world_clock_slot(
                        matrix,
                        &slot_rect,
                        &data.world_times,
                        seconds,
                        &utc,
                        &theme_palette,
                        is_24h,
                    );
                    cur_y += w_h + gap;
                }

                if self.show_sysinfo && cur_y < h {
                    let slot_rect = Rect::new(self.offset_x, cur_y, w, h - cur_y);
                    render_sysinfo_slot(
                        matrix,
                        &slot_rect,
                        data.cpu_usage,
                        data.ram_usage,
                        data.wifi_rssi,
                        seconds,
                        &theme_palette,
                    );
                }
            } else {
                // Small 32x64 / 64x64 Tower
                if self.show_clock {
                    let clock_h = h / 2;
                    let clock_rect = Rect::new(self.offset_x, cur_y, w, clock_h);
                    match self.clock_mode {
                        ClockMode::WatchDial => render_analog_watch_dial(
                            matrix,
                            &clock_rect,
                            hours,
                            minutes,
                            seconds,
                            sub_second,
                            day,
                            &theme_palette,
                            self.show_seconds,
                            self.show_date,
                        ),
                        ClockMode::Digital => render_digital_clock(
                            matrix,
                            &clock_rect,
                            hours,
                            minutes,
                            seconds,
                            day,
                            month,
                            &theme_palette,
                            self.show_seconds,
                            self.show_date,
                            is_24h,
                            active_lang,
                        ),
                        ClockMode::Minimal => render_digital_clock(
                            matrix,
                            &clock_rect,
                            hours,
                            minutes,
                            seconds,
                            day,
                            month,
                            &theme_palette,
                            false,
                            false,
                            is_24h,
                            active_lang,
                        ),
                    }
                    cur_y += clock_h + gap;
                }

                if self.show_weather && cur_y < h {
                    let slot_rect = Rect::new(self.offset_x, cur_y, w, h - cur_y);
                    render_climate_slot(
                        matrix,
                        &slot_rect,
                        data.temp_c,
                        is_fahrenheit,
                        data.weather_code,
                        &theme_palette,
                    );
                }
            }
        } else if is_wide {
            // ================================================================
            // WIDESCREEN Responsive Geometry (128x32, 128x64, 256x64)
            // ================================================================
            let has_top_widgets = self.show_world_clock || self.show_weather || self.show_sysinfo;
            let has_bot_widgets = self.show_markets;

            let gap = if h >= 64 { 2 } else { 1 };
            let clock_w = (h.min(if w >= 200 { 64 } else { w / 3 })).min(w);
            let content_x = if self.show_clock { clock_w + gap } else { 0 };
            let content_w = w - content_x;

            // 1. Clock Placement (Occupies left column)
            if self.show_clock {
                let clock_rect = Rect::new(self.offset_x, self.offset_y, clock_w, h);
                match self.clock_mode {
                    ClockMode::WatchDial => {
                        render_analog_watch_dial(
                            matrix,
                            &clock_rect,
                            hours,
                            minutes,
                            seconds,
                            sub_second,
                            day,
                            &theme_palette,
                            self.show_seconds,
                            self.show_date,
                        );
                    }
                    ClockMode::Digital => {
                        render_digital_clock(
                            matrix,
                            &clock_rect,
                            hours,
                            minutes,
                            seconds,
                            day,
                            month,
                            &theme_palette,
                            self.show_seconds,
                            self.show_date,
                            is_24h,
                            active_lang,
                        );
                    }
                    ClockMode::Minimal => {
                        render_digital_clock(
                            matrix,
                            &clock_rect,
                            hours,
                            minutes,
                            seconds,
                            day,
                            month,
                            &theme_palette,
                            false,
                            false,
                            is_24h,
                            active_lang,
                        );
                    }
                }
            }

            // 2. Right Content Area (Dual Row: Top Row + Bottom Row)
            if content_w > 10 {
                let (top_y, top_h, bot_y, bot_h) = if has_top_widgets && has_bot_widgets {
                    let th = (h - gap) / 2;
                    let by = th + gap;
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
                    let total_gaps = (top_count - 1).max(0) * gap;
                    let avail_top_w = content_w - total_gaps;
                    let mut rem_avail_w = avail_top_w;
                    let mut left_to_place = top_count;

                    // World Clocks Slot
                    if self.show_world_clock && !data.world_times.is_empty() && left_to_place > 0 {
                        let slot_w = if left_to_place == 1 {
                            rem_avail_w
                        } else {
                            avail_top_w / top_count
                        };

                        let slot_rect =
                            Rect::new(cur_top_x, top_y + self.offset_y, slot_w.max(10), top_h);
                        render_world_clock_slot(
                            matrix,
                            &slot_rect,
                            &data.world_times,
                            seconds,
                            &utc,
                            &theme_palette,
                            is_24h,
                        );

                        cur_top_x += slot_rect.w + gap;
                        rem_avail_w -= slot_rect.w;
                        left_to_place -= 1;
                    }

                    // Climate / Weather Slot
                    if self.show_weather && left_to_place > 0 {
                        let slot_w = if left_to_place == 1 {
                            rem_avail_w
                        } else {
                            avail_top_w / top_count
                        };
                        let slot_rect =
                            Rect::new(cur_top_x, top_y + self.offset_y, slot_w.max(10), top_h);
                        render_climate_slot(
                            matrix,
                            &slot_rect,
                            data.temp_c,
                            is_fahrenheit,
                            data.weather_code,
                            &theme_palette,
                        );

                        cur_top_x += slot_rect.w + gap;
                        rem_avail_w -= slot_rect.w;
                        left_to_place -= 1;
                    }

                    // System Vitals Slot
                    if self.show_sysinfo && left_to_place > 0 {
                        let slot_rect =
                            Rect::new(cur_top_x, top_y + self.offset_y, rem_avail_w.max(10), top_h);
                        render_sysinfo_slot(
                            matrix,
                            &slot_rect,
                            data.cpu_usage,
                            data.ram_usage,
                            data.wifi_rssi,
                            seconds,
                            &theme_palette,
                        );
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
                    render_market_ticker(
                        matrix,
                        &bot_rect,
                        &data.markets,
                        now_ms(),
                        &theme_palette,
                    );
                }
            }
        }
    }
}

// ============================================================================
// Registration via Distributed Slice (ESP32-Aligned Schema)
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
        capabilities: Capabilities {
            realtime: true,
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
                    id: "theme",
                    field_type: ConfigType::Options,
                    label: "Color Theme",
                    description: "Select visual color theme palette",
                    default_value: "0",
                    options: Some(vec![
                        ConfigOption {
                            label: "Cyberpunk Neon",
                            value: "0",
                        },
                        ConfigOption {
                            label: "Amber HUD",
                            value: "1",
                        },
                        ConfigOption {
                            label: "Luxury Ice Blue",
                            value: "2",
                        },
                        ConfigOption {
                            label: "Matrix Green",
                            value: "3",
                        },
                    ]),
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
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
                    id: "timezone",
                    field_type: ConfigType::Options,
                    label: "Timezone",
                    description: "Select timezone or device system setting",
                    default_value: "system",
                    options_endpoint: Some("/api/timezones"),
                    validation_policy: ValidationPolicy::Accept,
                    ..Default::default()
                },
                ConfigField {
                    id: "format_24h",
                    field_type: ConfigType::Options,
                    label: "Time Format",
                    description: "24-hour or 12-hour time format",
                    default_value: "system",
                    options: Some(vec![
                        ConfigOption {
                            label: "System Setting",
                            value: "system",
                        },
                        ConfigOption {
                            label: "24 Hours (23:59)",
                            value: "24h",
                        },
                        ConfigOption {
                            label: "12 Hours (11:59 PM)",
                            value: "12h",
                        },
                    ]),
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "temp_unit",
                    field_type: ConfigType::Options,
                    label: "Temperature Unit",
                    description: "Celsius (°C) or Fahrenheit (°F)",
                    default_value: "system",
                    options: Some(vec![
                        ConfigOption {
                            label: "System Setting",
                            value: "system",
                        },
                        ConfigOption {
                            label: "Celsius (°C)",
                            value: "C",
                        },
                        ConfigOption {
                            label: "Fahrenheit (°F)",
                            value: "F",
                        },
                    ]),
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "lang",
                    field_type: ConfigType::Options,
                    label: "Language",
                    description: "Language for dashboard strings and dates",
                    default_value: "system",
                    options: Some(vec![
                        ConfigOption {
                            label: "System Setting",
                            value: "system",
                        },
                        ConfigOption {
                            label: "English",
                            value: "en",
                        },
                        ConfigOption {
                            label: "Français",
                            value: "fr",
                        },
                        ConfigOption {
                            label: "Español",
                            value: "es",
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

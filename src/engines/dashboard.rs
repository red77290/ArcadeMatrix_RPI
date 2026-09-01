use crate::core::build_info::VERSION;
use crate::core::engine_contract::{
    Capabilities, ConfigField, ConfigOption, ConfigSchema, ConfigType, Engine, EngineConfig,
    EngineContext, EngineDescriptor, EngineError, EngineMetadata, Requirements, ValidationPolicy,
};
use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::base_renderer::ArcadeFont;
use crate::engines::renderers::BaseRenderer;
use chrono::{DateTime, Local, Timelike, Utc};
use linkme::distributed_slice;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    pub text_dim: (u8, u8, u8),
    pub panel_bg: (u8, u8, u8),
    pub border: (u8, u8, u8),
    pub green: (u8, u8, u8),
    pub red: (u8, u8, u8),
}

impl DashboardTheme {
    pub fn get(theme_id: i32) -> Self {
        match theme_id {
            1 => Self {
                // 1: Arcade Amber HUD
                primary: (255, 170, 0),
                secondary: (255, 130, 0),
                accent: (255, 210, 50),
                text: (255, 230, 180),
                text_dim: (120, 80, 20),
                panel_bg: (15, 10, 0),
                border: (50, 35, 10),
                green: (100, 255, 50),
                red: (255, 50, 50),
            },
            2 => Self {
                // 2: Minimalist Luxury
                primary: (220, 220, 230),
                secondary: (180, 190, 200),
                accent: (212, 175, 55),
                text: (240, 240, 240),
                text_dim: (90, 95, 105),
                panel_bg: (10, 12, 16),
                border: (35, 40, 50),
                green: (40, 200, 120),
                red: (230, 60, 60),
            },
            3 => Self {
                // 3: Matrix Phosphor
                primary: (0, 255, 70),
                secondary: (0, 190, 50),
                accent: (140, 255, 170),
                text: (210, 255, 220),
                text_dim: (20, 80, 30),
                panel_bg: (0, 12, 4),
                border: (0, 45, 15),
                green: (0, 255, 65),
                red: (255, 60, 60),
            },
            _ => Self {
                // 0: Cyberpunk Neon (Default)
                primary: (0, 240, 255),
                secondary: (255, 0, 128),
                accent: (255, 220, 0),
                text: (240, 245, 255),
                text_dim: (100, 110, 140),
                panel_bg: (8, 10, 18),
                border: (25, 35, 60),
                green: (0, 255, 136),
                red: (255, 51, 102),
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
    pub wifi_rssi: i32,
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

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

// ============================================================================
// Bounded Drawing & Clipping Helpers (Zero Overlap)
// ============================================================================

fn draw_pixel_clipped(
    matrix: &mut dyn MatrixBackend,
    x: i32,
    y: i32,
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
    color: (u8, u8, u8),
) {
    if x >= min_x && x < max_x && y >= min_y && y < max_y {
        matrix.set_pixel(x, y, color.0, color.1, color.2);
    }
}

fn draw_line_clipped(
    matrix: &mut dyn MatrixBackend,
    mut x0: i32,
    mut y0: i32,
    x1: i32,
    y1: i32,
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
    color: (u8, u8, u8),
) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        draw_pixel_clipped(matrix, x0, y0, min_x, max_x, min_y, max_y, color);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn fill_rect_clipped(
    matrix: &mut dyn MatrixBackend,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
    color: (u8, u8, u8),
) {
    let x_start = x.max(min_x);
    let x_end = (x + w).min(max_x);
    let y_start = y.max(min_y);
    let y_end = (y + h).min(max_y);
    for py in y_start..y_end {
        for px in x_start..x_end {
            matrix.set_pixel(px, py, color.0, color.1, color.2);
        }
    }
}

fn draw_rect_clipped(
    matrix: &mut dyn MatrixBackend,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
    color: (u8, u8, u8),
) {
    if w <= 0 || h <= 0 {
        return;
    }
    for px in x..x + w {
        draw_pixel_clipped(matrix, px, y, min_x, max_x, min_y, max_y, color);
        draw_pixel_clipped(matrix, px, y + h - 1, min_x, max_x, min_y, max_y, color);
    }
    for py in y..y + h {
        draw_pixel_clipped(matrix, x, py, min_x, max_x, min_y, max_y, color);
        draw_pixel_clipped(matrix, x + w - 1, py, min_x, max_x, min_y, max_y, color);
    }
}

fn draw_clipped_text(
    matrix: &mut dyn MatrixBackend,
    text: &str,
    font: &ArcadeFont<'_>,
    x: i32,
    y: i32,
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
    color: (u8, u8, u8),
) {
    let (pixels_by_char, _w, _h) = font.get_pixel_map(text, 1.0);
    for char_pixels in pixels_by_char {
        for (px, py) in char_pixels {
            draw_pixel_clipped(
                matrix,
                x + px,
                y + py,
                min_x,
                max_x,
                min_y,
                max_y,
                color,
            );
        }
    }
}

// ============================================================================
// Mini Icons (8x8 Tokens, Climate, WiFi)
// ============================================================================

fn draw_mini_weather_icon(
    matrix: &mut dyn MatrixBackend,
    x: i32,
    y: i32,
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
    weather_code: i32,
) {
    let sun_col = (255, 200, 0);
    let cloud_col = (180, 200, 220);
    let rain_col = (0, 160, 255);

    if weather_code == 800 || weather_code == 1 {
        // Sun
        for r in 0..4 {
            for c in 0..4 {
                draw_pixel_clipped(
                    matrix,
                    x + 2 + c,
                    y + 2 + r,
                    min_x,
                    max_x,
                    min_y,
                    max_y,
                    sun_col,
                );
            }
        }
        draw_pixel_clipped(matrix, x + 3, y, min_x, max_x, min_y, max_y, sun_col);
        draw_pixel_clipped(matrix, x + 3, y + 7, min_x, max_x, min_y, max_y, sun_col);
        draw_pixel_clipped(matrix, x, y + 3, min_x, max_x, min_y, max_y, sun_col);
        draw_pixel_clipped(matrix, x + 7, y + 3, min_x, max_x, min_y, max_y, sun_col);
    } else if weather_code >= 500 && weather_code < 600 {
        // Rain
        for r in 0..3 {
            for c in 0..6 {
                draw_pixel_clipped(
                    matrix,
                    x + 1 + c,
                    y + 1 + r,
                    min_x,
                    max_x,
                    min_y,
                    max_y,
                    cloud_col,
                );
            }
        }
        draw_pixel_clipped(matrix, x + 2, y + 5, min_x, max_x, min_y, max_y, rain_col);
        draw_pixel_clipped(matrix, x + 4, y + 6, min_x, max_x, min_y, max_y, rain_col);
        draw_pixel_clipped(matrix, x + 6, y + 5, min_x, max_x, min_y, max_y, rain_col);
    } else {
        // Cloud
        for r in 0..4 {
            for c in 0..6 {
                draw_pixel_clipped(
                    matrix,
                    x + 1 + c,
                    y + 2 + r,
                    min_x,
                    max_x,
                    min_y,
                    max_y,
                    cloud_col,
                );
            }
        }
        draw_pixel_clipped(matrix, x + 3, y + 1, min_x, max_x, min_y, max_y, cloud_col);
        draw_pixel_clipped(matrix, x + 4, y + 1, min_x, max_x, min_y, max_y, cloud_col);
    }
}

fn draw_mini_market_icon(
    matrix: &mut dyn MatrixBackend,
    x: i32,
    y: i32,
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
    symbol: &str,
) {
    let gold = (255, 180, 0);
    let blue = (0, 140, 255);
    let purple = (160, 40, 255);
    let white = (240, 240, 255);
    let green = (0, 220, 100);

    match symbol {
        "BTC" => {
            for py in 0..8 {
                for px in 0..8 {
                    if (px == 0 || px == 7) && (py == 0 || py == 7) {
                        continue;
                    }
                    if px == 0 || px == 7 || py == 0 || py == 7 {
                        draw_pixel_clipped(
                            matrix,
                            x + px,
                            y + py,
                            min_x,
                            max_x,
                            min_y,
                            max_y,
                            gold,
                        );
                    }
                }
            }
            draw_pixel_clipped(matrix, x + 2, y + 2, min_x, max_x, min_y, max_y, white);
            draw_pixel_clipped(matrix, x + 3, y + 2, min_x, max_x, min_y, max_y, white);
            draw_pixel_clipped(matrix, x + 4, y + 2, min_x, max_x, min_y, max_y, white);
            draw_pixel_clipped(matrix, x + 2, y + 3, min_x, max_x, min_y, max_y, white);
            draw_pixel_clipped(matrix, x + 5, y + 3, min_x, max_x, min_y, max_y, white);
            draw_pixel_clipped(matrix, x + 2, y + 4, min_x, max_x, min_y, max_y, white);
            draw_pixel_clipped(matrix, x + 3, y + 4, min_x, max_x, min_y, max_y, white);
            draw_pixel_clipped(matrix, x + 4, y + 4, min_x, max_x, min_y, max_y, white);
            draw_pixel_clipped(matrix, x + 2, y + 5, min_x, max_x, min_y, max_y, white);
            draw_pixel_clipped(matrix, x + 5, y + 5, min_x, max_x, min_y, max_y, white);
            draw_pixel_clipped(matrix, x + 2, y + 6, min_x, max_x, min_y, max_y, white);
            draw_pixel_clipped(matrix, x + 3, y + 6, min_x, max_x, min_y, max_y, white);
            draw_pixel_clipped(matrix, x + 4, y + 6, min_x, max_x, min_y, max_y, white);
        }
        "ETH" => {
            draw_pixel_clipped(matrix, x + 3, y + 0, min_x, max_x, min_y, max_y, blue);
            draw_pixel_clipped(matrix, x + 4, y + 0, min_x, max_x, min_y, max_y, blue);
            draw_pixel_clipped(matrix, x + 2, y + 1, min_x, max_x, min_y, max_y, blue);
            draw_pixel_clipped(matrix, x + 5, y + 1, min_x, max_x, min_y, max_y, blue);
            draw_pixel_clipped(matrix, x + 1, y + 2, min_x, max_x, min_y, max_y, blue);
            draw_pixel_clipped(matrix, x + 6, y + 2, min_x, max_x, min_y, max_y, blue);
            draw_pixel_clipped(matrix, x + 0, y + 3, min_x, max_x, min_y, max_y, white);
            draw_pixel_clipped(matrix, x + 7, y + 3, min_x, max_x, min_y, max_y, white);
            draw_pixel_clipped(matrix, x + 1, y + 4, min_x, max_x, min_y, max_y, blue);
            draw_pixel_clipped(matrix, x + 6, y + 4, min_x, max_x, min_y, max_y, blue);
            draw_pixel_clipped(matrix, x + 2, y + 5, min_x, max_x, min_y, max_y, blue);
            draw_pixel_clipped(matrix, x + 5, y + 5, min_x, max_x, min_y, max_y, blue);
            draw_pixel_clipped(matrix, x + 3, y + 6, min_x, max_x, min_y, max_y, blue);
            draw_pixel_clipped(matrix, x + 4, y + 6, min_x, max_x, min_y, max_y, blue);
            draw_pixel_clipped(matrix, x + 3, y + 7, min_x, max_x, min_y, max_y, blue);
            draw_pixel_clipped(matrix, x + 4, y + 7, min_x, max_x, min_y, max_y, blue);
        }
        "SOL" => {
            for px in 1..7 {
                draw_pixel_clipped(matrix, x + px, y + 1, min_x, max_x, min_y, max_y, purple);
                draw_pixel_clipped(matrix, x + px, y + 3, min_x, max_x, min_y, max_y, blue);
                draw_pixel_clipped(matrix, x + px, y + 5, min_x, max_x, min_y, max_y, green);
            }
        }
        "NVDA" => {
            for py in 1..7 {
                for px in 1..7 {
                    if px == 1 || px == 6 || py == 1 || py == 6 {
                        draw_pixel_clipped(
                            matrix,
                            x + px,
                            y + py,
                            min_x,
                            max_x,
                            min_y,
                            max_y,
                            green,
                        );
                    }
                }
            }
            draw_pixel_clipped(matrix, x + 3, y + 3, min_x, max_x, min_y, max_y, white);
            draw_pixel_clipped(matrix, x + 4, y + 3, min_x, max_x, min_y, max_y, white);
            draw_pixel_clipped(matrix, x + 3, y + 4, min_x, max_x, min_y, max_y, white);
            draw_pixel_clipped(matrix, x + 4, y + 4, min_x, max_x, min_y, max_y, white);
        }
        _ => {
            for py in 0..8 {
                for px in 0..8 {
                    if (px == 0 || px == 7) && (py == 0 || py == 7) {
                        continue;
                    }
                    if px == 0 || px == 7 || py == 0 || py == 7 {
                        draw_pixel_clipped(
                            matrix,
                            x + px,
                            y + py,
                            min_x,
                            max_x,
                            min_y,
                            max_y,
                            gold,
                        );
                    }
                }
            }
            draw_pixel_clipped(matrix, x + 3, y + 3, min_x, max_x, min_y, max_y, white);
            draw_pixel_clipped(matrix, x + 4, y + 3, min_x, max_x, min_y, max_y, white);
        }
    }
}

// ============================================================================
// Clock Renderers (Analog Watch Dial & Digital Modern)
// ============================================================================

fn render_analog_clock(
    matrix: &mut dyn MatrixBackend,
    rect_x: i32,
    rect_y: i32,
    rect_w: i32,
    rect_h: i32,
    now: &DateTime<Local>,
    sub_second: f32,
    theme: &DashboardTheme,
    show_seconds: bool,
) {
    if rect_w < 14 || rect_h < 14 {
        return;
    }

    fill_rect_clipped(
        matrix,
        rect_x,
        rect_y,
        rect_w,
        rect_h,
        rect_x,
        rect_x + rect_w,
        rect_y,
        rect_y + rect_h,
        theme.panel_bg,
    );

    let cx = rect_x + rect_w / 2;
    let cy = rect_y + rect_h / 2;
    let mut radius = (rect_w.min(rect_h) / 2) - 1;
    if radius < 6 {
        radius = 6;
    }

    let min_x = rect_x;
    let max_x = rect_x + rect_w;
    let min_y = rect_y;
    let max_y = rect_y + rect_h;

    // 1. Draw 3D Octagonal Pixel Bezel
    let s = radius * 41 / 100;
    let r = radius;

    // Top & Left Bezel
    draw_line_clipped(
        matrix,
        cx - s,
        cy - r,
        cx + s,
        cy - r,
        min_x,
        max_x,
        min_y,
        max_y,
        theme.border,
    );
    draw_line_clipped(
        matrix,
        cx - r,
        cy - s,
        cx - s,
        cy - r,
        min_x,
        max_x,
        min_y,
        max_y,
        theme.accent,
    );
    draw_line_clipped(
        matrix,
        cx - r,
        cy - s,
        cx - r,
        cy + s,
        min_x,
        max_x,
        min_y,
        max_y,
        theme.accent,
    );
    draw_line_clipped(
        matrix,
        cx - r,
        cy + s,
        cx - s,
        cy + r,
        min_x,
        max_x,
        min_y,
        max_y,
        theme.border,
    );

    // Bottom & Right Bezel
    draw_line_clipped(
        matrix,
        cx - s,
        cy + r,
        cx + s,
        cy + r,
        min_x,
        max_x,
        min_y,
        max_y,
        theme.border,
    );
    draw_line_clipped(
        matrix,
        cx + s,
        cy + r,
        cx + r,
        cy + s,
        min_x,
        max_x,
        min_y,
        max_y,
        theme.panel_bg,
    );
    draw_line_clipped(
        matrix,
        cx + r,
        cy - s,
        cx + r,
        cy + s,
        min_x,
        max_x,
        min_y,
        max_y,
        theme.panel_bg,
    );
    draw_line_clipped(
        matrix,
        cx + s,
        cy - r,
        cx + r,
        cy - s,
        min_x,
        max_x,
        min_y,
        max_y,
        theme.border,
    );

    // 2. 12 Hour Pips
    for i in 0..12 {
        let angle =
            (i as f32 * 30.0) * (std::f32::consts::PI / 180.0) - (std::f32::consts::PI / 2.0);
        let cos_a = angle.cos();
        let sin_a = angle.sin();

        let is_cardinal = i % 3 == 0;
        let r_outer = (radius - 2) as f32;
        let r_inner = (if is_cardinal {
            (radius - 4).max(1)
        } else {
            (radius - 3).max(1)
        }) as f32;

        let x1 = cx + (r_outer * cos_a) as i32;
        let y1 = cy + (r_outer * sin_a) as i32;
        let x2 = cx + (r_inner * cos_a) as i32;
        let y2 = cy + (r_inner * sin_a) as i32;

        let pip_color = if is_cardinal {
            theme.primary
        } else {
            theme.text_dim
        };
        if radius >= 14 && is_cardinal {
            draw_line_clipped(
                matrix, x1, y1, x2, y2, min_x, max_x, min_y, max_y, pip_color,
            );
        } else {
            draw_pixel_clipped(matrix, x1, y1, min_x, max_x, min_y, max_y, pip_color);
        }
    }

    // 3. Hour Hand
    let h12 = (now.hour() % 12) as f32 + (now.minute() as f32 / 60.0);
    let hour_angle = (h12 / 12.0) * 2.0 * std::f32::consts::PI - (std::f32::consts::PI / 2.0);
    let hour_len = ((radius as f32) * 0.50).max(3.0);
    let hx = cx + (hour_len * hour_angle.cos()) as i32;
    let hy = cy + (hour_len * hour_angle.sin()) as i32;
    draw_line_clipped(
        matrix, cx, cy, hx, hy, min_x, max_x, min_y, max_y, theme.text,
    );
    draw_line_clipped(
        matrix,
        cx + 1,
        cy,
        hx + 1,
        hy,
        min_x,
        max_x,
        min_y,
        max_y,
        theme.text,
    );

    // 4. Minute Hand
    let min_val = now.minute() as f32 + (now.second() as f32 / 60.0);
    let min_angle = (min_val / 60.0) * 2.0 * std::f32::consts::PI - (std::f32::consts::PI / 2.0);
    let min_len = ((radius as f32) * 0.78).max(4.0);
    let mx = cx + (min_len * min_angle.cos()) as i32;
    let my = cy + (min_len * min_angle.sin()) as i32;
    draw_line_clipped(
        matrix,
        cx,
        cy,
        mx,
        my,
        min_x,
        max_x,
        min_y,
        max_y,
        theme.secondary,
    );

    // 5. Sweeping Second Hand
    if show_seconds && radius >= 8 {
        let sec_val = now.second() as f32 + sub_second;
        let sec_angle =
            (sec_val / 60.0) * 2.0 * std::f32::consts::PI - (std::f32::consts::PI / 2.0);
        let sec_len = ((radius as f32) * 0.88).max(4.0);
        let sx = cx + (sec_len * sec_angle.cos()) as i32;
        let sy = cy + (sec_len * sec_angle.sin()) as i32;
        draw_line_clipped(
            matrix, cx, cy, sx, sy, min_x, max_x, min_y, max_y, theme.red,
        );
    }

    // 6. Center Jewel Pivot Dot
    draw_pixel_clipped(matrix, cx, cy, min_x, max_x, min_y, max_y, theme.primary);
}

fn render_digital_clock(
    matrix: &mut dyn MatrixBackend,
    font: &ArcadeFont<'_>,
    rect_x: i32,
    rect_y: i32,
    rect_w: i32,
    rect_h: i32,
    now: &DateTime<Local>,
    theme: &DashboardTheme,
    show_seconds: bool,
    show_date: bool,
) {
    fill_rect_clipped(
        matrix,
        rect_x,
        rect_y,
        rect_w,
        rect_h,
        rect_x,
        rect_x + rect_w,
        rect_y,
        rect_y + rect_h,
        theme.panel_bg,
    );
    draw_rect_clipped(
        matrix,
        rect_x,
        rect_y,
        rect_w,
        rect_h,
        rect_x,
        rect_x + rect_w,
        rect_y,
        rect_y + rect_h,
        theme.border,
    );

    let time_str = if show_seconds && rect_w >= 54 {
        now.format("%H:%M:%S").to_string()
    } else {
        now.format("%H:%M").to_string()
    };

    let (_, tw, _) = font.get_pixel_map(&time_str, 1.0);
    let tx = rect_x + (rect_w - tw) / 2;
    let ty = if show_date && rect_h >= 24 {
        rect_y + 3
    } else {
        rect_y + (rect_h - 7) / 2
    };

    draw_clipped_text(
        matrix,
        &time_str,
        font,
        tx,
        ty,
        rect_x + 1,
        rect_x + rect_w - 1,
        rect_y + 1,
        rect_y + rect_h - 1,
        theme.primary,
    );

    if show_date && rect_h >= 24 {
        let date_str = now.format("%d/%m").to_string();
        let (_, dw, _) = font.get_pixel_map(&date_str, 1.0);
        let dx = rect_x + (rect_w - dw) / 2;
        draw_clipped_text(
            matrix,
            &date_str,
            font,
            dx,
            rect_y + 14,
            rect_x + 1,
            rect_x + rect_w - 1,
            rect_y + 1,
            rect_y + rect_h - 1,
            theme.text,
        );
    }
}

// ============================================================================
// Dashboard Engine Implementation
// ============================================================================

impl DashboardEngine {
    pub fn new() -> Self {
        Self {
            base_renderer: BaseRenderer::new(),
            clock_mode: ClockMode::WatchDial, // Default Analog watch face matching ESP32
            theme_id: 0,
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
        self.theme_id = config.get_int("theme", 0);
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
                if last_fetch.elapsed() >= Duration::from_secs(60) {
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
                let (_, tw, _) = font.get_pixel_map(&time_str, 1.0);
                let tx = ((w - tw) / 2 + self.offset_x).max(1);
                draw_clipped_text(
                    matrix,
                    &time_str,
                    &font,
                    tx,
                    cur_y,
                    0,
                    w,
                    0,
                    h,
                    theme.primary,
                );
                cur_y += 10;
            }

            if self.show_date && cur_y < h - 30 {
                let date_str = now.format("%d/%m").to_string();
                let (_, dw, _) = font.get_pixel_map(&date_str, 1.0);
                let dx = ((w - dw) / 2 + self.offset_x).max(1);
                draw_clipped_text(matrix, &date_str, &font, dx, cur_y, 0, w, 0, h, theme.text);
                cur_y += 10;
            }

            if cur_y < h - 20 {
                for x in 2..w - 2 {
                    matrix.set_pixel(x, cur_y, theme.border.0, theme.border.1, theme.border.2);
                }
                cur_y += 3;
            }

            if self.show_weather && cur_y < h - 20 {
                let t_str = format!("{:.0}°C", data.temp_c);
                draw_mini_weather_icon(
                    matrix,
                    2 + self.offset_x,
                    cur_y,
                    0,
                    w,
                    0,
                    h,
                    data.weather_code,
                );
                let (_, vw, _) = font.get_pixel_map(&t_str, 1.0);
                draw_clipped_text(
                    matrix,
                    &t_str,
                    &font,
                    w - vw - 2 + self.offset_x,
                    cur_y,
                    0,
                    w,
                    0,
                    h,
                    theme.accent,
                );
                cur_y += 10;
            }

            if self.show_sysinfo && cur_y < h - 10 {
                let sys_str = format!("CPU:{:.0}%", data.cpu_usage);
                draw_clipped_text(
                    matrix,
                    &sys_str,
                    &font,
                    2 + self.offset_x,
                    cur_y,
                    0,
                    w,
                    0,
                    h,
                    theme.text,
                );
                cur_y += 10;
            }

            if self.show_markets && !data.markets.is_empty() && cur_y < h - 8 {
                let m = &data.markets[(now.second() as usize / 3) % data.markets.len()];
                let p_str = Self::format_market_price(m.price);
                let m_col = if m.change_24h >= 0.0 {
                    theme.green
                } else {
                    theme.red
                };
                draw_clipped_text(
                    matrix,
                    &m.symbol,
                    &font,
                    2 + self.offset_x,
                    h - 8,
                    0,
                    w,
                    0,
                    h,
                    theme.primary,
                );
                let (_, pw, _) = font.get_pixel_map(&p_str, 1.0);
                draw_clipped_text(
                    matrix,
                    &p_str,
                    &font,
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
                let clock_rect_x = self.offset_x;
                let clock_rect_y = self.offset_y;
                let clock_rect_w = clock_w;
                let clock_rect_h = h;

                match self.clock_mode {
                    ClockMode::WatchDial => {
                        render_analog_clock(
                            matrix,
                            clock_rect_x,
                            clock_rect_y,
                            clock_rect_w,
                            clock_rect_h,
                            &now,
                            sub_second,
                            &theme,
                            self.show_seconds,
                        );
                    }
                    ClockMode::Digital => {
                        render_digital_clock(
                            matrix,
                            &font,
                            clock_rect_x,
                            clock_rect_y,
                            clock_rect_w,
                            clock_rect_h,
                            &now,
                            &theme,
                            self.show_seconds,
                            self.show_date,
                        );
                    }
                    ClockMode::Minimal => {
                        render_digital_clock(
                            matrix,
                            &font,
                            clock_rect_x,
                            clock_rect_y,
                            clock_rect_w,
                            clock_rect_h,
                            &now,
                            &theme,
                            false,
                            false,
                        );
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
                            rem_w * 35 / 100
                        } else {
                            rem_w / left_to_place
                        };

                        let rx = cur_top_x;
                        let ry = top_y + self.offset_y;
                        let rw = slot_w.max(10);
                        let rh = top_h;

                        fill_rect_clipped(
                            matrix,
                            rx,
                            ry,
                            rw,
                            rh,
                            rx,
                            rx + rw,
                            ry,
                            ry + rh,
                            theme.panel_bg,
                        );
                        draw_rect_clipped(
                            matrix,
                            rx,
                            ry,
                            rw,
                            rh,
                            rx,
                            rx + rw,
                            ry,
                            ry + rh,
                            theme.border,
                        );

                        let idx = (now.second() as usize / 4) % data.world_times.len();
                        let wt = &data.world_times[idx];
                        let wt_time = utc + chrono::Duration::hours(wt.offset_hours as i64);
                        let wt_str = format!("{}", wt_time.format("%H:%M"));

                        if rw >= 40 {
                            draw_clipped_text(
                                matrix,
                                &format!("[{}]", wt.code),
                                &font,
                                rx + 2,
                                ry + (rh - 7) / 2,
                                rx + 1,
                                rx + rw - 1,
                                ry + 1,
                                ry + rh - 1,
                                theme.secondary,
                            );
                            let (_, tw, _) = font.get_pixel_map(&wt_str, 1.0);
                            draw_clipped_text(
                                matrix,
                                &wt_str,
                                &font,
                                rx + rw - tw - 2,
                                ry + (rh - 7) / 2,
                                rx + 1,
                                rx + rw - 1,
                                ry + 1,
                                ry + rh - 1,
                                theme.primary,
                            );
                        } else {
                            draw_clipped_text(
                                matrix,
                                &wt.code,
                                &font,
                                rx + 2,
                                ry + (rh - 7) / 2,
                                rx + 1,
                                rx + rw - 1,
                                ry + 1,
                                ry + rh - 1,
                                theme.secondary,
                            );
                        }

                        cur_top_x += rw + 2;
                        rem_w -= rw + 2;
                        left_to_place -= 1;
                    }

                    // Climate / Weather Slot
                    if self.show_weather && left_to_place > 0 {
                        let slot_w = if left_to_place == 1 {
                            rem_w
                        } else {
                            rem_w / left_to_place
                        };

                        let rx = cur_top_x;
                        let ry = top_y + self.offset_y;
                        let rw = slot_w.max(10);
                        let rh = top_h;

                        fill_rect_clipped(
                            matrix,
                            rx,
                            ry,
                            rw,
                            rh,
                            rx,
                            rx + rw,
                            ry,
                            ry + rh,
                            theme.panel_bg,
                        );
                        draw_rect_clipped(
                            matrix,
                            rx,
                            ry,
                            rw,
                            rh,
                            rx,
                            rx + rw,
                            ry,
                            ry + rh,
                            theme.border,
                        );

                        let icon_y = ry + (rh - 8) / 2;
                        draw_mini_weather_icon(
                            matrix,
                            rx + 2,
                            icon_y,
                            rx + 1,
                            rx + rw - 1,
                            ry + 1,
                            ry + rh - 1,
                            data.weather_code,
                        );

                        let t_str = format!("{:.0}°", data.temp_c);
                        draw_clipped_text(
                            matrix,
                            &t_str,
                            &font,
                            rx + 12,
                            ry + (rh - 7) / 2,
                            rx + 1,
                            rx + rw - 1,
                            ry + 1,
                            ry + rh - 1,
                            theme.accent,
                        );

                        cur_top_x += rw + 2;
                        rem_w -= rw + 2;
                        left_to_place -= 1;
                    }

                    // System Vitals Slot
                    if self.show_sysinfo && left_to_place > 0 {
                        let rx = cur_top_x;
                        let ry = top_y + self.offset_y;
                        let rw = rem_w.max(10);
                        let rh = top_h;

                        fill_rect_clipped(
                            matrix,
                            rx,
                            ry,
                            rw,
                            rh,
                            rx,
                            rx + rw,
                            ry,
                            ry + rh,
                            theme.panel_bg,
                        );
                        draw_rect_clipped(
                            matrix,
                            rx,
                            ry,
                            rw,
                            rh,
                            rx,
                            rx + rw,
                            ry,
                            ry + rh,
                            theme.border,
                        );

                        let ram_str = format!("R:{:.0}%", data.ram_usage);
                        draw_clipped_text(
                            matrix,
                            &ram_str,
                            &font,
                            rx + 2,
                            ry + (rh - 7) / 2,
                            rx + 1,
                            rx + rw - 1,
                            ry + 1,
                            ry + rh - 1,
                            theme.text_dim,
                        );

                        // 4-bar WiFi signal meter
                        let wx = rx + rw - 13;
                        let wy = ry + rh - 3;
                        let rssi_bars = if data.wifi_rssi > -60 {
                            4
                        } else if data.wifi_rssi > -70 {
                            3
                        } else {
                            2
                        };
                        for b in 0..4 {
                            let bh = (b + 1) * 2;
                            let col = if b < rssi_bars {
                                theme.accent
                            } else {
                                theme.border
                            };
                            for h_bar in 0..bh {
                                draw_pixel_clipped(
                                    matrix,
                                    wx + b * 3,
                                    wy - h_bar,
                                    rx + 1,
                                    rx + rw - 1,
                                    ry + 1,
                                    ry + rh - 1,
                                    col,
                                );
                                draw_pixel_clipped(
                                    matrix,
                                    wx + b * 3 + 1,
                                    wy - h_bar,
                                    rx + 1,
                                    rx + rw - 1,
                                    ry + 1,
                                    ry + rh - 1,
                                    col,
                                );
                            }
                        }
                    }
                }

                // --- BOTTOM ROW: INFINITE MARKET TICKER ---
                if bot_h > 0 && self.show_markets && !data.markets.is_empty() {
                    let rx = content_x + self.offset_x;
                    let ry = bot_y + self.offset_y;
                    let rw = content_w;
                    let rh = bot_h;

                    fill_rect_clipped(
                        matrix,
                        rx,
                        ry,
                        rw,
                        rh,
                        rx,
                        rx + rw,
                        ry,
                        ry + rh,
                        theme.panel_bg,
                    );
                    draw_rect_clipped(
                        matrix,
                        rx,
                        ry,
                        rw,
                        rh,
                        rx,
                        rx + rw,
                        ry,
                        ry + rh,
                        theme.border,
                    );

                    let min_x = rx + 1;
                    let max_x = rx + rw - 1;
                    let min_y = ry + 1;
                    let max_y = ry + rh - 1;

                    let item_w = 64;
                    let total_w = data.markets.len() as i32 * item_w;
                    let now_millis = now_ms();
                    let scroll_offset = ((now_millis * 14) / 1000) as i32 % total_w.max(1);

                    for (i, m) in data.markets.iter().enumerate() {
                        let slot_base_x = (i as i32 * item_w) - scroll_offset;

                        for k in 0..2 {
                            let pos_x = rx + 2 + slot_base_x + (k * total_w);
                            if pos_x + item_w < min_x || pos_x >= max_x {
                                continue;
                            }

                            let icon_y = ry + (rh - 8) / 2;
                            draw_mini_market_icon(
                                matrix, pos_x, icon_y, min_x, max_x, min_y, max_y, &m.symbol,
                            );

                            let text_y = ry + (rh - 7) / 2;
                            draw_clipped_text(
                                matrix,
                                &m.symbol,
                                &font,
                                pos_x + 10,
                                text_y,
                                min_x,
                                max_x,
                                min_y,
                                max_y,
                                theme.text,
                            );

                            let p_str = Self::format_market_price(m.price);
                            draw_clipped_text(
                                matrix,
                                &p_str,
                                &font,
                                pos_x + 32,
                                text_y,
                                min_x,
                                max_x,
                                min_y,
                                max_y,
                                theme.primary,
                            );

                            let trend_col = if m.change_24h >= 0.0 {
                                theme.green
                            } else {
                                theme.red
                            };
                            let chg_str = format!(
                                "{}{:.0}%",
                                if m.change_24h >= 0.0 { "+" } else { "" },
                                m.change_24h
                            );
                            draw_clipped_text(
                                matrix,
                                &chg_str,
                                &font,
                                pos_x + item_w - 18,
                                text_y,
                                min_x,
                                max_x,
                                min_y,
                                max_y,
                                trend_col,
                            );
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Registration via Distributed Slice
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
                    id: "theme",
                    field_type: ConfigType::Options,
                    label: "Color Theme",
                    description: "Color palette for dashboard widgets",
                    default_value: "0",
                    options: Some(vec![
                        ConfigOption {
                            label: "Cyberpunk Neon",
                            value: "0",
                        },
                        ConfigOption {
                            label: "Arcade Amber HUD",
                            value: "1",
                        },
                        ConfigOption {
                            label: "Minimalist Luxury",
                            value: "2",
                        },
                        ConfigOption {
                            label: "Matrix Phosphor",
                            value: "3",
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

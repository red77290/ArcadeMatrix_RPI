use super::data::{format_market_price, MarketQuote};
use super::font::draw_text_clipped;
use super::geometry::*;
use crate::core::matrix::MatrixBackend;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

static ICON_CACHE_8X8: OnceLock<Mutex<HashMap<String, Option<image::RgbaImage>>>> = OnceLock::new();

fn get_icon_cache() -> &'static Mutex<HashMap<String, Option<image::RgbaImage>>> {
    ICON_CACHE_8X8.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn get_or_load_market_icon_8x8(symbol: &str) -> Option<image::RgbaImage> {
    let sym = symbol.to_uppercase();
    {
        let cache = get_icon_cache().lock();
        if let Some(cached) = cache.get(&sym) {
            return cached.clone();
        }
    }

    let loaded = load_or_fetch_market_icon(&sym);
    let mut cache = get_icon_cache().lock();
    cache.insert(sym, loaded.clone());
    loaded
}

fn load_or_fetch_market_icon(symbol: &str) -> Option<image::RgbaImage> {
    let lower = symbol.to_lowercase();
    let paths = [
        format!("data/crypto_icons/{}.png", lower),
        format!("data/stock_icons/{}.png", lower),
        format!("data/crypto_icons/{}.png", symbol),
        format!("data/stock_icons/{}.png", symbol),
    ];

    for path in &paths {
        if Path::new(path).exists() {
            if let Ok(img) = image::open(path) {
                let rgba = img.into_rgba8();
                let resized =
                    image::imageops::resize(&rgba, 8, 8, image::imageops::FilterType::Triangle);
                return Some(resized);
            }
        }
    }

    // Attempt background download from standard ticker endpoints
    let urls = [
        format!(
            "https://financialmodelingprep.com/image-stock/{}.png",
            symbol
        ),
        format!("https://assets.coincap.io/assets/icons/{}@2x.png", lower),
    ];

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .user_agent("Mozilla/5.0")
        .build()
        .ok()?;

    for u in &urls {
        let proxy_url = format!("https://wsrv.nl/?url={}&w=8&h=8&output=png", u);
        if let Ok(resp) = client.get(&proxy_url).send() {
            if resp.status().is_success() {
                if let Ok(bytes) = resp.bytes() {
                    let dest = format!("data/crypto_icons/{}.png", lower);
                    let _ = std::fs::create_dir_all("data/crypto_icons");
                    let _ = std::fs::write(&dest, &bytes);

                    if let Ok(img) = image::load_from_memory(&bytes) {
                        let rgba = img.into_rgba8();
                        let resized = image::imageops::resize(
                            &rgba,
                            8,
                            8,
                            image::imageops::FilterType::Triangle,
                        );
                        return Some(resized);
                    }
                }
            }
        }
    }

    None
}

type Icon8x8 = [(u8, u8, u8); 64];

const C_BITCOIN: (u8, u8, u8) = (247, 147, 26); // Bitcoin Orange
const C_ETHEREUM: (u8, u8, u8) = (98, 126, 234); // Ethereum Blue-Purple
const C_SOLANA: (u8, u8, u8) = (20, 241, 149); // Solana Neon Green
const C_DOGE: (u8, u8, u8) = (194, 166, 50); // Doge Gold
const C_APPLE: (u8, u8, u8) = (225, 225, 225); // Apple Silver
const C_NVIDIA: (u8, u8, u8) = (118, 185, 0); // Nvidia Green
const C_TESLA: (u8, u8, u8) = (227, 25, 55); // Tesla Red
const C_WHITE: (u8, u8, u8) = (255, 255, 255);
const C_NONE: (u8, u8, u8) = (0, 0, 0);

// Official 8x8 pixel-art fallback bitmaps matching ESP32 firmware
const ICON_BTC: Icon8x8 = [
    C_NONE, C_BITCOIN, C_BITCOIN, C_BITCOIN, C_BITCOIN, C_BITCOIN, C_NONE, C_NONE, C_BITCOIN,
    C_BITCOIN, C_WHITE, C_BITCOIN, C_WHITE, C_BITCOIN, C_BITCOIN, C_NONE, C_BITCOIN, C_BITCOIN,
    C_WHITE, C_WHITE, C_BITCOIN, C_BITCOIN, C_BITCOIN, C_NONE, C_BITCOIN, C_BITCOIN, C_WHITE,
    C_BITCOIN, C_WHITE, C_BITCOIN, C_BITCOIN, C_NONE, C_BITCOIN, C_BITCOIN, C_WHITE, C_WHITE,
    C_BITCOIN, C_BITCOIN, C_BITCOIN, C_NONE, C_BITCOIN, C_BITCOIN, C_WHITE, C_BITCOIN, C_WHITE,
    C_BITCOIN, C_BITCOIN, C_NONE, C_NONE, C_BITCOIN, C_BITCOIN, C_BITCOIN, C_BITCOIN, C_BITCOIN,
    C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE,
];

const ICON_ETH: Icon8x8 = [
    C_NONE, C_NONE, C_NONE, C_ETHEREUM, C_ETHEREUM, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE,
    C_ETHEREUM, C_WHITE, C_ETHEREUM, C_NONE, C_NONE, C_NONE, C_NONE, C_ETHEREUM, C_ETHEREUM,
    C_WHITE, C_ETHEREUM, C_ETHEREUM, C_NONE, C_NONE, C_ETHEREUM, C_ETHEREUM, C_ETHEREUM, C_WHITE,
    C_ETHEREUM, C_ETHEREUM, C_ETHEREUM, C_NONE, C_NONE, C_ETHEREUM, C_ETHEREUM, C_WHITE,
    C_ETHEREUM, C_ETHEREUM, C_NONE, C_NONE, C_NONE, C_NONE, C_ETHEREUM, C_WHITE, C_ETHEREUM,
    C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_ETHEREUM, C_ETHEREUM, C_NONE, C_NONE, C_NONE,
    C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE,
];

const ICON_SOL: Icon8x8 = [
    C_SOLANA, C_SOLANA, C_SOLANA, C_SOLANA, C_SOLANA, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE,
    C_NONE, C_SOLANA, C_SOLANA, C_SOLANA, C_SOLANA, C_SOLANA, C_SOLANA, C_SOLANA, C_SOLANA,
    C_SOLANA, C_SOLANA, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_SOLANA, C_SOLANA,
    C_SOLANA, C_SOLANA, C_SOLANA, C_SOLANA, C_SOLANA, C_SOLANA, C_SOLANA, C_SOLANA, C_NONE, C_NONE,
    C_NONE, C_NONE, C_NONE, C_NONE, C_SOLANA, C_SOLANA, C_SOLANA, C_SOLANA, C_SOLANA, C_SOLANA,
    C_SOLANA, C_SOLANA, C_SOLANA, C_SOLANA, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE,
    C_NONE, C_NONE, C_NONE, C_NONE,
];

const ICON_DOGE: Icon8x8 = [
    C_NONE, C_DOGE, C_DOGE, C_DOGE, C_DOGE, C_DOGE, C_NONE, C_NONE, C_DOGE, C_DOGE, C_WHITE,
    C_WHITE, C_DOGE, C_DOGE, C_DOGE, C_NONE, C_DOGE, C_DOGE, C_WHITE, C_DOGE, C_WHITE, C_DOGE,
    C_DOGE, C_NONE, C_DOGE, C_DOGE, C_WHITE, C_DOGE, C_WHITE, C_DOGE, C_DOGE, C_NONE, C_DOGE,
    C_DOGE, C_WHITE, C_DOGE, C_WHITE, C_DOGE, C_DOGE, C_NONE, C_DOGE, C_DOGE, C_WHITE, C_WHITE,
    C_DOGE, C_DOGE, C_DOGE, C_NONE, C_NONE, C_DOGE, C_DOGE, C_DOGE, C_DOGE, C_DOGE, C_NONE, C_NONE,
    C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE,
];

const ICON_AAPL: Icon8x8 = [
    C_NONE, C_NONE, C_NONE, C_APPLE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_APPLE,
    C_APPLE, C_APPLE, C_NONE, C_NONE, C_NONE, C_NONE, C_APPLE, C_APPLE, C_APPLE, C_APPLE, C_APPLE,
    C_NONE, C_NONE, C_NONE, C_APPLE, C_APPLE, C_APPLE, C_APPLE, C_APPLE, C_NONE, C_NONE, C_NONE,
    C_APPLE, C_APPLE, C_APPLE, C_APPLE, C_APPLE, C_NONE, C_NONE, C_NONE, C_APPLE, C_APPLE, C_APPLE,
    C_APPLE, C_APPLE, C_NONE, C_NONE, C_NONE, C_NONE, C_APPLE, C_NONE, C_APPLE, C_NONE, C_NONE,
    C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE,
];

const ICON_NVDA: Icon8x8 = [
    C_NVIDIA, C_NVIDIA, C_NVIDIA, C_NVIDIA, C_NVIDIA, C_NVIDIA, C_NVIDIA, C_NONE, C_NVIDIA, C_NONE,
    C_NONE, C_NONE, C_NONE, C_NONE, C_NVIDIA, C_NONE, C_NVIDIA, C_NONE, C_NVIDIA, C_NVIDIA,
    C_NVIDIA, C_NONE, C_NVIDIA, C_NONE, C_NVIDIA, C_NONE, C_NVIDIA, C_WHITE, C_NVIDIA, C_NONE,
    C_NVIDIA, C_NONE, C_NVIDIA, C_NONE, C_NVIDIA, C_NVIDIA, C_NVIDIA, C_NONE, C_NVIDIA, C_NONE,
    C_NVIDIA, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NVIDIA, C_NONE, C_NVIDIA, C_NVIDIA,
    C_NVIDIA, C_NVIDIA, C_NVIDIA, C_NVIDIA, C_NVIDIA, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE,
    C_NONE, C_NONE, C_NONE, C_NONE,
];

const ICON_TSLA: Icon8x8 = [
    C_TESLA, C_TESLA, C_TESLA, C_TESLA, C_TESLA, C_TESLA, C_TESLA, C_NONE, C_NONE, C_NONE, C_TESLA,
    C_TESLA, C_TESLA, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_TESLA, C_NONE, C_NONE,
    C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_TESLA, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE,
    C_NONE, C_NONE, C_TESLA, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_TESLA,
    C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_TESLA, C_NONE, C_NONE, C_NONE,
    C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE, C_NONE,
];

const ICON_MSFT: Icon8x8 = [
    C_NONE,
    (242, 80, 34),
    (242, 80, 34),
    C_NONE,
    C_NONE,
    (127, 186, 0),
    (127, 186, 0),
    C_NONE,
    C_NONE,
    (242, 80, 34),
    (242, 80, 34),
    C_NONE,
    C_NONE,
    (127, 186, 0),
    (127, 186, 0),
    C_NONE,
    C_NONE,
    (242, 80, 34),
    (242, 80, 34),
    C_NONE,
    C_NONE,
    (127, 186, 0),
    (127, 186, 0),
    C_NONE,
    C_NONE,
    C_NONE,
    C_NONE,
    C_NONE,
    C_NONE,
    C_NONE,
    C_NONE,
    C_NONE,
    C_NONE,
    C_NONE,
    C_NONE,
    C_NONE,
    C_NONE,
    C_NONE,
    C_NONE,
    C_NONE,
    C_NONE,
    (0, 164, 239),
    (0, 164, 239),
    C_NONE,
    C_NONE,
    (255, 185, 0),
    (255, 185, 0),
    C_NONE,
    C_NONE,
    (0, 164, 239),
    (0, 164, 239),
    C_NONE,
    C_NONE,
    (255, 185, 0),
    (255, 185, 0),
    C_NONE,
    C_NONE,
    (0, 164, 239),
    (0, 164, 239),
    C_NONE,
    C_NONE,
    (255, 185, 0),
    (255, 185, 0),
    C_NONE,
];

pub fn draw_mini_market_icon(
    matrix: &mut dyn MatrixBackend,
    x: i32,
    y: i32,
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
    symbol: &str,
    _theme: &DashboardTheme,
) {
    // 1. Try downloaded & cached 8x8 PNG icon from disk/remote
    if let Some(img) = get_or_load_market_icon_8x8(symbol) {
        for py in 0..img.height() {
            for px in 0..img.width() {
                let p = img.get_pixel(px, py);
                if p[3] > 64 {
                    draw_pixel_clipped(
                        matrix,
                        x + px as i32,
                        y + py as i32,
                        min_x,
                        max_x,
                        min_y,
                        max_y,
                        (p[0], p[1], p[2]),
                    );
                }
            }
        }
        return;
    }

    // 2. Fallback to pixel-art bitmap or generic badge
    let icon_data: Option<&Icon8x8> = match symbol.to_uppercase().as_str() {
        "BTC" => Some(&ICON_BTC),
        "ETH" => Some(&ICON_ETH),
        "SOL" => Some(&ICON_SOL),
        "DOGE" => Some(&ICON_DOGE),
        "AAPL" => Some(&ICON_AAPL),
        "NVDA" => Some(&ICON_NVDA),
        "TSLA" => Some(&ICON_TSLA),
        "MSFT" => Some(&ICON_MSFT),
        _ => None,
    };

    if let Some(icon) = icon_data {
        for r in 0..8 {
            for c in 0..8 {
                let col = icon[r * 8 + c];
                if col != C_NONE {
                    draw_pixel_clipped(
                        matrix,
                        x + c as i32,
                        y + r as i32,
                        min_x,
                        max_x,
                        min_y,
                        max_y,
                        col,
                    );
                }
            }
        }
    } else {
        // Generic coin badge with golden border
        let gold = (255, 180, 0);
        for c in 0..8 {
            draw_pixel_clipped(matrix, x + c, y, min_x, max_x, min_y, max_y, gold);
            draw_pixel_clipped(matrix, x + c, y + 7, min_x, max_x, min_y, max_y, gold);
            draw_pixel_clipped(matrix, x, y + c, min_x, max_x, min_y, max_y, gold);
            draw_pixel_clipped(matrix, x + 7, y + c, min_x, max_x, min_y, max_y, gold);
        }
        draw_pixel_clipped(matrix, x + 3, y + 3, min_x, max_x, min_y, max_y, C_WHITE);
        draw_pixel_clipped(matrix, x + 4, y + 3, min_x, max_x, min_y, max_y, C_WHITE);
        draw_pixel_clipped(matrix, x + 3, y + 4, min_x, max_x, min_y, max_y, C_WHITE);
        draw_pixel_clipped(matrix, x + 4, y + 4, min_x, max_x, min_y, max_y, C_WHITE);
    }
}

pub fn render_market_ticker(
    matrix: &mut dyn MatrixBackend,
    rect: &Rect,
    markets: &[MarketQuote],
    now_millis: u128,
    theme: &DashboardTheme,
) {
    if markets.is_empty() || rect.w < 10 || rect.h < 8 {
        return;
    }

    fill_rect_clipped(
        matrix,
        rect,
        rect.min_x(),
        rect.max_x(),
        rect.min_y(),
        rect.max_y(),
        theme.panel_bg,
    );
    draw_rect_clipped(
        matrix,
        rect,
        rect.min_x(),
        rect.max_x(),
        rect.min_y(),
        rect.max_y(),
        theme.border,
    );

    let min_x = rect.inner_min_x();
    let max_x = rect.inner_max_x();
    let min_y = rect.inner_min_y();
    let max_y = rect.inner_max_y();

    let item_w = 88;
    let total_w = (markets.len() as i32 * item_w).max(1);
    let speed_px_per_sec = 20;
    let scroll_offset = ((now_millis * speed_px_per_sec) / 1000) as i32 % total_w;

    for (i, m) in markets.iter().enumerate() {
        let slot_base_x = (i as i32 * item_w) - scroll_offset;

        for k in -1..3 {
            let pos_x = rect.x + 2 + slot_base_x + (k * total_w);
            if pos_x + item_w < min_x || pos_x >= max_x {
                continue;
            }

            let icon_y = rect.y + (rect.h - 8) / 2;
            draw_mini_market_icon(
                matrix, pos_x, icon_y, min_x, max_x, min_y, max_y, &m.symbol, theme,
            );

            let text_y = rect.y + (rect.h - 7) / 2;
            draw_text_clipped(
                matrix,
                &m.symbol,
                pos_x + 10,
                text_y,
                min_x,
                max_x,
                min_y,
                max_y,
                theme.text,
            );

            let p_str = format_market_price(m.price);
            draw_text_clipped(
                matrix,
                &p_str,
                pos_x + 34,
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
            draw_text_clipped(
                matrix,
                &chg_str,
                pos_x + item_w - 20,
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

use super::data::{format_market_price, MarketQuote};
use super::font::draw_text_clipped;
use super::geometry::*;
use crate::core::matrix::MatrixBackend;

pub fn draw_mini_market_icon(
    matrix: &mut dyn MatrixBackend,
    x: i32,
    y: i32,
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
    symbol: &str,
) {
    let gold = COLOR_GOLD;
    let blue = (0, 140, 255);
    let purple = (160, 40, 255);
    let white = COLOR_TEXT;
    let green = COLOR_GREEN;

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

pub fn render_market_ticker(
    matrix: &mut dyn MatrixBackend,
    rect: &Rect,
    markets: &[MarketQuote],
    now_millis: u128,
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
        COLOR_PANEL_BG,
    );
    draw_rect_clipped(
        matrix,
        rect,
        rect.min_x(),
        rect.max_x(),
        rect.min_y(),
        rect.max_y(),
        COLOR_BORDER,
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
            draw_mini_market_icon(matrix, pos_x, icon_y, min_x, max_x, min_y, max_y, &m.symbol);

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
                COLOR_TEXT,
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
                COLOR_PRIMARY,
            );

            let trend_col = if m.change_24h >= 0.0 {
                COLOR_GREEN
            } else {
                COLOR_RED
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

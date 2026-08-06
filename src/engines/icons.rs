pub const ICON_BTC: [&str; 8] = [
    ".CCCCC..", "CCWCWCC.", "CCWWCCC.", "CCWCWCC.", "CCWWCCC.", "CCWCWCC.", ".CCCCC..", "........",
];

pub const ICON_ETH: [&str; 8] = [
    "...CC...", "..CWC...", ".CCWCC..", "CCCWCCC.", ".CCWCC..", "..CWC...", "...CC...", "........",
];

pub const ICON_SOL: [&str; 8] = [
    "CCCCC...", "...CCCCC", "CCCCC...", "...CCCCC", "CCCCC...", "...CCCCC", "CCCCC...", "........",
];

pub const ICON_AAPL: [&str; 8] = [
    "...C....", "..CCC...", ".CCCCCC.", ".CCCCCC.", ".CCCCCC.", ".CCCCCC.", "..C.C...", "........",
];

pub const ICON_NVDA: [&str; 8] = [
    "CCCCCCC.", "C.....C.", "C.CCC.C.", "C.CWC.C.", "C.CCC.C.", "C.....C.", "CCCCCCC.", "........",
];

pub const ICON_TSLA: [&str; 8] = [
    "CCCCCCC.", "..CCC...", "...C....", "...C....", "...C....", "...C....", "...C....", "........",
];

pub fn get_crypto_color(symbol: &str) -> (u8, u8, u8) {
    match symbol {
        "BTC" => (247, 147, 26),
        "ETH" => (98, 126, 234),
        "SOL" => (20, 241, 149),
        "DOGE" => (194, 166, 50),
        _ => (200, 200, 200),
    }
}

pub fn get_stock_color(symbol: &str) -> (u8, u8, u8) {
    match symbol {
        "AAPL" => (225, 225, 225),
        "NVDA" => (118, 185, 0),
        "TSLA" => (227, 25, 55),
        "MSFT" => (0, 164, 239),
        _ => (200, 200, 200),
    }
}

pub fn draw_icon(
    matrix: &mut dyn crate::core::matrix::MatrixBackend,
    icon: &[&str; 8],
    x: i32,
    y: i32,
    scale: i32,
    color: (u8, u8, u8),
) {
    for (row_idx, row) in icon.iter().enumerate() {
        for (col_idx, ch) in row.chars().enumerate() {
            let px_color = match ch {
                'C' => Some(color),
                'W' => Some((255, 255, 255)),
                _ => None,
            };

            if let Some(c) = px_color {
                for dy in 0..scale {
                    for dx in 0..scale {
                        let draw_x = x + (col_idx as i32 * scale) + dx;
                        let draw_y = y + (row_idx as i32 * scale) + dy;
                        if draw_x >= 0
                            && draw_x < matrix.width() as i32
                            && draw_y >= 0
                            && draw_y < matrix.height() as i32
                        {
                            matrix.set_pixel(draw_x, draw_y, c.0, c.1, c.2);
                        }
                    }
                }
            }
        }
    }
}

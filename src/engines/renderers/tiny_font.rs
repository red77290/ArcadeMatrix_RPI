//! 3x5 "tiny" pixel font used by the Graph y-axis labels and the Values engine
//! labels/units on small panels. The same bitmap ships in the ESP32 firmware,
//! so both signs draw identical text. Lowercase is drawn as uppercase.

use crate::core::matrix::IDrawingSurface;

/// Glyph width, height and advance in pixels.
pub const TINY_W: i32 = 3;
pub const TINY_H: i32 = 5;
pub const TINY_ADVANCE: i32 = 4;

/// Rows top to bottom; bit 2 = left pixel, bit 0 = right pixel.
#[rustfmt::skip]
fn glyph(c: char) -> Option<[u8; 5]> {
    let g = match c.to_ascii_uppercase() {
        ' ' => [0b000, 0b000, 0b000, 0b000, 0b000],
        '-' => [0b000, 0b000, 0b111, 0b000, 0b000],
        '.' => [0b000, 0b000, 0b000, 0b000, 0b010],
        '%' => [0b101, 0b001, 0b010, 0b100, 0b101],
        '$' => [0b011, 0b110, 0b010, 0b011, 0b110],
        '°' => [0b010, 0b101, 0b010, 0b000, 0b000],
        '/' => [0b001, 0b001, 0b010, 0b100, 0b100],
        ':' => [0b000, 0b010, 0b000, 0b010, 0b000],
        '+' => [0b000, 0b010, 0b111, 0b010, 0b000],
        '?' => [0b111, 0b001, 0b010, 0b000, 0b010],
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b001, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        'A' => [0b010, 0b101, 0b111, 0b101, 0b101],
        'B' => [0b110, 0b101, 0b110, 0b101, 0b110],
        'C' => [0b011, 0b100, 0b100, 0b100, 0b011],
        'D' => [0b110, 0b101, 0b101, 0b101, 0b110],
        'E' => [0b111, 0b100, 0b110, 0b100, 0b111],
        'F' => [0b111, 0b100, 0b110, 0b100, 0b100],
        'G' => [0b011, 0b100, 0b101, 0b101, 0b011],
        'H' => [0b101, 0b101, 0b111, 0b101, 0b101],
        'I' => [0b111, 0b010, 0b010, 0b010, 0b111],
        'J' => [0b001, 0b001, 0b001, 0b101, 0b010],
        'K' => [0b101, 0b101, 0b110, 0b101, 0b101],
        'L' => [0b100, 0b100, 0b100, 0b100, 0b111],
        'M' => [0b101, 0b111, 0b111, 0b101, 0b101],
        'N' => [0b110, 0b101, 0b101, 0b101, 0b101],
        'O' => [0b010, 0b101, 0b101, 0b101, 0b010],
        'P' => [0b110, 0b101, 0b110, 0b100, 0b100],
        'Q' => [0b010, 0b101, 0b101, 0b111, 0b011],
        'R' => [0b110, 0b101, 0b110, 0b101, 0b101],
        'S' => [0b011, 0b100, 0b010, 0b001, 0b110],
        'T' => [0b111, 0b010, 0b010, 0b010, 0b010],
        'U' => [0b101, 0b101, 0b101, 0b101, 0b111],
        'V' => [0b101, 0b101, 0b101, 0b101, 0b010],
        'W' => [0b101, 0b101, 0b111, 0b111, 0b101],
        'X' => [0b101, 0b101, 0b010, 0b101, 0b101],
        'Y' => [0b101, 0b101, 0b010, 0b010, 0b010],
        'Z' => [0b111, 0b001, 0b010, 0b100, 0b111],
        _ => return None,
    };
    Some(g)
}

/// Width in pixels of `text` (no trailing spacing column).
pub fn tiny_text_width(text: &str) -> i32 {
    let n = text.chars().count() as i32;
    if n == 0 {
        0
    } else {
        n * TINY_ADVANCE - 1
    }
}

/// Draws `text` with its top-left at (x, y), clipped to columns `[min_x, max_x)`.
pub fn draw_tiny_text(
    matrix: &mut dyn IDrawingSurface,
    text: &str,
    mut x: i32,
    y: i32,
    min_x: i32,
    max_x: i32,
    color: (u8, u8, u8),
) {
    for c in text.chars() {
        if let Some(rows) = glyph(c) {
            for (ry, bits) in rows.iter().enumerate() {
                for col in 0..TINY_W {
                    if bits & (0b100 >> col) != 0 {
                        let px = x + col;
                        if px >= min_x && px < max_x {
                            matrix.set_pixel(px, y + ry as i32, color.0, color.1, color.2);
                        }
                    }
                }
            }
        }
        x += TINY_ADVANCE;
    }
}

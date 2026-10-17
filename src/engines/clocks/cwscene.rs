//! Shared geometry for the clock faces adapted from the 64x64 "Clockwise" clockfaces.
//!
//! Those faces are drawn for a square panel. Rather than stretch one across a wide sign, the scene
//! is placed in the middle at its own size and a face draws its moving parts at scene coordinates,
//! which keeps the middle 64x64 behaving exactly as the original does. On a panel shorter than the
//! scene the artwork is halved so the whole picture still fits.
//!
//! The same helpers exist in the ESP32 firmware, so both signs lay these faces out identically.

use crate::core::matrix::IDrawingSurface;

pub const SIZE: i32 = 64;

/// 1 when the scene fits at its own size, 2 when it has to be halved to fit the panel height.
pub fn divisor(panel_h: i32) -> i32 {
    if panel_h >= SIZE {
        1
    } else {
        2
    }
}

pub fn scaled_size(panel_h: i32) -> i32 {
    SIZE / divisor(panel_h)
}

pub fn origin_x(panel_w: i32, panel_h: i32) -> i32 {
    (panel_w - scaled_size(panel_h)) / 2
}

pub fn origin_y(panel_h: i32) -> i32 {
    (panel_h - scaled_size(panel_h)) / 2
}

/// Scene coordinate to panel coordinate.
pub fn map_x(x: i32, panel_w: i32, panel_h: i32) -> i32 {
    origin_x(panel_w, panel_h) + x / divisor(panel_h)
}

pub fn map_y(y: i32, panel_h: i32) -> i32 {
    origin_y(panel_h) + y / divisor(panel_h)
}

#[inline]
fn rgb565(c: u16) -> (u8, u8, u8) {
    let r = ((c >> 11) & 0x1F) as u8;
    let g = ((c >> 5) & 0x3F) as u8;
    let b = (c & 0x1F) as u8;
    (
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    )
}

pub fn put(matrix: &mut dyn IDrawingSurface, x: i32, y: i32, c: u16) {
    if x < 0 || y < 0 || x >= matrix.width() as i32 || y >= matrix.height() as i32 {
        return;
    }
    let (r, g, b) = rgb565(c);
    matrix.set_pixel(x, y, r, g, b);
}

/// Paint a 64x64 background: the scene in the middle, with its outermost columns and rows carried
/// out to the panel edges so the extra width reads as more of the same picture.
pub fn draw_background(matrix: &mut dyn IDrawingSurface, bg: &[u16], panel_w: i32, panel_h: i32) {
    let d = divisor(panel_h);
    let ox = origin_x(panel_w, panel_h);
    let oy = origin_y(panel_h);
    for py in 0..panel_h {
        let mut sy = (py - oy) * d;
        sy = sy.clamp(0, SIZE - 1);
        for px in 0..panel_w {
            let mut sx = (px - ox) * d;
            sx = sx.clamp(0, SIZE - 1);
            put(matrix, px, py, bg[(sy * SIZE + sx) as usize]);
        }
    }
}

/// Fill the panel outside the scene with one colour, for faces whose artwork is a framed picture
/// rather than a landscape (repeating their edge would flood the panel).
pub fn fill_surround(matrix: &mut dyn IDrawingSurface, panel_w: i32, panel_h: i32, c: u16) {
    let ox = origin_x(panel_w, panel_h);
    let oy = origin_y(panel_h);
    let size = scaled_size(panel_h);
    for py in 0..panel_h {
        for px in 0..panel_w {
            if px >= ox && px < ox + size && py >= oy && py < oy + size {
                continue;
            }
            put(matrix, px, py, c);
        }
    }
}

/// Blit a sprite at scene coordinates, skipping `mask`.
#[allow(clippy::too_many_arguments)]
pub fn draw_sprite(
    matrix: &mut dyn IDrawingSurface,
    data: &[u16],
    w: i32,
    h: i32,
    scene_x: i32,
    scene_y: i32,
    panel_w: i32,
    panel_h: i32,
    mask: u16,
) {
    let d = divisor(panel_h);
    let left = map_x(scene_x, panel_w, panel_h);
    let top = map_y(scene_y, panel_h);
    let mut row = 0;
    while row < h {
        let mut col = 0;
        while col < w {
            let c = data[(row * w + col) as usize];
            if c != mask {
                put(matrix, left + col / d, top + row / d, c);
            }
            col += d;
        }
        row += d;
    }
}

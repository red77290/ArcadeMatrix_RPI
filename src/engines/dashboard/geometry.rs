use crate::core::matrix::MatrixBackend;

// ============================================================================
// Fixed High-Contrast Retro Arcade Palette (Zero Config Overload)
// ============================================================================

pub const COLOR_PRIMARY: (u8, u8, u8) = (0, 240, 255); // Neon Cyan
pub const COLOR_SECONDARY: (u8, u8, u8) = (255, 0, 128); // Neon Magenta
pub const COLOR_ACCENT: (u8, u8, u8) = (255, 220, 0); // Vibrant Gold
pub const COLOR_TEXT: (u8, u8, u8) = (240, 245, 255); // Crisp White
pub const COLOR_TEXT_DIM: (u8, u8, u8) = (100, 110, 140); // Dim Slate
pub const COLOR_PANEL_BG: (u8, u8, u8) = (8, 10, 18); // Deep Space Navy
pub const COLOR_BORDER: (u8, u8, u8) = (25, 35, 60); // Subtle Frame Border
pub const COLOR_GREEN: (u8, u8, u8) = (0, 255, 136); // Neon Green
pub const COLOR_RED: (u8, u8, u8) = (255, 51, 102); // Neon Red
pub const COLOR_GOLD: (u8, u8, u8) = (255, 180, 0); // Pure Gold

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub fn min_x(&self) -> i32 {
        self.x
    }

    pub fn max_x(&self) -> i32 {
        self.x + self.w
    }

    pub fn min_y(&self) -> i32 {
        self.y
    }

    pub fn max_y(&self) -> i32 {
        self.y + self.h
    }

    pub fn inner_min_x(&self) -> i32 {
        self.x + 1
    }

    pub fn inner_max_x(&self) -> i32 {
        self.x + self.w - 1
    }

    pub fn inner_min_y(&self) -> i32 {
        self.y + 1
    }

    pub fn inner_max_y(&self) -> i32 {
        self.y + self.h - 1
    }
}

pub fn draw_pixel_clipped(
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

pub fn draw_line_clipped(
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

pub fn fill_rect_clipped(
    matrix: &mut dyn MatrixBackend,
    rect: &Rect,
    clip_min_x: i32,
    clip_max_x: i32,
    clip_min_y: i32,
    clip_max_y: i32,
    color: (u8, u8, u8),
) {
    let x_start = rect.x.max(clip_min_x);
    let x_end = (rect.x + rect.w).min(clip_max_x);
    let y_start = rect.y.max(clip_min_y);
    let y_end = (rect.y + rect.h).min(clip_max_y);
    for py in y_start..y_end {
        for px in x_start..x_end {
            matrix.set_pixel(px, py, color.0, color.1, color.2);
        }
    }
}

pub fn draw_rect_clipped(
    matrix: &mut dyn MatrixBackend,
    rect: &Rect,
    clip_min_x: i32,
    clip_max_x: i32,
    clip_min_y: i32,
    clip_max_y: i32,
    color: (u8, u8, u8),
) {
    if rect.w <= 0 || rect.h <= 0 {
        return;
    }
    for px in rect.x..rect.x + rect.w {
        draw_pixel_clipped(
            matrix, px, rect.y, clip_min_x, clip_max_x, clip_min_y, clip_max_y, color,
        );
        draw_pixel_clipped(
            matrix,
            px,
            rect.y + rect.h - 1,
            clip_min_x,
            clip_max_x,
            clip_min_y,
            clip_max_y,
            color,
        );
    }
    for py in rect.y..rect.y + rect.h {
        draw_pixel_clipped(
            matrix, rect.x, py, clip_min_x, clip_max_x, clip_min_y, clip_max_y, color,
        );
        draw_pixel_clipped(
            matrix,
            rect.x + rect.w - 1,
            py,
            clip_min_x,
            clip_max_x,
            clip_min_y,
            clip_max_y,
            color,
        );
    }
}

use crate::core::matrix::MatrixBackend;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
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
        (self.x + self.w - 1).max(self.x)
    }
    pub fn inner_min_y(&self) -> i32 {
        self.y + 1
    }
    pub fn inner_max_y(&self) -> i32 {
        (self.y + self.h - 1).max(self.y)
    }
}

// ============================================================================
// Dashboard Theme Palettes (100% ESP32 Alignment)
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct DashboardTheme {
    pub primary: (u8, u8, u8),
    pub secondary: (u8, u8, u8),
    pub accent: (u8, u8, u8),
    pub panel_bg: (u8, u8, u8),
    pub text: (u8, u8, u8),
    pub text_dim: (u8, u8, u8),
    pub border: (u8, u8, u8),
    pub green: (u8, u8, u8),
    pub red: (u8, u8, u8),
}

pub fn get_dashboard_theme(theme_id: i32) -> DashboardTheme {
    match theme_id {
        1 => {
            // Theme 1: Amber / Retro HUD
            DashboardTheme {
                primary: (255, 180, 0),
                secondary: (255, 220, 100),
                accent: (255, 90, 0),
                panel_bg: (20, 15, 5),
                text: (255, 240, 200),
                text_dim: (120, 80, 20),
                border: (80, 50, 10),
                green: (50, 220, 50),
                red: (255, 60, 60),
            }
        }
        2 => {
            // Theme 2: Minimalist Luxury / Ice Blue
            DashboardTheme {
                primary: (255, 255, 255),
                secondary: (180, 220, 255),
                accent: (0, 180, 255),
                panel_bg: (5, 10, 20),
                text: (255, 255, 255),
                text_dim: (100, 120, 150),
                border: (40, 60, 90),
                green: (0, 230, 100),
                red: (255, 50, 70),
            }
        }
        3 => {
            // Theme 3: Matrix Phosphor Green
            DashboardTheme {
                primary: (0, 255, 70),
                secondary: (0, 200, 50),
                accent: (180, 255, 180),
                panel_bg: (0, 20, 5),
                text: (0, 240, 60),
                text_dim: (0, 90, 20),
                border: (0, 60, 15),
                green: (0, 255, 70),
                red: (255, 60, 60),
            }
        }
        _ => {
            // Theme 0: Cyberpunk Neon (Default)
            DashboardTheme {
                primary: (0, 230, 255),
                secondary: (255, 0, 140),
                accent: (0, 255, 120),
                panel_bg: (10, 5, 20),
                text: (255, 255, 255),
                text_dim: (90, 70, 120),
                border: (60, 20, 80),
                green: (0, 255, 120),
                red: (255, 40, 80),
            }
        }
    }
}

// ============================================================================
// Clipped Primitives
// ============================================================================

#[inline(always)]
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
        if x >= 0 && x < matrix.width() as i32 && y >= 0 && y < matrix.height() as i32 {
            matrix.set_pixel(x, y, color.0, color.1, color.2);
        }
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
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
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
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
    color: (u8, u8, u8),
) {
    let rx0 = rect.x.max(min_x);
    let rx1 = (rect.x + rect.w).min(max_x);
    let ry0 = rect.y.max(min_y);
    let ry1 = (rect.y + rect.h).min(max_y);

    for py in ry0..ry1 {
        for px in rx0..rx1 {
            if px >= 0 && px < matrix.width() as i32 && py >= 0 && py < matrix.height() as i32 {
                matrix.set_pixel(px, py, color.0, color.1, color.2);
            }
        }
    }
}

pub fn draw_rect_clipped(
    matrix: &mut dyn MatrixBackend,
    rect: &Rect,
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
    color: (u8, u8, u8),
) {
    let x0 = rect.x;
    let x1 = rect.x + rect.w - 1;
    let y0 = rect.y;
    let y1 = rect.y + rect.h - 1;

    for x in x0..=x1 {
        draw_pixel_clipped(matrix, x, y0, min_x, max_x, min_y, max_y, color);
        draw_pixel_clipped(matrix, x, y1, min_x, max_x, min_y, max_y, color);
    }
    for y in y0..=y1 {
        draw_pixel_clipped(matrix, x0, y, min_x, max_x, min_y, max_y, color);
        draw_pixel_clipped(matrix, x1, y, min_x, max_x, min_y, max_y, color);
    }
}

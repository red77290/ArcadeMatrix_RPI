use super::font::{draw_text_clipped, measure_text};
use super::geometry::*;
use crate::core::matrix::MatrixBackend;
use chrono::{DateTime, Local, Timelike};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockMode {
    Digital = 0,
    WatchDial = 1,
    Minimal = 2,
}

pub fn render_analog_watch_dial(
    matrix: &mut dyn MatrixBackend,
    rect: &Rect,
    now: &DateTime<Local>,
    sub_second: f32,
    show_seconds: bool,
) {
    if rect.w < 14 || rect.h < 14 {
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

    let cx = rect.x + rect.w / 2;
    let cy = rect.y + rect.h / 2;
    let mut radius = (rect.w.min(rect.h) / 2) - 1;
    if radius < 6 {
        radius = 6;
    }

    let min_x = rect.min_x();
    let max_x = rect.max_x();
    let min_y = rect.min_y();
    let max_y = rect.max_y();

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
        COLOR_BORDER,
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
        COLOR_ACCENT,
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
        COLOR_ACCENT,
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
        COLOR_BORDER,
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
        COLOR_BORDER,
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
        COLOR_PANEL_BG,
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
        COLOR_PANEL_BG,
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
        COLOR_BORDER,
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
            COLOR_PRIMARY
        } else {
            COLOR_TEXT_DIM
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
        matrix, cx, cy, hx, hy, min_x, max_x, min_y, max_y, COLOR_TEXT,
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
        COLOR_TEXT,
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
        COLOR_SECONDARY,
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
            matrix, cx, cy, sx, sy, min_x, max_x, min_y, max_y, COLOR_RED,
        );
    }

    // 6. Center Jewel Pivot Dot
    draw_pixel_clipped(matrix, cx, cy, min_x, max_x, min_y, max_y, COLOR_PRIMARY);
}

pub fn render_digital_clock(
    matrix: &mut dyn MatrixBackend,
    rect: &Rect,
    now: &DateTime<Local>,
    show_seconds: bool,
    show_date: bool,
) {
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

    let time_str = if show_seconds && rect.w >= 54 {
        now.format("%H:%M:%S").to_string()
    } else {
        now.format("%H:%M").to_string()
    };

    let tw = measure_text(&time_str);
    let tx = rect.x + (rect.w - tw) / 2;
    let ty = if show_date && rect.h >= 24 {
        rect.y + 4
    } else {
        rect.y + (rect.h - 7) / 2
    };

    draw_text_clipped(
        matrix,
        &time_str,
        tx,
        ty,
        rect.inner_min_x(),
        rect.inner_max_x(),
        rect.inner_min_y(),
        rect.inner_max_y(),
        COLOR_PRIMARY,
    );

    if show_date && rect.h >= 24 {
        let date_str = now.format("%d/%m").to_string();
        let dw = measure_text(&date_str);
        let dx = rect.x + (rect.w - dw) / 2;
        draw_text_clipped(
            matrix,
            &date_str,
            dx,
            rect.y + 16,
            rect.inner_min_x(),
            rect.inner_max_x(),
            rect.inner_min_y(),
            rect.inner_max_y(),
            COLOR_TEXT,
        );
    }
}

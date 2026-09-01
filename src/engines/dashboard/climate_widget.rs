use super::font::draw_text_clipped;
use super::geometry::*;
use crate::core::matrix::MatrixBackend;

pub fn draw_mini_weather_icon(
    matrix: &mut dyn MatrixBackend,
    x: i32,
    y: i32,
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
    weather_code: i32,
) {
    let sun_col = COLOR_ACCENT;
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

pub fn render_climate_slot(
    matrix: &mut dyn MatrixBackend,
    rect: &Rect,
    temp_c: f32,
    weather_code: i32,
) {
    if rect.w < 8 || rect.h < 8 {
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

    let icon_y = rect.y + (rect.h - 8) / 2;
    draw_mini_weather_icon(
        matrix,
        rect.x + 2,
        icon_y,
        rect.inner_min_x(),
        rect.inner_max_x(),
        rect.inner_min_y(),
        rect.inner_max_y(),
        weather_code,
    );

    let t_str = format!("{:.0}°", temp_c);
    let text_y = rect.y + (rect.h - 7) / 2;
    draw_text_clipped(
        matrix,
        &t_str,
        rect.x + 11,
        text_y,
        rect.inner_min_x(),
        rect.inner_max_x(),
        rect.inner_min_y(),
        rect.inner_max_y(),
        COLOR_ACCENT,
    );
}

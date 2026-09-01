use super::font::draw_text_clipped;
use super::geometry::*;
use crate::core::matrix::MatrixBackend;

pub fn render_sysinfo_slot(
    matrix: &mut dyn MatrixBackend,
    rect: &Rect,
    ram_usage: f32,
    wifi_rssi: i32,
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

    let ram_str = format!("{:.0}%", ram_usage);
    let text_y = rect.y + (rect.h - 7) / 2;
    draw_text_clipped(
        matrix,
        &ram_str,
        rect.x + 2,
        text_y,
        rect.inner_min_x(),
        rect.inner_max_x(),
        rect.inner_min_y(),
        rect.inner_max_y(),
        COLOR_TEXT_DIM,
    );

    // 4-bar WiFi signal meter
    let wx = rect.x + rect.w - 13;
    let wy = rect.y + rect.h - 3;
    let rssi_bars = if wifi_rssi > -60 {
        4
    } else if wifi_rssi > -70 {
        3
    } else {
        2
    };

    for b in 0..4 {
        let bh = (b + 1) * 2;
        let col = if b < rssi_bars {
            COLOR_ACCENT
        } else {
            COLOR_BORDER
        };
        for h_bar in 0..bh {
            draw_pixel_clipped(
                matrix,
                wx + b * 3,
                wy - h_bar,
                rect.inner_min_x(),
                rect.inner_max_x(),
                rect.inner_min_y(),
                rect.inner_max_y(),
                col,
            );
            draw_pixel_clipped(
                matrix,
                wx + b * 3 + 1,
                wy - h_bar,
                rect.inner_min_x(),
                rect.inner_max_x(),
                rect.inner_min_y(),
                rect.inner_max_y(),
                col,
            );
        }
    }
}

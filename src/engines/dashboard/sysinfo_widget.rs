use super::font::draw_text_clipped;
use super::geometry::*;
use crate::core::matrix::MatrixBackend;

pub fn render_sysinfo_slot(
    matrix: &mut dyn MatrixBackend,
    rect: &Rect,
    cpu_usage: f32,
    ram_usage: f32,
    wifi_rssi: i32,
    now_second: u32,
    theme: &DashboardTheme,
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

    let text_y = rect.y + (rect.h - 7) / 2;
    let cycle = (now_second / 3) % 2;

    // Draw 4-bar WiFi signal meter on the right side
    let wx = rect.x + rect.w - 11;
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
            theme.accent
        } else {
            theme.border
        };
        for h_bar in 0..bh {
            draw_pixel_clipped(
                matrix,
                wx + b * 2 + 1,
                wy - h_bar,
                rect.inner_min_x(),
                rect.inner_max_x(),
                rect.inner_min_y(),
                rect.inner_max_y(),
                col,
            );
        }
    }

    // Available width for text on the left of WiFi meter
    let avail_w = rect.w - 13;

    if cycle == 0 {
        // CPU Load
        let cpu_val = format!("{:.0}%", cpu_usage.clamp(0.0, 99.0));
        let cpu_str = if avail_w >= 36 {
            format!("CPU:{}", cpu_val)
        } else if avail_w >= 28 {
            format!("C:{}", cpu_val)
        } else {
            cpu_val
        };
        draw_text_clipped(
            matrix,
            &cpu_str,
            rect.x + 2,
            text_y,
            rect.inner_min_x(),
            rect.x + avail_w,
            rect.inner_min_y(),
            rect.inner_max_y(),
            theme.primary,
        );
    } else {
        // RAM Usage
        let ram_val = format!("{:.0}%", ram_usage.clamp(0.0, 99.0));
        let ram_str = if avail_w >= 36 {
            format!("RAM:{}", ram_val)
        } else if avail_w >= 28 {
            format!("R:{}", ram_val)
        } else {
            ram_val
        };
        draw_text_clipped(
            matrix,
            &ram_str,
            rect.x + 2,
            text_y,
            rect.inner_min_x(),
            rect.x + avail_w,
            rect.inner_min_y(),
            rect.inner_max_y(),
            theme.text_dim,
        );
    }
}

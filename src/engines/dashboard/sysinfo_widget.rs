use super::font::draw_text_clipped;
use super::geometry::*;
use crate::core::matrix::MatrixBackend;

#[inline]
fn get_gauge_color(ratio: f32) -> (u8, u8, u8) {
    if ratio < 0.20 {
        (0, 180, 255) // Bleu (0-20%)
    } else if ratio < 0.40 {
        (0, 230, 80) // Vert (20-40%)
    } else if ratio < 0.60 {
        (255, 215, 0) // Jaune (40-60%)
    } else if ratio < 0.80 {
        (255, 130, 0) // Orange (60-80%)
    } else {
        (255, 45, 45) // Rouge (80-100%)
    }
}

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

    // Draw 4-bar WiFi signal meter on the right side
    let wx = rect.x + rect.w - 10;
    let wy = rect.y + rect.h - 3;
    let rssi_bars = if wifi_rssi > -60 {
        4
    } else if wifi_rssi > -70 {
        3
    } else if wifi_rssi > -80 {
        2
    } else {
        1
    };

    for b in 0..4 {
        let bh = (b + 1) * 2;
        let col = if b < rssi_bars {
            theme.accent
        } else {
            (40, 45, 60)
        };
        for h_bar in 0..bh {
            draw_pixel_clipped(
                matrix,
                wx + b * 2,
                wy - h_bar,
                rect.inner_min_x(),
                rect.inner_max_x(),
                rect.inner_min_y(),
                rect.inner_max_y(),
                col,
            );
        }
    }

    // Available width on the left of the WiFi meter
    let avail_w = rect.w - 12;
    let cycle = (now_second / 3) % 2;

    let (label, usage_val) = if cycle == 0 {
        ("CPU", cpu_usage.clamp(0.0, 100.0))
    } else {
        ("RAM", ram_usage.clamp(0.0, 100.0))
    };

    if rect.h >= 13 {
        // Label on top row
        draw_text_clipped(
            matrix,
            label,
            rect.x + 2,
            rect.y + 2,
            rect.inner_min_x(),
            rect.x + avail_w,
            rect.inner_min_y(),
            rect.inner_max_y(),
            theme.primary,
        );

        // Multi-colored gauge bar across the available width
        let bar_x = rect.x + 2;
        let bar_w = (avail_w - 3).max(4);
        let bar_y = rect.y + rect.h - 4;
        let bar_h = if rect.h >= 24 { 3 } else { 2 };
        let usage_ratio = usage_val / 100.0;

        for px in 0..bar_w {
            let col_ratio = (px as f32 + 0.5) / bar_w as f32;
            let col = if col_ratio <= usage_ratio {
                get_gauge_color(col_ratio)
            } else {
                (25, 30, 45) // Subtle dark track for unfilled portion
            };

            for py in 0..bar_h {
                draw_pixel_clipped(
                    matrix,
                    bar_x + px,
                    bar_y + py,
                    rect.inner_min_x(),
                    rect.inner_max_x(),
                    rect.inner_min_y(),
                    rect.inner_max_y(),
                    col,
                );
            }
        }
    } else {
        // Very small vertical space fallback
        let text_y = rect.y + (rect.h - 7) / 2;
        draw_text_clipped(
            matrix,
            label,
            rect.x + 2,
            text_y,
            rect.inner_min_x(),
            rect.x + avail_w,
            rect.inner_min_y(),
            rect.inner_max_y(),
            theme.primary,
        );
    }
}

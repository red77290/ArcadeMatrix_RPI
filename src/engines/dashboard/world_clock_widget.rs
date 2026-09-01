use super::data::WorldTimeQuote;
use super::font::{draw_text_clipped, measure_text};
use super::geometry::*;
use crate::core::matrix::MatrixBackend;
use chrono::{DateTime, Duration, Local, Timelike, Utc};

pub fn render_world_clock_slot(
    matrix: &mut dyn MatrixBackend,
    rect: &Rect,
    world_times: &[WorldTimeQuote],
    now: &DateTime<Local>,
    utc: &DateTime<Utc>,
) {
    if world_times.is_empty() || rect.w < 8 || rect.h < 8 {
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

    let idx = (now.second() as usize / 3) % world_times.len();
    let wt = &world_times[idx];
    let wt_time = *utc + Duration::hours(wt.offset_hours as i64);
    let wt_str = format!("{}", wt_time.format("%H:%M"));

    let label_w = measure_text(&wt.code);
    let time_w = measure_text(&wt_str);
    let text_y = rect.y + (rect.h - 7) / 2;

    if rect.w >= (label_w + time_w + 5) {
        draw_text_clipped(
            matrix,
            &wt.code,
            rect.x + 2,
            text_y,
            rect.inner_min_x(),
            rect.inner_max_x(),
            rect.inner_min_y(),
            rect.inner_max_y(),
            COLOR_SECONDARY,
        );
        draw_text_clipped(
            matrix,
            &wt_str,
            rect.x + rect.w - time_w - 2,
            text_y,
            rect.inner_min_x(),
            rect.inner_max_x(),
            rect.inner_min_y(),
            rect.inner_max_y(),
            COLOR_PRIMARY,
        );
    } else {
        // Cycle between code and time on smaller slots
        let show_code = (now.second() % 4) < 2;
        if show_code {
            let x_cen = rect.x + (rect.w - label_w) / 2;
            draw_text_clipped(
                matrix,
                &wt.code,
                x_cen,
                text_y,
                rect.inner_min_x(),
                rect.inner_max_x(),
                rect.inner_min_y(),
                rect.inner_max_y(),
                COLOR_SECONDARY,
            );
        } else {
            let x_cen = rect.x + (rect.w - time_w) / 2;
            draw_text_clipped(
                matrix,
                &wt_str,
                x_cen,
                text_y,
                rect.inner_min_x(),
                rect.inner_max_x(),
                rect.inner_min_y(),
                rect.inner_max_y(),
                COLOR_PRIMARY,
            );
        }
    }
}

use crate::api::PriceHistory;
use crate::core::matrix::MatrixBackend;

pub fn draw_sparkline(
    matrix: &mut dyn MatrixBackend,
    history: &PriceHistory,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    line_color: (u8, u8, u8),
    fill_color: Option<(u8, u8, u8)>,
) {
    if history.points.is_empty() || w < 2 || h < 2 {
        return;
    }

    let min = history.min;
    let max = history.max;
    let range = (max - min).max(0.00001);
    let n = history.points.len();

    let mut prev_x: Option<i32> = None;
    let mut prev_y: Option<i32> = None;

    for i in 0..n {
        let p = history.points[i];
        let cur_x = x + (i as f64 / (n - 1).max(1) as f64 * (w - 1) as f64).round() as i32;
        let norm = ((p - min) / range).clamp(0.0, 1.0);
        let cur_y = (y + h as i32 - 1) - (norm * (h - 1) as f64).round() as i32;

        if let Some(fc) = fill_color {
            let bottom_y = y + h as i32 - 1;
            for fill_y in cur_y..=bottom_y {
                matrix.set_pixel(cur_x, fill_y, fc.0, fc.1, fc.2);
            }
        }

        if let (Some(px), Some(py)) = (prev_x, prev_y) {
            draw_line(matrix, px, py, cur_x, cur_y, line_color);
        } else {
            matrix.set_pixel(cur_x, cur_y, line_color.0, line_color.1, line_color.2);
        }

        prev_x = Some(cur_x);
        prev_y = Some(cur_y);
    }
}

fn draw_line(
    matrix: &mut dyn MatrixBackend,
    mut x0: i32,
    mut y0: i32,
    x1: i32,
    y1: i32,
    color: (u8, u8, u8),
) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        matrix.set_pixel(x0, y0, color.0, color.1, color.2);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::matrix::MockMatrix;

    #[test]
    fn test_draw_sparkline() {
        let mut matrix = MockMatrix::new(64, 32);
        let raw = vec![100.0, 150.0, 120.0, 200.0];
        let history = PriceHistory::from_raw(&raw).unwrap();

        draw_sparkline(
            &mut matrix,
            &history,
            0,
            0,
            64,
            32,
            (0, 255, 0),
            Some((0, 50, 0)),
        );

        // Verify some pixels were drawn on canvas
        let mut drawn_count = 0;
        for p in matrix.canvas.pixels() {
            if p[0] > 0 || p[1] > 0 || p[2] > 0 {
                drawn_count += 1;
            }
        }
        assert!(drawn_count > 0);
    }
}

use crate::core::matrix::MatrixBackend;

/// Draw a fast horizontal line.
pub fn draw_fast_hline(
    matrix: &mut dyn MatrixBackend,
    x: i32,
    y: i32,
    w: i32,
    color: (u8, u8, u8),
) {
    if y < 0 || y >= matrix.height() as i32 || w <= 0 {
        return;
    }
    let x_start = x.max(0);
    let x_end = (x + w).min(matrix.width() as i32);
    for px in x_start..x_end {
        matrix.set_pixel(px, y, color.0, color.1, color.2);
    }
}

/// Draw a fast vertical line.
pub fn draw_fast_vline(
    matrix: &mut dyn MatrixBackend,
    x: i32,
    y: i32,
    h: i32,
    color: (u8, u8, u8),
) {
    if x < 0 || x >= matrix.width() as i32 || h <= 0 {
        return;
    }
    let y_start = y.max(0);
    let y_end = (y + h).min(matrix.height() as i32);
    for py in y_start..y_end {
        matrix.set_pixel(x, py, color.0, color.1, color.2);
    }
}

/// Fill a rounded rectangle with radius `r`.
pub fn fill_round_rect(
    matrix: &mut dyn MatrixBackend,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    r: i32,
    color: (u8, u8, u8),
) {
    if w <= 0 || h <= 0 {
        return;
    }
    let radius = r.min(w / 2).min(h / 2);
    if radius <= 0 {
        for dy in 0..h {
            draw_fast_hline(matrix, x, y + dy, w, color);
        }
        return;
    }

    for dy in 0..h {
        let py = y + dy;
        let actual_inset = if radius == 2 {
            if dy == 0 || dy == h - 1 {
                1
            } else {
                0
            }
        } else if radius == 1 {
            0
        } else {
            let dist = if dy < radius {
                radius - 1 - dy
            } else if dy >= h - radius {
                dy - (h - radius)
            } else {
                0
            };
            let r_f = radius as f32;
            let d_f = dist as f32;
            (r_f - (r_f * r_f - d_f * d_f).max(0.0).sqrt()).round() as i32
        };
        draw_fast_hline(matrix, x + actual_inset, py, w - 2 * actual_inset, color);
    }
}

/// Draw a rounded rectangle outline with radius `r`.
pub fn draw_round_rect(
    matrix: &mut dyn MatrixBackend,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    r: i32,
    color: (u8, u8, u8),
) {
    if w <= 0 || h <= 0 {
        return;
    }
    let radius = r.min(w / 2).min(h / 2);
    if radius <= 0 {
        draw_fast_hline(matrix, x, y, w, color);
        draw_fast_hline(matrix, x, y + h - 1, w, color);
        draw_fast_vline(matrix, x, y, h, color);
        draw_fast_vline(matrix, x + w - 1, y, h, color);
        return;
    }

    // Top & Bottom lines
    draw_fast_hline(matrix, x + radius, y, w - 2 * radius, color);
    draw_fast_hline(matrix, x + radius, y + h - 1, w - 2 * radius, color);
    // Left & Right lines
    draw_fast_vline(matrix, x, y + radius, h - 2 * radius, color);
    draw_fast_vline(matrix, x + w - 1, y + radius, h - 2 * radius, color);

    // Corner pixels
    if radius == 2 {
        matrix.set_pixel(x + 1, y + 1, color.0, color.1, color.2);
        matrix.set_pixel(x + w - 2, y + 1, color.0, color.1, color.2);
        matrix.set_pixel(x + 1, y + h - 2, color.0, color.1, color.2);
        matrix.set_pixel(x + w - 2, y + h - 2, color.0, color.1, color.2);
    } else {
        for i in 1..radius {
            matrix.set_pixel(x + i, y + radius - i, color.0, color.1, color.2);
            matrix.set_pixel(x + w - 1 - i, y + radius - i, color.0, color.1, color.2);
            matrix.set_pixel(x + i, y + h - 1 - radius + i, color.0, color.1, color.2);
            matrix.set_pixel(
                x + w - 1 - i,
                y + h - 1 - radius + i,
                color.0,
                color.1,
                color.2,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::matrix::MockMatrix;

    #[test]
    fn test_draw_fast_hline_and_vline() {
        let mut matrix = MockMatrix::new(32, 32);
        draw_fast_hline(&mut matrix, 5, 10, 10, (255, 255, 255));
        assert_eq!(matrix.canvas.get_pixel(4, 10).0, [0, 0, 0]);
        assert_eq!(matrix.canvas.get_pixel(5, 10).0, [255, 255, 255]);
        assert_eq!(matrix.canvas.get_pixel(14, 10).0, [255, 255, 255]);
        assert_eq!(matrix.canvas.get_pixel(15, 10).0, [0, 0, 0]);

        draw_fast_vline(&mut matrix, 10, 5, 10, (0, 255, 0));
        assert_eq!(matrix.canvas.get_pixel(10, 4).0, [0, 0, 0]);
        assert_eq!(matrix.canvas.get_pixel(10, 5).0, [0, 255, 0]);
        assert_eq!(matrix.canvas.get_pixel(10, 14).0, [0, 255, 0]);
        assert_eq!(matrix.canvas.get_pixel(10, 15).0, [0, 0, 0]);
    }

    #[test]
    fn test_round_rect() {
        let mut matrix = MockMatrix::new(32, 32);
        fill_round_rect(&mut matrix, 2, 2, 20, 10, 2, (10, 20, 30));
        draw_round_rect(&mut matrix, 2, 2, 20, 10, 2, (100, 200, 250));
        // Outer corner pixel (2, 2) should remain blank
        assert_eq!(matrix.canvas.get_pixel(2, 2).0, [0, 0, 0]);
        // Corner pixel (3, 3) is outline
        assert_eq!(matrix.canvas.get_pixel(3, 3).0, [100, 200, 250]);
        // Center pixel (10, 5) is fill
        assert_eq!(matrix.canvas.get_pixel(10, 5).0, [10, 20, 30]);
    }
}

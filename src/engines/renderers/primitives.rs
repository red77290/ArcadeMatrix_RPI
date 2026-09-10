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

/// Draw text with horizontal marquee scrolling if its width exceeds the bounding box `[min_x, max_x)`.
///
/// If the text fits within `[min_x, max_x)`, it is drawn statically (optionally centered).
/// If it overflows, it scrolls continuously in marquee wrap-around mode based on `elapsed_ms`.
///
/// Returns `true` if the text is overflowing and actively scrolling, `false` otherwise.
pub fn draw_scrolling_text(
    matrix: &mut dyn MatrixBackend,
    text: &str,
    min_x: i32,
    max_x: i32,
    y: i32,
    scale: i32,
    color: (u8, u8, u8),
    elapsed_ms: u128,
    center_if_fits: bool,
) -> bool {
    let box_w = max_x - min_x;
    if box_w <= 0 || text.is_empty() {
        return false;
    }

    let char_count = text.chars().count() as i32;
    let s = scale.max(1);
    let text_w = char_count * 6 * s - s;

    let max_y = matrix.height() as i32;

    if text_w <= box_w {
        let draw_x = if center_if_fits {
            min_x + (box_w - text_w) / 2
        } else {
            min_x
        };
        crate::engines::dashboard::font::draw_text_scaled(
            matrix, text, draw_x, y, min_x, max_x, 0, max_y, s, color,
        );
        false
    } else {
        let gap = if s > 1 { 20 } else { 14 };
        let total_w = text_w + gap;
        let pause_ms: u128 = 1000;
        let scroll_ms = elapsed_ms.saturating_sub(pause_ms);
        let speed_px_per_sec: u128 = 20;
        let offset = ((scroll_ms * speed_px_per_sec) / 1000) as i32;
        let dx = offset.rem_euclid(total_w);

        let draw_x1 = min_x - dx;
        crate::engines::dashboard::font::draw_text_scaled(
            matrix, text, draw_x1, y, min_x, max_x, 0, max_y, s, color,
        );

        let draw_x2 = draw_x1 + total_w;
        if draw_x2 < max_x {
            crate::engines::dashboard::font::draw_text_scaled(
                matrix, text, draw_x2, y, min_x, max_x, 0, max_y, s, color,
            );
        }
        true
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

    #[test]
    fn test_draw_scrolling_text_fits() {
        let mut matrix = MockMatrix::new(64, 32);
        // "BTC" = 3 chars, text_w = 3*6 - 1 = 17 px. Bounding box = 10..40 (30 px). Fits!
        let scrolling = draw_scrolling_text(
            &mut matrix,
            "BTC",
            10,
            40,
            5,
            1,
            (255, 255, 255),
            5000,
            false,
        );
        assert!(!scrolling);
        // Outside bounds remains blank
        assert_eq!(matrix.canvas.get_pixel(9, 5).0, [0, 0, 0]);
        assert_eq!(matrix.canvas.get_pixel(40, 5).0, [0, 0, 0]);
        // First char pixel of 'B' at x=10, y=5 should be lit
        assert_eq!(matrix.canvas.get_pixel(10, 5).0, [255, 255, 255]);
    }

    #[test]
    fn test_draw_scrolling_text_overflow_and_pause() {
        let mut matrix = MockMatrix::new(32, 32);
        // "GOOGL" = 5 chars, text_w = 5*6 - 1 = 29 px. Bounding box = 0..20 (20 px). Overflows!
        // At elapsed_ms = 500 (within 1000ms pause), dx should be 0.
        let scrolling = draw_scrolling_text(
            &mut matrix,
            "GOOGL",
            0,
            20,
            5,
            1,
            (255, 255, 255),
            500,
            false,
        );
        assert!(scrolling);
        // 'G' glyph has rounded corner; col 1 row 0 is at (1, 5), col 0 row 1 is at (0, 6)
        assert_eq!(matrix.canvas.get_pixel(1, 5).0, [255, 255, 255]);
        // Pixels >= max_x (20) must be clipped strictly
        assert_eq!(matrix.canvas.get_pixel(20, 5).0, [0, 0, 0]);
        assert_eq!(matrix.canvas.get_pixel(25, 5).0, [0, 0, 0]);

        // At elapsed_ms = 1500 (500ms after pause @ 20px/s => offset = 10 px)
        matrix.clear();
        draw_scrolling_text(
            &mut matrix,
            "GOOGL",
            0,
            20,
            5,
            1,
            (255, 255, 255),
            1500,
            false,
        );
        // Text is shifted left by 10 px, pixels >= 20 must still be blank
        assert_eq!(matrix.canvas.get_pixel(20, 5).0, [0, 0, 0]);
    }
}

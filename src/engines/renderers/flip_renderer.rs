use crate::core::matrix::MatrixBackend;

/// Split-flap display renderer.
/// Each digit panel animates with a vertical "flip" effect (shrink then expand).
pub struct FlipRenderer {
    prev_chars: Vec<char>,
    flip_frame: Vec<u8>, // 0 = static, 1-8 = animating
}

// 3×5 pixel-art digits, same as shared across clocks
const DIGIT_BITMAPS: [[u8; 15]; 10] = [
    [1, 1, 1, 1, 0, 1, 1, 0, 1, 1, 0, 1, 1, 1, 1], // 0
    [0, 1, 0, 1, 1, 0, 0, 1, 0, 0, 1, 0, 1, 1, 1], // 1
    [1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1], // 2
    [1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1], // 3
    [1, 0, 1, 1, 0, 1, 1, 1, 1, 0, 0, 1, 0, 0, 1], // 4
    [1, 1, 1, 1, 0, 0, 1, 1, 1, 0, 0, 1, 1, 1, 1], // 5
    [1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 1, 1, 1, 1], // 6
    [1, 1, 1, 0, 0, 1, 0, 0, 1, 0, 1, 0, 0, 1, 0], // 7
    [1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1], // 8
    [1, 1, 1, 1, 0, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1], // 9
];

const CHAR_BITMAPS: [[u8; 15]; 26] = [
    [0, 1, 0, 1, 0, 1, 1, 1, 1, 1, 0, 1, 1, 0, 1], // A
    [1, 1, 0, 1, 0, 1, 1, 1, 0, 1, 0, 1, 1, 1, 0], // B
    [0, 1, 1, 1, 0, 0, 1, 0, 0, 1, 0, 0, 0, 1, 1], // C
    [1, 1, 0, 1, 0, 1, 1, 0, 1, 1, 0, 1, 1, 1, 0], // D
    [1, 1, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1, 1, 1], // E
    [1, 1, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1, 0, 0], // F
    [0, 1, 1, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 1, 1], // G
    [1, 0, 1, 1, 0, 1, 1, 1, 1, 1, 0, 1, 1, 0, 1], // H
    [1, 1, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0, 1, 1, 1], // I
    [0, 0, 1, 0, 0, 1, 0, 0, 1, 1, 0, 1, 0, 1, 0], // J
    [1, 0, 1, 1, 1, 0, 1, 0, 0, 1, 1, 0, 1, 0, 1], // K
    [1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 1, 1], // L
    [1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 0, 1], // M
    [1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1], // N
    [0, 1, 0, 1, 0, 1, 1, 0, 1, 1, 0, 1, 0, 1, 0], // O
    [1, 1, 0, 1, 0, 1, 1, 1, 0, 1, 0, 0, 1, 0, 0], // P
    [0, 1, 0, 1, 0, 1, 1, 0, 1, 1, 1, 1, 0, 1, 1], // Q
    [1, 1, 0, 1, 0, 1, 1, 1, 0, 1, 1, 0, 1, 0, 1], // R
    [0, 1, 1, 1, 0, 0, 0, 1, 0, 0, 0, 1, 1, 1, 0], // S
    [1, 1, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0], // T
    [1, 0, 1, 1, 0, 1, 1, 0, 1, 1, 0, 1, 0, 1, 0], // U
    [1, 0, 1, 1, 0, 1, 1, 0, 1, 0, 1, 0, 0, 1, 0], // V
    [1, 0, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1], // W
    [1, 0, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0, 1, 0, 1], // X
    [1, 0, 1, 1, 0, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0], // Y
    [1, 1, 1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 1, 1, 1], // Z
];

// Panel dimensions in pixels
const PANEL_W: i32 = 8;
const PANEL_H: i32 = 11;
const SPACING: i32 = 2;

impl FlipRenderer {
    pub fn new() -> Self {
        Self {
            prev_chars: Vec::new(),
            flip_frame: Vec::new(),
        }
    }

    pub fn render(
        &mut self,
        matrix: &mut dyn MatrixBackend,
        text: &str,
        offset_x: i32,
        offset_y: i32,
    ) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;

        let chars: Vec<char> = text.chars().collect();

        // Ensure state vectors are sized correctly
        if self.prev_chars.len() != chars.len() {
            self.prev_chars = chars.clone();
            self.flip_frame = vec![0u8; chars.len()];
        }

        // Detect changed characters and trigger their flip animation
        for (i, (&cur, &prev)) in chars.iter().zip(self.prev_chars.iter()).enumerate() {
            if cur != prev && self.flip_frame[i] == 0 {
                self.flip_frame[i] = 1;
            }
        }

        // Compute total width
        let total_w = chars.len() as i32 * (PANEL_W + SPACING) - SPACING;
        let start_x = (w - total_w) / 2 + offset_x;
        let start_y = (h - PANEL_H) / 2 + offset_y;

        for (i, &ch) in chars.iter().enumerate() {
            let cx = start_x + i as i32 * (PANEL_W + SPACING);
            let frame = self.flip_frame[i];

            // Draw the panel background (dark card)
            for dy in 0..PANEL_H {
                for dx in 0..PANEL_W {
                    let color = if dy == PANEL_H / 2 {
                        (5, 5, 5) // Horizontal crease line
                    } else {
                        (25, 25, 25) // Card bg
                    };
                    matrix.set_pixel(cx + dx, start_y + dy, color.0, color.1, color.2);
                }
            }

            if ch == ':' || ch == '/' || ch == '.' || ch == '-' {
                // Separator dots
                matrix.set_pixel(cx + 1, start_y + PANEL_H / 3, 255, 255, 255);
                matrix.set_pixel(cx + 1, start_y + 2 * PANEL_H / 3, 255, 255, 255);
            } else if frame > 0 {
                // Animating flip: shrink/expand panel height
                let shrink = if frame <= 4 { frame } else { 8 - frame } as i32;
                let shrink_px = (shrink * PANEL_H / 8).max(0);
                let top_y = start_y + shrink_px;
                let bot_y = (start_y + PANEL_H - shrink_px - 1).max(top_y);
                for dy in top_y..=bot_y {
                    for dx in 0..PANEL_W {
                        matrix.set_pixel(cx + dx, dy, 200, 200, 200);
                    }
                }
                // Draw crease
                matrix.set_pixel(cx, start_y + PANEL_H / 2, 5, 5, 5);
                matrix.set_pixel(cx + PANEL_W - 1, start_y + PANEL_H / 2, 5, 5, 5);

                // Advance animation
                self.flip_frame[i] += 1;
                if self.flip_frame[i] > 8 {
                    self.flip_frame[i] = 0;
                    self.prev_chars[i] = ch;
                }
            } else {
                // Static panel: draw the character
                self.draw_char_on_panel(matrix, ch, cx, start_y);
            }
        }

        // Update prev for chars that aren't animating
        for (i, &ch) in chars.iter().enumerate() {
            if self.flip_frame[i] == 0 {
                self.prev_chars[i] = ch;
            }
        }
    }

    fn draw_char_on_panel(&self, matrix: &mut dyn MatrixBackend, ch: char, px: i32, py: i32) {
        let bitmap_opt: Option<&[u8; 15]> = if let Some(d) = ch.to_digit(10) {
            Some(&DIGIT_BITMAPS[d as usize])
        } else {
            let idx = ch.to_ascii_uppercase() as usize;
            if idx >= 'A' as usize && idx <= 'Z' as usize {
                Some(&CHAR_BITMAPS[idx - 'A' as usize])
            } else {
                None
            }
        };

        if let Some(bm) = bitmap_opt {
            // Center the 3×5 glyph inside the 8×11 panel
            let glyph_x = px + (PANEL_W - 3) / 2;
            let glyph_y = py + (PANEL_H - 5) / 2;
            for row in 0..5i32 {
                for col in 0..3i32 {
                    if bm[(row * 3 + col) as usize] == 1 {
                        matrix.set_pixel(glyph_x + col, glyph_y + row, 240, 240, 240);
                    }
                }
            }
        }
    }
}

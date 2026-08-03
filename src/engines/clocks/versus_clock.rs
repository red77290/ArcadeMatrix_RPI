use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::base_renderer::ArcadeFont;
use crate::engines::renderers::BaseRenderer;

pub struct VersusClock {
    anim_frame: u32,
}

impl VersusClock {
    pub fn new() -> Self {
        Self { anim_frame: 0 }
    }

    pub fn render(
        &mut self,
        matrix: &mut dyn MatrixBackend,
        hours: u32,
        minutes: u32,
        font: &ArcadeFont<'_>,
        scale: u32,
    ) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;
        self.anim_frame += 1;

        let bar_w = w / 2 - 10;

        // Health percentages
        let p1_hp = 1.0 - (hours as f32 / 23.0).min(1.0);
        let p2_hp = 1.0 - (minutes as f32 / 59.0).min(1.0);

        // P1 Health Bar (left side, drains left-to-right)
        let p1_bar_len = (bar_w as f32 * p1_hp) as i32;
        // Background
        for x in 5..=(5 + bar_w) {
            for y in 2..=6 {
                matrix.set_pixel(x, y, 50, 0, 0);
            }
        }
        // Border
        matrix.set_pixel(5, 2, 180, 180, 180);
        matrix.set_pixel(5 + bar_w, 2, 180, 180, 180);
        matrix.set_pixel(5, 6, 180, 180, 180);
        matrix.set_pixel(5 + bar_w, 6, 180, 180, 180);
        // Fill (right-aligned, drains from left)
        if p1_bar_len > 0 {
            let c1: (u8, u8, u8) = if p1_hp > 0.3 {
                (255, 220, 0)
            } else {
                (255, 40, 40)
            };
            let x_start = 5 + bar_w - p1_bar_len + 1;
            for x in x_start..=(5 + bar_w - 1) {
                for y in 3..=5 {
                    matrix.set_pixel(x, y, c1.0, c1.1, c1.2);
                }
            }
        }

        // P2 Health Bar (right side, mirror)
        let p2_bar_len = (bar_w as f32 * p2_hp) as i32;
        let x2_start = w - 5 - bar_w;
        for x in x2_start..=(w - 5) {
            for y in 2..=6 {
                matrix.set_pixel(x, y, 50, 0, 0);
            }
        }
        matrix.set_pixel(x2_start, 2, 180, 180, 180);
        matrix.set_pixel(w - 5, 2, 180, 180, 180);
        matrix.set_pixel(x2_start, 6, 180, 180, 180);
        matrix.set_pixel(w - 5, 6, 180, 180, 180);
        if p2_bar_len > 0 {
            let c2: (u8, u8, u8) = if p2_hp > 0.3 {
                (255, 220, 0)
            } else {
                (255, 40, 40)
            };
            for x in x2_start..=(x2_start + p2_bar_len - 1) {
                for y in 3..=5 {
                    matrix.set_pixel(x, y, c2.0, c2.1, c2.2);
                }
            }
        }

        // "KO" blink in center
        if (self.anim_frame / 10) % 2 == 0 {
            // Draw "KO" as pixel art (6 wide, 5 tall per letter)
            self.draw_ko(matrix, w / 2 - 7, 0);
        }

        // Time display (HH:MM) centered — draw shadow then foreground
        let time_str = format!("{:02}:{:02}", hours, minutes);
        let text_w = time_str.len() as i32 * 8 * scale as i32;
        let tx = (w - text_w) / 2;
        let ty = (h - 10 * scale as i32) / 2 + 4;

        // Draw text with outline
        BaseRenderer::draw_text_at(
            matrix,
            &time_str,
            font,
            scale as f32,
            tx,
            ty,
            (255, 255, 255),
            (0, 0, 0),
        );

        // Bouncing fighter blobs at bottom corners
        let bounce1 = ((self.anim_frame as f32 * 0.2).sin() * 2.0) as i32;
        let bounce2 = ((self.anim_frame as f32 * 0.2).cos() * 2.0) as i32;

        for dy in 0..6i32 {
            for dx in 0..6i32 {
                matrix.set_pixel(10 + dx, h - 8 + bounce1 + dy, 0, 200, 255);
                matrix.set_pixel(w - 16 + dx, h - 8 + bounce2 + dy, 255, 100, 0);
            }
        }
    }

    fn draw_ko(&self, matrix: &mut dyn MatrixBackend, x: i32, y: i32) {
        // K (6×5)
        let k: [[u8; 6]; 5] = [
            [1, 0, 0, 0, 1, 0],
            [1, 0, 0, 1, 0, 0],
            [1, 1, 0, 0, 0, 0],
            [1, 0, 0, 1, 0, 0],
            [1, 0, 0, 0, 1, 0],
        ];
        // O (6×5) — offset by 7
        let o: [[u8; 6]; 5] = [
            [0, 1, 1, 1, 0, 0],
            [1, 0, 0, 0, 1, 0],
            [1, 0, 0, 0, 1, 0],
            [1, 0, 0, 0, 1, 0],
            [0, 1, 1, 1, 0, 0],
        ];
        for (row, bits) in k.iter().enumerate() {
            for (col, &bit) in bits.iter().enumerate() {
                if bit == 1 {
                    matrix.set_pixel(x + col as i32, y + row as i32, 255, 0, 0);
                }
            }
        }
        for (row, bits) in o.iter().enumerate() {
            for (col, &bit) in bits.iter().enumerate() {
                if bit == 1 {
                    matrix.set_pixel(x + 7 + col as i32, y + row as i32, 255, 0, 0);
                }
            }
        }
    }
}

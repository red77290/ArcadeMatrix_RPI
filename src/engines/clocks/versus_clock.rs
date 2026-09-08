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

        let is_tate = w < 48 || h > (w * 3) / 2;

        // Health percentages
        let p1_hp = 1.0 - (hours as f32 / 23.0).min(1.0);
        let p2_hp = 1.0 - (minutes as f32 / 59.0).min(1.0);

        // Bouncing fighter blobs at bottom corners
        let bounce1 = ((self.anim_frame as f32 * 0.2).sin() * 2.0) as i32;
        let bounce2 = ((self.anim_frame as f32 * 0.2).cos() * 2.0) as i32;

        if is_tate {
            // "KO" blink at top
            if (self.anim_frame / 10) % 2 == 0 {
                self.draw_ko(matrix, (w - 13) / 2, 1);
            }
            Self::draw_health_bar(matrix, 2, 7, w - 4, 3, p1_hp, true);
            Self::draw_health_bar(matrix, 2, 11, w - 4, 3, p2_hp, false);

            let h_str = format!("{:02}", hours);
            let m_str = format!("{:02}", minutes);
            let max_tier_h = (h / 2) - 10;
            let mut tate_scale = scale.max(1) as i32;
            while tate_scale > 1 {
                let (_, bw, bh) = font.get_pixel_map("88", tate_scale as f32);
                if bw <= w && bh <= max_tier_h {
                    break;
                }
                tate_scale -= 1;
            }

            let (_, bw, bh) = font.get_pixel_map("88", tate_scale as f32);
            let tx = (w - bw) / 2;
            let ty_h = (h / 2) - bh - 1;
            let ty_m = (h / 2) + 3;

            BaseRenderer::draw_text_at(
                matrix,
                &h_str,
                font,
                tate_scale as f32,
                tx,
                ty_h,
                (255, 255, 255),
                (0, 0, 0),
            );
            BaseRenderer::draw_text_at(
                matrix,
                &m_str,
                font,
                tate_scale as f32,
                tx,
                ty_m,
                (255, 255, 255),
                (0, 0, 0),
            );

            for dy in 0..5i32 {
                for dx in 0..5i32 {
                    matrix.set_pixel(2 + dx, h - 8 + bounce1 + dy, 0, 200, 255);
                    matrix.set_pixel(w - 7 + dx, h - 8 + bounce2 + dy, 255, 100, 0);
                }
            }
        } else {
            let bar_w = w / 2 - 10;
            Self::draw_health_bar(matrix, 5, 2, bar_w, 4, p1_hp, true);
            Self::draw_health_bar(matrix, w - 5 - bar_w, 2, bar_w, 4, p2_hp, false);

            // "KO" blink in center
            if (self.anim_frame / 10) % 2 == 0 {
                self.draw_ko(matrix, w / 2 - 7, 0);
            }

            let time_str = format!("{:02}:{:02}", hours, minutes);

            let (_, text_w, text_h) = font.get_pixel_map(&time_str, scale as f32);
            let tx = (w - text_w) / 2;
            let ty = (h - text_h) / 2 + 4;

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

            for dy in 0..6i32 {
                for dx in 0..6i32 {
                    matrix.set_pixel(10 + dx, h - 8 + bounce1 + dy, 0, 200, 255);
                    matrix.set_pixel(w - 16 + dx, h - 8 + bounce2 + dy, 255, 100, 0);
                }
            }
        }
    }

    fn draw_health_bar(
        matrix: &mut dyn MatrixBackend,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        hp: f32,
        is_player_1: bool,
    ) {
        let border = (180, 180, 180);
        let bg = (50, 0, 0);
        let fill = if hp <= 0.3 {
            (255, 40, 40)
        } else {
            (255, 220, 0)
        };

        for py in y..=(y + height) {
            for px in x..=(x + width) {
                matrix.set_pixel(px, py, bg.0, bg.1, bg.2);
            }
        }
        matrix.set_pixel(x, y, border.0, border.1, border.2);
        matrix.set_pixel(x + width, y, border.0, border.1, border.2);
        matrix.set_pixel(x, y + height, border.0, border.1, border.2);
        matrix.set_pixel(x + width, y + height, border.0, border.1, border.2);

        let fill_w = ((width as f32) * hp).round() as i32;
        if fill_w > 0 {
            if is_player_1 {
                let x_start = x + width - fill_w + 1;
                for px in x_start..=(x + width - 1) {
                    for py in (y + 1)..=(y + height - 1) {
                        matrix.set_pixel(px, py, fill.0, fill.1, fill.2);
                    }
                }
            } else {
                for px in (x + 1)..=(x + fill_w) {
                    for py in (y + 1)..=(y + height - 1) {
                        matrix.set_pixel(px, py, fill.0, fill.1, fill.2);
                    }
                }
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

use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::BaseRenderer;
use crate::engines::renderers::base_renderer::ArcadeFont;

pub struct SlotMachineClock {
    last_minute: i32,
    anim_frame: u32,
    spinning: bool,
    spin_speed: f32,
    y_offset: f32,
    current_time: String,
    target_time: String,
}

impl SlotMachineClock {
    pub fn new() -> Self {
        Self {
            last_minute: -1,
            anim_frame: 0,
            spinning: false,
            spin_speed: 0.0,
            y_offset: 0.0,
            current_time: "00:00".to_string(),
            target_time: "00:00".to_string(),
        }
    }

    pub fn render(
        &mut self,
        matrix: &mut dyn MatrixBackend,
        time_str: &str,
        font: &ArcadeFont<'_>,
        scale: u32,
    ) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;
        self.anim_frame += 1;

        let now_min: i32 = time_str
            .split(':')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        if self.last_minute == -1 {
            self.last_minute = now_min;
            self.current_time = time_str.to_string();
            self.target_time = time_str.to_string();
        } else if self.last_minute != now_min && !self.spinning {
            self.spinning = true;
            self.spin_speed = 15.0;
            self.target_time = time_str.to_string();
        } else if !self.spinning {
            self.current_time = time_str.to_string();
        }

        // Accurate text metrics from font
        let (_, tw, th) = font.get_pixel_map(&self.current_time, scale as f32);

        let tx = (w - tw) / 2;
        let ty = (h - th) / 2;

        // Draw slot machine border
        let frame_color = if self.spinning {
            (200, 150, 0)
        } else {
            (80, 80, 80)
        };
        for x in (tx - 4)..=(tx + tw + 4) {
            matrix.set_pixel(x, ty - 2, frame_color.0, frame_color.1, frame_color.2);
            matrix.set_pixel(x, ty + th + 2, frame_color.0, frame_color.1, frame_color.2);
        }
        for y in (ty - 2)..=(ty + th + 2) {
            matrix.set_pixel(tx - 4, y, frame_color.0, frame_color.1, frame_color.2);
            matrix.set_pixel(tx + tw + 4, y, frame_color.0, frame_color.1, frame_color.2);
        }

        if self.spinning {
            self.y_offset += self.spin_speed;
            self.spin_speed *= 0.95;

            if self.spin_speed < 0.5 {
                self.spinning = false;
                self.current_time = self.target_time.clone();
                self.last_minute = now_min;
                self.y_offset = 0.0;
            }

            // Draw blurred spinning text
            let blur_y = ty + (self.y_offset as i32 % (th * 2));
            BaseRenderer::draw_text_at(matrix, "88:88", font, scale as f32, tx, blur_y - th * 2, (80, 80, 80), (0, 0, 0));
            BaseRenderer::draw_text_at(matrix, "00:00", font, scale as f32, tx, blur_y, (40, 40, 40), (0, 0, 0));

            // Clip: mask overflow above and below the frame
            for x in 0..w {
                for y in 0..(ty - 2) {
                    matrix.set_pixel(x, y, 0, 0, 0);
                }
                for y in (ty + th + 3)..h {
                    matrix.set_pixel(x, y, 0, 0, 0);
                }
            }
        } else {
            // Static time display
            BaseRenderer::draw_text_at(matrix, &self.current_time.clone(), font, scale as f32, tx, ty, (255, 255, 255), (0, 0, 0));

            // Winning golden border blink
            if (self.anim_frame / 20) % 2 == 0 {
                for x in (tx - 4)..=(tx + tw + 4) {
                    matrix.set_pixel(x, ty - 2, 255, 220, 0);
                    matrix.set_pixel(x, ty + th + 2, 255, 220, 0);
                }
                for y in (ty - 2)..=(ty + th + 2) {
                    matrix.set_pixel(tx - 4, y, 255, 220, 0);
                    matrix.set_pixel(tx + tw + 4, y, 255, 220, 0);
                }
            }
        }

        // Decorative blinking LED dots on both sides
        let led_y = ty + th / 2;
        let (left_col, right_col) = if (self.anim_frame / 5) % 2 == 0 {
            ((255u8, 0u8, 0u8), (0u8, 255u8, 0u8))
        } else {
            ((0u8, 255u8, 0u8), (255u8, 0u8, 0u8))
        };
        matrix.set_pixel(tx - 8, led_y - 1, left_col.0, left_col.1, left_col.2);
        matrix.set_pixel(tx - 8, led_y, left_col.0, left_col.1, left_col.2);
        matrix.set_pixel(
            tx + tw + 8,
            led_y - 1,
            right_col.0,
            right_col.1,
            right_col.2,
        );
        matrix.set_pixel(tx + tw + 8, led_y, right_col.0, right_col.1, right_col.2);
    }
}

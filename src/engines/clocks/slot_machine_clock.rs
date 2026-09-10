use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::base_renderer::ArcadeFont;
use crate::engines::renderers::BaseRenderer;

pub struct SlotMachineClock {
    last_minute: i32,
    last_hour: i32,
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
            last_hour: -1,
            anim_frame: 0,
            spinning: false,
            spin_speed: 0.0,
            y_offset: 0.0,
            current_time: "00:00".to_string(),
            target_time: "00:00".to_string(),
        }
    }

    pub fn is_spinning(&self) -> bool {
        self.spinning
    }

    pub fn render(
        &mut self,
        matrix: &mut dyn MatrixBackend,
        time_str: &str,
        hours: u32,
        minutes: u32,
        font: &ArcadeFont<'_>,
        scale: u32,
    ) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;
        self.anim_frame += 1;

        let now_h = hours as i32;
        let now_min = minutes as i32;

        if self.last_minute == -1 {
            self.last_minute = now_min;
            self.last_hour = now_h;
            self.current_time = time_str.to_string();
            self.target_time = time_str.to_string();
        } else if (self.last_minute != now_min || self.last_hour != now_h) && !self.spinning {
            self.spinning = true;
            self.spin_speed = 15.0;
            self.target_time = time_str.to_string();
        } else if !self.spinning {
            self.current_time = time_str.to_string();
        }

        let is_tate = w < 48 || h > (w * 3) / 2;

        let frame_color = if self.spinning {
            (200, 150, 0)
        } else if (self.anim_frame / 20) % 2 == 0 {
            (255, 220, 0) // Winning golden border blink
        } else {
            (80, 80, 80)
        };

        let (left_col, right_col) = if (self.anim_frame / 5) % 2 == 0 {
            ((255u8, 0u8, 0u8), (0u8, 255u8, 0u8))
        } else {
            ((0u8, 255u8, 0u8), (255u8, 0u8, 0u8))
        };

        if self.spinning {
            self.y_offset += self.spin_speed;
            self.spin_speed *= 0.95;

            if self.spin_speed < 0.5 {
                self.spinning = false;
                self.current_time = self.target_time.clone();
                self.last_minute = now_min;
                self.last_hour = now_h;
                self.y_offset = 0.0;
            }
        }

        if is_tate {
            // Stacked Portrait Layout (HH on top, MM on bottom)
            let (h_str, m_str) = if self.current_time.contains(':') {
                let mut parts = self.current_time.split(':');
                (
                    parts.next().unwrap_or("00").to_string(),
                    parts.next().unwrap_or("00").to_string(),
                )
            } else {
                ("00".to_string(), "00".to_string())
            };

            let max_tier_h = ((h / 2) - 8).max(1);
            let mut tate_scale = scale.max(1) as i32;
            while tate_scale > 1 {
                let (_, bw, bh) = font.get_pixel_map("88", tate_scale as f32);
                if bw + 6 <= w && bh + 4 <= max_tier_h {
                    break;
                }
                tate_scale -= 1;
            }

            let (_, bw, bh) = font.get_pixel_map("88", tate_scale as f32);
            let tx = (w - bw) / 2;
            let ty_h = (h / 4) - (bh / 2);
            let ty_m = (3 * h / 4) - (bh / 2);

            // Slot Machine Reels Boxes
            Self::draw_box(matrix, tx - 3, ty_h - 2, bw + 6, bh + 4, frame_color);
            Self::draw_box(matrix, tx - 3, ty_m - 2, bw + 6, bh + 4, frame_color);

            if self.spinning {
                let th2 = bh * 2;
                let blur_y = (self.y_offset as i32) % th2;

                BaseRenderer::draw_text_at(
                    matrix,
                    "88",
                    font,
                    tate_scale as f32,
                    tx,
                    ty_h + blur_y - th2,
                    (80, 80, 80),
                    (0, 0, 0),
                );
                BaseRenderer::draw_text_at(
                    matrix,
                    "88",
                    font,
                    tate_scale as f32,
                    tx,
                    ty_m + blur_y - th2,
                    (80, 80, 80),
                    (0, 0, 0),
                );

                BaseRenderer::draw_text_at(
                    matrix,
                    "00",
                    font,
                    tate_scale as f32,
                    tx,
                    ty_h + blur_y,
                    (40, 40, 40),
                    (0, 0, 0),
                );
                BaseRenderer::draw_text_at(
                    matrix,
                    "00",
                    font,
                    tate_scale as f32,
                    tx,
                    ty_m + blur_y,
                    (40, 40, 40),
                    (0, 0, 0),
                );

                // Clip overflow
                for x in 0..w {
                    for y in 0..(ty_h - 2) {
                        matrix.set_pixel(x, y, 0, 0, 0);
                    }
                    for y in (ty_h + bh + 3)..(ty_m - 2) {
                        matrix.set_pixel(x, y, 0, 0, 0);
                    }
                    for y in (ty_m + bh + 3)..h {
                        matrix.set_pixel(x, y, 0, 0, 0);
                    }
                }
            } else {
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
            }

            // Blinking LEDs
            let led_offset_x = if w < 48 { 4 } else { 5 };
            matrix.set_pixel(
                tx - led_offset_x,
                ty_h + bh / 2,
                left_col.0,
                left_col.1,
                left_col.2,
            );
            matrix.set_pixel(
                tx + bw + led_offset_x - 1,
                ty_h + bh / 2,
                right_col.0,
                right_col.1,
                right_col.2,
            );
            matrix.set_pixel(
                tx - led_offset_x,
                ty_m + bh / 2,
                right_col.0,
                right_col.1,
                right_col.2,
            );
            matrix.set_pixel(
                tx + bw + led_offset_x - 1,
                ty_m + bh / 2,
                left_col.0,
                left_col.1,
                left_col.2,
            );
        } else {
            // Horizontal Landscape Layout
            let display_str = if self.spinning {
                &self.target_time
            } else {
                &self.current_time
            };
            let (_, tw, th) = font.get_pixel_map(display_str, scale as f32);
            let tx = (w - tw) / 2;
            let ty = (h - th) / 2;

            Self::draw_box(matrix, tx - 4, ty - 2, tw + 8, th + 4, frame_color);

            if self.spinning {
                let blur_y = ty + (self.y_offset as i32 % (th * 2));
                let (blur_top, blur_bot) = if self.target_time.len() > 5 {
                    ("88:88:88", "00:00:00")
                } else {
                    ("88:88", "00:00")
                };
                BaseRenderer::draw_text_at(
                    matrix,
                    blur_top,
                    font,
                    scale as f32,
                    tx,
                    blur_y - th * 2,
                    (80, 80, 80),
                    (0, 0, 0),
                );
                BaseRenderer::draw_text_at(
                    matrix,
                    blur_bot,
                    font,
                    scale as f32,
                    tx,
                    blur_y,
                    (40, 40, 40),
                    (0, 0, 0),
                );

                for x in 0..w {
                    for y in 0..(ty - 2) {
                        matrix.set_pixel(x, y, 0, 0, 0);
                    }
                    for y in (ty + th + 3)..h {
                        matrix.set_pixel(x, y, 0, 0, 0);
                    }
                }
            } else {
                BaseRenderer::draw_text_at(
                    matrix,
                    &self.current_time.clone(),
                    font,
                    scale as f32,
                    tx,
                    ty,
                    (255, 255, 255),
                    (0, 0, 0),
                );
            }

            // Decorative blinking LED dots on both sides
            let led_y = ty + th / 2;
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

    fn draw_box(
        matrix: &mut dyn MatrixBackend,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        color: (u8, u8, u8),
    ) {
        for px in x..=(x + w) {
            matrix.set_pixel(px, y, color.0, color.1, color.2);
            matrix.set_pixel(px, y + h, color.0, color.1, color.2);
        }
        for py in y..=(y + h) {
            matrix.set_pixel(x, py, color.0, color.1, color.2);
            matrix.set_pixel(x + w, py, color.0, color.1, color.2);
        }
    }
}

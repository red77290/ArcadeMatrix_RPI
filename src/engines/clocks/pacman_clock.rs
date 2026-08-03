use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::base_renderer::ArcadeFont;
use crate::engines::renderers::BaseRenderer;

pub struct PacmanClock {
    pac_x: f32,
    direction: f32,
    anim_frame: u32,
    last_minute: i32,
    transitioning: bool,
    old_time_str: String,
    new_time_str: String,
    speed: f32,
    radius: i32,
}

impl PacmanClock {
    pub fn new() -> Self {
        Self {
            pac_x: 0.0,
            direction: 1.0,
            anim_frame: 0,
            last_minute: -1,
            transitioning: false,
            old_time_str: String::new(),
            new_time_str: String::new(),
            speed: 2.0,
            radius: 4,
        }
    }

    pub fn render(
        &mut self,
        matrix: &mut dyn MatrixBackend,
        time_str: &str,
        _hours: u32,
        minutes: u32,
        font: &ArcadeFont<'_>,
        scale: u32,
    ) {
        let w = matrix.width() as f32;
        let h = matrix.height() as f32;
        self.anim_frame += 1;

        // Adapt radius and speed to matrix size (match Python)
        self.radius = ((6.0 * h / 32.0) as i32).max(4);
        self.speed = (3.0 * w / 64.0).max(1.5);

        let now_min = minutes as i32;

        if self.last_minute == -1 {
            self.last_minute = now_min;
            self.old_time_str = time_str.to_string();
            self.new_time_str = time_str.to_string();
        } else if self.last_minute != now_min && !self.transitioning {
            self.transitioning = true;
            self.old_time_str = self.new_time_str.clone();
            self.new_time_str = time_str.to_string();
            self.pac_x = -(self.radius as f32 * 3.0);
        } else if !self.transitioning {
            self.new_time_str = time_str.to_string();
        }

        let py = (h / 2.0) as i32;

        if !self.transitioning {
            // Static display: draw time in center + scattered pellets
            let text_w = time_str.len() as i32 * 8 * scale as i32; // approximation for centering
            let tx = (w as i32 - text_w) / 2;
            let ty = (h as i32 - 10 * scale as i32) / 2;
            BaseRenderer::draw_text_at(
                matrix,
                &self.new_time_str.clone(),
                font,
                scale as f32,
                tx,
                ty,
                (255, 255, 255),
                (0, 0, 0),
            );

            // Scattered pellets
            for i in 0..5 {
                let px = ((self.anim_frame as f32 * 0.1 + i as f32).sin() * (w / 2.0)) + (w / 2.0);
                let py = ((self.anim_frame as f32 * 0.15 + (i * 2) as f32).cos() * (h / 2.0))
                    + (h / 2.0);
                matrix.set_pixel(px as i32, py as i32, 255, 183, 174);
            }
        } else {
            // Transition animation
            self.pac_x += self.speed;

            let text_w = time_str.len() as i32 * 8 * scale as i32;
            let tx = (w as i32 - text_w) / 2;
            let ty = (h as i32 - 10 * scale as i32) / 2;

            // Draw old time (being "eaten" — visible only ahead of pac-man)
            BaseRenderer::draw_text_at(
                matrix,
                &self.old_time_str.clone(),
                font,
                scale as f32,
                tx,
                ty,
                (100, 100, 100),
                (0, 0, 0),
            );

            // Black mask over eaten portion (left of pac-man)
            for x in 0..self.pac_x as i32 {
                for y in 0..h as i32 {
                    matrix.set_pixel(x, y, 0, 0, 0);
                }
            }

            // Draw new time (revealed behind pac-man)
            let reveal_x = (self.pac_x as i32 - self.radius * 4).max(0);
            BaseRenderer::draw_text_at(
                matrix,
                &self.new_time_str.clone(),
                font,
                scale as f32,
                tx,
                ty,
                (255, 255, 255),
                (0, 0, 0),
            );
            // Black mask over unrevealed portion (right of reveal wave)
            for x in reveal_x..w as i32 {
                for y in 0..h as i32 {
                    matrix.set_pixel(x, y, 0, 0, 0);
                }
            }

            // Mouth animation
            let mouth_angle = ((self.anim_frame as f32 * 0.5).sin().abs() * 45.0) as i32;

            // Draw Pac-Man
            self.draw_pacman(matrix, self.pac_x as i32, py, self.radius, mouth_angle);

            // Draw ghosts trailing behind
            let ghost_colors: [(u8, u8, u8); 4] =
                [(255, 0, 0), (255, 184, 255), (0, 255, 255), (255, 184, 82)];
            for (i, &gc) in ghost_colors.iter().enumerate() {
                let gx = self.pac_x as i32 - (self.radius * 3) - (i as i32 * self.radius * 2);
                let gy_offset = ((self.anim_frame as f32 * 0.2 + i as f32).sin()
                    * (self.radius as f32 / 3.0)) as i32;
                self.draw_ghost(
                    matrix,
                    gx,
                    py + gy_offset,
                    self.radius - 1,
                    gc,
                    self.anim_frame,
                );
            }

            // (No extra pellets ahead of pacman to match Python exactly)

            // Check if transition is done
            if self.pac_x >= w + self.radius as f32 * 3.0 {
                self.transitioning = false;
                self.last_minute = now_min;
                self.old_time_str = self.new_time_str.clone();
            }
        }
    }

    fn draw_pacman(
        &self,
        matrix: &mut dyn MatrixBackend,
        cx: i32,
        cy: i32,
        r: i32,
        mouth_deg: i32,
    ) {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy > r * r {
                    continue;
                }
                // Simple mouth open: skip the wedge sector
                // mouth_deg is half-angle of opening in "degrees" (0..45)
                // Use pixel math: avoid pixels in front-upper and front-lower wedge
                let in_mouth = dx > 0 && dy.abs() * 45 < dx * mouth_deg;
                if !in_mouth {
                    matrix.set_pixel(cx + dx, cy + dy, 255, 255, 0);
                }
            }
        }
    }

    fn draw_ghost(
        &self,
        matrix: &mut dyn MatrixBackend,
        cx: i32,
        cy: i32,
        r: i32,
        color: (u8, u8, u8),
        tick: u32,
    ) {
        // Upper semicircle body
        for dy in -r..=0i32 {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r {
                    matrix.set_pixel(cx + dx, cy + dy, color.0, color.1, color.2);
                }
            }
        }
        // Rectangular lower body
        for dy in 0..=r {
            for dx in -r..=r {
                matrix.set_pixel(cx + dx, cy + dy, color.0, color.1, color.2);
            }
        }
        // Tentacles at bottom (alternating based on tick)
        let wave = (tick / 3) % 2 == 0;
        for i in 0..3i32 {
            let tx = cx - r + i * (r * 2 / 3) + r / 3;
            let bottom_y = cy + r;
            if (i % 2 == 0) == wave {
                matrix.set_pixel(tx, bottom_y + 1, 0, 0, 0);
            }
        }
        // White eyes
        matrix.set_pixel(cx - r / 2, cy - 1, 255, 255, 255);
        matrix.set_pixel(cx + r / 2, cy - 1, 255, 255, 255);
        // Blue pupils
        matrix.set_pixel(cx - r / 2 + 1, cy - 1, 0, 0, 200);
        matrix.set_pixel(cx + r / 2 + 1, cy - 1, 0, 0, 200);
    }
}

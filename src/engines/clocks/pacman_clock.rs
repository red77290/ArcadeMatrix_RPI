use crate::core::matrix::MatrixBackend;

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
    ) {
        let w = matrix.width() as f32;
        let h = matrix.height() as f32;
        self.anim_frame += 1;

        // Adapt radius and speed to matrix size
        self.radius = ((h / 8.0) as i32).max(3).min(6);
        self.speed = (w / 40.0).max(1.5);

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
            self.draw_time_text(matrix, &self.new_time_str.clone(), w as i32, h as i32);

            // Scattered pellets
            for i in 0..5u32 {
                let px = ((self.anim_frame.wrapping_mul(7).wrapping_add(i * 97)) % w as u32) as i32;
                let py2 =
                    ((self.anim_frame.wrapping_mul(5).wrapping_add(i * 53)) % h as u32) as i32;
                matrix.set_pixel(px, py2, 255, 183, 174);
            }
        } else {
            // Transition animation
            self.pac_x += self.speed;

            // Draw old time (being "eaten" — visible only ahead of pac-man)
            self.draw_time_text(matrix, &self.old_time_str.clone(), w as i32, h as i32);

            // Black mask over eaten portion (left of pac-man)
            for x in 0..self.pac_x as i32 {
                for y in 0..h as i32 {
                    matrix.set_pixel(x, y, 0, 0, 0);
                }
            }

            // Draw new time (revealed behind pac-man)
            let reveal_x = (self.pac_x as i32 - self.radius * 4).max(0);
            self.draw_time_text(matrix, &self.new_time_str.clone(), w as i32, h as i32);
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

            // Pellets ahead of pac-man
            let mut px_pel = self.pac_x as i32 + self.radius * 2;
            while px_pel < w as i32 {
                matrix.set_pixel(px_pel, py, 255, 184, 82);
                px_pel += 6;
            }

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

    fn draw_time_text(&self, matrix: &mut dyn MatrixBackend, time_str: &str, w: i32, h: i32) {
        let char_w = 4i32;
        let char_h = 5i32;
        let text_w = time_str.len() as i32 * char_w;
        let tx = (w - text_w) / 2;
        let ty = (h - char_h) / 2;
        Self::draw_pixels(matrix, time_str, tx, ty, (255, 255, 255));
    }

    fn draw_pixels(
        matrix: &mut dyn MatrixBackend,
        text: &str,
        x: i32,
        y: i32,
        color: (u8, u8, u8),
    ) {
        let segments: [[u8; 15]; 10] = [
            [1, 1, 1, 1, 0, 1, 1, 0, 1, 1, 0, 1, 1, 1, 1],
            [0, 1, 0, 1, 1, 0, 0, 1, 0, 0, 1, 0, 1, 1, 1],
            [1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1],
            [1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1],
            [1, 0, 1, 1, 0, 1, 1, 1, 1, 0, 0, 1, 0, 0, 1],
            [1, 1, 1, 1, 0, 0, 1, 1, 1, 0, 0, 1, 1, 1, 1],
            [1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 1, 1, 1, 1],
            [1, 1, 1, 0, 0, 1, 0, 0, 1, 0, 1, 0, 0, 1, 0],
            [1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1],
            [1, 1, 1, 1, 0, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1],
        ];
        let mut cx = x;
        for ch in text.chars() {
            if ch == ':' {
                matrix.set_pixel(cx + 1, y + 1, color.0, color.1, color.2);
                matrix.set_pixel(cx + 1, y + 3, color.0, color.1, color.2);
                cx += 3;
                continue;
            }
            if let Some(d) = ch.to_digit(10) {
                for row in 0..5i32 {
                    for col in 0..3i32 {
                        if segments[d as usize][(row * 3 + col) as usize] == 1 {
                            matrix.set_pixel(cx + col, y + row, color.0, color.1, color.2);
                        }
                    }
                }
            }
            cx += 4;
        }
    }
}

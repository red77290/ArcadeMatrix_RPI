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

        let now_min = minutes as i32;

        if self.last_minute == -1 {
            self.last_minute = now_min;
            self.old_time_str = time_str.to_string();
            self.new_time_str = time_str.to_string();
        } else if (self.last_minute != now_min || self.old_time_str != time_str)
            && !self.transitioning
        {
            self.transitioning = true;
            self.old_time_str = self.new_time_str.clone();
            self.new_time_str = time_str.to_string();
            self.pac_x = -(self.radius as f32 * 3.0);
        } else if !self.transitioning {
            self.new_time_str = time_str.to_string();
        }

        let is_tate = (w < 48.0) || (h > (w * 1.5));
        let active_scale = if is_tate {
            let max_tier_h = ((h as i32 / 2) - 10).max(1);
            let mut s = scale.max(1) as i32;
            while s > 1 {
                let (_, bw, bh) = font.get_pixel_map("88", s as f32);
                if bw <= w as i32 && bh <= max_tier_h {
                    break;
                }
                s -= 1;
            }
            s as u32
        } else {
            scale.max(1)
        };

        // Measure font height to ensure Pacman is scaled larger than the digits
        let active_str = if self.transitioning {
            &self.old_time_str
        } else {
            &self.new_time_str
        };
        let (pixels, _, _) =
            font.get_pixel_map(if is_tate { "88" } else { active_str }, active_scale as f32);
        let mut text_h = 0;
        let mut text_w = 0;
        for char_pixels in &pixels {
            for &(px, py) in char_pixels {
                text_w = text_w.max(px + 1);
                text_h = text_h.max(py + 1);
            }
        }
        let max_pac_radius = if is_tate {
            ((w as i32 / 2) - 2).min((h as i32 / 4) - 2)
        } else {
            (h as i32 / 2) - 1
        };
        let target_r = ((text_h as f32 * 0.70) as i32) + 1;
        self.radius = target_r.max(3).min(max_pac_radius);
        self.speed = (0.8 * w / 64.0).max(0.6);

        let py = (h / 2.0) as i32;

        if is_tate {
            // Stacked Portrait Layout (HH on top, MM on bottom)
            let (h_new, m_new) = if self.new_time_str.contains(':') {
                let mut parts = self.new_time_str.split(':');
                (
                    parts.next().unwrap_or("00").to_string(),
                    parts.next().unwrap_or("00").to_string(),
                )
            } else {
                ("00".to_string(), "00".to_string())
            };
            let (h_old, m_old) = if self.old_time_str.contains(':') {
                let mut parts = self.old_time_str.split(':');
                (
                    parts.next().unwrap_or("00").to_string(),
                    parts.next().unwrap_or("00").to_string(),
                )
            } else {
                ("00".to_string(), "00".to_string())
            };

            let tx = (w as i32 - text_w) / 2;
            let ty_h = (h as i32 / 4) - (text_h / 2);
            let ty_m = (3 * h as i32 / 4) - (text_h / 2);
            let dot_y = (h as i32) / 2;
            let dot_x = [w as i32 / 4, w as i32 / 2, 3 * w as i32 / 4];
            let dot_color = (255, 183, 174);

            let ghost_spacing = self.radius as f32 * 2.2;
            let leg_len = w + self.radius as f32 * 4.0 + 4.0 * ghost_spacing;
            let max_path = 3.0 * leg_len;

            if !self.transitioning {
                BaseRenderer::draw_text_at(
                    matrix,
                    &h_new,
                    font,
                    active_scale as f32,
                    tx,
                    ty_h,
                    (255, 255, 255),
                    (0, 0, 0),
                );
                BaseRenderer::draw_text_at(
                    matrix,
                    &m_new,
                    font,
                    active_scale as f32,
                    tx,
                    ty_m,
                    (255, 255, 255),
                    (0, 0, 0),
                );
                for &dx in &dot_x {
                    for oy in -1..=0 {
                        for ox in -1..=0 {
                            matrix.set_pixel(
                                dx + ox,
                                dot_y + oy,
                                dot_color.0,
                                dot_color.1,
                                dot_color.2,
                            );
                        }
                    }
                }
            } else {
                self.pac_x += self.speed;
                let mouth_angle = ((self.anim_frame as f32 * 0.5).sin().abs() * 45.0) as i32;
                let ghost_colors: [(u8, u8, u8); 4] =
                    [(255, 0, 0), (255, 184, 255), (0, 255, 255), (255, 184, 82)];

                if self.pac_x < leg_len {
                    // Tier 1: Hours line (Left -> Right)
                    let current_pac_x = -self.radius as f32 * 2.0 + self.pac_x;

                    BaseRenderer::draw_text_at(
                        matrix,
                        &h_old,
                        font,
                        active_scale as f32,
                        tx,
                        ty_h,
                        (100, 100, 100),
                        (0, 0, 0),
                    );
                    for x in 0..current_pac_x as i32 {
                        for y in (ty_h - 2)..=(ty_h + text_h + 2) {
                            matrix.set_pixel(x, y, 0, 0, 0);
                        }
                    }

                    let reveal_x = (current_pac_x as i32
                        - (self.radius * 3 + 4 * ghost_spacing as i32))
                        .max(0);
                    if reveal_x > 0 {
                        BaseRenderer::draw_text_at(
                            matrix,
                            &h_new,
                            font,
                            active_scale as f32,
                            tx,
                            ty_h,
                            (255, 255, 255),
                            (0, 0, 0),
                        );
                        for x in reveal_x..w as i32 {
                            for y in (ty_h - 2)..=(ty_h + text_h + 2) {
                                matrix.set_pixel(x, y, 0, 0, 0);
                            }
                        }
                    }

                    for &dx in &dot_x {
                        for oy in -1..=0 {
                            for ox in -1..=0 {
                                matrix.set_pixel(
                                    dx + ox,
                                    dot_y + oy,
                                    dot_color.0,
                                    dot_color.1,
                                    dot_color.2,
                                );
                            }
                        }
                    }
                    BaseRenderer::draw_text_at(
                        matrix,
                        &m_old,
                        font,
                        active_scale as f32,
                        tx,
                        ty_m,
                        (100, 100, 100),
                        (0, 0, 0),
                    );

                    self.draw_pacman(
                        matrix,
                        current_pac_x as i32,
                        ty_h + text_h / 2,
                        self.radius,
                        mouth_angle,
                        true,
                    );
                    for (i, &gc) in ghost_colors.iter().enumerate() {
                        let gx = current_pac_x as i32
                            - (self.radius * 2 + 3)
                            - (i as i32 * ghost_spacing as i32);
                        let gy = ty_h
                            + text_h / 2
                            + ((self.anim_frame as f32 * 0.4 + i as f32).sin()
                                * (self.radius as f32 / 3.0)) as i32;
                        self.draw_ghost(matrix, gx, gy, self.radius - 1, gc, self.anim_frame);
                    }
                } else if self.pac_x < 2.0 * leg_len {
                    // Tier 2: Middle dots (Right -> Left)
                    let progress = self.pac_x - leg_len;
                    let current_pac_x = (w + self.radius as f32 * 2.0) - progress;

                    BaseRenderer::draw_text_at(
                        matrix,
                        &h_new,
                        font,
                        active_scale as f32,
                        tx,
                        ty_h,
                        (255, 255, 255),
                        (0, 0, 0),
                    );

                    for &dx in &dot_x {
                        if (dx as f32) < (current_pac_x - self.radius as f32)
                            || (dx as f32)
                                > (current_pac_x + self.radius as f32 * 3.0 + 4.0 * ghost_spacing)
                        {
                            for oy in -1..=0 {
                                for ox in -1..=0 {
                                    matrix.set_pixel(
                                        dx + ox,
                                        dot_y + oy,
                                        dot_color.0,
                                        dot_color.1,
                                        dot_color.2,
                                    );
                                }
                            }
                        }
                    }

                    BaseRenderer::draw_text_at(
                        matrix,
                        &m_old,
                        font,
                        active_scale as f32,
                        tx,
                        ty_m,
                        (100, 100, 100),
                        (0, 0, 0),
                    );

                    self.draw_pacman(
                        matrix,
                        current_pac_x as i32,
                        dot_y,
                        self.radius,
                        mouth_angle,
                        false,
                    );
                    for (i, &gc) in ghost_colors.iter().enumerate() {
                        let gx = current_pac_x as i32
                            + (self.radius * 2 + 3)
                            + (i as i32 * ghost_spacing as i32);
                        let gy = dot_y
                            + ((self.anim_frame as f32 * 0.4 + i as f32).sin()
                                * (self.radius as f32 / 3.0)) as i32;
                        self.draw_ghost(matrix, gx, gy, self.radius - 1, gc, self.anim_frame);
                    }
                } else {
                    // Tier 3: Minutes line (Left -> Right)
                    let progress = self.pac_x - 2.0 * leg_len;
                    let current_pac_x = -self.radius as f32 * 2.0 + progress;

                    BaseRenderer::draw_text_at(
                        matrix,
                        &h_new,
                        font,
                        active_scale as f32,
                        tx,
                        ty_h,
                        (255, 255, 255),
                        (0, 0, 0),
                    );
                    for &dx in &dot_x {
                        for oy in -1..=0 {
                            for ox in -1..=0 {
                                matrix.set_pixel(
                                    dx + ox,
                                    dot_y + oy,
                                    dot_color.0,
                                    dot_color.1,
                                    dot_color.2,
                                );
                            }
                        }
                    }

                    BaseRenderer::draw_text_at(
                        matrix,
                        &m_old,
                        font,
                        active_scale as f32,
                        tx,
                        ty_m,
                        (100, 100, 100),
                        (0, 0, 0),
                    );
                    for x in 0..current_pac_x as i32 {
                        for y in (ty_m - 2)..=(ty_m + text_h + 2) {
                            matrix.set_pixel(x, y, 0, 0, 0);
                        }
                    }

                    let reveal_x = (current_pac_x as i32
                        - (self.radius * 3 + 4 * ghost_spacing as i32))
                        .max(0);
                    if reveal_x > 0 {
                        BaseRenderer::draw_text_at(
                            matrix,
                            &m_new,
                            font,
                            active_scale as f32,
                            tx,
                            ty_m,
                            (255, 255, 255),
                            (0, 0, 0),
                        );
                        for x in reveal_x..w as i32 {
                            for y in (ty_m - 2)..=(ty_m + text_h + 2) {
                                matrix.set_pixel(x, y, 0, 0, 0);
                            }
                        }
                    }

                    self.draw_pacman(
                        matrix,
                        current_pac_x as i32,
                        ty_m + text_h / 2,
                        self.radius,
                        mouth_angle,
                        true,
                    );
                    for (i, &gc) in ghost_colors.iter().enumerate() {
                        let gx = current_pac_x as i32
                            - (self.radius * 2 + 3)
                            - (i as i32 * ghost_spacing as i32);
                        let gy = ty_m
                            + text_h / 2
                            + ((self.anim_frame as f32 * 0.4 + i as f32).sin()
                                * (self.radius as f32 / 3.0)) as i32;
                        self.draw_ghost(matrix, gx, gy, self.radius - 1, gc, self.anim_frame);
                    }
                }

                if self.pac_x >= max_path {
                    self.transitioning = false;
                    self.last_minute = now_min;
                    self.old_time_str = self.new_time_str.clone();
                }
            }
        } else if !self.transitioning {
            // Static display: draw time in center + scattered pellets
            let (pixels, _, _) = font.get_pixel_map(&self.new_time_str, active_scale as f32);
            let mut text_w = 0;
            let mut text_h = 0;
            for char_pixels in &pixels {
                for &(px, py) in char_pixels {
                    text_w = text_w.max(px + 1);
                    text_h = text_h.max(py + 1);
                }
            }
            let tx = (w as i32 - text_w) / 2;
            let ty = (h as i32 - text_h) / 2;

            BaseRenderer::draw_text_at(
                matrix,
                &self.new_time_str.clone(),
                font,
                active_scale as f32,
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

            let (pixels, _, _) = font.get_pixel_map(&self.old_time_str, active_scale as f32);
            let mut text_w = 0;
            let mut text_h = 0;
            for char_pixels in &pixels {
                for &(px, py) in char_pixels {
                    text_w = text_w.max(px + 1);
                    text_h = text_h.max(py + 1);
                }
            }
            let tx = (w as i32 - text_w) / 2;
            let ty = (h as i32 - text_h) / 2;

            // Draw old time (being "eaten" — visible only ahead of pac-man)
            BaseRenderer::draw_text_at(
                matrix,
                &self.old_time_str.clone(),
                font,
                active_scale as f32,
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

            let (new_pixels, _, _) = font.get_pixel_map(&self.new_time_str, active_scale as f32);
            let mut new_w = 0;
            let mut new_h = 0;
            for char_pixels in &new_pixels {
                for &(px, py) in char_pixels {
                    new_w = new_w.max(px + 1);
                    new_h = new_h.max(py + 1);
                }
            }
            let new_tx = (w as i32 - new_w) / 2;
            let new_ty = (h as i32 - new_h) / 2;

            BaseRenderer::draw_text_at(
                matrix,
                &self.new_time_str.clone(),
                font,
                active_scale as f32,
                new_tx,
                new_ty,
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
            self.draw_pacman(
                matrix,
                self.pac_x as i32,
                py,
                self.radius,
                mouth_angle,
                true,
            );

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
        facing_right: bool,
    ) {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy > r * r {
                    continue;
                }
                let in_mouth = if facing_right {
                    dx > 0 && dy.abs() * 45 < dx * mouth_deg
                } else {
                    dx < 0 && dy.abs() * 45 < (-dx) * mouth_deg
                };
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

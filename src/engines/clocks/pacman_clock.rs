use crate::core::matrix::IDrawingSurface;
use crate::engines::clocks::pacsprites::*;
use crate::engines::renderers::base_renderer::ArcadeFont;
use crate::engines::renderers::BaseRenderer;

pub struct PacmanClock {
    /// Ms. Pac-Man: the same parade with a bow, an eye and lips over the body.
    ms_variant: bool,
    pac_x: f32,
    direction: f32,
    last_minute: i32,
    last_hour: i32,
    transitioning: bool,
    old_time_str: String,
    new_time_str: String,
    speed: f32,
    radius: i32,
    /// millis() at the start of the current parade. The ESP32 clocks the parade on the wall clock
    /// so it runs at the same speed whatever the panel manages per second; stepping pac_x once per
    /// frame made it run at the frame rate instead.
    trans_start_ms: u128,
    /// `clock_speed` as a percentage, the same knob the ESP32 exposes.
    speed_pct: i32,
    offset_x: i32,
    offset_y: i32,
    /// `clock_color_1`, or the face's own default. The colon is not configurable: the ESP32 face
    /// fixes it at a blue that reads against the digits whatever colour they are.
    digit_color: (u8, u8, u8),
}

impl PacmanClock {
    /// Ms. Pac-Man: identical behaviour, her sprite.
    pub fn new_ms() -> Self {
        let mut c = Self::new();
        c.ms_variant = true;
        c
    }

    pub fn new() -> Self {
        Self {
            ms_variant: false,
            pac_x: 0.0,
            direction: 1.0,
            last_minute: -1,
            last_hour: -1,
            transitioning: false,
            old_time_str: String::new(),
            new_time_str: String::new(),
            speed: 2.0,
            trans_start_ms: 0,
            speed_pct: 100,
            offset_x: 0,
            offset_y: 0,
            digit_color: Self::COLON_COLOR,
            radius: 4,
        }
    }

    pub fn is_transitioning(&self) -> bool {
        self.transitioning
    }

    pub fn render(
        &mut self,
        matrix: &mut dyn IDrawingSurface,
        time_str: &str,
        hours: u32,
        minutes: u32,
        font: &ArcadeFont<'_>,
        scale: u32,
    ) {
        let w = matrix.width() as f32;
        let h = matrix.height() as f32;

        // Each variant carries its own outline colour when nothing else is configured, as on the
        // ESP32. The clock engine republishes the instance's own setting before every render, so
        // this only applies while this face is the one on screen.
        if BaseRenderer::glow_setting().0 == 0 {
            BaseRenderer::set_glow(
                2,
                if self.ms_variant {
                    (255, 60, 160) // her hot pink
                } else {
                    (255, 255, 0) // Pac-Man's yellow
                },
            );
        }

        let now_h = hours as i32;
        let now_min = minutes as i32;

        if self.last_minute == -1 {
            self.last_minute = now_min;
            self.last_hour = now_h;
            self.old_time_str = time_str.to_string();
            self.new_time_str = time_str.to_string();
        } else if (self.last_minute != now_min || self.last_hour != now_h) && !self.transitioning {
            self.transitioning = true;
            self.old_time_str = self.new_time_str.clone();
            self.new_time_str = time_str.to_string();
            self.pac_x = 0.0;
            self.trans_start_ms = Self::now_ms();
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
        // The 14 px ghost fills the panel (or the tier, in tate) with a small margin, which is how
        // the ESP32 face sizes the parade. Following the text height instead made the sprites a
        // different size on each platform for the same panel.
        let lane = if is_tate {
            (w as i32).min(h as i32 / 2)
        } else {
            h as i32
        };
        self.radius = ((lane - 1) / 2).max(3);
        // Pixels per second off the ghost's own width, scaled by clock_speed, exactly as the
        // ESP32 sizes it - not pixels per frame off the panel width.
        let ghost_px = GHOST_BODY_COLS * Self::sprite_scale(self.radius);
        self.speed = 2.0 * ghost_px as f32 * (self.speed_pct as f32 / 100.0);

        let py = (h / 2.0) as i32 + self.offset_y;

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

            let tx = (w as i32 - text_w) / 2 + self.offset_x;
            let ty_h = (h as i32 / 4) - (text_h / 2);
            let ty_m = (3 * h as i32 / 4) - (text_h / 2);
            let dot_y = (h as i32) / 2;
            let dot_x = [w as i32 / 4, w as i32 / 2, 3 * w as i32 / 4];
            let dot_color = (255, 183, 174);

            let pac_sprite_w = PAC_FRAME_CLOSED_COLS * Self::sprite_scale(self.radius);
            let ghost_spacing = self.radius as f32 * 2.2;
            let leg_len = w + pac_sprite_w as f32 * 2.0 + 4.0 * ghost_spacing;
            let max_path = 3.0 * leg_len;

            if !self.transitioning {
                BaseRenderer::draw_text_at(
                    matrix,
                    &h_new,
                    font,
                    active_scale as f32,
                    tx,
                    ty_h,
                    self.digit_color,
                    (0, 0, 0),
                );
                BaseRenderer::draw_text_at(
                    matrix,
                    &m_new,
                    font,
                    active_scale as f32,
                    tx,
                    ty_m,
                    self.digit_color,
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
                self.pac_x = ((Self::now_ms().saturating_sub(self.trans_start_ms)) as f32 / 1000.0)
                    * self.speed;
                let mouth_angle = Self::chomp_angle();
                let ghost_colors: [(u8, u8, u8); 4] =
                    [(255, 0, 0), (255, 184, 255), (0, 255, 255), (255, 184, 82)];

                if self.pac_x < leg_len {
                    let current_pac_x = (self.pac_x - pac_sprite_w as f32) as i32;
                    let reveal_x =
                        (current_pac_x - (self.radius * 3 + 4 * ghost_spacing as i32)).max(0);

                    // 1. Draw new hours behind reveal wave (0..reveal_x)
                    if reveal_x > 0 {
                        Self::draw_clipped_text(
                            matrix,
                            &h_new,
                            font,
                            active_scale as f32,
                            tx,
                            ty_h,
                            self.digit_color,
                            (0, 0, 0),
                            0,
                            reveal_x,
                            true,
                            Self::COLON_COLOR,
                        );
                    }

                    // 2. Draw old hours ahead of Pacman (current_pac_x..w)
                    if current_pac_x < w as i32 {
                        Self::draw_clipped_text(
                            matrix,
                            &h_old,
                            font,
                            active_scale as f32,
                            tx,
                            ty_h,
                            self.digit_color,
                            (0, 0, 0),
                            current_pac_x.max(0),
                            w as i32,
                            true,
                            Self::COLON_COLOR,
                        );
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
                        self.digit_color,
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
                        let gy = ty_h + text_h / 2;
                        self.draw_ghost(matrix, gx, gy, self.radius, gc, Self::skirt_tick(), false);
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
                        self.digit_color,
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
                        self.digit_color,
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
                        let gy = dot_y;
                        self.draw_ghost(matrix, gx, gy, self.radius, gc, Self::skirt_tick(), true);
                    }
                } else {
                    let progress = self.pac_x - 2.0 * leg_len;
                    let current_pac_x = (progress - pac_sprite_w as f32) as i32;
                    let reveal_x =
                        (current_pac_x - (self.radius * 3 + 4 * ghost_spacing as i32)).max(0);

                    BaseRenderer::draw_text_at(
                        matrix,
                        &h_new,
                        font,
                        active_scale as f32,
                        tx,
                        ty_h,
                        self.digit_color,
                        (0, 0, 0),
                    );

                    // 1. Draw new minutes behind reveal wave (0..reveal_x)
                    if reveal_x > 0 {
                        Self::draw_clipped_text(
                            matrix,
                            &m_new,
                            font,
                            active_scale as f32,
                            tx,
                            ty_m,
                            self.digit_color,
                            (0, 0, 0),
                            0,
                            reveal_x,
                            true,
                            Self::COLON_COLOR,
                        );
                    }

                    // 2. Draw old minutes ahead of Pacman (current_pac_x..w)
                    if current_pac_x < w as i32 {
                        Self::draw_clipped_text(
                            matrix,
                            &m_old,
                            font,
                            active_scale as f32,
                            tx,
                            ty_m,
                            self.digit_color,
                            (0, 0, 0),
                            current_pac_x.max(0),
                            w as i32,
                            true,
                            Self::COLON_COLOR,
                        );
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
                        let gy = ty_m + text_h / 2;
                        self.draw_ghost(matrix, gx, gy, self.radius, gc, Self::skirt_tick(), false);
                    }
                }

                if self.pac_x >= max_path {
                    self.transitioning = false;
                    self.last_minute = now_min;
                    self.last_hour = now_h;
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
            let tx = (w as i32 - text_w) / 2 + self.offset_x;
            let ty = (h as i32 - text_h) / 2 + self.offset_y;

            // The time, with the colon blinking, which is all the ESP32 face shows when nothing is
            // parading. The five pellets that used to drift here were a Pi-only flourish: their x
            // and y ran off sines of different frequencies, so the one visible dot traced a figure
            // eight across the panel.
            Self::draw_clipped_text(
                matrix,
                &self.new_time_str.clone(),
                font,
                active_scale as f32,
                tx,
                ty,
                self.digit_color,
                (0, 0, 0),
                0,
                w as i32,
                Self::colon_on(),
                Self::COLON_COLOR,
            );
        } else {
            // Transition animation
            self.pac_x =
                ((Self::now_ms().saturating_sub(self.trans_start_ms)) as f32 / 1000.0) * self.speed;

            let (pixels, _, _) = font.get_pixel_map(&self.old_time_str, active_scale as f32);
            let mut text_w = 0;
            let mut text_h = 0;
            for char_pixels in &pixels {
                for &(px, py) in char_pixels {
                    text_w = text_w.max(px + 1);
                    text_h = text_h.max(py + 1);
                }
            }
            let tx = (w as i32 - text_w) / 2 + self.offset_x;
            let ty = (h as i32 - text_h) / 2 + self.offset_y;

            let (new_pixels, _, _) = font.get_pixel_map(&self.new_time_str, active_scale as f32);
            let mut new_w = 0;
            let mut new_h = 0;
            for char_pixels in &new_pixels {
                for &(px, py) in char_pixels {
                    new_w = new_w.max(px + 1);
                    new_h = new_h.max(py + 1);
                }
            }
            let new_tx = (w as i32 - new_w) / 2 + self.offset_x;
            let new_ty = (h as i32 - new_h) / 2 + self.offset_y;

            // Mouth animation
            let mouth_angle = Self::chomp_angle();
            let ghost_colors: [(u8, u8, u8); 4] =
                [(255, 0, 0), (255, 184, 255), (0, 255, 255), (255, 184, 82)];
            let s = Self::sprite_scale(self.radius);
            let pac_w = PAC_FRAME_CLOSED_COLS * s;
            let ghost_w = GHOST_BODY_COLS * s;
            let ghost_gap = ghost_w + 2 * s; // ghost centre -> next ghost centre
            let first_ghost = pac_w / 2 + 4 * s + ghost_w / 2; // Pac-Man centre -> first ghost
            let dot_color = (255, 183, 174);
            // The energizer waits at the right edge. Reaching it is what ends the first leg: it is
            // what frightens the ghosts, so they turn blue and everyone reverses there rather than
            // after the whole line has left the panel. Matches the ESP32 face.
            let energizer_x = w as i32 - 4 * s;

            // pac_x is how far the parade has travelled. Leg 0 runs until his mouth reaches the
            // energizer; leg 1 walks the same distance back off the left edge.
            let leg_to_dot = (energizer_x + pac_w / 2) as f32;
            if self.pac_x <= leg_to_dot {
                let current_pac_x = (self.pac_x - pac_w as f32 / 2.0) as i32;

                // 1. New time runs all the way up to Pac-Man's mouth, so the clock is never missing
                // from the strip the parade is crossing; only the sprites themselves cover it.
                let reveal_x = current_pac_x.max(0);
                if reveal_x > 0 {
                    Self::draw_clipped_text(
                        matrix,
                        &self.new_time_str,
                        font,
                        active_scale as f32,
                        new_tx,
                        new_ty,
                        self.digit_color,
                        (0, 0, 0),
                        0,
                        reveal_x,
                        true,
                        Self::COLON_COLOR,
                    );
                }

                // 2. Draw old time ahead of Pacman (current_pac_x..w)
                if current_pac_x < w as i32 {
                    Self::draw_clipped_text(
                        matrix,
                        &self.old_time_str,
                        font,
                        active_scale as f32,
                        tx,
                        ty,
                        self.digit_color,
                        (0, 0, 0),
                        current_pac_x.max(0),
                        w as i32,
                        true,
                        Self::COLON_COLOR,
                    );
                }

                // 3. The energizer, flashing until he gets to it.
                if (Self::now_ms() / 180) % 2 == 0 {
                    let er = (2 * s).max(2);
                    for dy in 0..er {
                        for dx in 0..er {
                            matrix.set_pixel(
                                energizer_x - er / 2 + dx,
                                py - er / 2 + dy,
                                dot_color.0,
                                dot_color.1,
                                dot_color.2,
                            );
                        }
                    }
                }

                // A few crumbs at the mouth, so he still reads as eating the old time.
                // Kept to a byte, as the ESP32's uint8_t seed is: the full millisecond count
                // overflows once it is multiplied below.
                let seed =
                    ((((Self::now_ms() / 40) as u32) ^ (current_pac_x as u32)) & 0xFF) as i32;
                for pcrumb in 0..4 {
                    let cut_x = current_pac_x + pac_w / 2;
                    let ox = (seed + pcrumb * 7).rem_euclid(3 * s);
                    let oy = (seed * 3 + pcrumb * 11).rem_euclid(8 * s) - 4 * s;
                    if cut_x + ox < w as i32 {
                        matrix.set_pixel(
                            cut_x + ox,
                            py + oy,
                            dot_color.0,
                            dot_color.1,
                            dot_color.2,
                        );
                    }
                }

                self.draw_pacman(matrix, current_pac_x, py, self.radius, mouth_angle, true);
                for (i, &gc) in ghost_colors.iter().enumerate() {
                    let gx = current_pac_x - first_ghost - (i as i32 * ghost_gap);
                    self.draw_ghost(matrix, gx, py, self.radius, gc, Self::skirt_tick(), false);
                }
            } else {
                // The energizer has been eaten. The ghosts are blue and everyone has turned where
                // they stood, so the line carries on from the positions it held and walks back off
                // the left edge with Pac-Man behind it.
                // Through the clipping helper like every other leg, so the colon keeps its own
                // colour here too. draw_text_at paints the whole string one colour, which left the
                // colon white on the way back and blue everywhere else.
                Self::draw_clipped_text(
                    matrix,
                    &self.new_time_str.clone(),
                    font,
                    active_scale as f32,
                    new_tx,
                    new_ty,
                    self.digit_color,
                    (0, 0, 0),
                    0,
                    w as i32,
                    true,
                    Self::COLON_COLOR,
                );

                let back = energizer_x - (self.pac_x - leg_to_dot) as i32;
                for (i, &gc) in ghost_colors.iter().enumerate() {
                    let gx = back - first_ghost - (i as i32 * ghost_gap);
                    self.draw_ghost(matrix, gx, py, self.radius, gc, Self::skirt_tick(), true);
                }
                self.draw_pacman(matrix, back, py, self.radius, mouth_angle, false);
            }

            // Done once Pac-Man, the rightmost of them on the way back, has cleared the left edge.
            if self.pac_x >= 2.0 * leg_to_dot {
                self.transitioning = false;
                self.last_minute = now_min;
                self.last_hour = now_h;
                self.old_time_str = self.new_time_str.clone();
            }
        }
    }

    fn draw_clipped_text(
        matrix: &mut dyn IDrawingSurface,
        text: &str,
        font: &ArcadeFont<'_>,
        size: f32,
        x: i32,
        y: i32,
        primary: (u8, u8, u8),
        secondary: (u8, u8, u8),
        clip_min_x: i32,
        clip_max_x: i32,
        colon_on: bool,
        colon_color: (u8, u8, u8),
    ) {
        if clip_min_x >= clip_max_x {
            return;
        }
        let (pixels_by_char, _, _) = font.get_pixel_map(text, size);
        // Resolve the outline the same way BaseRenderer::draw_text_at does. This helper paints its
        // own pixels so it saw none of that, which left the digits glowing on the legs drawn
        // through draw_text_at and plain on the legs drawn through here.
        let (secondary, primary, offset) =
            BaseRenderer::glow_for(primary, secondary, (size as i32).max(1));

        for (char_pixels, ch) in pixels_by_char.iter().zip(text.chars()) {
            if ch == ':' && !colon_on {
                continue;
            }
            for &(gx, gy) in char_pixels {
                let px = x + gx;
                let py = y + gy;
                if px < clip_min_x || px >= clip_max_x {
                    continue;
                }
                for i in 1..=offset {
                    if px - i >= clip_min_x && px - i < clip_max_x {
                        matrix.set_pixel(px - i, py, secondary.0, secondary.1, secondary.2);
                    }
                    if px + i >= clip_min_x && px + i < clip_max_x {
                        matrix.set_pixel(px + i, py, secondary.0, secondary.1, secondary.2);
                    }
                    if px >= clip_min_x && px < clip_max_x {
                        matrix.set_pixel(px, py - i, secondary.0, secondary.1, secondary.2);
                        matrix.set_pixel(px, py + i, secondary.0, secondary.1, secondary.2);
                    }
                    if px + i >= clip_min_x && px + i < clip_max_x {
                        matrix.set_pixel(px + i, py + i, secondary.0, secondary.1, secondary.2);
                    }
                    if px - i >= clip_min_x && px - i < clip_max_x {
                        matrix.set_pixel(px - i, py - i, secondary.0, secondary.1, secondary.2);
                    }
                    if px + i >= clip_min_x && px + i < clip_max_x {
                        matrix.set_pixel(px + i, py - i, secondary.0, secondary.1, secondary.2);
                    }
                    if px - i >= clip_min_x && px - i < clip_max_x {
                        matrix.set_pixel(px - i, py + i, secondary.0, secondary.1, secondary.2);
                    }
                }
            }
        }

        for (char_pixels, ch) in pixels_by_char.iter().zip(text.chars()) {
            if ch == ':' && !colon_on {
                continue;
            }
            let ink = if ch == ':' { colon_color } else { primary };
            for &(gx, gy) in char_pixels {
                let px = x + gx;
                let py = y + gy;
                if px >= clip_min_x && px < clip_max_x {
                    matrix.set_pixel(px, py, ink.0, ink.1, ink.2);
                }
            }
        }
    }

    /// Closed - half - open - half on a 200 ms cycle, the ESP32's chompSeq. Returned as the mouth
    /// angle draw_pacman maps back onto the three frames.
    fn chomp_angle() -> i32 {
        const SEQ: [i32; 4] = [0, 20, 40, 20];
        SEQ[((Self::now_ms() / 50) & 3) as usize]
    }

    /// The skirt swaps every 160 ms, as on the ESP32.
    fn skirt_tick() -> u32 {
        (Self::now_ms() / 160) as u32
    }

    /// Milliseconds since the epoch, the Pi's stand-in for the ESP32's millis().
    fn now_ms() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    }

    /// Instance settings the parade honours, applied when the config changes.
    pub fn configure(
        &mut self,
        speed_pct: i32,
        offset_x: i32,
        offset_y: i32,
        digit_color: Option<(u8, u8, u8)>,
    ) {
        self.speed_pct = speed_pct.clamp(25, 300);
        self.offset_x = offset_x;
        self.offset_y = offset_y;
        // Unset, or set to black, falls back to the face's own palette.
        self.digit_color = match digit_color {
            Some(c) if c != (0, 0, 0) => c,
            _ => Self::COLON_COLOR,
        };
    }

    /// The colon the ESP32 face draws, fixed rather than taken from the instance colours.
    const COLON_COLOR: (u8, u8, u8) = (60, 100, 255);

    /// The colon is on for half a second at a time, as on the ESP32, and is read from the clock
    /// rather than the frame counter so it keeps time whatever the panel's frame rate.
    fn colon_on() -> bool {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| (d.as_millis() / 500) % 2 == 0)
            .unwrap_or(true)
    }

    /// One integer scale for the whole parade, the way the ESP32 face sizes it: the 14 px ghost
    /// fills the lane with a small margin, and Pac-Man is drawn at the same scale.
    fn sprite_scale(r: i32) -> i32 {
        ((2 * r - 1) / GHOST_BODY_COLS).max(1)
    }

    /// Blit one row-mask sprite. `mirror` flips it horizontally, which is how Pac-Man faces left.
    #[allow(clippy::too_many_arguments)]
    fn blit(
        matrix: &mut dyn IDrawingSurface,
        rows: &[u16],
        n_cols: i32,
        left: i32,
        top: i32,
        s: i32,
        color: (u8, u8, u8),
        mirror: bool,
    ) {
        for (r, &bits) in rows.iter().enumerate() {
            let y = top + r as i32 * s;
            for c in 0..n_cols {
                let src = if mirror { n_cols - 1 - c } else { c };
                if bits & (1u16 << (n_cols - 1 - src)) == 0 {
                    continue;
                }
                for dy in 0..s {
                    for dx in 0..s {
                        matrix.set_pixel(left + c * s + dx, y + dy, color.0, color.1, color.2);
                    }
                }
            }
        }
    }

    /// Pac-Man from the same 13x13 pixel art the ESP32 face uses, rather than a drawn circle with a
    /// wedge cut out of it. `mouth_deg` is the caller's animation, mapped onto the three frames.
    fn draw_pacman(
        &self,
        matrix: &mut dyn IDrawingSurface,
        cx: i32,
        cy: i32,
        r: i32,
        mouth_deg: i32,
        facing_right: bool,
    ) {
        let s = Self::sprite_scale(r);
        let w = PAC_FRAME_CLOSED_COLS * s;
        let h = PAC_FRAME_CLOSED_ROWS * s;
        let left = cx - w / 2;
        let top = cy - h / 2;
        let frame = if mouth_deg < 15 {
            0
        } else if mouth_deg < 30 {
            1
        } else {
            2
        };

        if self.ms_variant {
            // She is not one colour, so her frames are colour-indexed rather than a plain mask.
            let art: &[&str; 13] = match frame {
                0 => &MSPAC_CLOSED,
                1 => &MSPAC_HALF,
                _ => &MSPAC_OPEN,
            };
            for (row_i, row) in art.iter().enumerate() {
                let bytes = row.as_bytes();
                for c in 0..MSPAC_COLS {
                    let src = if facing_right { c } else { MSPAC_COLS - 1 - c };
                    let col = match bytes[src as usize] {
                        b'y' => (255, 255, 0),
                        b'r' => (228, 0, 88),    // bow
                        b'p' => (255, 150, 200), // bow highlight
                        b'k' => (16, 16, 40),    // eye
                        b'l' => (255, 80, 150),  // lips
                        _ => continue,
                    };
                    for dy in 0..s {
                        for dx in 0..s {
                            matrix.set_pixel(
                                left + c * s + dx,
                                top + row_i as i32 * s + dy,
                                col.0,
                                col.1,
                                col.2,
                            );
                        }
                    }
                }
            }
            return;
        }

        let rows: &[u16] = match frame {
            0 => &PAC_FRAME_CLOSED,
            1 => &PAC_FRAME_HALF,
            _ => &PAC_FRAME_OPEN,
        };
        Self::blit(
            matrix,
            rows,
            PAC_FRAME_CLOSED_COLS,
            left,
            top,
            s,
            (255, 255, 0),
            !facing_right,
        );
    }

    /// Ghosts from the same 14 px pixel art the ESP32 face uses: a 12-row body with a two-frame
    /// skirt, eyes and pupils over it, or the frightened face the energizer produces.
    #[allow(clippy::too_many_arguments)]
    fn draw_ghost(
        &self,
        matrix: &mut dyn IDrawingSurface,
        cx: i32,
        cy: i32,
        r: i32,
        color: (u8, u8, u8),
        tick: u32,
        frightened: bool,
    ) {
        let s = Self::sprite_scale(r);
        let cols = GHOST_BODY_COLS;
        let w = cols * s;
        let h = (GHOST_BODY_ROWS + SKIRT_A_ROWS) * s;
        let left = cx - w / 2;
        let top = cy - h / 2;

        let body = if frightened { (33, 33, 255) } else { color };
        Self::blit(matrix, &GHOST_BODY, cols, left, top, s, body, false);
        let skirt: &[u16] = if (tick / 3) % 2 == 0 {
            &SKIRT_A
        } else {
            &SKIRT_B
        };
        Self::blit(
            matrix,
            skirt,
            cols,
            left,
            top + GHOST_BODY_ROWS * s,
            s,
            body,
            false,
        );
        if frightened {
            Self::blit(
                matrix,
                &FRIGHT_FACE,
                cols,
                left,
                top + 5 * s,
                s,
                (255, 184, 174),
                false,
            );
        } else {
            Self::blit(
                matrix,
                &EYES,
                cols,
                left,
                top + 3 * s,
                s,
                (255, 255, 255),
                false,
            );
            Self::blit(
                matrix,
                &PUPIL_R,
                cols,
                left,
                top + 5 * s,
                s,
                (33, 33, 255),
                false,
            );
        }
    }
}

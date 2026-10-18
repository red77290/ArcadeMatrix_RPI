use crate::core::matrix::IDrawingSurface as Matrix;
use crate::engines::clocks::cwassets::sonic::*;
use std::time::Instant;

#[derive(PartialEq, Clone, Copy)]
enum Phase {
    Idle,
    HeroEnter,
    JumpStrike,
    Impact,
    HeroExit,
}

fn rgb565_to_rgb888(val: u16) -> (u8, u8, u8) {
    let r5 = ((val >> 11) & 0x1F) as u32;
    let g6 = ((val >> 5) & 0x3F) as u32;
    let b5 = (val & 0x1F) as u32;
    (
        ((r5 * 255 + 15) / 31) as u8,
        ((g6 * 255 + 31) / 63) as u8,
        ((b5 * 255 + 15) / 31) as u8,
    )
}

pub struct SonicClock {
    phase: Phase,
    phase_timer: f32,
    ring_timer: f32,
    ring_frame: usize,
    motobug_timer: f32,
    run_timer: f32,
    run_frame: usize,
    ball_timer: f32,
    ball_frame: usize,
    sonic_x: f32,
    sonic_y: f32,
    climb_stage: u8,
    jump_t: f32,
    shown_hh: String,
    shown_mm: String,
    last_minute: i32,
    last_second: i32,
    snap_to_now: bool,
    last_frame_ms: Instant,
}

impl Default for SonicClock {
    fn default() -> Self {
        Self::new()
    }
}

impl SonicClock {
    pub fn new() -> Self {
        Self {
            phase: Phase::Idle,
            phase_timer: 0.0,
            ring_timer: 0.0,
            ring_frame: 0,
            motobug_timer: 0.0,
            run_timer: 0.0,
            run_frame: 0,
            ball_timer: 0.0,
            ball_frame: 0,
            sonic_x: -40.0,
            sonic_y: 0.0,
            climb_stage: 0,
            jump_t: 0.0,
            shown_hh: "00".to_string(),
            shown_mm: "00".to_string(),
            last_minute: -1,
            last_second: -1,
            snap_to_now: true,
            last_frame_ms: Instant::now(),
        }
    }

    pub fn on_activated(&mut self) {
        self.snap_to_now = true;
    }

    pub fn on_display_geometry_changed(&mut self) {
        self.snap_to_now = true;
        self.motobug_timer = 0.0;
        self.phase = Phase::Idle;
        self.phase_timer = 0.0;
        self.sonic_x = -40.0;
    }

    #[allow(clippy::too_many_arguments)]
    fn blit_sprite(
        matrix: &mut dyn Matrix,
        palette: &[u16],
        pixels: &[u8],
        w: i32,
        h: i32,
        x: i32,
        y: i32,
        flip_h: bool,
    ) {
        let panel_w = matrix.width() as i32;
        let panel_h = matrix.height() as i32;
        for row in 0..h {
            let py = y + row;
            if py < 0 || py >= panel_h {
                continue;
            }
            for col in 0..w {
                let px = x + col;
                if px < 0 || px >= panel_w {
                    continue;
                }
                let src_col = if flip_h { w - 1 - col } else { col };
                let Some(&idx) = pixels.get((row * w + src_col) as usize) else {
                    continue;
                };
                if idx == 0 {
                    continue;
                }
                if let Some(&col565) = palette.get(idx as usize) {
                    let (r, g, b) = rgb565_to_rgb888(col565);
                    matrix.set_pixel(px, py, r, g, b);
                }
            }
        }
    }

    fn draw_arcade_digit(
        matrix: &mut dyn Matrix,
        x: i32,
        y: i32,
        c: char,
        color: (u8, u8, u8),
        scale: i32,
    ) {
        let idx = if c.is_ascii_digit() {
            (c as u8 - b'0') as usize
        } else if c == ':' {
            10
        } else {
            return;
        };

        for r in 0..5 {
            let row_bits = ARCADE_FONT_3x5[idx][r as usize];
            let py = y + r * scale;
            for b in 0..3 {
                if ((row_bits >> (2 - b)) & 1) != 0 {
                    let px = x + b * scale;
                    matrix.fill_rect(px, py, scale, scale, color.0, color.1, color.2);
                }
            }
        }
    }

    fn draw_arcade_time(
        matrix: &mut dyn Matrix,
        start_x: i32,
        start_y: i32,
        s: &str,
        color: (u8, u8, u8),
        scale: i32,
    ) {
        let bytes = s.as_bytes();
        let digit_w = 3 * scale;
        let digit_gap = scale;
        if bytes.len() == 2 {
            Self::draw_arcade_digit(matrix, start_x, start_y, bytes[0] as char, color, scale);
            Self::draw_arcade_digit(
                matrix,
                start_x + digit_w + digit_gap,
                start_y,
                bytes[1] as char,
                color,
                scale,
            );
            return;
        }
        if bytes.len() < 5 {
            return;
        }
        let sp = 2 * scale;
        let x_h1 = start_x;
        let x_h2 = x_h1 + digit_w + digit_gap;
        let x_colon = x_h2 + 2 * scale + sp;
        let x_m1 = x_colon + 2 * scale + sp;
        let x_m2 = x_m1 + digit_w + digit_gap;

        Self::draw_arcade_digit(matrix, x_h1, start_y, bytes[0] as char, color, scale);
        Self::draw_arcade_digit(matrix, x_h2, start_y, bytes[1] as char, color, scale);
        Self::draw_arcade_digit(matrix, x_colon, start_y, bytes[2] as char, color, scale);
        Self::draw_arcade_digit(matrix, x_m1, start_y, bytes[3] as char, color, scale);
        Self::draw_arcade_digit(matrix, x_m2, start_y, bytes[4] as char, color, scale);
    }

    fn draw_green_hill_ground(matrix: &mut dyn Matrix, w: i32, h: i32, g_height: i32) {
        let g_top = h - g_height;
        let grass_light = (0, 224, 0);
        let grass_mid = (0, 160, 0);
        let grass_dark = (0, 100, 0);
        let soil_light = (208, 112, 16);
        let soil_dark = (144, 64, 0);

        matrix.fill_rect(0, g_top, w, 1, grass_light.0, grass_light.1, grass_light.2);
        matrix.fill_rect(0, g_top + 1, w, 1, grass_mid.0, grass_mid.1, grass_mid.2);
        for x in 0..w {
            if (x % 4) == 0 || (x % 4) == 1 {
                matrix.set_pixel(x, g_top + 2, grass_dark.0, grass_dark.1, grass_dark.2);
            }
        }

        let mut x = 0;
        while x < w {
            let bw = if x + 8 > w { w - x } else { 8 };
            let mut y = g_top + 3;
            while y < h {
                let bh = if y + 6 > h { h - y } else { 6 };
                let alt = ((x / 8) + (y / 6)) % 2 == 0;
                let col = if alt { soil_light } else { soil_dark };
                matrix.fill_rect(x, y, bw, bh, col.0, col.1, col.2);
                y += 6;
            }
            x += 8;
        }
    }

    fn draw_checkered_platform(matrix: &mut dyn Matrix, px: i32, py: i32, pw: i32, ph: i32) {
        let grass_light = (0, 224, 0);
        let grass_mid = (0, 160, 0);
        let soil_light = (208, 112, 16);
        let soil_dark = (144, 64, 0);

        matrix.fill_rect(px, py, pw, 1, grass_light.0, grass_light.1, grass_light.2);
        matrix.fill_rect(px, py + 1, pw, 1, grass_mid.0, grass_mid.1, grass_mid.2);

        let mut x = px;
        while x < px + pw {
            let bw = if x + 8 > px + pw { px + pw - x } else { 8 };
            let mut y = py + 2;
            while y < py + ph {
                let bh = if y + 4 > py + ph { py + ph - y } else { 4 };
                let alt = ((x / 8) + (y / 4)) % 2 == 0;
                let col = if alt { soil_light } else { soil_dark };
                matrix.fill_rect(x, y, bw, bh, col.0, col.1, col.2);
                y += 4;
            }
            x += 8;
        }
    }

    fn draw_scene(matrix: &mut dyn Matrix, w: i32, h: i32) {
        let sky_blue = (64, 160, 248);
        let is_portrait = h > w;

        if is_portrait {
            matrix.fill_rect(0, 0, w, h, sky_blue.0, sky_blue.1, sky_blue.2);

            let mountain_col = (32, 144, 96);
            matrix.fill_triangle(
                0,
                48,
                16,
                28,
                32,
                48,
                mountain_col.0,
                mountain_col.1,
                mountain_col.2,
            );
            matrix.fill_triangle(
                24,
                48,
                44,
                24,
                64,
                48,
                mountain_col.0,
                mountain_col.1,
                mountain_col.2,
            );

            if w >= 64 {
                Self::draw_checkered_platform(matrix, 16, 190, 40, 6);
                Self::draw_checkered_platform(matrix, 8, 130, 40, 6);
                Self::draw_checkered_platform(matrix, 8, 70, 48, 6);
            } else {
                Self::draw_checkered_platform(matrix, 0, 56, w, 6);
                Self::draw_checkered_platform(matrix, 8, 88, w - 8, 6);
            }

            Self::draw_green_hill_ground(matrix, w, h, 12);
        } else if h >= 48 {
            matrix.fill_rect(0, 0, w, 48, sky_blue.0, sky_blue.1, sky_blue.2);

            let mountain_col = (32, 144, 96);
            let mut x = 0;
            while x < w {
                matrix.fill_triangle(
                    x,
                    48,
                    x + 20,
                    24,
                    x + 40,
                    48,
                    mountain_col.0,
                    mountain_col.1,
                    mountain_col.2,
                );
                x += 40;
            }

            Self::draw_green_hill_ground(matrix, w, h, 16);
        } else {
            matrix.fill_rect(0, 0, w, 23, sky_blue.0, sky_blue.1, sky_blue.2);
            Self::draw_green_hill_ground(matrix, w, h, 9);

            matrix.draw_line(120, 10, 118, 23, 160, 112, 48);
            matrix.draw_line(121, 10, 119, 23, 160, 112, 48);
            matrix.fill_triangle(114, 8, 120, 6, 126, 8, 0, 180, 0);
            matrix.fill_triangle(114, 8, 120, 11, 126, 8, 0, 180, 0);
        }
    }

    pub fn render(&mut self, matrix: &mut dyn Matrix, hours: u32, minutes: u32, seconds: u32) {
        let now = Instant::now();
        let mut dt = now.duration_since(self.last_frame_ms).as_secs_f32();
        if dt <= 0.0 || dt > 0.2 {
            dt = 0.016;
        }
        self.last_frame_ms = now;
        self.draw(matrix, hours, minutes, seconds, dt);
    }

    pub fn draw(
        &mut self,
        matrix: &mut dyn Matrix,
        hours: u32,
        minutes: u32,
        seconds: u32,
        dt: f32,
    ) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;
        let is_portrait = h > w;

        let hh = format!("{:02}", hours);
        let mm = format!("{:02}", minutes);

        if self.snap_to_now || self.last_minute == -1 {
            self.snap_to_now = false;
            self.shown_hh = hh.clone();
            self.shown_mm = mm.clone();
            self.last_minute = minutes as i32;
            self.last_second = seconds as i32;
            self.ring_frame = 0;
            self.ring_timer = 0.0;
            self.motobug_timer = 0.0;
            self.phase = Phase::Idle;
            self.phase_timer = 0.0;
            self.sonic_x = -40.0;
        }

        if self.last_second != seconds as i32 {
            self.last_second = seconds as i32;
        }

        if self.last_minute != minutes as i32 && self.phase == Phase::Idle {
            self.phase = Phase::HeroEnter;
            self.phase_timer = 0.0;
            if is_portrait {
                self.sonic_x = -20.0;
                self.sonic_y = (h - 16 - 24) as f32;
                self.climb_stage = 0;
                self.jump_t = 0.0;
            } else {
                self.sonic_x = -30.0;
                self.sonic_y = if h >= 48 { 64.0 - 16.0 - 39.0 } else { 0.0 };
            }
        }

        self.ring_timer += dt;
        if self.ring_timer >= 0.12 {
            self.ring_timer = 0.0;
            self.ring_frame = (self.ring_frame + 1) % 4;
        }

        self.motobug_timer += dt;
        let moto_offset = (self.motobug_timer * 1.5).sin() * if is_portrait { 8.0 } else { 14.0 };
        let moto_flip = (self.motobug_timer * 1.5).cos() < 0.0;

        if self.phase == Phase::HeroEnter || self.phase == Phase::HeroExit {
            self.run_timer += dt;
            if self.run_timer >= 0.07 {
                self.run_timer = 0.0;
                self.run_frame = (self.run_frame + 1) % 4;
            }
        }

        if self.phase == Phase::JumpStrike
            || self.phase == Phase::Impact
            || (is_portrait
                && self.phase == Phase::HeroEnter
                && (self.climb_stage == 1 || self.climb_stage == 3 || self.climb_stage == 5))
        {
            self.ball_timer += dt;
            if self.ball_timer >= 0.05 {
                self.ball_timer = 0.0;
                self.ball_frame = (self.ball_frame + 1) % 4;
            }
        }

        match self.phase {
            Phase::Idle => {}
            Phase::HeroEnter => {
                if is_portrait {
                    if w >= 64 {
                        let ground_y = (h - 16 - 24) as f32;
                        match self.climb_stage {
                            0 => {
                                self.sonic_x += 80.0 * dt;
                                self.sonic_y = ground_y;
                                if self.sonic_x >= 14.0 {
                                    self.climb_stage = 1;
                                    self.jump_t = 0.0;
                                }
                            }
                            1 => {
                                self.jump_t += dt * 2.8;
                                let t = self.jump_t.min(1.0);
                                self.sonic_x = 14.0 + (36.0 - 14.0) * t;
                                let base_y = ground_y + (166.0 - ground_y) * t;
                                let arc = 4.0 * 14.0 * t * (1.0 - t);
                                self.sonic_y = base_y - arc;
                                if self.jump_t >= 1.0 {
                                    self.jump_t = 0.0;
                                    self.sonic_y = 166.0;
                                    self.climb_stage = 2;
                                }
                            }
                            2 => {
                                self.sonic_x -= 65.0 * dt;
                                self.sonic_y = 166.0;
                                if self.sonic_x <= 30.0 {
                                    self.climb_stage = 3;
                                    self.jump_t = 0.0;
                                }
                            }
                            3 => {
                                self.jump_t += dt * 2.8;
                                let t = self.jump_t.min(1.0);
                                self.sonic_x = 30.0 + (16.0 - 30.0) * t;
                                let base_y = 166.0 + (106.0 - 166.0) * t;
                                let arc = 4.0 * 14.0 * t * (1.0 - t);
                                self.sonic_y = base_y - arc;
                                if self.jump_t >= 1.0 {
                                    self.jump_t = 0.0;
                                    self.sonic_y = 106.0;
                                    self.climb_stage = 4;
                                }
                            }
                            4 => {
                                self.sonic_x += 65.0 * dt;
                                self.sonic_y = 106.0;
                                if self.sonic_x >= 24.0 {
                                    self.climb_stage = 5;
                                    self.jump_t = 0.0;
                                }
                            }
                            5 => {
                                self.jump_t += dt * 2.8;
                                let t = self.jump_t.min(1.0);
                                self.sonic_x = 24.0 + (28.0 - 24.0) * t;
                                let base_y = 106.0 + (46.0 - 106.0) * t;
                                let arc = 4.0 * 14.0 * t * (1.0 - t);
                                self.sonic_y = base_y - arc;
                                if self.jump_t >= 1.0 {
                                    self.jump_t = 0.0;
                                    self.sonic_y = 46.0;
                                    self.climb_stage = 6;
                                }
                            }
                            _ => {
                                self.sonic_x -= 65.0 * dt;
                                self.sonic_y = 46.0;
                                if self.sonic_x <= 26.0 {
                                    self.sonic_x = 26.0;
                                    self.phase = Phase::JumpStrike;
                                    self.phase_timer = 0.0;
                                }
                            }
                        }
                    } else {
                        let ground_y = (h - 12 - SONIC_SMS_IDLE_H) as f32;
                        match self.climb_stage {
                            0 => {
                                self.sonic_x += 65.0 * dt;
                                self.sonic_y = ground_y;
                                if self.sonic_x >= 8.0 {
                                    self.climb_stage = 1;
                                    self.jump_t = 0.0;
                                }
                            }
                            1 => {
                                self.jump_t += dt * 2.8;
                                let t = self.jump_t.min(1.0);
                                self.sonic_x = 8.0 + (20.0 - 8.0) * t;
                                let base_y =
                                    ground_y + ((88.0 - SONIC_SMS_IDLE_H as f32) - ground_y) * t;
                                let arc = 4.0 * 12.0 * t * (1.0 - t);
                                self.sonic_y = base_y - arc;
                                if self.jump_t >= 1.0 {
                                    self.jump_t = 0.0;
                                    self.sonic_y = 88.0 - SONIC_SMS_IDLE_H as f32;
                                    self.climb_stage = 2;
                                }
                            }
                            2 => {
                                self.sonic_x -= 65.0 * dt;
                                self.sonic_y = 88.0 - SONIC_SMS_IDLE_H as f32;
                                if self.sonic_x <= 10.0 {
                                    self.climb_stage = 3;
                                    self.jump_t = 0.0;
                                }
                            }
                            3 => {
                                self.jump_t += dt * 2.8;
                                let t = self.jump_t.min(1.0);
                                self.sonic_x = 10.0 + (7.0 - 10.0) * t;
                                let base_y = (88.0 - SONIC_SMS_IDLE_H as f32)
                                    + ((56.0 - SONIC_SMS_IDLE_H as f32)
                                        - (88.0 - SONIC_SMS_IDLE_H as f32))
                                        * t;
                                let arc = 4.0 * 12.0 * t * (1.0 - t);
                                self.sonic_y = base_y - arc;
                                if self.jump_t >= 1.0 {
                                    self.jump_t = 0.0;
                                    self.sonic_y = 56.0 - SONIC_SMS_IDLE_H as f32;
                                    self.climb_stage = 4;
                                }
                            }
                            _ => {
                                self.sonic_x = 7.0;
                                self.sonic_y = 56.0 - SONIC_SMS_IDLE_H as f32;
                                self.phase = Phase::JumpStrike;
                                self.phase_timer = 0.0;
                            }
                        }
                    }
                } else {
                    let target_x = if h >= 48 { 36.0 } else { 10.0 };
                    self.sonic_x += 130.0 * dt;
                    if self.sonic_x >= target_x {
                        self.sonic_x = target_x;
                        self.phase = Phase::JumpStrike;
                        self.phase_timer = 0.0;
                    }
                }
            }
            Phase::JumpStrike => {
                self.phase_timer += dt;
                if self.phase_timer >= 0.22 {
                    self.phase = Phase::Impact;
                    self.phase_timer = 0.0;
                    self.shown_hh = hh;
                    self.shown_mm = mm;
                    self.last_minute = minutes as i32;
                }
            }
            Phase::Impact => {
                self.phase_timer += dt;
                if self.phase_timer >= 0.18 {
                    self.phase = Phase::HeroExit;
                    self.phase_timer = 0.0;
                }
            }
            Phase::HeroExit => {
                if is_portrait {
                    self.sonic_x += 85.0 * dt;
                    self.sonic_y = if w >= 64 {
                        70.0 - 24.0
                    } else {
                        56.0 - SONIC_SMS_IDLE_H as f32
                    };
                    if self.sonic_x > w as f32 + 24.0 {
                        self.phase = Phase::Idle;
                        self.phase_timer = 0.0;
                    }
                } else {
                    self.sonic_x += 150.0 * dt;
                    if self.sonic_x > w as f32 + 24.0 {
                        self.phase = Phase::Idle;
                        self.phase_timer = 0.0;
                    }
                }
            }
        }

        Self::draw_scene(matrix, w, h);

        let time_buf = format!("{}:{}", self.shown_hh, self.shown_mm);
        let yellow_time = (252, 224, 0);

        let ring_pal = SONIC_RING_PALS[self.ring_frame % 4];
        let ring_pix = SONIC_RING_PIXELS[self.ring_frame % 4];

        if is_portrait {
            if w >= 64 {
                Self::draw_arcade_time(matrix, 10, 16, &time_buf, yellow_time, 2);
                Self::blit_sprite(
                    matrix,
                    &SONIC_ITEM_BOX_PAL,
                    &SONIC_ITEM_BOX_PIXELS,
                    SONIC_ITEM_BOX_W,
                    SONIC_ITEM_BOX_H,
                    16,
                    70 - 32,
                    false,
                );
                Self::blit_sprite(matrix, ring_pal, ring_pix, 16, 16, 20, 110, false);
                Self::blit_sprite(matrix, ring_pal, ring_pix, 16, 16, 28, 170, false);
                let moto_x = 12 + moto_offset as i32;
                Self::blit_sprite(
                    matrix,
                    &SONIC_MOTOBUG_PAL,
                    &SONIC_MOTOBUG_PIXELS,
                    SONIC_MOTOBUG_W,
                    SONIC_MOTOBUG_H,
                    moto_x,
                    h - 16 - 29,
                    moto_flip,
                );
            } else {
                let time_x = (w - 14) / 2;
                Self::draw_arcade_time(matrix, time_x, 8, &self.shown_hh, yellow_time, 2);
                matrix.fill_rect(
                    time_x + 6,
                    19,
                    2,
                    2,
                    yellow_time.0,
                    yellow_time.1,
                    yellow_time.2,
                );
                Self::draw_arcade_time(matrix, time_x, 23, &self.shown_mm, yellow_time, 2);
                Self::blit_sprite(matrix, ring_pal, ring_pix, 16, 16, (w - 16) / 2, 68, false);
            }

            if self.phase != Phase::Idle {
                let is_airborne = self.phase == Phase::JumpStrike
                    || self.phase == Phase::Impact
                    || (self.phase == Phase::HeroEnter
                        && (self.climb_stage == 1
                            || self.climb_stage == 3
                            || self.climb_stage == 5));
                let flip_h = is_portrait
                    && w < 64
                    && self.phase == Phase::HeroEnter
                    && self.climb_stage == 2;
                if is_airborne {
                    Self::blit_sprite(
                        matrix,
                        &SONIC_MD_IDLE_PAL,
                        SONIC_MD_BALL_PIXELS[self.ball_frame % 4],
                        SONIC_MD_BALL_0_W,
                        SONIC_MD_BALL_0_H,
                        self.sonic_x as i32,
                        self.sonic_y as i32,
                        false,
                    );
                    if self.phase == Phase::Impact {
                        matrix.fill_circle(w / 2, if w >= 64 { 54 } else { 36 }, 5, 255, 255, 255);
                    }
                } else {
                    Self::blit_sprite(
                        matrix,
                        &SONIC_SMS_IDLE_PAL,
                        SONIC_SMS_RUN_PIXELS[self.run_frame % 4],
                        SONIC_SMS_RUN_0_W,
                        SONIC_SMS_RUN_0_H,
                        self.sonic_x as i32,
                        self.sonic_y as i32,
                        flip_h,
                    );
                }
            }
        } else if h >= 48 {
            Self::blit_sprite(
                matrix,
                &SONIC_ITEM_BOX_PAL,
                &SONIC_ITEM_BOX_PIXELS,
                SONIC_ITEM_BOX_W,
                SONIC_ITEM_BOX_H,
                52,
                16,
                false,
            );
            Self::draw_arcade_time(matrix, 84, 15, &time_buf, yellow_time, 4);
            Self::blit_sprite(matrix, ring_pal, ring_pix, 16, 16, 178, 22, false);
            let moto_x = 208 + moto_offset as i32;
            Self::blit_sprite(
                matrix,
                &SONIC_MOTOBUG_PAL,
                &SONIC_MOTOBUG_PIXELS,
                SONIC_MOTOBUG_W,
                SONIC_MOTOBUG_H,
                moto_x,
                64 - 16 - 29,
                moto_flip,
            );

            if self.phase != Phase::Idle {
                if self.phase == Phase::JumpStrike || self.phase == Phase::Impact {
                    Self::blit_sprite(
                        matrix,
                        &SONIC_MD_IDLE_PAL,
                        SONIC_MD_BALL_PIXELS[self.ball_frame % 4],
                        SONIC_MD_BALL_0_W,
                        SONIC_MD_BALL_0_H,
                        self.sonic_x as i32 + 4,
                        self.sonic_y as i32 + 12,
                        false,
                    );
                    if self.phase == Phase::Impact {
                        matrix.fill_circle(52 + 16, 16 + 16, 5, 255, 255, 255);
                    }
                } else {
                    Self::blit_sprite(
                        matrix,
                        &SONIC_MD_IDLE_PAL,
                        SONIC_MD_RUN_PIXELS[self.run_frame % 4],
                        SONIC_MD_RUN_0_W,
                        SONIC_MD_RUN_0_H,
                        self.sonic_x as i32,
                        self.sonic_y as i32,
                        false,
                    );
                }
            }
        } else {
            Self::blit_sprite(matrix, ring_pal, ring_pix, 16, 16, 10, 6, false);
            Self::draw_arcade_time(matrix, 31, 6, &time_buf, yellow_time, 3);

            if self.phase != Phase::Idle {
                if self.phase == Phase::JumpStrike || self.phase == Phase::Impact {
                    Self::blit_sprite(
                        matrix,
                        &SONIC_MD_IDLE_PAL,
                        SONIC_MD_BALL_PIXELS[self.ball_frame % 4],
                        SONIC_MD_BALL_0_W,
                        SONIC_MD_BALL_0_H,
                        self.sonic_x as i32,
                        self.sonic_y as i32,
                        false,
                    );
                    if self.phase == Phase::Impact {
                        matrix.fill_circle(20, 14, 3, 255, 255, 255);
                    }
                } else {
                    Self::blit_sprite(
                        matrix,
                        &SONIC_SMS_IDLE_PAL,
                        SONIC_SMS_RUN_PIXELS[self.run_frame % 4],
                        SONIC_SMS_RUN_0_W,
                        SONIC_SMS_RUN_0_H,
                        self.sonic_x as i32,
                        self.sonic_y as i32,
                        false,
                    );
                }
            }
        }
    }
}

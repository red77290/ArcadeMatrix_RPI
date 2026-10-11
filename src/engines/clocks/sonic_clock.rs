//! Official Sega Genesis / Mega Drive Sonic the Hedgehog clock face for ArcadeMatrix.
//!
//! Features:
//! - Green Hill Zone backdrop with checkered platforms, distant mountains, and palm trees.
//! - Animated Motobug patrol on the ground.
//! - Rotating golden rings (4-frame rotation animation).
//! - Item Monitor Box smash on minute change.
//! - Sonic sprint, spin jump, and impact sequence on minute flip.
//! - Arcade 3x5 font for time digits.
//! - Full support for wide (256x64), compact (128x32), and Tate / portrait orientations.
//! - Canonical `IDrawingSurface` implementation.

use crate::core::matrix::IDrawingSurface;
use crate::engines::clocks::cwassets::sonic::*;
use crate::engines::clocks::cwscene;
use std::time::Instant;

#[derive(PartialEq, Clone, Copy, Debug)]
enum Phase {
    Idle,
    HeroEnter,
    JumpStrike,
    Impact,
    HeroExit,
}

#[inline]
fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 & 0xF8) << 8) | ((g as u16 & 0xFC) << 3) | ((b as u16) >> 3)
}

pub struct SonicClock {
    phase: Phase,
    phase_timer: f32,
    motobug_timer: f32,
    ring_timer: f32,
    ring_frame: usize,
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
    last_frame: Instant,
    dirty: u8,
}

impl SonicClock {
    pub fn new() -> Self {
        Self {
            phase: Phase::Idle,
            phase_timer: 0.0,
            motobug_timer: 0.0,
            ring_timer: 0.0,
            ring_frame: 0,
            run_timer: 0.0,
            run_frame: 0,
            ball_timer: 0.0,
            ball_frame: 0,
            sonic_x: -40.0,
            sonic_y: 0.0,
            climb_stage: 0,
            jump_t: 0.0,
            shown_hh: String::from("--"),
            shown_mm: String::from("--"),
            last_minute: -1,
            last_frame: Instant::now(),
            dirty: 3,
        }
    }

    fn blit_sprite(
        matrix: &mut dyn IDrawingSurface,
        palette: &[u16],
        pixels: &[u8],
        w: i32,
        h: i32,
        x: i32,
        y: i32,
        flip_h: bool,
    ) {
        let mw = matrix.width() as i32;
        let mh = matrix.height() as i32;
        for row in 0..h {
            let py = y + row;
            if py < 0 || py >= mh {
                continue;
            }
            for col in 0..w {
                let px = x + col;
                if px < 0 || px >= mw {
                    continue;
                }
                let src_col = if flip_h { w - 1 - col } else { col };
                let idx = pixels[(row * w + src_col) as usize];
                if idx == 0 {
                    continue;
                }
                let color = palette[idx as usize];
                cwscene::put(matrix, px, py, color);
            }
        }
    }

    fn draw_arcade_digit(
        matrix: &mut dyn IDrawingSurface,
        x: i32,
        y: i32,
        c: char,
        color: u16,
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
            let row_bits = ARCADE_FONT_3x5[idx][r];
            let py = y + r as i32 * scale;
            for b in 0..3 {
                if ((row_bits >> (2 - b)) & 1) != 0 {
                    let px = x + b as i32 * scale;
                    for dy in 0..scale {
                        for dx in 0..scale {
                            cwscene::put(matrix, px + dx, py + dy, color);
                        }
                    }
                }
            }
        }
    }

    fn draw_arcade_time(
        matrix: &mut dyn IDrawingSurface,
        start_x: i32,
        start_y: i32,
        str_time: &str,
        color: u16,
        scale: i32,
    ) {
        let chars: Vec<char> = str_time.chars().collect();
        if chars.len() < 5 {
            return;
        }

        let digit_w = 3 * scale;
        let digit_gap = 1 * scale;
        let s = 2 * scale;

        let x_h1 = start_x;
        let x_h2 = x_h1 + digit_w + digit_gap;
        let x_colon = x_h2 + 2 * scale + s;
        let x_m1 = x_colon + 2 * scale + s;
        let x_m2 = x_m1 + digit_w + digit_gap;

        Self::draw_arcade_digit(matrix, x_h1, start_y, chars[0], color, scale);
        Self::draw_arcade_digit(matrix, x_h2, start_y, chars[1], color, scale);
        Self::draw_arcade_digit(matrix, x_colon, start_y, chars[2], color, scale);
        Self::draw_arcade_digit(matrix, x_m1, start_y, chars[3], color, scale);
        Self::draw_arcade_digit(matrix, x_m2, start_y, chars[4], color, scale);
    }

    fn draw_green_hill_ground(matrix: &mut dyn IDrawingSurface, w: i32, h: i32, g_height: i32) {
        let g_top = h - g_height;
        let grass_light = rgb565(0, 224, 0);
        let grass_mid = rgb565(0, 160, 0);
        let grass_dark = rgb565(0, 100, 0);
        let soil_light = rgb565(208, 112, 16);
        let soil_dark = rgb565(144, 64, 0);

        for x in 0..w {
            cwscene::put(matrix, x, g_top, grass_light);
            cwscene::put(matrix, x, g_top + 1, grass_mid);
            if (x % 4 == 0) || (x % 4 == 1) {
                cwscene::put(matrix, x, g_top + 2, grass_dark);
            }
        }

        let mut x = 0;
        while x < w {
            let bw = if (x + 8) > w { w - x } else { 8 };
            let mut y = g_top + 3;
            while y < h {
                let bh = if (y + 6) > h { h - y } else { 6 };
                let alt = ((x / 8) + (y / 6)) % 2 == 0;
                let color = if alt { soil_light } else { soil_dark };
                for dy in 0..bh {
                    for dx in 0..bw {
                        cwscene::put(matrix, x + dx, y + dy, color);
                    }
                }
                y += 6;
            }
            x += 8;
        }
    }

    fn draw_checkered_platform(
        matrix: &mut dyn IDrawingSurface,
        px: i32,
        py: i32,
        pw: i32,
        ph: i32,
    ) {
        let grass_light = rgb565(0, 224, 0);
        let grass_mid = rgb565(0, 160, 0);
        let soil_light = rgb565(208, 112, 16);
        let soil_dark = rgb565(144, 64, 0);

        for x in px..px + pw {
            cwscene::put(matrix, x, py, grass_light);
            cwscene::put(matrix, x, py + 1, grass_mid);
        }

        let mut x = px;
        while x < px + pw {
            let bw = if (x + 8) > (px + pw) {
                (px + pw) - x
            } else {
                8
            };
            let mut y = py + 2;
            while y < py + ph {
                let bh = if (y + 4) > (py + ph) {
                    (py + ph) - y
                } else {
                    4
                };
                let alt = ((x / 8) + (y / 4)) % 2 == 0;
                let color = if alt { soil_light } else { soil_dark };
                for dy in 0..bh {
                    for dx in 0..bw {
                        cwscene::put(matrix, x + dx, y + dy, color);
                    }
                }
                y += 4;
            }
            x += 8;
        }
    }

    fn fill_triangle(
        matrix: &mut dyn IDrawingSurface,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        color: u16,
    ) {
        let min_x = x0.min(x1).min(x2);
        let max_x = x0.max(x1).max(x2);
        let min_y = y0.min(y1).min(y2);
        let max_y = y0.max(y1).max(y2);

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let d1 = (x - x1) * (y0 - y1) - (x0 - x1) * (y - y1);
                let d2 = (x - x2) * (y1 - y2) - (x1 - x2) * (y - y2);
                let d3 = (x - x0) * (y2 - y0) - (x2 - x0) * (y - y0);

                let has_neg = (d1 < 0) || (d2 < 0) || (d3 < 0);
                let has_pos = (d1 > 0) || (d2 > 0) || (d3 > 0);

                if !(has_neg && has_pos) {
                    cwscene::put(matrix, x, y, color);
                }
            }
        }
    }

    fn draw_scene(matrix: &mut dyn IDrawingSurface, w: i32, h: i32) {
        let sky_blue = rgb565(64, 160, 248);
        let is_portrait = h > w;

        if is_portrait {
            for y in 0..h {
                for x in 0..w {
                    cwscene::put(matrix, x, y, sky_blue);
                }
            }
            let mountain_col = rgb565(32, 144, 96);
            Self::fill_triangle(matrix, 0, 48, 16, 28, 32, 48, mountain_col);
            Self::fill_triangle(matrix, 24, 48, 44, 24, 64, 48, mountain_col);

            Self::draw_checkered_platform(matrix, 16, 190, 40, 6);
            Self::draw_checkered_platform(matrix, 8, 130, 40, 6);
            Self::draw_checkered_platform(matrix, 8, 70, 48, 6);

            Self::draw_green_hill_ground(matrix, w, h, 16);
        } else if h >= 48 {
            for y in 0..48.min(h) {
                for x in 0..w {
                    cwscene::put(matrix, x, y, sky_blue);
                }
            }
            let mountain_col = rgb565(32, 144, 96);
            let mut x = 0;
            while x < w {
                Self::fill_triangle(matrix, x, 48, x + 20, 24, x + 40, 48, mountain_col);
                x += 40;
            }
            Self::draw_green_hill_ground(matrix, w, h, 16);
        } else {
            for y in 0..23.min(h) {
                for x in 0..w {
                    cwscene::put(matrix, x, y, sky_blue);
                }
            }
            Self::draw_green_hill_ground(matrix, w, h, 9);

            // Palm tree on right (x=114..126)
            let trunk_col = rgb565(160, 112, 48);
            for y in 10..23 {
                cwscene::put(matrix, 120, y, trunk_col);
                cwscene::put(matrix, 121, y, trunk_col);
            }
            let palm_green = rgb565(0, 180, 0);
            Self::fill_triangle(matrix, 114, 8, 120, 6, 126, 8, palm_green);
            Self::fill_triangle(matrix, 114, 8, 120, 11, 126, 8, palm_green);
        }
    }

    pub fn render(&mut self, matrix: &mut dyn IDrawingSurface, hours: u32, minutes: u32) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;
        let is_portrait = h > w;

        let dt = self.last_frame.elapsed().as_secs_f32().min(0.2);
        self.last_frame = Instant::now();

        let hh = format!("{:02}", hours);
        let mm = format!("{:02}", minutes);

        if self.last_minute < 0 {
            self.shown_hh = hh.clone();
            self.shown_mm = mm.clone();
            self.last_minute = minutes as i32;
            self.phase = Phase::Idle;
            self.phase_timer = 0.0;
            self.sonic_x = -40.0;
            self.dirty = 3;
        }

        // Trigger minute transition sprint
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
                self.sonic_y = if h >= 48 { (64 - 16 - 39) as f32 } else { 0.0 };
            }
            self.dirty = 3;
        }

        // Golden ring animation timer
        self.ring_timer += dt;
        if self.ring_timer >= 0.12 {
            self.ring_timer = 0.0;
            self.ring_frame = (self.ring_frame + 1) % 4;
            self.dirty = self.dirty.max(2);
        }

        // Motobug patrol timer
        self.motobug_timer += dt;
        let moto_offset = (self.motobug_timer * 1.5).sin() * (if is_portrait { 8.0 } else { 14.0 });
        let moto_flip = (self.motobug_timer * 1.5).cos() < 0.0;

        // Running cycle timer
        if self.phase == Phase::HeroEnter || self.phase == Phase::HeroExit {
            self.run_timer += dt;
            if self.run_timer >= 0.07 {
                self.run_timer = 0.0;
                self.run_frame = (self.run_frame + 1) % 4;
                self.dirty = self.dirty.max(2);
            }
        }

        // Spin ball animation timer
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
                self.dirty = self.dirty.max(2);
            }
        }

        // State Machine
        match self.phase {
            Phase::Idle => {}
            Phase::HeroEnter => {
                self.dirty = self.dirty.max(2);
                if is_portrait {
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
                self.dirty = self.dirty.max(2);
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
                self.dirty = self.dirty.max(2);
                if self.phase_timer >= 0.18 {
                    self.phase = Phase::HeroExit;
                    self.phase_timer = 0.0;
                }
            }
            Phase::HeroExit => {
                self.dirty = self.dirty.max(2);
                if is_portrait {
                    self.sonic_x += 85.0 * dt;
                    self.sonic_y = (70 - 24) as f32;
                    if self.sonic_x > (w + 24) as f32 {
                        self.phase = Phase::Idle;
                        self.phase_timer = 0.0;
                        self.dirty = 3;
                    }
                } else {
                    self.sonic_x += 150.0 * dt;
                    if self.sonic_x > (w + 24) as f32 {
                        self.phase = Phase::Idle;
                        self.phase_timer = 0.0;
                        self.dirty = 3;
                    }
                }
            }
        }

        // Render Frame
        Self::draw_scene(matrix, w, h);

        let time_buf = format!("{}:{}", self.shown_hh, self.shown_mm);
        let yellow_time = rgb565(252, 224, 0);

        let ring_pal = SONIC_RING_PALS[self.ring_frame % 4];
        let ring_pix = SONIC_RING_PIXELS[self.ring_frame % 4];

        if is_portrait {
            // Symmetrically centered time at top (scale 2 = 44px wide, centered at 10)
            Self::draw_arcade_time(matrix, 10, 16, &time_buf, yellow_time, 2);

            // Monitor box sitting on Platform 1 (y=70, monitor height 32 -> y=38)
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

            // Rotating golden rings floating above platforms
            Self::blit_sprite(matrix, ring_pal, ring_pix, 16, 16, 20, 110, false);
            Self::blit_sprite(matrix, ring_pal, ring_pix, 16, 16, 28, 170, false);

            // Motobug patrol on bottom ground
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

            // Sonic minute reward sequence
            if self.phase != Phase::Idle {
                let is_airborne = self.phase == Phase::JumpStrike
                    || self.phase == Phase::Impact
                    || (self.phase == Phase::HeroEnter
                        && (self.climb_stage == 1
                            || self.climb_stage == 3
                            || self.climb_stage == 5));
                if is_airborne {
                    Self::blit_sprite(
                        matrix,
                        &SONIC_MD_IDLE_PAL,
                        SONIC_MD_BALL_PIXELS[self.ball_frame],
                        SONIC_MD_BALL_0_W,
                        SONIC_MD_BALL_0_H,
                        self.sonic_x as i32,
                        self.sonic_y as i32,
                        false,
                    );
                    if self.phase == Phase::Impact {
                        for dy in -5..=5 {
                            for dx in -5..=5 {
                                if dx * dx + dy * dy <= 25 {
                                    cwscene::put(matrix, 32 + dx, 54 + dy, 0xFFFF);
                                }
                            }
                        }
                    }
                } else {
                    Self::blit_sprite(
                        matrix,
                        &SONIC_SMS_IDLE_PAL,
                        SONIC_SMS_RUN_PIXELS[self.run_frame],
                        SONIC_SMS_RUN_0_W,
                        SONIC_SMS_RUN_0_H,
                        self.sonic_x as i32,
                        self.sonic_y as i32,
                        false,
                    );
                }
            }
        } else if h >= 48 {
            // 256x64 Layout
            // Item Monitor box (x=52, y=64-16-32=16)
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

            // Centered Arcade Time (scale 4 = 88px wide, centered at 84)
            Self::draw_arcade_time(matrix, 84, 15, &time_buf, yellow_time, 4);

            // Rotating golden ring (x=178, y=22)
            Self::blit_sprite(matrix, ring_pal, ring_pix, 16, 16, 178, 22, false);

            // Badnik Motobug on far right
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

            // Sonic enters on minute change
            if self.phase != Phase::Idle {
                if self.phase == Phase::JumpStrike || self.phase == Phase::Impact {
                    Self::blit_sprite(
                        matrix,
                        &SONIC_MD_IDLE_PAL,
                        SONIC_MD_BALL_PIXELS[self.ball_frame],
                        SONIC_MD_BALL_0_W,
                        SONIC_MD_BALL_0_H,
                        self.sonic_x as i32 + 4,
                        self.sonic_y as i32 + 12,
                        false,
                    );
                    if self.phase == Phase::Impact {
                        for dy in -5..=5 {
                            for dx in -5..=5 {
                                if dx * dx + dy * dy <= 25 {
                                    cwscene::put(matrix, 52 + 16 + dx, 16 + 16 + dy, 0xFFFF);
                                }
                            }
                        }
                    }
                } else {
                    Self::blit_sprite(
                        matrix,
                        &SONIC_MD_IDLE_PAL,
                        SONIC_MD_RUN_PIXELS[self.run_frame],
                        SONIC_MD_RUN_0_W,
                        SONIC_MD_RUN_0_H,
                        self.sonic_x as i32,
                        self.sonic_y as i32,
                        false,
                    );
                }
            }
        } else {
            // 128x32 Layout
            // Rotating golden ring on left (x=10, y=6)
            Self::blit_sprite(matrix, ring_pal, ring_pix, 16, 16, 10, 6, false);

            // Centered Arcade Time (scale 3 = 66px wide, centered at 31)
            Self::draw_arcade_time(matrix, 31, 6, &time_buf, yellow_time, 3);

            // Sonic enters on minute change
            if self.phase != Phase::Idle {
                if self.phase == Phase::JumpStrike || self.phase == Phase::Impact {
                    Self::blit_sprite(
                        matrix,
                        &SONIC_MD_IDLE_PAL,
                        SONIC_MD_BALL_PIXELS[self.ball_frame],
                        SONIC_MD_BALL_0_W,
                        SONIC_MD_BALL_0_H,
                        self.sonic_x as i32,
                        self.sonic_y as i32,
                        false,
                    );
                    if self.phase == Phase::Impact {
                        for dy in -3..=3 {
                            for dx in -3..=3 {
                                if dx * dx + dy * dy <= 9 {
                                    cwscene::put(matrix, 20 + dx, 14 + dy, 0xFFFF);
                                }
                            }
                        }
                    }
                } else {
                    Self::blit_sprite(
                        matrix,
                        &SONIC_SMS_IDLE_PAL,
                        SONIC_SMS_RUN_PIXELS[self.run_frame],
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

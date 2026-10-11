//! Official Capcom NES Mega Man 2 clock face for ArcadeMatrix.
//!
//! Features:
//! - Mega Man walking, jumping, charging buster, and firing charge blast on minute change.
//! - Metool enemy on platform.
//! - Dynamic nighttime city skyline with lit windows.
//! - 5x7 Capcom arcade font for digits and time.
//! - Full support for wide/tall (256x64), Tate/portrait mode, and compact 128x32.
//! - Canonical `IDrawingSurface` implementation.

use crate::core::matrix::IDrawingSurface;
use crate::engines::clocks::cwassets::megaman::*;
use crate::engines::clocks::cwscene;
use std::time::Instant;

#[derive(PartialEq, Clone, Copy, Debug)]
enum Phase {
    Idle,
    RunIn,
    ChargeShoot,
    BlastFlying,
    Victory,
}

#[inline]
fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 & 0xF8) << 8) | ((g as u16 & 0xFC) << 3) | ((b as u16) >> 3)
}

pub struct MegamanClock {
    phase: Phase,
    phase_timer: f32,
    walk_timer: f32,
    walk_frame: usize,
    bullet_active: bool,
    bullet_x: f32,
    bullet_y: f32,
    megaman_x: f32,
    megaman_y: f32,
    metool_x: f32,
    shown_hh: String,
    shown_mm: String,
    last_minute: i32,
    last_frame: Instant,
    dirty: u8,
}

impl MegamanClock {
    pub fn new() -> Self {
        Self {
            phase: Phase::Idle,
            phase_timer: 0.0,
            walk_timer: 0.0,
            walk_frame: 0,
            bullet_active: false,
            bullet_x: 0.0,
            bullet_y: 0.0,
            megaman_x: 14.0,
            megaman_y: 0.0,
            metool_x: 100.0,
            shown_hh: String::from("--"),
            shown_mm: String::from("--"),
            last_minute: -1,
            last_frame: Instant::now(),
            dirty: 3,
        }
    }

    fn blit_sprite(
        matrix: &mut dyn IDrawingSurface,
        data: &[u16],
        w: i32,
        h: i32,
        x: i32,
        y: i32,
        transparent: bool,
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
                let c = data[(row * w + col) as usize];
                if transparent && c == MASK {
                    continue;
                }
                cwscene::put(matrix, px, py, c);
            }
        }
    }

    fn draw_capcom_digit(matrix: &mut dyn IDrawingSurface, x: i32, y: i32, c: char, color: u16) {
        let idx = if c.is_ascii_digit() {
            c as usize - '0' as usize
        } else if c == ':' {
            10
        } else {
            return;
        };

        for row in 0..7 {
            let row_bits = CAPCOM_FONT[idx][row];
            for col in 0..5 {
                if ((row_bits >> (4 - col)) & 1) != 0 {
                    cwscene::put(matrix, x + col as i32, y + row as i32, color);
                }
            }
        }
    }

    fn draw_capcom_time(
        matrix: &mut dyn IDrawingSurface,
        start_x: i32,
        start_y: i32,
        time_str: &str,
        color: u16,
    ) {
        let shadow = rgb565(10, 20, 40);
        let mut cur_x = start_x;
        for c in time_str.chars() {
            Self::draw_capcom_digit(matrix, cur_x + 1, start_y + 1, c, shadow);
            Self::draw_capcom_digit(matrix, cur_x, start_y, c, color);
            cur_x += if c == ':' { 4 } else { 7 };
        }
    }

    fn draw_skyline(&self, matrix: &mut dyn IDrawingSurface, w: i32, ground_top: i32) {
        let sky_col = rgb565(8, 12, 32);
        for y in 0..ground_top {
            for x in 0..w {
                cwscene::put(matrix, x, y, sky_col);
            }
        }

        // City skyline silhouettes with lit windows
        let bldg_col = rgb565(20, 28, 55);
        let bldg_light = rgb565(35, 48, 85);
        let window_col = rgb565(240, 220, 100);

        let buildings = [
            (0, 32, 18),
            (20, 22, 16),
            (38, 40, 22),
            (62, 28, 20),
            (84, 18, 18),
            (104, 36, 24),
            (130, 25, 18),
            (150, 42, 22),
            (174, 20, 20),
            (196, 32, 26),
            (224, 26, 28),
        ];

        for (bx, bh, bw) in buildings {
            let by = ground_top - bh;
            for y in by..ground_top {
                for x in bx..bx + bw {
                    if x < w {
                        cwscene::put(matrix, x, y, bldg_col);
                    }
                }
            }
            // Top outline
            for x in bx..bx + bw {
                if x < w {
                    cwscene::put(matrix, x, by, bldg_light);
                }
            }
            // Lit windows
            for wy in (by + 4..ground_top - 6).step_by(6) {
                for wx in (bx + 3..bx + bw - 3).step_by(5) {
                    if wx < w && ((wx + wy) % 7 != 0) {
                        cwscene::put(matrix, wx, wy, window_col);
                    }
                }
            }
        }
    }

    fn draw_scene(&self, matrix: &mut dyn IDrawingSurface, w: i32, h: i32) {
        let is_portrait = h > w;
        let ground_top = if is_portrait { h - 24 } else { h - 8 };

        self.draw_skyline(matrix, w, ground_top);

        // Ground brick tiles
        let mut gx = 0;
        while gx < w {
            Self::blit_sprite(
                matrix,
                &GROUND_TILE,
                GROUND_TILE_W,
                GROUND_TILE_H,
                gx,
                ground_top,
                false,
            );
            gx += GROUND_TILE_W;
        }

        // Metool enemy
        let metool_draw_x = (w - 30).max(30);
        Self::blit_sprite(
            matrix,
            &METOOL_SLEEP,
            METOOL_W,
            METOOL_H,
            metool_draw_x,
            ground_top - METOOL_H,
            true,
        );

        // Mega Man rendering
        let mm_ground_y = ground_top - MEGAMAN_IDLE_H;
        match self.phase {
            Phase::Idle => {
                Self::blit_sprite(
                    matrix,
                    &MEGAMAN_IDLE,
                    MEGAMAN_IDLE_W,
                    MEGAMAN_IDLE_H,
                    self.megaman_x as i32,
                    mm_ground_y,
                    true,
                );
            }
            Phase::RunIn => {
                let (pix, sw, sh) = match self.walk_frame % 3 {
                    0 => (&MEGAMAN_RUN1[..], MEGAMAN_RUN1_W, MEGAMAN_RUN1_H),
                    1 => (
                        &MEGAMAN_RUN_PASS1[..],
                        MEGAMAN_RUN_PASS_W,
                        MEGAMAN_RUN_PASS_H,
                    ),
                    _ => (&MEGAMAN_RUN2[..], MEGAMAN_RUN2_W, MEGAMAN_RUN2_H),
                };
                Self::blit_sprite(
                    matrix,
                    pix,
                    sw,
                    sh,
                    self.megaman_x as i32,
                    ground_top - sh,
                    true,
                );
            }
            Phase::ChargeShoot => {
                Self::blit_sprite(
                    matrix,
                    &MEGAMAN_SHOOT,
                    MEGAMAN_SHOOT_W,
                    MEGAMAN_SHOOT_H,
                    self.megaman_x as i32,
                    ground_top - MEGAMAN_SHOOT_H,
                    true,
                );
            }
            Phase::BlastFlying => {
                Self::blit_sprite(
                    matrix,
                    &MEGAMAN_JUMP_SHOOT,
                    MEGAMAN_JUMP_SHOOT_W,
                    MEGAMAN_JUMP_SHOOT_H,
                    self.megaman_x as i32,
                    ground_top - MEGAMAN_JUMP_SHOOT_H - 4,
                    true,
                );
            }
            Phase::Victory => {
                Self::blit_sprite(
                    matrix,
                    &MEGAMAN_IDLE,
                    MEGAMAN_IDLE_W,
                    MEGAMAN_IDLE_H,
                    self.megaman_x as i32,
                    mm_ground_y,
                    true,
                );
            }
        }

        // Flying Plasma Charge Bullet
        if self.bullet_active {
            Self::blit_sprite(
                matrix,
                &BUSTER_BULLET,
                BULLET_W,
                BULLET_H,
                self.bullet_x as i32,
                self.bullet_y as i32,
                true,
            );
        }

        // Time display
        let time_str = format!("{}:{}", self.shown_hh, self.shown_mm);
        let time_col = rgb565(80, 220, 255);
        if is_portrait {
            Self::draw_capcom_time(matrix, 12, 16, &time_str, time_col);
        } else if h >= 48 {
            Self::draw_capcom_time(matrix, 18, 12, &time_str, time_col);
        } else {
            Self::draw_capcom_time(matrix, 14, 4, &time_str, time_col);
        }
    }

    pub fn render(&mut self, matrix: &mut dyn IDrawingSurface, hours: u32, minutes: u32) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;
        let is_portrait = h > w;
        let ground_top = if is_portrait { h - 24 } else { h - 8 };

        let dt = self.last_frame.elapsed().as_secs_f32().min(0.2);
        self.last_frame = Instant::now();

        let hh = format!("{:02}", hours);
        let mm = format!("{:02}", minutes);
        if self.shown_hh == "--" {
            self.shown_hh = hh.clone();
            self.shown_mm = mm.clone();
            self.dirty = 3;
        }

        // Minute change trigger
        if minutes as i32 != self.last_minute && self.phase == Phase::Idle {
            self.last_minute = minutes as i32;
            self.phase = Phase::RunIn;
            self.phase_timer = 0.0;
            self.megaman_x = -24.0;
            self.dirty = 3;
        }

        let target_shoot_x = if is_portrait { 6.0 } else { 18.0 };

        match self.phase {
            Phase::Idle => {
                self.megaman_x = target_shoot_x;
            }
            Phase::RunIn => {
                self.walk_timer += dt;
                if self.walk_timer >= 0.10 {
                    self.walk_timer = 0.0;
                    self.walk_frame += 1;
                }
                self.megaman_x += 50.0 * dt;
                if self.megaman_x >= target_shoot_x {
                    self.megaman_x = target_shoot_x;
                    self.phase = Phase::ChargeShoot;
                    self.phase_timer = 0.0;
                }
                self.dirty = self.dirty.max(2);
            }
            Phase::ChargeShoot => {
                self.phase_timer += dt;
                if self.phase_timer >= 0.3 {
                    self.phase = Phase::BlastFlying;
                    self.phase_timer = 0.0;
                    self.bullet_active = true;
                    self.bullet_x = self.megaman_x + MEGAMAN_SHOOT_W as f32;
                    self.bullet_y = (ground_top - MEGAMAN_SHOOT_H + 8) as f32;
                }
                self.dirty = self.dirty.max(2);
            }
            Phase::BlastFlying => {
                self.bullet_x += 160.0 * dt;
                if self.bullet_x >= (w - 20) as f32 {
                    self.bullet_active = false;
                    self.phase = Phase::Victory;
                    self.phase_timer = 0.0;
                    // Flip time upon bullet impact!
                    self.shown_hh = hh.clone();
                    self.shown_mm = mm.clone();
                }
                self.dirty = self.dirty.max(2);
            }
            Phase::Victory => {
                self.phase_timer += dt;
                if self.phase_timer >= 1.2 {
                    self.phase = Phase::Idle;
                    self.dirty = 3;
                }
                self.dirty = self.dirty.max(2);
            }
        }

        if self.phase == Phase::Idle && self.dirty == 0 {
            return;
        }
        if self.dirty > 0 {
            self.dirty -= 1;
        }

        self.draw_scene(matrix, w, h);
    }
}

impl Default for MegamanClock {
    fn default() -> Self {
        Self::new()
    }
}

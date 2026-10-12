//! Authentic SNK Neo Geo Metal Slug clock face for ArcadeMatrix.
//!
//! Features:
//! - Marco Rossi animated walking, shooting machine gun with fiery muzzle tracers, throwing grenade, and victory pose.
//! - Rebel Army Di-Cokka Tank: Stance, cannon fire, wrecked state.
//! - Rebel Patrol Helicopter cruising overhead.
//! - Flying grenade ballistic arc with explosive blast fireballs.
//! - Authentic SNK Neo Geo Arcade HUD ("1UP", score based on time, glowing arcade digits).
//! - Zero-legacy canonical `IDrawingSurface` implementation.

use crate::core::matrix::IDrawingSurface;
use crate::engines::clocks::cwassets::metal_slug::*;
use crate::engines::clocks::cwscene;
use std::time::Instant;

#[derive(PartialEq, Clone, Copy, Debug)]
enum Phase {
    Idle,
    HeroEnter,
    Firefight,
    GrenadeAssault,
    Explosion,
    HeroExit,
}

#[inline]
fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 & 0xF8) << 8) | ((g as u16 & 0xFC) << 3) | ((b as u16) >> 3)
}

pub struct MetalSlugClock {
    phase: Phase,
    phase_timer: f32,
    anim_timer: f32,
    anim_frame: usize,
    walk_timer: f32,
    walk_frame: usize,
    explosion_frame: usize,
    explosion_timer: f32,
    marco_x: f32,
    tank_x: f32,
    heli_x: f32,
    grenade_active: bool,
    grenade_x: f32,
    grenade_y: f32,
    grenade_t: f32,
    shown_hh: String,
    shown_mm: String,
    last_minute: i32,
    last_frame: Instant,
    dirty: u8,
}

impl MetalSlugClock {
    pub fn new() -> Self {
        Self {
            phase: Phase::Idle,
            phase_timer: 0.0,
            anim_timer: 0.0,
            anim_frame: 0,
            walk_timer: 0.0,
            walk_frame: 0,
            explosion_frame: 0,
            explosion_timer: 0.0,
            marco_x: -50.0,
            tank_x: 185.0,
            heli_x: -60.0,
            grenade_active: false,
            grenade_x: 0.0,
            grenade_y: 0.0,
            grenade_t: 0.0,
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
                let idx = pixels[(row * w + src_col) as usize] as usize;
                if idx == 0 || idx >= palette.len() {
                    continue;
                }
                cwscene::put(matrix, px, py, palette[idx]);
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
            c as usize - '0' as usize
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
                    let px = x + b * scale;
                    for sy in 0..scale {
                        for sx in 0..scale {
                            cwscene::put(matrix, px + sx, py + sy, color);
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
        str_val: &str,
        color: u16,
        scale: i32,
    ) {
        let chars: Vec<char> = str_val.chars().collect();
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

        let shadow = rgb565(20, 10, 10);
        Self::draw_arcade_digit(matrix, x_h1 + 1, start_y + 1, chars[0], shadow, scale);
        Self::draw_arcade_digit(matrix, x_h2 + 1, start_y + 1, chars[1], shadow, scale);
        Self::draw_arcade_digit(matrix, x_colon + 1, start_y + 1, chars[2], shadow, scale);
        Self::draw_arcade_digit(matrix, x_m1 + 1, start_y + 1, chars[3], shadow, scale);
        Self::draw_arcade_digit(matrix, x_m2 + 1, start_y + 1, chars[4], shadow, scale);

        Self::draw_arcade_digit(matrix, x_h1, start_y, chars[0], color, scale);
        Self::draw_arcade_digit(matrix, x_h2, start_y, chars[1], color, scale);
        Self::draw_arcade_digit(matrix, x_colon, start_y, chars[2], color, scale);
        Self::draw_arcade_digit(matrix, x_m1, start_y, chars[3], color, scale);
        Self::draw_arcade_digit(matrix, x_m2, start_y, chars[4], color, scale);
    }

    fn draw_scene(&self, matrix: &mut dyn IDrawingSurface, w: i32, h: i32) {
        let is_portrait = h > w;
        let ground_top = if is_portrait { h - 36 } else { h.min(64) };

        // Desert battle ground background
        let sky_col = rgb565(180, 120, 70);
        let sand_col = rgb565(150, 90, 45);
        let sand_dark = rgb565(90, 50, 25);

        for y in 0..ground_top - 12 {
            for x in 0..w {
                cwscene::put(matrix, x, y, sky_col);
            }
        }
        for y in ground_top - 12..ground_top {
            for x in 0..w {
                cwscene::put(matrix, x, y, sand_col);
            }
        }
        for y in ground_top..h {
            for x in 0..w {
                cwscene::put(matrix, x, y, sand_dark);
            }
        }

        // Helicopter cruising in the sky
        Self::blit_sprite(
            matrix,
            &ENEMY_HELI_PAL[..],
            &ENEMY_HELI_PIXELS[..],
            ENEMY_HELI_W,
            ENEMY_HELI_H,
            self.heli_x as i32,
            if is_portrait { 36 } else { 16 },
            false,
        );

        // Tank rendering
        let tank_draw_y = ground_top - TANK_ALIVE_H;
        if self.phase == Phase::Explosion {
            Self::blit_sprite(
                matrix,
                &TANK_DEAD_PAL[..],
                &TANK_DEAD_PIXELS[..],
                TANK_DEAD_W,
                TANK_DEAD_H,
                self.tank_x as i32,
                ground_top - TANK_DEAD_H,
                false,
            );
        } else {
            Self::blit_sprite(
                matrix,
                &TANK_ALIVE_PAL[..],
                &TANK_ALIVE_PIXELS[..],
                TANK_ALIVE_W,
                TANK_ALIVE_H,
                self.tank_x as i32,
                tank_draw_y,
                false,
            );
        }

        // Marco Rossi rendering
        match self.phase {
            Phase::Idle => {
                Self::blit_sprite(
                    matrix,
                    &MARCO_IDLE_0_PAL[..],
                    &MARCO_IDLE_0_PIXELS[..],
                    MARCO_IDLE_0_W,
                    MARCO_IDLE_0_H,
                    self.marco_x as i32,
                    ground_top - MARCO_IDLE_0_H,
                    false,
                );
            }
            Phase::HeroEnter | Phase::HeroExit => {
                let (pal, pix, sw, sh) = if self.walk_frame % 2 == 0 {
                    (
                        &MARCO_WALK_0_PAL[..],
                        &MARCO_WALK_0_PIXELS[..],
                        MARCO_WALK_0_W,
                        MARCO_WALK_0_H,
                    )
                } else {
                    (
                        &MARCO_WALK_1_PAL[..],
                        &MARCO_WALK_1_PIXELS[..],
                        MARCO_WALK_1_W,
                        MARCO_WALK_1_H,
                    )
                };
                Self::blit_sprite(
                    matrix,
                    pal,
                    pix,
                    sw,
                    sh,
                    self.marco_x as i32,
                    ground_top - sh,
                    self.phase == Phase::HeroExit,
                );
            }
            Phase::Firefight => {
                let marco_y = ground_top - MARCO_SHOOT_0_H;
                let (pal, pix, sw, sh) = if self.anim_frame % 2 == 0 {
                    (
                        &MARCO_SHOOT_0_PAL[..],
                        &MARCO_SHOOT_0_PIXELS[..],
                        MARCO_SHOOT_0_W,
                        MARCO_SHOOT_0_H,
                    )
                } else {
                    (
                        &MARCO_SHOOT_1_PAL[..],
                        &MARCO_SHOOT_1_PIXELS[..],
                        MARCO_SHOOT_1_W,
                        MARCO_SHOOT_1_H,
                    )
                };
                Self::blit_sprite(
                    matrix,
                    pal,
                    pix,
                    sw,
                    sh,
                    self.marco_x as i32,
                    marco_y,
                    false,
                );

                // Bullet tracers
                let tracer_color = rgb565(255, 240, 60);
                let orange_core = rgb565(255, 120, 20);
                let muzzle_x = self.marco_x as i32 + MARCO_SHOOT_0_W;
                let muzzle_y = marco_y + 10;
                if self.anim_frame % 2 == 1 {
                    let tracer_len = if is_portrait { 18 } else { 40 };
                    for dx in 2..tracer_len {
                        cwscene::put(matrix, muzzle_x + dx, muzzle_y, tracer_color);
                        if dx + 2 < tracer_len {
                            cwscene::put(matrix, muzzle_x + dx + 2, muzzle_y + 1, orange_core);
                        }
                    }
                }
            }
            Phase::GrenadeAssault => {
                Self::blit_sprite(
                    matrix,
                    &MARCO_GRENADE_PAL[..],
                    &MARCO_GRENADE_PIXELS[..],
                    MARCO_GRENADE_W,
                    MARCO_GRENADE_H,
                    self.marco_x as i32,
                    ground_top - MARCO_GRENADE_H,
                    false,
                );
            }
            Phase::Explosion => {
                Self::blit_sprite(
                    matrix,
                    &MARCO_VICTORY_PAL[..],
                    &MARCO_VICTORY_PIXELS[..],
                    MARCO_VICTORY_W,
                    MARCO_VICTORY_H,
                    self.marco_x as i32,
                    ground_top - MARCO_VICTORY_H,
                    false,
                );
            }
        }

        // Flying grenade
        if self.grenade_active {
            let gx = self.grenade_x as i32;
            let gy = self.grenade_y as i32;
            let grenade_color = rgb565(180, 160, 40);
            let spark = rgb565(255, 220, 80);
            for dy in 0..3 {
                for dx in 0..3 {
                    cwscene::put(matrix, gx + dx, gy + dy, grenade_color);
                }
            }
            cwscene::put(matrix, gx + 1, gy + 1, spark);
        }

        // Explosions
        if self.phase == Phase::Explosion {
            let (exp_pal, exp_pix, ew, eh) = match self.explosion_frame % 4 {
                0 => (
                    &EXPLOSION_0_PAL[..],
                    &EXPLOSION_0_PIXELS[..],
                    EXPLOSION_0_W,
                    EXPLOSION_0_H,
                ),
                1 => (
                    &EXPLOSION_1_PAL[..],
                    &EXPLOSION_1_PIXELS[..],
                    EXPLOSION_1_W,
                    EXPLOSION_1_H,
                ),
                2 => (
                    &EXPLOSION_2_PAL[..],
                    &EXPLOSION_2_PIXELS[..],
                    EXPLOSION_2_W,
                    EXPLOSION_2_H,
                ),
                _ => (
                    &EXPLOSION_3_PAL[..],
                    &EXPLOSION_3_PIXELS[..],
                    EXPLOSION_3_W,
                    EXPLOSION_3_H,
                ),
            };
            let exp_x = if is_portrait {
                self.tank_x as i32 + 4
            } else {
                self.tank_x as i32 + 10
            };
            Self::blit_sprite(
                matrix,
                exp_pal,
                exp_pix,
                ew,
                eh,
                exp_x,
                ground_top - eh - 4,
                false,
            );
        }

        // SNK Neo Geo HUD
        let hud_bg = rgb565(8, 8, 16);
        let hud_yellow = rgb565(248, 208, 0);
        let hud_white = rgb565(255, 255, 255);
        let hud_red = rgb565(230, 40, 30);
        let hud_cyan = rgb565(40, 220, 240);

        let time_str = format!("{}:{}", self.shown_hh, self.shown_mm);

        if is_portrait {
            for y in 0..10 {
                for x in 0..w {
                    cwscene::put(matrix, x, y, hud_bg);
                }
            }
            for x in 0..w {
                cwscene::put(matrix, x, 10, rgb565(40, 40, 70));
            }
            if w >= 64 {
                Self::draw_arcade_digit(matrix, 4, 2, '1', hud_red, 1);
                let score_str = format!("00{}{}", self.shown_hh, self.shown_mm);
                let mut sx = 18;
                for c in score_str.chars() {
                    if sx >= w - 4 {
                        break;
                    }
                    Self::draw_arcade_digit(matrix, sx, 2, c, hud_white, 1);
                    sx += 4;
                }
                Self::draw_arcade_time(matrix, 10, 14, &time_str, hud_yellow, 2);
            } else {
                // 32-wide Tate (128x32 rotated: w=32, h=128)
                let time_x = (w - 18) / 2;
                Self::draw_arcade_time(matrix, time_x, 14, &self.shown_hh, hud_yellow, 2);
                Self::draw_arcade_time(matrix, time_x, 30, &self.shown_mm, hud_yellow, 2);
            }
        } else if h >= 48 {
            for y in 0..12 {
                for x in 0..w {
                    cwscene::put(matrix, x, y, hud_bg);
                }
            }
            for x in 0..w {
                cwscene::put(matrix, x, 12, rgb565(40, 40, 70));
            }
            Self::draw_arcade_digit(matrix, 6, 3, '1', hud_red, 1);
            let score_str = format!("00{}{}", self.shown_hh, self.shown_mm);
            let mut sx = 20;
            for c in score_str.chars() {
                Self::draw_arcade_digit(matrix, sx, 3, c, hud_white, 1);
                sx += 4;
            }
            let time_x = ((w - 44) / 2).max(60);
            Self::draw_arcade_time(matrix, time_x, 1, &time_str, hud_yellow, 2);

            // Arms indicator
            Self::draw_arcade_digit(matrix, (w - 70).max(10), 3, '4', hud_cyan, 1);
        } else {
            // 128x32 compact HUD
            for y in 0..9 {
                for x in 0..w {
                    cwscene::put(matrix, x, y, hud_bg);
                }
            }
            Self::draw_arcade_time(matrix, 6, 2, &time_str, hud_yellow, 1);
        }
    }

    pub fn render(&mut self, matrix: &mut dyn IDrawingSurface, hours: u32, minutes: u32) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;
        let is_portrait = h > w;
        let target_tank_x = if is_portrait {
            (w - 34) as f32
        } else {
            (w - 70).max(40) as f32
        };
        let target_fire_x = if is_portrait { 2.0 } else { 35.0 };

        let dt = self.last_frame.elapsed().as_secs_f32().min(0.2);
        self.last_frame = Instant::now();

        let hh = format!("{:02}", hours);
        let mm = format!("{:02}", minutes);
        if self.shown_hh == "--" {
            self.shown_hh = hh.clone();
            self.shown_mm = mm.clone();
            self.tank_x = target_tank_x;
            self.dirty = 3;
        }

        // Minute change combat sequence
        if minutes as i32 != self.last_minute && self.phase == Phase::Idle {
            self.last_minute = minutes as i32;
            self.phase = Phase::HeroEnter;
            self.phase_timer = 0.0;
            self.marco_x = -30.0;
            self.tank_x = target_tank_x;
            self.grenade_active = false;
            self.dirty = 3;
        }

        self.anim_timer += dt;
        if self.anim_timer >= 0.12 {
            self.anim_timer = 0.0;
            self.anim_frame += 1;
        }

        // Helicopter cruising
        let heli_max = if is_portrait {
            w as f32 + 50.0
        } else {
            (w + 40) as f32
        };
        self.heli_x += 22.0 * dt;
        if self.heli_x > heli_max {
            self.heli_x = -60.0;
        }

        // Combat sequence state machine
        match self.phase {
            Phase::Idle => {
                self.marco_x = target_fire_x;
            }
            Phase::HeroEnter => {
                self.walk_timer += dt;
                if self.walk_timer >= 0.10 {
                    self.walk_timer = 0.0;
                    self.walk_frame += 1;
                }
                self.marco_x += 42.0 * dt;
                if self.marco_x >= target_fire_x {
                    self.marco_x = target_fire_x;
                    self.phase = Phase::Firefight;
                    self.phase_timer = 0.0;
                }
                self.dirty = self.dirty.max(2);
            }
            Phase::Firefight => {
                self.phase_timer += dt;
                if self.phase_timer >= 1.2 {
                    self.phase = Phase::GrenadeAssault;
                    self.phase_timer = 0.0;
                    self.grenade_active = true;
                    self.grenade_t = 0.0;
                    self.grenade_x = self.marco_x + 18.0;
                    self.grenade_y = (h - 30) as f32;
                }
                self.dirty = self.dirty.max(2);
            }
            Phase::GrenadeAssault => {
                self.grenade_t += dt * 2.2;
                let t = self.grenade_t.min(1.0);
                let start_x = self.marco_x + 18.0;
                let target_x = self.tank_x + 10.0;
                let ground_y = (if is_portrait { h - 36 } else { h.min(64) }) as f32 - 10.0;

                self.grenade_x = start_x + (target_x - start_x) * t;
                let arc = 4.0 * 20.0 * t * (1.0 - t);
                self.grenade_y = ground_y - arc;

                if self.grenade_t >= 1.0 {
                    self.grenade_active = false;
                    self.phase = Phase::Explosion;
                    self.phase_timer = 0.0;
                    self.explosion_frame = 0;
                    self.explosion_timer = 0.0;
                    // Flip time upon grenade explosion!
                    self.shown_hh = hh.clone();
                    self.shown_mm = mm.clone();
                }
                self.dirty = self.dirty.max(2);
            }
            Phase::Explosion => {
                self.explosion_timer += dt;
                if self.explosion_timer >= 0.15 {
                    self.explosion_timer = 0.0;
                    self.explosion_frame += 1;
                }
                self.phase_timer += dt;
                if self.phase_timer >= 1.4 {
                    self.phase = Phase::HeroExit;
                    self.phase_timer = 0.0;
                }
                self.dirty = self.dirty.max(2);
            }
            Phase::HeroExit => {
                self.walk_timer += dt;
                if self.walk_timer >= 0.10 {
                    self.walk_timer = 0.0;
                    self.walk_frame += 1;
                }
                self.marco_x -= 42.0 * dt;
                if self.marco_x < -40.0 {
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

impl Default for MetalSlugClock {
    fn default() -> Self {
        Self::new()
    }
}

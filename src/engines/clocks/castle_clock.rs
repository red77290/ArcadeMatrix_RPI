//! Authentic Konami Castlevania (NES) Clock Face for ArcadeMatrix.
//!
//! Features:
//! - Simon Belmont walking and whip attack animation on minute flip.
//! - Blood Moon with flying vampire bat flapping across the moon.
//! - Animated candle torch with 3-frame flickering flame.
//! - Gothic 3x5 font for time display.
//! - Full support for Tate/portrait mode (multi-tier castle platforms), wide/tall 256x64, and compact 128x32.
//! - Zero-legacy canonical `IDrawingSurface`.

use crate::core::matrix::IDrawingSurface;
use crate::engines::clocks::cwassets::castlevania::*;
use crate::engines::clocks::cwscene;
use std::time::Instant;

#[derive(PartialEq, Clone, Copy, Debug)]
enum Phase {
    Idle,
    HeroEnter,
    WhipWindup,
    WhipStrike,
    HeroExit,
}

#[inline]
fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 & 0xF8) << 8) | ((g as u16 & 0xFC) << 3) | ((b as u16) >> 3)
}

pub struct CastleClock {
    phase: Phase,
    phase_timer: f32,
    walk_timer: f32,
    walk_frame: usize,
    flame_timer: f32,
    flame_frame: usize,
    bat_timer: f32,
    bat_frame: usize,
    bat_x: f32,
    simon_x: f32,
    simon_y: f32,
    climb_stage: u8,
    jump_t: f32,
    shown_hh: String,
    shown_mm: String,
    last_minute: i32,
    last_frame: Instant,
    dirty: u8,
}

impl CastleClock {
    pub fn new() -> Self {
        Self {
            phase: Phase::Idle,
            phase_timer: 0.0,
            walk_timer: 0.0,
            walk_frame: 0,
            flame_timer: 0.0,
            flame_frame: 0,
            bat_timer: 0.0,
            bat_frame: 0,
            bat_x: 64.0,
            simon_x: -40.0,
            simon_y: 0.0,
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
                let idx = pixels[(row * w + src_col) as usize] as usize;
                if idx == 0 || idx >= palette.len() {
                    continue;
                }
                cwscene::put(matrix, px, py, palette[idx]);
            }
        }
    }

    fn draw_gothic_digit(
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
            let row_bits = GOTHIC_FONT_3x5[idx][r];
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

    fn draw_gothic_time(
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

        Self::draw_gothic_digit(matrix, x_h1, start_y, chars[0], color, scale);
        Self::draw_gothic_digit(matrix, x_h2, start_y, chars[1], color, scale);
        Self::draw_gothic_digit(matrix, x_colon, start_y, chars[2], color, scale);
        Self::draw_gothic_digit(matrix, x_m1, start_y, chars[3], color, scale);
        Self::draw_gothic_digit(matrix, x_m2, start_y, chars[4], color, scale);
    }

    fn draw_circle_fill(matrix: &mut dyn IDrawingSurface, cx: i32, cy: i32, r: i32, color: u16) {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r {
                    cwscene::put(matrix, cx + dx, cy + dy, color);
                }
            }
        }
    }

    fn draw_scene(&self, matrix: &mut dyn IDrawingSurface, w: i32, h: i32) {
        let is_portrait = h > w;

        if is_portrait {
            // Dracula Castle Tower in Tate Mode
            let night_sky = rgb565(16, 0, 24);
            for y in 0..h {
                for x in 0..w {
                    cwscene::put(matrix, x, y, night_sky);
                }
            }

            // Blood Moon
            let moon_outer = rgb565(220, 40, 20);
            let moon_inner = rgb565(240, 80, 40);
            Self::draw_circle_fill(matrix, 32, 14, 11, moon_outer);
            Self::draw_circle_fill(matrix, 31, 13, 8, moon_inner);

            // Flying bat across blood moon
            let bat_pixels = if self.bat_frame % 2 == 0 {
                &BAT_F0_PIXELS[..]
            } else {
                &BAT_F1_PIXELS[..]
            };
            let bat_pal = if self.bat_frame % 2 == 0 {
                &BAT_F0_PAL[..]
            } else {
                &BAT_F1_PAL[..]
            };
            Self::blit_sprite(
                matrix,
                bat_pal,
                bat_pixels,
                BAT_F0_W,
                BAT_F0_H,
                self.bat_x as i32,
                8,
                false,
            );

            // Stone cornice at y=56
            let stone_base = rgb565(80, 80, 96);
            let stone_light = rgb565(140, 140, 160);
            let stone_dark = rgb565(36, 36, 48);

            for x in 0..w {
                cwscene::put(matrix, x, 56, stone_light);
                for dy in 57..61 {
                    cwscene::put(matrix, x, dy, stone_base);
                }
            }
            let mut sx = 0;
            while sx < w {
                for dy in 57..61 {
                    cwscene::put(matrix, sx, dy, stone_dark);
                }
                sx += 16;
            }

            // Platforms at y=190, 130, 70
            let platforms = [(16, 190, 40), (8, 130, 40), (8, 70, 48)];
            for (px, py, pw) in platforms {
                for x in px..px + pw {
                    for dy in py..py + 5 {
                        cwscene::put(matrix, x, dy, stone_base);
                    }
                    cwscene::put(matrix, x, py, stone_light);
                }
            }

            // Candle stand on Platform 1 (x=46, y=56..69)
            let torch_metal = rgb565(140, 100, 50);
            for y in 56..70 {
                for x in 46..50 {
                    cwscene::put(matrix, x, y, torch_metal);
                }
            }
            for y in 52..56 {
                for x in 44..52 {
                    cwscene::put(matrix, x, y, torch_metal);
                }
            }

            // Animated candle flame
            let flame_pal = match self.flame_frame % 3 {
                0 => &FLAME_F0_PAL[..],
                1 => &FLAME_F1_PAL[..],
                _ => &FLAME_F2_PAL[..],
            };
            let flame_pix = match self.flame_frame % 3 {
                0 => &FLAME_F0_PIXELS[..],
                1 => &FLAME_F1_PIXELS[..],
                _ => &FLAME_F2_PIXELS[..],
            };
            Self::blit_sprite(
                matrix, flame_pal, flame_pix, FLAME_F0_W, FLAME_F0_H, 44, 42, false,
            );
        } else if h >= 48 {
            // Dracula's Castle Parapets (256x64)
            let night_sky = rgb565(16, 0, 24);
            for y in 0..48 {
                for x in 0..w {
                    cwscene::put(matrix, x, y, night_sky);
                }
            }

            // Blood Moon
            let moon_outer = rgb565(220, 40, 20);
            let moon_inner = rgb565(240, 80, 40);
            Self::draw_circle_fill(matrix, (w - 40).max(40), 22, 18, moon_outer);
            Self::draw_circle_fill(matrix, (w - 42).max(38), 20, 15, moon_inner);

            // Parapets at y=48..63
            let stone_base = rgb565(80, 80, 96);
            let stone_light = rgb565(140, 140, 160);
            let stone_dark = rgb565(36, 36, 48);

            for y in 48..h {
                for x in 0..w {
                    cwscene::put(matrix, x, y, stone_base);
                }
            }
            for x in 0..w {
                cwscene::put(matrix, x, 48, stone_light);
            }
            let mut sx = 0;
            while sx < w {
                for y in 48..h {
                    cwscene::put(matrix, sx, y, stone_dark);
                    cwscene::put(matrix, sx + 1, y, stone_light);
                }
                sx += 16;
            }

            // Candle stand on right
            let torch_metal = rgb565(140, 100, 50);
            let torch_x = (w - 16).max(10);
            for y in 34..48 {
                for x in torch_x..torch_x + 4 {
                    cwscene::put(matrix, x, y, torch_metal);
                }
            }
            for y in 30..34 {
                for x in torch_x - 2..torch_x + 6 {
                    cwscene::put(matrix, x, y, torch_metal);
                }
            }

            // Torch flame
            let flame_pal = match self.flame_frame % 3 {
                0 => &FLAME_F0_PAL[..],
                1 => &FLAME_F1_PAL[..],
                _ => &FLAME_F2_PAL[..],
            };
            let flame_pix = match self.flame_frame % 3 {
                0 => &FLAME_F0_PIXELS[..],
                1 => &FLAME_F1_PIXELS[..],
                _ => &FLAME_F2_PIXELS[..],
            };
            Self::blit_sprite(
                matrix,
                flame_pal,
                flame_pix,
                FLAME_F0_W,
                FLAME_F0_H,
                torch_x - 2,
                20,
                false,
            );

            // Flying Bat
            let bat_pixels = if self.bat_frame % 2 == 0 {
                &BAT_F0_PIXELS[..]
            } else {
                &BAT_F1_PIXELS[..]
            };
            let bat_pal = if self.bat_frame % 2 == 0 {
                &BAT_F0_PAL[..]
            } else {
                &BAT_F1_PAL[..]
            };
            Self::blit_sprite(
                matrix,
                bat_pal,
                bat_pixels,
                BAT_F0_W,
                BAT_F0_H,
                self.bat_x as i32,
                10,
                false,
            );
        } else {
            // 128x32 Dungeon floor
            for y in 0..26 {
                for x in 0..w {
                    cwscene::put(matrix, x, y, 0x0000);
                }
            }
            let brick_col = rgb565(116, 40, 12);
            let brick_mortar = rgb565(40, 16, 8);
            for y in 26..h {
                for x in 0..w {
                    cwscene::put(matrix, x, y, brick_col);
                }
            }
            for x in 0..w {
                cwscene::put(matrix, x, 26, rgb565(180, 70, 20));
            }
            let mut sx = 0;
            while sx < w {
                for y in 26..h {
                    cwscene::put(matrix, sx, y, brick_mortar);
                }
                sx += 8;
            }

            // Candle Torch on right
            let torch_x = (w - 12).max(10);
            for y in 20..26 {
                for x in torch_x..torch_x + 2 {
                    cwscene::put(matrix, x, y, rgb565(140, 100, 50));
                }
            }
            let flame_pal = match self.flame_frame % 3 {
                0 => &FLAME_F0_PAL[..],
                1 => &FLAME_F1_PAL[..],
                _ => &FLAME_F2_PAL[..],
            };
            let flame_pix = match self.flame_frame % 3 {
                0 => &FLAME_F0_PIXELS[..],
                1 => &FLAME_F1_PIXELS[..],
                _ => &FLAME_F2_PIXELS[..],
            };
            Self::blit_sprite(
                matrix,
                flame_pal,
                flame_pix,
                FLAME_F0_W,
                FLAME_F0_H,
                torch_x - 3,
                10,
                false,
            );

            // Flying Bat
            let bat_pixels = if self.bat_frame % 2 == 0 {
                &BAT_F0_PIXELS[..]
            } else {
                &BAT_F1_PIXELS[..]
            };
            let bat_pal = if self.bat_frame % 2 == 0 {
                &BAT_F0_PAL[..]
            } else {
                &BAT_F1_PAL[..]
            };
            Self::blit_sprite(
                matrix,
                bat_pal,
                bat_pixels,
                BAT_F0_W,
                BAT_F0_H,
                self.bat_x as i32,
                4,
                false,
            );
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
        if self.shown_hh == "--" {
            self.shown_hh = hh.clone();
            self.shown_mm = mm.clone();
            self.dirty = 3;
        }

        // Minute change trigger
        if minutes as i32 != self.last_minute && self.phase == Phase::Idle {
            self.last_minute = minutes as i32;
            self.phase = Phase::HeroEnter;
            self.phase_timer = 0.0;
            if is_portrait {
                self.simon_x = -16.0;
                self.simon_y = (h - 8 - 30) as f32;
                self.climb_stage = 0;
                self.jump_t = 0.0;
            } else {
                self.simon_x = -20.0;
                self.simon_y = if h >= 48 { (48 - 30) as f32 } else { 0.0 };
            }
            self.dirty = 3;
        }

        // Flame animation (120ms)
        self.flame_timer += dt;
        if self.flame_timer >= 0.12 {
            self.flame_timer = 0.0;
            self.flame_frame = (self.flame_frame + 1) % 3;
            self.dirty = self.dirty.max(2);
        }

        // Bat flight
        self.bat_timer += dt;
        if self.bat_timer >= 0.14 {
            self.bat_timer = 0.0;
            self.bat_frame = (self.bat_frame + 1) % 2;
        }
        self.bat_x -= 35.0 * dt;
        if self.bat_x < -20.0 {
            self.bat_x = w as f32 + 10.0;
        }

        // Simon movement & attack state machine
        let target_x = if is_portrait {
            14.0
        } else if h >= 48 {
            (w - 70).max(30) as f32
        } else {
            (w - 55).max(20) as f32
        };

        match self.phase {
            Phase::Idle => {}
            Phase::HeroEnter => {
                self.walk_timer += dt;
                if self.walk_timer >= 0.12 {
                    self.walk_timer = 0.0;
                    self.walk_frame = (self.walk_frame + 1) % 3;
                }
                self.simon_x += 45.0 * dt;
                if self.simon_x >= target_x {
                    self.simon_x = target_x;
                    self.phase = Phase::WhipWindup;
                    self.phase_timer = 0.0;
                }
                self.dirty = self.dirty.max(2);
            }
            Phase::WhipWindup => {
                self.phase_timer += dt;
                if self.phase_timer >= 0.22 {
                    self.phase = Phase::WhipStrike;
                    self.phase_timer = 0.0;
                    // Minute strike flips digits!
                    self.shown_hh = hh.clone();
                    self.shown_mm = mm.clone();
                }
                self.dirty = self.dirty.max(2);
            }
            Phase::WhipStrike => {
                self.phase_timer += dt;
                if self.phase_timer >= 0.35 {
                    self.phase = Phase::HeroExit;
                    self.phase_timer = 0.0;
                }
                self.dirty = self.dirty.max(2);
            }
            Phase::HeroExit => {
                self.walk_timer += dt;
                if self.walk_timer >= 0.12 {
                    self.walk_timer = 0.0;
                    self.walk_frame = (self.walk_frame + 1) % 3;
                }
                self.simon_x -= 45.0 * dt;
                if self.simon_x < -35.0 {
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

        // Time display
        let time_str = format!("{}:{}", self.shown_hh, self.shown_mm);
        let gold_col = rgb565(255, 200, 40);

        if is_portrait {
            Self::draw_gothic_time(matrix, 6, 28, &time_str, gold_col, 2);
        } else if h >= 48 {
            Self::draw_gothic_time(matrix, 20, 14, &time_str, gold_col, 2);
        } else {
            Self::draw_gothic_time(matrix, 6, 6, &time_str, gold_col, 1);
        }

        // Simon Belmont rendering
        let simon_draw_y = if is_portrait {
            (h - 8 - 30) as i32
        } else if h >= 48 {
            (48 - 30) as i32
        } else {
            (26 - 30 + 6) as i32
        };

        match self.phase {
            Phase::Idle => {}
            Phase::HeroEnter => {
                let pix = match self.walk_frame % 3 {
                    0 => &SIMON_WALK_0_PIXELS[..],
                    1 => &SIMON_WALK_1_PIXELS[..],
                    _ => &SIMON_WALK_2_PIXELS[..],
                };
                Self::blit_sprite(
                    matrix,
                    &SIMON_IDLE_PAL[..],
                    pix,
                    SIMON_WALK_0_W,
                    SIMON_WALK_0_H,
                    self.simon_x as i32,
                    simon_draw_y,
                    false,
                );
            }
            Phase::WhipWindup => {
                Self::blit_sprite(
                    matrix,
                    &SIMON_IDLE_PAL[..],
                    &SIMON_WHIP_WINDUP_PIXELS[..],
                    SIMON_WHIP_WINDUP_W,
                    SIMON_WHIP_WINDUP_H,
                    self.simon_x as i32 - 4,
                    simon_draw_y,
                    false,
                );
            }
            Phase::WhipStrike => {
                Self::blit_sprite(
                    matrix,
                    &SIMON_IDLE_PAL[..],
                    &SIMON_WHIP_STRIKE_PIXELS[..],
                    SIMON_WHIP_STRIKE_W,
                    SIMON_WHIP_STRIKE_H,
                    self.simon_x as i32,
                    simon_draw_y,
                    false,
                );
                // Draw extended leather whip
                Self::blit_sprite(
                    matrix,
                    &SIMON_IDLE_PAL[..],
                    &WHIP_EXTENDED_PIXELS[..],
                    WHIP_EXTENDED_W,
                    WHIP_EXTENDED_H,
                    self.simon_x as i32 + SIMON_WHIP_STRIKE_W - 2,
                    simon_draw_y + 11,
                    false,
                );
            }
            Phase::HeroExit => {
                let pix = match self.walk_frame % 3 {
                    0 => &SIMON_WALK_0_PIXELS[..],
                    1 => &SIMON_WALK_1_PIXELS[..],
                    _ => &SIMON_WALK_2_PIXELS[..],
                };
                Self::blit_sprite(
                    matrix,
                    &SIMON_IDLE_PAL[..],
                    pix,
                    SIMON_WALK_0_W,
                    SIMON_WALK_0_H,
                    self.simon_x as i32,
                    simon_draw_y,
                    true,
                );
            }
        }
    }
}

impl Default for CastleClock {
    fn default() -> Self {
        Self::new()
    }
}

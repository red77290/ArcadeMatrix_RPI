use crate::core::matrix::IDrawingSurface as Matrix;
use crate::engines::clocks::cwassets::mario::*;
use crate::engines::renderers::base_renderer::ArcadeFont;
use std::time::Instant;

#[derive(PartialEq, Clone, Copy)]
enum Phase {
    Waiting,
    RunIn,
    Jump,
    Kick,
    RunOut,
}

#[rustfmt::skip]
const BLOCK_14X14: [u16; 196] = [
    0x000E, 0x9A40, 0x9A40, 0x9A40, 0x9A40, 0x9A40, 0x9A40, 0x9A40, 0x9A40, 0x9A40, 0x9A40, 0x9A40, 0x9A40, 0x000E,
    0x9A40, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0x0000,
    0x9A40, 0xE4E4, 0x0000, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0x0000, 0xE4E4, 0x0000,
    0x9A40, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0x0000,
    0x9A40, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0x0000,
    0x9A40, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0x0000,
    0x9A40, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0x0000,
    0x9A40, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0x0000,
    0x9A40, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0x0000,
    0x9A40, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0x0000,
    0x9A40, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0x0000,
    0x9A40, 0xE4E4, 0x0000, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0x0000, 0xE4E4, 0x0000,
    0x9A40, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0xE4E4, 0x0000,
    0x000E, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x000E,
];

const DIGITS_4X6: [[u8; 6]; 10] = [
    [0x6, 0x9, 0x9, 0x9, 0x9, 0x6], // 0
    [0x2, 0x6, 0x2, 0x2, 0x2, 0x7], // 1
    [0x6, 0x9, 0x1, 0x2, 0x4, 0xF], // 2
    [0xE, 0x1, 0x6, 0x1, 0x1, 0xE], // 3
    [0x9, 0x9, 0xF, 0x1, 0x1, 0x1], // 4
    [0xF, 0x8, 0xE, 0x1, 0x1, 0xE], // 5
    [0x6, 0x8, 0xE, 0x9, 0x9, 0x6], // 6
    [0xF, 0x1, 0x2, 0x2, 0x4, 0x4], // 7
    [0x6, 0x9, 0x6, 0x9, 0x9, 0x6], // 8
    [0x6, 0x9, 0x9, 0x7, 0x1, 0x6], // 9
];

/// Exact glyph bitmaps for '0'..'9' from `Super_Mario_Bros__24pt7b` (`MarioFont.h`).
struct MarioDigitGlyph {
    width: u8,
    height: u8,
    x_advance: i32,
    x_offset: i32,
    y_offset: i32,
    bytes: &'static [u8],
}

const MARIO_DIGIT_GLYPHS: [MarioDigitGlyph; 10] = [
    MarioDigitGlyph {
        width: 7,
        height: 7,
        x_advance: 8,
        x_offset: 0,
        y_offset: -6,
        bytes: &[0x7D, 0x9F, 0x3E, 0x7C, 0xF9, 0xDF, 0x00],
    }, // '0'
    MarioDigitGlyph {
        width: 5,
        height: 7,
        x_advance: 8,
        x_offset: 1,
        y_offset: -6,
        bytes: &[0x77, 0x9C, 0xE7, 0x3B, 0xE0],
    }, // '1'
    MarioDigitGlyph {
        width: 7,
        height: 7,
        x_advance: 8,
        x_offset: 0,
        y_offset: -6,
        bytes: &[0x7D, 0x9C, 0x39, 0xE7, 0x1C, 0x3F, 0x80],
    }, // '2'
    MarioDigitGlyph {
        width: 7,
        height: 7,
        x_advance: 8,
        x_offset: 0,
        y_offset: -6,
        bytes: &[0x7D, 0x9C, 0x38, 0x7C, 0xF9, 0xDF, 0x00],
    }, // '3'
    MarioDigitGlyph {
        width: 7,
        height: 7,
        x_advance: 8,
        x_offset: 0,
        y_offset: -6,
        bytes: &[0x39, 0xE7, 0x7F, 0xBE, 0x0C, 0x18, 0x00],
    }, // '4'
    MarioDigitGlyph {
        width: 7,
        height: 7,
        x_advance: 8,
        x_offset: 0,
        y_offset: -6,
        bytes: &[0xFF, 0x83, 0xF1, 0xFC, 0xF9, 0xDF, 0x00],
    }, // '5'
    MarioDigitGlyph {
        width: 7,
        height: 7,
        x_advance: 8,
        x_offset: 0,
        y_offset: -6,
        bytes: &[0x7D, 0x9E, 0x1F, 0x7C, 0xF9, 0xDF, 0x00],
    }, // '6'
    MarioDigitGlyph {
        width: 7,
        height: 7,
        x_advance: 8,
        x_offset: 0,
        y_offset: -6,
        bytes: &[0xFF, 0x1C, 0x39, 0xC7, 0x1C, 0x38, 0x00],
    }, // '7'
    MarioDigitGlyph {
        width: 7,
        height: 7,
        x_advance: 8,
        x_offset: 0,
        y_offset: -6,
        bytes: &[0x7D, 0x9F, 0x3C, 0xFC, 0xF9, 0xDF, 0x00],
    }, // '8'
    MarioDigitGlyph {
        width: 7,
        height: 7,
        x_advance: 8,
        x_offset: 0,
        y_offset: -6,
        bytes: &[0x7D, 0x9F, 0x3E, 0x7D, 0xF0, 0xDF, 0x00],
    }, // '9'
];

const BLOCK_W: i32 = 19;
const BLOCK_H: i32 = 19;
const GROUND_W: i32 = 8;
const GROUND_H: i32 = 8;
const HILL_W: i32 = 20;
const HILL_H: i32 = 22;
const BUSH_W: i32 = 21;
const BUSH_H: i32 = 9;
const CLOUD_W: i32 = 13;
const CLOUD_H: i32 = 12;
const CLOUD_FULL_W: i32 = 19;
const CLOUD_FULL_H: i32 = 12;
const MARIO_W: i32 = 13;
const MARIO_H: i32 = 16;
const MARIO_JUMP_W: i32 = 17;
const SCENE: i32 = 64;

#[inline]
fn rgb565_to_rgb888(col: u16) -> (u8, u8, u8) {
    let r5 = ((col >> 11) & 0x1F) as u32;
    let g6 = ((col >> 5) & 0x3F) as u32;
    let b5 = (col & 0x1F) as u32;
    (
        ((r5 * 255 + 15) / 31) as u8,
        ((g6 * 255 + 31) / 63) as u8,
        ((b5 * 255 + 15) / 31) as u8,
    )
}

pub struct MarioClock {
    phase: Phase,
    runner_x: f32,
    runner_y: f32,
    climb_stage: u8,
    jump_t: f32,
    jump_target: usize,
    block_bounce: [f32; 2],
    pending_digits: bool,
    shown_hh: String,
    shown_mm: String,
    last_minute: i32,
    shell_x: f32,
    shell_dir: f32,
    jump_start_x: f32,
    jump_target_x: f32,
    coin_pop: bool,
    snap_to_now: bool,
    last_frame_ms: Instant,
    speed_pct: i32,
}

impl Default for MarioClock {
    fn default() -> Self {
        Self::new()
    }
}

impl MarioClock {
    pub fn new() -> Self {
        Self {
            phase: Phase::Waiting,
            runner_x: -40.0,
            runner_y: 0.0,
            climb_stage: 0,
            jump_t: 0.0,
            jump_target: 1,
            block_bounce: [0.0, 0.0],
            pending_digits: false,
            shown_hh: "--".to_string(),
            shown_mm: "--".to_string(),
            last_minute: -1,
            shell_x: 40.0,
            shell_dir: 1.0,
            jump_start_x: 0.0,
            jump_target_x: 0.0,
            coin_pop: false,
            snap_to_now: true,
            last_frame_ms: Instant::now(),
            speed_pct: 100,
        }
    }

    pub fn configure(&mut self, clock_speed: i32) {
        self.speed_pct = clock_speed.clamp(25, 300);
    }

    pub fn on_activated(&mut self) {
        self.snap_to_now = true;
    }

    pub fn on_display_geometry_changed(&mut self) {
        self.phase = Phase::Waiting;
        self.last_minute = -1;
        self.shell_x = 40.0;
        self.shell_dir = 1.0;
        self.runner_x = -40.0;
        self.coin_pop = false;
        self.snap_to_now = true;
    }

    fn blit_sprite(
        matrix: &mut dyn Matrix,
        data: &[u16],
        w: i32,
        h: i32,
        x: i32,
        y: i32,
        transparent: bool,
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
                let px = x + if flip_h { w - 1 - col } else { col };
                if px < 0 || px >= panel_w {
                    continue;
                }
                let Some(&c) = data.get((row * w + col) as usize) else {
                    continue;
                };
                if transparent && c == SKY_COLOR {
                    continue;
                }
                let (r, g, b) = rgb565_to_rgb888(c);
                matrix.set_pixel(px, py, r, g, b);
            }
        }
    }

    fn draw_digit_4x6(matrix: &mut dyn Matrix, px: i32, py: i32, c: char, color: (u8, u8, u8)) {
        if !c.is_ascii_digit() {
            return;
        }
        let rows = &DIGITS_4X6[(c as u8 - b'0') as usize];
        for (r, &row) in rows.iter().enumerate() {
            for b in 0..4 {
                if (row & (0x8 >> b)) != 0 {
                    matrix.set_pixel(px + b, py + r as i32, color.0, color.1, color.2);
                }
            }
        }
    }

    fn draw_mario_font_digits(matrix: &mut dyn Matrix, cursor_x: i32, baseline_y: i32, s: &str) {
        let mut cx = cursor_x;
        for ch in s.chars() {
            if !ch.is_ascii_digit() {
                continue;
            }
            let glyph = &MARIO_DIGIT_GLYPHS[(ch as u8 - b'0') as usize];
            let mut bit_idx = 0usize;
            let mut bits = 0u8;
            for yy in 0..glyph.height as i32 {
                for xx in 0..glyph.width as i32 {
                    if (bit_idx & 7) == 0 {
                        bits = glyph.bytes.get(bit_idx >> 3).copied().unwrap_or(0);
                    }
                    bit_idx += 1;
                    if (bits & 0x80) != 0 {
                        // Black digits (0x0000) inside the golden block, matching MarioClock.cpp:113
                        matrix.set_pixel(
                            cx + glyph.x_offset + xx,
                            baseline_y + glyph.y_offset + yy,
                            0,
                            0,
                            0,
                        );
                    }
                    bits <<= 1;
                }
            }
            cx += glyph.x_advance;
        }
    }

    fn draw_block_at(matrix: &mut dyn Matrix, x: i32, y: i32, text: &str, downscaled: bool) {
        if downscaled {
            Self::blit_sprite(matrix, &BLOCK_14X14, 14, 14, x, y, true, false);
            let chars: Vec<char> = text.chars().collect();
            if chars.len() == 1 {
                Self::draw_digit_4x6(matrix, x + 5, y + 4, chars[0], (0, 0, 0));
            } else if chars.len() >= 2 {
                Self::draw_digit_4x6(matrix, x + 2, y + 4, chars[0], (0, 0, 0));
                Self::draw_digit_4x6(matrix, x + 7, y + 4, chars[1], (0, 0, 0));
            }
        } else {
            Self::blit_sprite(matrix, &BLOCK, BLOCK_W, BLOCK_H, x, y, false, false);
            let cursor_x = x + if text.len() == 1 { 6 } else { 2 };
            let baseline_y = y + 12;
            Self::draw_mario_font_digits(matrix, cursor_x, baseline_y, text);
        }
    }

    fn draw_scene(matrix: &mut dyn Matrix, w: i32, h: i32) {
        let (sr, sg, sb) = rgb565_to_rgb888(SKY_COLOR);
        matrix.fill_rect(0, 0, w, h, sr, sg, sb);

        let is_portrait = h > w;
        let ground_top = h - GROUND_H;
        let mut x = 0;
        while x < w {
            Self::blit_sprite(
                matrix, &GROUND, GROUND_W, GROUND_H, x, ground_top, false, false,
            );
            x += GROUND_W;
        }

        if is_portrait {
            if w >= 64 {
                let mut px = 8;
                while px < 56 {
                    Self::blit_sprite(matrix, &GROUND, GROUND_W, GROUND_H, px, 70, false, false);
                    px += GROUND_W;
                }
                let mut px = 16;
                while px < 48 {
                    Self::blit_sprite(matrix, &GROUND, GROUND_W, GROUND_H, px, 130, false, false);
                    px += GROUND_W;
                }
                let mut px = 8;
                while px < 56 {
                    Self::blit_sprite(matrix, &GROUND, GROUND_W, GROUND_H, px, 190, false, false);
                    px += GROUND_W;
                }
                Self::blit_sprite(matrix, &CLOUD1, CLOUD_W, CLOUD_H, 4, 4, true, false);
                Self::blit_sprite(
                    matrix,
                    &CLOUD2,
                    CLOUD_W,
                    CLOUD_H,
                    w - CLOUD_W - 4,
                    10,
                    true,
                    false,
                );
            } else {
                let mut px = 0;
                while px < w {
                    Self::blit_sprite(matrix, &GROUND, GROUND_W, GROUND_H, px, 56, false, false);
                    px += GROUND_W;
                }
                let mut px = 8;
                while px < w {
                    Self::blit_sprite(matrix, &GROUND, GROUND_W, GROUND_H, px, 88, false, false);
                    px += GROUND_W;
                }
                Self::blit_sprite(matrix, &CLOUD1, CLOUD_W, CLOUD_H, 2, 2, true, false);
                Self::blit_sprite(
                    matrix,
                    &CLOUD2,
                    CLOUD_W,
                    CLOUD_H,
                    w - CLOUD_W - 1,
                    6,
                    true,
                    false,
                );
            }
            return;
        }

        if h >= 48 {
            Self::blit_sprite(
                matrix,
                &HILL,
                HILL_W,
                HILL_H,
                0,
                ground_top - HILL_H + 2,
                true,
                false,
            );
            Self::blit_sprite(
                matrix,
                &BUSH,
                BUSH_W,
                BUSH_H,
                w - BUSH_W,
                ground_top - BUSH_H,
                true,
                false,
            );
        }

        Self::blit_sprite(
            matrix,
            &CLOUD1,
            CLOUD_W,
            CLOUD_H,
            0,
            if h >= 48 { 14 } else { 2 },
            true,
            false,
        );
        Self::blit_sprite(
            matrix,
            &CLOUD2,
            CLOUD_W,
            CLOUD_H,
            w - CLOUD_W,
            if h >= 48 { 6 } else { 2 },
            true,
            false,
        );

        if w >= 128 && h >= 48 {
            Self::blit_sprite(
                matrix,
                &CLOUD_FULL,
                CLOUD_FULL_W,
                CLOUD_FULL_H,
                40,
                6,
                true,
                false,
            );
            Self::blit_sprite(
                matrix,
                &CLOUD_FULL,
                CLOUD_FULL_W,
                CLOUD_FULL_H,
                72,
                14,
                true,
                false,
            );
        }
        if w >= 256 && h >= 48 {
            Self::blit_sprite(
                matrix,
                &CLOUD_FULL,
                CLOUD_FULL_W,
                CLOUD_FULL_H,
                165,
                6,
                true,
                false,
            );
            Self::blit_sprite(
                matrix,
                &CLOUD_FULL,
                CLOUD_FULL_W,
                CLOUD_FULL_H,
                198,
                14,
                true,
                false,
            );
        }
    }

    pub fn render(
        &mut self,
        matrix: &mut dyn Matrix,
        hours: u32,
        minutes: u32,
        _font: &ArcadeFont,
        _scale_mult: u32,
    ) {
        let now = Instant::now();
        let mut dt = now.duration_since(self.last_frame_ms).as_secs_f32();
        if dt <= 0.0 || dt > 0.2 {
            dt = 0.016;
        }
        self.last_frame_ms = now;
        self.draw(matrix, hours, minutes, dt);
    }

    pub fn draw(&mut self, matrix: &mut dyn Matrix, hours: u32, minutes: u32, dt: f32) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;
        let is_portrait = h > w;
        let is_wide_tall = h >= 48 && !is_portrait;
        let ground_top = if is_portrait { 70 } else { h - GROUND_H };

        let (hour_x, minute_x, block_y) = if is_portrait {
            if w >= 64 {
                (10, 35, 24)
            } else {
                (1, 17, 18)
            }
        } else if is_wide_tall {
            let scene_left = (w - SCENE) / 2;
            (scene_left + 13, scene_left + 32, 8)
        } else {
            (w - 46, w - 24, 3)
        };

        let hh = format!("{}", hours);
        let mm = format!("{:02}", minutes);
        let speed_scale = self.speed_pct as f32 / 100.0;

        if self.snap_to_now {
            self.snap_to_now = false;
            self.shown_hh = hh.clone();
            self.shown_mm = mm.clone();
            self.last_minute = minutes as i32;
            self.phase = Phase::Waiting;
            self.pending_digits = false;
            self.block_bounce = [0.0, 0.0];
            self.runner_x = -40.0;
            self.runner_y = (ground_top - MARIO_H) as f32;
            self.shell_x = 40.0;
            self.shell_dir = 1.0;
            self.coin_pop = false;
        }

        if self.last_minute != minutes as i32 {
            let new_hour = self.last_minute >= 0 && minutes == 0;
            self.last_minute = minutes as i32;
            self.jump_target = if new_hour { 0 } else { 1 };
            if is_wide_tall || is_portrait {
                if self.phase == Phase::Waiting {
                    self.phase = Phase::RunIn;
                    self.runner_x = -(MARIO_JUMP_W as f32);
                    self.climb_stage = 0;
                    self.runner_y = if is_portrait {
                        (h - GROUND_H - MARIO_H) as f32
                    } else {
                        (ground_top - MARIO_H) as f32
                    };
                    self.pending_digits = true;
                } else {
                    self.pending_digits = true;
                }
            } else {
                self.phase = Phase::RunIn;
                self.runner_x = -(MARIO_W as f32);
                self.runner_y = (ground_top - MARIO_H) as f32;
                self.pending_digits = true;
                self.coin_pop = false;
            }
        }
        if self.shown_hh.starts_with('-') {
            self.shown_hh = hh.clone();
            self.shown_mm = mm.clone();
        }

        if is_portrait {
            if w >= 64 {
                let target_x = ((if self.jump_target == 0 {
                    hour_x
                } else {
                    minute_x
                }) + BLOCK_W / 2
                    - MARIO_W / 2) as f32;
                let pace = 75.0 * speed_scale;
                let ground_y = (h - GROUND_H - MARIO_H) as f32;

                match self.phase {
                    Phase::Waiting | Phase::Kick => {}
                    Phase::RunIn => match self.climb_stage {
                        0 => {
                            self.runner_x += pace * dt;
                            self.runner_y = ground_y;
                            if self.runner_x >= 14.0 {
                                self.climb_stage = 1;
                                self.jump_t = 0.0;
                            }
                        }
                        1 => {
                            self.jump_t += dt * 2.6 * speed_scale;
                            let t = self.jump_t.min(1.0);
                            self.runner_x = 14.0 + 22.0 * t;
                            let base_y = ground_y + ((190 - MARIO_H) as f32 - ground_y) * t;
                            let arc = 4.0 * 14.0 * t * (1.0 - t);
                            self.runner_y = base_y - arc;
                            if self.jump_t >= 1.0 {
                                self.jump_t = 0.0;
                                self.runner_y = (190 - MARIO_H) as f32;
                                self.climb_stage = 2;
                            }
                        }
                        2 => {
                            self.runner_x -= pace * dt;
                            self.runner_y = (190 - MARIO_H) as f32;
                            if self.runner_x <= 30.0 {
                                self.climb_stage = 3;
                                self.jump_t = 0.0;
                            }
                        }
                        3 => {
                            self.jump_t += dt * 2.8 * speed_scale;
                            let t = self.jump_t.min(1.0);
                            self.runner_x = 30.0 + (18.0 - 30.0) * t;
                            let base_y = (190 - MARIO_H) as f32
                                + ((130 - MARIO_H) as f32 - (190 - MARIO_H) as f32) * t;
                            let arc = 4.0 * 14.0 * t * (1.0 - t);
                            self.runner_y = base_y - arc;
                            if self.jump_t >= 1.0 {
                                self.jump_t = 0.0;
                                self.runner_y = (130 - MARIO_H) as f32;
                                self.climb_stage = 4;
                            }
                        }
                        4 => {
                            self.runner_x += pace * dt;
                            self.runner_y = (130 - MARIO_H) as f32;
                            if self.runner_x >= 24.0 {
                                self.climb_stage = 5;
                                self.jump_t = 0.0;
                            }
                        }
                        5 => {
                            self.jump_t += dt * 2.8 * speed_scale;
                            let t = self.jump_t.min(1.0);
                            self.runner_x = 24.0 + (32.0 - 24.0) * t;
                            let base_y = (130 - MARIO_H) as f32
                                + ((70 - MARIO_H) as f32 - (130 - MARIO_H) as f32) * t;
                            let arc = 4.0 * 14.0 * t * (1.0 - t);
                            self.runner_y = base_y - arc;
                            if self.jump_t >= 1.0 {
                                self.jump_t = 0.0;
                                self.runner_y = (70 - MARIO_H) as f32;
                                self.climb_stage = 6;
                            }
                        }
                        _ => {
                            if self.runner_x < target_x {
                                self.runner_x += pace * dt;
                                if self.runner_x >= target_x {
                                    self.runner_x = target_x;
                                    self.phase = Phase::Jump;
                                    self.jump_t = 0.0;
                                }
                            } else {
                                self.runner_x -= pace * dt;
                                if self.runner_x <= target_x {
                                    self.runner_x = target_x;
                                    self.phase = Phase::Jump;
                                    self.jump_t = 0.0;
                                }
                            }
                            self.runner_y = (70 - MARIO_H) as f32;
                        }
                    },
                    Phase::Jump => {
                        self.jump_t += dt * 4.0 * speed_scale;
                        if self.jump_t >= 0.5 && self.pending_digits {
                            self.pending_digits = false;
                            self.block_bounce[self.jump_target] = 0.001;
                            self.shown_hh = hh.clone();
                            self.shown_mm = mm.clone();
                        }
                        if self.jump_t >= 1.0 {
                            self.jump_t = 0.0;
                            self.phase = Phase::RunOut;
                        }
                    }
                    Phase::RunOut => {
                        self.runner_x += pace * dt;
                        self.runner_y = (70 - MARIO_H) as f32;
                        if self.runner_x > (w + MARIO_JUMP_W + 16) as f32 {
                            self.phase = Phase::Waiting;
                        }
                    }
                }
            } else {
                // 32x128 portrait
                let pace = 65.0 * speed_scale;
                let target_block_x = if self.jump_target == 0 { 2.0 } else { 18.0 };
                let ground_y = (h - GROUND_H - MARIO_H) as f32;

                match self.phase {
                    Phase::Waiting | Phase::Kick => {}
                    Phase::RunIn => match self.climb_stage {
                        0 => {
                            self.runner_x += pace * dt;
                            self.runner_y = ground_y;
                            if self.runner_x >= 8.0 {
                                self.climb_stage = 1;
                                self.jump_t = 0.0;
                            }
                        }
                        1 => {
                            self.jump_t += dt * 2.8 * speed_scale;
                            let t = self.jump_t.min(1.0);
                            self.runner_x = 8.0 + (20.0 - 8.0) * t;
                            let base_y = ground_y + ((88 - MARIO_H) as f32 - ground_y) * t;
                            let arc = 4.0 * 12.0 * t * (1.0 - t);
                            self.runner_y = base_y - arc;
                            if self.jump_t >= 1.0 {
                                self.jump_t = 0.0;
                                self.runner_y = (88 - MARIO_H) as f32;
                                self.climb_stage = 2;
                            }
                        }
                        2 => {
                            self.runner_x -= pace * dt;
                            self.runner_y = (88 - MARIO_H) as f32;
                            if self.runner_x <= 10.0 {
                                self.climb_stage = 3;
                                self.jump_t = 0.0;
                            }
                        }
                        3 => {
                            self.jump_t += dt * 2.8 * speed_scale;
                            let t = self.jump_t.min(1.0);
                            self.runner_x = 10.0 + (target_block_x - 10.0) * t;
                            let base_y = (88 - MARIO_H) as f32
                                + ((56 - MARIO_H) as f32 - (88 - MARIO_H) as f32) * t;
                            let arc = 4.0 * 12.0 * t * (1.0 - t);
                            self.runner_y = base_y - arc;
                            if self.jump_t >= 1.0 {
                                self.jump_t = 0.0;
                                self.runner_y = (56 - MARIO_H) as f32;
                                self.climb_stage = 4;
                            }
                        }
                        _ => {
                            self.runner_y = (56 - MARIO_H) as f32;
                            if (self.runner_x - target_block_x).abs() <= 1.5 {
                                self.runner_x = target_block_x;
                                self.phase = Phase::Jump;
                                self.jump_t = 0.0;
                            } else if self.runner_x < target_block_x {
                                self.runner_x += pace * dt;
                                if self.runner_x >= target_block_x {
                                    self.runner_x = target_block_x;
                                    self.phase = Phase::Jump;
                                    self.jump_t = 0.0;
                                }
                            } else {
                                self.runner_x -= pace * dt;
                                if self.runner_x <= target_block_x {
                                    self.runner_x = target_block_x;
                                    self.phase = Phase::Jump;
                                    self.jump_t = 0.0;
                                }
                            }
                        }
                    },
                    Phase::Jump => {
                        self.jump_t += dt * 3.8 * speed_scale;
                        let t = self.jump_t.min(1.0);
                        let lift = (t * std::f32::consts::PI).sin() * 10.0;
                        self.runner_y = (56 - MARIO_H) as f32 - lift;
                        if self.jump_t >= 0.5 && self.pending_digits {
                            self.pending_digits = false;
                            self.block_bounce[self.jump_target] = 0.001;
                            self.shown_hh = hh.clone();
                            self.shown_mm = mm.clone();
                            self.coin_pop = true;
                        }
                        if self.jump_t >= 1.0 {
                            self.jump_t = 0.0;
                            self.runner_y = (56 - MARIO_H) as f32;
                            self.phase = Phase::RunOut;
                        }
                    }
                    Phase::RunOut => {
                        self.runner_x += pace * dt;
                        self.runner_y = (56 - MARIO_H) as f32;
                        if self.runner_x > (w + 16) as f32 {
                            self.phase = Phase::Waiting;
                        }
                    }
                }
            }
        } else if is_wide_tall {
            let target_x = ((if self.jump_target == 0 {
                hour_x
            } else {
                minute_x
            }) + BLOCK_W / 2
                - MARIO_W / 2) as f32;
            let pace = 85.0 * speed_scale;

            match self.phase {
                Phase::Waiting | Phase::Kick => {}
                Phase::RunIn => {
                    self.runner_x += pace * dt;
                    if self.runner_x >= target_x {
                        self.runner_x = target_x;
                        self.phase = Phase::Jump;
                        self.jump_t = 0.0;
                    }
                }
                Phase::Jump => {
                    self.jump_t += dt * 4.0 * speed_scale;
                    if self.jump_t >= 0.5 && self.pending_digits {
                        self.pending_digits = false;
                        self.block_bounce[self.jump_target] = 0.001;
                        self.shown_hh = hh.clone();
                        self.shown_mm = mm.clone();
                    }
                    if self.jump_t >= 1.0 {
                        self.jump_t = 0.0;
                        self.phase = Phase::RunOut;
                    }
                }
                Phase::RunOut => {
                    self.runner_x += pace * dt;
                    if self.runner_x > (w + MARIO_JUMP_W + 16) as f32 {
                        self.phase = Phase::Waiting;
                    }
                }
            }
        } else {
            // 128x32 compact mode shell physics
            let target_x = ((if self.jump_target == 0 {
                hour_x
            } else {
                minute_x
            }) + BLOCK_W / 2
                - KOOPA_SHELL_W / 2) as f32;
            let pace = 75.0 * speed_scale;

            match self.phase {
                Phase::Waiting => {
                    let patrol_speed = 22.0 * speed_scale;
                    self.shell_x += self.shell_dir * patrol_speed * dt;
                    if self.shell_x >= 65.0 {
                        self.shell_x = 65.0;
                        self.shell_dir = -1.0;
                    } else if self.shell_x <= 20.0 {
                        self.shell_x = 20.0;
                        self.shell_dir = 1.0;
                    }
                    self.runner_x = -40.0;
                }
                Phase::RunIn => {
                    let patrol_speed = 22.0 * speed_scale;
                    self.shell_x += self.shell_dir * patrol_speed * dt;
                    if self.shell_x >= 65.0 {
                        self.shell_x = 65.0;
                        self.shell_dir = -1.0;
                    } else if self.shell_x <= 20.0 {
                        self.shell_x = 20.0;
                        self.shell_dir = 1.0;
                    }

                    self.runner_x += pace * dt;
                    self.runner_y = (ground_top - MARIO_H) as f32;
                    if (self.runner_x >= 10.0 && (self.shell_x - self.runner_x) <= 35.0)
                        || self.runner_x >= 25.0
                    {
                        self.jump_start_x = self.runner_x;
                        self.jump_target_x = self.shell_x;
                        self.jump_t = 0.0;
                        self.phase = Phase::Jump;
                    }
                }
                Phase::Jump => {
                    self.jump_t += dt * 2.2 * speed_scale;
                    let t = self.jump_t.min(1.0);
                    self.runner_x =
                        self.jump_start_x + (self.jump_target_x - self.jump_start_x) * t;
                    let arc = (t * std::f32::consts::PI).sin() * 22.0;
                    self.runner_y = (ground_top - MARIO_H) as f32 - arc;
                    if self.jump_t >= 1.0 {
                        self.jump_t = 0.0;
                        self.runner_x = self.jump_target_x;
                        self.runner_y = (ground_top - MARIO_H) as f32;
                        self.phase = Phase::Kick;
                        self.shell_dir = 1.0;
                    }
                }
                Phase::Kick => {
                    let shell_speed = 280.0 * speed_scale;
                    self.shell_x += shell_speed * dt;
                    let hop_progress = ((self.shell_x - self.jump_target_x) / 30.0).clamp(0.0, 1.0);
                    self.runner_y = (ground_top - MARIO_H) as f32
                        - (hop_progress * std::f32::consts::PI).sin() * 6.0;

                    if self.shell_x >= target_x - 4.0 {
                        self.shell_x = target_x - 4.0;
                        self.block_bounce[self.jump_target] = 0.001;
                        self.shown_hh = hh.clone();
                        self.shown_mm = mm.clone();
                        self.pending_digits = false;
                        self.coin_pop = true;
                        self.phase = Phase::RunOut;
                    }
                }
                Phase::RunOut => {
                    self.runner_x -= pace * dt;
                    self.runner_y = (ground_top - MARIO_H) as f32;
                    if self.shell_x > 55.0 {
                        self.shell_x -= 40.0 * speed_scale * dt;
                    }
                    if self.runner_x < -(MARIO_W as f32) - 8.0 {
                        self.phase = Phase::Waiting;
                        self.shell_dir = -1.0;
                    }
                }
            }
        }

        for i in 0..2 {
            if self.block_bounce[i] > 0.0 {
                self.block_bounce[i] += dt * 3.2;
                if self.block_bounce[i] >= 1.0 {
                    self.block_bounce[i] = 0.0;
                    if !is_wide_tall && !is_portrait {
                        self.coin_pop = false;
                    }
                }
            }
        }

        Self::draw_scene(matrix, w, h);

        for i in 0..2 {
            let b = self.block_bounce[i];
            let lift = if b > 0.0 {
                ((b * std::f32::consts::PI).sin() * 4.0) as i32
            } else {
                0
            };
            let bx = if i == 0 { hour_x } else { minute_x };
            let by = block_y - lift;
            let text = if i == 0 {
                &self.shown_hh
            } else {
                &self.shown_mm
            };
            Self::draw_block_at(matrix, bx, by, text, is_portrait && w < 64);
        }

        if is_portrait && w < 64 && self.coin_pop && self.block_bounce[self.jump_target] > 0.0 {
            let b = self.block_bounce[self.jump_target];
            let coin_lift = ((b * std::f32::consts::PI).sin() * 6.0) as i32;
            let target_bx = if self.jump_target == 0 {
                hour_x
            } else {
                minute_x
            };
            Self::blit_sprite(
                matrix,
                &GOLD_COIN,
                8,
                8,
                target_bx + 3,
                block_y - 8 - coin_lift,
                true,
                false,
            );
        }

        let run_frames: [&[u16]; 3] = [&MARIO_RUN1, &MARIO_RUN2, &MARIO_RUN3];

        if is_wide_tall || is_portrait {
            if self.phase != Phase::Waiting {
                let mut airborne = false;
                let draw_y: i32;
                let mut flip_h = false;

                if is_portrait {
                    if w >= 64 {
                        if self.phase == Phase::Jump {
                            airborne = true;
                            let lift = ((self.jump_t * std::f32::consts::PI).sin()
                                * (70 - block_y - BLOCK_H - 2) as f32)
                                as i32;
                            draw_y = (70 - MARIO_H) - lift;
                        } else if self.phase == Phase::RunIn
                            && (self.climb_stage == 1
                                || self.climb_stage == 3
                                || self.climb_stage == 5)
                        {
                            airborne = true;
                            draw_y = self.runner_y as i32;
                        } else {
                            draw_y = self.runner_y as i32;
                        }
                    } else {
                        airborne = self.phase == Phase::Jump
                            || (self.phase == Phase::RunIn
                                && (self.climb_stage == 1 || self.climb_stage == 3));
                        draw_y = self.runner_y as i32;
                        let target_block_x = if self.jump_target == 0 { 2.0 } else { 18.0 };
                        if self.phase == Phase::RunIn
                            && (self.climb_stage == 2
                                || (self.climb_stage == 4 && self.runner_x > target_block_x))
                        {
                            flip_h = true;
                        }
                    }
                } else {
                    airborne = self.phase == Phase::Jump;
                    let lift = if airborne {
                        ((self.jump_t * std::f32::consts::PI).sin()
                            * (ground_top - block_y - BLOCK_H - 2) as f32)
                            as i32
                    } else {
                        0
                    };
                    draw_y = ground_top - MARIO_H - lift;
                }

                if airborne {
                    Self::blit_sprite(
                        matrix,
                        &MARIO_JUMP,
                        MARIO_JUMP_W,
                        MARIO_H,
                        self.runner_x as i32,
                        draw_y,
                        true,
                        flip_h,
                    );
                } else {
                    let f = (((self.runner_x as i32 + 1200) / 6).rem_euclid(3)) as usize;
                    Self::blit_sprite(
                        matrix,
                        run_frames[f],
                        MARIO_RUN_W[f],
                        MARIO_H,
                        (self.runner_x as i32) - 2 + MARIO_RUN_LEFT[f],
                        draw_y,
                        true,
                        flip_h,
                    );
                }
            }
        } else {
            // 128x32: Draw Mario if on screen
            if self.runner_x >= -(MARIO_W as f32) && self.runner_x < w as f32 {
                if self.phase == Phase::Jump {
                    Self::blit_sprite(
                        matrix,
                        &MARIO_JUMP,
                        MARIO_JUMP_W,
                        MARIO_H,
                        self.runner_x as i32,
                        self.runner_y as i32,
                        true,
                        false,
                    );
                } else {
                    let f = (((self.runner_x as i32 + 1200) / 6).rem_euclid(3)) as usize;
                    Self::blit_sprite(
                        matrix,
                        run_frames[f],
                        MARIO_RUN_W[f],
                        MARIO_H,
                        (self.runner_x as i32) - 2 + MARIO_RUN_LEFT[f],
                        self.runner_y as i32,
                        true,
                        false,
                    );
                }
            }

            let in_shell = self.phase == Phase::Kick || self.phase == Phase::RunOut;
            if in_shell {
                Self::blit_sprite(
                    matrix,
                    &KOOPA_SHELL,
                    KOOPA_SHELL_W,
                    KOOPA_SHELL_H,
                    self.shell_x as i32,
                    ground_top - KOOPA_SHELL_H,
                    true,
                    false,
                );
            } else {
                let kf = (((self.shell_x as i32 + 500) / 4).rem_euclid(2)) as usize;
                let flip_h = self.shell_dir > 0.0;
                let k_sprite = if kf == 0 { &KOOPA_WALK1 } else { &KOOPA_WALK2 };
                Self::blit_sprite(
                    matrix,
                    k_sprite,
                    KOOPA_W,
                    KOOPA_H,
                    self.shell_x as i32,
                    ground_top - KOOPA_H,
                    true,
                    flip_h,
                );
            }

            if self.coin_pop && self.block_bounce[self.jump_target] > 0.0 {
                let target_x = if self.jump_target == 0 {
                    hour_x
                } else {
                    minute_x
                };
                let b = self.block_bounce[self.jump_target];
                let coin_lift = ((b * std::f32::consts::PI).sin() * 6.0) as i32;
                Self::blit_sprite(
                    matrix,
                    &GOLD_COIN,
                    8,
                    8,
                    target_x + 5,
                    block_y - 4 - coin_lift,
                    true,
                    false,
                );
            }
        }
    }
}

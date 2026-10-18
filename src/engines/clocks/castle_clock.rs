use crate::core::matrix::IDrawingSurface;
use crate::engines::clocks::cwassets::castlevania::*;
use std::time::Instant;

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

const BAT_PALS: [&[u16]; 2] = [&BAT_F0_PAL, &BAT_F1_PAL];
const BAT_PIXELS: [&[u8]; 2] = [&BAT_F0_PIXELS, &BAT_F1_PIXELS];

const FLAME_PALS: [&[u16]; 3] = [&FLAME_F0_PAL, &FLAME_F1_PAL, &FLAME_F2_PAL];
const FLAME_PIXELS: [&[u8]; 3] = [&FLAME_F0_PIXELS, &FLAME_F1_PIXELS, &FLAME_F2_PIXELS];

const SIMON_WALK_PALS: [&[u16]; 3] = [&SIMON_IDLE_PAL, &SIMON_IDLE_PAL, &SIMON_IDLE_PAL];
const SIMON_WALK_PIXELS: [&[u8]; 3] = [
    &SIMON_WALK_0_PIXELS,
    &SIMON_WALK_1_PIXELS,
    &SIMON_WALK_2_PIXELS,
];

#[derive(PartialEq, Clone, Copy)]
enum Phase {
    Idle,
    HeroEnter,
    WhipWindup,
    WhipStrike,
    Impact,
    HeroExit,
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
    last_second: i32,
    snap_to_now: bool,
    last_frame_ms: Instant,
}

impl Default for CastleClock {
    fn default() -> Self {
        Self::new()
    }
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
            bat_x: 140.0,
            simon_x: -40.0,
            simon_y: 0.0,
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
        self.phase = Phase::Idle;
        self.phase_timer = 0.0;
        self.bat_x = 140.0;
        self.simon_x = -40.0;
    }

    #[allow(clippy::too_many_arguments)]
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
                    matrix.draw_pixel(px, py, r, g, b);
                }
            }
        }
    }

    fn draw_gothic_digit(
        matrix: &mut dyn IDrawingSurface,
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

        for r in 0..5i32 {
            let row_bits = GOTHIC_FONT_3x5[idx][r as usize];
            let py = y + r * scale;
            for b in 0..3i32 {
                if ((row_bits >> (2 - b)) & 1) != 0 {
                    let px = x + b * scale;
                    matrix.fill_rect(px, py, scale, scale, color.0, color.1, color.2);
                }
            }
        }
    }

    fn draw_gothic_time(
        matrix: &mut dyn IDrawingSurface,
        start_x: i32,
        start_y: i32,
        s: &str,
        color: (u8, u8, u8),
        scale: i32,
    ) {
        let chars: Vec<char> = s.chars().collect();
        if chars.len() < 5 {
            return;
        }
        let digit_w = 3 * scale;
        let digit_gap = scale;
        let sym_s = 2 * scale;

        let x_h1 = start_x;
        let x_h2 = x_h1 + digit_w + digit_gap;
        let x_colon = x_h2 + 2 * scale + sym_s;
        let x_m1 = x_colon + 2 * scale + sym_s;
        let x_m2 = x_m1 + digit_w + digit_gap;

        Self::draw_gothic_digit(matrix, x_h1, start_y, chars[0], color, scale);
        Self::draw_gothic_digit(matrix, x_h2, start_y, chars[1], color, scale);
        Self::draw_gothic_digit(matrix, x_colon, start_y, chars[2], color, scale);
        Self::draw_gothic_digit(matrix, x_m1, start_y, chars[3], color, scale);
        Self::draw_gothic_digit(matrix, x_m2, start_y, chars[4], color, scale);
    }

    fn draw_scene(&self, matrix: &mut dyn IDrawingSurface, w: i32, h: i32) {
        let is_portrait = h > w;

        if is_portrait {
            // --- Portrait (64x256 / 64x128) Dracula Castle Tower ---
            matrix.fill_rect(0, 0, w, h, 16, 0, 24);

            // Blood Moon top center - separated from clock time below
            let moon_x = w / 2;
            let moon_r = if w >= 64 { 11 } else { 6 };
            matrix.fill_circle(moon_x, 12, moon_r, 220, 40, 20);
            matrix.fill_circle(moon_x - 1, 11, (moon_r - 3).max(1), 240, 80, 40);

            // Flying Vampire Bat across moon
            let bat_pal = BAT_PALS[self.bat_frame % 2];
            let bat_pix = BAT_PIXELS[self.bat_frame % 2];
            Self::blit_sprite(
                matrix,
                bat_pal,
                bat_pix,
                BAT_F0_W,
                BAT_F0_H,
                self.bat_x as i32,
                8,
                false,
            );

            // Stone battlement / cornice at y=56
            let stone_base = (80u8, 80u8, 96u8);
            let stone_light = (140u8, 140u8, 160u8);
            let stone_dark = (36u8, 36u8, 48u8);

            matrix.draw_line(
                0,
                56,
                w - 1,
                56,
                stone_light.0,
                stone_light.1,
                stone_light.2,
            );
            matrix.fill_rect(0, 57, w, 4, stone_base.0, stone_base.1, stone_base.2);
            let mut x = 0;
            while x < w {
                matrix.draw_line(x, 57, x, 60, stone_dark.0, stone_dark.1, stone_dark.2);
                x += 16;
            }

            // Bottom dungeon floor (8px high)
            matrix.fill_rect(0, h - 8, w, 8, stone_dark.0, stone_dark.1, stone_dark.2);
            matrix.draw_line(
                0,
                h - 8,
                w - 1,
                h - 8,
                stone_light.0,
                stone_light.1,
                stone_light.2,
            );

            if w >= 64 {
                // Platform 3 (bottom): y=190, from x=16 to x=56 (width 40)
                matrix.fill_rect(16, 190, 40, 5, stone_base.0, stone_base.1, stone_base.2);
                matrix.draw_line(
                    16,
                    190,
                    55,
                    190,
                    stone_light.0,
                    stone_light.1,
                    stone_light.2,
                );
                // Platform 2 (middle): y=130, from x=8 to x=48 (width 40)
                matrix.fill_rect(8, 130, 40, 5, stone_base.0, stone_base.1, stone_base.2);
                matrix.draw_line(8, 130, 47, 130, stone_light.0, stone_light.1, stone_light.2);
                // Platform 1 (top): y=70, from x=8 to x=56 (width 48)
                matrix.fill_rect(8, 70, 48, 5, stone_base.0, stone_base.1, stone_base.2);
                matrix.draw_line(8, 70, 55, 70, stone_light.0, stone_light.1, stone_light.2);
            } else {
                // 32-wide Tate (128x32 rotated: w=32, h=128)
                matrix.fill_rect(0, 56, w, 5, stone_base.0, stone_base.1, stone_base.2);
                matrix.draw_line(
                    0,
                    56,
                    w - 1,
                    56,
                    stone_light.0,
                    stone_light.1,
                    stone_light.2,
                );
                matrix.fill_rect(8, 88, w - 8, 5, stone_base.0, stone_base.1, stone_base.2);
                matrix.draw_line(
                    8,
                    88,
                    w - 1,
                    88,
                    stone_light.0,
                    stone_light.1,
                    stone_light.2,
                );
            }

            // Candle torch stand on Platform 1
            let torch_x = if w >= 64 { 46 } else { w - 7 };
            let torch_plat_y = if w >= 64 { 70 } else { 56 };
            matrix.fill_rect(torch_x, torch_plat_y - 14, 4, 14, 140, 100, 50);
            matrix.fill_rect(torch_x - 2, torch_plat_y - 18, 8, 4, 140, 100, 50);

            let flame_pal = FLAME_PALS[self.flame_frame % 3];
            let flame_pix = FLAME_PIXELS[self.flame_frame % 3];
            Self::blit_sprite(
                matrix,
                flame_pal,
                flame_pix,
                FLAME_F0_W,
                FLAME_F0_H,
                torch_x - 2,
                torch_plat_y - 28,
                false,
            );
        } else if h >= 48 {
            // --- 256x64 Dracula's Castle Parapets ---
            matrix.fill_rect(0, 0, w, 48, 16, 0, 24);

            // Blood Moon on right
            matrix.fill_circle(216, 22, 18, 220, 40, 20);
            matrix.fill_circle(214, 20, 15, 240, 80, 40);

            // Parapets at y=48..63
            let stone_base = (80u8, 80u8, 96u8);
            let stone_light = (140u8, 140u8, 160u8);
            let stone_dark = (36u8, 36u8, 48u8);

            matrix.fill_rect(0, 48, w, h - 48, stone_base.0, stone_base.1, stone_base.2);
            matrix.draw_line(
                0,
                48,
                w - 1,
                48,
                stone_light.0,
                stone_light.1,
                stone_light.2,
            );

            let mut x = 0;
            while x < w {
                matrix.draw_line(x, 48, x, h - 1, stone_dark.0, stone_dark.1, stone_dark.2);
                matrix.draw_line(
                    x + 1,
                    48,
                    x + 1,
                    h - 1,
                    stone_light.0,
                    stone_light.1,
                    stone_light.2,
                );
                x += 16;
            }

            // Candle Torch Stand on right (x=240, y=26..47)
            matrix.fill_rect(240, 34, 4, 14, 140, 100, 50);
            matrix.fill_rect(238, 30, 8, 4, 140, 100, 50);

            let flame_pal = FLAME_PALS[self.flame_frame % 3];
            let flame_pix = FLAME_PIXELS[self.flame_frame % 3];
            Self::blit_sprite(
                matrix, flame_pal, flame_pix, FLAME_F0_W, FLAME_F0_H, 238, 20, false,
            );

            let bat_pal = BAT_PALS[self.bat_frame % 2];
            let bat_pix = BAT_PIXELS[self.bat_frame % 2];
            Self::blit_sprite(
                matrix,
                bat_pal,
                bat_pix,
                BAT_F0_W,
                BAT_F0_H,
                self.bat_x as i32,
                10,
                false,
            );
        } else {
            // --- 128x32 Dungeon Floor ---
            matrix.fill_rect(0, 0, w, 26, 0, 0, 0);

            // Blood Moon in upper-left sky
            matrix.fill_circle(14, 10, 8, 220, 40, 20);
            matrix.fill_circle(13, 9, 6, 240, 80, 40);

            // Brick floor at y=26..31
            matrix.fill_rect(0, 26, w, h - 26, 116, 40, 12);
            matrix.draw_line(0, 26, w - 1, 26, 180, 70, 20);

            let mut x = 0;
            while x < w {
                matrix.draw_line(x, 26, x, h - 1, 40, 16, 8);
                x += 8;
            }

            // Candle Torch on right
            matrix.fill_rect(116, 20, 2, 6, 140, 100, 50);
            let flame_pal = FLAME_PALS[self.flame_frame % 3];
            let flame_pix = FLAME_PIXELS[self.flame_frame % 3];
            Self::blit_sprite(
                matrix, flame_pal, flame_pix, FLAME_F0_W, FLAME_F0_H, 113, 10, false,
            );

            let bat_pal = BAT_PALS[self.bat_frame % 2];
            let bat_pix = BAT_PIXELS[self.bat_frame % 2];
            Self::blit_sprite(
                matrix,
                bat_pal,
                bat_pix,
                BAT_F0_W,
                BAT_F0_H,
                self.bat_x as i32,
                4,
                false,
            );
        }
    }

    pub fn render(
        &mut self,
        matrix: &mut dyn IDrawingSurface,
        hours: u32,
        minutes: u32,
        seconds: u32,
    ) {
        let now = Instant::now();
        let dt = now
            .duration_since(self.last_frame_ms)
            .as_secs_f32()
            .clamp(0.001, 0.2);
        self.last_frame_ms = now;
        self.draw(matrix, hours, minutes, seconds, dt);
    }

    pub fn draw(
        &mut self,
        matrix: &mut dyn IDrawingSurface,
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
            self.phase = Phase::Idle;
            self.phase_timer = 0.0;
            self.walk_timer = 0.0;
            self.walk_frame = 0;
            self.flame_timer = 0.0;
            self.flame_frame = 0;
            self.bat_timer = 0.0;
            self.bat_frame = 0;
            self.bat_x = w as f32;
            self.simon_x = -40.0;
        }

        if self.last_second != seconds as i32 {
            self.last_second = seconds as i32;
        }

        if self.last_minute != minutes as i32 && self.phase == Phase::Idle {
            self.phase = Phase::HeroEnter;
            self.phase_timer = 0.0;
            if is_portrait {
                self.simon_x = -16.0;
                self.simon_y = (h - 8 - 30) as f32;
                self.climb_stage = 0;
                self.jump_t = 0.0;
            } else {
                self.simon_x = -20.0;
                self.simon_y = if h >= 48 { 48.0 - 30.0 } else { 0.0 };
            }
        }

        // Flame flicker (every 120ms)
        self.flame_timer += dt;
        if self.flame_timer >= 0.12 {
            self.flame_timer = 0.0;
            self.flame_frame = (self.flame_frame + 1) % 3;
        }

        // Bat flap and glide across sky
        self.bat_timer += dt;
        if self.bat_timer >= 0.18 {
            self.bat_timer = 0.0;
            self.bat_frame = (self.bat_frame + 1) % 2;
        }
        self.bat_x -= (if is_portrait { 18.0 } else { 25.0 }) * dt;
        if self.bat_x < -20.0 {
            self.bat_x = w as f32 + 10.0;
        }

        if self.phase == Phase::HeroEnter || self.phase == Phase::HeroExit {
            self.walk_timer += dt;
            if self.walk_timer >= 0.12 {
                self.walk_timer = 0.0;
                self.walk_frame = self.walk_frame.wrapping_add(1);
            }
        }

        match self.phase {
            Phase::Idle => {}
            Phase::HeroEnter => {
                if is_portrait {
                    if w >= 64 {
                        let floor_y = (h - 8 - 30) as f32;
                        match self.climb_stage {
                            0 => {
                                self.simon_x += 50.0 * dt;
                                self.simon_y = floor_y;
                                if self.simon_x >= 12.0 {
                                    self.climb_stage = 1;
                                    self.jump_t = 0.0;
                                }
                            }
                            1 => {
                                self.jump_t += dt * 2.6;
                                let t = self.jump_t.min(1.0);
                                self.simon_x = 12.0 + (36.0 - 12.0) * t;
                                let base_y = floor_y + (160.0 - floor_y) * t;
                                let arc = 4.0 * 14.0 * t * (1.0 - t);
                                self.simon_y = base_y - arc;
                                if self.jump_t >= 1.0 {
                                    self.jump_t = 0.0;
                                    self.simon_y = 160.0;
                                    self.climb_stage = 2;
                                }
                            }
                            2 => {
                                self.simon_x -= 45.0 * dt;
                                self.simon_y = 160.0;
                                if self.simon_x <= 30.0 {
                                    self.climb_stage = 3;
                                    self.jump_t = 0.0;
                                }
                            }
                            3 => {
                                self.jump_t += dt * 2.8;
                                let t = self.jump_t.min(1.0);
                                self.simon_x = 30.0 + (18.0 - 30.0) * t;
                                let base_y = 160.0 + (100.0 - 160.0) * t;
                                let arc = 4.0 * 14.0 * t * (1.0 - t);
                                self.simon_y = base_y - arc;
                                if self.jump_t >= 1.0 {
                                    self.jump_t = 0.0;
                                    self.simon_y = 100.0;
                                    self.climb_stage = 4;
                                }
                            }
                            4 => {
                                self.simon_x += 45.0 * dt;
                                self.simon_y = 100.0;
                                if self.simon_x >= 24.0 {
                                    self.climb_stage = 5;
                                    self.jump_t = 0.0;
                                }
                            }
                            5 => {
                                self.jump_t += dt * 2.8;
                                let t = self.jump_t.min(1.0);
                                self.simon_x = 24.0 + (16.0 - 24.0) * t;
                                let base_y = 100.0 + (40.0 - 100.0) * t;
                                let arc = 4.0 * 14.0 * t * (1.0 - t);
                                self.simon_y = base_y - arc;
                                if self.jump_t >= 1.0 {
                                    self.jump_t = 0.0;
                                    self.simon_y = 40.0;
                                    self.climb_stage = 6;
                                }
                            }
                            _ => {
                                self.simon_x += 45.0 * dt;
                                self.simon_y = 40.0;
                                if self.simon_x >= 14.0 {
                                    self.simon_x = 14.0;
                                    self.phase = Phase::WhipWindup;
                                    self.phase_timer = 0.0;
                                }
                            }
                        }
                    } else {
                        let floor_y = (h - 8 - 30) as f32;
                        match self.climb_stage {
                            0 => {
                                self.simon_x += 50.0 * dt;
                                self.simon_y = floor_y;
                                if self.simon_x >= 8.0 {
                                    self.climb_stage = 1;
                                    self.jump_t = 0.0;
                                }
                            }
                            1 => {
                                self.jump_t += dt * 2.8;
                                let t = self.jump_t.min(1.0);
                                self.simon_x = 8.0 + (20.0 - 8.0) * t;
                                let base_y = floor_y + ((88.0 - 30.0) - floor_y) * t;
                                let arc = 4.0 * 12.0 * t * (1.0 - t);
                                self.simon_y = base_y - arc;
                                if self.jump_t >= 1.0 {
                                    self.jump_t = 0.0;
                                    self.simon_y = 88.0 - 30.0;
                                    self.climb_stage = 2;
                                }
                            }
                            2 => {
                                self.simon_x -= 45.0 * dt;
                                self.simon_y = 88.0 - 30.0;
                                if self.simon_x <= 10.0 {
                                    self.climb_stage = 3;
                                    self.jump_t = 0.0;
                                }
                            }
                            3 => {
                                self.jump_t += dt * 2.8;
                                let t = self.jump_t.min(1.0);
                                self.simon_x = 10.0 + (2.0 - 10.0) * t;
                                let base_y = (88.0 - 30.0) + ((56.0 - 30.0) - (88.0 - 30.0)) * t;
                                let arc = 4.0 * 12.0 * t * (1.0 - t);
                                self.simon_y = base_y - arc;
                                if self.jump_t >= 1.0 {
                                    self.jump_t = 0.0;
                                    self.simon_y = 56.0 - 30.0;
                                    self.climb_stage = 4;
                                }
                            }
                            _ => {
                                self.simon_x = 2.0;
                                self.simon_y = 56.0 - 30.0;
                                self.phase = Phase::WhipWindup;
                                self.phase_timer = 0.0;
                            }
                        }
                    }
                } else {
                    let target_x = if h >= 48 { 20.0 } else { 6.0 };
                    self.simon_x += 65.0 * dt;
                    if self.simon_x >= target_x {
                        self.simon_x = target_x;
                        self.phase = Phase::WhipWindup;
                        self.phase_timer = 0.0;
                    }
                }
            }
            Phase::WhipWindup => {
                self.phase_timer += dt;
                if self.phase_timer >= 0.15 {
                    self.phase = Phase::WhipStrike;
                    self.phase_timer = 0.0;
                }
            }
            Phase::WhipStrike => {
                self.phase_timer += dt;
                if self.phase_timer >= 0.20 {
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
                    self.simon_x += 60.0 * dt;
                    self.simon_y = (if w >= 64 { 70.0 } else { 56.0 }) - 30.0;
                    if self.simon_x > w as f32 + 24.0 {
                        self.phase = Phase::Idle;
                        self.phase_timer = 0.0;
                    }
                } else {
                    self.simon_x += 75.0 * dt;
                    if self.simon_x > w as f32 + 24.0 {
                        self.phase = Phase::Idle;
                        self.phase_timer = 0.0;
                    }
                }
            }
        }

        // --- Render frame ---
        self.draw_scene(matrix, w, h);

        let time_buf = format!("{}:{}", self.shown_hh, self.shown_mm);
        let crimson = (248u8, 56u8, 0u8);

        let step_cycle = self.walk_frame % 4;
        let wf = if step_cycle == 3 { 1 } else { step_cycle };
        let walk_pal = SIMON_WALK_PALS[wf];
        let walk_pix = SIMON_WALK_PIXELS[wf];

        if is_portrait {
            let time_shadow = (120u8, 10u8, 10u8);
            let time_ivory = (255u8, 245u8, 220u8);
            if w >= 64 {
                Self::draw_gothic_time(matrix, 11, 35, &time_buf, time_shadow, 2);
                Self::draw_gothic_time(matrix, 10, 34, &time_buf, time_ivory, 2);
            } else {
                let x_pos = 3;
                let hh_chars: Vec<char> = self.shown_hh.chars().collect();
                let mm_chars: Vec<char> = self.shown_mm.chars().collect();
                let h0 = hh_chars.first().copied().unwrap_or('0');
                let h1 = hh_chars.get(1).copied().unwrap_or('0');
                let m0 = mm_chars.first().copied().unwrap_or('0');
                let m1 = mm_chars.get(1).copied().unwrap_or('0');

                Self::draw_gothic_digit(matrix, x_pos + 1, 19, h0, time_shadow, 2);
                Self::draw_gothic_digit(matrix, x_pos + 9, 19, h1, time_shadow, 2);
                Self::draw_gothic_digit(matrix, x_pos, 18, h0, time_ivory, 2);
                Self::draw_gothic_digit(matrix, x_pos + 8, 18, h1, time_ivory, 2);

                Self::draw_gothic_digit(matrix, x_pos + 1, 37, m0, time_shadow, 2);
                Self::draw_gothic_digit(matrix, x_pos + 9, 37, m1, time_shadow, 2);
                Self::draw_gothic_digit(matrix, x_pos, 36, m0, time_ivory, 2);
                Self::draw_gothic_digit(matrix, x_pos + 8, 36, m1, time_ivory, 2);
            }

            if self.phase != Phase::Idle {
                if self.phase == Phase::WhipWindup {
                    Self::blit_sprite(
                        matrix,
                        &SIMON_IDLE_PAL,
                        &SIMON_WHIP_WINDUP_PIXELS,
                        SIMON_WHIP_WINDUP_W,
                        SIMON_WHIP_WINDUP_H,
                        self.simon_x as i32 - 8,
                        self.simon_y as i32,
                        false,
                    );
                } else if self.phase == Phase::WhipStrike || self.phase == Phase::Impact {
                    Self::blit_sprite(
                        matrix,
                        &SIMON_IDLE_PAL,
                        &SIMON_WHIP_STRIKE_PIXELS,
                        SIMON_WHIP_STRIKE_W,
                        SIMON_WHIP_STRIKE_H,
                        self.simon_x as i32,
                        self.simon_y as i32,
                        false,
                    );
                    Self::blit_sprite(
                        matrix,
                        &SIMON_IDLE_PAL,
                        &WHIP_EXTENDED_PIXELS,
                        WHIP_EXTENDED_W,
                        WHIP_EXTENDED_H,
                        self.simon_x as i32 + SIMON_WHIP_STRIKE_W,
                        self.simon_y as i32 + 11,
                        false,
                    );
                    if self.phase == Phase::Impact {
                        let spark_offset = if w >= 64 { 16 } else { 8 };
                        matrix.fill_circle(
                            self.simon_x as i32 + SIMON_WHIP_STRIKE_W + spark_offset,
                            self.simon_y as i32 + 12,
                            3,
                            255,
                            255,
                            255,
                        );
                    }
                } else {
                    let is_jumping = self.phase == Phase::HeroEnter
                        && (self.climb_stage == 1
                            || self.climb_stage == 3
                            || self.climb_stage == 5);
                    let flip_h = is_portrait
                        && w < 64
                        && self.phase == Phase::HeroEnter
                        && self.climb_stage == 2;
                    let p_pix = if is_jumping {
                        SIMON_WALK_PIXELS[1]
                    } else {
                        walk_pix
                    };
                    Self::blit_sprite(
                        matrix,
                        walk_pal,
                        p_pix,
                        16,
                        30,
                        self.simon_x as i32,
                        self.simon_y as i32,
                        flip_h,
                    );
                }
            }
        } else if h >= 48 {
            // --- 256x64 Layout: Blood Red Gothic Digits Centered (scale 4 = 88px wide) ---
            Self::draw_gothic_time(matrix, 84, 16, &time_buf, crimson, 4);

            if self.phase != Phase::Idle {
                if self.phase == Phase::WhipWindup {
                    Self::blit_sprite(
                        matrix,
                        &SIMON_IDLE_PAL,
                        &SIMON_WHIP_WINDUP_PIXELS,
                        SIMON_WHIP_WINDUP_W,
                        SIMON_WHIP_WINDUP_H,
                        self.simon_x as i32 - 8,
                        self.simon_y as i32,
                        false,
                    );
                } else if self.phase == Phase::WhipStrike || self.phase == Phase::Impact {
                    Self::blit_sprite(
                        matrix,
                        &SIMON_IDLE_PAL,
                        &SIMON_WHIP_STRIKE_PIXELS,
                        SIMON_WHIP_STRIKE_W,
                        SIMON_WHIP_STRIKE_H,
                        self.simon_x as i32,
                        self.simon_y as i32,
                        false,
                    );
                    Self::blit_sprite(
                        matrix,
                        &SIMON_IDLE_PAL,
                        &WHIP_EXTENDED_PIXELS,
                        WHIP_EXTENDED_W,
                        WHIP_EXTENDED_H,
                        self.simon_x as i32 + SIMON_WHIP_STRIKE_W,
                        self.simon_y as i32 + 11,
                        false,
                    );
                    if self.phase == Phase::Impact {
                        matrix.fill_circle(
                            self.simon_x as i32 + SIMON_WHIP_STRIKE_W + 38,
                            self.simon_y as i32 + 12,
                            3,
                            255,
                            255,
                            255,
                        );
                    }
                } else {
                    Self::blit_sprite(
                        matrix,
                        walk_pal,
                        walk_pix,
                        16,
                        30,
                        self.simon_x as i32,
                        self.simon_y as i32,
                        false,
                    );
                }
            }
        } else {
            // --- 128x32 Layout: Blood Red Gothic Digits Centered (scale 3 = 66px wide) ---
            Self::draw_gothic_time(matrix, 31, 8, &time_buf, crimson, 3);

            if self.phase != Phase::Idle {
                if self.phase == Phase::WhipWindup {
                    Self::blit_sprite(
                        matrix,
                        &SIMON_IDLE_PAL,
                        &SIMON_WHIP_WINDUP_PIXELS,
                        SIMON_WHIP_WINDUP_W,
                        SIMON_WHIP_WINDUP_H,
                        self.simon_x as i32 - 8,
                        self.simon_y as i32,
                        false,
                    );
                } else if self.phase == Phase::WhipStrike || self.phase == Phase::Impact {
                    Self::blit_sprite(
                        matrix,
                        &SIMON_IDLE_PAL,
                        &SIMON_WHIP_STRIKE_PIXELS,
                        SIMON_WHIP_STRIKE_W,
                        SIMON_WHIP_STRIKE_H,
                        self.simon_x as i32,
                        self.simon_y as i32,
                        false,
                    );
                    Self::blit_sprite(
                        matrix,
                        &SIMON_IDLE_PAL,
                        &WHIP_EXTENDED_PIXELS,
                        WHIP_EXTENDED_W,
                        WHIP_EXTENDED_H,
                        self.simon_x as i32 + SIMON_WHIP_STRIKE_W,
                        self.simon_y as i32 + 11,
                        false,
                    );
                    if self.phase == Phase::Impact {
                        matrix.fill_circle(
                            self.simon_x as i32 + SIMON_WHIP_STRIKE_W + 38,
                            self.simon_y as i32 + 12,
                            2,
                            255,
                            255,
                            255,
                        );
                    }
                } else {
                    Self::blit_sprite(
                        matrix,
                        walk_pal,
                        walk_pix,
                        16,
                        30,
                        self.simon_x as i32,
                        self.simon_y as i32,
                        false,
                    );
                }
            }
        }
    }
}

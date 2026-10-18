use crate::core::matrix::IDrawingSurface;
use crate::engines::clocks::cwassets::megaman::*;
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

const SKYLINE_BUILDINGS: [(i32, i32, i32); 10] = [
    (0, 35, 18),
    (20, 25, 16),
    (40, 42, 22),
    (65, 30, 25),
    (95, 20, 20),
    (120, 38, 22),
    (145, 28, 18),
    (165, 45, 24),
    (192, 22, 26),
    (222, 34, 30),
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

#[derive(PartialEq, Clone, Copy)]
enum Phase {
    Idle,
    HeroEnter,
    HeroJump,
    HeroExit,
}

pub struct MegamanClock {
    phase: Phase,
    phase_timer: f32,
    run_timer: f32,
    run_frame: usize,
    climb_stage: u8,
    jump_t: f32,

    megaman_x: f32,
    megaman_y: f32,

    is_charge_shot: bool,
    bullet_active: bool,
    bullet_start_x: f32,
    bullet_start_y: f32,
    bullet_x: f32,
    bullet_y: f32,
    bullet_target_x: f32,
    bullet_target_y: f32,

    spark_active: bool,
    spark_timer: f32,
    pod_bounce: f32,

    shown_hh: String,
    shown_mm: String,
    last_minute: i32,
    last_second: i32,
    snap_to_now: bool,
    last_tick: Instant,
}

impl Default for MegamanClock {
    fn default() -> Self {
        Self::new()
    }
}

impl MegamanClock {
    pub fn new() -> Self {
        Self {
            phase: Phase::Idle,
            phase_timer: 0.0,
            run_timer: 0.0,
            run_frame: 0,
            climb_stage: 0,
            jump_t: 0.0,
            megaman_x: -50.0,
            megaman_y: 0.0,
            is_charge_shot: false,
            bullet_active: false,
            bullet_start_x: 0.0,
            bullet_start_y: 0.0,
            bullet_x: 0.0,
            bullet_y: 0.0,
            bullet_target_x: 0.0,
            bullet_target_y: 0.0,
            spark_active: false,
            spark_timer: 0.0,
            pod_bounce: 0.0,
            shown_hh: "00".to_string(),
            shown_mm: "00".to_string(),
            last_minute: -1,
            last_second: -1,
            snap_to_now: true,
            last_tick: Instant::now(),
        }
    }

    pub fn on_activated(&mut self) {
        self.snap_to_now = true;
    }

    pub fn on_display_geometry_changed(&mut self) {
        self.snap_to_now = true;
        self.climb_stage = 0;
        self.jump_t = 0.0;
        self.phase = Phase::Idle;
    }

    fn blit_sprite(
        surface: &mut dyn IDrawingSurface,
        data: &[u16],
        w: i32,
        h: i32,
        x: i32,
        y: i32,
        transparent: bool,
        flip_h: bool,
    ) {
        let panel_w = surface.width() as i32;
        let panel_h = surface.height() as i32;
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
                if transparent && c == MASK {
                    continue;
                }
                let (r, g, b) = rgb565_to_rgb888(c);
                surface.draw_pixel(px, py, r, g, b);
            }
        }
    }

    fn draw_capcom_digit(
        surface: &mut dyn IDrawingSurface,
        x: i32,
        y: i32,
        c: u8,
        color: (u8, u8, u8),
    ) {
        let idx = if c.is_ascii_digit() {
            (c - b'0') as usize
        } else if c == b':' {
            10
        } else {
            return;
        };

        for row in 0..7i32 {
            let row_bits = CAPCOM_FONT[idx][row as usize];
            for col in 0..5i32 {
                if ((row_bits >> (4 - col)) & 1) != 0 {
                    surface.draw_pixel(x + col, y + row, color.0, color.1, color.2);
                }
            }
        }
    }

    fn draw_digit_4x6(
        surface: &mut dyn IDrawingSurface,
        px: i32,
        py: i32,
        c: u8,
        color: (u8, u8, u8),
    ) {
        if !c.is_ascii_digit() {
            return;
        }
        let idx = (c - b'0') as usize;
        for r in 0..6i32 {
            let row = DIGITS_4X6[idx][r as usize];
            for b in 0..4i32 {
                if (row & (0x8 >> b)) != 0 {
                    surface.draw_pixel(px + b, py + r, color.0, color.1, color.2);
                }
            }
        }
    }

    fn draw_time_pod(
        surface: &mut dyn IDrawingSurface,
        x: i32,
        y: i32,
        text: &str,
        bounce: i32,
        highlight: bool,
        downscaled: bool,
    ) {
        let py = y - bounce;
        let border_col = if highlight {
            (255, 255, 255)
        } else {
            (0, 112, 236)
        };
        let inner_col = if highlight {
            (0, 232, 216)
        } else {
            (0, 40, 80)
        };
        let bg_col = (10, 16, 28);
        let text_col = if highlight {
            (255, 255, 255)
        } else {
            (0, 232, 216)
        };

        let pw = if downscaled { 14 } else { POD_W };
        let ph = if downscaled { 14 } else { POD_H };

        surface.fill_rect(x, py, pw, ph, bg_col.0, bg_col.1, bg_col.2);
        surface.draw_rect(x, py, pw, ph, border_col.0, border_col.1, border_col.2);
        surface.fill_rect(
            x + 1,
            py + 1,
            pw - 2,
            1,
            inner_col.0,
            inner_col.1,
            inner_col.2,
        );
        surface.fill_rect(
            x + 1,
            py + ph - 2,
            pw - 2,
            1,
            inner_col.0,
            inner_col.1,
            inner_col.2,
        );

        let bytes = text.as_bytes();
        let len = bytes.len() as i32;
        if downscaled {
            let tx = x + if len == 2 { 2 } else { 5 };
            let ty = py + 4;
            for (i, &b) in bytes.iter().enumerate() {
                Self::draw_digit_4x6(surface, tx + (i as i32) * 5, ty, b, text_col);
            }
        } else {
            let total_w = if len == 2 { 11 } else { 5 };
            let tx = x + (POD_W - total_w) / 2;
            let ty = py + (POD_H - 7) / 2;
            for (i, &b) in bytes.iter().enumerate() {
                Self::draw_capcom_digit(surface, tx + (i as i32) * 6, ty, b, text_col);
            }
        }
    }

    fn draw_energy_gauge(surface: &mut dyn IDrawingSurface, x: i32, y: i32, seconds: i32) {
        let bars = (seconds.clamp(0, 59) * 28) / 59;
        surface.fill_rect(x, y, 6, 30, 0, 0, 0);
        surface.draw_rect(x, y, 6, 30, 255, 255, 255);

        for b in 0..28i32 {
            let by = y + 28 - b;
            if b < bars {
                let col = if b % 2 == 0 {
                    (252, 224, 0)
                } else {
                    (255, 255, 255)
                };
                surface.fill_rect(x + 1, by, 4, 1, col.0, col.1, col.2);
            }
        }
    }

    fn draw_mini_energy_gauge(surface: &mut dyn IDrawingSurface, x: i32, y: i32, seconds: i32) {
        surface.fill_rect(x, y, 22, 4, 0, 0, 0);
        surface.draw_rect(x, y, 22, 4, 0, 112, 236);
        let fill_len = (seconds.clamp(0, 59) * 20) / 59;
        if fill_len > 0 {
            surface.fill_rect(x + 1, y + 1, fill_len, 1, 252, 224, 0);
            surface.fill_rect(x + 1, y + 2, fill_len, 1, 255, 255, 255);
        }
    }

    fn draw_tech_platform(
        surface: &mut dyn IDrawingSurface,
        start_x: i32,
        start_y: i32,
        tile_count: i32,
    ) {
        for i in 0..tile_count {
            Self::blit_sprite(
                surface,
                &GROUND_TILE,
                GROUND_TILE_W,
                GROUND_TILE_H,
                start_x + i * GROUND_TILE_W,
                start_y,
                false,
                false,
            );
        }
    }

    fn draw_scene(surface: &mut dyn IDrawingSurface, w: i32, h: i32) {
        let is_portrait = h > w;

        // Night sky vertical gradient
        let denom = if h > 0 { h } else { 1 };
        for y in 0..h {
            let grad = (10 + (y * 22) / denom).clamp(0, 255) as u8;
            surface.fill_rect(0, y, w, 1, 6, 10, grad);
        }

        // Skyline silhouettes
        for &(bx, bh, bw_raw) in &SKYLINE_BUILDINGS {
            if bx >= w {
                continue;
            }
            let bw = if bx + bw_raw > w { w - bx } else { bw_raw };
            let by = (h - bh).max(0);
            surface.fill_rect(bx, by, bw, h - by, 16, 24, 40);

            let mut wx = bx + 3;
            while wx < bx + bw - 3 {
                let mut wy = by + 4;
                while wy < h - 8 {
                    if (wx + wy) % 7 == 0 {
                        surface.draw_pixel(wx, wy, 252, 224, 0);
                    } else if (wx + wy) % 11 == 0 {
                        surface.draw_pixel(wx, wy, 0, 232, 216);
                    }
                    wy += 6;
                }
                wx += 5;
            }
        }

        if is_portrait {
            if w >= 64 {
                Self::draw_tech_platform(surface, 16, 190, 3);
                Self::draw_tech_platform(surface, 8, 130, 3);
                Self::blit_sprite(
                    surface,
                    &METOOL_SLEEP,
                    METOOL_W,
                    METOOL_H,
                    38,
                    130 - METOOL_H,
                    true,
                    false,
                );
                Self::draw_tech_platform(surface, 8, 70, 3);
            } else {
                Self::draw_tech_platform(surface, 0, 56, 2);
                Self::draw_tech_platform(surface, 8, 88, 2);
            }

            let ground_top = h - GROUND_TILE_H;
            let mut gx = 0;
            while gx < w {
                Self::blit_sprite(
                    surface,
                    &GROUND_TILE,
                    GROUND_TILE_W,
                    GROUND_TILE_H,
                    gx,
                    ground_top,
                    false,
                    false,
                );
                gx += GROUND_TILE_W;
            }
        } else {
            let ground_top = h - GROUND_TILE_H;
            let mut gx = 0;
            while gx < w {
                Self::blit_sprite(
                    surface,
                    &GROUND_TILE,
                    GROUND_TILE_W,
                    GROUND_TILE_H,
                    gx,
                    ground_top,
                    false,
                    false,
                );
                gx += GROUND_TILE_W;
            }
        }
    }

    pub fn render(
        &mut self,
        surface: &mut dyn IDrawingSurface,
        hours: u32,
        minutes: u32,
        seconds: u32,
    ) {
        let now = Instant::now();
        let dt = now
            .duration_since(self.last_tick)
            .as_secs_f32()
            .clamp(0.001, 0.2);
        self.last_tick = now;
        self.draw(surface, hours, minutes, seconds, dt);
    }

    pub fn draw(
        &mut self,
        surface: &mut dyn IDrawingSurface,
        hours: u32,
        minutes: u32,
        seconds: u32,
        dt: f32,
    ) {
        let w = surface.width() as i32;
        let h = surface.height() as i32;

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
            self.run_timer = 0.0;
            self.run_frame = 0;
            self.megaman_x = -50.0;
            self.bullet_active = false;
            self.spark_active = false;
            self.pod_bounce = 0.0;
        }

        if self.last_second != seconds as i32 {
            self.last_second = seconds as i32;
        }

        // Minute change trigger: Mega Man enters running to fire his Buster at the clock!
        if self.last_minute != -1
            && (minutes as i32) != self.last_minute
            && self.phase == Phase::Idle
        {
            self.phase = Phase::HeroEnter;
            self.phase_timer = 0.0;
            self.run_timer = 0.0;
            self.run_frame = 0;
            self.climb_stage = 0;
            self.jump_t = 0.0;
            if h > w {
                self.megaman_x = -20.0;
                self.megaman_y = (h - GROUND_TILE_H - MEGAMAN_RUN1_H) as f32;
            } else {
                self.megaman_x = -30.0;
            }
            self.is_charge_shot = minutes == 0;
            self.bullet_active = false;
            self.spark_active = false;
        }

        // Run animation step cycle
        if self.phase == Phase::HeroEnter || self.phase == Phase::HeroExit {
            self.run_timer += dt;
            if self.run_timer >= 0.09 {
                self.run_timer -= 0.09;
                self.run_frame = self.run_frame.wrapping_add(1);
            }
        }

        let is_portrait = h > w;
        let is_wide_tall = h >= 48;
        let ground_top = h - if is_wide_tall { GROUND_TILE_H } else { 6 };

        let (hour_x, min_x, pod_y) = if is_portrait {
            if w >= 64 {
                (12, 38, 10)
            } else {
                (1, 17, 18)
            }
        } else if is_wide_tall {
            let mid_x = w / 2;
            (mid_x - 28, mid_x + 4, 10)
        } else {
            (w - 54, w - 27, 6)
        };

        let jump_start_x = (hour_x as f32 - if is_wide_tall { 44.0 } else { 32.0 }).max(4.0);

        match self.phase {
            Phase::Idle => {}
            Phase::HeroEnter => {
                if is_portrait {
                    if w >= 64 {
                        let floor_y = (h - GROUND_TILE_H - MEGAMAN_RUN1_H) as f32;
                        if self.climb_stage == 0 {
                            self.megaman_x += 60.0 * dt;
                            self.megaman_y = floor_y;
                            if self.megaman_x >= 12.0 {
                                self.climb_stage = 1;
                                self.jump_t = 0.0;
                            }
                        } else if self.climb_stage == 1 {
                            self.jump_t += dt * 2.6;
                            let t = self.jump_t.min(1.0);
                            self.megaman_x = 12.0 + (36.0 - 12.0) * t;
                            let base_y = floor_y + ((190.0 - MEGAMAN_RUN1_H as f32) - floor_y) * t;
                            let arc = 4.0 * 14.0 * t * (1.0 - t);
                            self.megaman_y = base_y - arc;
                            if self.jump_t >= 1.0 {
                                self.jump_t = 0.0;
                                self.megaman_y = 190.0 - MEGAMAN_RUN1_H as f32;
                                self.climb_stage = 2;
                            }
                        } else if self.climb_stage == 2 {
                            self.megaman_x -= 50.0 * dt;
                            self.megaman_y = 190.0 - MEGAMAN_RUN1_H as f32;
                            if self.megaman_x <= 30.0 {
                                self.climb_stage = 3;
                                self.jump_t = 0.0;
                            }
                        } else if self.climb_stage == 3 {
                            self.jump_t += dt * 2.8;
                            let t = self.jump_t.min(1.0);
                            self.megaman_x = 30.0 + (18.0 - 30.0) * t;
                            let base_y = (190.0 - MEGAMAN_RUN1_H as f32)
                                + ((130.0 - MEGAMAN_RUN1_H as f32)
                                    - (190.0 - MEGAMAN_RUN1_H as f32))
                                    * t;
                            let arc = 4.0 * 14.0 * t * (1.0 - t);
                            self.megaman_y = base_y - arc;
                            if self.jump_t >= 1.0 {
                                self.jump_t = 0.0;
                                self.megaman_y = 130.0 - MEGAMAN_RUN1_H as f32;
                                self.climb_stage = 4;
                            }
                        } else if self.climb_stage == 4 {
                            self.megaman_x += 50.0 * dt;
                            self.megaman_y = 130.0 - MEGAMAN_RUN1_H as f32;
                            if self.megaman_x >= 24.0 {
                                self.climb_stage = 5;
                                self.jump_t = 0.0;
                            }
                        } else if self.climb_stage == 5 {
                            self.jump_t += dt * 2.8;
                            let t = self.jump_t.min(1.0);
                            self.megaman_x = 24.0 + (16.0 - 24.0) * t;
                            let base_y = (130.0 - MEGAMAN_RUN1_H as f32)
                                + ((70.0 - MEGAMAN_RUN1_H as f32)
                                    - (130.0 - MEGAMAN_RUN1_H as f32))
                                    * t;
                            let arc = 4.0 * 14.0 * t * (1.0 - t);
                            self.megaman_y = base_y - arc;
                            if self.jump_t >= 1.0 {
                                self.jump_t = 0.0;
                                self.megaman_y = 70.0 - MEGAMAN_RUN1_H as f32;
                                self.climb_stage = 6;
                            }
                        } else if self.climb_stage == 6 {
                            self.megaman_x += 50.0 * dt;
                            self.megaman_y = 70.0 - MEGAMAN_RUN1_H as f32;
                            if self.megaman_x >= 18.0 {
                                self.megaman_x = 18.0;
                                self.phase = Phase::HeroJump;
                                self.phase_timer = 0.0;
                            }
                        }
                    } else {
                        let floor_y = (h - GROUND_TILE_H - MEGAMAN_RUN1_H) as f32;
                        if self.climb_stage == 0 {
                            self.megaman_x += 60.0 * dt;
                            self.megaman_y = floor_y;
                            if self.megaman_x >= 8.0 {
                                self.climb_stage = 1;
                                self.jump_t = 0.0;
                            }
                        } else if self.climb_stage == 1 {
                            self.jump_t += dt * 2.8;
                            let t = self.jump_t.min(1.0);
                            self.megaman_x = 8.0 + (20.0 - 8.0) * t;
                            let base_y = floor_y + ((88.0 - MEGAMAN_RUN1_H as f32) - floor_y) * t;
                            let arc = 4.0 * 12.0 * t * (1.0 - t);
                            self.megaman_y = base_y - arc;
                            if self.jump_t >= 1.0 {
                                self.jump_t = 0.0;
                                self.megaman_y = 88.0 - MEGAMAN_RUN1_H as f32;
                                self.climb_stage = 2;
                            }
                        } else if self.climb_stage == 2 {
                            self.megaman_x -= 50.0 * dt;
                            self.megaman_y = 88.0 - MEGAMAN_RUN1_H as f32;
                            if self.megaman_x <= 10.0 {
                                self.climb_stage = 3;
                                self.jump_t = 0.0;
                            }
                        } else if self.climb_stage == 3 {
                            self.jump_t += dt * 2.8;
                            let t = self.jump_t.min(1.0);
                            self.megaman_x = 10.0 + (6.0 - 10.0) * t;
                            let base_y = (88.0 - MEGAMAN_RUN1_H as f32)
                                + ((56.0 - MEGAMAN_RUN1_H as f32) - (88.0 - MEGAMAN_RUN1_H as f32))
                                    * t;
                            let arc = 4.0 * 12.0 * t * (1.0 - t);
                            self.megaman_y = base_y - arc;
                            if self.jump_t >= 1.0 {
                                self.jump_t = 0.0;
                                self.megaman_y = 56.0 - MEGAMAN_RUN1_H as f32;
                                self.climb_stage = 4;
                            }
                        } else if self.climb_stage == 4 {
                            self.megaman_x = 6.0;
                            self.megaman_y = 56.0 - MEGAMAN_RUN1_H as f32;
                            self.phase = Phase::HeroJump;
                            self.phase_timer = 0.0;
                        }
                    }
                } else if is_wide_tall {
                    self.megaman_x += 85.0 * dt;
                    self.megaman_y = (ground_top - MEGAMAN_RUN1_H) as f32;
                    if self.megaman_x >= jump_start_x {
                        self.phase = Phase::HeroJump;
                        self.phase_timer = 0.0;
                    }
                } else {
                    self.megaman_x += 85.0 * dt;
                    self.megaman_y = (ground_top - MEGAMAN_RUN1_H) as f32;
                    let shoot_start_x = 18.0;
                    if self.megaman_x >= shoot_start_x {
                        self.megaman_x = shoot_start_x;
                        self.phase = Phase::HeroJump;
                        self.phase_timer = 0.0;
                    }
                }
            }
            Phase::HeroJump => {
                self.phase_timer += dt;

                if is_portrait {
                    let jump_duration = 0.85;
                    let p = (self.phase_timer / jump_duration).min(1.0);
                    let start_x = if w >= 64 { 18.0 } else { 4.0 };
                    let travel_x = if w >= 64 { 10.0 } else { 4.0 };
                    let base_plat_y = if w >= 64 { 70.0 } else { 56.0 };
                    self.megaman_x = start_x + travel_x * p;
                    let max_lift = 14.0;
                    let lift = (p * std::f32::consts::PI).sin() * max_lift;
                    self.megaman_y = (base_plat_y - MEGAMAN_JUMP_H as f32) - lift;

                    if p >= 0.32 && !self.bullet_active && !self.spark_active {
                        self.bullet_active = true;
                        self.bullet_start_x = self.megaman_x + MEGAMAN_JUMP_SHOOT_W as f32 - 2.0;
                        self.bullet_start_y = self.megaman_y + 6.0;
                        self.bullet_x = self.bullet_start_x;
                        self.bullet_y = self.bullet_start_y;
                        self.bullet_target_x = (min_x + 6) as f32;
                        self.bullet_target_y = (pod_y + 6) as f32;
                    }

                    if p >= 1.0 {
                        self.phase = Phase::HeroExit;
                        self.phase_timer = 0.0;
                    }
                } else if is_wide_tall {
                    let jump_duration = 0.75;
                    let p = (self.phase_timer / jump_duration).min(1.0);
                    self.megaman_x += 80.0 * dt;
                    let max_lift = 24.0;
                    let lift = (p * std::f32::consts::PI).sin() * max_lift;
                    self.megaman_y = (ground_top - MEGAMAN_JUMP_H) as f32 - lift;

                    if p >= 0.32 && !self.bullet_active && !self.spark_active {
                        self.bullet_active = true;
                        self.bullet_start_x = self.megaman_x + MEGAMAN_JUMP_SHOOT_W as f32 - 2.0;
                        self.bullet_start_y = self.megaman_y + 8.0;
                        self.bullet_x = self.bullet_start_x;
                        self.bullet_y = self.bullet_start_y;
                        self.bullet_target_x = (min_x + 6) as f32;
                        self.bullet_target_y = (pod_y + 12) as f32;
                    }

                    if p >= 1.0 {
                        self.phase = Phase::HeroExit;
                        self.phase_timer = 0.0;
                    }
                } else {
                    // 128x32 horizontal: Mega Man advances straight without jumping, stops and fires Buster lemon
                    self.megaman_y = (ground_top - MEGAMAN_SHOOT_H) as f32;

                    if self.phase_timer >= 0.06 && !self.bullet_active && !self.spark_active {
                        self.bullet_active = true;
                        self.bullet_start_x = self.megaman_x + MEGAMAN_SHOOT_W as f32 - 2.0;
                        self.bullet_start_y = self.megaman_y + 8.0;
                        self.bullet_x = self.bullet_start_x;
                        self.bullet_y = self.bullet_start_y;
                        self.bullet_target_x = (min_x + 6) as f32;
                        self.bullet_target_y = (pod_y + 7) as f32;
                    }

                    if self.phase_timer >= 0.42 {
                        self.phase = Phase::HeroExit;
                        self.phase_timer = 0.0;
                    }
                }
            }
            Phase::HeroExit => {
                self.megaman_x += (if is_portrait { 85.0 } else { 95.0 }) * dt;
                self.megaman_y = if is_portrait {
                    (if w >= 64 { 70.0 } else { 56.0 }) - MEGAMAN_RUN1_H as f32
                } else {
                    (ground_top - MEGAMAN_RUN1_H) as f32
                };
                if self.megaman_x > w as f32 + 24.0 {
                    self.phase = Phase::Idle;
                    self.phase_timer = 0.0;
                    self.megaman_x = -50.0;
                }
            }
        }

        // Projectile physics (flies across screen to minute pod)
        if self.bullet_active {
            let bullet_speed = 420.0 * dt;
            self.bullet_x += bullet_speed;
            if self.bullet_target_x > self.bullet_start_x {
                let prog = ((self.bullet_x - self.bullet_start_x)
                    / (self.bullet_target_x - self.bullet_start_x))
                    .clamp(0.0, 1.0);
                self.bullet_y =
                    self.bullet_start_y + (self.bullet_target_y - self.bullet_start_y) * prog;
            }
            if self.bullet_x >= self.bullet_target_x {
                self.bullet_active = false;
                self.spark_active = true;
                self.spark_timer = 0.0;
                self.pod_bounce = if self.is_charge_shot { 5.0 } else { 3.5 };
                self.shown_hh = hh;
                self.shown_mm = mm;
                self.last_minute = minutes as i32;
            }
        }

        // Hit Spark physics
        if self.spark_active {
            self.spark_timer += dt;
            self.pod_bounce *= 0.85;
            if self.spark_timer >= 0.22 {
                self.spark_active = false;
                self.pod_bounce = 0.0;
            }
        }

        // --- Render frame ---
        Self::draw_scene(surface, w, h);

        let bounce_int = self.pod_bounce as i32;
        let downscale = is_portrait && w < 64;
        Self::draw_time_pod(
            surface,
            hour_x,
            pod_y,
            &self.shown_hh,
            if self.is_charge_shot { bounce_int } else { 0 },
            self.is_charge_shot && self.spark_active,
            downscale,
        );
        Self::draw_time_pod(
            surface,
            min_x,
            pod_y,
            &self.shown_mm,
            bounce_int,
            self.spark_active,
            downscale,
        );

        // Energy gauge
        if is_portrait {
            if w >= 64 {
                Self::draw_energy_gauge(surface, 3, 8, seconds as i32);
            } else {
                Self::draw_mini_energy_gauge(surface, 1, h - 5, seconds as i32);
            }
        } else if is_wide_tall {
            Self::draw_energy_gauge(surface, 3, 6, seconds as i32);
            if w >= 128 {
                let metool_x = w - METOOL_W - 4;
                Self::blit_sprite(
                    surface,
                    &METOOL_SLEEP,
                    METOOL_W,
                    METOOL_H,
                    metool_x,
                    ground_top - METOOL_H,
                    true,
                    false,
                );
            }
        } else {
            Self::draw_mini_energy_gauge(surface, 6, 2, seconds as i32);
        }

        // Mega Man rendering (appears only during minute sequence)
        if self.phase == Phase::HeroEnter || self.phase == Phase::HeroExit {
            let is_jumping = is_portrait
                && self.phase == Phase::HeroEnter
                && (self.climb_stage == 1 || self.climb_stage == 3 || self.climb_stage == 5);
            let flip_h = is_portrait
                && self.phase == Phase::HeroEnter
                && if w >= 64 {
                    self.climb_stage == 2 || self.climb_stage == 3 || self.climb_stage == 5
                } else {
                    self.climb_stage == 2 || self.climb_stage == 3
                };
            if is_jumping {
                Self::blit_sprite(
                    surface,
                    &MEGAMAN_JUMP,
                    MEGAMAN_JUMP_W,
                    MEGAMAN_JUMP_H,
                    self.megaman_x as i32,
                    self.megaman_y as i32,
                    true,
                    flip_h,
                );
            } else {
                let (run_sprite, rw, rh): (&[u16], i32, i32) = match self.run_frame % 4 {
                    0 => (&MEGAMAN_RUN1, MEGAMAN_RUN1_W, MEGAMAN_RUN1_H),
                    1 => (&MEGAMAN_RUN_PASS1, MEGAMAN_RUN_PASS_W, MEGAMAN_RUN_PASS_H),
                    2 => (&MEGAMAN_RUN2, MEGAMAN_RUN2_W, MEGAMAN_RUN2_H),
                    _ => (&MEGAMAN_RUN_PASS2, MEGAMAN_RUN_PASS_W, MEGAMAN_RUN_PASS_H),
                };
                Self::blit_sprite(
                    surface,
                    run_sprite,
                    rw,
                    rh,
                    self.megaman_x as i32,
                    self.megaman_y as i32,
                    true,
                    flip_h,
                );
            }
        } else if self.phase == Phase::HeroJump {
            if is_portrait || is_wide_tall {
                let jump_dur = if is_portrait { 0.85 } else { 0.75 };
                let p = self.phase_timer / jump_dur;
                if !(0.28..0.65).contains(&p) {
                    Self::blit_sprite(
                        surface,
                        &MEGAMAN_JUMP,
                        MEGAMAN_JUMP_W,
                        MEGAMAN_JUMP_H,
                        self.megaman_x as i32,
                        self.megaman_y as i32,
                        true,
                        false,
                    );
                } else {
                    Self::blit_sprite(
                        surface,
                        &MEGAMAN_JUMP_SHOOT,
                        MEGAMAN_JUMP_SHOOT_W,
                        MEGAMAN_JUMP_SHOOT_H,
                        self.megaman_x as i32,
                        self.megaman_y as i32,
                        true,
                        false,
                    );
                    if p < 0.40 {
                        let aura = if ((self.phase_timer * 30.0) as i32) % 2 == 0 {
                            (0, 232, 216)
                        } else {
                            (252, 224, 0)
                        };
                        surface.draw_circle(
                            self.megaman_x as i32 + MEGAMAN_JUMP_SHOOT_W - 1,
                            self.megaman_y as i32 + 8,
                            3,
                            aura.0,
                            aura.1,
                            aura.2,
                        );
                    }
                }
            } else {
                // 128x32 horizontal: standing Buster cannon blast pose on ground
                Self::blit_sprite(
                    surface,
                    &MEGAMAN_SHOOT,
                    MEGAMAN_SHOOT_W,
                    MEGAMAN_SHOOT_H,
                    self.megaman_x as i32,
                    self.megaman_y as i32,
                    true,
                    false,
                );
                if self.phase_timer < 0.20 {
                    let aura = if ((self.phase_timer * 30.0) as i32) % 2 == 0 {
                        (0, 232, 216)
                    } else {
                        (252, 224, 0)
                    };
                    surface.draw_circle(
                        self.megaman_x as i32 + MEGAMAN_SHOOT_W - 1,
                        self.megaman_y as i32 + 8,
                        3,
                        aura.0,
                        aura.1,
                        aura.2,
                    );
                }
            }
        }

        // Bullet drawing
        if self.bullet_active {
            if self.is_charge_shot {
                surface.fill_circle(
                    self.bullet_x as i32 + 4,
                    self.bullet_y as i32 + 3,
                    5,
                    0,
                    232,
                    216,
                );
                surface.fill_circle(
                    self.bullet_x as i32 + 4,
                    self.bullet_y as i32 + 3,
                    3,
                    255,
                    255,
                    255,
                );
            } else {
                Self::blit_sprite(
                    surface,
                    &BUSTER_BULLET,
                    BULLET_W,
                    BULLET_H,
                    self.bullet_x as i32,
                    self.bullet_y as i32,
                    true,
                    false,
                );
            }
        }

        // Hit Spark drawing
        if self.spark_active {
            let pod_h = if downscale { 14 } else { POD_H };
            let spark_x = min_x - (SPARK_W / 2) + 2;
            let spark_y = pod_y + (pod_h / 2) - (SPARK_H / 2);
            Self::blit_sprite(
                surface, &HIT_SPARK, SPARK_W, SPARK_H, spark_x, spark_y, true, false,
            );
        }
    }
}

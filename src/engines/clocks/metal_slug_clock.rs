use crate::core::matrix::IDrawingSurface;
use crate::engines::clocks::cwassets::metal_slug::*;
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

#[derive(PartialEq, Clone, Copy)]
enum Phase {
    Idle,
    HeroEnter,
    Firefight,
    GrenadeAssault,
    Explosion,
    HeroExit,
}

const BACKDROPS: [(&[u16], &[u8]); 4] = [
    (&BG_STAGE_MARKET_PAL, &BG_STAGE_MARKET_PACKED),
    (&BG_STAGE_DOMES_PAL, &BG_STAGE_DOMES_PACKED),
    (&BG_STAGE_BUNKER_PAL, &BG_STAGE_BUNKER_PACKED),
    (&BG_STAGE_FORTRESS_PAL, &BG_STAGE_FORTRESS_PACKED),
];

const EXPLOSION_SPRITES: [(&[u16], &[u8], i32, i32); 4] = [
    (
        &EXPLOSION_0_PAL,
        &EXPLOSION_0_PIXELS,
        EXPLOSION_0_W,
        EXPLOSION_0_H,
    ),
    (
        &EXPLOSION_1_PAL,
        &EXPLOSION_1_PIXELS,
        EXPLOSION_1_W,
        EXPLOSION_1_H,
    ),
    (
        &EXPLOSION_2_PAL,
        &EXPLOSION_2_PIXELS,
        EXPLOSION_2_W,
        EXPLOSION_2_H,
    ),
    (
        &EXPLOSION_3_PAL,
        &EXPLOSION_3_PIXELS,
        EXPLOSION_3_W,
        EXPLOSION_3_H,
    ),
];

pub struct MetalSlugClock {
    phase: Phase,
    phase_timer: f32,
    anim_timer: f32,
    anim_frame: i32,
    walk_timer: f32,
    walk_frame: i32,
    explosion_timer: f32,
    explosion_frame: i32,

    marco_x: f32,
    tank_x: f32,
    heli_x: f32,

    tank_bullet_x: f32,
    tank_bullet_active: bool,

    grenade_x: f32,
    grenade_y: f32,
    grenade_vx: f32,
    grenade_vy: f32,
    grenade_active: bool,

    shown_hh: String,
    shown_mm: String,
    last_minute: i32,
    last_second: i32,
    snap_to_now: bool,
    current_backdrop_idx: usize,
    last_tick: Instant,
}

impl Default for MetalSlugClock {
    fn default() -> Self {
        Self::new()
    }
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
            explosion_timer: 0.0,
            explosion_frame: 0,
            marco_x: -50.0,
            tank_x: 185.0,
            heli_x: -60.0,
            tank_bullet_x: 0.0,
            tank_bullet_active: false,
            grenade_x: 0.0,
            grenade_y: 0.0,
            grenade_vx: 0.0,
            grenade_vy: 0.0,
            grenade_active: false,
            shown_hh: "10".to_string(),
            shown_mm: "42".to_string(),
            last_minute: -1,
            last_second: -1,
            snap_to_now: true,
            current_backdrop_idx: 0,
            last_tick: Instant::now(),
        }
    }

    pub fn on_activated(&mut self) {
        self.snap_to_now = true;
        self.current_backdrop_idx = (self.current_backdrop_idx + 1) % (NUM_BACKDROPS as usize);
    }

    pub fn on_display_geometry_changed(&mut self) {
        self.snap_to_now = true;
    }

    fn blit_sprite(
        surface: &mut dyn IDrawingSurface,
        palette: &[u16],
        pixels: &[u8],
        w: i32,
        h: i32,
        x: i32,
        y: i32,
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
                    surface.draw_pixel(px, py, r, g, b);
                }
            }
        }
    }

    fn blit_backdrop(
        surface: &mut dyn IDrawingSurface,
        palette: &[u16],
        packed_pixels: &[u8],
        w: i32,
        h: i32,
    ) {
        let panel_w = surface.width() as i32;
        let panel_h = surface.height() as i32;

        if panel_w < w && panel_h < h && panel_w > 0 && panel_h > 0 {
            let row_stride_bytes = (w * 3 / 4) as usize;
            for py in 0..panel_h {
                let sy = (py * h / panel_h).clamp(0, h - 1) as usize;
                let row_base = sy * row_stride_bytes;
                for px in 0..panel_w {
                    let sx = (px * w / panel_w).clamp(0, w - 1) as usize;
                    let group = sx / 4;
                    let sub = sx % 4;
                    let src = row_base + group * 3;
                    if src + 2 >= packed_pixels.len() {
                        continue;
                    }
                    let b0 = packed_pixels[src];
                    let b1 = packed_pixels[src + 1];
                    let b2 = packed_pixels[src + 2];
                    let idx = match sub {
                        0 => (b0 & 0x3F) as usize,
                        1 => (((b0 >> 6) & 0x03) | ((b1 & 0x0F) << 2)) as usize,
                        2 => (((b1 >> 4) & 0x0F) | ((b2 & 0x03) << 4)) as usize,
                        _ => ((b2 >> 2) & 0x3F) as usize,
                    };
                    if let Some(&c) = palette.get(idx) {
                        let (r, g, b) = rgb565_to_rgb888(c);
                        surface.draw_pixel(px, py, r, g, b);
                    }
                }
            }
            return;
        }

        let draw_w = w.min(panel_w);
        let draw_h = h.min(panel_h);

        for row in 0..draw_h {
            let mut src = (row * (w * 3 / 4)) as usize;
            let mut col = 0;
            while col < draw_w {
                if src + 2 >= packed_pixels.len() {
                    break;
                }
                let b0 = packed_pixels[src];
                let b1 = packed_pixels[src + 1];
                let b2 = packed_pixels[src + 2];
                src += 3;

                let p0 = (b0 & 0x3F) as usize;
                let p1 = (((b0 >> 6) & 0x03) | ((b1 & 0x0F) << 2)) as usize;
                let p2 = (((b1 >> 4) & 0x0F) | ((b2 & 0x03) << 4)) as usize;
                let p3 = ((b2 >> 2) & 0x3F) as usize;

                if col < draw_w {
                    if let Some(&c) = palette.get(p0) {
                        let (r, g, b) = rgb565_to_rgb888(c);
                        surface.draw_pixel(col, row, r, g, b);
                    }
                }
                if col + 1 < draw_w {
                    if let Some(&c) = palette.get(p1) {
                        let (r, g, b) = rgb565_to_rgb888(c);
                        surface.draw_pixel(col + 1, row, r, g, b);
                    }
                }
                if col + 2 < draw_w {
                    if let Some(&c) = palette.get(p2) {
                        let (r, g, b) = rgb565_to_rgb888(c);
                        surface.draw_pixel(col + 2, row, r, g, b);
                    }
                }
                if col + 3 < draw_w {
                    if let Some(&c) = palette.get(p3) {
                        let (r, g, b) = rgb565_to_rgb888(c);
                        surface.draw_pixel(col + 3, row, r, g, b);
                    }
                }
                col += 4;
            }
        }
    }

    fn draw_arcade_digit(
        surface: &mut dyn IDrawingSurface,
        x: i32,
        y: i32,
        c: u8,
        color: (u8, u8, u8),
        scale: i32,
    ) {
        let idx = if c.is_ascii_digit() {
            (c - b'0') as usize
        } else if c == b':' {
            10
        } else {
            return;
        };

        for r in 0..5i32 {
            let row_bits = ARCADE_FONT_3x5[idx][r as usize];
            let py = y + r * scale;
            for b in 0..3i32 {
                if ((row_bits >> (2 - b)) & 1) != 0 {
                    let px = x + b * scale;
                    surface.fill_rect(px, py, scale, scale, color.0, color.1, color.2);
                }
            }
        }
    }

    fn draw_arcade_time(
        surface: &mut dyn IDrawingSurface,
        start_x: i32,
        start_y: i32,
        s: &str,
        color: (u8, u8, u8),
        scale: i32,
    ) {
        let bytes = s.as_bytes();
        if bytes.len() < 5 {
            return;
        }
        let digit_w = 3 * scale;
        let digit_gap = scale;
        let s_pad = 2 * scale;

        let x_h1 = start_x;
        let x_h2 = x_h1 + digit_w + digit_gap;
        let x_colon = x_h2 + 2 * scale + s_pad;
        let x_m1 = x_colon + 2 * scale + s_pad;
        let x_m2 = x_m1 + digit_w + digit_gap;

        let shadow_color = (20, 10, 10);
        Self::draw_arcade_digit(
            surface,
            x_h1 + 1,
            start_y + 1,
            bytes[0],
            shadow_color,
            scale,
        );
        Self::draw_arcade_digit(
            surface,
            x_h2 + 1,
            start_y + 1,
            bytes[1],
            shadow_color,
            scale,
        );
        Self::draw_arcade_digit(
            surface,
            x_colon + 1,
            start_y + 1,
            bytes[2],
            shadow_color,
            scale,
        );
        Self::draw_arcade_digit(
            surface,
            x_m1 + 1,
            start_y + 1,
            bytes[3],
            shadow_color,
            scale,
        );
        Self::draw_arcade_digit(
            surface,
            x_m2 + 1,
            start_y + 1,
            bytes[4],
            shadow_color,
            scale,
        );

        Self::draw_arcade_digit(surface, x_h1, start_y, bytes[0], color, scale);
        Self::draw_arcade_digit(surface, x_h2, start_y, bytes[1], color, scale);
        Self::draw_arcade_digit(surface, x_colon, start_y, bytes[2], color, scale);
        Self::draw_arcade_digit(surface, x_m1, start_y, bytes[3], color, scale);
        Self::draw_arcade_digit(surface, x_m2, start_y, bytes[4], color, scale);
    }

    fn draw_scene(&self, surface: &mut dyn IDrawingSurface, w: i32, h: i32) {
        let is_portrait = h > w;
        let ground_top = if is_portrait {
            if h < 192 {
                h - 13
            } else {
                h - 26
            }
        } else {
            64
        };

        if is_portrait {
            Self::blit_backdrop(
                surface,
                &BG_STAGE_VERTICAL_PAL,
                &BG_STAGE_VERTICAL_PACKED,
                BG_VERTICAL_W,
                BG_VERTICAL_H,
            );
        } else {
            let (pal, packed) = BACKDROPS[self.current_backdrop_idx % BACKDROPS.len()];
            Self::blit_backdrop(surface, pal, packed, BG_BACKDROP_W, BG_BACKDROP_H);
        }

        // 2. Flying Helicopter in upper sky
        let heli_max = if is_portrait { w as f32 + 50.0 } else { 270.0 };
        if self.heli_x > -50.0 && self.heli_x < heli_max {
            let hy = if is_portrait {
                46 + ((self.phase_timer * 3.0).sin() * 3.0) as i32
            } else {
                8 + ((self.phase_timer * 3.0).sin() * 3.0) as i32
            };
            Self::blit_sprite(
                surface,
                &ENEMY_HELI_PAL,
                &ENEMY_HELI_PIXELS,
                ENEMY_HELI_W,
                ENEMY_HELI_H,
                self.heli_x as i32,
                hy,
                false,
            );
        }

        // 3. Rebel Army Di-Cokka Tank
        let tank_y = ground_top - TANK_ALIVE_H;
        if self.phase == Phase::Explosion || self.phase == Phase::HeroExit {
            Self::blit_sprite(
                surface,
                &TANK_DEAD_PAL,
                &TANK_DEAD_PIXELS,
                TANK_DEAD_W,
                TANK_DEAD_H,
                self.tank_x as i32,
                ground_top - TANK_DEAD_H,
                false,
            );
        } else {
            Self::blit_sprite(
                surface,
                &TANK_ALIVE_PAL,
                &TANK_ALIVE_PIXELS,
                TANK_ALIVE_W,
                TANK_ALIVE_H,
                self.tank_x as i32,
                tank_y,
                false,
            );
        }

        // 4. Tank Cannon Shell
        if self.tank_bullet_active && self.tank_bullet_x > self.marco_x {
            Self::blit_sprite(
                surface,
                &TANK_BULLET_PAL,
                &TANK_BULLET_PIXELS,
                TANK_BULLET_W,
                TANK_BULLET_H,
                self.tank_bullet_x as i32,
                ground_top - 22,
                false,
            );
        }

        // 5. Marco Rossi animation (appears only during minute sequence, hidden in Idle)
        if self.phase == Phase::HeroEnter || self.phase == Phase::HeroExit {
            if self.walk_frame % 2 == 0 {
                Self::blit_sprite(
                    surface,
                    &MARCO_WALK_0_PAL,
                    &MARCO_WALK_0_PIXELS,
                    MARCO_WALK_0_W,
                    MARCO_WALK_0_H,
                    self.marco_x as i32,
                    ground_top - MARCO_WALK_0_H,
                    false,
                );
            } else {
                Self::blit_sprite(
                    surface,
                    &MARCO_WALK_1_PAL,
                    &MARCO_WALK_1_PIXELS,
                    MARCO_WALK_1_W,
                    MARCO_WALK_1_H,
                    self.marco_x as i32,
                    ground_top - MARCO_WALK_1_H,
                    false,
                );
            }
        } else if self.phase == Phase::Firefight {
            let marco_y = ground_top - MARCO_SHOOT_0_H;
            if self.anim_frame % 2 == 0 {
                Self::blit_sprite(
                    surface,
                    &MARCO_SHOOT_0_PAL,
                    &MARCO_SHOOT_0_PIXELS,
                    MARCO_SHOOT_0_W,
                    MARCO_SHOOT_0_H,
                    self.marco_x as i32,
                    marco_y,
                    false,
                );
            } else {
                Self::blit_sprite(
                    surface,
                    &MARCO_SHOOT_1_PAL,
                    &MARCO_SHOOT_1_PIXELS,
                    MARCO_SHOOT_1_W,
                    MARCO_SHOOT_1_H,
                    self.marco_x as i32,
                    marco_y,
                    false,
                );
            }

            let muzzle_x = self.marco_x as i32 + MARCO_SHOOT_0_W;
            let muzzle_y = marco_y + 10;
            if self.anim_frame % 2 == 1 {
                let tracer_len = if is_portrait { 18 } else { 40 };
                surface.fill_rect(muzzle_x + 2, muzzle_y, tracer_len, 1, 255, 240, 60);
                surface.fill_rect(muzzle_x + 4, muzzle_y + 1, tracer_len - 6, 1, 255, 120, 20);
            }
        } else if self.phase == Phase::GrenadeAssault {
            Self::blit_sprite(
                surface,
                &MARCO_GRENADE_PAL,
                &MARCO_GRENADE_PIXELS,
                MARCO_GRENADE_W,
                MARCO_GRENADE_H,
                self.marco_x as i32,
                ground_top - MARCO_GRENADE_H,
                false,
            );
        } else if self.phase == Phase::Explosion {
            Self::blit_sprite(
                surface,
                &MARCO_VICTORY_PAL,
                &MARCO_VICTORY_PIXELS,
                MARCO_VICTORY_W,
                MARCO_VICTORY_H,
                self.marco_x as i32,
                ground_top - MARCO_VICTORY_H,
                false,
            );
        }

        // 6. Flying Grenade Arc
        if self.grenade_active {
            surface.fill_rect(
                self.grenade_x as i32,
                self.grenade_y as i32,
                3,
                3,
                180,
                160,
                40,
            );
            surface.draw_pixel(
                self.grenade_x as i32 + 1,
                self.grenade_y as i32 + 1,
                255,
                220,
                80,
            );
        }

        // 7. Fiery Explosive Fireballs
        if self.phase == Phase::Explosion {
            let ef = (self.explosion_frame as usize) % 4;
            let (pal, pix, ew, eh) = EXPLOSION_SPRITES[ef];
            let exp_x = if is_portrait {
                self.tank_x as i32 + 4
            } else {
                self.tank_x as i32 + 15
            };
            Self::blit_sprite(surface, pal, pix, ew, eh, exp_x, ground_top - eh - 4, false);
        }

        // 8. Authentic SNK Neo Geo Arcade HUD
        let hud_yellow = (248, 208, 0);
        let hud_white = (255, 255, 255);
        let hud_red = (230, 40, 30);
        let hud_cyan = (40, 220, 240);

        let time_str = format!("{}:{}", self.shown_hh, self.shown_mm);

        if is_portrait {
            surface.fill_rect(0, 0, w, 10, 8, 8, 16);
            surface.fill_rect(0, 10, w, 1, 40, 40, 70);

            if w >= 64 {
                Self::draw_arcade_digit(surface, 4, 2, b'1', hud_red, 1);
                surface.fill_rect(9, 2, 1, 5, hud_red.0, hud_red.1, hud_red.2);
                surface.fill_rect(11, 2, 1, 5, hud_red.0, hud_red.1, hud_red.2);
                surface.fill_rect(9, 6, 3, 1, hud_red.0, hud_red.1, hud_red.2);
                surface.fill_rect(14, 2, 1, 5, hud_red.0, hud_red.1, hud_red.2);
                surface.fill_rect(14, 2, 3, 1, hud_red.0, hud_red.1, hud_red.2);
                surface.fill_rect(16, 2, 1, 3, hud_red.0, hud_red.1, hud_red.2);
                surface.fill_rect(14, 4, 3, 1, hud_red.0, hud_red.1, hud_red.2);

                let score_str = format!("00{}{}", self.shown_hh, self.shown_mm);
                let mut sx = 20;
                for b in score_str.bytes() {
                    if sx >= w - 4 {
                        break;
                    }
                    Self::draw_arcade_digit(surface, sx, 2, b, hud_white, 1);
                    sx += 4;
                }

                Self::draw_arcade_time(surface, 10, 14, &time_str, hud_yellow, 2);
            } else {
                let time_x = (w - 14) / 2;
                let hh_bytes = self.shown_hh.as_bytes();
                let mm_bytes = self.shown_mm.as_bytes();
                if hh_bytes.len() >= 2 && mm_bytes.len() >= 2 {
                    let shadow_color = (20, 10, 10);
                    Self::draw_arcade_digit(surface, time_x + 1, 15, hh_bytes[0], shadow_color, 2);
                    Self::draw_arcade_digit(surface, time_x + 9, 15, hh_bytes[1], shadow_color, 2);
                    Self::draw_arcade_digit(surface, time_x + 1, 29, mm_bytes[0], shadow_color, 2);
                    Self::draw_arcade_digit(surface, time_x + 9, 29, mm_bytes[1], shadow_color, 2);
                    Self::draw_arcade_digit(surface, time_x, 14, hh_bytes[0], hud_yellow, 2);
                    Self::draw_arcade_digit(surface, time_x + 8, 14, hh_bytes[1], hud_yellow, 2);
                    Self::draw_arcade_digit(surface, time_x, 28, mm_bytes[0], hud_yellow, 2);
                    Self::draw_arcade_digit(surface, time_x + 8, 28, mm_bytes[1], hud_yellow, 2);
                }
            }
        } else {
            surface.fill_rect(0, 0, w, 12, 8, 8, 16);
            surface.fill_rect(0, 12, w, 1, 40, 40, 70);

            Self::draw_arcade_digit(surface, 6, 3, b'1', hud_red, 1);
            surface.fill_rect(11, 3, 1, 5, hud_red.0, hud_red.1, hud_red.2);
            surface.fill_rect(13, 3, 1, 5, hud_red.0, hud_red.1, hud_red.2);
            surface.fill_rect(11, 7, 3, 1, hud_red.0, hud_red.1, hud_red.2);
            surface.fill_rect(16, 3, 1, 5, hud_red.0, hud_red.1, hud_red.2);
            surface.fill_rect(16, 3, 3, 1, hud_red.0, hud_red.1, hud_red.2);
            surface.fill_rect(18, 3, 1, 3, hud_red.0, hud_red.1, hud_red.2);
            surface.fill_rect(16, 5, 3, 1, hud_red.0, hud_red.1, hud_red.2);

            let score_str = format!("00{}{}", self.shown_hh, self.shown_mm);
            let mut sx = 22;
            for b in score_str.bytes() {
                Self::draw_arcade_digit(surface, sx, 3, b, hud_white, 1);
                sx += 4;
            }

            Self::draw_arcade_time(surface, 106, 1, &time_str, hud_yellow, 2);

            Self::draw_arcade_digit(surface, 185, 3, b'4', hud_cyan, 1);
            surface.fill_rect(195, 2, 7, 7, 220, 160, 20);
            surface.draw_rect(195, 2, 7, 7, 255, 240, 80);
            surface.fill_rect(197, 4, 1, 3, 0, 0, 0);
            surface.fill_rect(199, 4, 1, 3, 0, 0, 0);
            surface.fill_rect(197, 5, 3, 1, 0, 0, 0);

            let mut ax = 205;
            for b in self.shown_mm.bytes() {
                Self::draw_arcade_digit(surface, ax, 3, b, hud_yellow, 1);
                ax += 4;
            }

            surface.fill_circle(235, 5, 2, 240, 80, 20);
            surface.draw_pixel(236, 3, 255, 255, 0);
            Self::draw_arcade_digit(surface, 241, 3, b'1', hud_white, 1);
            Self::draw_arcade_digit(surface, 246, 3, b'0', hud_white, 1);
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
        let mut dt = now.duration_since(self.last_tick).as_secs_f32();
        if dt <= 0.0 || dt > 0.5 {
            dt = 0.033;
        }
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
        let is_portrait = h > w;
        let ground_top = if is_portrait {
            if h < 192 {
                h - 13
            } else {
                h - 26
            }
        } else {
            64
        };
        let target_tank_x = if is_portrait {
            if w < 64 {
                (w - 22) as f32
            } else {
                w as f32 - 34.0
            }
        } else {
            185.0
        };
        let target_fire_x = if is_portrait {
            if w < 64 {
                0.0
            } else {
                2.0
            }
        } else {
            42.0
        };

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
            self.anim_timer = 0.0;
            self.anim_frame = 0;
            self.walk_timer = 0.0;
            self.walk_frame = 0;
            self.tank_x = target_tank_x;
            self.marco_x = -50.0;
            self.heli_x = -60.0;
            self.tank_bullet_active = false;
            self.grenade_active = false;
        }

        if self.last_second != seconds as i32 {
            self.last_second = seconds as i32;
        }

        self.phase_timer += dt;
        self.anim_timer += dt;

        if self.anim_timer >= 0.12 {
            self.anim_timer -= 0.12;
            self.anim_frame += 1;
        }

        if self.phase == Phase::HeroEnter || self.phase == Phase::HeroExit {
            self.walk_timer += dt;
            if self.walk_timer >= 0.10 {
                self.walk_timer -= 0.10;
                self.walk_frame += 1;
            }
        }

        let current_min = minutes as i32;

        let heli_max = if is_portrait { w as f32 + 50.0 } else { 280.0 };
        self.heli_x += 22.0 * dt;
        if self.heli_x > heli_max {
            self.heli_x = -60.0;
        }

        let minute_changed = self.last_minute != -1 && current_min != self.last_minute;

        match self.phase {
            Phase::Idle => {
                self.tank_bullet_active = false;
                self.grenade_active = false;
                self.tank_x = target_tank_x;
                self.marco_x = -50.0;
                if minute_changed {
                    self.phase = Phase::HeroEnter;
                    self.phase_timer = 0.0;
                    self.walk_timer = 0.0;
                    self.walk_frame = 0;
                    self.marco_x = -40.0;
                }
            }
            Phase::HeroEnter => {
                self.marco_x += 85.0 * dt;
                if self.marco_x >= target_fire_x {
                    self.marco_x = target_fire_x;
                    self.phase = Phase::Firefight;
                    self.phase_timer = 0.0;
                    self.anim_timer = 0.0;
                    self.anim_frame = 0;
                }
            }
            Phase::Firefight => {
                if self.phase_timer >= 0.65 {
                    self.phase = Phase::GrenadeAssault;
                    self.phase_timer = 0.0;
                    self.grenade_active = true;
                    self.grenade_x = self.marco_x + 18.0;
                    self.grenade_y = ground_top as f32 - 28.0;
                    self.grenade_vx =
                        (self.tank_x + 10.0 - self.grenade_x) / if is_portrait { 0.5 } else { 0.7 };
                    self.grenade_vy = if is_portrait { -70.0 } else { -95.0 };
                }
            }
            Phase::GrenadeAssault => {
                if self.grenade_active {
                    self.grenade_x += self.grenade_vx * dt;
                    self.grenade_vy += 220.0 * dt;
                    self.grenade_y += self.grenade_vy * dt;

                    if self.grenade_x >= self.tank_x + 8.0
                        || self.grenade_y >= ground_top as f32 - 16.0
                        || self.phase_timer >= 0.9
                    {
                        self.grenade_active = false;
                        self.phase = Phase::Explosion;
                        self.phase_timer = 0.0;
                        self.explosion_frame = 0;
                        self.explosion_timer = 0.0;
                        self.shown_hh = hh;
                        self.shown_mm = mm;
                        self.last_minute = current_min;
                    }
                }
            }
            Phase::Explosion => {
                self.explosion_timer += dt;
                if self.explosion_timer >= 0.12 {
                    self.explosion_timer -= 0.12;
                    if self.explosion_frame < 3 {
                        self.explosion_frame += 1;
                    }
                }
                if self.phase_timer >= 0.70 {
                    self.phase = Phase::HeroExit;
                    self.phase_timer = 0.0;
                    self.walk_timer = 0.0;
                    self.walk_frame = 0;
                }
            }
            Phase::HeroExit => {
                self.marco_x += 95.0 * dt;
                if self.marco_x > w as f32 + 26.0 {
                    self.phase = Phase::Idle;
                    self.phase_timer = 0.0;
                    self.marco_x = -50.0;
                    self.tank_x = target_tank_x;
                }
            }
        }

        self.draw_scene(surface, w, h);
    }
}

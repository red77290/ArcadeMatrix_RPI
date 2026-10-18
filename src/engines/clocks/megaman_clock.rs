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

// 4x6 Compact Pixel Font (0-9) for 32px wide portrait panels
const COMPACT_FONT_4X6: [[u8; 6]; 10] = [
    [0x6, 0x9, 0x9, 0x9, 0x9, 0x6], // 0
    [0x2, 0x6, 0x2, 0x2, 0x2, 0x7], // 1
    [0x6, 0x9, 0x2, 0x4, 0x8, 0xF], // 2
    [0xE, 0x1, 0x6, 0x1, 0x1, 0xE], // 3
    [0x9, 0x9, 0xF, 0x1, 0x1, 0x1], // 4
    [0xF, 0x8, 0xE, 0x1, 0x1, 0xE], // 5
    [0x6, 0x8, 0xE, 0x9, 0x9, 0x6], // 6
    [0xF, 0x1, 0x2, 0x4, 0x4, 0x4], // 7
    [0x6, 0x9, 0x6, 0x9, 0x9, 0x6], // 8
    [0x6, 0x9, 0x7, 0x1, 0x1, 0x6], // 9
];

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

#[derive(PartialEq, Clone, Copy)]
enum AnimPhase {
    Idle,
    RunLeft,
    JumpLeft,
    RunRight,
    JumpRight,
    WalkBack,
}

pub struct MegamanClock {
    phase: AnimPhase,
    displayed_hour: String,
    displayed_min: String,
    pending_hour: String,
    pending_min: String,
    hour_changing: bool,

    last_tick: Instant,
    phase_timer: f32,
    blink_timer: f32,
    run_timer: f32,
    run_frame: u8,

    mm_x: f32,
    mm_y: f32,
    mm_vy: f32,
    ground_y: f32,
    facing_left: bool,

    target_hour_x: f32,
    target_min_x: f32,
    pod_base_y: f32,

    hour_pod_offset_y: f32,
    min_pod_offset_y: f32,
    hour_pod_flashing: bool,
    min_pod_flashing: bool,

    spark_active: bool,
    spark_x: f32,
    spark_y: f32,
    spark_timer: f32,
}

impl Default for MegamanClock {
    fn default() -> Self {
        Self::new()
    }
}

impl MegamanClock {
    pub fn new() -> Self {
        Self {
            phase: AnimPhase::Idle,
            displayed_hour: String::new(),
            displayed_min: String::new(),
            pending_hour: String::new(),
            pending_min: String::new(),
            hour_changing: false,
            last_tick: Instant::now(),
            phase_timer: 0.0,
            blink_timer: 0.0,
            run_timer: 0.0,
            run_frame: 0,
            mm_x: 16.0,
            mm_y: 0.0,
            mm_vy: 0.0,
            ground_y: 0.0,
            facing_left: false,
            target_hour_x: 0.0,
            target_min_x: 0.0,
            pod_base_y: 0.0,
            hour_pod_offset_y: 0.0,
            min_pod_offset_y: 0.0,
            hour_pod_flashing: false,
            min_pod_flashing: false,
            spark_active: false,
            spark_x: 0.0,
            spark_y: 0.0,
            spark_timer: 0.0,
        }
    }

    pub fn on_activated(&mut self) {
        self.displayed_hour.clear();
        self.displayed_min.clear();
        self.phase = AnimPhase::Idle;
    }

    pub fn on_display_geometry_changed(&mut self) {
        self.displayed_hour.clear();
        self.displayed_min.clear();
        self.phase = AnimPhase::Idle;
        self.spark_active = false;
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_sprite(
        surface: &mut dyn IDrawingSurface,
        sprite: &[u16],
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        flip_x: bool,
        downscale: bool,
    ) {
        let sw = surface.width() as i32;
        let sh = surface.height() as i32;
        let step = if downscale { 2 } else { 1 };
        let mut sy = 0;
        let mut dy = 0;
        while sy < h {
            let py = y + dy;
            if py >= 0 && py < sh {
                let mut sx = 0;
                let mut dx = 0;
                while sx < w {
                    let px = x + dx;
                    if px >= 0 && px < sw {
                        let idx_x = if flip_x { w - 1 - sx } else { sx };
                        let col = sprite[(sy * w + idx_x) as usize];
                        if col != MASK {
                            let (r, g, b) = rgb565_to_rgb888(col);
                            surface.draw_pixel(px, py, r, g, b);
                        }
                    }
                    sx += step;
                    dx += 1;
                }
            }
            sy += step;
            dy += 1;
        }
    }

    fn draw_skyline(surface: &mut dyn IDrawingSurface, ground_y: i32) {
        let w = surface.width() as i32;
        let h = surface.height() as i32;

        let (sr, sg, sb) = rgb565_to_rgb888(0x0005);
        surface.fill_rect(0, 0, w, ground_y, sr, sg, sb);

        if h >= 48 {
            let star_coords: [(i32, i32); 11] = [
                (14, 5),
                (38, 11),
                (62, 4),
                (89, 8),
                (112, 6),
                (138, 12),
                (164, 5),
                (188, 9),
                (210, 4),
                (234, 10),
                (248, 6),
            ];
            for (i, &(sx, sy)) in star_coords.iter().enumerate() {
                if sx < w && sy < ground_y {
                    let c = if i % 2 == 0 { 0xFFFF } else { 0x7BEF };
                    let (r, g, b) = rgb565_to_rgb888(c);
                    surface.draw_pixel(sx, sy, r, g, b);
                }
            }
        }

        for &(bx, bh_raw, bw) in &SKYLINE_BUILDINGS {
            if bx >= w {
                break;
            }
            let bh = if h < 48 { bh_raw / 2 } else { bh_raw };
            let by = ground_y - bh;
            let (br, bg, bb) = rgb565_to_rgb888(0x0864);
            let (or, og, ob) = rgb565_to_rgb888(0x10E6);
            surface.fill_rect(bx, by, bw, bh, br, bg, bb);
            surface.draw_rect(bx, by, bw, bh, or, og, ob);

            let (wr, wg, wb) = rgb565_to_rgb888(0xFFE0);
            let mut wy = by + 3;
            while wy < ground_y - 3 {
                let mut wx = bx + 3;
                while wx < bx + bw - 3 {
                    if ((wx + wy) % 7) != 0 {
                        surface.fill_rect(wx, wy, 2, 2, wr, wg, wb);
                    }
                    wx += 5;
                }
                wy += 5;
            }
        }
    }

    fn draw_ground(surface: &mut dyn IDrawingSurface, y: i32) {
        let w = surface.width() as i32;
        let mut x = 0;
        while x < w {
            Self::draw_sprite(
                surface,
                &GROUND_TILE,
                x,
                y,
                GROUND_TILE_W,
                GROUND_TILE_H,
                false,
                false,
            );
            x += GROUND_TILE_W;
        }
    }

    fn draw_tech_platform(
        surface: &mut dyn IDrawingSurface,
        start_x: i32,
        start_y: i32,
        tile_count: i32,
    ) {
        let h = surface.height() as i32;
        // On ESP32 GROUND_TILE_W is 16, whereas in cwassets/megaman.rs GROUND_TILE_W is 8.
        // Multiply tile_count by 2 so 3 tiles = 48px and 2 tiles = 32px, matching ESP32 pixel widths.
        let tiles_8px = tile_count * 2;
        let pw = tiles_8px * GROUND_TILE_W;

        let girder_h = h - (start_y + GROUND_TILE_H);
        if girder_h > 0 {
            let gx1 = start_x + 4;
            let gx2 = start_x + pw - 8;
            let (g1r, g1g, g1b) = rgb565_to_rgb888(0x18E5);
            let (g2r, g2g, g2b) = rgb565_to_rgb888(0x31EA);
            let (g3r, g3g, g3b) = rgb565_to_rgb888(0x10C4);
            surface.fill_rect(gx1, start_y + GROUND_TILE_H, 4, girder_h, g1r, g1g, g1b);
            surface.draw_line(
                gx1 + 1,
                start_y + GROUND_TILE_H,
                gx1 + 1,
                h - 1,
                g2r,
                g2g,
                g2b,
            );
            if pw >= 28 {
                surface.fill_rect(gx2, start_y + GROUND_TILE_H, 4, girder_h, g1r, g1g, g1b);
                surface.draw_line(
                    gx2 + 1,
                    start_y + GROUND_TILE_H,
                    gx2 + 1,
                    h - 1,
                    g2r,
                    g2g,
                    g2b,
                );
            }
            let mut ry = start_y + GROUND_TILE_H + 3;
            while ry < h {
                surface.draw_line(gx1, ry, gx1 + 3, ry, g3r, g3g, g3b);
                if pw >= 28 {
                    surface.draw_line(gx2, ry, gx2 + 3, ry, g3r, g3g, g3b);
                }
                ry += 5;
            }
        }

        for i in 0..tiles_8px {
            Self::draw_sprite(
                surface,
                &GROUND_TILE,
                start_x + i * GROUND_TILE_W,
                start_y,
                GROUND_TILE_W,
                GROUND_TILE_H,
                false,
                false,
            );
        }

        let (br, bg, bb) = rgb565_to_rgb888(0x2988);
        let (cr, cg, cb) = rgb565_to_rgb888(0x075B);
        surface.draw_line(
            start_x,
            start_y,
            start_x,
            start_y + GROUND_TILE_H - 1,
            br,
            bg,
            bb,
        );
        surface.draw_line(
            start_x + pw - 1,
            start_y,
            start_x + pw - 1,
            start_y + GROUND_TILE_H - 1,
            br,
            bg,
            bb,
        );
        surface.draw_pixel(start_x + 1, start_y + 2, cr, cg, cb);
        surface.draw_pixel(start_x + pw - 2, start_y + 2, cr, cg, cb);
    }

    fn draw_energy_gauge(surface: &mut dyn IDrawingSurface, x: i32, y: i32, seconds: i32) {
        let bar_h = 42;
        let bar_w = 5;
        surface.fill_rect(x, y, bar_w, bar_h, 0, 0, 0);
        let (rr, rg, rb) = rgb565_to_rgb888(0x7BEF);
        surface.draw_rect(x, y, bar_w, bar_h, rr, rg, rb);

        let total_pips = 20;
        let lit_pips = ((seconds.clamp(0, 59) * total_pips) / 59).clamp(1, total_pips);

        let (p1r, p1g, p1b) = rgb565_to_rgb888(0xFCE0);
        let (p2r, p2g, p2b) = rgb565_to_rgb888(0xFFFF);
        let (p0r, p0g, p0b) = rgb565_to_rgb888(0x2104);

        for i in 0..total_pips {
            let py = y + bar_h - 3 - i * 2;
            if i < lit_pips {
                surface.draw_line(x + 1, py, x + bar_w - 2, py, p1r, p1g, p1b);
                surface.draw_pixel(x + 2, py, p2r, p2g, p2b);
            } else {
                surface.draw_line(x + 1, py, x + bar_w - 2, py, p0r, p0g, p0b);
            }
        }
    }

    fn draw_mini_energy_gauge(surface: &mut dyn IDrawingSurface, x: i32, y: i32, seconds: i32) {
        let bar_w = 30;
        let bar_h = 4;
        surface.fill_rect(x, y, bar_w, bar_h, 0, 0, 0);
        let (rr, rg, rb) = rgb565_to_rgb888(0x7BEF);
        surface.draw_rect(x, y, bar_w, bar_h, rr, rg, rb);

        let total_pips = 14;
        let lit_pips = ((seconds.clamp(0, 59) * total_pips) / 59).clamp(1, total_pips);
        let (p1r, p1g, p1b) = rgb565_to_rgb888(0xFCE0);
        let (p0r, p0g, p0b) = rgb565_to_rgb888(0x2104);

        for i in 0..total_pips {
            let px = x + 1 + i * 2;
            if i < lit_pips {
                surface.draw_line(px, y + 1, px, y + bar_h - 2, p1r, p1g, p1b);
            } else {
                surface.draw_line(px, y + 1, px, y + bar_h - 2, p0r, p0g, p0b);
            }
        }
    }

    fn draw_capcom_pod(
        surface: &mut dyn IDrawingSurface,
        x: i32,
        y: i32,
        text: &str,
        flashing: bool,
        downscale: bool,
    ) {
        let pod_w = if downscale { 14 } else { POD_W };
        let pod_h = if downscale { 14 } else { POD_H };

        let border_out = if flashing { 0xFFFF } else { 0x039D };
        let border_in = if flashing { 0xFFE0 } else { 0x075B };
        let pod_bg = if flashing { 0x039D } else { 0x0843 };

        let (bgr, bgg, bgb) = rgb565_to_rgb888(pod_bg);
        let (bor, bog, bob) = rgb565_to_rgb888(border_out);
        let (bir, big, bib) = rgb565_to_rgb888(border_in);

        surface.fill_rect(x + 1, y + 1, pod_w - 2, pod_h - 2, bgr, bgg, bgb);
        surface.draw_rect(x, y, pod_w, pod_h, bor, bog, bob);
        surface.draw_rect(x + 1, y + 1, pod_w - 2, pod_h - 2, bir, big, bib);

        surface.draw_pixel(x, y, 0, 0, 0);
        surface.draw_pixel(x + pod_w - 1, y, 0, 0, 0);
        surface.draw_pixel(x, y + pod_h - 1, 0, 0, 0);
        surface.draw_pixel(x + pod_w - 1, y + pod_h - 1, 0, 0, 0);
        if !downscale {
            let (rvr, rvg, rvb) = rgb565_to_rgb888(0xAD55);
            surface.draw_pixel(x + 2, y + 2, rvr, rvg, rvb);
            surface.draw_pixel(x + pod_w - 3, y + 2, rvr, rvg, rvb);
            surface.draw_pixel(x + 2, y + pod_h - 3, rvr, rvg, rvb);
            surface.draw_pixel(x + pod_w - 3, y + pod_h - 3, rvr, rvg, rvb);
        }

        let bytes = text.as_bytes();
        let d0 = if !bytes.is_empty() { bytes[0] } else { b'0' };
        let d1 = if bytes.len() >= 2 { bytes[1] } else { b'0' };

        if downscale {
            Self::draw_compact_digit(surface, x + 2, y + 4, d0, flashing);
            Self::draw_compact_digit(surface, x + 7, y + 4, d1, flashing);
        } else {
            let scale = 1;
            let start_x = x + (pod_w - 11) / 2;
            let start_y = y + (pod_h - 7) / 2;
            Self::draw_capcom_digit(surface, start_x, start_y, d0, scale, flashing);
            Self::draw_capcom_digit(surface, start_x + 6, start_y, d1, scale, flashing);
        }
    }

    fn draw_compact_digit(
        surface: &mut dyn IDrawingSurface,
        x: i32,
        y: i32,
        c: u8,
        flashing: bool,
    ) {
        if !c.is_ascii_digit() {
            return;
        }
        let idx = (c - b'0') as usize;
        let fg = if flashing { 0xFFFF } else { 0xFCE0 };
        let (r, g, b) = rgb565_to_rgb888(fg);
        for row in 0..6i32 {
            let bits = COMPACT_FONT_4X6[idx][row as usize];
            for col in 0..4i32 {
                if (bits & (0x08 >> col)) != 0 {
                    surface.draw_pixel(x + col, y + row, r, g, b);
                }
            }
        }
    }

    fn draw_capcom_digit(
        surface: &mut dyn IDrawingSurface,
        x: i32,
        y: i32,
        c: u8,
        scale: i32,
        flashing: bool,
    ) {
        let idx = if c.is_ascii_digit() {
            (c - b'0') as usize
        } else if c == b':' {
            10
        } else {
            return;
        };

        let fg = if flashing { 0xFFFF } else { 0xFCE0 };
        let hi = 0xFFFF;
        let shadow = 0x0010;

        let (sr, sg, sb) = rgb565_to_rgb888(shadow);
        for row in 0..7i32 {
            let bits = CAPCOM_FONT[idx][row as usize];
            for col in 0..5i32 {
                if (bits & (0x10 >> col)) != 0 {
                    surface.fill_rect(
                        x + col * scale + 1,
                        y + row * scale + 1,
                        scale,
                        scale,
                        sr,
                        sg,
                        sb,
                    );
                }
            }
        }

        for row in 0..7i32 {
            let bits = CAPCOM_FONT[idx][row as usize];
            let draw_col = if row < 2 { hi } else { fg };
            let (r, g, b) = rgb565_to_rgb888(draw_col);
            for col in 0..5i32 {
                if (bits & (0x10 >> col)) != 0 {
                    surface.fill_rect(x + col * scale, y + row * scale, scale, scale, r, g, b);
                }
            }
        }
    }

    fn update_animation(
        &mut self,
        dt: f32,
        target_hour: &str,
        target_min: &str,
        rest_x: f32,
        downscale: bool,
    ) {
        self.blink_timer += dt;
        if self.blink_timer >= 3.6 {
            self.blink_timer = 0.0;
        }

        if self.spark_active {
            self.spark_timer += dt;
            if self.spark_timer >= 0.22 {
                self.spark_active = false;
            }
        }

        if self.hour_pod_offset_y < 0.0 {
            self.hour_pod_offset_y += 28.0 * dt;
            if self.hour_pod_offset_y >= 0.0 {
                self.hour_pod_offset_y = 0.0;
                self.hour_pod_flashing = false;
            }
        }
        if self.min_pod_offset_y < 0.0 {
            self.min_pod_offset_y += 28.0 * dt;
            if self.min_pod_offset_y >= 0.0 {
                self.min_pod_offset_y = 0.0;
                self.min_pod_flashing = false;
            }
        }

        if self.displayed_min.is_empty() {
            self.displayed_hour = target_hour.to_string();
            self.displayed_min = target_min.to_string();
            self.mm_x = rest_x;
            return;
        }

        if self.phase == AnimPhase::Idle
            && (self.displayed_min != target_min || self.displayed_hour != target_hour)
        {
            self.pending_hour = target_hour.to_string();
            self.pending_min = target_min.to_string();
            self.hour_changing = self.displayed_hour != target_hour;
            self.phase_timer = 0.0;
            self.run_timer = 0.0;

            if self.hour_changing {
                self.phase = AnimPhase::RunLeft;
            } else {
                self.phase = AnimPhase::RunRight;
            }
        }

        let pod_w = if downscale { 14 } else { POD_W };
        let pod_h = if downscale { 14 } else { POD_H };
        let mm_w = if downscale {
            MEGAMAN_IDLE_W / 2
        } else {
            MEGAMAN_IDLE_W
        };
        let run_speed = if downscale { 55.0 } else { 95.0 };
        let gravity = if downscale { 340.0 } else { 520.0 };

        match self.phase {
            AnimPhase::Idle => {
                self.mm_x = rest_x;
                self.mm_y = self.ground_y;
                self.facing_left = false;
            }
            AnimPhase::RunLeft => {
                self.run_timer += dt;
                if self.run_timer >= 0.08 {
                    self.run_timer = 0.0;
                    self.run_frame = (self.run_frame + 1) % 4;
                }
                let goal_x = self.target_hour_x + (pod_w - mm_w) as f32 * 0.5;
                if (self.mm_x - goal_x).abs() <= run_speed * dt + 1.5 {
                    self.mm_x = goal_x;
                    self.phase = AnimPhase::JumpLeft;
                    self.mm_y = self.ground_y;
                    let dist_y = (self.ground_y - (self.pod_base_y + pod_h as f32)).max(6.0);
                    self.mm_vy = -(2.0 * gravity * (dist_y + 4.0)).sqrt();
                } else if self.mm_x < goal_x {
                    self.mm_x += run_speed * dt;
                    self.facing_left = false;
                } else {
                    self.mm_x -= run_speed * dt;
                    self.facing_left = true;
                }
            }
            AnimPhase::JumpLeft => {
                self.mm_y += self.mm_vy * dt;
                self.mm_vy += gravity * dt;

                if self.mm_vy < 0.0
                    && self.mm_y <= self.pod_base_y + pod_h as f32
                    && self.displayed_hour != self.pending_hour
                {
                    self.displayed_hour = self.pending_hour.clone();
                    self.hour_pod_offset_y = -4.0;
                    self.hour_pod_flashing = true;
                    self.mm_vy = 45.0;

                    self.spark_active = true;
                    self.spark_x =
                        self.target_hour_x + (pod_w / 2 - if downscale { 3 } else { 6 }) as f32;
                    self.spark_y = self.pod_base_y + (pod_h - if downscale { 3 } else { 6 }) as f32;
                    self.spark_timer = 0.0;
                }

                if self.mm_y >= self.ground_y {
                    self.mm_y = self.ground_y;
                    self.mm_vy = 0.0;
                    self.phase = AnimPhase::RunRight;
                }
            }
            AnimPhase::RunRight => {
                self.run_timer += dt;
                if self.run_timer >= 0.08 {
                    self.run_timer = 0.0;
                    self.run_frame = (self.run_frame + 1) % 4;
                }
                let goal_x = self.target_min_x + (pod_w - mm_w) as f32 * 0.5;
                if (self.mm_x - goal_x).abs() <= run_speed * dt + 1.5 {
                    self.mm_x = goal_x;
                    self.phase = AnimPhase::JumpRight;
                    self.mm_y = self.ground_y;
                    let dist_y = (self.ground_y - (self.pod_base_y + pod_h as f32)).max(6.0);
                    self.mm_vy = -(2.0 * gravity * (dist_y + 4.0)).sqrt();
                } else if self.mm_x < goal_x {
                    self.mm_x += run_speed * dt;
                    self.facing_left = false;
                } else {
                    self.mm_x -= run_speed * dt;
                    self.facing_left = true;
                }
            }
            AnimPhase::JumpRight => {
                self.mm_y += self.mm_vy * dt;
                self.mm_vy += gravity * dt;

                if self.mm_vy < 0.0
                    && self.mm_y <= self.pod_base_y + pod_h as f32
                    && self.displayed_min != self.pending_min
                {
                    self.displayed_min = self.pending_min.clone();
                    self.min_pod_offset_y = -4.0;
                    self.min_pod_flashing = true;
                    self.mm_vy = 45.0;

                    self.spark_active = true;
                    self.spark_x =
                        self.target_min_x + (pod_w / 2 - if downscale { 3 } else { 6 }) as f32;
                    self.spark_y = self.pod_base_y + (pod_h - if downscale { 3 } else { 6 }) as f32;
                    self.spark_timer = 0.0;
                }

                if self.mm_y >= self.ground_y {
                    self.mm_y = self.ground_y;
                    self.mm_vy = 0.0;
                    self.phase = AnimPhase::WalkBack;
                }
            }
            AnimPhase::WalkBack => {
                self.run_timer += dt;
                if self.run_timer >= 0.08 {
                    self.run_timer = 0.0;
                    self.run_frame = (self.run_frame + 1) % 4;
                }
                if (self.mm_x - rest_x).abs() <= run_speed * dt + 1.5 {
                    self.mm_x = rest_x;
                    self.facing_left = false;
                    self.phase = AnimPhase::Idle;
                } else if self.mm_x < rest_x {
                    self.mm_x += run_speed * dt;
                    self.facing_left = false;
                } else {
                    self.mm_x -= run_speed * dt;
                    self.facing_left = true;
                }
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
            .clamp(0.001, 0.1);
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
        let hour_buf = format!("{:02}", hours);
        let min_buf = format!("{:02}", minutes);
        let seconds = seconds as i32;

        let w = surface.width() as i32;
        let h = surface.height() as i32;

        let is_portrait = h > w;
        let is_wide_tall = !is_portrait && h >= 48;

        let mut downscale = false;
        let ground_top: i32;
        let pod_y: i32;
        let hour_x: i32;
        let min_x: i32;
        let rest_x: f32;

        if is_portrait {
            if w >= 64 {
                ground_top = h - GROUND_TILE_H - 12;
                pod_y = 10;
                hour_x = 12;
                min_x = 38;
                rest_x = 22.0;
                Self::draw_skyline(surface, ground_top);
                Self::draw_ground(surface, ground_top);
                let (fr, fg, fb) = rgb565_to_rgb888(0x0842);
                surface.fill_rect(0, ground_top + GROUND_TILE_H, w, 12, fr, fg, fb);
                Self::draw_energy_gauge(surface, 3, 8, seconds);
            } else {
                downscale = true;
                ground_top = h - GROUND_TILE_H - 8;
                pod_y = 18;
                hour_x = 1;
                min_x = 17;
                rest_x = 10.0;
                Self::draw_skyline(surface, ground_top);
                Self::draw_ground(surface, ground_top);
                let (fr, fg, fb) = rgb565_to_rgb888(0x0842);
                surface.fill_rect(0, ground_top + GROUND_TILE_H, w, 8, fr, fg, fb);
                Self::draw_mini_energy_gauge(surface, 1, h - 5, seconds);
            }
        } else if is_wide_tall {
            ground_top = h - GROUND_TILE_H;
            let mid_x = w / 2;
            pod_y = 10;
            hour_x = mid_x - 28;
            min_x = mid_x + 4;
            rest_x = if w >= 128 { 18.0 } else { 8.0 };

            Self::draw_skyline(surface, ground_top);
            if w >= 128 {
                Self::draw_tech_platform(surface, 4, ground_top, 3);
                Self::draw_tech_platform(surface, mid_x - 32, ground_top, 4);
                Self::draw_tech_platform(surface, w - 52, ground_top, 3);
            } else {
                Self::draw_ground(surface, ground_top);
            }

            Self::draw_energy_gauge(surface, 3, 6, seconds);

            if w >= 128 {
                let metool = if (seconds % 6) == 0 {
                    &METOOL_PEEK[..]
                } else {
                    &METOOL_SLEEP[..]
                };
                Self::draw_sprite(
                    surface,
                    metool,
                    w - METOOL_W - 4,
                    ground_top - METOOL_H,
                    METOOL_W,
                    METOOL_H,
                    false,
                    false,
                );
            }
        } else {
            ground_top = h - 4;
            pod_y = 6;
            hour_x = w - 54;
            min_x = w - 27;
            rest_x = 14.0;

            Self::draw_skyline(surface, ground_top);
            Self::draw_tech_platform(surface, 2, ground_top, 2);
            Self::draw_tech_platform(surface, hour_x - 4, ground_top, 4);
            Self::draw_mini_energy_gauge(surface, 6, 2, seconds);
        }

        let mm_h = if downscale {
            MEGAMAN_IDLE_H / 2
        } else {
            MEGAMAN_IDLE_H
        };
        self.ground_y = (ground_top - mm_h) as f32;
        if self.phase == AnimPhase::Idle {
            self.mm_y = self.ground_y;
        }
        self.target_hour_x = hour_x as f32;
        self.target_min_x = min_x as f32;
        self.pod_base_y = pod_y as f32;

        self.update_animation(dt, &hour_buf, &min_buf, rest_x, downscale);

        Self::draw_capcom_pod(
            surface,
            hour_x,
            pod_y + self.hour_pod_offset_y as i32,
            &self.displayed_hour,
            self.hour_pod_flashing,
            downscale,
        );
        Self::draw_capcom_pod(
            surface,
            min_x,
            pod_y + self.min_pod_offset_y as i32,
            &self.displayed_min,
            self.min_pod_flashing,
            downscale,
        );

        if !downscale && (seconds % 2 == 0) {
            let colon_x = (hour_x + POD_W + min_x) / 2 - 1;
            let colon_y = pod_y + 6;
            let (cr, cg, cb) = rgb565_to_rgb888(0x075B);
            surface.fill_rect(colon_x, colon_y, 2, 2, cr, cg, cb);
            surface.fill_rect(colon_x, colon_y + 6, 2, 2, cr, cg, cb);
        }

        let mx = self.mm_x as i32;
        let my = self.mm_y as i32;

        match self.phase {
            AnimPhase::Idle => {
                let spr = if self.blink_timer >= 3.42 {
                    &MEGAMAN_BLINK[..]
                } else {
                    &MEGAMAN_IDLE[..]
                };
                Self::draw_sprite(
                    surface,
                    spr,
                    mx,
                    my,
                    MEGAMAN_IDLE_W,
                    MEGAMAN_IDLE_H,
                    false,
                    downscale,
                );
            }
            AnimPhase::RunLeft | AnimPhase::RunRight | AnimPhase::WalkBack => {
                let (spr, sw, sh, y_off) = match self.run_frame {
                    0 => (&MEGAMAN_RUN1[..], MEGAMAN_RUN1_W, MEGAMAN_RUN1_H, 2),
                    1 => (
                        &MEGAMAN_RUN_PASS1[..],
                        MEGAMAN_RUN_PASS_W,
                        MEGAMAN_RUN_PASS_H,
                        0,
                    ),
                    2 => (&MEGAMAN_RUN2[..], MEGAMAN_RUN2_W, MEGAMAN_RUN2_H, 2),
                    _ => (
                        &MEGAMAN_RUN_PASS1[..],
                        MEGAMAN_RUN_PASS_W,
                        MEGAMAN_RUN_PASS_H,
                        0,
                    ),
                };
                Self::draw_sprite(
                    surface,
                    spr,
                    mx,
                    my + if downscale { y_off / 2 } else { y_off },
                    sw,
                    sh,
                    self.facing_left,
                    downscale,
                );
            }
            AnimPhase::JumpLeft | AnimPhase::JumpRight => {
                Self::draw_sprite(
                    surface,
                    &MEGAMAN_JUMP,
                    mx,
                    my,
                    MEGAMAN_JUMP_W,
                    MEGAMAN_JUMP_H,
                    self.facing_left,
                    downscale,
                );
            }
        }

        if self.spark_active {
            Self::draw_sprite(
                surface,
                &HIT_SPARK,
                self.spark_x as i32,
                self.spark_y as i32,
                SPARK_W,
                SPARK_H,
                false,
                downscale,
            );
        }
    }
}

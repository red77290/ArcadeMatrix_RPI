//! 100% integer `Q8.8` fixed-point arcade fireworks & celebration particle renderer.
//!
//! Zero floating-point operations in `update()`/`render()`, zero heap allocations,
//! deterministic 60 FPS staggered crown bursts and cooling color LUT.

use crate::core::matrix::IDrawingSurface;

pub const MAX_FIREWORK_PARTICLES: usize = 24;

/// 16-direction unit circle in `Q8.8` fixed-point (`256 == 1.0`).
pub const FIREWORK_DIRS_Q8: [[i16; 2]; 16] = [
    [256, 0],
    [237, 98],
    [181, 181],
    [98, 237],
    [0, 256],
    [-98, 237],
    [-181, 181],
    [-237, 98],
    [-256, 0],
    [-237, -98],
    [-181, -181],
    [-98, -237],
    [0, -256],
    [98, -237],
    [181, -181],
    [237, -98],
];

/// Vertical bounce offset lookup table for celebration banner headers.
pub const BOUNCE_LUT: [i8; 12] = [0, -3, -5, -6, -6, -5, -3, -1, 0, -2, -1, 0];

/// Cooling color palettes (White -> Gold -> Primary Accent -> Ember Red).
pub const COOLING_PALETTES_RGB: [[(u8, u8, u8); 4]; 4] = [
    // 0: Cyan burst
    [
        (255, 255, 255),
        (255, 225, 64),
        (0, 235, 255),
        (220, 45, 20),
    ],
    // 1: Magenta burst
    [
        (255, 255, 255),
        (255, 225, 64),
        (255, 50, 220),
        (220, 45, 20),
    ],
    // 2: Gold burst
    [
        (255, 255, 255),
        (255, 240, 120),
        (255, 180, 0),
        (210, 40, 15),
    ],
    // 3: Emerald burst
    [
        (255, 255, 255),
        (255, 225, 64),
        (40, 255, 110),
        (220, 45, 20),
    ],
];

/// Fixed-size Plain-Old-Data firework particle (`0` heap allocation).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParticlePod {
    pub x_q8: i16,
    pub y_q8: i16,
    pub vx_q8: i16,
    pub vy_q8: i16,
    pub life: u8,
    pub max_life: u8,
    pub color_idx: u8,
    /// `0` = inactive, `1` = rising rocket (Phase 1), `2` = crown burst spark (Phase 2).
    pub phase: u8,
}

/// Deterministic `Q8.8` fixed-point firework renderer with a pre-allocated pool of 24 particles.
#[derive(Clone, Copy, Debug)]
pub struct FireworksLutRenderer {
    pub particles: [ParticlePod; MAX_FIREWORK_PARTICLES],
    pub frame_tick: u32,
    pub burst_seq: u8,
}

impl Default for FireworksLutRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl FireworksLutRenderer {
    pub const fn new() -> Self {
        Self {
            particles: [ParticlePod {
                x_q8: 0,
                y_q8: 0,
                vx_q8: 0,
                vy_q8: 0,
                life: 0,
                max_life: 0,
                color_idx: 0,
                phase: 0,
            }; MAX_FIREWORK_PARTICLES],
            frame_tick: 0,
            burst_seq: 0,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Spawns the initial 2 rising rockets at `frame_tick == 0` (`0.0s - 0.6s`).
    fn spawn_initial_rockets(&mut self, width: i32, height: i32) {
        let w = width.max(32);
        let h = height.max(32);
        let x0 = ((w / 4).clamp(6, w - 6) as i16) << 8;
        let x1 = (((w * 3) / 4).clamp(6, w - 6) as i16) << 8;
        let y_start = ((h - 2) as i16) << 8;
        let target_rise_px = ((h * 3) / 5).max(16) as i16;
        let vy_up = ((target_rise_px << 8) / 36).max(96);

        self.particles[0] = ParticlePod {
            x_q8: x0,
            y_q8: y_start,
            vx_q8: 14,
            vy_q8: vy_up,
            life: 36,
            max_life: 36,
            color_idx: 0,
            phase: 1,
        };
        self.particles[1] = ParticlePod {
            x_q8: x1,
            y_q8: y_start,
            vx_q8: -14,
            vy_q8: vy_up,
            life: 36,
            max_life: 36,
            color_idx: 1,
            phase: 1,
        };
    }

    /// Spawns a 16-particle crown burst centered at `(cx_px, cy_px)` in integer `Q8.8` space.
    pub fn spawn_crown_burst(
        &mut self,
        cx_px: i32,
        cy_px: i32,
        palette_idx: u8,
        speed_scale_num: i16,
    ) {
        let cx_q8 = (cx_px as i16).saturating_mul(256);
        let cy_q8 = (cy_px as i16).saturating_mul(256);
        let scale = speed_scale_num.clamp(2, 6);

        // Use slots 0..16 for the primary 16-ray crown and slots 16..24 for inner ring sparks
        for (dir_idx, dir) in FIREWORK_DIRS_Q8.iter().enumerate() {
            let vx = (dir[0] * scale) >> 2;
            let vy = (dir[1] * scale) >> 2;
            self.particles[dir_idx] = ParticlePod {
                x_q8: cx_q8,
                y_q8: cy_q8,
                vx_q8: vx,
                vy_q8: vy,
                life: 44,
                max_life: 44,
                color_idx: palette_idx & 3,
                phase: 2,
            };
        }

        for i in 0..8 {
            let dir = FIREWORK_DIRS_Q8[(i * 2 + 1) & 15];
            let vx = (dir[0] * (scale - 1).max(1)) >> 3;
            let vy = (dir[1] * (scale - 1).max(1)) >> 3;
            self.particles[16 + i] = ParticlePod {
                x_q8: cx_q8,
                y_q8: cy_q8,
                vx_q8: vx,
                vy_q8: vy,
                life: 32,
                max_life: 32,
                color_idx: (palette_idx.wrapping_add(1)) & 3,
                phase: 2,
            };
        }
    }

    /// Advances the fixed-point physics by one 60 FPS tick (`0` floats, `0` heap allocations).
    pub fn update(&mut self, width: i32, height: i32) {
        let w = width.max(32);
        let h = height.max(32);

        if self.frame_tick == 0 {
            self.spawn_initial_rockets(w, h);
        } else if self.frame_tick >= 36 && (self.frame_tick - 36).is_multiple_of(48) {
            // Staggered crown bursts across Phase 2 (0.6s - 7.5s)
            let seq = self.burst_seq as i32;
            self.burst_seq = self.burst_seq.wrapping_add(1);
            let cx = match seq & 3 {
                0 => w / 4,
                1 => (w * 3) / 4,
                2 => w / 2,
                _ => (w * 2) / 5,
            };
            let cy = match seq % 3 {
                0 => (h * 2) / 5,
                1 => h / 3,
                _ => (h * 3) / 7,
            };
            let speed = if h >= 64 { 5 } else { 3 };
            self.spawn_crown_burst(cx, cy, (seq as u8) & 3, speed);
        }

        for p in self.particles.iter_mut() {
            if p.phase == 0 || p.life == 0 {
                p.phase = 0;
                continue;
            }
            p.life = p.life.saturating_sub(1);
            if p.life == 0 {
                p.phase = 0;
                continue;
            }

            if p.phase == 1 {
                // Phase 1: Rocket rising from bottom with amber spark trail (y_q8 -= vy_q8)
                p.x_q8 = p.x_q8.saturating_add(p.vx_q8);
                p.y_q8 = p.y_q8.saturating_sub(p.vy_q8);
                p.vy_q8 = p.vy_q8.saturating_sub(2).max(32);
            } else {
                // Phase 2: Crown burst with integer gravity (+5) and air drag (vx -= vx >> 5)
                p.x_q8 = p.x_q8.saturating_add(p.vx_q8);
                p.y_q8 = p.y_q8.saturating_add(p.vy_q8);
                p.vy_q8 = p.vy_q8.saturating_add(5);
                p.vx_q8 = p.vx_q8.saturating_sub(p.vx_q8 >> 5);
            }
        }

        self.frame_tick = self.frame_tick.wrapping_add(1);
    }

    /// Renders active rockets and crown burst particles onto `surface` (`0` heap allocations).
    pub fn render(&self, surface: &mut dyn IDrawingSurface) {
        let w = surface.width() as i32;
        let h = surface.height() as i32;

        for p in &self.particles {
            if p.phase == 0 || p.life == 0 {
                continue;
            }
            let px = (p.x_q8 >> 8) as i32;
            let py = (p.y_q8 >> 8) as i32;

            if p.phase == 1 {
                // Rocket head (bright white-gold) + amber spark trail below
                if px >= 0 && px < w && py >= 0 && py < h {
                    surface.set_pixel(px, py, 255, 250, 200);
                }
                if px >= 0 && px < w && (py + 1) >= 0 && (py + 1) < h {
                    surface.set_pixel(px, py + 1, 255, 190, 40);
                }
                if px >= 0 && px < w && (py + 2) >= 0 && (py + 2) < h && (p.life & 1) == 0 {
                    surface.set_pixel(px, py + 2, 220, 90, 15);
                }
            } else {
                // Stroboscopic arcade twinkle: `(life > 12) || ((life & 1) != 0)`
                if p.life <= 12 && (p.life & 1) == 0 {
                    continue;
                }
                let max_l = p.max_life.max(1);
                let stage = if p.life > (max_l * 3) / 4 {
                    0 // White core flash
                } else if p.life > max_l / 2 {
                    1 // Gold
                } else if p.life > max_l / 4 {
                    2 // Cyan / Magenta / Emerald accent
                } else {
                    3 // Cooling ember red
                };
                let (r, g, b) = COOLING_PALETTES_RGB[(p.color_idx & 3) as usize][stage];
                if px >= 0 && px < w && py >= 0 && py < h {
                    surface.set_pixel(px, py, r, g, b);
                }
                // High-energy crossglint during initial flash on HD panels
                if stage == 0 && h >= 64 {
                    if (px + 1) < w && py >= 0 && py < h {
                        surface.set_pixel(px + 1, py, r, g, b);
                    }
                    if px >= 0 && px < w && (py + 1) < h {
                        surface.set_pixel(px, py + 1, r, g, b);
                    }
                }
            }
        }
    }

    #[inline]
    pub fn header_bounce_offset(&self) -> i32 {
        let idx = ((self.frame_tick / 3) as usize) % BOUNCE_LUT.len();
        BOUNCE_LUT[idx] as i32
    }
}

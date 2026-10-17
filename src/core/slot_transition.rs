//! Visual transition played when the rotation moves from one slot to the next.
//!
//! Moving between slots is otherwise an instant cut, and the next engine does not always have a
//! frame ready: a GIF has to be opened and read from disk first, so the panel sits blank for a
//! moment. The effect is drawn over whatever the incoming engine has managed to render, covering
//! the panel over the first half of its duration and revealing it again over the second, which
//! gives the engine underneath time to arrive.
//!
//! Mirrors the ESP32 firmware's `slot_transition` setting, including the effect names, so both
//! signs can be configured the same way.

use crate::core::matrix::IDrawingSurface;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotEffect {
    None,
    Wipe,
    Curtain,
    Shutter,
    Dissolve,
    Checker,
    MatrixRain,
    Slide,
    Zoom,
    Glitch,
    Random,
}

impl SlotEffect {
    pub fn parse(name: &str) -> SlotEffect {
        match name.trim().to_lowercase().as_str() {
            "wipe" => SlotEffect::Wipe,
            "curtain" => SlotEffect::Curtain,
            "shutter" | "blinds" => SlotEffect::Shutter,
            "dissolve" => SlotEffect::Dissolve,
            "checker" | "checkerboard" => SlotEffect::Checker,
            "matrix" | "matrix_rain" => SlotEffect::MatrixRain,
            "slide" | "smooth_slide" => SlotEffect::Slide,
            "zoom" | "tunnel_zoom" => SlotEffect::Zoom,
            "glitch" | "cyber_glitch" => SlotEffect::Glitch,
            "random" => SlotEffect::Random,
            _ => SlotEffect::None,
        }
    }

    fn concrete(self, seed: u32) -> SlotEffect {
        if self != SlotEffect::Random {
            return self;
        }
        const POOL: [SlotEffect; 9] = [
            SlotEffect::Wipe,
            SlotEffect::Curtain,
            SlotEffect::Shutter,
            SlotEffect::Dissolve,
            SlotEffect::Checker,
            SlotEffect::MatrixRain,
            SlotEffect::Slide,
            SlotEffect::Zoom,
            SlotEffect::Glitch,
        ];
        POOL[(seed as usize) % POOL.len()]
    }
}

/// The veil colour every effect paints with, and one accent so the moving edge reads.
const VEIL: (u8, u8, u8) = (16, 16, 20);
const ACCENTS: [(u8, u8, u8); 5] = [
    (0, 255, 255),
    (0, 255, 80),
    (255, 170, 0),
    (255, 0, 255),
    (255, 255, 0),
];

pub struct SlotTransition {
    effect: SlotEffect,
    duration: Duration,
    started: Option<Instant>,
    active: SlotEffect,
    counter: u32,
}

impl Default for SlotTransition {
    fn default() -> Self {
        Self {
            effect: SlotEffect::None,
            duration: Duration::from_millis(500),
            started: None,
            active: SlotEffect::None,
            counter: 0,
        }
    }
}

impl SlotTransition {
    pub fn configure(&mut self, effect: &str, duration_ms: u32) {
        self.effect = SlotEffect::parse(effect);
        self.duration = Duration::from_millis(duration_ms.clamp(100, 3000) as u64);
    }

    /// Called when the rotation advances. Does nothing when no effect is configured.
    pub fn start(&mut self) {
        if self.effect == SlotEffect::None {
            self.started = None;
            return;
        }
        self.counter = self.counter.wrapping_add(1);
        self.active = self.effect.concrete(self.counter);
        self.started = Some(Instant::now());
    }

    pub fn is_running(&self) -> bool {
        match self.started {
            Some(t) => t.elapsed() < self.duration,
            None => false,
        }
    }

    /// Paints the current frame of the effect over whatever the engine drew. Returns false when the
    /// transition has finished, so the caller can stop asking.
    pub fn render(&mut self, matrix: &mut dyn IDrawingSurface) -> bool {
        let started = match self.started {
            Some(t) => t,
            None => return false,
        };
        let elapsed = started.elapsed();
        if elapsed >= self.duration {
            self.started = None;
            return false;
        }

        let progress = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        // Covering over the first half, revealing over the second.
        let closing = progress < 0.5;
        let t = if closing {
            progress * 2.0
        } else {
            1.0 - (progress - 0.5) * 2.0
        };
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;
        let accent = ACCENTS[(self.counter as usize) % ACCENTS.len()];

        match self.active {
            SlotEffect::Wipe => {
                let edge = (t * w as f32) as i32;
                let x0 = if closing { 0 } else { w - edge };
                fill(matrix, x0, 0, edge, h, VEIL);
                let bar = if closing { edge } else { w - edge };
                vline(matrix, bar, h, accent);
            }
            SlotEffect::Curtain => {
                let half = (t * (w as f32 / 2.0 + 1.0)) as i32;
                fill(matrix, 0, 0, half, h, VEIL);
                fill(matrix, w - half, 0, half, h, VEIL);
                vline(matrix, half - 1, h, accent);
                vline(matrix, w - half, h, accent);
            }
            SlotEffect::Shutter => {
                let slat = if h >= 32 { 8 } else { 4 };
                let fill_h = (t * slat as f32) as i32;
                let mut y = 0;
                while y < h {
                    fill(matrix, 0, y, w, fill_h, VEIL);
                    if fill_h > 0 {
                        hline(matrix, y + fill_h - 1, w, accent);
                    }
                    y += slat;
                }
            }
            SlotEffect::Dissolve => {
                let level = (t * 16.0) as u32;
                let mut y = 0;
                while y < h {
                    let mut x = 0;
                    while x < w {
                        let nibble = (((x * 7) ^ (y * 13)) & 0x0F) as u32;
                        if nibble < level {
                            px(matrix, x, y, VEIL);
                            px(matrix, x + 1, y, VEIL);
                            px(matrix, x, y + 1, VEIL);
                            px(matrix, x + 1, y + 1, VEIL);
                        }
                        x += 2;
                    }
                    y += 2;
                }
            }
            SlotEffect::Checker => {
                let cell = if h >= 32 { 8 } else { 4 };
                let cols = (w + cell - 1) / cell;
                let rows = (h + cell - 1) / cell;
                let total = cols * rows;
                let filled = (t * total as f32) as i32;
                for i in 0..total {
                    let idx = ((i * 7) + (i % 3) * 11) % total;
                    if idx >= filled {
                        continue;
                    }
                    let c = if i % 2 == 0 { VEIL } else { accent };
                    fill(
                        matrix,
                        (i % cols) * cell,
                        (i / cols) * cell,
                        cell - 1,
                        cell - 1,
                        c,
                    );
                }
            }
            SlotEffect::MatrixRain => {
                // Columns of green fall in, then drain away.
                let step = 4;
                let mut x = 0;
                while x < w {
                    let phase = ((x / step) as u32).wrapping_mul(2654435761) >> 24;
                    let lead = (t * (h + 16) as f32) as i32 - (phase as i32 % 12);
                    let mut k = 0;
                    while k < 10 {
                        let y = lead - k;
                        if y >= 0 && y < h {
                            let g = 255u32.saturating_sub((k as u32) * 22);
                            px(matrix, x, y, (0, g as u8, (g / 6) as u8));
                        }
                        k += 1;
                    }
                    if lead > 0 {
                        fill(matrix, x, 0, 1, lead.min(h), VEIL);
                    }
                    x += step;
                }
            }
            SlotEffect::Slide => {
                let edge = (t * w as f32) as i32;
                fill(matrix, 0, 0, edge, h, VEIL);
                vline(matrix, edge, h, accent);
            }
            SlotEffect::Zoom => {
                let half_w = (t * (w as f32 / 2.0)) as i32;
                let half_h = (t * (h as f32 / 2.0)) as i32;
                fill(
                    matrix,
                    w / 2 - half_w,
                    h / 2 - half_h,
                    half_w * 2,
                    half_h * 2,
                    VEIL,
                );
                if half_w > 0 && half_h > 0 {
                    hline_at(matrix, w / 2 - half_w, h / 2 - half_h, half_w * 2, accent);
                    hline_at(
                        matrix,
                        w / 2 - half_w,
                        h / 2 + half_h - 1,
                        half_w * 2,
                        accent,
                    );
                }
            }
            SlotEffect::Glitch => {
                let bands = 7;
                let band_h = (h / bands).max(1);
                for b in 0..bands {
                    let seed = self.counter.wrapping_add(b as u32).wrapping_mul(2654435761) >> 20;
                    let shift = ((seed % 21) as i32 - 10) * (t * 3.0) as i32;
                    let y = b * band_h;
                    let cover = (t * w as f32) as i32;
                    fill(matrix, shift, y, cover, band_h, VEIL);
                    if b % 2 == 0 {
                        hline_at(matrix, shift, y, cover, accent);
                    }
                }
            }
            SlotEffect::None | SlotEffect::Random => {}
        }
        true
    }
}

// ---- small drawing helpers, all clipped ----

fn px(matrix: &mut dyn IDrawingSurface, x: i32, y: i32, c: (u8, u8, u8)) {
    if x >= 0 && y >= 0 && x < matrix.width() as i32 && y < matrix.height() as i32 {
        matrix.set_pixel(x, y, c.0, c.1, c.2);
    }
}

fn fill(matrix: &mut dyn IDrawingSurface, x: i32, y: i32, w: i32, h: i32, c: (u8, u8, u8)) {
    for yy in y..(y + h) {
        for xx in x..(x + w) {
            px(matrix, xx, yy, c);
        }
    }
}

fn vline(matrix: &mut dyn IDrawingSurface, x: i32, h: i32, c: (u8, u8, u8)) {
    for y in 0..h {
        px(matrix, x, y, c);
    }
}

fn hline(matrix: &mut dyn IDrawingSurface, y: i32, w: i32, c: (u8, u8, u8)) {
    for x in 0..w {
        px(matrix, x, y, c);
    }
}

fn hline_at(matrix: &mut dyn IDrawingSurface, x: i32, y: i32, w: i32, c: (u8, u8, u8)) {
    for xx in x..(x + w) {
        px(matrix, xx, y, c);
    }
}

use crate::core::matrix::MatrixBackend;
use rand::Rng;

struct MatrixColumn {
    x: i32,
    y: f32,
    speed: f32,
    trail_len: usize,
    char_code: u32, // Katakana codepoint offset (0x30A0 - 0x30FF)
    char_timer: u32,
}

pub struct TrueMatrixRenderer {
    columns: Vec<MatrixColumn>,
    /// Persistent fade buffer: each pixel is (r, g, b) and decays each frame
    buffer: Vec<Vec<(u8, u8, u8)>>,
    width: u32,
    height: u32,
}

impl TrueMatrixRenderer {
    pub fn new(width: u32, height: u32) -> Self {
        let mut rng = rand::thread_rng();
        let h = height as f32;

        // One column every 10px, as in the Python version
        let columns = (0..width as i32)
            .step_by(10)
            .map(|x| MatrixColumn {
                x,
                y: rng.gen_range(-h..0.0),
                speed: rng.gen_range(8.0_f32..12.0),
                trail_len: rng.gen_range(4..12),
                char_code: rng.gen_range(0u32..96),
                char_timer: 0,
            })
            .collect();

        let buffer = vec![vec![(0u8, 0u8, 0u8); width as usize]; height as usize];

        Self {
            columns,
            buffer,
            width,
            height,
        }
    }

    pub fn render(&mut self, matrix: &mut dyn MatrixBackend) {
        let mut rng = rand::thread_rng();
        let h = self.height;
        let w = self.width;

        // 1. Fade the entire buffer (simulates RGBA alpha-composite overlay each frame)
        for row in self.buffer.iter_mut() {
            for px in row.iter_mut() {
                px.0 = (px.0 as f32 * 0.82) as u8;
                px.1 = (px.1 as f32 * 0.82) as u8;
                px.2 = (px.2 as f32 * 0.82) as u8;
            }
        }

        // 2. Advance columns and paint into buffer
        for col in &mut self.columns {
            col.y += col.speed * 0.3; // Step per render call (called ~25fps)
            col.char_timer += 1;
            if col.char_timer > 3 {
                col.char_timer = 0;
                col.char_code = rng.gen_range(0u32..96);
            }

            if col.y > h as f32 {
                if rng.gen_bool(0.1) {
                    col.y = rng.gen_range(-20.0_f32..-10.0);
                }
                // else reset to top next frame
                col.y = rng.gen_range(-20.0_f32..-10.0);
                col.speed = rng.gen_range(8.0_f32..12.0);
                col.trail_len = rng.gen_range(4..12);
            }

            let head_y = col.y as i32;
            if head_y >= 0 && head_y < h as i32 && col.x >= 0 && col.x < w as i32 {
                // Bright head pixel — occasional white flicker
                let is_white = rng.gen_bool(0.15);
                let head_color = if is_white {
                    (255, 255, 255)
                } else {
                    (180, 255, 180)
                };
                self.buffer[head_y as usize][col.x as usize] = head_color;

                // Katakana character represented as a pixel dot cluster (approx 4×6 dots)
                // We don't have a full font renderer here, so we paint a bright dot
                // and mark the cell — visual effect is sufficient at LED matrix resolution
            }

            // Trail behind head
            for i in 1..col.trail_len {
                let trail_y = head_y - i as i32;
                if trail_y >= 0 && trail_y < h as i32 {
                    let intensity = (255.0 * (1.0 - i as f32 / col.trail_len as f32)) as u8;
                    let existing = self.buffer[trail_y as usize][col.x as usize];
                    // Blend: take the max of existing fade and new trail
                    self.buffer[trail_y as usize][col.x as usize] = (
                        existing.0.max(0),
                        existing.1.max(intensity),
                        existing.2.max(0),
                    );
                }
            }
        }

        // 3. Flush buffer to matrix
        for y in 0..h as usize {
            for x in 0..w as usize {
                let px = self.buffer[y][x];
                if px.0 > 2 || px.1 > 2 || px.2 > 2 {
                    matrix.set_pixel(x as i32, y as i32, px.0, px.1, px.2);
                }
            }
        }
    }
}

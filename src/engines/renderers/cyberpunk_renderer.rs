use crate::core::matrix::MatrixBackend;
use rand::Rng;

struct Drop {
    x: i32,
    y: f32,
    speed: f32,
    length: usize,
}

pub struct CyberpunkRenderer {
    drops: Vec<Drop>,
}

impl CyberpunkRenderer {
    pub fn new(width: u32, height: u32) -> Self {
        let mut rng = rand::thread_rng();
        let h = height as f32;
        let num_drops = (width / 3).max(15) as usize;
        let min_len = (height / 6).max(5) as usize;
        let max_len = (height / 3 + 5).max(10) as usize;

        let drops = (0..num_drops)
            .map(|_| Drop {
                x: rng.gen_range(0..width as i32),
                y: rng.gen_range(-h..0.0),
                speed: rng.gen_range(1.0_f32..((h / 10.0).max(3.0))),
                length: rng.gen_range(min_len..=max_len),
            })
            .collect();

        Self { drops }
    }

    pub fn render(&mut self, matrix: &mut dyn MatrixBackend) {
        let w = matrix.width() as i32;
        let h = matrix.height() as f32;
        let mut rng = rand::thread_rng();

        let min_len = (matrix.height() / 6).max(5) as usize;
        let max_len = (matrix.height() / 3 + 5).max(10) as usize;

        for d in &mut self.drops {
            d.y += d.speed;

            // Reset drop when the tail clears the bottom
            if d.y - d.length as f32 > h {
                d.x = rng.gen_range(0..w);
                d.y = rng.gen_range(-20.0_f32..0.0);
                d.speed = rng.gen_range(1.0_f32..(h / 10.0).max(3.0));
                d.length = rng.gen_range(min_len..=max_len);
            }

            // Draw trail from head (bright) to tail (dim)
            for j in 0..d.length {
                let py = d.y as i32 - j as i32;
                if py < 0 || py >= h as i32 {
                    continue;
                }
                if j == 0 {
                    // Bright head — white flash occasionally
                    if rng.gen_bool(0.1) {
                        matrix.set_pixel(d.x, py, 255, 255, 255);
                    } else {
                        matrix.set_pixel(d.x, py, 0, 255, 70);
                    }
                } else {
                    // Fading green tail
                    let fade = 255u8.saturating_sub(((j as f32 / d.length as f32) * 220.0) as u8);
                    matrix.set_pixel(d.x, py, 0, fade, 0);
                }
            }
        }
    }
}

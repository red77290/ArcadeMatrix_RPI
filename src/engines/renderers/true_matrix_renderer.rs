use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::base_renderer::BaseRenderer;
use rand::Rng;

struct MatrixColumn {
    x: i32,
    y: f32,
    speed: f32,
    last_grid_y: i32,
    char_code: char, // Real Katakana character
}

pub struct TrueMatrixRenderer {
    columns: Vec<MatrixColumn>,
    /// Persistent fade buffer: each pixel is (r, g, b) and decays each frame
    buffer: Vec<Vec<(u8, u8, u8)>>,
    width: u32,
    height: u32,
    renderer: BaseRenderer,
}

impl TrueMatrixRenderer {
    pub fn new(width: u32, height: u32) -> Self {
        let mut rng = rand::thread_rng();
        let h = height as f32;

        let kh = (height as i32 / 9).max(5).min(20);
        let kw = (kh * 2 / 3).max(3);
        let col_spacing = (width as i32 / 20).max(kw + 3) as usize;

        // Match python spacing: i * 10
        let col_spacing = 10;
        let columns = (0..width as i32)
            .step_by(col_spacing)
            .map(|x| MatrixColumn {
                x,
                y: rng.gen_range(-h..-10.0), // Match python init
                speed: rng.gen_range(8.0_f32..12.0),
                last_grid_y: -100,
                char_code: std::char::from_u32(rng.gen_range(0x30A0..0x3100)).unwrap_or('ア'),
            })
            .collect();

        let buffer = vec![vec![(0u8, 0u8, 0u8); width as usize]; height as usize];
        let renderer = BaseRenderer::from_font_path("DotGothic16.ttf");

        Self {
            columns,
            buffer,
            width,
            height,
            renderer,
        }
    }

    pub fn render(&mut self, matrix: &mut dyn MatrixBackend) {
        let mut rng = rand::thread_rng();
        let h = self.height;
        let w = self.width;

        // 1. Fade the entire buffer. Python used alpha 40 (~15% fade per jump).
        // Since we run at ~25fps and cross a grid cell every 3 frames, we use 0.94.
        // 0.94^3 = 0.83 (17% fade per jump).
        for row in self.buffer.iter_mut() {
            for px in row.iter_mut() {
                px.0 = (px.0 as f32 * 0.94) as u8;
                px.1 = (px.1 as f32 * 0.94) as u8;
                px.2 = (px.2 as f32 * 0.94) as u8;
            }
        }

        for col in &mut self.columns {
            col.y += col.speed * 0.3; // Moves ~3px per frame

            let grid_y = (col.y as i32 / 10) * 10; // Snap to 10px vertical grid
            
            if grid_y != col.last_grid_y {
                col.last_grid_y = grid_y;
                col.char_code = std::char::from_u32(rng.gen_range(0x30A0..0x3100)).unwrap_or('ア');

                if grid_y > -20 && grid_y < h as i32 {
                    let is_white = rng.gen_bool(0.2); // Match python < 0.2
                    let head_color = if is_white {
                        (255, 255, 255)
                    } else {
                        (180, 255, 180)
                    };

                    let char_str = col.char_code.to_string();
                    let font = self.renderer.font();
                    
                    // Python size 12
                    let (pixels, _, _) = font.get_pixel_map(&char_str, 1.5);
                    
                    for char_pixels in pixels {
                        for &(gx, gy) in &char_pixels {
                            let px = col.x + gx;
                            let py = grid_y + gy;
                            
                            if py >= 0 && py < h as i32 && px >= 0 && px < w as i32 {
                                self.buffer[py as usize][px as usize] = head_color;
                            }
                        }
                    }
                }
            }

            if col.y > h as f32 {
                // Match Python probability. Checked every frame (3x more often), so 0.05 gives plenty of rain.
                if rng.gen_bool(0.05) {
                    col.y = rng.gen_range(-20.0_f32..-10.0);
                }
                col.speed = rng.gen_range(8.0_f32..12.0);
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

use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::base_renderer::BaseRenderer;
use rand::Rng;

struct MatrixColumn {
    x: i32,
    y: f32,
    speed: f32,
    glyphs: Vec<char>,
    trail_len: usize,
}

pub struct TrueMatrixRenderer {
    columns: Vec<MatrixColumn>,
    buffer: Vec<Vec<(u8, u8, u8)>>,
    width: u32,
    height: u32,
    renderer: BaseRenderer,
}

impl TrueMatrixRenderer {
    pub fn new(width: u32, height: u32) -> Self {
        let mut rng = rand::thread_rng();
        let col_spacing = 8;
        let num_cols = (width as i32 / col_spacing).max(1);
        let num_rows = ((height as i32 + 8) / 8).max(4) as usize;

        let columns = (0..num_cols)
            .map(|i| {
                let x = i * col_spacing + (width as i32 % col_spacing) / 2;
                let y = rng.gen_range(-(height as f32)..-8.0);
                let speed = rng.gen_range(0.2..0.5);
                let glyphs = (0..num_rows + 4)
                    .map(|_| std::char::from_u32(rng.gen_range(0x30A0..0x3100)).unwrap_or('ア'))
                    .collect();
                MatrixColumn {
                    x,
                    y,
                    speed,
                    glyphs,
                    trail_len: rng.gen_range(4..8),
                }
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

        // 1. Soft fade of buffer so rain trails flow smoothly
        for row in self.buffer.iter_mut() {
            for px in row.iter_mut() {
                px.0 = (px.0 as f32 * 0.85) as u8;
                px.1 = (px.1 as f32 * 0.85) as u8;
                px.2 = (px.2 as f32 * 0.85) as u8;
            }
        }

        let font = self.renderer.font();

        for col in &mut self.columns {
            col.y += col.speed * 4.0;

            let head_grid_y = (col.y / 8.0) as i32;

            // Occasionally mutate a random glyph in the trail for classic Matrix code morphing
            if rng.gen_bool(0.04) && !col.glyphs.is_empty() {
                let idx = rng.gen_range(0..col.glyphs.len());
                col.glyphs[idx] =
                    std::char::from_u32(rng.gen_range(0x30A0..0x3100)).unwrap_or('ア');
            }

            // Draw head and trail
            for r in 0..col.trail_len {
                let grid_y = (head_grid_y - r as i32) * 8;
                if grid_y < -8 || grid_y >= h as i32 {
                    continue;
                }

                let glyph_idx =
                    ((head_grid_y - r as i32).max(0) as usize) % col.glyphs.len().max(1);
                let char_str = col.glyphs[glyph_idx].to_string();
                let (pixels, _, _) = font.get_pixel_map(&char_str, 1.0);

                let head_color = if r == 0 {
                    (255, 255, 255)
                } else if r == 1 {
                    (160, 255, 160)
                } else {
                    let factor = (1.0 - (r as f32 / col.trail_len as f32)).max(0.1);
                    (0, (200.0 * factor) as u8, (40.0 * factor) as u8)
                };

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

            if col.y > (h as f32 + (col.trail_len * 8) as f32) {
                col.y = rng.gen_range(-16.0_f32..-8.0);
                col.speed = rng.gen_range(0.2..0.5);
            }
        }

        // Flush buffer to matrix
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

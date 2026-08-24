use crate::core::matrix::MatrixBackend;
use rand::Rng;
use rusttype::{point, Font, Scale};
use std::path::Path;

pub struct TrueMatrixRenderer {
    matrix_cols: Vec<i32>,
    buffer: Vec<Vec<(u8, u8, u8)>>,
    width: u32,
    height: u32,
    font: Option<Font<'static>>,
}

impl TrueMatrixRenderer {
    pub fn new(width: u32, height: u32) -> Self {
        let mut rng = rand::thread_rng();
        // Spaced every 10 pixels for optimal matrix density: range(0, width, 10)
        let col_count = ((width + 9) / 10).max(1) as usize;
        let matrix_cols: Vec<i32> = (0..col_count)
            .map(|_| rng.gen_range(-(height as i32).max(10)..-10))
            .collect();

        let buffer = vec![vec![(0u8, 0u8, 0u8); width as usize]; height as usize];

        // Attempt to load DotGothic16.ttf from common font paths
        let font_candidates = [
            "fonts/DotGothic16.ttf",
            "../fonts/DotGothic16.ttf",
            "/usr/local/share/arcadematrix/fonts/DotGothic16.ttf",
        ];

        let mut font = None;
        for path_str in &font_candidates {
            if Path::new(path_str).exists() {
                if let Ok(data) = std::fs::read(path_str) {
                    if let Some(f) = Font::try_from_vec(data) {
                        font = Some(f);
                        break;
                    }
                }
            }
        }

        if font.is_none() {
            let embedded = include_bytes!("../../../fonts/PressStart2P.ttf");
            font = Font::try_from_bytes(embedded as &[u8]);
        }

        Self {
            matrix_cols,
            buffer,
            width,
            height,
            font,
        }
    }

    pub fn render(&mut self, matrix: &mut dyn MatrixBackend) {
        let current_w = matrix.width() as u32;
        let current_h = matrix.height() as u32;
        let mut rng = rand::thread_rng();

        if current_w != self.width || current_h != self.height {
            self.width = current_w;
            self.height = current_h;
            let col_count = ((self.width + 9) / 10).max(1) as usize;
            self.matrix_cols = (0..col_count)
                .map(|_| rng.gen_range(-(self.height as i32).max(10)..-10))
                .collect();
            self.buffer = vec![vec![(0u8, 0u8, 0u8); self.width as usize]; self.height as usize];
        }

        let h = self.height as i32;
        let w = self.width as i32;

        // 1. Fade existing buffer by alpha overlay (0, 0, 0, 40) -> factor ≈ 215/255 (0.843)
        for row in self.buffer.iter_mut() {
            for px in row.iter_mut() {
                px.0 = ((px.0 as u16 * 215) / 255) as u8;
                px.1 = ((px.1 as u16 * 215) / 255) as u8;
                px.2 = ((px.2 as u16 * 215) / 255) as u8;
            }
        }

        // 2. Render Japanese Katakana (0x30A0..=0x30FF) for active columns
        let font_ref = self.font.as_ref();

        for i in 0..self.matrix_cols.len() {
            let y = self.matrix_cols[i];
            let col_x = (i as i32) * 10;

            if y > -20 && y < h {
                let ch_code = rng.gen_range(0x30A0..=0x30FF);
                let ch = std::char::from_u32(ch_code).unwrap_or('ア');

                let color = if rng.gen_bool(0.2) {
                    (255u8, 255u8, 255u8) // Bright white leading head
                } else {
                    (180u8, 255u8, 180u8) // Bright matrix green
                };

                if let Some(font) = font_ref {
                    let scale = Scale::uniform(12.0);
                    let v_metrics = font.v_metrics(scale);
                    let glyphs: Vec<_> = font
                        .layout(&ch.to_string(), scale, point(0.0, v_metrics.ascent))
                        .collect();

                    for glyph in glyphs {
                        if let Some(bb) = glyph.pixel_bounding_box() {
                            glyph.draw(|gx, gy, v| {
                                if v > 0.25 {
                                    let px = col_x + bb.min.x + gx as i32;
                                    let py = y + bb.min.y + gy as i32;

                                    if px >= 0 && px < w && py >= 0 && py < h {
                                        self.buffer[py as usize][px as usize] = color;
                                    }
                                }
                            });
                        }
                    }
                }
            }

            // Advance column position by 8 to 12 pixels
            self.matrix_cols[i] += rng.gen_range(8..=12);

            // Reset column when reaching bottom with 10% chance per frame
            if self.matrix_cols[i] > h {
                if rng.gen_bool(0.1) {
                    self.matrix_cols[i] = rng.gen_range(-20..-10);
                }
            }
        }

        // 3. Flush buffer to matrix
        for y in 0..self.height as usize {
            for x in 0..self.width as usize {
                let px = self.buffer[y][x];
                if px.0 > 2 || px.1 > 2 || px.2 > 2 {
                    matrix.set_pixel(x as i32, y as i32, px.0, px.1, px.2);
                }
            }
        }
    }
}

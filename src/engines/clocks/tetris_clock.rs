use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::base_renderer::ArcadeFont;
use rand::Rng;
use std::collections::HashSet;

#[derive(Clone, PartialEq)]
enum BlockState {
    In,
    Fixed,
    Out,
}

#[derive(Clone)]
struct Block {
    x: f32,
    y: f32,
    target_x: f32,
    target_y: f32,
    dy: f32,
    color: (u8, u8, u8),
    state: BlockState,
    char_index: usize,
}

pub struct TetrisClock {
    blocks: Vec<Block>,
    last_time_str: String,
    gameboy_palette: bool,
    block_size: i32,
    base_dy: f32,
}

impl TetrisClock {
    pub fn new(gameboy_palette: bool) -> Self {
        Self {
            blocks: Vec::new(),
            last_time_str: String::new(),
            gameboy_palette,
            block_size: 3,
            base_dy: 1.5,
        }
    }

    /// Builds target pixel positions for each character of `time_str`,
    /// treating each character as a grid of block_size×block_size cells.
    /// Returns Vec<Vec<(f32, f32)>> indexed by character index.
    fn build_targets(
        &self,
        time_str: &str,
        w: u32,
        h: u32,
        font: &ArcadeFont<'_>,
        scale_val: u32,
    ) -> Vec<Vec<(f32, f32)>> {
        let block = self.block_size;
        let (pixels_by_char, text_width, text_height) =
            font.get_pixel_map(time_str, scale_val as f32);

        let start_x = ((w as i32) - text_width) / 2;
        let start_y = ((h as i32) - text_height) / 2;

        let mut result: Vec<Vec<(f32, f32)>> = Vec::new();

        for char_pixels in pixels_by_char {
            let mut targets = Vec::new();
            let mut block_set = HashSet::new();

            for (gx, gy) in char_pixels {
                block_set.insert((gx / block as i32, gy / block as i32));
            }

            for (bx, by) in block_set {
                let tx = start_x + (bx * block as i32);
                let ty = start_y + (by * block as i32);
                targets.push((tx as f32, ty as f32));
            }
            result.push(targets);
        }

        result
    }

    pub fn render(
        &mut self,
        matrix: &mut dyn MatrixBackend,
        time_str: &str,
        font: &ArcadeFont<'_>,
        scale: u32,
    ) {
        let w = matrix.width();
        let h = matrix.height();
        let mut rng = rand::thread_rng();

        // Match python parity for falling speed
        self.base_dy = (h as f32 / 15.0).max(1.5);

        let colors_normal: [(u8, u8, u8); 7] = [
            (0, 240, 240),
            (0, 0, 240),
            (240, 160, 0),
            (240, 240, 0),
            (0, 240, 0),
            (160, 0, 240),
            (240, 0, 0),
        ];
        let colors_gb: [(u8, u8, u8); 4] =
            [(15, 56, 15), (48, 98, 48), (139, 172, 15), (155, 188, 15)];

        if self.last_time_str != time_str {
            let targets_by_char = self.build_targets(time_str, w, h, font, scale);
            let last_targets_by_char = self.build_targets(&self.last_time_str, w, h, font, scale);

            // Find which character indices changed
            let changed: Vec<usize> = time_str
                .chars()
                .enumerate()
                .filter(|(i, ch)| {
                    self.last_time_str
                        .chars()
                        .nth(*i)
                        .map_or(true, |c| c != *ch)
                })
                .map(|(i, _)| i)
                .collect();

            if self.last_time_str.is_empty() || time_str.len() != self.last_time_str.len() {
                // Full reset
                self.blocks.clear();
                for (char_idx, targets) in targets_by_char.iter().enumerate() {
                    let palette_len = if self.gameboy_palette {
                        colors_gb.len()
                    } else {
                        colors_normal.len()
                    };
                    let color = if self.gameboy_palette {
                        colors_gb[char_idx % palette_len]
                    } else {
                        colors_normal[char_idx % palette_len]
                    };
                    for &(tx, ty) in targets {
                        self.blocks.push(Block {
                            x: tx,
                            y: ty - h as f32 - rng.gen_range(0.0..h as f32),
                            target_x: tx,
                            target_y: ty,
                            dy: rng.gen_range(self.base_dy..self.base_dy * 2.5),
                            color,
                            state: BlockState::In,
                            char_index: char_idx,
                        });
                    }
                }
            } else {
                // Smart update: only rebuild changed characters
                for &char_idx in &changed {
                    // Mark old blocks for this char as falling out
                    for b in self.blocks.iter_mut().filter(|b| b.char_index == char_idx) {
                        b.state = BlockState::Out;
                        b.dy = rng.gen_range(self.base_dy * 0.5..self.base_dy);
                    }
                    // Add new blocks
                    if char_idx < targets_by_char.len() {
                        let palette_len = if self.gameboy_palette {
                            colors_gb.len()
                        } else {
                            colors_normal.len()
                        };
                        let color = if self.gameboy_palette {
                            colors_gb[char_idx % palette_len]
                        } else {
                            colors_normal[char_idx % palette_len]
                        };
                        for &(tx, ty) in &targets_by_char[char_idx] {
                            self.blocks.push(Block {
                                x: tx,
                                y: ty - h as f32 - rng.gen_range(0.0..h as f32),
                                target_x: tx,
                                target_y: ty,
                                dy: rng.gen_range(self.base_dy..self.base_dy * 2.5),
                                color,
                                state: BlockState::In,
                                char_index: char_idx,
                            });
                        }
                    }
                }
            }

            self.last_time_str = time_str.to_string();
        }

        // Physics + draw
        let block = self.block_size;
        let mut keep = Vec::new();
        for mut b in self.blocks.drain(..) {
            match b.state {
                BlockState::In => {
                    b.y += b.dy;
                    if b.y >= b.target_y {
                        b.y = b.target_y;
                        b.state = BlockState::Fixed;
                    }
                    keep.push(b.clone());
                }
                BlockState::Out => {
                    b.y += b.dy;
                    b.dy += 0.4; // Gravity
                    if b.y < h as f32 {
                        keep.push(b.clone());
                    }
                    // else drop it (don't push)
                }
                BlockState::Fixed => {
                    keep.push(b.clone());
                }
            }

            // Draw the block
            for dy in 0..block {
                for dx in 0..block {
                    matrix.set_pixel(
                        b.x as i32 + dx,
                        b.y as i32 + dy,
                        b.color.0,
                        b.color.1,
                        b.color.2,
                    );
                }
            }
        }
        self.blocks = keep;
    }
}

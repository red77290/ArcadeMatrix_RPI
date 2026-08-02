use crate::core::matrix::MatrixBackend;
use rand::Rng;

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
    fn build_targets(&self, time_str: &str, w: u32, h: u32) -> Vec<Vec<(f32, f32)>> {
        // 4×6 bitmap glyphs for digits 0-9 and colon
        let digit_glyphs: [u8; 10 * 24] = [
            // 0
            0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 1, 1, 1, 1, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0, // 1
            0, 0, 1, 0, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1, 1, 1, // 2
            0, 1, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 0, 1, 1, 1, 1, // 3
            1, 1, 1, 0, 0, 0, 0, 1, 0, 1, 1, 0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 1, 1, 0, // 4
            0, 0, 1, 1, 0, 1, 0, 1, 1, 0, 0, 1, 1, 1, 1, 1, 0, 0, 0, 1, 0, 0, 0, 1, // 5
            1, 1, 1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 1, 1, 0, // 6
            0, 1, 1, 0, 1, 0, 0, 0, 1, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0, // 7
            1, 1, 1, 1, 0, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, // 8
            0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0, // 9
            0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 1, 1, 0,
        ];
        let colon_glyph: [u8; 24] = [
            0, 0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];

        let char_w = 4;
        let char_h = 6;
        let spacing = 1i32;
        let block = self.block_size;

        // Compute total pixel width of rendered string
        let mut total_px_w: i32 = 0;
        for ch in time_str.chars() {
            if ch == ':' {
                total_px_w += 2 * block + spacing;
            } else {
                total_px_w += char_w * block + spacing;
            }
        }
        if !time_str.is_empty() {
            total_px_w -= spacing;
        }

        let total_px_h = char_h * block;
        let start_x = ((w as i32) - total_px_w) / 2;
        let start_y = ((h as i32) - total_px_h) / 2;

        let mut result: Vec<Vec<(f32, f32)>> = Vec::new();
        let mut cur_x = start_x;

        for (char_idx, ch) in time_str.chars().enumerate() {
            let mut targets = Vec::new();
            if ch == ':' {
                // Colon: two dots
                let dot_y1 = start_y + (char_h * block) / 3;
                let dot_y2 = start_y + (2 * char_h * block) / 3;
                targets.push((cur_x as f32, dot_y1 as f32));
                targets.push((cur_x as f32, dot_y2 as f32));
                cur_x += 2 * block + spacing;
            } else if let Some(d) = ch.to_digit(10) {
                let glyph_base = (d as usize) * (char_w as usize * char_h as usize);
                for row in 0..char_h {
                    for col in 0..char_w {
                        let px = digit_glyphs[glyph_base + (row * char_w + col) as usize];
                        if px == 1 {
                            let tx = cur_x + col * block;
                            let ty = start_y + row * block;
                            targets.push((tx as f32, ty as f32));
                        }
                    }
                }
                cur_x += char_w * block + spacing;
            } else {
                cur_x += char_w * block + spacing;
            }
            result.push(targets);
        }

        result
    }

    pub fn render(&mut self, matrix: &mut dyn MatrixBackend, time_str: &str) {
        let w = matrix.width();
        let h = matrix.height();
        let mut rng = rand::thread_rng();

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
            let targets_by_char = self.build_targets(time_str, w, h);
            let last_targets_by_char = self.build_targets(&self.last_time_str, w, h);

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

use crate::core::matrix::IDrawingSurface;
use crate::engines::renderers::base_renderer::ArcadeFont;
use crate::engines::renderers::BaseRenderer;
use rand::Rng;
use std::collections::HashSet;

#[derive(Clone, PartialEq)]
enum BlockState {
    In,
    Fixed,
    Out,
}

/// Which sides of a glyph cell face open space: 1 left, 2 right, 4 top, 8 bottom.
fn edges_of(cells: &HashSet<(i32, i32)>, bx: i32, by: i32) -> u8 {
    (if cells.contains(&(bx - 1, by)) { 0 } else { 1 })
        | (if cells.contains(&(bx + 1, by)) { 0 } else { 2 })
        | (if cells.contains(&(bx, by - 1)) { 0 } else { 4 })
        | (if cells.contains(&(bx, by + 1)) { 0 } else { 8 })
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
    /// Which sides of this cell face open space in the finished digit: 1 left, 2 right, 4 top,
    /// 8 bottom. Worked out while the targets are built, so tracing the outline costs nothing at
    /// draw time. 0 for a cell buried inside the shape.
    edges: u8,
}

pub struct TetrisClock {
    blocks: Vec<Block>,
    last_time_str: String,
    gameboy_palette: bool,
    block_size: i32,
    base_dy: f32,
    /// `clock_speed` as a percentage, the same knob the ESP32 uses for the block fall.
    speed_pct: i32,
    last_frame_time: Option<std::time::Instant>,
}

impl TetrisClock {
    /// `clock_speed` as a percentage, applied when the instance config changes.
    pub fn configure(&mut self, speed_pct: i32) {
        self.speed_pct = speed_pct.clamp(25, 300);
    }

    pub fn new(gameboy_palette: bool) -> Self {
        Self {
            blocks: Vec::new(),
            last_time_str: String::new(),
            gameboy_palette,
            block_size: 3,
            base_dy: 1.0,
            speed_pct: 100,
            last_frame_time: None,
        }
    }

    /// Builds target pixel positions for each character of `time_str`,
    /// treating each character as a grid of block_size×block_size cells.
    /// Returns Vec<Vec<(x, y, edges)>> indexed by character index; see `Block::edges`.
    fn build_targets(
        &self,
        time_str: &str,
        w: u32,
        h: u32,
        font: &ArcadeFont<'_>,
        scale_val: u32,
    ) -> Vec<Vec<(f32, f32, u8)>> {
        let is_tate = w < 48 || h > (w * 3) / 2;
        let block = self.block_size;

        if is_tate {
            // Stacked Portrait Layout matching ESP32 TetrisClock
            let chars: Vec<char> = time_str.chars().collect();
            let tier_count = if chars.len() >= 8 { 3 } else { 2 };

            let (_, bw, bh) = font.get_pixel_map("88", 1.0);
            let bw = bw.max(1);
            let bh = bh.max(1);

            let scaled_w = bw * block;
            let scaled_h = bh * block;
            let tier_start_x = (w as i32 - scaled_w) / 2;

            let tier_y: [i32; 3] = if tier_count == 3 {
                [
                    (h as i32 / 6) - (scaled_h / 2),
                    (h as i32 / 2) - (scaled_h / 2),
                    (5 * h as i32 / 6) - (scaled_h / 2),
                ]
            } else {
                [
                    (h as i32 / 4) - (scaled_h / 2),
                    (3 * h as i32 / 4) - (scaled_h / 2),
                    0,
                ]
            };

            let mut result: Vec<Vec<(f32, f32, u8)>> = Vec::with_capacity(chars.len());
            let mut i = 0;
            let mut current_tier = 0;

            while i < chars.len() {
                if chars[i] == ':' || chars[i] == '/' || chars[i] == '.' || chars[i] == '-' {
                    // Omit separator targets in stacked portrait mode
                    result.push(Vec::new());
                    i += 1;
                    continue;
                }

                // Take up to 2 characters for this tier
                let mut tier_str = String::with_capacity(2);
                tier_str.push(chars[i]);
                if i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
                    tier_str.push(chars[i + 1]);
                }

                let (tier_pixels, _, _) = font.get_pixel_map(&tier_str, 1.0);
                let ty = tier_y[current_tier.min(tier_count - 1)];

                for char_pixels in tier_pixels {
                    let mut targets = Vec::new();
                    let mut block_set = HashSet::new();
                    for (gx, gy) in char_pixels {
                        block_set.insert((gx, gy));
                    }
                    for &(bx, by) in &block_set {
                        let tx = tier_start_x + (bx * block);
                        let target_y = ty + (by * block);
                        targets.push((tx as f32, target_y as f32, edges_of(&block_set, bx, by)));
                    }
                    result.push(targets);
                }

                i += tier_str.len();
                current_tier += 1;
            }

            result
        } else {
            let (pixels_by_char, text_width, text_height) = font.get_pixel_map(time_str, 1.0);

            let scaled_width = text_width * block;
            let scaled_height = text_height * block;

            let start_x = ((w as i32) - scaled_width) / 2;
            let start_y = ((h as i32) - scaled_height) / 2;

            let mut result: Vec<Vec<(f32, f32, u8)>> = Vec::new();

            for char_pixels in pixels_by_char {
                let mut targets = Vec::new();
                let mut block_set = HashSet::new();

                for (gx, gy) in char_pixels {
                    block_set.insert((gx, gy));
                }

                for &(bx, by) in &block_set {
                    let tx = start_x + (bx * block);
                    let ty = start_y + (by * block);
                    targets.push((tx as f32, ty as f32, edges_of(&block_set, bx, by)));
                }
                result.push(targets);
            }

            result
        }
    }

    pub fn render(
        &mut self,
        matrix: &mut dyn IDrawingSurface,
        time_str: &str,
        font: &ArcadeFont<'_>,
        scale: u32,
    ) {
        let w = matrix.width();
        let h = matrix.height();
        let mut rng = rand::thread_rng();

        // Frame-rate independent physics matching ESP32
        let now = std::time::Instant::now();
        let time_scale = if let Some(last) = self.last_frame_time {
            let dt = now.duration_since(last).as_secs_f32();
            if dt < 0.001 {
                // Nominal 60fps frame when called in tight unit test loop
                1.0
            } else {
                (dt / 0.016667).clamp(0.2, 3.0)
            }
        } else {
            1.0
        };
        self.last_frame_time = Some(now);

        let is_tate = w < 48 || h > (w * 3) / 2;
        // clock_speed is a percentage: lower is slower, as on the ESP32, where it scales how long
        // a digit takes to land.
        let speed = self.speed_pct as f32 / 100.0;
        self.base_dy = if is_tate {
            (((h as f32 / 3.0) / 45.0) * speed).clamp(0.6, 1.5)
        } else {
            ((h as f32 / 45.0) * speed).clamp(0.6, 1.5)
        };

        if is_tate {
            let (_, bw, bh) = font.get_pixel_map("88", 1.0);
            let bw = bw.max(1);
            let bh = bh.max(1);
            let tier_count = if time_str.len() >= 8 { 3 } else { 2 };
            let s_max_w = (w as i32 / bw).max(1);
            let s_max_h = ((h as i32 / tier_count) / bh).max(1);
            self.block_size = (scale as i32).min(s_max_w.min(s_max_h)).max(1);
        } else {
            let (_, bw, bh) = font.get_pixel_map(time_str, 1.0);
            let bw = bw.max(1);
            let bh = bh.max(1);
            let s_max = (w as i32 / bw).min(h as i32 / bh).max(1);
            self.block_size = (scale as i32).min(s_max).max(1);
        }

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
            let _last_targets_by_char = self.build_targets(&self.last_time_str, w, h, font, scale);

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
                    for &(tx, ty, edges) in targets {
                        let spawn_offset = if is_tate {
                            (h as f32 / 3.0) + rng.gen_range(0.0..(h as f32 / 4.0).max(1.0))
                        } else {
                            h as f32 + rng.gen_range(0.0..(h as f32 / 2.0).max(1.0))
                        };
                        self.blocks.push(Block {
                            x: tx,
                            y: ty - spawn_offset,
                            target_x: tx,
                            target_y: ty,
                            dy: rng.gen_range(self.base_dy..self.base_dy * 2.0),
                            color,
                            state: BlockState::In,
                            char_index: char_idx,
                            edges,
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
                        for &(tx, ty, edges) in &targets_by_char[char_idx] {
                            let spawn_offset = if is_tate {
                                (h as f32 / 3.0) + rng.gen_range(0.0..(h as f32 / 4.0).max(1.0))
                            } else {
                                h as f32 + rng.gen_range(0.0..(h as f32 / 2.0).max(1.0))
                            };
                            self.blocks.push(Block {
                                x: tx,
                                y: ty - spawn_offset,
                                target_x: tx,
                                target_y: ty,
                                dy: rng.gen_range(self.base_dy..self.base_dy * 2.0),
                                color,
                                state: BlockState::In,
                                char_index: char_idx,
                                edges,
                            });
                        }
                    }
                }
            }

            self.last_time_str = time_str.to_string();
        }

        // Physics + draw
        let block = self.block_size;

        // With the glow on, the digits stand outlined from the moment the time changes and the
        // blocks drop into that shape. Each cell already knows which of its sides face open space,
        // so this is one pass with no searching, and a buried cell costs only the test.
        let (glow_mode, glow_color) = BaseRenderer::glow_setting();
        if glow_mode != 0 {
            for b in &self.blocks {
                if b.state == BlockState::Out || b.edges == 0 {
                    continue;
                }
                // Neon traces each cell in the colour of the block landing there, the way the
                // Matrix face takes its halo from the text; mode 2 outlines it all in one colour.
                let c = if glow_mode == 1 { b.color } else { glow_color };
                let x0 = b.target_x as i32;
                let y0 = b.target_y as i32;
                let mut put = |x: i32, y: i32| matrix.set_pixel(x, y, c.0, c.1, c.2);
                if b.edges & 1 != 0 {
                    for k in 0..block {
                        put(x0 - 1, y0 + k);
                    }
                }
                if b.edges & 2 != 0 {
                    for k in 0..block {
                        put(x0 + block, y0 + k);
                    }
                }
                if b.edges & 4 != 0 {
                    for k in 0..block {
                        put(x0 + k, y0 - 1);
                    }
                }
                if b.edges & 8 != 0 {
                    for k in 0..block {
                        put(x0 + k, y0 + block);
                    }
                }
                // Close the ring at the corners the two runs leave open.
                if b.edges & 5 == 5 {
                    put(x0 - 1, y0 - 1);
                }
                if b.edges & 6 == 6 {
                    put(x0 + block, y0 - 1);
                }
                if b.edges & 9 == 9 {
                    put(x0 - 1, y0 + block);
                }
                if b.edges & 10 == 10 {
                    put(x0 + block, y0 + block);
                }
            }
        }

        let mut keep = Vec::new();
        for mut b in self.blocks.drain(..) {
            match b.state {
                BlockState::In => {
                    b.y += b.dy * time_scale;
                    if b.y >= b.target_y {
                        b.y = b.target_y;
                        b.state = BlockState::Fixed;
                    }
                    keep.push(b.clone());
                }
                BlockState::Out => {
                    b.y += b.dy * time_scale;
                    b.dy += 0.4 * time_scale; // Gravity matches ESP32
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

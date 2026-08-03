use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::BaseRenderer;
use crate::engines::renderers::base_renderer::ArcadeFont;

pub struct FlipRenderer {
    prev_chars: Vec<char>,
    flip_frame: Vec<u8>,
}

impl FlipRenderer {
    pub fn new() -> Self {
        Self {
            prev_chars: Vec::new(),
            flip_frame: Vec::new(),
        }
    }

    fn get_layout(&self, font: &ArcadeFont<'_>, scale: u32) -> (i32, i32, i32) {
        let mut max_w = 0;
        let mut max_h = 0;
        for ch in "0123456789AMP ".chars() {
            let (_, lw, lh) = font.get_pixel_map(&ch.to_string(), scale as f32);
            max_w = max_w.max(lw);
            max_h = max_h.max(lh);
        }
        if max_w == 0 { max_w = 6 * scale as i32; }
        if max_h == 0 { max_h = 10 * scale as i32; }

        let panel_w = (max_w + 2).max(4);
        let panel_h = (max_h + 4).max(8);
        let spacing = 2;

        (panel_w, panel_h, spacing)
    }

    pub fn render(
        &mut self,
        matrix: &mut dyn MatrixBackend,
        text: &str,
        font: &ArcadeFont<'_>,
        scale: u32,
        offset_x: i32,
        offset_y: i32,
    ) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;

        let chars: Vec<char> = text.chars().collect();

        if self.prev_chars.len() != chars.len() {
            self.prev_chars = chars.clone();
            self.flip_frame = vec![0u8; chars.len()];
        }

        for (i, (&cur, &prev)) in chars.iter().zip(self.prev_chars.iter()).enumerate() {
            if cur != prev && self.flip_frame[i] == 0 {
                self.flip_frame[i] = 1;
            }
        }

        let (panel_w, panel_h, spacing) = self.get_layout(font, scale);

        let mut total_w = 0;
        for &ch in &chars {
            if ch == ':' || ch == '/' || ch == '.' || ch == '-' {
                total_w += 2 + spacing;
            } else {
                total_w += panel_w + spacing;
            }
        }
        if !chars.is_empty() {
            total_w -= spacing;
        }

        let start_x = (w - total_w) / 2 + offset_x;
        let start_y = (h - panel_h) / 2 + offset_y;

        let mut cx = start_x;
        for (i, &ch) in chars.iter().enumerate() {
            let frame = self.flip_frame[i];

            if ch == ':' || ch == '/' || ch == '.' || ch == '-' {
                matrix.set_pixel(cx, start_y + panel_h / 3, 255, 255, 255);
                matrix.set_pixel(cx + 1, start_y + panel_h / 3, 255, 255, 255);
                matrix.set_pixel(cx, start_y + 2 * panel_h / 3, 255, 255, 255);
                matrix.set_pixel(cx + 1, start_y + 2 * panel_h / 3, 255, 255, 255);
                cx += 2 + spacing;
            } else {
                if frame > 0 {
                    let shrink = if frame <= 4 { frame } else { 8 - frame } as i32;
                    let shrink_px = (shrink as f32 / 4.0 * (panel_h as f32 / 2.0)) as i32;
                    
                    let top_y = start_y + shrink_px;
                    let bot_y = (start_y + panel_h - shrink_px - 1).max(top_y);
                    
                    for dy in top_y..=bot_y {
                        for dx in 0..panel_w {
                            matrix.set_pixel(cx + dx, dy, 255, 255, 255);
                        }
                    }
                    
                    let mid_y = start_y + panel_h / 2;
                    for dx in 0..panel_w {
                        matrix.set_pixel(cx + dx, mid_y, 0, 0, 0);
                    }

                    self.flip_frame[i] += 1;
                    if self.flip_frame[i] > 8 {
                        self.flip_frame[i] = 0;
                        self.prev_chars[i] = ch;
                    }
                } else {
                    for dy in 0..panel_h {
                        for dx in 0..panel_w {
                            matrix.set_pixel(cx + dx, start_y + dy, 255, 255, 255);
                        }
                    }
                    
                    BaseRenderer::draw_text_at(matrix, &ch.to_string(), font, scale as f32, cx + 1, start_y + 1, (0, 0, 0), (0, 0, 0));
                    
                    let mid_y = start_y + panel_h / 2;
                    for dx in 0..panel_w {
                        matrix.set_pixel(cx + dx, mid_y, 0, 0, 0);
                    }
                }
                cx += panel_w + spacing;
            }
        }

        for (i, &ch) in chars.iter().enumerate() {
            if self.flip_frame[i] == 0 {
                self.prev_chars[i] = ch;
            }
        }
    }
}

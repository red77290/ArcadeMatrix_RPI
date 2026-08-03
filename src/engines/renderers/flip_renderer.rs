use crate::core::matrix::MatrixBackend;
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
        if max_w == 0 {
            max_w = 6 * scale as i32;
        }
        if max_h == 0 {
            max_h = 10 * scale as i32;
        }

        let panel_w = (max_w + 2).max(4);
        let panel_h = (max_h + 4).max(8);
        let spacing = 2;

        (panel_w, panel_h, spacing)
    }

    fn draw_flap(
        matrix: &mut dyn MatrixBackend,
        cx: i32,
        cy: i32,
        panel_w: i32,
        _panel_h: i32,
        pixels: &Vec<Vec<(i32, i32)>>,
        ox: i32,
        oy: i32,
        flap_top: i32,
        flap_bot: i32,
        crop_top: i32,
        crop_bot: i32,
        is_moving_flap: bool,
    ) {
        if flap_top > flap_bot {
            return;
        }

        // Fill white background
        for y in flap_top..=flap_bot {
            for x in 0..panel_w {
                matrix.set_pixel(cx + x, cy + y, 255, 255, 255);
            }
            // Side borders for the card
            matrix.set_pixel(cx, cy + y, 200, 200, 200);
            matrix.set_pixel(cx + panel_w - 1, cy + y, 200, 200, 200);
        }

        if is_moving_flap {
            // Draw a darker shadow on the moving edge
            let moving_y = if crop_top == 0 { flap_top } else { flap_bot };
            for x in 0..panel_w {
                matrix.set_pixel(cx + x, cy + moving_y, 100, 100, 100);
            }
        } else {
            // Draw top/bottom borders for the static card
            let static_y = if crop_top == 0 { flap_top } else { flap_bot };
            for x in 0..panel_w {
                matrix.set_pixel(cx + x, cy + static_y, 200, 200, 200);
            }
        }

        // Draw black text pixels if they fall in the crop region
        for char_pixels in pixels {
            for &(px, py) in char_pixels {
                let local_y = oy + py;
                if local_y >= crop_top && local_y <= crop_bot {
                    let mapped_y = if crop_bot == crop_top {
                        flap_top
                    } else {
                        flap_top
                            + (local_y - crop_top) * (flap_bot - flap_top) / (crop_bot - crop_top)
                    };
                    matrix.set_pixel(cx + ox + px, cy + mapped_y, 0, 0, 0);
                }
            }
        }
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
        let top_end = panel_h / 2 - 1;
        let bot_start = panel_h / 2;
        let bot_end = panel_h - 1;

        let mut cx = start_x;
        for (i, &cur) in chars.iter().enumerate() {
            let prev = self.prev_chars[i];
            let frame = self.flip_frame[i];

            if cur == ':' || cur == '/' || cur == '.' || cur == '-' {
                matrix.set_pixel(cx, start_y + panel_h / 3, 255, 255, 255);
                matrix.set_pixel(cx + 1, start_y + panel_h / 3, 255, 255, 255);
                matrix.set_pixel(cx, start_y + 2 * panel_h / 3, 255, 255, 255);
                matrix.set_pixel(cx + 1, start_y + 2 * panel_h / 3, 255, 255, 255);
                cx += 2 + spacing;
            } else {
                let (cur_pixels, cur_w, cur_h) = font.get_pixel_map(&cur.to_string(), scale as f32);
                let (prev_pixels, prev_w, prev_h) =
                    font.get_pixel_map(&prev.to_string(), scale as f32);
                let cur_ox = (panel_w - cur_w) / 2;
                let cur_oy = (panel_h - cur_h) / 2;
                let prev_ox = (panel_w - prev_w) / 2;
                let prev_oy = (panel_h - prev_h) / 2;

                if frame == 0 {
                    // Static display of current character
                    Self::draw_flap(
                        matrix,
                        cx,
                        start_y,
                        panel_w,
                        panel_h,
                        &cur_pixels,
                        cur_ox,
                        cur_oy,
                        0,
                        top_end,
                        0,
                        top_end,
                        false,
                    );
                    Self::draw_flap(
                        matrix,
                        cx,
                        start_y,
                        panel_w,
                        panel_h,
                        &cur_pixels,
                        cur_ox,
                        cur_oy,
                        bot_start,
                        bot_end,
                        bot_start,
                        bot_end,
                        false,
                    );
                } else {
                    // Animating
                    // 1. Draw static TOP half of NEW character
                    Self::draw_flap(
                        matrix,
                        cx,
                        start_y,
                        panel_w,
                        panel_h,
                        &cur_pixels,
                        cur_ox,
                        cur_oy,
                        0,
                        top_end,
                        0,
                        top_end,
                        false,
                    );

                    // 2. Draw static BOT half of OLD character
                    Self::draw_flap(
                        matrix,
                        cx,
                        start_y,
                        panel_w,
                        panel_h,
                        &prev_pixels,
                        prev_ox,
                        prev_oy,
                        bot_start,
                        bot_end,
                        bot_start,
                        bot_end,
                        false,
                    );

                    let shrink = if frame <= 4 { frame } else { 8 - frame } as i32;
                    let shrink_px = (shrink as f32 / 4.0 * (panel_h as f32 / 2.0)) as i32;

                    if frame <= 4 {
                        // 3a. Draw falling TOP half of OLD character
                        Self::draw_flap(
                            matrix,
                            cx,
                            start_y,
                            panel_w,
                            panel_h,
                            &prev_pixels,
                            prev_ox,
                            prev_oy,
                            shrink_px,
                            top_end,
                            0,
                            top_end,
                            true,
                        );
                    } else {
                        // 3b. Draw falling BOT half of NEW character
                        Self::draw_flap(
                            matrix,
                            cx,
                            start_y,
                            panel_w,
                            panel_h,
                            &cur_pixels,
                            cur_ox,
                            cur_oy,
                            bot_start,
                            bot_end - shrink_px,
                            bot_start,
                            bot_end,
                            true,
                        );
                    }

                    self.flip_frame[i] += 1;
                    if self.flip_frame[i] > 8 {
                        self.flip_frame[i] = 0;
                    }
                }

                // Draw center black line for the split flap mechanism
                let mid_y = start_y + panel_h / 2;
                for dx in 0..panel_w {
                    matrix.set_pixel(cx + dx, mid_y, 0, 0, 0);
                }

                cx += panel_w + spacing;
            }
        }

        // Update prev_chars only after the animation completes!
        for (i, &ch) in chars.iter().enumerate() {
            if self.flip_frame[i] == 0 {
                self.prev_chars[i] = ch;
            }
        }
    }
}

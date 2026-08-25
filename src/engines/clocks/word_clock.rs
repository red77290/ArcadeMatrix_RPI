use crate::core::i18n::{self, Lang};
use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::base_renderer::ArcadeFont;
use crate::engines::renderers::BaseRenderer;

pub struct WordClock;

impl WordClock {
    pub fn new() -> Self {
        Self
    }

    pub fn render(
        &self,
        matrix: &mut dyn MatrixBackend,
        hours: u32,
        minutes: u32,
        font: &ArcadeFont<'_>,
        scale: u32,
        lang: &str,
    ) {
        let l = Lang::from_code(lang);
        let lines = i18n::word_clock_lines(l, hours, minutes);
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;
        self.draw_lines(matrix, &lines, font, scale, w, h);
    }

    fn draw_lines(
        &self,
        matrix: &mut dyn MatrixBackend,
        raw_lines: &[String],
        font: &ArcadeFont<'_>,
        requested_scale: u32,
        w: i32,
        h: i32,
    ) {
        let scale = requested_scale.max(1);
        let max_chars = (w / (6 * scale as i32)).max(1) as usize;
        let mut lines = Vec::new();

        for raw in raw_lines {
            if raw.len() <= max_chars {
                lines.push(raw.clone());
            } else {
                let words: Vec<&str> = raw.split_whitespace().collect();
                let mut current = String::new();
                for word in words {
                    if current.is_empty() {
                        current = word.to_string();
                    } else if current.len() + 1 + word.len() <= max_chars {
                        current.push(' ');
                        current.push_str(word);
                    } else {
                        lines.push(current);
                        current = word.to_string();
                    }
                }
                if !current.is_empty() {
                    lines.push(current);
                }
            }
        }

        let mut line_spacing = if h >= 64 { 3 } else { 1 } * scale as i32;
        let mut total_h = 0;
        let mut line_heights = Vec::new();

        for line in &lines {
            let (_, _, lh) = font.get_pixel_map(line, scale as f32);
            let final_lh = if lh == 0 { 8 * scale as i32 } else { lh };
            line_heights.push(final_lh);
            total_h += final_lh + line_spacing;
        }
        if total_h > 0 {
            total_h -= line_spacing;
        }

        if total_h > h && line_spacing > 1 {
            line_spacing = 1;
            total_h = 0;
            for lh in &line_heights {
                total_h += *lh + line_spacing;
            }
            if total_h > 0 {
                total_h -= line_spacing;
            }
        }

        let mut y = (h - total_h) / 2;
        for (i, line) in lines.iter().enumerate() {
            let (_, lw, _) = font.get_pixel_map(line, scale as f32);
            let x = (w - lw) / 2;
            let c = if i % 2 == 0 {
                (0, 220, 255)
            } else {
                (255, 120, 0)
            };
            BaseRenderer::draw_text_at(matrix, line, font, scale as f32, x, y, c, (0, 0, 0));
            y += line_heights[i] + line_spacing;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockMatrix {
        w: u32,
        h: u32,
    }
    impl MatrixBackend for MockMatrix {
        fn width(&self) -> u32 {
            self.w
        }
        fn height(&self) -> u32 {
            self.h
        }
        fn set_pixel(&mut self, _x: i32, _y: i32, _red: u8, _green: u8, _blue: u8) {}
        fn clear(&mut self) {}
        fn update(&mut self) {}
        fn set_brightness(&mut self, _brightness: u8) {}
    }

    #[test]
    fn test_word_clock_renders_without_crash() {
        let clock = WordClock::new();
        let mut matrix = MockMatrix { w: 128, h: 32 };
        let renderer = BaseRenderer::new();
        let font = renderer.font();

        for m in (0..=55).step_by(5) {
            clock.render(&mut matrix, 10, m, &font, 1, "fr");
            clock.render(&mut matrix, 10, m, &font, 1, "en");
            clock.render(&mut matrix, 10, m, &font, 1, "es");
        }
    }
}

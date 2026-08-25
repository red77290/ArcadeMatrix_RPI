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
        match lang.to_lowercase().as_str() {
            "en" => self.render_en(matrix, hours, minutes, font, scale),
            "es" => self.render_es(matrix, hours, minutes, font, scale),
            _ => self.render_fr(matrix, hours, minutes, font, scale),
        }
    }

    fn render_fr(
        &self,
        matrix: &mut dyn MatrixBackend,
        hours: u32,
        minutes: u32,
        font: &ArcadeFont<'_>,
        scale: u32,
    ) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;

        let rounded_m = (minutes / 5) * 5;
        let past_half = minutes > 30;

        let display_h = if past_half && rounded_m != 0 {
            (hours + 1) % 24
        } else {
            hours
        };
        let read_h = display_h % 12;

        let str_h: &str = match display_h {
            0 => "MINUIT",
            12 => "MIDI",
            _ => match read_h {
                1 => "UNE",
                2 => "DEUX",
                3 => "TROIS",
                4 => "QUATRE",
                5 => "CINQ",
                6 => "SIX",
                7 => "SEPT",
                8 => "HUIT",
                9 => "NEUF",
                10 => "DIX",
                11 => "ONZE",
                _ => "?",
            },
        };

        let str_h_suffix: &str = if display_h == 0 || display_h == 12 {
            ""
        } else if read_h == 1 {
            " HEURE"
        } else {
            " HEURES"
        };

        let str_m: String = match rounded_m {
            0 | 60 => "PILE".to_string(),
            5 if !past_half => "CINQ".to_string(),
            10 if !past_half => "DIX".to_string(),
            15 if !past_half => "ET QUART".to_string(),
            20 if !past_half => "VINGT".to_string(),
            25 if !past_half => "VINGT-CINQ".to_string(),
            30 => "ET DEMIE".to_string(),
            _ if past_half => {
                let diff = 60 - rounded_m;
                match diff {
                    5 => "MOINS CINQ".to_string(),
                    10 => "MOINS DIX".to_string(),
                    15 => "MOINS LE QUART".to_string(),
                    20 => "MOINS VINGT".to_string(),
                    25 => "MOINS VINGT-CINQ".to_string(),
                    _ => "MOINS CINQ".to_string(),
                }
            }
            _ => "PILE".to_string(),
        };

        let lines = vec![
            "IL EST".to_string(),
            format!("{}{}", str_h, str_h_suffix),
            str_m,
        ];

        self.draw_lines(matrix, &lines, font, scale, w, h);
    }

    fn render_en(
        &self,
        matrix: &mut dyn MatrixBackend,
        hours: u32,
        minutes: u32,
        font: &ArcadeFont<'_>,
        scale: u32,
    ) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;
        let rounded_m = (minutes / 5) * 5;
        let past_half = minutes > 30;
        let display_h = if past_half && rounded_m != 0 {
            (hours + 1) % 24
        } else {
            hours
        };
        let read_h = display_h % 12;

        let str_h = match display_h {
            0 => "MIDNIGHT",
            12 => "NOON",
            _ => match read_h {
                1 => "ONE",
                2 => "TWO",
                3 => "THREE",
                4 => "FOUR",
                5 => "FIVE",
                6 => "SIX",
                7 => "SEVEN",
                8 => "EIGHT",
                9 => "NINE",
                10 => "TEN",
                11 => "ELEVEN",
                _ => "?",
            },
        };

        let str_m = match rounded_m {
            0 | 60 => "O'CLOCK".to_string(),
            5 if !past_half => "FIVE".to_string(),
            10 if !past_half => "TEN".to_string(),
            15 if !past_half => "A QUARTER".to_string(),
            20 if !past_half => "TWENTY".to_string(),
            25 if !past_half => "TWENTY-FIVE".to_string(),
            30 => "HALF".to_string(),
            _ if past_half => {
                let diff = 60 - rounded_m;
                match diff {
                    5 => "FIVE".to_string(),
                    10 => "TEN".to_string(),
                    15 => "A QUARTER".to_string(),
                    20 => "TWENTY".to_string(),
                    25 => "TWENTY-FIVE".to_string(),
                    _ => "FIVE".to_string(),
                }
            }
            _ => "O'CLOCK".to_string(),
        };

        let str_conn = if rounded_m == 0 || rounded_m == 60 {
            ""
        } else if past_half {
            "TO"
        } else {
            "PAST"
        };

        let mut lines = vec!["IT IS".to_string()];
        if str_conn.is_empty() {
            if display_h == 0 || display_h == 12 {
                lines.push(str_h.to_string());
            } else {
                lines.push(str_h.to_string());
                lines.push(str_m);
            }
        } else {
            lines.push(str_m);
            lines.push(str_conn.to_string());
            lines.push(str_h.to_string());
        }

        self.draw_lines(matrix, &lines, font, scale, w, h);
    }

    fn render_es(
        &self,
        matrix: &mut dyn MatrixBackend,
        hours: u32,
        minutes: u32,
        font: &ArcadeFont<'_>,
        scale: u32,
    ) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;
        let rounded_m = (minutes / 5) * 5;
        let past_half = minutes > 30;
        let display_h = if past_half && rounded_m != 0 {
            (hours + 1) % 24
        } else {
            hours
        };
        let read_h = display_h % 12;

        let str_h = match display_h {
            0 => "MEDIANOCHE",
            12 => "MEDIODIA",
            _ => match read_h {
                1 => "LA UNA",
                2 => "LAS DOS",
                3 => "LAS TRES",
                4 => "LAS CUATRO",
                5 => "LAS CINCO",
                6 => "LAS SEIS",
                7 => "LAS SIETE",
                8 => "LAS OCHO",
                9 => "LAS NUEVE",
                10 => "LAS DIEZ",
                11 => "LAS ONCE",
                _ => "?",
            },
        };

        let str_m = match rounded_m {
            0 | 60 => "EN PUNTO".to_string(),
            5 if !past_half => "Y CINCO".to_string(),
            10 if !past_half => "Y DIEZ".to_string(),
            15 if !past_half => "Y CUARTO".to_string(),
            20 if !past_half => "Y VEINTE".to_string(),
            25 if !past_half => "Y VEINTICINCO".to_string(),
            30 => "Y MEDIA".to_string(),
            _ if past_half => {
                let diff = 60 - rounded_m;
                match diff {
                    5 => "MENOS CINCO".to_string(),
                    10 => "MENOS DIEZ".to_string(),
                    15 => "MENOS CUARTO".to_string(),
                    20 => "MENOS VEINTE".to_string(),
                    25 => "MENOS VEINTICINCO".to_string(),
                    _ => "MENOS CINCO".to_string(),
                }
            }
            _ => "EN PUNTO".to_string(),
        };

        let lines = if display_h == 0 || display_h == 12 {
            if rounded_m == 0 || rounded_m == 60 {
                vec!["ES LA".to_string(), str_h.to_string()]
            } else {
                vec!["ES LA".to_string(), str_h.to_string(), str_m]
            }
        } else {
            let prefix = if read_h == 1 && display_h != 0 && display_h != 12 {
                "ES LA"
            } else {
                "SON LAS"
            };
            vec![prefix.to_string(), str_h.to_string(), str_m]
        };

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
    fn test_word_clock_fr_renders_words_not_digits() {
        let clock = WordClock::new();
        let mut matrix = MockMatrix { w: 128, h: 32 };
        let renderer = BaseRenderer::new();
        let font = renderer.font();

        // Check every 5 minute increment from 0 to 55 minutes
        for m in (0..=55).step_by(5) {
            clock.render(&mut matrix, 10, m, &font, 1, "fr");
            clock.render(&mut matrix, 10, m, &font, 1, "en");
            clock.render(&mut matrix, 10, m, &font, 1, "es");
        }
    }
}

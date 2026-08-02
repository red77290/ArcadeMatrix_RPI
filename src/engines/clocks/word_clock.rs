use crate::core::matrix::MatrixBackend;

pub struct WordClock;

impl WordClock {
    pub fn new() -> Self {
        Self
    }

    pub fn render(&self, matrix: &mut dyn MatrixBackend, hours: u32, minutes: u32) {
        // Determine language from compile-time default; could be wired to config later
        self.render_fr(matrix, hours, minutes);
    }

    fn render_fr(&self, matrix: &mut dyn MatrixBackend, hours: u32, minutes: u32) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;

        let rounded_m = (minutes / 5) * 5;
        let past_half = minutes > 32;

        let display_h = if past_half { (hours + 1) % 24 } else { hours };
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
                    _ => format!("MOINS {}", diff),
                }
            }
            _ => format!("{}", rounded_m),
        };

        let lines = [
            "IL EST".to_string(),
            format!("{}{}", str_h, str_h_suffix),
            str_m,
        ];

        // Pixel-text rendering: 3×5 glyph per character, 1px spacing
        // Calculate total height
        let char_h = 5i32;
        let line_spacing = 3i32;
        let total_h = lines.len() as i32 * (char_h + line_spacing) - line_spacing;
        let mut y = (h - total_h) / 2;

        for (i, line) in lines.iter().enumerate() {
            let text_w = self.text_pixel_width(line);
            let x = (w - text_w) / 2;
            let color = if i % 2 == 0 {
                (0, 220, 255)
            } else {
                (255, 120, 0)
            };
            self.draw_word(matrix, line, x, y, color);
            y += char_h + line_spacing;
        }
    }

    fn text_pixel_width(&self, text: &str) -> i32 {
        let mut w = 0i32;
        for ch in text.chars() {
            w += if ch == ' ' { 3 } else { 4 };
        }
        if w > 0 {
            w - 1
        } else {
            0
        }
    }

    fn draw_word(
        &self,
        matrix: &mut dyn MatrixBackend,
        text: &str,
        x: i32,
        y: i32,
        color: (u8, u8, u8),
    ) {
        let mut cx = x;
        for ch in text.chars() {
            if ch == ' ' {
                cx += 3;
                continue;
            }
            self.draw_char(matrix, ch, cx, y, color);
            cx += 4;
        }
    }

    fn draw_char(
        &self,
        matrix: &mut dyn MatrixBackend,
        ch: char,
        x: i32,
        y: i32,
        color: (u8, u8, u8),
    ) {
        // 3×5 bitmaps for A-Z and dash
        let bitmap: [u8; 15] = match ch.to_ascii_uppercase() {
            'A' => [0, 1, 0, 1, 0, 1, 1, 1, 1, 1, 0, 1, 1, 0, 1],
            'B' => [1, 1, 0, 1, 0, 1, 1, 1, 0, 1, 0, 1, 1, 1, 0],
            'C' => [0, 1, 1, 1, 0, 0, 1, 0, 0, 1, 0, 0, 0, 1, 1],
            'D' => [1, 1, 0, 1, 0, 1, 1, 0, 1, 1, 0, 1, 1, 1, 0],
            'E' => [1, 1, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1, 1, 1],
            'F' => [1, 1, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1, 0, 0],
            'G' => [0, 1, 1, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 1, 1],
            'H' => [1, 0, 1, 1, 0, 1, 1, 1, 1, 1, 0, 1, 1, 0, 1],
            'I' => [1, 1, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0, 1, 1, 1],
            'J' => [0, 0, 1, 0, 0, 1, 0, 0, 1, 1, 0, 1, 0, 1, 0],
            'K' => [1, 0, 1, 1, 1, 0, 1, 0, 0, 1, 1, 0, 1, 0, 1],
            'L' => [1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 1, 1],
            'M' => [1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 0, 1],
            'N' => [1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1],
            'O' => [0, 1, 0, 1, 0, 1, 1, 0, 1, 1, 0, 1, 0, 1, 0],
            'P' => [1, 1, 0, 1, 0, 1, 1, 1, 0, 1, 0, 0, 1, 0, 0],
            'Q' => [0, 1, 0, 1, 0, 1, 1, 0, 1, 1, 1, 1, 0, 1, 1],
            'R' => [1, 1, 0, 1, 0, 1, 1, 1, 0, 1, 1, 0, 1, 0, 1],
            'S' => [0, 1, 1, 1, 0, 0, 0, 1, 0, 0, 0, 1, 1, 1, 0],
            'T' => [1, 1, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0],
            'U' => [1, 0, 1, 1, 0, 1, 1, 0, 1, 1, 0, 1, 0, 1, 0],
            'V' => [1, 0, 1, 1, 0, 1, 1, 0, 1, 0, 1, 0, 0, 1, 0],
            'W' => [1, 0, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1],
            'X' => [1, 0, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0, 1, 0, 1],
            'Y' => [1, 0, 1, 1, 0, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0],
            'Z' => [1, 1, 1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 1, 1, 1],
            '-' => [0, 0, 0, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0],
            _ => [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        };
        for row in 0..5i32 {
            for col in 0..3i32 {
                if bitmap[(row * 3 + col) as usize] == 1 {
                    matrix.set_pixel(x + col, y + row, color.0, color.1, color.2);
                }
            }
        }
    }
}

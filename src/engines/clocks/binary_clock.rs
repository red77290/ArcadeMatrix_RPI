use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::base_renderer::ArcadeFont;

pub struct BinaryClock;

impl BinaryClock {
    pub fn new() -> Self {
        Self
    }

    pub fn render(
        &self,
        matrix: &mut dyn MatrixBackend,
        hours: u32,
        minutes: u32,
        seconds: u32,
        _font: &ArcadeFont<'_>,
        _scale: u32,
    ) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;

        // 6 columns: H1 H2 M1 M2 S1 S2
        let digits = [
            hours / 10,
            hours % 10,
            minutes / 10,
            minutes % 10,
            seconds / 10,
            seconds % 10,
        ];
        // Max representable bits per column (BCD: tens are 0-2, units are 0-9)
        let max_bits: [u32; 6] = [2, 4, 3, 4, 3, 4];

        // Colors per group: Hours = color1 (cyan), Minutes = color2 (magenta), Seconds = white
        let colors: [(u8, u8, u8); 3] = [(0, 220, 255), (255, 0, 180), (200, 200, 200)];
        let dim_color: (u8, u8, u8) = (25, 25, 25);

        let dot_r = ((w / 20).max(2)).min(h / 12) as i32;
        let spacing_x = w / 8;
        let spacing_y = h / 6;

        // Center the 6 columns (with gap after col 1 and col 3)
        // Extra gap between H/M and M/S pairs
        let mut start_x = (w - 5 * spacing_x) / 2;
        let start_y = h - h / 6;

        for (col, &val) in digits.iter().enumerate() {
            // Extra horizontal gap between pairs (H|M and M|S)
            if col == 2 || col == 4 {
                start_x += spacing_x / 2;
            }
            let x = start_x + col as i32 * spacing_x;
            let color_idx = col / 2; // 0=H, 1=M, 2=S
            let fg = colors[color_idx];

            for bit in 0..max_bits[col] {
                let y = start_y - bit as i32 * spacing_y;
                let is_on = (val >> bit) & 1 == 1;

                if is_on {
                    // Filled circle for "on" bit
                    for dy in -dot_r..=dot_r {
                        for dx in -dot_r..=dot_r {
                            if dx * dx + dy * dy <= dot_r * dot_r {
                                matrix.set_pixel(x + dx, y + dy, fg.0, fg.1, fg.2);
                            }
                        }
                    }
                } else {
                    // Dim outline circle for "off" bit
                    for dy in -dot_r..=dot_r {
                        for dx in -dot_r..=dot_r {
                            let dist_sq = dx * dx + dy * dy;
                            if dist_sq <= dot_r * dot_r && dist_sq >= (dot_r - 1) * (dot_r - 1) {
                                matrix.set_pixel(
                                    x + dx,
                                    y + dy,
                                    dim_color.0,
                                    dim_color.1,
                                    dim_color.2,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

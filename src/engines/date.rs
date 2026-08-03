use crate::core::config::Config;
use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::{
    BaseRenderer, CyberpunkRenderer, FlipRenderer, TrueMatrixRenderer,
};
use chrono::Local;

pub struct DateEngine {
    base_renderer: BaseRenderer,
    cyberpunk: CyberpunkRenderer,
    flip: FlipRenderer,
    true_matrix: TrueMatrixRenderer,
    last_font: String,
}

impl DateEngine {
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            base_renderer: BaseRenderer::new(),
            cyberpunk: CyberpunkRenderer::new(w, h),
            flip: FlipRenderer::new(),
            true_matrix: TrueMatrixRenderer::new(w, h),
            last_font: String::new(),
        }
    }

    pub fn render(&mut self, matrix: &mut dyn MatrixBackend, config: &Config) {
        let settings = config.settings.read();
        let now = Local::now();
        let date_str = now.format(&settings.date_format).to_string();

        // Reload font if changed
        if settings.date_font != self.last_font {
            self.base_renderer = BaseRenderer::from_font_path(&settings.date_font);
            self.last_font = settings.date_font.clone();
        }

        let color1 = parse_hex_color(&settings.date_color_1).unwrap_or((255, 255, 255));
        let color2 = parse_hex_color(&settings.date_color_2).unwrap_or((255, 255, 255));

        match settings.date_theme {
            18 => {
                self.cyberpunk.render(matrix);
                self.base_renderer.render_text(
                    matrix,
                    &date_str,
                    0,
                    settings.date_size,
                    settings.date_offset_x,
                    settings.date_offset_y,
                    Some(color1),
                    None,
                );
            }
            21 => {
                self.true_matrix.render(matrix);
                self.base_renderer.render_text(
                    matrix,
                    &date_str,
                    0,
                    settings.date_size,
                    settings.date_offset_x,
                    settings.date_offset_y,
                    Some((0, 255, 70)),
                    None,
                );
            }
            20 => {
                let font = self.base_renderer.font();
                self.flip.render(matrix, &date_str, &font, settings.date_size, settings.date_offset_x, settings.date_offset_y);
            }
            _ => {
                self.base_renderer.render_text(
                    matrix,
                    &date_str,
                    settings.date_theme,
                    settings.date_size,
                    settings.date_offset_x,
                    settings.date_offset_y,
                    Some(color1),
                    Some(color2),
                );
            }
        }
    }
}

fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some((r, g, b))
    } else {
        None
    }
}

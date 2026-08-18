use crate::core::config::Config;
use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::{
    BaseRenderer, CyberpunkRenderer, FlipRenderer, TrueMatrixRenderer,
};

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
        let tz: chrono_tz::Tz = config
            .settings
            .read()
            .timezone
            .parse()
            .unwrap_or(chrono_tz::UTC);
        let now = chrono::Utc::now().with_timezone(&tz);
        let mut format_str = settings.date_format.clone();
        format_str = format_str.replace("YYYY", "%Y");
        format_str = format_str.replace("YY", "%y");
        format_str = format_str.replace("MM", "%m");
        format_str = format_str.replace("DD", "%d");
        let date_str = now.format(&format_str).to_string();

        // Reload font if changed
        if settings.date_font != self.last_font {
            self.base_renderer = BaseRenderer::from_font_path(&settings.date_font);
            self.last_font = settings.date_font.clone();
            self.flip.reset();
        }

        let color1 = parse_hex_color(&settings.date_color_1);
        let color2 = parse_hex_color(&settings.date_color_2);

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
                    color1,
                    None,
                );
            }
            21 => {
                self.true_matrix.render(matrix);
                self.base_renderer.render_text(
                    matrix,
                    &date_str,
                    21,
                    settings.date_size,
                    settings.date_offset_x,
                    settings.date_offset_y,
                    Some((0, 140, 0)),
                    Some((0, 0, 0)),
                );
            }
            19 => {
                let font = self.base_renderer.font();
                self.flip.render(
                    matrix,
                    &date_str,
                    &font,
                    settings.date_size,
                    settings.date_offset_x,
                    settings.date_offset_y,
                );
            }
            20 => {
                self.base_renderer.render_text(
                    matrix,
                    &date_str,
                    20,
                    settings.date_size,
                    settings.date_offset_x,
                    settings.date_offset_y,
                    color1,
                    color2,
                );
            }
            _ => {
                self.base_renderer.render_text(
                    matrix,
                    &date_str,
                    settings.date_theme,
                    settings.date_size,
                    settings.date_offset_x,
                    settings.date_offset_y,
                    None,
                    None,
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

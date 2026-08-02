use crate::core::matrix::MatrixBackend;
use crate::core::theme::get_theme_info;
use rusttype::{Font, Scale};
use std::sync::OnceLock;

// Embedded fallback font (always available, zero-cost after first init)
static EMBEDDED_FONT: OnceLock<Font<'static>> = OnceLock::new();

fn get_embedded_font() -> &'static Font<'static> {
    EMBEDDED_FONT.get_or_init(|| {
        let font_data = include_bytes!("../../../fonts/PressStart2P.ttf");
        Font::try_from_bytes(font_data as &[u8]).expect("Embedded font is malformed")
    })
}

pub struct BaseRenderer {
    /// Loaded font bytes — kept alive so the Font<'_> can borrow from them.
    custom_font_bytes: Option<Box<[u8]>>,
}

impl BaseRenderer {
    /// Uses the embedded PressStart2P font.
    pub fn new() -> Self {
        Self {
            custom_font_bytes: None,
        }
    }

    /// Loads a font from `fonts/<filename>` on disk, falling back to embedded on error.
    pub fn from_font_path(filename: &str) -> Self {
        let path = format!("fonts/{}", filename);
        match std::fs::read(&path) {
            Ok(bytes) => {
                let boxed: Box<[u8]> = bytes.into_boxed_slice();
                // Validate parseable before storing
                if Font::try_from_bytes(&boxed).is_some() {
                    Self {
                        custom_font_bytes: Some(boxed),
                    }
                } else {
                    tracing::warn!(
                        "Font '{}' could not be parsed by rusttype, using embedded fallback.",
                        path
                    );
                    Self {
                        custom_font_bytes: None,
                    }
                }
            }
            Err(_) => {
                tracing::warn!(
                    "Font '{}' not found on disk, using embedded fallback.",
                    path
                );
                Self {
                    custom_font_bytes: None,
                }
            }
        }
    }

    pub fn render_text(
        &self,
        matrix: &mut dyn MatrixBackend,
        text: &str,
        theme_id: i32,
        size: u32,
        offset_x: i32,
        offset_y: i32,
        color1_override: Option<(u8, u8, u8)>,
        color2_override: Option<(u8, u8, u8)>,
    ) {
        let theme = get_theme_info(theme_id);
        let primary = color1_override.unwrap_or(theme.primary_color);
        let secondary = color2_override.unwrap_or(theme.secondary_color);

        // Build Font on the fly from the stored bytes slice, or use the embedded static ref
        let font_owned: Option<Font<'_>>;
        let font: &Font<'_> = match &self.custom_font_bytes {
            Some(bytes) => {
                font_owned = Font::try_from_bytes(bytes.as_ref());
                match font_owned.as_ref() {
                    Some(f) => f,
                    None => get_embedded_font(),
                }
            }
            None => get_embedded_font(),
        };

        let scale = Scale::uniform(8.0 * size as f32);
        let v_metrics = font.v_metrics(scale);

        let glyphs: Vec<_> = font
            .layout(text, scale, rusttype::point(0.0, v_metrics.ascent))
            .collect();
        let text_width = glyphs
            .iter()
            .rev()
            .next()
            .map(|g| g.position().x + g.unpositioned().h_metrics().advance_width)
            .unwrap_or(0.0) as i32;
        let text_height = (v_metrics.ascent - v_metrics.descent) as i32;

        let start_x = (matrix.width() as i32 - text_width) / 2 + offset_x;
        let start_y = (matrix.height() as i32 - text_height) / 2 + offset_y;

        // Determine 3D shadow depth based on matrix width
        let shadow_depth = (matrix.width() as i32 / 64).max(1);
        
        // Nintendo (0), Capcom (1), Sega (3) are NOT in 3D, they just use a flat outline
        let is_3d = !(theme_id == 0 || theme_id == 1 || theme_id == 3);

        // Render secondary color (outline or 3D shadow)
        if secondary != (0, 0, 0) {
            for glyph in &glyphs {
                if let Some(bb) = glyph.pixel_bounding_box() {
                    glyph.draw(|x, y, v| {
                        if v > 0.5 {
                            if is_3d {
                                // 3D extrusion
                                for d in 1..=shadow_depth {
                                    let px = start_x + bb.min.x + x as i32 + d;
                                    let py = start_y + bb.min.y + y as i32 + d;
                                    matrix.set_pixel(px, py, secondary.0, secondary.1, secondary.2);
                                }
                            } else {
                                // Flat 1px outline
                                let px = start_x + bb.min.x + x as i32;
                                let py = start_y + bb.min.y + y as i32;
                                matrix.set_pixel(px - 1, py, secondary.0, secondary.1, secondary.2);
                                matrix.set_pixel(px + 1, py, secondary.0, secondary.1, secondary.2);
                                matrix.set_pixel(px, py - 1, secondary.0, secondary.1, secondary.2);
                                matrix.set_pixel(px, py + 1, secondary.0, secondary.1, secondary.2);
                            }
                        }
                    });
                }
            }
        }

        // Render main text with primary color
        for glyph in &glyphs {
            if let Some(bb) = glyph.pixel_bounding_box() {
                glyph.draw(|x, y, v| {
                    if v > 0.5 {
                        let px = start_x + bb.min.x + x as i32;
                        let py = start_y + bb.min.y + y as i32;
                        matrix.set_pixel(px, py, primary.0, primary.1, primary.2);
                    }
                });
            }
        }
    }
}

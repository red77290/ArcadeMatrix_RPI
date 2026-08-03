use crate::core::matrix::MatrixBackend;
use crate::core::theme::get_theme_info;
use bdf_parser::BdfFont;
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

pub enum ArcadeFont<'a> {
    Ttf(Font<'a>),
    Bdf(&'a BdfFont),
}

impl<'a> ArcadeFont<'a> {
    pub fn get_pixel_map(&self, text: &str, size: f32) -> (Vec<Vec<(i32, i32)>>, i32, i32) {
        match self {
            ArcadeFont::Ttf(font) => {
                let scale = Scale::uniform(8.0 * size);
                let v_metrics = font.v_metrics(scale);
                let glyphs: Vec<_> = font
                    .layout(text, scale, rusttype::point(0.0, v_metrics.ascent))
                    .collect();
                let mut pixels_by_char = Vec::new();
                let mut max_x = 0;
                for glyph in &glyphs {
                    let mut char_pixels = Vec::new();
                    if let Some(bb) = glyph.pixel_bounding_box() {
                        glyph.draw(|gx, gy, v| {
                            if v > 0.5 {
                                let px = bb.min.x + gx as i32;
                                let py = bb.min.y + gy as i32;
                                char_pixels.push((px, py));
                            }
                        });
                        max_x = max_x.max(bb.max.x);
                    }
                    pixels_by_char.push(char_pixels);
                }
                (
                    pixels_by_char,
                    max_x,
                    (v_metrics.ascent - v_metrics.descent) as i32,
                )
            }
            ArcadeFont::Bdf(font) => {
                let mut pixels_by_char = Vec::new();
                let mut cur_x = 0;
                let scale_int = size.max(1.0) as i32;

                let global_bb = font.metadata.bounding_box.size;
                let ascent = font
                    .properties
                    .try_get(bdf_parser::Property::FontAscent)
                    .unwrap_or(global_bb.y);
                let descent = font
                    .properties
                    .try_get(bdf_parser::Property::FontDescent)
                    .unwrap_or(0);

                for c in text.chars() {
                    let mut char_pixels = Vec::new();
                    if let Some(glyph) = font.glyphs.get(c) {
                        let bb = glyph.bounding_box;
                        // In BDF, Y offsets are from baseline up. So top of bounding box is baseline - offset.y - size.y
                        let top_y = ascent - (bb.offset.y + bb.size.y);
                        let get_px = |gx: i32, gy: i32| -> bool {
                            if gx < 0 || gy < 0 || gx >= bb.size.x as i32 || gy >= bb.size.y as i32
                            {
                                false
                            } else {
                                glyph.pixel(gx as usize, gy as usize)
                            }
                        };

                        // Process pixels slightly outside the bounding box too, as Scale2x/3x can expand into empty space
                        for y in -1..=(bb.size.y as i32) {
                            for x in -1..=(bb.size.x as i32) {
                                let px = cur_x + bb.offset.x + x;
                                let py = top_y + y;

                                if get_px(x, y) {
                                    for sy in 0..scale_int {
                                        for sx in 0..scale_int {
                                            char_pixels
                                                .push((px * scale_int + sx, py * scale_int + sy));
                                        }
                                    }
                                }
                            }
                        }
                        cur_x += glyph.device_width.x;
                    } else if c == ' ' {
                        cur_x += global_bb.x / 2;
                    }
                    pixels_by_char.push(char_pixels);
                }
                (
                    pixels_by_char,
                    cur_x * scale_int,
                    (ascent + descent) * scale_int,
                )
            }
        }
    }
}

pub struct BaseRenderer {
    /// Loaded TTF font bytes
    custom_font_bytes: Option<Box<[u8]>>,
    /// Loaded BDF font
    custom_bdf_font: Option<BdfFont>,
}

impl BaseRenderer {
    /// Uses the embedded PressStart2P font.
    pub fn new() -> Self {
        Self {
            custom_font_bytes: None,
            custom_bdf_font: None,
        }
    }

    pub fn from_font_path(filename: &str) -> Self {
        let path = format!("fonts/{}", filename);
        match std::fs::read(&path) {
            Ok(bytes) => {
                if filename.to_lowercase().ends_with(".bdf") {
                    if let Ok(bdf) = BdfFont::parse(&bytes) {
                        return Self {
                            custom_font_bytes: None,
                            custom_bdf_font: Some(bdf),
                        };
                    } else {
                        tracing::warn!("Failed to parse BDF '{}', using embedded fallback.", path);
                    }
                } else {
                    let boxed: Box<[u8]> = bytes.into_boxed_slice();
                    if Font::try_from_bytes(&boxed).is_some() {
                        return Self {
                            custom_font_bytes: Some(boxed),
                            custom_bdf_font: None,
                        };
                    } else {
                        tracing::warn!(
                            "Font '{}' could not be parsed by rusttype, using embedded fallback.",
                            path
                        );
                    }
                }
            }
            Err(_) => tracing::warn!(
                "Font '{}' not found on disk, using embedded fallback.",
                path
            ),
        }
        Self {
            custom_font_bytes: None,
            custom_bdf_font: None,
        }
    }

    /// Returns the active ArcadeFont, falling back to embedded TTF if needed.
    pub fn font(&self) -> ArcadeFont<'_> {
        if let Some(bdf) = &self.custom_bdf_font {
            return ArcadeFont::Bdf(bdf);
        }
        match &self.custom_font_bytes {
            Some(bytes) => ArcadeFont::Ttf(
                Font::try_from_bytes(bytes.as_ref()).unwrap_or_else(|| get_embedded_font().clone()),
            ),
            None => ArcadeFont::Ttf(get_embedded_font().clone()),
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

        let font_owned: Option<Font<'_>>;
        let font = if let Some(bdf) = &self.custom_bdf_font {
            ArcadeFont::Bdf(bdf)
        } else {
            match &self.custom_font_bytes {
                Some(bytes) => {
                    font_owned = Font::try_from_bytes(bytes.as_ref());
                    ArcadeFont::Ttf(
                        font_owned
                            .as_ref()
                            .unwrap_or_else(|| get_embedded_font())
                            .clone(),
                    )
                }
                None => ArcadeFont::Ttf(get_embedded_font().clone()),
            }
        };

        let (pixels_by_char, text_width, text_height) = font.get_pixel_map(text, size as f32);

        let start_x = (matrix.width() as i32 - text_width) / 2 + offset_x;
        let start_y = (matrix.height() as i32 - text_height) / 2 + offset_y;

        let offset = (size as i32).max(1);

        if secondary != (0, 0, 0) {
            for char_pixels in &pixels_by_char {
                for &(gx, gy) in char_pixels {
                    let px = start_x + gx;
                    let py = start_y + gy;

                    if theme_id >= 4 && theme_id <= 17 {
                        // Arcade 3D Outline Effect, scaled by offset
                        // Draw colored drop shadow
                        for i in 1..=(offset * 2) {
                            matrix.set_pixel(
                                px + i,
                                py + (offset * 2),
                                secondary.0,
                                secondary.1,
                                secondary.2,
                            );
                            matrix.set_pixel(
                                px + (offset * 2),
                                py + i,
                                secondary.0,
                                secondary.1,
                                secondary.2,
                            );
                        }
                        // Black outline on edges
                        for i in 1..=offset {
                            matrix.set_pixel(px - i, py, 0, 0, 0);
                            matrix.set_pixel(px + i, py, 0, 0, 0);
                            matrix.set_pixel(px, py - i, 0, 0, 0);
                            matrix.set_pixel(px, py + i, 0, 0, 0);

                            // Black outline around the shadow itself
                            matrix.set_pixel(px + (offset * 2) + i, py + (offset * 2), 0, 0, 0);
                            matrix.set_pixel(px + (offset * 2), py + (offset * 2) + i, 0, 0, 0);
                        }
                    } else if theme_id == 0 || theme_id == 1 || theme_id == 3 {
                        // Normal Outline (8-way solid outline)
                        for i in 1..=offset {
                            matrix.set_pixel(px + i, py, secondary.0, secondary.1, secondary.2);
                            matrix.set_pixel(px - i, py, secondary.0, secondary.1, secondary.2);
                            matrix.set_pixel(px, py + i, secondary.0, secondary.1, secondary.2);
                            matrix.set_pixel(px, py - i, secondary.0, secondary.1, secondary.2);
                            matrix.set_pixel(px + i, py + i, secondary.0, secondary.1, secondary.2);
                            matrix.set_pixel(px - i, py - i, secondary.0, secondary.1, secondary.2);
                            matrix.set_pixel(px + i, py - i, secondary.0, secondary.1, secondary.2);
                            matrix.set_pixel(px - i, py + i, secondary.0, secondary.1, secondary.2);
                        }
                    } else if theme_id != 19 {
                        // Drop shadow (scaled and solid)
                        for i in 1..=offset {
                            matrix.set_pixel(px + i, py + i, secondary.0, secondary.1, secondary.2);
                        }
                    }
                }
            }
        }

        for char_pixels in &pixels_by_char {
            for &(gx, gy) in char_pixels {
                matrix.set_pixel(start_x + gx, start_y + gy, primary.0, primary.1, primary.2);
            }
        }
    }

    pub fn draw_text_at(
        matrix: &mut dyn MatrixBackend,
        text: &str,
        font: &ArcadeFont<'_>,
        size: f32,
        x: i32,
        y: i32,
        primary: (u8, u8, u8),
        secondary: (u8, u8, u8),
    ) {
        let (pixels_by_char, _, _) = font.get_pixel_map(text, size);

        let offset = (size as i32).max(1);

        if secondary != (0, 0, 0) {
            for char_pixels in &pixels_by_char {
                for &(gx, gy) in char_pixels {
                    let px = x + gx;
                    let py = y + gy;
                    for i in 1..=offset {
                        matrix.set_pixel(px - i, py, secondary.0, secondary.1, secondary.2);
                        matrix.set_pixel(px + i, py, secondary.0, secondary.1, secondary.2);
                        matrix.set_pixel(px, py - i, secondary.0, secondary.1, secondary.2);
                        matrix.set_pixel(px, py + i, secondary.0, secondary.1, secondary.2);
                        matrix.set_pixel(px + i, py + i, secondary.0, secondary.1, secondary.2);
                        matrix.set_pixel(px - i, py - i, secondary.0, secondary.1, secondary.2);
                        matrix.set_pixel(px + i, py - i, secondary.0, secondary.1, secondary.2);
                        matrix.set_pixel(px - i, py + i, secondary.0, secondary.1, secondary.2);
                    }
                }
            }
        }

        for char_pixels in &pixels_by_char {
            for &(gx, gy) in char_pixels {
                matrix.set_pixel(x + gx, y + gy, primary.0, primary.1, primary.2);
            }
        }
    }
}

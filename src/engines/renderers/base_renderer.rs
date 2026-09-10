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
                    .layout(text, scale, rusttype::point(0.0, v_metrics.ascent.round()))
                    .collect();
                let mut pixels_by_char = Vec::new();
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
                    }
                    pixels_by_char.push(char_pixels);
                }

                // Normalize visual bounding box across all rendered characters
                let mut min_x = i32::MAX;
                let mut max_x = i32::MIN;
                let mut min_y = i32::MAX;
                let mut max_y = i32::MIN;
                let mut has_pixels = false;

                for char_pixels in &pixels_by_char {
                    for &(px, py) in char_pixels {
                        min_x = min_x.min(px);
                        max_x = max_x.max(px);
                        min_y = min_y.min(py);
                        max_y = max_y.max(py);
                        has_pixels = true;
                    }
                }

                if has_pixels {
                    for char_pixels in &mut pixels_by_char {
                        for (px, py) in char_pixels.iter_mut() {
                            *px -= min_x;
                            *py -= min_y;
                        }
                    }
                    let actual_w = max_x - min_x + 1;
                    let actual_h = max_y - min_y + 1;
                    (pixels_by_char, actual_w, actual_h)
                } else {
                    (
                        pixels_by_char,
                        0,
                        (v_metrics.ascent - v_metrics.descent) as i32,
                    )
                }
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

                // Normalize visual bounding box
                let mut min_x = i32::MAX;
                let mut max_x = i32::MIN;
                let mut min_y = i32::MAX;
                let mut max_y = i32::MIN;
                let mut has_pixels = false;

                for char_pixels in &pixels_by_char {
                    for &(px, py) in char_pixels {
                        min_x = min_x.min(px);
                        max_x = max_x.max(px);
                        min_y = min_y.min(py);
                        max_y = max_y.max(py);
                        has_pixels = true;
                    }
                }

                if has_pixels {
                    for char_pixels in &mut pixels_by_char {
                        for (px, py) in char_pixels.iter_mut() {
                            *px -= min_x;
                            *py -= min_y;
                        }
                    }
                    let actual_w = max_x - min_x + 1;
                    let actual_h = max_y - min_y + 1;
                    (pixels_by_char, actual_w, actual_h)
                } else {
                    (
                        pixels_by_char,
                        cur_x * scale_int,
                        (ascent + descent) * scale_int,
                    )
                }
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

    pub fn draw_themed_text_at(
        &self,
        matrix: &mut dyn MatrixBackend,
        text: &str,
        theme_id: i32,
        size: u32,
        start_x: i32,
        start_y: i32,
        color1_override: Option<(u8, u8, u8)>,
        color2_override: Option<(u8, u8, u8)>,
    ) {
        let theme = get_theme_info(theme_id);
        let primary = color1_override.unwrap_or(theme.primary_color);
        let secondary = color2_override.unwrap_or(theme.secondary_color);

        let font = self.font();
        let (pixels_by_char, _, _) = font.get_pixel_map(text, size as f32);

        let is_logo_theme = theme_id == 0 || theme_id == 1 || theme_id == 3; // Nintendo, Capcom, Sega
        let is_3d_theme = theme_id >= 4 && theme_id <= 17;
        let is_flip_theme = theme_id == 19;
        let is_matrix_theme = theme_id == 18 || theme_id == 21;

        let effect_depth = if size >= 5 { 2 } else { 1 };
        let shadow_depth = effect_depth + 1;

        if is_3d_theme {
            // 1. 3D extrusion in secondary color
            for char_pixels in &pixels_by_char {
                for &(gx, gy) in char_pixels {
                    let px = start_x + gx;
                    let py = start_y + gy;
                    for i in 1..=shadow_depth {
                        matrix.set_pixel(px + i, py + i, secondary.0, secondary.1, secondary.2);
                        matrix.set_pixel(px + i - 1, py + i, secondary.0, secondary.1, secondary.2);
                        matrix.set_pixel(px + i, py + i - 1, secondary.0, secondary.1, secondary.2);
                    }
                }
            }

            // 2. 8-way black carve-out outline (distance 1) separating face from 3D shadow
            for char_pixels in &pixels_by_char {
                for &(gx, gy) in char_pixels {
                    let px = start_x + gx;
                    let py = start_y + gy;
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            if dx != 0 || dy != 0 {
                                matrix.set_pixel(px + dx, py + dy, 0, 0, 0);
                            }
                        }
                    }
                }
            }
        } else if is_logo_theme {
            // 4-way outline
            for char_pixels in &pixels_by_char {
                for &(gx, gy) in char_pixels {
                    let px = start_x + gx;
                    let py = start_y + gy;
                    for i in 1..=effect_depth {
                        matrix.set_pixel(px + i, py, secondary.0, secondary.1, secondary.2);
                        matrix.set_pixel(px - i, py, secondary.0, secondary.1, secondary.2);
                        matrix.set_pixel(px, py + i, secondary.0, secondary.1, secondary.2);
                        matrix.set_pixel(px, py - i, secondary.0, secondary.1, secondary.2);
                    }
                }
            }
        } else if is_matrix_theme {
            // 4-way crisp black halo around text
            for char_pixels in &pixels_by_char {
                for &(gx, gy) in char_pixels {
                    let px = start_x + gx;
                    let py = start_y + gy;
                    matrix.set_pixel(px + 1, py, 0, 0, 0);
                    matrix.set_pixel(px - 1, py, 0, 0, 0);
                    matrix.set_pixel(px, py + 1, 0, 0, 0);
                    matrix.set_pixel(px, py - 1, 0, 0, 0);
                }
            }
        } else if !is_flip_theme {
            for char_pixels in &pixels_by_char {
                for &(gx, gy) in char_pixels {
                    let px = start_x + gx;
                    let py = start_y + gy;
                    matrix.set_pixel(
                        px + effect_depth,
                        py + effect_depth,
                        secondary.0,
                        secondary.1,
                        secondary.2,
                    );
                    matrix.set_pixel(
                        px + effect_depth - 1,
                        py + effect_depth,
                        secondary.0,
                        secondary.1,
                        secondary.2,
                    );
                    matrix.set_pixel(
                        px + effect_depth,
                        py + effect_depth - 1,
                        secondary.0,
                        secondary.1,
                        secondary.2,
                    );
                }
            }
        }

        // 3. Primary glyph face on top of everything
        for char_pixels in &pixels_by_char {
            for &(gx, gy) in char_pixels {
                matrix.set_pixel(start_x + gx, start_y + gy, primary.0, primary.1, primary.2);
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
        let font = self.font();
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;

        let mut effective_size = size.max(1);
        while effective_size > 1 {
            let (_, text_width, text_height) = font.get_pixel_map(text, effective_size as f32);
            if text_width <= w && text_height <= h {
                break;
            }
            effective_size -= 1;
        }

        let (_, text_width, text_height) = font.get_pixel_map(text, effective_size as f32);

        let start_x = (w - text_width) / 2 + offset_x;
        let start_y = (h - text_height) / 2 + offset_y;

        self.draw_themed_text_at(
            matrix,
            text,
            theme_id,
            effective_size,
            start_x,
            start_y,
            color1_override,
            color2_override,
        );
    }

    pub fn render_tate_time(
        &self,
        matrix: &mut dyn MatrixBackend,
        hours: u32,
        minutes: u32,
        seconds: u32,
        theme_id: i32,
        custom_size: u32,
        offset_x: i32,
        offset_y: i32,
        color1_override: Option<(u8, u8, u8)>,
        color2_override: Option<(u8, u8, u8)>,
    ) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;

        let font = self.font();
        let h_str = format!("{:02}", hours);
        let m_str = format!("{:02}", minutes);
        let s_str = format!("{:02}", seconds);

        let tier_count = if h >= 128 { 3 } else { 2 };
        let max_tier_h = h / tier_count;

        // Find the maximum scale <= requested size that fits within display bounds
        let target_size = if custom_size > 0 { custom_size } else { 4 };
        let mut scale = target_size;
        while scale > 1 {
            let (_, dw, dh) = font.get_pixel_map(&h_str, scale as f32);
            if dw <= w && dh <= max_tier_h {
                break;
            }
            scale -= 1;
        }

        let (_, h_w, h_h) = font.get_pixel_map(&h_str, scale as f32);
        let (_, m_w, m_h) = font.get_pixel_map(&m_str, scale as f32);
        let (_, s_w, s_h) = font.get_pixel_map(&s_str, scale as f32);

        let draw_x_h = (w - h_w) / 2 + offset_x;
        let draw_x_m = (w - m_w) / 2 + offset_x;
        let draw_x_s = (w - s_w) / 2 + offset_x;

        if h >= 128 {
            // 3 Tiers (Hours, Minutes, Seconds)
            let y_h = (h / 6) - (h_h / 2) + offset_y;
            let y_m = (h / 2) - (m_h / 2) + offset_y;
            let y_s = (5 * h / 6) - (s_h / 2) + offset_y;

            self.draw_themed_text_at(
                matrix,
                &h_str,
                theme_id,
                scale,
                draw_x_h,
                y_h,
                color1_override,
                color2_override,
            );
            self.draw_themed_text_at(
                matrix,
                &m_str,
                theme_id,
                scale,
                draw_x_m,
                y_m,
                color1_override,
                color2_override,
            );

            // Seconds tier (exact same scale, alignment, and color as hours and minutes)
            self.draw_themed_text_at(
                matrix,
                &s_str,
                theme_id,
                scale,
                draw_x_s,
                y_s,
                color1_override,
                color2_override,
            );
        } else {
            // 2 Tiers (Hours top, Minutes bottom, pulsing dots in center)
            let y_h = (h / 4) - (h_h / 2) + offset_y + 2;
            let y_m = (3 * h / 4) - (m_h / 2) + offset_y - 2;

            self.draw_themed_text_at(
                matrix,
                &h_str,
                theme_id,
                scale,
                draw_x_h,
                y_h,
                color1_override,
                color2_override,
            );
            self.draw_themed_text_at(
                matrix,
                &m_str,
                theme_id,
                scale,
                draw_x_m,
                y_m,
                color1_override,
                color2_override,
            );

            // Center Pulsing Colon
            let dot_x = (w / 2) - 1 + offset_x;
            let dot_y1 = (h / 2) - 3 + offset_y;
            let dot_y2 = (h / 2) + 2 + offset_y;
            let theme = get_theme_info(theme_id);
            let primary = color1_override.unwrap_or(theme.primary_color);
            for dy in 0..2 {
                for dx in 0..2 {
                    matrix.set_pixel(dot_x + dx, dot_y1 + dy, primary.0, primary.1, primary.2);
                    matrix.set_pixel(dot_x + dx, dot_y2 + dy, primary.0, primary.1, primary.2);
                }
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

        for char_pixels in &pixels_by_char {
            for &(gx, gy) in char_pixels {
                matrix.set_pixel(x + gx, y + gy, primary.0, primary.1, primary.2);
            }
        }
    }

    pub fn draw_text_clipped(
        matrix: &mut dyn MatrixBackend,
        text: &str,
        font: &ArcadeFont<'_>,
        size: f32,
        x: i32,
        y: i32,
        clip_min_x: i32,
        clip_max_x: i32,
        primary: (u8, u8, u8),
        secondary: (u8, u8, u8),
    ) {
        let (pixels_by_char, _, _) = font.get_pixel_map(text, size);
        let offset = (size as i32).max(1);

        if secondary != (0, 0, 0) && secondary != primary {
            for char_pixels in &pixels_by_char {
                for &(gx, gy) in char_pixels {
                    let px = x + gx;
                    let py = y + gy;
                    for i in 1..=offset {
                        let pts = [
                            (px - i, py),
                            (px + i, py),
                            (px, py - i),
                            (px, py + i),
                            (px + i, py + i),
                            (px - i, py - i),
                            (px + i, py - i),
                            (px - i, py + i),
                        ];
                        for (cx, cy) in pts {
                            if cx >= clip_min_x && cx < clip_max_x {
                                matrix.set_pixel(cx, cy, secondary.0, secondary.1, secondary.2);
                            }
                        }
                    }
                }
            }
        }

        for char_pixels in &pixels_by_char {
            for &(gx, gy) in char_pixels {
                let px = x + gx;
                let py = y + gy;
                if px >= clip_min_x && px < clip_max_x {
                    matrix.set_pixel(px, py, primary.0, primary.1, primary.2);
                }
            }
        }
    }
}

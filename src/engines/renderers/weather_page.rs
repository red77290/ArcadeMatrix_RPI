//! One weather page (a day of the forecast, or the MQTT Data "NOW" page)
//! drawn exactly like the ESP32 firmware's `WeatherEngine::drawForecast`, so
//! both firmwares show the same layout on 128x32 and 256x64 panels. Shared by
//! the OpenWeatherMap weather engine and the MQTT Data engine's weather pages.

use crate::api::DayForecast;

/// Draws `slide` into `img` with its left edge at `base_x` (a slide of the
/// weather panorama, or a whole panel). Panels at least 256x64 get the
/// three-column layout; everything else gets the 128x32 layout.
#[allow(clippy::too_many_arguments)]
pub fn draw_weather_page(
    img: &mut image::RgbaImage,
    slide: &DayForecast,
    base_x: u32,
    mw: u32,
    mh: u32,
    offset_x: i32,
    offset_y: i32,
) {
    // In °F the high goes above the low (US convention): swap the two
    // temperature rows for forecast days. °C keeps low on top.
    let high_on_top = !slide.is_now && slide.temp_max.ends_with("°F");
    let swapped;
    let slide = if high_on_top {
        swapped = DayForecast {
            temp_min: slide.temp_max.clone(),
            temp_max: slide.temp_min.clone(),
            ..slide.clone()
        };
        &swapped
    } else {
        slide
    };

    let color_label = (180, 180, 255);
    let color_desc = (210, 210, 210);
    let color_low = (120, 200, 255);
    let color_high = (255, 150, 50);
    // Top / bottom temperature row colours. The MQTT Data "NOW" page draws the
    // live reading in white and the humidity / wind line in the condition grey.
    let color_morning = if slide.is_now {
        (255, 255, 255)
    } else if high_on_top {
        color_high
    } else {
        color_low
    };
    let color_afternoon = if slide.is_now {
        color_desc
    } else if high_on_top {
        color_low
    } else {
        color_high
    };

    if mw >= 256 && mh >= 64 {
        // --- 256x64: three columns, as the ESP32 `drawForecast` ---
        let margin = 8;
        let icon_x = base_x as i32 + margin + offset_x;
        let row1 = slide.temp_min.as_str();
        let row2 = now_row2(
            slide,
            base_x as i32 + mw as i32 - margin + offset_x,
            icon_x + 48 + margin,
            2,
        );
        let icon_y = (mh as i32 - 48) / 2 + offset_y;
        draw_icon_scaled(img, &slide.icon, icon_x, icon_y, 2);

        let row_top = 10 + offset_y;
        let row_bottom = 38 + offset_y;
        let right_edge = base_x as i32 + mw as i32 - margin + offset_x;
        let temp_w = if slide.is_now {
            glcd_w(row1, 2)
        } else {
            glcd_w(row1, 2).max(glcd_w(row2, 2))
        };
        let mut canvas = SlideCanvas::new(img, base_x as i32, mw as i32);
        canvas.text(
            row1,
            right_edge - glcd_w(row1, 2),
            row_top,
            2,
            color_morning,
        );
        canvas.text(
            row2,
            right_edge - glcd_w(row2, 2),
            row_bottom,
            2,
            color_afternoon,
        );

        let mid_x = icon_x + 48 + margin;
        let mid_w = (right_edge - temp_w - margin) - mid_x;
        let mut label = if slide.label_long.is_empty() {
            slide.label.as_str()
        } else {
            slide.label_long.as_str()
        };
        if glcd_w(label, 2) > mid_w {
            label = &slide.label;
        }
        canvas.text(label, mid_x, row_top, 2, color_label);

        let mut desc = if slide.condition_long.is_empty() {
            slide.condition.as_str()
        } else {
            slide.condition_long.as_str()
        };
        if !desc.is_empty() {
            let size = if glcd_w(desc, 2) <= mid_w { 2 } else { 1 };
            if size == 1 && glcd_w(desc, 1) > mid_w {
                desc = &slide.condition;
            }
            let y = if size == 2 {
                row_bottom
            } else {
                row_bottom + 4
            };
            canvas.text(desc, mid_x, y, size, color_desc);
        }
    } else {
        // --- 128x32: icon | day + condition | temps right-aligned ---
        let icon_x = base_x as i32 + 4 + offset_x;
        let row1 = slide.temp_min.as_str();
        let row2 = now_row2(
            slide,
            base_x as i32 + mw as i32 - 4 + offset_x,
            icon_x + 24 + 4,
            1,
        );
        let icon_y = (mh as i32 - 24) / 2 + offset_y;
        draw_icon_scaled(img, &slide.icon, icon_x, icon_y, 1);

        let (y1, y2) = (4 + offset_y, 18 + offset_y);
        let right_edge = base_x as i32 + mw as i32 - 4 + offset_x;
        let mut canvas = SlideCanvas::new(img, base_x as i32, mw as i32);
        canvas.text(row1, right_edge - glcd_w(row1, 1), y1, 1, color_morning);
        canvas.text(row2, right_edge - glcd_w(row2, 1), y2, 1, color_afternoon);

        let mid_x = icon_x + 24 + 4;
        let temp_w = if slide.is_now {
            glcd_w(row1, 1)
        } else {
            glcd_w(row1, 1).max(glcd_w(row2, 1))
        };
        let mid_w = right_edge - temp_w - 4 - mid_x;
        let label = fit_long_short(&slide.label_long, &slide.label, mid_w);
        canvas.text(&label, mid_x, y1, 1, color_label);
        if !slide.is_now && !slide.condition.is_empty() {
            let desc = fit_long_short(&slide.condition_long, &slide.condition, mid_w);
            canvas.text(&desc, mid_x, y2, 1, color_desc);
        }
    }
}

/// The 24x24 weather icon (same design as the ESP32 `drawIcon`) at an
/// integer scale: coordinates and radii are multiplied, and a 1 px line
/// becomes `scale` parallel lines offset in x.
pub fn draw_icon_scaled(img: &mut image::RgbaImage, icon: &str, x: i32, y: i32, scale: i32) {
    use image::Rgba;
    use imageproc::drawing::{draw_filled_circle_mut, draw_filled_rect_mut, draw_line_segment_mut};
    use imageproc::rect::Rect;

    let s = scale.max(1);
    let px = |v: i32| x + v * s;
    let py = |v: i32| y + v * s;
    let circle = |img: &mut image::RgbaImage, cx: i32, cy: i32, r: i32, c: Rgba<u8>| {
        draw_filled_circle_mut(img, (px(cx), py(cy)), r * s, c);
    };
    let rect = |img: &mut image::RgbaImage, rx: i32, ry: i32, w: u32, h: u32, c: Rgba<u8>| {
        draw_filled_rect_mut(
            img,
            Rect::at(px(rx), py(ry)).of_size(w * s as u32, h * s as u32),
            c,
        );
    };
    let line = |img: &mut image::RgbaImage, x0: i32, y0: i32, x1: i32, y1: i32, c: Rgba<u8>| {
        for o in 0..s {
            draw_line_segment_mut(
                img,
                ((px(x0) + o) as f32, py(y0) as f32),
                ((px(x1) + o) as f32, py(y1) as f32),
                c,
            );
        }
    };

    let yellow = Rgba([255, 255, 0, 255]);
    let dark_yellow = Rgba([255, 200, 0, 255]);
    let light_grey = Rgba([200, 200, 200, 255]);
    let dark_grey = Rgba([150, 150, 150, 255]);
    let thunder_grey = Rgba([100, 100, 100, 255]);
    let blue = Rgba([0, 150, 255, 255]);
    let white = Rgba([255, 255, 255, 255]);
    let green = Rgba([0, 255, 0, 255]);

    if icon.contains("01") {
        // Sun
        circle(img, 12, 12, 6, yellow);
        line(img, 12, 2, 12, 4, dark_yellow);
        line(img, 12, 20, 12, 22, dark_yellow);
        line(img, 2, 12, 4, 12, dark_yellow);
        line(img, 20, 12, 22, 12, dark_yellow);
        line(img, 5, 5, 7, 7, dark_yellow);
        line(img, 19, 19, 17, 17, dark_yellow);
        line(img, 19, 5, 17, 7, dark_yellow);
        line(img, 5, 19, 7, 17, dark_yellow);
    } else if icon.contains("02") || icon.contains("03") || icon.contains("04") {
        // Clouds
        if icon.contains("02") {
            // Sun behind cloud
            circle(img, 8, 8, 4, yellow);
        }
        circle(img, 8, 14, 5, light_grey);
        circle(img, 14, 11, 6, white);
        circle(img, 20, 14, 5, light_grey);
        rect(img, 8, 14, 12, 6, light_grey);
    } else if icon.contains("09") || icon.contains("10") {
        // Rain
        circle(img, 8, 10, 5, dark_grey);
        circle(img, 14, 8, 6, light_grey);
        circle(img, 20, 10, 5, dark_grey);
        rect(img, 8, 10, 12, 6, dark_grey);
        line(img, 8, 18, 6, 22, blue);
        line(img, 14, 18, 12, 22, blue);
        line(img, 20, 18, 18, 22, blue);
    } else if icon.contains("11") {
        // Thunder
        circle(img, 8, 10, 5, thunder_grey);
        circle(img, 14, 8, 6, dark_grey);
        circle(img, 20, 10, 5, thunder_grey);
        rect(img, 8, 10, 12, 6, thunder_grey);
        line(img, 14, 16, 10, 20, yellow);
        line(img, 10, 20, 16, 20, yellow);
        line(img, 16, 20, 12, 24, yellow);
    } else if icon.contains("13") {
        // Snow
        circle(img, 14, 14, 2, white);
        line(img, 14, 8, 14, 20, white);
        line(img, 8, 14, 20, 14, white);
        line(img, 10, 10, 18, 18, white);
        line(img, 18, 10, 10, 18, white);
    } else {
        // Unknown
        circle(img, 12, 12, 6, green);
    }
}

/// Width of `text` in the GLCD 5x7 font at `size`, as the ESP32 computes it
/// (`len*6*size - size`, 0 for empty text).
fn glcd_w(text: &str, size: i32) -> i32 {
    let n = text.chars().count() as i32;
    if n == 0 {
        0
    } else {
        n * 6 * size - size
    }
}

/// Row 2 text: on the NOW page the wind direction is dropped first when the
/// right-aligned line would start left of the middle column.
fn now_row2<'a>(slide: &'a DayForecast, right_edge: i32, mid_x: i32, size: i32) -> &'a str {
    if slide.is_now
        && !slide.now_line2_short.is_empty()
        && right_edge - glcd_w(&slide.temp_max, size) < mid_x
    {
        &slide.now_line2_short
    } else {
        &slide.temp_max
    }
}

/// The long text if it fits `width` at size 1, else the short one, else the
/// short one truncated ("Partly cl." style, 128x32).
fn fit_long_short(long: &str, short: &str, width: i32) -> String {
    if !long.is_empty() && glcd_w(long, 1) <= width {
        return long.to_string();
    }
    if glcd_w(short, 1) <= width {
        return short.to_string();
    }
    let max_chars = ((width + 1) / 6).max(1) as usize;
    let prefix = |k: usize| -> &str {
        match short.char_indices().nth(k) {
            Some((i, _)) => &short[..i],
            None => short,
        }
    };
    if max_chars > 3 {
        format!("{}.", prefix(max_chars - 1))
    } else {
        prefix(max_chars).to_string()
    }
}

/// Draws GLCD text into one slide of the weather panorama, clipped to that slide.
struct SlideCanvas<'a> {
    img: &'a mut image::RgbaImage,
    x0: i32,
    w: i32,
}

impl<'a> SlideCanvas<'a> {
    fn new(img: &'a mut image::RgbaImage, x0: i32, w: i32) -> Self {
        Self { img, x0, w }
    }

    fn text(&mut self, text: &str, x: i32, y: i32, size: i32, c: (u8, u8, u8)) {
        let (x0, x1) = (self.x0, self.x0 + self.w);
        let h = self.img.height() as i32;
        crate::engines::dashboard::font::draw_text_scaled(self, text, x, y, x0, x1, 0, h, size, c);
    }
}

impl crate::core::matrix::IDrawingSurface for SlideCanvas<'_> {
    fn width(&self) -> u32 {
        self.img.width()
    }
    fn height(&self) -> u32 {
        self.img.height()
    }
    fn physical_width(&self) -> u32 {
        self.img.width()
    }
    fn physical_height(&self) -> u32 {
        self.img.height()
    }
    fn set_rotation(&mut self, _rotation: u8) {}
    fn rotation(&self) -> u8 {
        0
    }
    fn set_pixel(&mut self, x: i32, y: i32, r: u8, g: u8, b: u8) {
        if x >= 0 && y >= 0 && (x as u32) < self.img.width() && (y as u32) < self.img.height() {
            self.img
                .put_pixel(x as u32, y as u32, image::Rgba([r, g, b, 255]));
        }
    }
    fn clear(&mut self) {}
    fn present(&mut self) {}
    fn set_brightness(&mut self, _brightness: u8) {}
    fn mark_dirty(&mut self) {}
    fn is_dirty(&self) -> bool {
        true
    }
}

//! Time in words, adapted from the 64x64 "Clockwise" clockface cw-cf-0x02.
//!
//! The hour on one line and the minutes under it, a rule beneath, then the date. Wording and date
//! order come from the localisation layer, so the face follows the language the sign is set to.
//! Laid out for a wide panel; smaller ones get the notice. Matches the ESP32 face.

use crate::core::matrix::IDrawingSurface;
use crate::engines::renderers::base_renderer::ArcadeFont;
use crate::engines::renderers::BaseRenderer;
use chrono::{Datelike, Local};

const ACCENT: (u8, u8, u8) = (72, 176, 200);

pub struct WordsClock;

impl WordsClock {
    pub fn new() -> Self {
        Self
    }

    pub fn render(
        &mut self,
        matrix: &mut dyn IDrawingSurface,
        hours: u32,
        minutes: u32,
        font: &ArcadeFont<'_>,
        scale: u32,
        lang: &str,
    ) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;
        if w < 192 || h < 64 {
            super::wide_only_notice(matrix, font, scale);
            return;
        }
        matrix.clear();

        let (hour_words, minute_words) = spoken_time(hours, minutes, lang);
        let s = scale.max(1) as f32;

        centre(matrix, &hour_words, font, s, h * 22 / 100, ACCENT, w);
        centre(
            matrix,
            &minute_words,
            font,
            s,
            h * 45 / 100,
            (255, 255, 255),
            w,
        );

        let rule_w = w * 3 / 4;
        let rule_y = h * 63 / 100;
        for x in ((w - rule_w) / 2)..((w - rule_w) / 2 + rule_w) {
            matrix.set_pixel(x, rule_y, 255, 255, 255);
        }

        let now = Local::now();
        let date = date_line(
            now.weekday().num_days_from_sunday() as usize,
            now.month0() as usize,
            now.day() as i32,
            lang,
        );
        centre(matrix, &date, font, s, h * 80 / 100, ACCENT, w);
    }
}

fn centre(
    matrix: &mut dyn IDrawingSurface,
    text: &str,
    font: &ArcadeFont<'_>,
    scale: f32,
    centre_y: i32,
    color: (u8, u8, u8),
    panel_w: i32,
) {
    if text.is_empty() {
        return;
    }
    let (_, tw, th) = font.get_pixel_map(text, scale);
    BaseRenderer::draw_text_at(
        matrix,
        text,
        font,
        scale,
        (panel_w - tw as i32) / 2,
        centre_y - th as i32 / 2,
        color,
        (0, 0, 0),
    );
}

pub use crate::core::i18n::{date_line, spoken_time};

impl Default for WordsClock {
    fn default() -> Self {
        Self::new()
    }
}

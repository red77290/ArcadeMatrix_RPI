//! Handheld-index face, adapted from the 64x64 "Clockwise" clockface cw-cf-0x06.
//!
//! The device sits in the middle of the panel with its edge columns carried to the sides. Inside
//! that square it behaves as the original: the time, a weekday marker, the seconds bar filling
//! across the minute, the blinking lamp and a creature that changes every minute.
//!
//! Laid out for a wide panel; on a small one it says so rather than overlapping itself, which is
//! what the ESP32 face does too.

use crate::core::matrix::IDrawingSurface;
use crate::engines::clocks::cwassets::pokedex::{
    POKEDEX_BG, POKEMON1, POKEMON2, POKEMON3, POKEMON4, POKEMON5, POKEMON6, POKEMON7,
};
use crate::engines::clocks::cwscene;
use crate::engines::renderers::base_renderer::ArcadeFont;
use crate::engines::renderers::BaseRenderer;
use chrono::{Datelike, Local};

const LIGHT_GREEN: u16 = 0x754d;
const DARK_GREEN: u16 = 0x0264;
const DARK_BLUE: u16 = 0x016D;
const LIGHT_BLUE: u16 = 0x24fe;

pub struct PokedexClock {
    sprite: usize,
    last_minute: u32,
}

impl PokedexClock {
    pub fn new() -> Self {
        Self {
            sprite: 0,
            last_minute: 99,
        }
    }

    pub fn render(
        &mut self,
        matrix: &mut dyn IDrawingSurface,
        hours: u32,
        minutes: u32,
        seconds: u32,
        font: &ArcadeFont<'_>,
        scale: u32,
    ) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;
        if w < 192 || h < 64 {
            super::wide_only_notice(matrix, font, scale);
            return;
        }
        let d = cwscene::divisor(h);

        if minutes != self.last_minute {
            self.last_minute = minutes;
            self.sprite = (self.sprite + 1) % 7;
        }

        cwscene::draw_background(matrix, &POKEDEX_BG, w, h);

        let creatures: [&[u16]; 7] = [
            &POKEMON1, &POKEMON2, &POKEMON3, &POKEMON4, &POKEMON5, &POKEMON6, &POKEMON7,
        ];
        cwscene::draw_sprite(matrix, creatures[self.sprite], 16, 16, 8, 21, w, h, 0x0000);

        BaseRenderer::draw_text_at(
            matrix,
            &format!("{}", hours),
            font,
            scale.max(1) as f32,
            cwscene::map_x(35, w, h),
            cwscene::map_y(18, h),
            (255, 255, 255),
            (0, 0, 0),
        );
        BaseRenderer::draw_text_at(
            matrix,
            &format!("{:02}", minutes),
            font,
            scale.max(1) as f32,
            cwscene::map_x(46, w, h),
            cwscene::map_y(26, h),
            (255, 255, 255),
            (0, 0, 0),
        );

        // Weekday marker, two rows of four as in the original.
        let wd = Local::now().weekday().num_days_from_sunday() as i32;
        let x = 36 + (if wd > 3 { wd - 4 } else { wd }) * 6;
        let y = 35 + if wd > 3 { 5 } else { 0 };
        fill(
            matrix,
            cwscene::map_x(x, w, h),
            cwscene::map_y(y, h),
            5 / d.max(1),
            4 / d.max(1),
            DARK_BLUE,
        );

        // Seconds bar across the minute.
        if seconds == 0 {
            fill(
                matrix,
                cwscene::map_x(9, w, h),
                cwscene::map_y(53, h),
                11 / d,
                (5 / d).max(1),
                LIGHT_GREEN,
            );
        } else {
            let len = ((10 * seconds as i32) / 59) + 1;
            fill(
                matrix,
                cwscene::map_x(9, w, h),
                cwscene::map_y(53, h),
                (len / d).max(1),
                (5 / d).max(1),
                DARK_GREEN,
            );
        }

        // Blinking lamp.
        let lamp = if seconds % 2 == 0 { 0x07FF } else { LIGHT_BLUE };
        fill(
            matrix,
            cwscene::map_x(5, w, h),
            cwscene::map_y(4, h),
            2,
            (4 / d).max(1),
            lamp,
        );
        fill(
            matrix,
            cwscene::map_x(4, w, h),
            cwscene::map_y(5, h),
            (4 / d).max(1),
            2,
            lamp,
        );
    }
}

fn fill(matrix: &mut dyn IDrawingSurface, x: i32, y: i32, w: i32, h: i32, c: u16) {
    for yy in y..(y + h) {
        for xx in x..(x + w) {
            cwscene::put(matrix, xx, yy, c);
        }
    }
}

impl Default for PokedexClock {
    fn default() -> Self {
        Self::new()
    }
}

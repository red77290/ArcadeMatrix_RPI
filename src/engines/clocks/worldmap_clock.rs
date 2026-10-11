//! World clock face, adapted from the 64x64 "Clockwise" clockface cw-cf-0x03.
//!
//! The marker stands for the timezone the sign is set to, with the map anchored to put that
//! longitude under it, and the night side dimmed from where the sun actually is. The original could
//! only show 64 px of a 120 px map; on a wide panel it continues and wraps. Matches the ESP32 face.

use crate::core::matrix::IDrawingSurface;
use crate::engines::clocks::cwassets::worldmap::WORLD_MAP;
use crate::engines::clocks::cwscene;
use crate::engines::renderers::base_renderer::ArcadeFont;
use crate::engines::renderers::BaseRenderer;
use chrono::{Local, Timelike, Utc};

const MAP_W: i32 = 120;
const MAP_H: i32 = 56;
const TZ_SIZE: i32 = 5; // pixels per hour of longitude, as in the original
const HOME_COLUMN: i32 = 32; // the marker's column inside the 64 px scene
const REFERENCE_OFFSET: f32 = -3.0; // the source face pinned this longitude under the marker
const MAP_MASK: u16 = 0xF81F;
const MARKER: u16 = 0xF000;

pub struct WorldMapClock;

impl WorldMapClock {
    pub fn new() -> Self {
        Self
    }

    fn column_for_offset(offset_hours: f32) -> i32 {
        let c = HOME_COLUMN + ((offset_hours - REFERENCE_OFFSET) * TZ_SIZE as f32).round() as i32;
        c.rem_euclid(MAP_W)
    }

    pub fn render(
        &mut self,
        matrix: &mut dyn IDrawingSurface,
        hours: u32,
        minutes: u32,
        font: &ArcadeFont<'_>,
        scale: u32,
    ) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;
        let d = cwscene::divisor(h);
        let ox = cwscene::origin_x(w, h);
        let oy = cwscene::origin_y(h);

        // Offset of the configured timezone, taken from the running clock.
        let local = Local::now();
        let utc_offset = local.offset().local_minus_utc() as f32 / 3600.0;
        let offset = (Self::column_for_offset(utc_offset) - HOME_COLUMN).rem_euclid(MAP_W);

        // Where the sun is: the longitude whose local time is noon.
        let utc = Utc::now();
        let utc_hours = utc.hour() as f32 + utc.minute() as f32 / 60.0;
        let sun_col = Self::column_for_offset(12.0 - utc_hours);

        for py in 0..(MAP_H / d) {
            let sy = py * d;
            for px in 0..w {
                let scene_x = (px - ox) * d;
                let col = (offset + scene_x).rem_euclid(MAP_W);
                let mut c = WORLD_MAP[(sy * MAP_W + col) as usize];
                if c == MAP_MASK {
                    continue;
                }
                let mut away = (col - sun_col).abs();
                if away > MAP_W / 2 {
                    away = MAP_W - away;
                }
                if away > 6 * TZ_SIZE {
                    // night side: the same picture, dimmed
                    let r = (c >> 11) & 0x1F;
                    let g = (c >> 5) & 0x3F;
                    let b = c & 0x1F;
                    c = ((r / 3) << 11) | ((g / 3) << 5) | (b / 3);
                }
                cwscene::put(matrix, px, oy + py, c);
            }
        }

        let marker_x = cwscene::map_x(HOME_COLUMN, w, h);
        for y in 0..(64 / d) {
            cwscene::put(matrix, marker_x, oy + y, MARKER);
        }

        let text = format!("{}:{:02}", hours, minutes);
        BaseRenderer::draw_text_at(
            matrix,
            &text,
            font,
            scale.max(1) as f32,
            cwscene::map_x(1, w, h),
            cwscene::map_y(58, h),
            (255, 255, 255),
            (0, 0, 0),
        );
    }
}

impl Default for WorldMapClock {
    fn default() -> Self {
        Self::new()
    }
}

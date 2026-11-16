//! Shared zero-duplication sprite bank and clipped RGB565 blitter.
//!
//! References static pixel slices from the Clockwise / arcade clock faces
//! (`cwassets::mario`, `cwassets::sonic`, `cwassets::megaman`, `cwassets::metal_slug`,
//! `cwassets::pokedex`, and `pacsprites`) and provides 12x12 pixel-art context icons
//! for `AgendaEngine` and `SyncCelebrationEngine`.

use crate::core::matrix::IDrawingSurface;
use crate::engines::clocks::cwassets::{mario, megaman, metal_slug, pokedex, sonic};
use crate::engines::clocks::pacsprites;

/// Standard RGB565 chroma-key transparent color used across Clockwise assets.
pub const TRANSPARENT_MASK_565: u16 = 0x000E;

/// Context icon and cameo identifiers shared between `AgendaEngine` (`.ics` cache POD)
/// and `FrontendSync` celebrations (100% parity with ESP32 `SpriteId`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpriteId {
    CalendarDefault = 0,
    BirthdayCake = 1,
    CyberCall = 2,
    MedicalHeart = 3,
    FoodPizza = 4,
    SportSonic = 5,
    TravelPlane = 6,
    GamingPacman = 7,
    MetoolSleep = 8,
    GoldCoin = 9,
    SonicRing = 10,
    TrophyCup = 11,
    CrownRecord = 12,
}

impl SpriteId {
    #[inline]
    pub const fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::BirthdayCake,
            2 => Self::CyberCall,
            3 => Self::MedicalHeart,
            4 => Self::FoodPizza,
            5 => Self::SportSonic,
            6 => Self::TravelPlane,
            7 => Self::GamingPacman,
            8 => Self::MetoolSleep,
            9 => Self::GoldCoin,
            10 => Self::SonicRing,
            11 => Self::TrophyCup,
            12 => Self::CrownRecord,
            _ => Self::CalendarDefault,
        }
    }
}

/// Console family for `SyncCelebration` hero cameos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CelebrationHeroFamily {
    NintendoMario,
    SegaSonic,
    CapcomMegaMan,
    SnkMetalSlug,
    ArcadePacMan,
}

impl CelebrationHeroFamily {
    /// Selects the hero cameo family matching the console `system` slug.
    pub fn from_system(system: &str) -> Self {
        let mut buf = [0u8; 32];
        let mut len = 0usize;
        for b in system.trim().bytes() {
            if len >= buf.len() {
                break;
            }
            if b.is_ascii_alphanumeric() {
                buf[len] = b.to_ascii_lowercase();
                len += 1;
            }
        }
        let norm = core::str::from_utf8(&buf[..len]).unwrap_or("");
        if matches!(
            norm,
            "nes"
                | "snes"
                | "sfc"
                | "gb"
                | "gbc"
                | "gba"
                | "n64"
                | "fds"
                | "gamecube"
                | "gc"
                | "wii"
                | "wiiu"
                | "switch"
                | "nds"
                | "3ds"
                | "virtualboy"
                | "gameandwatch"
        ) {
            Self::NintendoMario
        } else if matches!(
            norm,
            "mastersystem"
                | "sms"
                | "megadrive"
                | "genesis"
                | "md"
                | "gamegear"
                | "gg"
                | "sega32x"
                | "32x"
                | "segacd"
                | "megacd"
                | "saturn"
                | "dreamcast"
                | "sg1000"
                | "naomi"
                | "atomiswave"
        ) {
            Self::SegaSonic
        } else if matches!(
            norm,
            "cps1"
                | "cps2"
                | "cps3"
                | "cps"
                | "pcengine"
                | "tg16"
                | "pce"
                | "pcenginecd"
                | "supergrafx"
                | "psx"
                | "ps1"
                | "ps2"
                | "psp"
        ) {
            Self::CapcomMegaMan
        } else if matches!(norm, "neogeo" | "neogeocd" | "ngp" | "ngpc" | "snk") {
            Self::SnkMetalSlug
        } else {
            Self::ArcadePacMan
        }
    }
}

/// Zero-allocation static sprite reference supporting direct RGB565 arrays,
/// palette-indexed arrays, and 1-bit row masks.
#[derive(Clone, Copy, Debug)]
pub struct SpriteFrameRef<'a> {
    pub pixels_565: Option<&'a [u16]>,
    pub palette_565: Option<&'a [u16]>,
    pub indexed_pixels: Option<&'a [u8]>,
    pub row_mask_16: Option<&'a [u16]>,
    pub mask_color_565: u16,
    pub width: i32,
    pub height: i32,
    pub transparent_565: u16,
}

impl<'a> SpriteFrameRef<'a> {
    #[inline]
    pub const fn from_rgb565(
        pixels: &'a [u16],
        width: i32,
        height: i32,
        transparent_565: u16,
    ) -> Self {
        Self {
            pixels_565: Some(pixels),
            palette_565: None,
            indexed_pixels: None,
            row_mask_16: None,
            mask_color_565: 0xFFFF,
            width,
            height,
            transparent_565,
        }
    }

    #[inline]
    pub const fn from_indexed(
        palette: &'a [u16],
        pixels: &'a [u8],
        width: i32,
        height: i32,
    ) -> Self {
        Self {
            pixels_565: None,
            palette_565: Some(palette),
            indexed_pixels: Some(pixels),
            row_mask_16: None,
            mask_color_565: 0xFFFF,
            width,
            height,
            transparent_565: TRANSPARENT_MASK_565,
        }
    }

    #[inline]
    pub const fn from_row_mask(rows: &'a [u16], width: i32, height: i32, color_565: u16) -> Self {
        Self {
            pixels_565: None,
            palette_565: None,
            indexed_pixels: None,
            row_mask_16: Some(rows),
            mask_color_565: color_565,
            width,
            height,
            transparent_565: TRANSPARENT_MASK_565,
        }
    }
}

#[inline]
pub const fn rgb565_to_rgb888(val: u16) -> (u8, u8, u8) {
    let r5 = ((val >> 11) & 0x1F) as u32;
    let g6 = ((val >> 5) & 0x3F) as u32;
    let b5 = (val & 0x1F) as u32;
    (
        ((r5 * 255 + 15) / 31) as u8,
        ((g6 * 255 + 31) / 63) as u8,
        ((b5 * 255 + 15) / 31) as u8,
    )
}

#[inline]
pub const fn rgb888_to_rgb565(r: u8, g: u8, b: u8) -> u16 {
    (((r as u16) & 0xF8) << 8) | (((g as u16) & 0xFC) << 3) | ((b as u16) >> 3)
}

/// Blits a `SpriteFrameRef` onto `surface` at `(x, y)` with clipping to `[clip_min_x..clip_max_x, clip_min_y..clip_max_y)`.
/// Performs zero heap allocations.
#[allow(clippy::too_many_arguments)]
pub fn blit_sprite_clipped(
    surface: &mut dyn IDrawingSurface,
    frame: &SpriteFrameRef<'_>,
    x: i32,
    y: i32,
    flip_h: bool,
    clip_min_x: i32,
    clip_max_x: i32,
    clip_min_y: i32,
    clip_max_y: i32,
) {
    let min_x = clip_min_x.max(0);
    let max_x = clip_max_x.min(surface.width() as i32);
    let min_y = clip_min_y.max(0);
    let max_y = clip_max_y.min(surface.height() as i32);
    if min_x >= max_x || min_y >= max_y || frame.width <= 0 || frame.height <= 0 {
        return;
    }

    let w = frame.width;
    let h = frame.height;

    if let Some(pixels) = frame.pixels_565 {
        for row in 0..h {
            let py = y + row;
            if py < min_y || py >= max_y {
                continue;
            }
            let row_off = (row * w) as usize;
            for col in 0..w {
                let px = x + col;
                if px < min_x || px >= max_x {
                    continue;
                }
                let src_col = if flip_h { w - 1 - col } else { col } as usize;
                if let Some(&c565) = pixels.get(row_off + src_col) {
                    if c565 != frame.transparent_565 {
                        let (r, g, b) = rgb565_to_rgb888(c565);
                        surface.set_pixel(px, py, r, g, b);
                    }
                }
            }
        }
    } else if let (Some(pal), Some(indexed)) = (frame.palette_565, frame.indexed_pixels) {
        for row in 0..h {
            let py = y + row;
            if py < min_y || py >= max_y {
                continue;
            }
            let row_off = (row * w) as usize;
            for col in 0..w {
                let px = x + col;
                if px < min_x || px >= max_x {
                    continue;
                }
                let src_col = if flip_h { w - 1 - col } else { col } as usize;
                let Some(&idx) = indexed.get(row_off + src_col) else {
                    continue;
                };
                if idx == 0 {
                    continue;
                }
                if let Some(&c565) = pal.get(idx as usize) {
                    if c565 != frame.transparent_565 {
                        let (r, g, b) = rgb565_to_rgb888(c565);
                        surface.set_pixel(px, py, r, g, b);
                    }
                }
            }
        }
    } else if let Some(rows) = frame.row_mask_16 {
        let (r, g, b) = rgb565_to_rgb888(frame.mask_color_565);
        for row in 0..h {
            let py = y + row;
            if py < min_y || py >= max_y {
                continue;
            }
            let Some(&mask) = rows.get(row as usize) else {
                continue;
            };
            for col in 0..w {
                let px = x + col;
                if px < min_x || px >= max_x {
                    continue;
                }
                let bit_col = if flip_h { col } else { w - 1 - col };
                if ((mask >> bit_col) & 1) != 0 {
                    surface.set_pixel(px, py, r, g, b);
                }
            }
        }
    }
}

/// Blits a `SpriteFrameRef` onto `surface` at `(x, y)` clipped to full panel bounds.
#[inline]
pub fn blit_sprite(
    surface: &mut dyn IDrawingSurface,
    frame: &SpriteFrameRef<'_>,
    x: i32,
    y: i32,
    flip_h: bool,
) {
    let w = surface.width() as i32;
    let h = surface.height() as i32;
    blit_sprite_clipped(surface, frame, x, y, flip_h, 0, w, 0, h);
}

// ============================================================================
// 12x12 Pixel-Art Context Icons for Agenda & High-Score / RA Cards (RGB565)
// ============================================================================
const K: u16 = TRANSPARENT_MASK_565;
const W: u16 = 0xFFFF; // White
const R: u16 = 0xF800; // Red
const C: u16 = 0x07FF; // Cyan
const Y: u16 = 0xFFE0; // Yellow / Gold
const O: u16 = 0xFD20; // Orange / Amber
const G: u16 = 0x07E0; // Green
const B: u16 = 0x039F; // Blue
const M: u16 = 0xF81F; // Magenta / Pink
const S: u16 = 0x8410; // Silver / Gray
const D: u16 = 0x4208; // Dark Gray
const T: u16 = 0x9A40; // Brown / Crust

#[rustfmt::skip]
pub const ICON_CALENDAR_12X12: [u16; 144] = [
    K, K, S, S, K, K, K, K, S, S, K, K,
    K, R, R, R, R, R, R, R, R, R, R, K,
    K, R, W, W, R, R, R, R, W, W, R, K,
    K, R, R, R, R, R, R, R, R, R, R, K,
    K, W, W, W, W, W, W, W, W, W, W, K,
    K, W, D, D, W, D, D, W, C, C, W, K,
    K, W, D, D, W, D, D, W, C, C, W, K,
    K, W, W, W, W, W, W, W, W, W, W, K,
    K, W, D, D, W, C, C, W, D, D, W, K,
    K, W, D, D, W, C, C, W, D, D, W, K,
    K, W, W, W, W, W, W, W, W, W, W, K,
    K, K, D, D, D, D, D, D, D, D, K, K,
];

#[rustfmt::skip]
pub const ICON_BIRTHDAY_12X12: [u16; 144] = [
    K, K, K, Y, K, K, Y, K, K, Y, K, K,
    K, K, K, O, K, K, O, K, K, O, K, K,
    K, K, K, W, K, K, W, K, K, W, K, K,
    K, K, M, M, M, M, M, M, M, M, M, K,
    K, K, W, W, M, W, W, M, W, W, M, K,
    K, T, T, T, T, T, T, T, T, T, T, K,
    K, M, M, M, M, M, M, M, M, M, M, K,
    K, W, W, W, W, W, W, W, W, W, W, K,
    K, T, T, R, T, T, R, T, T, R, T, K,
    K, T, T, T, T, T, T, T, T, T, T, K,
    S, S, S, S, S, S, S, S, S, S, S, S,
    K, S, S, S, S, S, S, S, S, S, S, K,
];

#[rustfmt::skip]
pub const ICON_CYBER_CALL_12X12: [u16; 144] = [
    K, K, C, C, C, C, C, C, C, C, K, K,
    K, C, D, D, D, D, D, D, D, D, C, K,
    K, C, D, G, G, D, D, C, C, D, C, K,
    K, C, D, G, G, D, D, C, C, D, C, K,
    K, C, D, D, D, D, D, D, D, D, C, K,
    K, C, D, W, W, W, W, W, W, D, C, K,
    K, C, D, D, D, D, D, D, D, D, C, K,
    K, K, C, C, C, C, C, C, C, C, K, K,
    K, K, K, K, S, S, S, S, K, K, K, K,
    K, K, S, S, S, S, S, S, S, S, K, K,
    K, S, S, D, S, S, S, S, D, S, S, K,
    K, K, K, K, K, K, K, K, K, K, K, K,
];

#[rustfmt::skip]
pub const ICON_MEDICAL_12X12: [u16; 144] = [
    K, K, R, R, R, K, K, R, R, R, K, K,
    K, R, R, W, W, R, R, R, R, R, R, K,
    R, R, W, R, R, R, R, R, R, R, R, R,
    R, R, R, R, R, W, W, R, R, R, R, R,
    R, R, R, R, W, W, W, W, R, R, R, R,
    R, R, R, R, R, W, W, R, R, R, R, R,
    K, R, R, R, R, R, R, R, R, R, R, K,
    K, K, R, R, R, R, R, R, R, R, K, K,
    K, K, K, R, R, R, R, R, R, K, K, K,
    K, K, K, K, R, R, R, R, K, K, K, K,
    K, K, K, K, K, R, R, K, K, K, K, K,
    K, K, K, K, K, K, K, K, K, K, K, K,
];

#[rustfmt::skip]
pub const ICON_FOOD_PIZZA_12X12: [u16; 144] = [
    K, K, K, K, T, T, T, T, K, K, K, K,
    K, K, T, T, O, O, O, O, T, T, K, K,
    K, T, O, Y, Y, Y, Y, Y, Y, O, T, K,
    K, K, Y, Y, R, R, Y, Y, Y, Y, K, K,
    K, K, Y, Y, R, R, Y, G, Y, Y, K, K,
    K, K, K, Y, Y, Y, Y, Y, Y, K, K, K,
    K, K, K, Y, R, Y, Y, R, Y, K, K, K,
    K, K, K, K, Y, Y, Y, Y, K, K, K, K,
    K, K, K, K, Y, G, Y, Y, K, K, K, K,
    K, K, K, K, K, Y, Y, K, K, K, K, K,
    K, K, K, K, K, Y, Y, K, K, K, K, K,
    K, K, K, K, K, K, K, K, K, K, K, K,
];

#[rustfmt::skip]
pub const ICON_SPORT_BALL_12X12: [u16; 144] = [
    K, K, K, K, W, W, W, W, K, K, K, K,
    K, K, W, W, D, D, W, W, W, W, K, K,
    K, W, W, D, D, D, D, W, W, D, W, K,
    K, W, W, W, D, D, W, W, D, D, W, K,
    W, D, W, W, W, W, W, W, W, W, W, W,
    W, D, D, W, W, D, D, W, W, W, D, W,
    W, W, W, W, D, D, D, D, W, W, D, W,
    W, W, W, W, W, D, D, W, W, W, W, W,
    K, W, D, D, W, W, W, W, D, D, W, K,
    K, W, W, D, W, W, W, W, D, W, W, K,
    K, K, W, W, W, D, D, W, W, W, K, K,
    K, K, K, K, W, W, W, W, K, K, K, K,
];

#[rustfmt::skip]
pub const ICON_TRAVEL_PLANE_12X12: [u16; 144] = [
    K, K, K, K, K, K, K, K, K, W, W, K,
    K, K, K, K, K, K, K, K, W, W, C, K,
    K, K, K, C, C, K, K, W, W, C, K, K,
    K, K, K, K, C, C, W, W, C, K, K, K,
    K, K, K, K, C, W, W, C, K, K, K, K,
    K, C, K, K, W, W, C, C, K, K, K, K,
    K, C, C, W, W, C, K, C, C, K, K, K,
    K, K, W, W, C, K, K, K, C, C, K, K,
    K, W, W, C, K, K, K, K, K, C, K, K,
    K, W, K, C, C, K, K, K, K, K, K, K,
    K, K, K, K, C, K, K, K, K, K, K, K,
    K, K, K, K, K, K, K, K, K, K, K, K,
];

#[rustfmt::skip]
pub const ICON_TROPHY_12X12: [u16; 144] = [
    K, K, Y, Y, Y, Y, Y, Y, Y, Y, K, K,
    K, Y, Y, W, Y, Y, Y, Y, O, Y, Y, K,
    Y, K, Y, W, Y, Y, Y, Y, O, Y, K, Y,
    Y, K, Y, Y, Y, Y, Y, Y, O, Y, K, Y,
    K, Y, K, Y, Y, Y, Y, O, Y, K, Y, K,
    K, K, K, Y, Y, Y, Y, O, Y, K, K, K,
    K, K, K, K, Y, Y, O, Y, K, K, K, K,
    K, K, K, K, K, Y, O, K, K, K, K, K,
    K, K, K, K, K, Y, O, K, K, K, K, K,
    K, K, K, Y, Y, Y, Y, Y, Y, K, K, K,
    K, K, Y, O, O, O, O, O, O, Y, K, K,
    K, K, K, K, K, K, K, K, K, K, K, K,
];

#[rustfmt::skip]
pub const ICON_CROWN_12X12: [u16; 144] = [
    K, K, K, K, K, K, K, K, K, K, K, K,
    K, Y, K, K, K, Y, Y, K, K, K, Y, K,
    K, Y, Y, K, K, Y, Y, K, K, Y, Y, K,
    K, Y, W, Y, Y, Y, Y, Y, Y, O, Y, K,
    K, Y, Y, R, Y, Y, Y, Y, C, Y, Y, K,
    K, Y, Y, Y, Y, G, G, Y, Y, Y, Y, K,
    K, Y, Y, Y, Y, Y, Y, Y, Y, Y, Y, K,
    K, O, O, O, O, O, O, O, O, O, O, K,
    K, Y, Y, Y, Y, Y, Y, Y, Y, Y, Y, K,
    K, K, K, K, K, K, K, K, K, K, K, K,
    K, K, K, K, K, K, K, K, K, K, K, K,
    K, K, K, K, K, K, K, K, K, K, K, K,
];

// ============================================================================
// Clock Cameo Sprite Accessors (Zero Duplication)
// ============================================================================

/// Returns Mario's small walk-cycle frame (`0..3`) from `cwassets::mario`.
#[inline]
pub fn mario_run_frame(step: usize) -> SpriteFrameRef<'static> {
    match step % 3 {
        0 => SpriteFrameRef::from_rgb565(
            &mario::MARIO_RUN1,
            mario::MARIO_RUN_W[0],
            16,
            mario::SKY_COLOR,
        ),
        1 => SpriteFrameRef::from_rgb565(
            &mario::MARIO_RUN2,
            mario::MARIO_RUN_W[1],
            16,
            mario::SKY_COLOR,
        ),
        _ => SpriteFrameRef::from_rgb565(
            &mario::MARIO_RUN3,
            mario::MARIO_RUN_W[2],
            16,
            mario::SKY_COLOR,
        ),
    }
}

/// Returns Mario's jump frame (`17x16`) from `cwassets::mario`.
#[inline]
pub const fn mario_jump_frame() -> SpriteFrameRef<'static> {
    SpriteFrameRef::from_rgb565(&mario::MARIO_JUMP, 17, 16, mario::SKY_COLOR)
}

/// Returns the 8x8 gold coin sprite from `cwassets::mario`.
#[inline]
pub const fn mario_gold_coin_frame() -> SpriteFrameRef<'static> {
    SpriteFrameRef::from_rgb565(&mario::GOLD_COIN, 8, 8, mario::SKY_COLOR)
}

/// Returns Sonic's SMS 20x24 run frame (`0..4`) from `cwassets::sonic`.
#[inline]
pub fn sonic_sms_run_frame(step: usize) -> SpriteFrameRef<'static> {
    SpriteFrameRef::from_indexed(
        &sonic::SONIC_SMS_IDLE_PAL,
        sonic::SONIC_SMS_RUN_PIXELS[step % 4],
        sonic::SONIC_SMS_RUN_0_W,
        sonic::SONIC_SMS_RUN_0_H,
    )
}

/// Returns Sonic's Mega Drive 32x36 run frame (`0..4`) from `cwassets::sonic`.
#[inline]
pub fn sonic_md_run_frame(step: usize) -> SpriteFrameRef<'static> {
    SpriteFrameRef::from_indexed(
        &sonic::SONIC_MD_IDLE_PAL,
        sonic::SONIC_MD_RUN_PIXELS[step % 4],
        sonic::SONIC_MD_RUN_0_W,
        sonic::SONIC_MD_RUN_0_H,
    )
}

/// Returns Sonic's spin-jump ball 24x24 frame (`0..4`) from `cwassets::sonic`.
#[inline]
pub fn sonic_ball_frame(step: usize) -> SpriteFrameRef<'static> {
    SpriteFrameRef::from_indexed(
        &sonic::SONIC_MD_IDLE_PAL,
        sonic::SONIC_MD_BALL_PIXELS[step % 4],
        sonic::SONIC_MD_BALL_0_W,
        sonic::SONIC_MD_BALL_0_H,
    )
}

/// Returns Sonic's rotating 16x16 gold ring frame (`0..4`) from `cwassets::sonic`.
#[inline]
pub fn sonic_ring_frame(step: usize) -> SpriteFrameRef<'static> {
    let idx = step % 4;
    SpriteFrameRef::from_indexed(
        sonic::SONIC_RING_PALS[idx],
        sonic::SONIC_RING_PIXELS[idx],
        16,
        16,
    )
}

/// Returns Mega Man's run/shoot/jump frame from `cwassets::megaman`.
#[inline]
pub fn megaman_run_frame(step: usize) -> SpriteFrameRef<'static> {
    match step % 4 {
        0 => SpriteFrameRef::from_rgb565(
            &megaman::MEGAMAN_RUN1,
            megaman::MEGAMAN_RUN1_W,
            megaman::MEGAMAN_RUN1_H,
            megaman::MASK,
        ),
        1 => SpriteFrameRef::from_rgb565(
            &megaman::MEGAMAN_RUN_PASS1,
            megaman::MEGAMAN_RUN_PASS_W,
            megaman::MEGAMAN_RUN_PASS_H,
            megaman::MASK,
        ),
        2 => SpriteFrameRef::from_rgb565(
            &megaman::MEGAMAN_RUN2,
            megaman::MEGAMAN_RUN2_W,
            megaman::MEGAMAN_RUN2_H,
            megaman::MASK,
        ),
        _ => SpriteFrameRef::from_rgb565(
            &megaman::MEGAMAN_RUN_PASS2,
            megaman::MEGAMAN_RUN_PASS_W,
            megaman::MEGAMAN_RUN_PASS_H,
            megaman::MASK,
        ),
    }
}

#[inline]
pub const fn megaman_jump_shoot_frame() -> SpriteFrameRef<'static> {
    SpriteFrameRef::from_rgb565(
        &megaman::MEGAMAN_JUMP_SHOOT,
        megaman::MEGAMAN_JUMP_SHOOT_W,
        megaman::MEGAMAN_JUMP_SHOOT_H,
        megaman::MASK,
    )
}

#[inline]
pub const fn megaman_bullet_frame() -> SpriteFrameRef<'static> {
    SpriteFrameRef::from_rgb565(
        &megaman::BUSTER_BULLET,
        megaman::BULLET_W,
        megaman::BULLET_H,
        megaman::MASK,
    )
}

#[inline]
pub const fn megaman_spark_frame() -> SpriteFrameRef<'static> {
    SpriteFrameRef::from_rgb565(
        &megaman::HIT_SPARK,
        megaman::SPARK_W,
        megaman::SPARK_H,
        megaman::MASK,
    )
}

/// Returns the sleeping/peeking Metool (16x13) from `cwassets::megaman`.
#[inline]
pub const fn metool_sleep_frame(peek: bool) -> SpriteFrameRef<'static> {
    if peek {
        SpriteFrameRef::from_rgb565(
            &megaman::METOOL_PEEK,
            megaman::METOOL_W,
            megaman::METOOL_H,
            megaman::MASK,
        )
    } else {
        SpriteFrameRef::from_rgb565(
            &megaman::METOOL_SLEEP,
            megaman::METOOL_W,
            megaman::METOOL_H,
            megaman::MASK,
        )
    }
}

/// Returns Marco Rossi's walk or victory frame from `cwassets::metal_slug`.
#[inline]
pub fn marco_walk_frame(step: usize) -> SpriteFrameRef<'static> {
    if step.is_multiple_of(2) {
        SpriteFrameRef::from_indexed(
            &metal_slug::MARCO_WALK_0_PAL,
            &metal_slug::MARCO_WALK_0_PIXELS,
            metal_slug::MARCO_WALK_0_W,
            metal_slug::MARCO_WALK_0_H,
        )
    } else {
        SpriteFrameRef::from_indexed(
            &metal_slug::MARCO_WALK_1_PAL,
            &metal_slug::MARCO_WALK_1_PIXELS,
            metal_slug::MARCO_WALK_1_W,
            metal_slug::MARCO_WALK_1_H,
        )
    }
}

#[inline]
pub const fn marco_victory_frame() -> SpriteFrameRef<'static> {
    SpriteFrameRef::from_indexed(
        &metal_slug::MARCO_VICTORY_PAL,
        &metal_slug::MARCO_VICTORY_PIXELS,
        metal_slug::MARCO_VICTORY_W,
        metal_slug::MARCO_VICTORY_H,
    )
}

/// Returns Pac-Man's 13x13 mouth frame (`0..4`) from `pacsprites`.
#[inline]
pub fn pacman_frame(step: usize) -> SpriteFrameRef<'static> {
    let rows = match step % 4 {
        0 => &pacsprites::PAC_FRAME_CLOSED[..],
        1 | 3 => &pacsprites::PAC_FRAME_HALF[..],
        _ => &pacsprites::PAC_FRAME_OPEN[..],
    };
    SpriteFrameRef::from_row_mask(
        rows,
        pacsprites::PAC_FRAME_OPEN_COLS,
        pacsprites::PAC_FRAME_OPEN_ROWS,
        0xFFE0,
    )
}

/// Draws a 14x14 frightened blue ghost from `pacsprites` at `(x, y)`.
pub fn draw_frightened_ghost(
    surface: &mut dyn IDrawingSurface,
    x: i32,
    y: i32,
    step: usize,
    flash_white: bool,
) {
    let body_col = if flash_white { 0xFFFF } else { 0x19FF };
    let face_col = if flash_white { 0xF800 } else { 0xFFDF };
    let body_ref = SpriteFrameRef::from_row_mask(
        &pacsprites::GHOST_BODY,
        pacsprites::GHOST_BODY_COLS,
        pacsprites::GHOST_BODY_ROWS,
        body_col,
    );
    let skirt_rows = if step.is_multiple_of(2) {
        &pacsprites::SKIRT_A[..]
    } else {
        &pacsprites::SKIRT_B[..]
    };
    let skirt_ref = SpriteFrameRef::from_row_mask(
        skirt_rows,
        pacsprites::SKIRT_A_COLS,
        pacsprites::SKIRT_A_ROWS,
        body_col,
    );
    let face_ref = SpriteFrameRef::from_row_mask(
        &pacsprites::FRIGHT_FACE,
        pacsprites::FRIGHT_FACE_COLS,
        pacsprites::FRIGHT_FACE_ROWS,
        face_col,
    );
    blit_sprite(surface, &body_ref, x, y, false);
    blit_sprite(surface, &skirt_ref, x, y + 12, false);
    blit_sprite(surface, &face_ref, x, y + 5, false);
}

/// Returns a Pokedex creature (`0..7`, `16x16`) from `cwassets::pokedex`.
#[inline]
pub fn pokedex_creature_frame(index: usize) -> SpriteFrameRef<'static> {
    match index % 7 {
        0 => SpriteFrameRef::from_rgb565(&pokedex::POKEMON1, 16, 16, 0xFF2F),
        1 => SpriteFrameRef::from_rgb565(&pokedex::POKEMON2, 16, 16, 0x76D5),
        2 => SpriteFrameRef::from_rgb565(&pokedex::POKEMON3, 16, 16, 0xED70),
        3 => SpriteFrameRef::from_rgb565(&pokedex::POKEMON4, 16, 16, 0x863A),
        4 => SpriteFrameRef::from_rgb565(&pokedex::POKEMON5, 16, 16, 0x6487),
        5 => SpriteFrameRef::from_rgb565(&pokedex::POKEMON6, 16, 16, 0x069A),
        _ => SpriteFrameRef::from_rgb565(&pokedex::POKEMON7, 16, 16, 0x9930),
    }
}

/// Resolves a `SpriteId` to its `SpriteFrameRef<'static>` at `anim_tick`.
pub fn get_sprite_frame(id: SpriteId, anim_tick: u32) -> SpriteFrameRef<'static> {
    let step = (anim_tick as usize) / 6;
    match id {
        SpriteId::CalendarDefault => {
            SpriteFrameRef::from_rgb565(&ICON_CALENDAR_12X12, 12, 12, TRANSPARENT_MASK_565)
        }
        SpriteId::BirthdayCake => {
            SpriteFrameRef::from_rgb565(&ICON_BIRTHDAY_12X12, 12, 12, TRANSPARENT_MASK_565)
        }
        SpriteId::CyberCall => {
            SpriteFrameRef::from_rgb565(&ICON_CYBER_CALL_12X12, 12, 12, TRANSPARENT_MASK_565)
        }
        SpriteId::MedicalHeart => {
            SpriteFrameRef::from_rgb565(&ICON_MEDICAL_12X12, 12, 12, TRANSPARENT_MASK_565)
        }
        SpriteId::FoodPizza => {
            SpriteFrameRef::from_rgb565(&ICON_FOOD_PIZZA_12X12, 12, 12, TRANSPARENT_MASK_565)
        }
        SpriteId::SportSonic => {
            SpriteFrameRef::from_rgb565(&ICON_SPORT_BALL_12X12, 12, 12, TRANSPARENT_MASK_565)
        }
        SpriteId::TravelPlane => {
            SpriteFrameRef::from_rgb565(&ICON_TRAVEL_PLANE_12X12, 12, 12, TRANSPARENT_MASK_565)
        }
        SpriteId::GamingPacman => pacman_frame(step),
        SpriteId::MetoolSleep => metool_sleep_frame((anim_tick / 90).is_multiple_of(8)),
        SpriteId::GoldCoin => mario_gold_coin_frame(),
        SpriteId::SonicRing => sonic_ring_frame(step),
        SpriteId::TrophyCup => {
            SpriteFrameRef::from_rgb565(&ICON_TROPHY_12X12, 12, 12, TRANSPARENT_MASK_565)
        }
        SpriteId::CrownRecord => {
            SpriteFrameRef::from_rgb565(&ICON_CROWN_12X12, 12, 12, TRANSPARENT_MASK_565)
        }
    }
}

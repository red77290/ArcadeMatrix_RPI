use image::{Rgb, RgbImage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeGroup {
    Publisher,
    Character,
    Effect,
    GameClock,
}

#[derive(Debug, Clone)]
pub struct ThemeInfo {
    pub id: i32,
    pub name: &'static str,
    pub group: ThemeGroup,
    pub primary_color: (u8, u8, u8),
    pub secondary_color: (u8, u8, u8),
    /// Minimum `(width, height)` in landscape orientation (`w >= h`).
    pub min_landscape: (u32, u32),
    /// Minimum `(width, height)` in portrait/Tate orientation (`h > w`), or `None` if portrait is not supported.
    pub min_portrait: Option<(u32, u32)>,
}

impl ThemeInfo {
    /// Checks whether this theme is compatible with the given logical canvas dimensions `(w, h)`.
    pub const fn is_compatible(&self, w: u32, h: u32) -> bool {
        if h > w {
            match self.min_portrait {
                Some((min_w, min_h)) => w >= min_w && h >= min_h,
                None => false,
            }
        } else {
            w >= self.min_landscape.0 && h >= self.min_landscape.1
        }
    }
}

const fn theme(
    id: i32,
    name: &'static str,
    group: ThemeGroup,
    primary_color: (u8, u8, u8),
    secondary_color: (u8, u8, u8),
) -> ThemeInfo {
    ThemeInfo {
        id,
        name,
        group,
        primary_color,
        secondary_color,
        min_landscape: (64, 32),
        min_portrait: Some((32, 64)),
    }
}

const fn theme_with_bounds(
    id: i32,
    name: &'static str,
    group: ThemeGroup,
    primary_color: (u8, u8, u8),
    secondary_color: (u8, u8, u8),
    min_landscape: (u32, u32),
    min_portrait: Option<(u32, u32)>,
) -> ThemeInfo {
    ThemeInfo {
        id,
        name,
        group,
        primary_color,
        secondary_color,
        min_landscape,
        min_portrait,
    }
}

/// Single source of truth for every clock theme.
/// Add a new entry here and it is automatically picked up by the renderer
/// lookup (`get_theme_info`) and exposed to the UI via `all_themes()` and the
/// `/api/themes` endpoint. There is no other list to keep in sync.
pub static THEMES: &[ThemeInfo] = &[
    theme(
        0,
        "Nintendo",
        ThemeGroup::Publisher,
        (228, 0, 15),
        (255, 255, 255),
    ),
    theme(
        1,
        "Capcom",
        ThemeGroup::Publisher,
        (255, 215, 0),
        (0, 75, 175),
    ),
    theme(
        2,
        "Taito",
        ThemeGroup::Publisher,
        (0, 155, 219),
        (255, 255, 255),
    ),
    theme(
        3,
        "Sega",
        ThemeGroup::Publisher,
        (0, 85, 170),
        (255, 255, 255),
    ),
    theme(
        4,
        "Cave",
        ThemeGroup::Publisher,
        (138, 43, 226),
        (255, 255, 0),
    ),
    theme(
        5,
        "Konami",
        ThemeGroup::Publisher,
        (255, 69, 0),
        (255, 255, 255),
    ),
    theme(
        6,
        "SNK",
        ThemeGroup::Publisher,
        (30, 144, 255),
        (255, 215, 0),
    ),
    theme(
        7,
        "Technos",
        ThemeGroup::Publisher,
        (0, 0, 139),
        (255, 255, 255),
    ),
    theme(
        8,
        "IGS",
        ThemeGroup::Publisher,
        (50, 205, 50),
        (255, 215, 0),
    ),
    theme(9, "Hudson", ThemeGroup::Publisher, (255, 255, 0), (0, 0, 0)),
    theme(
        10,
        "Banpresto",
        ThemeGroup::Publisher,
        (255, 0, 0),
        (0, 0, 0),
    ),
    theme(
        11,
        "Namco",
        ThemeGroup::Publisher,
        (255, 0, 0),
        (255, 215, 0),
    ),
    theme(
        12,
        "Ryu (Animated)",
        ThemeGroup::Character,
        (255, 255, 0),
        (255, 0, 0),
    ),
    theme(
        13,
        "Mario (Animated)",
        ThemeGroup::Character,
        (255, 50, 50),
        (255, 255, 255),
    ),
    theme(
        14,
        "Marco (Animated)",
        ThemeGroup::Character,
        (255, 140, 0),
        (0, 100, 0),
    ),
    theme(
        15,
        "Megaman (Animated)",
        ThemeGroup::Character,
        (0, 255, 255),
        (0, 0, 255),
    ),
    theme(
        16,
        "Bub (Animated)",
        ThemeGroup::Character,
        (0, 255, 0),
        (255, 0, 255),
    ),
    theme(
        17,
        "Space Invader (Animated)",
        ThemeGroup::Character,
        (0, 255, 0),
        (0, 0, 0),
    ),
    theme(18, "Cyberpunk", ThemeGroup::Effect, (0, 140, 0), (0, 0, 0)),
    theme(
        19,
        "Flip Clock",
        ThemeGroup::Effect,
        (255, 255, 255),
        (0, 0, 0),
    ),
    theme(
        20,
        "Custom Gradient",
        ThemeGroup::Effect,
        (255, 255, 255),
        (0, 0, 0),
    ),
    theme(
        21,
        "True Matrix",
        ThemeGroup::Effect,
        (0, 140, 0),
        (0, 0, 0),
    ),
    theme(
        22,
        "Pong Clock",
        ThemeGroup::GameClock,
        (255, 255, 255),
        (0, 0, 0),
    ),
    theme(
        23,
        "Tetris Clock",
        ThemeGroup::GameClock,
        (255, 255, 255),
        (0, 0, 0),
    ),
    theme(
        24,
        "Word Clock",
        ThemeGroup::GameClock,
        (255, 255, 255),
        (0, 0, 0),
    ),
    theme(
        25,
        "Binary Clock",
        ThemeGroup::GameClock,
        (0, 255, 255),
        (0, 0, 0),
    ),
    theme(
        26,
        "Pac-Man Clock",
        ThemeGroup::GameClock,
        (255, 255, 0),
        (0, 0, 0),
    ),
    theme(
        27,
        "Versus Clock",
        ThemeGroup::GameClock,
        (255, 255, 255),
        (0, 0, 0),
    ),
    theme(
        28,
        "Slot Machine Clock",
        ThemeGroup::GameClock,
        (255, 255, 255),
        (0, 0, 0),
    ),
    theme(
        29,
        "Tetris Gameboy Clock",
        ThemeGroup::GameClock,
        (15, 56, 15),
        (139, 172, 15),
    ),
    theme(
        30,
        "Mario Clock",
        ThemeGroup::GameClock,
        (255, 50, 50),
        (255, 255, 255),
    ),
    theme(
        31,
        "Castlevania Clock",
        ThemeGroup::GameClock,
        (181, 140, 96),
        (160, 20, 20),
    ),
    theme_with_bounds(
        32,
        "Pocket Index",
        ThemeGroup::GameClock,
        (255, 255, 255),
        (0, 0, 0),
        (192, 64),
        Some((64, 128)),
    ),
    theme_with_bounds(
        33,
        "World Clock",
        ThemeGroup::GameClock,
        (255, 255, 255),
        (0, 0, 0),
        (192, 64),
        Some((64, 128)),
    ),
    theme(
        34,
        "Ms. Pac-Man Clock",
        ThemeGroup::GameClock,
        (255, 255, 0),
        (255, 60, 160),
    ),
    theme(
        35,
        "Mega Man Clock",
        ThemeGroup::GameClock,
        (0, 255, 255),
        (0, 0, 255),
    ),
    theme_with_bounds(
        37,
        "Words Clock",
        ThemeGroup::GameClock,
        (255, 255, 255),
        (72, 176, 200),
        (192, 64),
        None,
    ),
    theme(
        39,
        "Sonic Clock",
        ThemeGroup::GameClock,
        (0, 85, 255),
        (255, 224, 0),
    ),
    theme_with_bounds(
        41,
        "Metal Slug Clock",
        ThemeGroup::GameClock,
        (255, 140, 0),
        (0, 100, 0),
        (192, 64),
        Some((32, 128)),
    ),
];

/// Returns the full, ordered list of available themes (single source of truth).
pub fn all_themes() -> &'static [ThemeInfo] {
    THEMES
}

/// Checks whether a given theme ID is compatible with the logical panel dimensions `(w, h)`.
pub fn is_theme_compatible(theme_id: i32, w: u32, h: u32) -> bool {
    THEMES
        .iter()
        .find(|t| t.id == theme_id)
        .map(|t| t.is_compatible(w, h))
        .unwrap_or(true)
}

/// Whether a clock/date theme is animated and therefore needs a high frame rate
/// (~25fps) instead of the 1fps static cadence. Kept next to the theme catalog
/// so cadence stays in sync with the renderers.
pub fn is_realtime_theme(theme_id: i32) -> bool {
    matches!(theme_id, 12..=19 | 21..=35 | 37 | 39 | 41)
}

pub fn get_theme_info(theme_id: i32) -> ThemeInfo {
    THEMES
        .iter()
        .find(|t| t.id == theme_id)
        .cloned()
        .unwrap_or(ThemeInfo {
            id: -1,
            name: "Default",
            group: ThemeGroup::Publisher,
            primary_color: (255, 255, 255),
            secondary_color: (0, 0, 0),
            min_landscape: (64, 32),
            min_portrait: Some((32, 64)),
        })
}

pub fn parse_hex_color(hex: &str) -> (u8, u8, u8) {
    let s = hex.trim_start_matches('#');
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(255);
        let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(255);
        (r, g, b)
    } else {
        (255, 255, 255)
    }
}

pub fn interpolate_color(c1: (u8, u8, u8), c2: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    let r = (c1.0 as f32 + (c2.0 as f32 - c1.0 as f32) * t) as u8;
    let g = (c1.1 as f32 + (c2.1 as f32 - c1.1 as f32) * t) as u8;
    let b = (c1.2 as f32 + (c2.2 as f32 - c1.2 as f32) * t) as u8;
    (r, g, b)
}

pub fn generate_gradient_mask(w: u32, h: u32, c1: (u8, u8, u8), c2: (u8, u8, u8)) -> RgbImage {
    let mut img = RgbImage::new(w, h);
    for y in 0..h {
        let t = if h > 1 {
            y as f32 / (h - 1) as f32
        } else {
            0.0
        };
        let (r, g, b) = interpolate_color(c1, c2, t);
        for x in 0..w {
            img.put_pixel(x, y, Rgb([r, g, b]));
        }
    }
    img
}

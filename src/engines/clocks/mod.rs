pub mod binary_clock;
pub mod castle_clock;
pub mod cwassets;
pub mod cwscene;
pub mod mario_clock;
pub mod megaman_clock;
pub mod metal_slug_clock;
pub mod pacman_clock;
pub mod pacsprites;
pub mod pokedex_clock;
pub mod pong_clock;
pub mod slot_machine_clock;
pub mod sonic_clock;
pub mod tetris_clock;
pub mod versus_clock;
pub mod word_clock;
pub mod words_clock;
pub mod worldmap_clock;

pub use binary_clock::BinaryClock;
pub use castle_clock::CastleClock;
pub use mario_clock::MarioClock;
pub use megaman_clock::MegamanClock;
pub use metal_slug_clock::MetalSlugClock;
pub use pacman_clock::PacmanClock;
pub use pokedex_clock::PokedexClock;
pub use pong_clock::PongClock;
pub use slot_machine_clock::SlotMachineClock;
pub use sonic_clock::SonicClock;
pub use tetris_clock::TetrisClock;
pub use versus_clock::VersusClock;
pub use word_clock::WordClock;
pub use words_clock::WordsClock;
pub use worldmap_clock::WorldMapClock;

use crate::core::matrix::IDrawingSurface;
use crate::engines::renderers::base_renderer::ArcadeFont;
use crate::engines::renderers::BaseRenderer;

/// Faces laid out for 256x64 say so on a smaller panel rather than drawing a mess. The ESP32
/// firmware shows the same notice, and the theme name carries the same warning.
pub fn wide_only_notice(matrix: &mut dyn IDrawingSurface, font: &ArcadeFont<'_>, scale: u32) {
    matrix.clear();
    let h = matrix.height() as i32;
    BaseRenderer::draw_text_at(
        matrix,
        "NEEDS",
        font,
        scale.max(1) as f32,
        2,
        h / 2 - 8,
        (255, 160, 0),
        (0, 0, 0),
    );
    BaseRenderer::draw_text_at(
        matrix,
        "256x64",
        font,
        scale.max(1) as f32,
        2,
        h / 2 + 1,
        (255, 160, 0),
        (0, 0, 0),
    );
}

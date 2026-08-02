use crate::core::config::Config;
use crate::core::matrix::MatrixBackend;
use crate::engines::clocks::*;
use crate::engines::renderers::*;
use chrono::Local;

pub struct ClockEngine {
    base_renderer: BaseRenderer,
    cyberpunk: CyberpunkRenderer,
    flip: FlipRenderer,
    true_matrix: TrueMatrixRenderer,
    pong: PongClock,
    tetris: TetrisClock,
    tetris_gb: TetrisClock,
    word: WordClock,
    binary: BinaryClock,
    pacman: PacmanClock,
    versus: VersusClock,
    slot_machine: SlotMachineClock,
    last_font: String,
}

impl ClockEngine {
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            base_renderer: BaseRenderer::new(),
            cyberpunk: CyberpunkRenderer::new(w, h),
            flip: FlipRenderer::new(),
            true_matrix: TrueMatrixRenderer::new(w, h),
            pong: PongClock::new(w, h),
            tetris: TetrisClock::new(false),
            tetris_gb: TetrisClock::new(true),
            word: WordClock::new(),
            binary: BinaryClock::new(),
            pacman: PacmanClock::new(),
            versus: VersusClock::new(),
            slot_machine: SlotMachineClock::new(),
            last_font: String::new(),
        }
    }

    pub fn render(&mut self, matrix: &mut dyn MatrixBackend, config: &Config) {
        let settings = config.settings.read();
        let now = Local::now();

        // Full time string with seconds (for binary clock)
        let time_str_full = if settings.time_24h {
            now.format("%H:%M:%S").to_string()
        } else {
            now.format("%I:%M:%S %p").to_string()
        };

        // Short time string for display clocks
        let time_str = if settings.time_24h {
            now.format("%H:%M:%S").to_string()
        } else {
            now.format("%I:%M:%S").to_string()
        };

        let hours = now.hour();
        let minutes = now.minute();
        let seconds = now.second();

        // Reload font from disk if the config font changes
        if settings.time_font != self.last_font {
            self.base_renderer = BaseRenderer::from_font_path(&settings.time_font);
            self.last_font = settings.time_font.clone();
        }

        match settings.time_theme {
            18 => {
                self.cyberpunk.render(matrix);
                self.base_renderer.render_text(
                    matrix,
                    &time_str,
                    0,
                    settings.time_size,
                    settings.time_offset_x,
                    settings.time_offset_y,
                    Some((0, 255, 255)),
                    None,
                );
            }
            21 => {
                self.true_matrix.render(matrix);
                self.base_renderer.render_text(
                    matrix,
                    &time_str,
                    0,
                    settings.time_size,
                    settings.time_offset_x,
                    settings.time_offset_y,
                    Some((0, 255, 70)),
                    None,
                );
            }
            20 => {
                self.flip.render(
                    matrix,
                    &time_str,
                    settings.time_offset_x,
                    settings.time_offset_y,
                );
            }
            22 => self.pong.update_and_render(matrix, hours, minutes),
            23 => self.tetris.render(matrix, &time_str),
            24 => self.word.render(matrix, hours, minutes),
            25 => self.binary.render(matrix, hours, minutes, seconds),
            26 => self.pacman.render(matrix, &time_str, hours, minutes),
            27 => self.versus.render(matrix, hours, minutes),
            28 => self.slot_machine.render(matrix, &time_str),
            29 => self.tetris_gb.render(matrix, &time_str),
            _ => self.base_renderer.render_text(
                matrix,
                &time_str,
                settings.time_theme,
                settings.time_size,
                settings.time_offset_x,
                settings.time_offset_y,
                None,
                None,
            ),
        }
    }
}

// Helper to expose chrono fields without re-deriving
trait ChronoTimeExt {
    fn hour(&self) -> u32;
    fn minute(&self) -> u32;
    fn second(&self) -> u32;
}

impl ChronoTimeExt for chrono::DateTime<chrono::Local> {
    fn hour(&self) -> u32 {
        chrono::Timelike::hour(self)
    }
    fn minute(&self) -> u32 {
        chrono::Timelike::minute(self)
    }
    fn second(&self) -> u32 {
        chrono::Timelike::second(self)
    }
}

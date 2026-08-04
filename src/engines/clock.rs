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
    last_theme: i32,
    last_size: u32,
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
            last_theme: -1, // Invalid dummy default
            last_size: 0,
        }
    }

    pub fn render(&mut self, matrix: &mut dyn MatrixBackend, config: &Config) {
        let settings = config.settings.read();
        let now = Local::now();

        // Full time string with seconds (for binary clock)
        let time_str_full = now.format(&settings.time_format).to_string();

        // Short time string for display clocks
        let mut format_str = settings.time_format.clone();
        if settings.time_theme == 19 && !format_str.contains("%S") {
            format_str.push_str(":%S");
        }
        let time_str = now.format(&format_str).to_string();

        let hours = now.hour();
        let minutes = now.minute();
        let seconds = now.second();

        // Reload font from disk if the config font changes
        let mut reset_clocks = false;
        if settings.time_font != self.last_font {
            self.base_renderer = BaseRenderer::from_font_path(&settings.time_font);
            self.last_font = settings.time_font.clone();
            reset_clocks = true;
        }
        if settings.time_theme != self.last_theme {
            tracing::info!(
                from = self.last_theme,
                to = settings.time_theme,
                "clock theme change -> resetting sub-clocks"
            );
            self.last_theme = settings.time_theme;
            reset_clocks = true;
        }
        if settings.time_size != self.last_size {
            self.last_size = settings.time_size;
            reset_clocks = true;
        }

        if reset_clocks {
            let w = matrix.width() as u32;
            let h = matrix.height() as u32;
            self.pong = PongClock::new(w, h);
            self.tetris = TetrisClock::new(false);
            self.tetris_gb = TetrisClock::new(true);
            self.word = WordClock::new();
            self.binary = BinaryClock::new();
            self.pacman = PacmanClock::new();
            self.versus = VersusClock::new();
            self.slot_machine = SlotMachineClock::new();
            self.flip.reset();
            matrix.clear(); // Clear artifact pixels from old layout
        }

        let font = self.base_renderer.font();

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
            19 => self.flip.render(
                matrix,
                &time_str,
                &font,
                settings.time_size,
                settings.time_offset_x,
                settings.time_offset_y,
            ),
            20 => {
                // Custom Gradient
                let color1 = parse_hex_color(&settings.clock_color_1).unwrap_or((0, 255, 255));
                let color2 = parse_hex_color(&settings.clock_color_2).unwrap_or((255, 0, 255));
                self.base_renderer.render_text(
                    matrix,
                    &time_str,
                    20,
                    settings.time_size,
                    settings.time_offset_x,
                    settings.time_offset_y,
                    Some(color1),
                    Some(color2),
                );
            }
            22 => self
                .pong
                .update_and_render(matrix, hours, minutes, &font, settings.time_size),
            23 => self
                .tetris
                .render(matrix, &time_str, &font, settings.time_size),
            24 => self.word.render(
                matrix,
                hours,
                minutes,
                &font,
                settings.time_size,
                &settings.weather_lang,
            ),
            25 => self
                .binary
                .render(matrix, hours, minutes, seconds, &font, settings.time_size),
            26 => self
                .pacman
                .render(matrix, &time_str, hours, minutes, &font, settings.time_size),
            27 => self
                .versus
                .render(matrix, hours, minutes, &font, settings.time_size),
            28 => self
                .slot_machine
                .render(matrix, &time_str, &font, settings.time_size),
            29 => self
                .tetris_gb
                .render(matrix, &time_str, &font, settings.time_size),
            _ => {
                self.base_renderer.render_text(
                    matrix,
                    &time_str,
                    settings.time_theme,
                    settings.time_size,
                    settings.time_offset_x,
                    settings.time_offset_y,
                    None,
                    None,
                );
            }
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

fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some((r, g, b))
    } else {
        None
    }
}

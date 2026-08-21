use linkme::distributed_slice;
use crate::core::engine_contract::{Engine, EngineDescriptor, EngineMetadata, Capabilities, Requirements, ConfigSchema, EngineFactory, EngineConfig, EngineContext, EngineError};
use crate::engines::clocks::*;
use crate::engines::renderers::*;
use chrono::Timelike;
use parking_lot::RwLock;

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
    
    // Config states
    time_format: String,
    time_font: String,
    time_size: u32,
    time_theme: i32,
    clock_color_1: String,
    clock_color_2: String,
    time_offset_x: i32,
    time_offset_y: i32,
    
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
            
            time_format: "%H:%M:%S".to_string(),
            time_font: "PressStart2P.ttf".to_string(),
            time_size: 2,
            time_theme: 0,
            clock_color_1: "#ffffff".to_string(),
            clock_color_2: "#ffffff".to_string(),
            time_offset_x: 0,
            time_offset_y: 0,

            last_font: String::new(),
            last_theme: -1,
            last_size: 0,
        }
    }
}

impl Engine for ClockEngine {
    fn initialize(
        &mut self,
        _context: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        self.time_format = config.get_string("format", "%H:%M:%S");
        self.time_font = config.get_string("font", "PressStart2P.ttf");
        self.time_size = config.get_int("size", 2) as u32;
        self.time_theme = config.get_int("theme", 0);
        self.clock_color_1 = config.get_string("color_1", "#ffffff");
        self.clock_color_2 = config.get_string("color_2", "#ffffff");
        self.time_offset_x = config.get_int("offset_x", 0);
        self.time_offset_y = config.get_int("offset_y", 0);
        Ok(())
    }

    fn activate(&mut self) {}
    fn deactivate(&mut self) {}
    fn update(&mut self, _context: &mut EngineContext) {}

    fn render(&mut self, context: &mut EngineContext) {
        let matrix = &mut *context.matrix;
        
        let tz: chrono_tz::Tz = context.config
            .settings
            .read()
            .system
            .timezone
            .parse()
            .unwrap_or(chrono_tz::UTC);
        let now = chrono::Utc::now().with_timezone(&tz);

        // Full time string with seconds (for binary clock)
        let time_str_full = now.format(&self.time_format).to_string();

        // Short time string for display clocks
        let mut format_str = self.time_format.clone();
        if self.time_theme == 19 && !format_str.contains("%S") {
            format_str.push_str(":%S");
        }
        let time_str = now.format(&format_str).to_string();

        let hours = now.hour();
        let minutes = now.minute();
        let seconds = now.second();

        // Reload font from disk if the config font changes
        let mut reset_clocks = false;
        if self.time_font != self.last_font {
            self.base_renderer = BaseRenderer::from_font_path(&self.time_font);
            self.last_font = self.time_font.clone();
            reset_clocks = true;
        }
        if self.time_theme != self.last_theme {
            tracing::info!(
                from = self.last_theme,
                to = self.time_theme,
                "clock theme change -> resetting sub-clocks"
            );
            self.last_theme = self.time_theme;
            reset_clocks = true;
        }
        if self.time_size != self.last_size {
            self.last_size = self.time_size;
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

        match self.time_theme {
            18 => {
                self.cyberpunk.render(matrix);
                self.base_renderer.render_text(
                    matrix,
                    &time_str,
                    0,
                    self.time_size,
                    self.time_offset_x,
                    self.time_offset_y,
                    Some((0, 255, 255)),
                    None,
                );
            }
            21 => {
                self.true_matrix.render(matrix);
                self.base_renderer.render_text(
                    matrix,
                    &time_str,
                    21,
                    self.time_size,
                    self.time_offset_x,
                    self.time_offset_y,
                    Some((0, 140, 0)),
                    Some((0, 0, 0)),
                );
            }
            19 => self.flip.render(
                matrix,
                &time_str,
                &font,
                self.time_size,
                self.time_offset_x,
                self.time_offset_y,
            ),
            20 => {
                // Custom Gradient
                let color1 = parse_hex_color(&self.clock_color_1).unwrap_or((0, 255, 255));
                let color2 = parse_hex_color(&self.clock_color_2).unwrap_or((255, 0, 255));
                self.base_renderer.render_text(
                    matrix,
                    &time_str,
                    20,
                    self.time_size,
                    self.time_offset_x,
                    self.time_offset_y,
                    Some(color1),
                    Some(color2),
                );
            }
            22 => self
                .pong
                .update_and_render(matrix, hours, minutes, &font, self.time_size),
            23 => self
                .tetris
                .render(matrix, &time_str, &font, self.time_size),
            24 => self.word.render(
                matrix,
                hours,
                minutes,
                &font,
                self.time_size,
                &context.config.settings.read().system.lang,
            ),
            25 => self
                .binary
                .render(matrix, hours, minutes, seconds, &font, self.time_size),
            26 => self
                .pacman
                .render(matrix, &time_str, hours, minutes, &font, self.time_size),
            27 => self
                .versus
                .render(matrix, hours, minutes, &font, self.time_size),
            28 => self
                .slot_machine
                .render(matrix, &time_str, &font, self.time_size),
            29 => self
                .tetris_gb
                .render(matrix, &time_str, &font, self.time_size),
            _ => {
                self.base_renderer.render_text(
                    matrix,
                    &time_str,
                    self.time_theme,
                    self.time_size,
                    self.time_offset_x,
                    self.time_offset_y,
                    None,
                    None,
                );
            }
        }
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


#[distributed_slice(crate::core::registry::ENGINES)]
fn register_ClockEngine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "clock",
            name: "ClockEngine",
            category: "info",
            version: "1.0.0",
        },
        capabilities: Capabilities::default(),
        requirements: Requirements::default(),
        schema: ConfigSchema { fields: vec![] },
        factory: || -> Box<dyn crate::core::engine_contract::Engine> {
            // We pass 0, 0 since width/height are handled dynamically now or don't matter in new()
            Box::new(ClockEngine::new(64, 32))
        },
    }
}

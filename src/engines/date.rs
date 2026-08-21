use linkme::distributed_slice;
use crate::core::engine_contract::{Engine, EngineDescriptor, EngineMetadata, Capabilities, Requirements, ConfigSchema, EngineFactory, EngineConfig, EngineContext, EngineError};
use crate::engines::renderers::{
    BaseRenderer, CyberpunkRenderer, FlipRenderer, TrueMatrixRenderer,
};
use parking_lot::RwLock;

pub struct DateEngine {
    base_renderer: BaseRenderer,
    cyberpunk: CyberpunkRenderer,
    flip: FlipRenderer,
    true_matrix: TrueMatrixRenderer,
    
    date_format: String,
    date_font: String,
    date_size: u32,
    date_theme: i32,
    date_color_1: String,
    date_color_2: String,
    date_offset_x: i32,
    date_offset_y: i32,
    
    last_font: String,
}

impl DateEngine {
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            base_renderer: BaseRenderer::new(),
            cyberpunk: CyberpunkRenderer::new(w, h),
            flip: FlipRenderer::new(),
            true_matrix: TrueMatrixRenderer::new(w, h),
            
            date_format: "%d/%m".to_string(),
            date_font: "PressStart2P.ttf".to_string(),
            date_size: 2,
            date_theme: 0,
            date_color_1: "#ffffff".to_string(),
            date_color_2: "#ffffff".to_string(),
            date_offset_x: 0,
            date_offset_y: 0,
            
            last_font: String::new(),
        }
    }
}

impl Engine for DateEngine {
    fn initialize(
        &mut self,
        _context: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        self.date_format = config.get_string("format", "%d/%m");
        self.date_font = config.get_string("font", "PressStart2P.ttf");
        self.date_size = config.get_int("size", 2) as u32;
        self.date_theme = config.get_int("theme", 0);
        self.date_color_1 = config.get_string("color_1", "#ffffff");
        self.date_color_2 = config.get_string("color_2", "#ffffff");
        self.date_offset_x = config.get_int("offset_x", 0);
        self.date_offset_y = config.get_int("offset_y", 0);
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
        
        let mut format_str = self.date_format.clone();
        format_str = format_str.replace("YYYY", "%Y");
        format_str = format_str.replace("YY", "%y");
        format_str = format_str.replace("MM", "%m");
        format_str = format_str.replace("DD", "%d");
        let date_str = now.format(&format_str).to_string();

        // Reload font if changed
        if self.date_font != self.last_font {
            self.base_renderer = BaseRenderer::from_font_path(&self.date_font);
            self.last_font = self.date_font.clone();
            self.flip.reset();
        }

        let color1 = parse_hex_color(&self.date_color_1);
        let color2 = parse_hex_color(&self.date_color_2);

        match self.date_theme {
            18 => {
                self.cyberpunk.render(matrix);
                self.base_renderer.render_text(
                    matrix,
                    &date_str,
                    0,
                    self.date_size,
                    self.date_offset_x,
                    self.date_offset_y,
                    color1,
                    None,
                );
            }
            21 => {
                self.true_matrix.render(matrix);
                self.base_renderer.render_text(
                    matrix,
                    &date_str,
                    21,
                    self.date_size,
                    self.date_offset_x,
                    self.date_offset_y,
                    Some((0, 140, 0)),
                    Some((0, 0, 0)),
                );
            }
            19 => {
                let font = self.base_renderer.font();
                self.flip.render(
                    matrix,
                    &date_str,
                    &font,
                    self.date_size,
                    self.date_offset_x,
                    self.date_offset_y,
                );
            }
            20 => {
                self.base_renderer.render_text(
                    matrix,
                    &date_str,
                    20,
                    self.date_size,
                    self.date_offset_x,
                    self.date_offset_y,
                    color1,
                    color2,
                );
            }
            _ => {
                self.base_renderer.render_text(
                    matrix,
                    &date_str,
                    self.date_theme,
                    self.date_size,
                    self.date_offset_x,
                    self.date_offset_y,
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
fn register_DateEngine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "date",
            name: "DateEngine",
            category: "info",
            version: "1.0.0",
        },
        capabilities: Capabilities::default(),
        requirements: Requirements::default(),
        schema: ConfigSchema { fields: vec![] },
        factory: || -> Box<dyn crate::core::engine_contract::Engine> {
            // We pass 0, 0 since width/height are handled dynamically now or don't matter in new()
            Box::new(DateEngine::new(64, 32))
        },
    }
}

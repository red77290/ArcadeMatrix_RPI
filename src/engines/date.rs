use crate::core::engine_contract::{
    Capabilities, ConfigSchema, Engine, EngineConfig, EngineContext, EngineDescriptor, EngineError,
    EngineMetadata, Requirements,
};
use crate::engines::renderers::{
    BaseRenderer, CyberpunkRenderer, FlipRenderer, TrueMatrixRenderer,
};
use linkme::distributed_slice;

pub struct DateEngine {
    base_renderer: BaseRenderer,
    cyberpunk: CyberpunkRenderer,
    flip: FlipRenderer,
    true_matrix: TrueMatrixRenderer,

    date_format: String,
    date_font: String,
    date_size: u32,
    date_theme: i32,
    timezone: String,
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
            timezone: "".to_string(),
            date_color_1: "#ffffff".to_string(),
            date_color_2: "#ffffff".to_string(),
            date_offset_x: 0,
            date_offset_y: 0,

            last_font: String::new(),
        }
    }

    /// Reads every configurable field. Shared by `initialize` and
    /// `on_config_changed` so live UI edits apply without a restart.
    fn apply_config(&mut self, config: &dyn EngineConfig) {
        self.date_format = config.get_string("format", "%d/%m");
        self.date_font = config.get_string("font", "PressStart2P.ttf");
        self.date_size = config.get_int("size", 2) as u32;
        self.date_theme = config.get_int("theme", 0);
        self.timezone = config.get_string("timezone", "");
        self.date_color_1 = config.get_string("color_1", "#ffffff");
        self.date_color_2 = config.get_string("color_2", "#ffffff");
        self.date_offset_x = config.get_int("offset_x", 0);
        self.date_offset_y = config.get_int("offset_y", 0);
    }
}

impl Engine for DateEngine {
    fn initialize(
        &mut self,
        _context: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        self.apply_config(config);
        Ok(())
    }

    fn activate(&mut self) {}
    fn deactivate(&mut self) {}
    fn update(&mut self, _context: &mut EngineContext) {}

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        self.apply_config(config);
    }

    fn is_realtime(&self) -> bool {
        crate::core::theme::is_realtime_theme(self.date_theme)
    }

    fn render(&mut self, context: &mut EngineContext) {
        let matrix = &mut *context.matrix;
        let tz_str = if !self.timezone.is_empty() {
            self.timezone.clone()
        } else {
            context.config.settings.read().system.timezone.clone()
        };

        let now = if let Some(tz) = crate::engines::clock::parse_tz(&tz_str) {
            chrono::Utc::now().with_timezone(&tz).naive_local()
        } else {
            chrono::Local::now().naive_local()
        };

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
            let w = matrix.width() as u32;
            let h = matrix.height() as u32;
            self.cyberpunk = CyberpunkRenderer::new(w, h);
            self.true_matrix = TrueMatrixRenderer::new(w, h);
        }

        let color1 = parse_hex_color(&self.date_color_1);
        let color2 = parse_hex_color(&self.date_color_2);

        match self.date_theme {
            18 => {
                self.cyberpunk.render(matrix);
                self.base_renderer.render_text(
                    matrix,
                    &date_str,
                    18,
                    self.date_size,
                    self.date_offset_x,
                    self.date_offset_y,
                    Some((0, 140, 0)),
                    Some((0, 0, 0)),
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
fn register_date_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "date",
            name: "DateEngine",
            category: "info",
            version: crate::core::build_info::VERSION,
        },
        capabilities: Capabilities::default(),
        requirements: Requirements::default(),
        schema: ConfigSchema {
            fields: vec![
                crate::core::engine_contract::ConfigField {
                    id: "theme",
                    field_type: crate::core::engine_contract::ConfigType::Options,
                    label: "Theme",
                    description: "Date theme",
                    default_value: "0",
                    min_val: Some("0"),
                    options_endpoint: Some("/api/themes"),
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Clamp,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "format",
                    field_type: crate::core::engine_contract::ConfigType::String,
                    label: "Format",
                    description: "Date format (e.g. DD/MM)",
                    default_value: "%d/%m",
                    validation_policy:
                        crate::core::engine_contract::ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "font",
                    field_type: crate::core::engine_contract::ConfigType::String,
                    label: "Font",
                    description: "Font file path",
                    default_value: "PressStart2P.ttf",
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Accept,
                    options_endpoint: Some("/api/fonts"),
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "timezone",
                    field_type: crate::core::engine_contract::ConfigType::Options,
                    label: "Timezone",
                    description: "Select timezone or region",
                    default_value: "Europe/Paris",
                    options_endpoint: Some("/api/timezones"),
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Accept,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "size",
                    field_type: crate::core::engine_contract::ConfigType::Integer,
                    label: "Size",
                    description: "Font size scale",
                    default_value: "2",
                    min_val: Some("1"),
                    max_val: Some("10"),
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Clamp,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "color_1",
                    field_type: crate::core::engine_contract::ConfigType::String,
                    label: "Primary Color",
                    description: "Hex color",
                    default_value: "#FFFFFF",
                    validation_policy:
                        crate::core::engine_contract::ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "color_2",
                    field_type: crate::core::engine_contract::ConfigType::String,
                    label: "Secondary Color",
                    description: "Hex color for secondary elements",
                    default_value: "#FFFFFF",
                    validation_policy:
                        crate::core::engine_contract::ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "offset_x",
                    field_type: crate::core::engine_contract::ConfigType::Integer,
                    label: "X Offset",
                    description: "Horizontal shift",
                    default_value: "0",
                    min_val: Some("-64"),
                    max_val: Some("64"),
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Clamp,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "offset_y",
                    field_type: crate::core::engine_contract::ConfigType::Integer,
                    label: "Y Offset",
                    description: "Vertical shift",
                    default_value: "0",
                    min_val: Some("-32"),
                    max_val: Some("32"),
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Clamp,
                    ..Default::default()
                },
            ],
        },
        factory: || -> Box<dyn crate::core::engine_contract::Engine> {
            // We pass 0, 0 since width/height are handled dynamically now or don't matter in new()
            Box::new(DateEngine::new(64, 32))
        },
    }
}

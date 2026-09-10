use crate::core::engine_contract::{
    Capabilities, ConfigSchema, Engine, EngineConfig, EngineContext, EngineDescriptor, EngineError,
    EngineMetadata, Requirements,
};
use crate::core::matrix::MatrixBackend;
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
        let fmt = config.get_string("format", "");
        self.date_format = if !fmt.is_empty() {
            fmt
        } else {
            config.get_string("date_format", "%d/%m")
        };

        let font = config.get_string("font", "");
        self.date_font = if !font.is_empty() {
            font
        } else {
            config.get_string("date_font", "PressStart2P.ttf")
        };

        let size = config.get_int("size", 0);
        self.date_size = if size > 0 {
            size as u32
        } else {
            config.get_int("date_size", 2).max(1) as u32
        };

        let theme = config.get_int("theme", -1);
        self.date_theme = if theme >= 0 {
            theme
        } else {
            config.get_int("date_theme", 0)
        };

        let tz = config.get_string("timezone", "");
        self.timezone = if !tz.is_empty() {
            tz
        } else {
            config.get_string("date_timezone", "")
        };

        let c1 = config.get_string("color_1", "");
        self.date_color_1 = if !c1.is_empty() {
            c1
        } else {
            config.get_string("date_color_1", "#ffffff")
        };

        let c2 = config.get_string("color_2", "");
        self.date_color_2 = if !c2.is_empty() {
            c2
        } else {
            config.get_string("date_color_2", "#ffffff")
        };

        let ox = config.get_int("offset_x", 0);
        self.date_offset_x = if ox != 0 {
            ox
        } else {
            config.get_int("date_offset_x", 0)
        };

        let oy = config.get_int("offset_y", 0);
        self.date_offset_y = if oy != 0 {
            oy
        } else {
            config.get_int("date_offset_y", 0)
        };
    }

    fn render_tate_date(
        &self,
        matrix: &mut dyn MatrixBackend,
        date_str: &str,
        theme_id: i32,
        effective_size: u32,
        color1: Option<(u8, u8, u8)>,
        color2: Option<(u8, u8, u8)>,
    ) -> bool {
        let parts: Vec<&str> = date_str
            .split(|c| c == '/' || c == '-' || c == ' ' || c == '.')
            .filter(|s| !s.is_empty())
            .collect();
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;
        let font = self.base_renderer.font();

        if parts.len() == 3 && h >= 96 {
            let max_tier_h = h / 3;
            let mut scale = effective_size;
            while scale > 1 {
                let (_, w1, h1) = font.get_pixel_map(parts[0], scale as f32);
                let (_, w2, h2) = font.get_pixel_map(parts[1], scale as f32);
                let (_, w3, h3) = font.get_pixel_map(parts[2], scale as f32);
                if w1.max(w2).max(w3) <= w && h1.max(h2).max(h3) <= max_tier_h {
                    break;
                }
                scale -= 1;
            }

            let (_, w1, h1) = font.get_pixel_map(parts[0], scale as f32);
            let (_, w2, h2) = font.get_pixel_map(parts[1], scale as f32);
            let (_, w3, h3) = font.get_pixel_map(parts[2], scale as f32);
            let x1 = (w - w1) / 2 + self.date_offset_x;
            let x2 = (w - w2) / 2 + self.date_offset_x;
            let x3 = (w - w3) / 2 + self.date_offset_x;
            let y1 = (h / 6) - (h1 / 2) + self.date_offset_y;
            let y2 = (h / 2) - (h2 / 2) + self.date_offset_y;
            let y3 = (5 * h / 6) - (h3 / 2) + self.date_offset_y;

            self.base_renderer
                .draw_themed_text_at(matrix, parts[0], theme_id, scale, x1, y1, color1, color2);
            self.base_renderer
                .draw_themed_text_at(matrix, parts[1], theme_id, scale, x2, y2, color1, color2);
            self.base_renderer
                .draw_themed_text_at(matrix, parts[2], theme_id, scale, x3, y3, color1, color2);
            true
        } else if parts.len() >= 2 {
            let max_tier_h = h / 2;
            let mut scale = effective_size;
            while scale > 1 {
                let (_, w1, h1) = font.get_pixel_map(parts[0], scale as f32);
                let (_, w2, h2) = font.get_pixel_map(parts[1], scale as f32);
                if w1.max(w2) <= w && h1.max(h2) <= max_tier_h {
                    break;
                }
                scale -= 1;
            }

            let (_, w1, h1) = font.get_pixel_map(parts[0], scale as f32);
            let (_, w2, h2) = font.get_pixel_map(parts[1], scale as f32);
            let x1 = (w - w1) / 2 + self.date_offset_x;
            let x2 = (w - w2) / 2 + self.date_offset_x;
            let y1 = (h / 4) - (h1 / 2) + self.date_offset_y;
            let y2 = (3 * h / 4) - (h2 / 2) + self.date_offset_y;

            self.base_renderer
                .draw_themed_text_at(matrix, parts[0], theme_id, scale, x1, y1, color1, color2);
            self.base_renderer
                .draw_themed_text_at(matrix, parts[1], theme_id, scale, x2, y2, color1, color2);
            true
        } else {
            false
        }
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
        let tz_str = if !self.timezone.is_empty() && self.timezone != "system" {
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

        let is_tate = matrix.width() < 48 || matrix.height() > (matrix.width() * 3) / 2;
        let effective_size = self.date_size.max(1);

        let color1 = parse_hex_color(&self.date_color_1);
        let color2 = parse_hex_color(&self.date_color_2);

        match self.date_theme {
            18 => {
                self.cyberpunk.render(matrix);
                self.base_renderer.render_text(
                    matrix,
                    &date_str,
                    18,
                    effective_size,
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
                    effective_size,
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
                    effective_size,
                    self.date_offset_x,
                    self.date_offset_y,
                );
            }
            20 => {
                if is_tate
                    && self.render_tate_date(matrix, &date_str, 20, effective_size, color1, color2)
                {
                    return;
                }
                self.base_renderer.render_text(
                    matrix,
                    &date_str,
                    20,
                    effective_size,
                    self.date_offset_x,
                    self.date_offset_y,
                    color1,
                    color2,
                );
            }
            _ => {
                if is_tate
                    && self.render_tate_date(
                        matrix,
                        &date_str,
                        self.date_theme,
                        effective_size,
                        None,
                        None,
                    )
                {
                    return;
                }
                self.base_renderer.render_text(
                    matrix,
                    &date_str,
                    self.date_theme,
                    effective_size,
                    self.date_offset_x,
                    self.date_offset_y,
                    None,
                    None,
                );
            }
        }
    }

    fn on_display_geometry_changed(&mut self, geometry: &crate::core::types::DisplayGeometry) {
        let w = geometry.logical_width;
        let h = geometry.logical_height;
        self.cyberpunk = CyberpunkRenderer::new(w, h);
        self.true_matrix = TrueMatrixRenderer::new(w, h);
        self.flip.reset();
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
        available: true,
        unavailable_reason: None,
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

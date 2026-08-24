use crate::core::engine_contract::{
    Capabilities, ConfigSchema, Engine, EngineConfig, EngineContext, EngineDescriptor, EngineError,
    EngineMetadata, Requirements,
};
use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::BaseRenderer;
use linkme::distributed_slice;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePayload {
    pub text: String,
    pub color: String,
    pub size: u32,
    pub direction: String,
    pub speed: u32,
    #[serde(alias = "timeoutSeconds")]
    pub timeout_seconds: u32,
}

impl MessagePayload {
    pub fn new(
        text: String,
        color: &str,
        size: u32,
        direction: &str,
        timeout_seconds: u32,
    ) -> Self {
        Self {
            text,
            color: color.to_string(),
            size,
            direction: direction.to_string(),
            speed: 50,
            timeout_seconds,
        }
    }
}

pub struct MessageEngine {
    base_renderer: BaseRenderer,
    offset_x: f32,
    text: String,
    color: String,
    size: u32,
    direction: String,
    speed: u32,
    last_update: Option<Instant>,
}

impl MessageEngine {
    pub fn new() -> Self {
        Self {
            base_renderer: BaseRenderer::new(),
            offset_x: 64.0,
            text: String::new(),
            color: "#ffffff".to_string(),
            size: 1,
            direction: "left".to_string(),
            speed: 50,
            last_update: None,
        }
    }
}

impl MessageEngine {
    pub fn reset_state(&mut self, width: f32) {
        self.offset_x = width;
    }

    pub fn render_payload(
        &mut self,
        matrix: &mut dyn MatrixBackend,
        payload: &MessagePayload,
    ) -> bool {
        let move_px = 33.0 / payload.speed.max(1) as f32; // Assuming ~33ms frame time

        if payload.direction == "none" {
            self.offset_x = 0.0;
        } else {
            self.offset_x -= move_px;
        }

        let font = self.base_renderer.font();
        let (pixels, _, _) = font.get_pixel_map(&payload.text, payload.size as f32);
        let mut text_w = 0;
        for char_pixels in &pixels {
            for &(px, _) in char_pixels {
                text_w = text_w.max(px + 1);
            }
        }

        let mut finished = false;
        if payload.direction != "none" && self.offset_x < -(text_w as f32) {
            self.offset_x = matrix.width() as f32;
            finished = true;
        }

        let mut r = 255;
        let mut g = 255;
        let mut b = 255;

        if payload.color.starts_with('#') && payload.color.len() == 7 {
            if let Ok(c) = u32::from_str_radix(&payload.color[1..], 16) {
                r = ((c >> 16) & 0xFF) as u8;
                g = ((c >> 8) & 0xFF) as u8;
                b = (c & 0xFF) as u8;
            }
        }

        self.base_renderer.render_text(
            matrix,
            &payload.text,
            -1,
            payload.size,
            self.offset_x as i32,
            0,
            Some((r, g, b)),
            None,
        );

        finished
    }
}

impl Engine for MessageEngine {
    fn initialize(
        &mut self,
        _context: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        self.text = config.get_string("text", "Hello");
        self.color = config.get_string("color", "#ffffff");
        self.size = config.get_int("size", 1) as u32;
        self.direction = config.get_string("direction", "left");
        self.speed = config.get_int("speed", 50) as u32;
        Ok(())
    }

    fn activate(&mut self) {
        self.offset_x = 64.0;
        self.last_update = Some(Instant::now());
    }

    fn update(&mut self, _context: &mut EngineContext) {
        let now = Instant::now();
        let dt = self
            .last_update
            .map(|l| now.duration_since(l))
            .unwrap_or(Duration::ZERO);
        self.last_update = Some(now);

        let move_px = (dt.as_millis() as f32) / (self.speed.max(1) as f32);

        if self.direction == "none" {
            self.offset_x = 0.0;
        } else {
            self.offset_x -= move_px;
        }
    }

    fn render(&mut self, context: &mut EngineContext) {
        let matrix = &mut *context.matrix;

        let font = self.base_renderer.font();
        let (pixels, _, _) = font.get_pixel_map(&self.text, self.size as f32);
        let mut text_w = 0;
        for char_pixels in &pixels {
            for &(px, _) in char_pixels {
                text_w = text_w.max(px + 1);
            }
        }

        if self.direction != "none" && self.offset_x < -(text_w as f32) {
            self.offset_x = matrix.width() as f32;
        }

        let mut r = 255;
        let mut g = 255;
        let mut b = 255;

        if self.color.starts_with('#') && self.color.len() == 7 {
            if let Ok(c) = u32::from_str_radix(&self.color[1..], 16) {
                r = ((c >> 16) & 0xFF) as u8;
                g = ((c >> 8) & 0xFF) as u8;
                b = (c & 0xFF) as u8;
            }
        }

        self.base_renderer.render_text(
            matrix,
            &self.text,
            -1,
            self.size,
            self.offset_x as i32,
            0,
            Some((r, g, b)),
            None,
        );
    }

    fn deactivate(&mut self) {}

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        self.text = config.get_string("text", "Hello");
        self.color = config.get_string("color", "#ffffff");
        self.size = config.get_int("size", 1) as u32;
        self.direction = config.get_string("direction", "left");
        self.speed = config.get_int("speed", 50) as u32;
    }
}

#[distributed_slice(crate::core::registry::ENGINES)]
fn register_message_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "message",
            name: "MessageEngine",
            category: "info",
            version: crate::core::build_info::VERSION,
        },
        capabilities: Capabilities {
            realtime: true,
            ..Default::default()
        },
        requirements: Requirements::default(),
        schema: ConfigSchema {
            fields: vec![
                crate::core::engine_contract::ConfigField {
                    id: "text",
                    field_type: crate::core::engine_contract::ConfigType::String,
                    label: "Text",
                    description: "The message to display",
                    default_value: "Hello",
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Accept,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "color",
                    field_type: crate::core::engine_contract::ConfigType::String,
                    label: "Color",
                    description: "Hex color, e.g. #ffffff",
                    default_value: "#ffffff",
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Accept,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "size",
                    field_type: crate::core::engine_contract::ConfigType::Integer,
                    label: "Font Size",
                    description: "Text scale multiplier",
                    default_value: "1",
                    min_val: Some("1"),
                    max_val: Some("4"),
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Clamp,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "direction",
                    field_type: crate::core::engine_contract::ConfigType::Options,
                    label: "Direction",
                    description: "Scroll direction, or static",
                    default_value: "left",
                    options: Some(vec![
                        crate::core::engine_contract::ConfigOption {
                            label: "Scroll left",
                            value: "left",
                        },
                        crate::core::engine_contract::ConfigOption {
                            label: "Static (no scroll)",
                            value: "none",
                        },
                    ]),
                    validation_policy:
                        crate::core::engine_contract::ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "speed",
                    field_type: crate::core::engine_contract::ConfigType::Integer,
                    label: "Scroll Speed",
                    description: "Lower = faster (ms per pixel). Ignored when static.",
                    default_value: "50",
                    min_val: Some("1"),
                    max_val: Some("500"),
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Clamp,
                    ..Default::default()
                },
            ],
        },
        factory: || -> Box<dyn crate::core::engine_contract::Engine> {
            Box::new(MessageEngine::new())
        },
    }
}

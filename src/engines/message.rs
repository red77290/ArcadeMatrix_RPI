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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
            speed: 40,
            timeout_seconds,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScrollDirection {
    Rtl,
    Ltr,
    Ttb,
    Btt,
    None,
}

impl ScrollDirection {
    fn parse(s: &str) -> Self {
        if s.eq_ignore_ascii_case("rtl") || s.eq_ignore_ascii_case("left") {
            ScrollDirection::Rtl
        } else if s.eq_ignore_ascii_case("ltr") || s.eq_ignore_ascii_case("right") {
            ScrollDirection::Ltr
        } else if s.eq_ignore_ascii_case("ttb") || s.eq_ignore_ascii_case("down") {
            ScrollDirection::Ttb
        } else if s.eq_ignore_ascii_case("btt") || s.eq_ignore_ascii_case("up") {
            ScrollDirection::Btt
        } else {
            ScrollDirection::None
        }
    }
}

pub struct MessageEngine {
    base_renderer: BaseRenderer,
    offset_x: f32,
    offset_y: f32,
    move_accumulator: f32,
    text: String,
    color: String,
    parsed_color: (u8, u8, u8),
    size: u32,
    direction: ScrollDirection,
    speed: u32,
    text_w: i32,
    text_h: i32,
    cached_pixels: Vec<Vec<(i32, i32)>>,
    last_update: Option<Instant>,
    last_payload: Option<std::sync::Arc<MessagePayload>>,
}

impl MessageEngine {
    pub fn new() -> Self {
        Self {
            base_renderer: BaseRenderer::new(),
            offset_x: 64.0,
            offset_y: 0.0,
            move_accumulator: 0.0,
            text: String::new(),
            color: "#ffffff".to_string(),
            parsed_color: (255, 255, 255),
            size: 1,
            direction: ScrollDirection::Rtl,
            speed: 40,
            text_w: 0,
            text_h: 0,
            cached_pixels: Vec::new(),
            last_update: None,
            last_payload: None,
        }
    }

    pub fn reset_state(&mut self, width: f32, height: f32) {
        self.move_accumulator = 0.0;
        let font = self.base_renderer.font();
        let (pixels, _, _) = font.get_pixel_map(&self.text, self.size as f32);
        let mut text_w = 0;
        let mut text_h = 0;
        for char_pixels in &pixels {
            for &(px, py) in char_pixels {
                text_w = text_w.max(px + 1);
                text_h = text_h.max(py + 1);
            }
        }
        self.text_w = text_w;
        self.text_h = text_h;
        self.cached_pixels = pixels;

        // Parse hex color into RGB triplet once (Cold Path)
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
        self.parsed_color = (r, g, b);

        match self.direction {
            ScrollDirection::Rtl => {
                self.offset_x = width;
                self.offset_y = ((height as i32 - text_h) / 2).max(0) as f32;
            }
            ScrollDirection::Ltr => {
                self.offset_x = -(text_w as f32);
                self.offset_y = ((height as i32 - text_h) / 2).max(0) as f32;
            }
            ScrollDirection::Ttb => {
                self.offset_x = ((width as i32 - text_w) / 2).max(0) as f32;
                self.offset_y = -(text_h as f32);
            }
            ScrollDirection::Btt => {
                self.offset_x = ((width as i32 - text_w) / 2).max(0) as f32;
                self.offset_y = height;
            }
            ScrollDirection::None => {
                self.offset_x = ((width as i32 - text_w) / 2).max(0) as f32;
                self.offset_y = ((height as i32 - text_h) / 2).max(0) as f32;
            }
        }
    }

    pub fn render_payload(
        &mut self,
        matrix: &mut dyn MatrixBackend,
        payload: &MessagePayload,
    ) -> bool {
        let move_px = 33.0 / payload.speed.max(1) as f32; // Assuming ~33ms frame time
        let font = self.base_renderer.font();
        let (pixels, _, _) = font.get_pixel_map(&payload.text, payload.size as f32);
        let mut text_w = 0;
        let mut text_h = 0;
        for char_pixels in &pixels {
            for &(px, py) in char_pixels {
                text_w = text_w.max(px + 1);
                text_h = text_h.max(py + 1);
            }
        }

        let mut finished = false;
        let dir = payload.direction.to_lowercase();
        let mat_w = matrix.width() as f32;
        let mat_h = matrix.height() as f32;

        if dir == "rtl" || dir == "left" {
            self.offset_x -= move_px;
            self.offset_y = ((matrix.height() as i32 - text_h) / 2).max(0) as f32;
            if self.offset_x < -(text_w as f32) {
                self.offset_x = mat_w;
                finished = true;
            }
        } else if dir == "ltr" || dir == "right" {
            self.offset_x += move_px;
            self.offset_y = ((matrix.height() as i32 - text_h) / 2).max(0) as f32;
            if self.offset_x > mat_w {
                self.offset_x = -(text_w as f32);
                finished = true;
            }
        } else if dir == "ttb" || dir == "down" {
            self.offset_y += move_px;
            self.offset_x = ((matrix.width() as i32 - text_w) / 2).max(0) as f32;
            if self.offset_y > mat_h {
                self.offset_y = -(text_h as f32);
                finished = true;
            }
        } else if dir == "btt" || dir == "up" {
            self.offset_y -= move_px;
            self.offset_x = ((matrix.width() as i32 - text_w) / 2).max(0) as f32;
            if self.offset_y < -(text_h as f32) {
                self.offset_y = mat_h;
                finished = true;
            }
        } else {
            self.offset_x = ((matrix.width() as i32 - text_w) / 2).max(0) as f32;
            self.offset_y = ((matrix.height() as i32 - text_h) / 2).max(0) as f32;
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
            self.offset_y as i32,
            Some((r, g, b)),
            None,
        );

        finished
    }
}

impl Engine for MessageEngine {
    fn initialize(
        &mut self,
        context: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        self.text = config.get_string("text", "ArcadeMatrix");
        self.color = config.get_string("color", "#ffffff");
        self.size = config.get_int("size", 1) as u32;
        self.direction = ScrollDirection::parse(&config.get_string("direction", "rtl"));
        self.speed = config.get_int("speed", 50) as u32;
        self.reset_state(
            context.matrix.width() as f32,
            context.matrix.height() as f32,
        );
        Ok(())
    }

    fn activate(&mut self) {
        self.reset_state(128.0, 64.0);
        self.last_update = Some(Instant::now());
    }

    fn update(&mut self, context: &mut EngineContext) {
        let snap = context.config.producer_snapshot.load();
        if let Some(ref payload) = snap.message_payload {
            let is_changed = match &self.last_payload {
                Some(last) => !std::sync::Arc::ptr_eq(last, payload),
                None => true,
            };
            if is_changed {
                self.last_payload = Some(std::sync::Arc::clone(payload));
                self.text = payload.text.clone();
                self.color = payload.color.clone();
                self.size = payload.size;
                self.direction = ScrollDirection::parse(&payload.direction);
                self.speed = payload.speed;
                self.reset_state(
                    context.matrix.width() as f32,
                    context.matrix.height() as f32,
                );
            }
        }

        let now = Instant::now();
        let dt = self
            .last_update
            .map(|l| now.duration_since(l))
            .unwrap_or(Duration::ZERO);
        self.last_update = Some(now);

        let dt_secs = dt.as_secs_f32();
        let dt_ms = if dt_secs > 0.1 {
            16.6
        } else {
            dt_secs * 1000.0
        };
        if dt_ms <= 0.0 {
            return;
        }

        let step_ms = (self.speed.max(1) as f32).max(1.0);
        let move_px = dt_ms / step_ms;
        let mat_w = context.matrix.width() as f32;
        let mat_h = context.matrix.height() as f32;

        match self.direction {
            ScrollDirection::Rtl => {
                self.offset_x -= move_px;
                self.offset_y = ((context.matrix.height() as i32 - self.text_h) / 2).max(0) as f32;
                if self.offset_x < -(self.text_w as f32) {
                    self.offset_x = mat_w;
                }
            }
            ScrollDirection::Ltr => {
                self.offset_x += move_px;
                self.offset_y = ((context.matrix.height() as i32 - self.text_h) / 2).max(0) as f32;
                if self.offset_x > mat_w {
                    self.offset_x = -(self.text_w as f32);
                }
            }
            ScrollDirection::Ttb => {
                self.offset_y += move_px;
                self.offset_x = ((context.matrix.width() as i32 - self.text_w) / 2).max(0) as f32;
                if self.offset_y > mat_h {
                    self.offset_y = -(self.text_h as f32);
                }
            }
            ScrollDirection::Btt => {
                self.offset_y -= move_px;
                self.offset_x = ((context.matrix.width() as i32 - self.text_w) / 2).max(0) as f32;
                if self.offset_y < -(self.text_h as f32) {
                    self.offset_y = mat_h;
                }
            }
            ScrollDirection::None => {
                self.offset_x = ((context.matrix.width() as i32 - self.text_w) / 2).max(0) as f32;
                self.offset_y = ((context.matrix.height() as i32 - self.text_h) / 2).max(0) as f32;
            }
        }
    }

    fn render(&mut self, context: &mut EngineContext) {
        let matrix = &mut *context.matrix;
        let (r, g, b) = self.parsed_color;

        let start_x = self.offset_x.round() as i32;
        let start_y = self.offset_y.round() as i32;

        for char_pixels in &self.cached_pixels {
            for &(gx, gy) in char_pixels {
                matrix.set_pixel(start_x + gx, start_y + gy, r, g, b);
            }
        }
    }

    fn deactivate(&mut self) {}

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        self.text = config.get_string("text", "ArcadeMatrix");
        self.color = config.get_string("color", "#ffffff");
        self.size = config.get_int("size", 1) as u32;
        self.direction = ScrollDirection::parse(&config.get_string("direction", "rtl"));
        self.speed = config.get_int("speed", 50) as u32;
        self.reset_state(128.0, 64.0);
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
        available: true,
        unavailable_reason: None,
        schema: ConfigSchema {
            fields: vec![
                crate::core::engine_contract::ConfigField {
                    id: "text",
                    field_type: crate::core::engine_contract::ConfigType::String,
                    label: "Message Text",
                    description: "Text banner or message to display",
                    default_value: "ArcadeMatrix",
                    validation_policy: crate::core::engine_contract::ValidationPolicy::Accept,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "color",
                    field_type: crate::core::engine_contract::ConfigType::String,
                    label: "Text Color",
                    description: "Hex color code (#RRGGBB)",
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
                    description: "Scroll direction or static",
                    default_value: "rtl",
                    options: Some(vec![
                        crate::core::engine_contract::ConfigOption {
                            label: "Right to Left (RTL)",
                            value: "rtl",
                        },
                        crate::core::engine_contract::ConfigOption {
                            label: "Left to Right (LTR)",
                            value: "ltr",
                        },
                        crate::core::engine_contract::ConfigOption {
                            label: "Top to Bottom (TTB)",
                            value: "ttb",
                        },
                        crate::core::engine_contract::ConfigOption {
                            label: "Bottom to Top (BTT)",
                            value: "btt",
                        },
                        crate::core::engine_contract::ConfigOption {
                            label: "Static (No Scroll)",
                            value: "static",
                        },
                    ]),
                    validation_policy:
                        crate::core::engine_contract::ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                crate::core::engine_contract::ConfigField {
                    id: "speed",
                    field_type: crate::core::engine_contract::ConfigType::Integer,
                    label: "Scroll Speed (ms)",
                    description: "Lower = faster (ms per pixel update). Ignored when static.",
                    default_value: "50",
                    min_val: Some("10"),
                    max_val: Some("200"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_direction_parsing() {
        assert_eq!(ScrollDirection::parse("rtl"), ScrollDirection::Rtl);
        assert_eq!(ScrollDirection::parse("left"), ScrollDirection::Rtl);
        assert_eq!(ScrollDirection::parse("ltr"), ScrollDirection::Ltr);
        assert_eq!(ScrollDirection::parse("right"), ScrollDirection::Ltr);
        assert_eq!(ScrollDirection::parse("ttb"), ScrollDirection::Ttb);
        assert_eq!(ScrollDirection::parse("down"), ScrollDirection::Ttb);
        assert_eq!(ScrollDirection::parse("btt"), ScrollDirection::Btt);
        assert_eq!(ScrollDirection::parse("up"), ScrollDirection::Btt);
        assert_eq!(ScrollDirection::parse("static"), ScrollDirection::None);
        assert_eq!(ScrollDirection::parse("none"), ScrollDirection::None);
        assert_eq!(ScrollDirection::parse("invalid"), ScrollDirection::None);
    }

    #[test]
    fn test_message_engine_subpixel_movement() {
        let mut engine = MessageEngine::new();
        engine.speed = 50; // 50 ms per pixel -> 0.02 px/ms
        engine.direction = ScrollDirection::Rtl;
        engine.offset_x = 128.0;

        // Frame duration 16.6ms -> 16.6 / 50.0 = 0.332 px movement
        let dt = Duration::from_micros(16600);
        let dt_secs = dt.as_secs_f32();
        let dt_ms = if dt_secs > 0.1 {
            16.6
        } else {
            dt_secs * 1000.0
        };
        let step_ms = (engine.speed.max(1) as f32).max(1.0);
        let move_px = dt_ms / step_ms;

        assert!((move_px - 0.332).abs() < 0.01);
        engine.offset_x -= move_px;
        assert!((engine.offset_x - 127.668).abs() < 0.01);
        // After 3 frames (~50ms), should move ~1px
        assert_eq!(engine.offset_x.round() as i32, 128);
    }

    #[test]
    fn test_message_engine_clamped_dt() {
        // Large delta time (e.g. 5s stall or pause) must be clamped to 16.6ms to avoid teleporting
        let dt = Duration::from_secs(5);
        let dt_secs = dt.as_secs_f32();
        let dt_ms = if dt_secs > 0.1 {
            16.6
        } else {
            dt_secs * 1000.0
        };
        assert_eq!(dt_ms, 16.6);
    }

    #[test]
    fn test_message_engine_wraparound_rtl() {
        let mut engine = MessageEngine::new();
        engine.direction = ScrollDirection::Rtl;
        engine.text_w = 40;
        let mat_w = 128.0f32;
        engine.offset_x = -41.0; // Past left boundary (-text_w)

        if engine.offset_x < -(engine.text_w as f32) {
            engine.offset_x = mat_w;
        }
        assert_eq!(engine.offset_x, 128.0);
    }

    #[test]
    fn test_message_engine_wraparound_ltr() {
        let mut engine = MessageEngine::new();
        engine.direction = ScrollDirection::Ltr;
        engine.text_w = 40;
        let mat_w = 128.0f32;
        engine.offset_x = 129.0; // Past right boundary (mat_w)

        if engine.offset_x > mat_w {
            engine.offset_x = -(engine.text_w as f32);
        }
        assert_eq!(engine.offset_x, -40.0);
    }
}

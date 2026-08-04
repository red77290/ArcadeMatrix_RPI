use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::BaseRenderer;
use serde::{Deserialize, Serialize};

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
}

impl MessageEngine {
    pub fn new() -> Self {
        Self {
            base_renderer: BaseRenderer::new(),
            offset_x: 64.0,
        }
    }

    pub fn reset(&mut self, width: f32) {
        self.offset_x = width;
    }

    pub fn render(&mut self, matrix: &mut dyn MatrixBackend, payload: &MessagePayload) -> bool {
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
            0,
            payload.size,
            self.offset_x as i32,
            0,
            Some((r, g, b)),
            None,
        );

        finished
    }
}

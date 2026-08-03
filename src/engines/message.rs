use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::BaseRenderer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePayload {
    pub text: String,
    pub color: u32,
    pub size: u32,
    pub direction: String,
    pub speed: u32,
    #[serde(alias = "timeoutSeconds")]
    pub timeout_seconds: u32,
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

    pub fn render(&mut self, matrix: &mut dyn MatrixBackend, payload: &MessagePayload) {
        let move_px = 33.0 / payload.speed.max(1) as f32; // Assuming ~33ms frame time
        self.offset_x -= move_px;

        let font = self.base_renderer.font();
        let (pixels, _, _) = font.get_pixel_map(&payload.text, payload.size as f32);
        let mut text_w = 0;
        for char_pixels in &pixels {
            for &(px, _) in char_pixels {
                text_w = text_w.max(px + 1);
            }
        }

        if self.offset_x < -(text_w as f32) {
            self.offset_x = matrix.width() as f32;
        }

        // Decode RGB565 integer sent from Web UI
        let r = ((payload.color >> 11) & 0x1F) as u8 * 8;
        let g = ((payload.color >> 5) & 0x3F) as u8 * 4;
        let b = (payload.color & 0x1F) as u8 * 8;

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
    }
}

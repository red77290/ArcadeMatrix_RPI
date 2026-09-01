use crate::core::engine_contract::{
    Capabilities, ConfigSchema, Engine, EngineConfig, EngineContext, EngineDescriptor, EngineError,
    EngineMetadata, Requirements,
};
use crate::core::matrix::MatrixBackend;
use image::RgbImage;
use linkme::distributed_slice;

pub struct MarqueeEngine {
    pub image: Option<RgbImage>,
}

impl MarqueeEngine {
    pub fn new() -> Self {
        Self { image: None }
    }

    pub fn render_image(&self, matrix: &mut dyn MatrixBackend, image: &RgbImage) {
        let mw = matrix.width() as u32;
        let mh = matrix.height() as u32;
        let iw = image.width();
        let ih = image.height();

        if iw == 0 || ih == 0 {
            return;
        }

        let scale_x = mw / iw;
        let scale_y = mh / ih;
        let scale = scale_x.min(scale_y);

        if scale > 1 {
            let new_w = iw * scale;
            let new_h = ih * scale;
            let scaled =
                image::imageops::resize(image, new_w, new_h, image::imageops::FilterType::Nearest);
            let offset_x = (mw - new_w) / 2;
            let offset_y = (mh - new_h) / 2;
            matrix.draw_image(&scaled, offset_x as i32, offset_y as i32);
        } else {
            let offset_x = if mw > iw { (mw - iw) / 2 } else { 0 };
            let offset_y = if mh > ih { (mh - ih) / 2 } else { 0 };
            matrix.draw_image(image, offset_x as i32, offset_y as i32);
        }
    }
}

impl Engine for MarqueeEngine {
    fn initialize(
        &mut self,
        _context: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        let path = config.get_string("image_path", "");
        if !path.is_empty() {
            if let Ok(img) = image::open(&path) {
                self.image = Some(img.to_rgb8());
            }
        }
        Ok(())
    }

    fn activate(&mut self) {}
    fn update(&mut self, _context: &mut EngineContext) {}

    fn render(&mut self, context: &mut EngineContext) {
        if let Some(img) = &self.image {
            self.render_image(&mut *context.matrix, img);
        }
    }

    fn deactivate(&mut self) {}
    fn on_config_changed(&mut self, _config: &dyn EngineConfig) {}

    fn allows_overlay(&self) -> bool {
        false
    }

    fn allow_rotation(&self) -> bool {
        false
    }
}

#[distributed_slice(crate::core::registry::ENGINES)]
fn register_marquee_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "marquee",
            name: "MarqueeEngine",
            category: "image",
            version: crate::core::build_info::VERSION,
        },
        capabilities: Capabilities {
            allow_rotation: false,
            allows_overlay: false,
            ..Default::default()
        },
        requirements: Requirements::default(),
        available: true,
        unavailable_reason: None,
        schema: ConfigSchema { fields: vec![] },
        factory: || -> Box<dyn crate::core::engine_contract::Engine> {
            Box::new(MarqueeEngine::new())
        },
    }
}

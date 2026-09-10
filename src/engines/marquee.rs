use crate::core::engine_contract::{
    Capabilities, ConfigField, ConfigSchema, ConfigType, Engine, EngineConfig, EngineContext,
    EngineDescriptor, EngineError, EngineMetadata, Requirements, ValidationPolicy,
};
use crate::core::matrix::MatrixBackend;
use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, RgbImage};
use linkme::distributed_slice;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub struct MarqueeEngine {
    pub file_path: String,
    pub speed_multiplier: f32,
    pub fit_mode: String,
    pub frames: Vec<(RgbImage, Duration)>,
    pub frame_index: usize,
    pub frame_elapsed: Duration,
    pub last_update: Option<Instant>,
}

impl MarqueeEngine {
    pub fn new() -> Self {
        Self {
            file_path: "data/marquees/marquee.gif".to_string(),
            speed_multiplier: 1.0,
            fit_mode: "fit".to_string(),
            frames: Vec::new(),
            frame_index: 0,
            frame_elapsed: Duration::ZERO,
            last_update: None,
        }
    }

    pub fn load_file<P: AsRef<Path>>(&mut self, path: P) -> bool {
        let p = path.as_ref();
        if !p.exists() {
            return false;
        }
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if ext == "gif" {
            let file = match File::open(p) {
                Ok(f) => f,
                Err(_) => return false,
            };
            let reader = std::io::BufReader::new(file);
            let decoder = match GifDecoder::new(reader) {
                Ok(d) => d,
                Err(_) => return false,
            };
            let mut frames = Vec::new();
            if let Ok(decoded_frames) = decoder.into_frames().collect_frames() {
                for frame in decoded_frames {
                    let delay = Duration::from(frame.delay());
                    let rgba_img = frame.into_buffer();
                    let rgb_img = image::DynamicImage::ImageRgba8(rgba_img).into_rgb8();
                    frames.push((rgb_img, delay));
                }
            }
            if !frames.is_empty() {
                self.frames = frames;
                self.frame_index = 0;
                self.frame_elapsed = Duration::ZERO;
                self.last_update = Some(Instant::now());
                return true;
            }
        } else if let Ok(img) = image::open(p) {
            self.frames = vec![(img.to_rgb8(), Duration::from_secs(3600))];
            self.frame_index = 0;
            self.frame_elapsed = Duration::ZERO;
            self.last_update = Some(Instant::now());
            return true;
        }
        false
    }

    pub fn resolve_file(&self) -> Option<PathBuf> {
        if !self.file_path.is_empty() {
            let p = PathBuf::from(&self.file_path);
            if p.exists() {
                return Some(p);
            }
        }
        let candidates = [
            "data/marquees/marquee.gif",
            "data/marquees/marquee.png",
            "data/marquees/marquee.jpg",
            "data/marquees/custom_marquee.gif",
            "data/marquees/custom_marquee.png",
            "data/marquees/custom_marquee.jpg",
        ];
        for c in &candidates {
            let p = PathBuf::from(c);
            if p.exists() {
                return Some(p);
            }
        }
        None
    }

    pub fn render_image(&self, matrix: &mut dyn MatrixBackend, image: &RgbImage) {
        let mw = matrix.width() as u32;
        let mh = matrix.height() as u32;
        let iw = image.width();
        let ih = image.height();

        if iw == 0 || ih == 0 {
            return;
        }

        match self.fit_mode.to_lowercase().as_str() {
            "stretch" => {
                let scaled =
                    image::imageops::resize(image, mw, mh, image::imageops::FilterType::Nearest);
                matrix.draw_image(&scaled, 0, 0);
            }
            "center" => {
                let offset_x = (mw as i32 - iw as i32) / 2;
                let offset_y = (mh as i32 - ih as i32) / 2;
                matrix.draw_image(image, offset_x, offset_y);
            }
            _ => {
                // "fit" (default): scale to fit within (mw, mh) preserving aspect ratio
                let ratio_w = mw as f32 / iw as f32;
                let ratio_h = mh as f32 / ih as f32;
                let ratio = ratio_w.min(ratio_h);
                let new_w = ((iw as f32 * ratio).round() as u32).clamp(1, mw);
                let new_h = ((ih as f32 * ratio).round() as u32).clamp(1, mh);
                let scaled = image::imageops::resize(
                    image,
                    new_w,
                    new_h,
                    image::imageops::FilterType::Nearest,
                );
                let offset_x = (mw as i32 - new_w as i32) / 2;
                let offset_y = (mh as i32 - new_h as i32) / 2;
                matrix.draw_image(&scaled, offset_x, offset_y);
            }
        }
    }
}

impl Engine for MarqueeEngine {
    fn initialize(
        &mut self,
        _context: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        self.file_path = config.get_string("file_path", "data/marquees/custom_marquee.gif");
        self.speed_multiplier = config
            .get_string("speed_multiplier", "1.0")
            .parse::<f32>()
            .unwrap_or(1.0)
            .clamp(0.25, 3.0);
        self.fit_mode = config.get_string("fit_mode", "fit");
        if let Some(path) = self.resolve_file() {
            self.load_file(path);
        }
        Ok(())
    }

    fn activate(&mut self) {
        if let Some(path) = self.resolve_file() {
            self.load_file(path);
        }
        self.last_update = Some(Instant::now());
    }

    fn update(&mut self, _context: &mut EngineContext) {
        if self.frames.len() <= 1 {
            return;
        }
        let now = Instant::now();
        let last = self.last_update.unwrap_or(now);
        let dt = now.duration_since(last);
        self.last_update = Some(now);

        let speed = if self.speed_multiplier > 0.0 {
            self.speed_multiplier
        } else {
            1.0
        };
        self.frame_elapsed += dt.mul_f32(speed);

        let current_delay = self.frames[self.frame_index].1;
        if self.frame_elapsed >= current_delay {
            self.frame_elapsed = Duration::ZERO;
            self.frame_index = (self.frame_index + 1) % self.frames.len();
        }
    }

    fn render(&mut self, context: &mut EngineContext) {
        context.matrix.clear();
        let lock = context.config.image_obj.lock();
        if let Some(img) = lock.as_ref() {
            self.render_image(&mut *context.matrix, img);
        } else if !self.frames.is_empty() {
            let img = &self.frames[self.frame_index].0;
            self.render_image(&mut *context.matrix, img);
        }
    }

    fn deactivate(&mut self) {}

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        self.file_path = config.get_string("file_path", "data/marquees/custom_marquee.gif");
        self.speed_multiplier = config
            .get_string("speed_multiplier", "1.0")
            .parse::<f32>()
            .unwrap_or(1.0)
            .clamp(0.25, 3.0);
        self.fit_mode = config.get_string("fit_mode", "fit");
        if let Some(path) = self.resolve_file() {
            self.load_file(path);
        }
    }

    fn allows_overlay(&self) -> bool {
        false
    }

    fn allow_rotation(&self) -> bool {
        true
    }

    fn is_realtime(&self) -> bool {
        true
    }

    fn self_paced(&self) -> bool {
        false
    }
}

#[distributed_slice(crate::core::registry::ENGINES)]
fn register_marquee_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "marquee",
            name: "Gameroom Marquee",
            category: "arcade",
            version: crate::core::build_info::VERSION,
        },
        capabilities: Capabilities {
            allow_rotation: true,
            allows_overlay: false,
            realtime: true,
            supports_128x32: true,
            supports_256x64: true,
            interruptible: true,
        },
        requirements: Requirements::default(),
        available: true,
        unavailable_reason: None,
        schema: ConfigSchema {
            fields: vec![
                ConfigField {
                    id: "file_path",
                    field_type: ConfigType::FileAsset,
                    label: "Marquee File",
                    description: "Path to marquee GIF or image file",
                    default_value: "data/marquees/marquee.gif",
                    options: Some(vec![crate::core::engine_contract::ConfigOption {
                        label: "Allowed Extensions",
                        value: ".gif,.png,.jpg,.jpeg",
                    }]),
                    options_endpoint: Some("/api/upload?target=marquee"),
                    validation_policy: ValidationPolicy::Accept,
                    ..Default::default()
                },
                ConfigField {
                    id: "speed_multiplier",
                    field_type: ConfigType::Float,
                    label: "Speed Multiplier",
                    description: "Animation playback speed factor",
                    default_value: "1.0",
                    min_val: Some("0.25"),
                    max_val: Some("3.0"),
                    step: Some("0.25"),
                    validation_policy: ValidationPolicy::Clamp,
                    ..Default::default()
                },
                ConfigField {
                    id: "fit_mode",
                    field_type: ConfigType::Options,
                    label: "Fit Mode",
                    description: "Display scaling mode (fit, center, stretch)",
                    default_value: "fit",
                    options: Some(vec![
                        crate::core::engine_contract::ConfigOption {
                            label: "Fit Screen",
                            value: "fit",
                        },
                        crate::core::engine_contract::ConfigOption {
                            label: "Center (1:1)",
                            value: "center",
                        },
                        crate::core::engine_contract::ConfigOption {
                            label: "Stretch",
                            value: "stretch",
                        },
                    ]),
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
            ],
        },
        factory: || -> Box<dyn crate::core::engine_contract::Engine> {
            Box::new(MarqueeEngine::new())
        },
    }
}

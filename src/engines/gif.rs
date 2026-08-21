use crate::core::engine_contract::{Engine, EngineConfig, EngineContext, EngineError, EngineDescriptor, EngineMetadata, Capabilities, Requirements, ConfigSchema, EngineFactory};
use linkme::distributed_slice;
use std::time::Instant;
use crate::core::matrix::MatrixBackend;
use image::RgbImage;
use rand::seq::SliceRandom;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct GifEngine {
    current_gif_path: Option<PathBuf>,
    /// Each frame stores the image and its display duration
    frames: Vec<(RgbImage, Duration)>,
    frame_index: usize,
    last_played_gif: Option<PathBuf>,
    /// Accumulated time since last frame advance
    frame_elapsed: Duration,
    target_width: u32,
    target_height: u32,
    loop_count: u32,
    /// Index of the frame last actually drawn to the matrix (for swap-skip)
    last_drawn_index: Option<usize>,
    last_update: Option<Instant>,
    playlists: Vec<String>,
}

impl GifEngine {
    pub fn new(target_width: u32, target_height: u32) -> Self {
        Self {
            current_gif_path: None,
            frames: Vec::new(),
            frame_index: 0,
            last_played_gif: None,
            frame_elapsed: Duration::ZERO,
            target_width,
            target_height,
            loop_count: 0,
            last_drawn_index: None,
            last_update: None,
            playlists: Vec::new(),
        }
    }

    pub fn load_gif<P: AsRef<Path>>(&mut self, path: P) -> bool {
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(_) => return false,
        };
        let reader = std::io::BufReader::new(file);

        let decoder = match image::codecs::gif::GifDecoder::new(reader) {
            Ok(d) => d,
            Err(_) => return false,
        };

        let mut frames = Vec::new();
        use image::AnimationDecoder;
        if let Ok(decoded_frames) = decoder.into_frames().collect_frames() {
            for frame in decoded_frames {
                // `frame.into_buffer()` returns an RgbaImage. We convert to RgbImage.
                let rgba_img = frame.clone().into_buffer();
                let rgb_img = image::DynamicImage::ImageRgba8(rgba_img).into_rgb8();

                // Resize to target dimensions using Nearest neighbor for speed and retro look
                let resized = image::imageops::resize(
                    &rgb_img,
                    self.target_width,
                    self.target_height,
                    image::imageops::FilterType::Nearest,
                );

                // delay in image crate is a Delay struct. We can extract numerator/denominator.
                let (num, den) = frame.delay().numer_denom_ms();
                let ms = if den == 0 { 0 } else { num / den };
                let delay = Duration::from_millis(if ms == 0 { 50 } else { ms as u64 });

                frames.push((resized, delay));
            }
        }

        if !frames.is_empty() {
            let pb = path.as_ref().to_path_buf();
            self.last_played_gif = Some(pb.clone());
            self.current_gif_path = Some(pb);
            self.frames = frames;
            self.frame_index = 0;
            self.frame_elapsed = Duration::ZERO;
            self.loop_count = 0;
            self.last_drawn_index = None;
            true
        } else {
            false
        }
    }

    pub fn play_random_playlist_gif(&mut self, selected_playlists: &[String]) -> bool {
        let mut valid_files = Vec::new();

        if !selected_playlists.is_empty() {
            for p_str in selected_playlists {
                // Sanitize the path string to handle any stray quotes or missing prefixes
                let mut cleaned = p_str.replace("\"", "").trim().to_string();
                if !cleaned.starts_with("gifs/") && !cleaned.starts_with("data/") {
                    cleaned = format!("gifs/{}", cleaned);
                }

                let p = Path::new(&cleaned);
                if p.is_dir() {
                    if let Ok(entries) = std::fs::read_dir(p) {
                        for entry in entries.flatten() {
                            let fname = entry.file_name().to_string_lossy().to_string();
                            if fname.to_lowercase().ends_with(".gif") && !fname.starts_with("._") {
                                valid_files.push(entry.path());
                            }
                        }
                    }
                }
            }
        }

        if valid_files.is_empty() {
            // Fallback: scan all subdirectories of /gifs/
            if let Ok(entries) = std::fs::read_dir("gifs") {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        if let Ok(sub_entries) = std::fs::read_dir(entry.path()) {
                            for sub in sub_entries.flatten() {
                                let fname = sub.file_name().to_string_lossy().to_string();
                                if fname.to_lowercase().ends_with(".gif")
                                    && !fname.starts_with("._")
                                {
                                    valid_files.push(sub.path());
                                }
                            }
                        }
                    }
                }
            }
        }

        if valid_files.is_empty() {
            return false;
        }

        let mut rng = rand::thread_rng();
        // Avoid picking exact same GIF twice if multiple available
        if valid_files.len() > 1 {
            if let Some(ref last) = self.last_played_gif {
                valid_files.retain(|p| p != last);
            }
        }

        if let Some(chosen) = valid_files.choose(&mut rng) {
            return self.load_gif(chosen);
        }

        false
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Renders the current GIF frame to the matrix.
    /// Call this every iteration; it internally tracks elapsed time and
    /// advances to the next frame only when the frame's own delay has elapsed.
    /// `dt` = time elapsed since the last render call.
    ///
    /// Returns `true` if a new frame was actually drawn (i.e. the displayed
    /// image changed), `false` if the current frame is identical to the one
    /// already on the panel. Callers can skip the (memory-bus heavy) canvas
    /// swap when this returns `false`, which drastically reduces DDR traffic
    /// and prevents Wi-Fi/SDIO DMA starvation during playback.
    pub fn render_next_frame(&mut self, matrix: &mut dyn MatrixBackend, dt: Duration) -> bool {
        if self.frames.is_empty() {
            return false;
        }

        self.frame_elapsed += dt;
        let (_, delay) = self.frames[self.frame_index];

        while self.frame_elapsed >= delay {
            self.frame_elapsed -= delay;
            self.frame_index += 1;
            if self.frame_index >= self.frames.len() {
                self.frame_index = 0;
                self.loop_count += 1;
            }
        }

        // Skip redraw + swap when the frame hasn't advanced since last draw.
        if self.last_drawn_index == Some(self.frame_index) {
            return false;
        }

        let (ref img, _) = self.frames[self.frame_index];
        matrix.draw_image(img, 0, 0);
        self.last_drawn_index = Some(self.frame_index);
        true
    }

    pub fn has_finished_loops(&self, target_loops: u32) -> bool {
        self.loop_count >= target_loops
    }

    /// Force-redraw the current frame to the matrix without advancing.
    /// Used when an overlay (e.g. fighters) forces a swap every iteration and
    /// the gif image must be present on the freshly-cleared canvas.
    pub fn redraw_current(&mut self, matrix: &mut dyn MatrixBackend) {
        if let Some((ref img, _)) = self.frames.get(self.frame_index) {
            matrix.draw_image(img, 0, 0);
            self.last_drawn_index = Some(self.frame_index);
        }
    }
}

impl Engine for GifEngine {
    fn initialize(&mut self, _context: &mut EngineContext, config: &dyn EngineConfig) -> Result<(), EngineError> {
        let playlists_str = config.get_string("playlists", "");
        if !playlists_str.is_empty() {
            self.playlists = playlists_str.split(',').map(|s| s.trim().to_string()).collect();
        } else {
            self.playlists = Vec::new();
        }
        Ok(())
    }

    fn activate(&mut self) {
        self.play_random_playlist_gif(&self.playlists.clone());
        self.last_update = Some(Instant::now());
    }

    fn update(&mut self, _context: &mut EngineContext) {
        let now = Instant::now();
        if let Some(last) = self.last_update {
            self.frame_elapsed += now.duration_since(last);
        }
        self.last_update = Some(now);

        if self.frames.is_empty() {
            return;
        }

        let (_, delay) = self.frames[self.frame_index];
        while self.frame_elapsed >= delay {
            self.frame_elapsed -= delay;
            self.frame_index += 1;
            if self.frame_index >= self.frames.len() {
                self.frame_index = 0;
                self.loop_count += 1;
            }
        }
    }

    fn render(&mut self, context: &mut EngineContext) {
        if self.frames.is_empty() {
            return;
        }
        let (ref img, _) = self.frames[self.frame_index];
        context.matrix.draw_image(img, 0, 0);
    }

    fn deactivate(&mut self) {
        self.frames.clear();
        self.current_gif_path = None;
    }

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        let playlists_str = config.get_string("playlists", "");
        if !playlists_str.is_empty() {
            self.playlists = playlists_str.split(',').map(|s| s.trim().to_string()).collect();
        } else {
            self.playlists = Vec::new();
        }
    }

    fn is_finished(&self) -> bool {
        self.has_finished_loops(1) || self.is_empty()
    }
}

#[distributed_slice(crate::core::registry::ENGINES)]
fn register_gif_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "gifs",
            name: "GifEngine",
            category: "media",
            version: "1.0.0",
        },
        capabilities: Capabilities::default(),
        requirements: Requirements::default(),
        schema: ConfigSchema { fields: vec![] },
        factory: || -> Box<dyn crate::core::engine_contract::Engine> {
            Box::new(GifEngine::new(64, 32))
        },
    }
}

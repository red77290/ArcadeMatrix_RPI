use crate::core::engine_contract::{
    Capabilities, ConfigSchema, Engine, EngineConfig, EngineContext, EngineDescriptor, EngineError,
    EngineMetadata, Requirements,
};
use crate::core::matrix::MatrixBackend;
use crate::core::types::DisplayGeometry;
use image::RgbImage;
use linkme::distributed_slice;
use rand::seq::SliceRandom;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::time::Instant;

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
    geometry: DisplayGeometry,
    loop_count: u32,
    /// Index of the frame last actually drawn to the matrix (for swap-skip)
    last_drawn_index: Option<usize>,
    last_update: Option<Instant>,
    playlists: Vec<String>,
    /// Number of GIFs to play (one full loop each) before the engine reports
    /// `is_finished()` and the rotation advances. Fed from the rotation entry's
    /// numeric value via `set_rotation_budget`.
    target_count: u32,
    /// How many GIFs have completed a full loop in the current rotation visit.
    gifs_played: u32,
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
            geometry: DisplayGeometry::new(target_width, target_height, 0, 0),
            loop_count: 0,
            last_drawn_index: None,
            last_update: None,
            playlists: Vec::new(),
            target_count: 1,
            gifs_played: 0,
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

                let src_w = rgb_img.width();
                let src_h = rgb_img.height();

                // Compute uniform scaling preserving aspect ratio (zero distortion/stretching)
                let (new_w, new_h) = if src_w > 0 && src_h > 0 {
                    let scale_x = self.target_width as f32 / src_w as f32;
                    let scale_y = self.target_height as f32 / src_h as f32;
                    let scale = scale_x.min(scale_y);
                    let nw = ((src_w as f32 * scale).round() as u32).clamp(1, self.target_width);
                    let nh = ((src_h as f32 * scale).round() as u32).clamp(1, self.target_height);
                    (nw, nh)
                } else {
                    (self.target_width, self.target_height)
                };

                let resized = image::imageops::resize(
                    &rgb_img,
                    new_w,
                    new_h,
                    image::imageops::FilterType::Nearest,
                );

                // Create a canvas filled with black and center the resized frame (letterboxed)
                let mut canvas = RgbImage::new(self.target_width, self.target_height);
                let offset_x = (self.target_width.saturating_sub(new_w)) / 2;
                let offset_y = (self.target_height.saturating_sub(new_h)) / 2;
                image::imageops::overlay(&mut canvas, &resized, offset_x as i64, offset_y as i64);

                let (num, den) = frame.delay().numer_denom_ms();
                let ms = if den == 0 { 0 } else { num / den };
                let ms_clamped = if ms == 0 {
                    100 // Standard GIF fallback for 0ms delay headers
                } else if ms < 20 {
                    20 // Cap at 50fps max to avoid stuttering
                } else {
                    ms
                };
                let delay = Duration::from_millis(ms_clamped as u64);

                frames.push((canvas, delay));
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

    pub fn is_tate(&self) -> bool {
        self.target_height > self.target_width
            || self.target_width < 48
            || self.target_height > (self.target_width * 3) / 2
            || self.geometry.layout_class == crate::core::types::LayoutClass::Portrait
            || self.geometry.layout_class == crate::core::types::LayoutClass::Tall
            || self.geometry.rotation == 1
            || self.geometry.rotation == 3
    }

    pub fn get_candidate_roots(is_vertical: bool) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/pi".to_string());

        // STRICT SEPARATION:
        // In vertical (Tate) mode: ONLY vertical roots ("gifs_tate", "gifs/tate", "tate")
        // In horizontal (Yoko) mode: ONLY horizontal roots ("gifs", "gifs/yoko", "yoko")
        // NEVER mix horizontal and vertical roots!
        let sub_dirs: &[&str] = if is_vertical {
            &["gifs_tate", "gifs/tate", "tate"]
        } else {
            &["gifs", "gifs/yoko", "yoko"]
        };

        let base_prefixes = [
            "",
            "/",
            "data/",
            "release/sdCard/",
            "../",
            "../ArcadeMatrix/release/sdCard/",
            "/opt/arcadematrix/",
            &format!("{}/", home),
            &format!("{}/ArcadeMatrix_RPi/", home),
            &format!("{}/ArcadeMatrix/", home),
            &format!("{}/ArcadeMatrix/release/sdCard/", home),
            "/media/",
            "/mnt/",
        ];

        for base in &base_prefixes {
            for sub in sub_dirs {
                let candidate = if base.is_empty() {
                    PathBuf::from(sub)
                } else if base.starts_with('/') {
                    Path::new(base).join(sub)
                } else {
                    PathBuf::from(format!("{}{}", base, sub))
                };
                if candidate.is_dir() && !roots.contains(&candidate) {
                    roots.push(candidate);
                }
            }
        }
        roots
    }

    pub fn play_random_playlist_gif(&mut self, selected_playlists: &[String]) -> bool {
        let is_vertical = self.is_tate();
        let candidate_roots = Self::get_candidate_roots(is_vertical);
        let mut valid_files = Vec::new();

        if !selected_playlists.is_empty() {
            for p_str in selected_playlists {
                let raw_clean = p_str.replace('\"', "").trim().to_string();
                let trimmed = raw_clean.trim_start_matches('/').to_string();

                if trimmed.is_empty()
                    || trimmed == "all"
                    || trimmed == "gifs"
                    || trimmed == "gifs_tate"
                    || trimmed == "tate"
                    || trimmed == "yoko"
                {
                    // Scan all candidate roots for current orientation
                    for root in &candidate_roots {
                        Self::scan_folder_recursive(root, &mut valid_files);
                    }
                    continue;
                }

                // If user selected a specific sub-folder (e.g. "Arcade" or "gifs/Arcade" or "gifs_tate/Arcade")
                let sub_folder = trimmed
                    .trim_start_matches("gifs_tate/")
                    .trim_start_matches("gifs/")
                    .trim_start_matches("tate/")
                    .trim_start_matches("yoko/")
                    .trim_start_matches("data/gifs_tate/")
                    .trim_start_matches("data/gifs/")
                    .trim_start_matches('/')
                    .to_string();

                let mut found = false;
                for root in &candidate_roots {
                    let cand1 = root.join(&trimmed);
                    if cand1.is_dir() {
                        Self::scan_folder_recursive(&cand1, &mut valid_files);
                        found = true;
                        break;
                    }
                    let cand2 = root.join(&sub_folder);
                    if cand2.is_dir() {
                        Self::scan_folder_recursive(&cand2, &mut valid_files);
                        found = true;
                        break;
                    }
                }

                if !found {
                    let p_raw = Path::new(&raw_clean);
                    if p_raw.is_dir() {
                        let p_str_lower = raw_clean.to_lowercase();
                        let matches_orientation = if is_vertical {
                            p_str_lower.contains("tate") || !p_str_lower.contains("gifs/")
                        } else {
                            !p_str_lower.contains("tate")
                        };
                        if matches_orientation {
                            Self::scan_folder_recursive(p_raw, &mut valid_files);
                        }
                    }
                }
            }
        }

        // Fallback: If no files found from specific playlists, scan all available roots for THIS orientation
        if valid_files.is_empty() {
            for root in &candidate_roots {
                Self::scan_folder_recursive(root, &mut valid_files);
                if !valid_files.is_empty() {
                    break;
                }
            }
        }

        // STRICT SEPARATION:
        // "en vertical, s'il n'y a pas de gifs vertical tu ne joue rien, meme chose à l'horizontal"
        // ZERO fallback to alternate orientation! If no files match the current orientation, return false and play nothing!
        if valid_files.is_empty() {
            self.frames.clear();
            self.current_gif_path = None;
            return false;
        }

        let mut rng = rand::thread_rng();
        if valid_files.len() > 1 {
            if let Some(ref last) = self.last_played_gif {
                valid_files.retain(|p| p != last);
            }
        }

        if let Some(chosen) = valid_files.choose(&mut rng) {
            self.load_gif(chosen)
        } else {
            self.frames.clear();
            self.current_gif_path = None;
            false
        }
    }

    fn scan_folder(dir: &Path, out: &mut Vec<PathBuf>) {
        Self::scan_folder_recursive(dir, out);
    }

    fn scan_folder_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    Self::scan_folder_recursive(&p, out);
                } else {
                    let fname = entry.file_name().to_string_lossy().to_string();
                    if fname.to_lowercase().ends_with(".gif") && !fname.starts_with("._") {
                        out.push(p);
                    }
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    fn apply_config(&mut self, config: &dyn EngineConfig) {
        let p1 = config.get_string("playlists", "");
        let p2 = config.get_string("folder", "");
        let p3 = config.get_string("variant", "");
        let playlists_str = if !p1.is_empty() {
            p1
        } else if !p2.is_empty() {
            p2
        } else if !p3.is_empty() {
            p3
        } else {
            "all".to_string()
        };
        self.playlists = if playlists_str.is_empty() || playlists_str == "all" {
            Vec::new()
        } else {
            playlists_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        };

        let count = config.get_int("gifs_count", 0);
        if count > 0 {
            self.target_count = count as u32;
        }
    }

    pub fn draw_current_frame(&mut self, matrix: &mut dyn MatrixBackend) -> bool {
        if self.frames.is_empty() {
            return false;
        }

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

    pub fn redraw_current(&mut self, matrix: &mut dyn MatrixBackend) {
        if let Some((ref img, _)) = self.frames.get(self.frame_index) {
            matrix.draw_image(img, 0, 0);
            self.last_drawn_index = Some(self.frame_index);
        }
    }
}

impl Engine for GifEngine {
    fn initialize(
        &mut self,
        context: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        self.target_width = context.matrix.width();
        self.target_height = context.matrix.height();
        self.geometry = DisplayGeometry::new(self.target_width, self.target_height, 0, 0);
        self.apply_config(config);
        Ok(())
    }

    fn activate(&mut self) {
        self.gifs_played = 0;
        self.loop_count = 0;
        self.play_random_playlist_gif(&self.playlists.clone());
        self.last_update = Some(Instant::now());
    }

    fn is_realtime(&self) -> bool {
        true
    }

    fn update(&mut self, context: &mut EngineContext) {
        let w = context.matrix.width();
        let h = context.matrix.height();
        if self.target_width != w || self.target_height != h {
            self.target_width = w;
            self.target_height = h;
            self.geometry =
                DisplayGeometry::new(w, h, self.geometry.rotation, self.geometry.version);
            self.play_random_playlist_gif(&self.playlists.clone());
        }

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

        if self.loop_count >= 1 {
            self.loop_count = 0;
            self.gifs_played += 1;
            self.play_random_playlist_gif(&self.playlists.clone());
            self.last_update = Some(Instant::now());
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

    fn on_display_geometry_changed(&mut self, geometry: &crate::core::types::DisplayGeometry) {
        let prev_tate = self.is_tate();
        self.target_width = geometry.logical_width;
        self.target_height = geometry.logical_height;
        self.geometry = *geometry;
        if self.is_tate() != prev_tate {
            self.play_random_playlist_gif(&self.playlists.clone());
        }
    }

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        self.apply_config(config);
    }

    fn set_rotation_budget(&mut self, budget: u32) {
        self.target_count = budget.max(1);
    }

    fn self_paced(&self) -> bool {
        true
    }

    fn is_finished(&self) -> bool {
        self.gifs_played >= self.target_count || self.is_empty()
    }
}

#[distributed_slice(crate::core::registry::ENGINES)]
fn register_gif_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "gifs",
            name: "GifPlayer",
            category: "media",
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
            fields: vec![crate::core::engine_contract::ConfigField {
                id: "playlists",
                field_type: crate::core::engine_contract::ConfigType::String,
                label: "Playlists",
                description: "Comma-separated active playlists",
                default_value: "",
                validation_policy: crate::core::engine_contract::ValidationPolicy::Accept,
                options_endpoint: Some("/api/playlists"),
                multiple: true,
                ..Default::default()
            }],
        },
        factory: || -> Box<dyn crate::core::engine_contract::Engine> {
            Box::new(GifEngine::new(64, 32))
        },
    }
}

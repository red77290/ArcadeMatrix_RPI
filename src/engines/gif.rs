use crate::core::matrix::MatrixBackend;
use gif::Decoder;
use image::{Rgb, RgbImage};
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
}

impl GifEngine {
    pub fn new() -> Self {
        Self {
            current_gif_path: None,
            frames: Vec::new(),
            frame_index: 0,
            last_played_gif: None,
            frame_elapsed: Duration::ZERO,
        }
    }

    pub fn load_gif<P: AsRef<Path>>(&mut self, path: P) -> bool {
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(_) => return false,
        };

        let mut decoder = match Decoder::new(file) {
            Ok(d) => d,
            Err(_) => return false,
        };

        let mut frames = Vec::new();
        while let Ok(Some(frame)) = decoder.read_next_frame() {
            let width = frame.width as u32;
            let height = frame.height as u32;
            let mut img = RgbImage::new(width, height);

            for (i, pixel) in frame.buffer.chunks_exact(4).enumerate() {
                let x = (i as u32) % width;
                let y = (i as u32) / width;
                img.put_pixel(x, y, Rgb([pixel[0], pixel[1], pixel[2]]));
            }

            // frame.delay is in units of 10ms (centiseconds).
            // Clamp to at least 50ms for broken/zero-delay GIFs.
            let delay_cs = frame.delay;
            let delay = Duration::from_millis(if delay_cs == 0 {
                5
            } else {
                delay_cs as u64 * 10
            });

            frames.push((img, delay));
        }

        if !frames.is_empty() {
            let pb = path.as_ref().to_path_buf();
            self.last_played_gif = Some(pb.clone());
            self.current_gif_path = Some(pb);
            self.frames = frames;
            self.frame_index = 0;
            self.frame_elapsed = Duration::ZERO;
            true
        } else {
            false
        }
    }

    pub fn play_random_playlist_gif(&mut self, selected_playlists: &[String]) -> bool {
        let mut valid_files = Vec::new();

        if !selected_playlists.is_empty() {
            for p_str in selected_playlists {
                let p = Path::new(p_str);
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

    /// Renders the current GIF frame to the matrix.
    /// Call this every iteration; it internally tracks elapsed time and
    /// advances to the next frame only when the frame's own delay has elapsed.
    /// `dt` = time elapsed since the last render call.
    pub fn render_next_frame(&mut self, matrix: &mut dyn MatrixBackend, dt: Duration) {
        if self.frames.is_empty() {
            return;
        }

        self.frame_elapsed += dt;
        let (_, delay) = self.frames[self.frame_index];

        if self.frame_elapsed >= delay {
            self.frame_elapsed = Duration::ZERO;
            self.frame_index = (self.frame_index + 1) % self.frames.len();
        }

        let (ref img, _) = self.frames[self.frame_index];
        matrix.draw_image(img, 0, 0);
    }
}

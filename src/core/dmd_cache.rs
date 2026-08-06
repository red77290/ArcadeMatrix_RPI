use parking_lot::Mutex;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::info;

pub struct DmdCache {
    cache_dir: PathBuf,
    negative_cache: Mutex<HashSet<String>>,
    http_client: reqwest::blocking::Client,
}

impl DmdCache {
    pub fn new<P: AsRef<Path>>(cache_dir: P) -> Self {
        let path = cache_dir.as_ref().to_path_buf();
        let _ = fs::create_dir_all(&path);
        let http_client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());

        Self {
            cache_dir: path,
            negative_cache: Mutex::new(HashSet::new()),
            http_client,
        }
    }

    pub fn get_cached_path(&self, system: &str, game: &str) -> Option<PathBuf> {
        let key = format!("{}/{}", system, game);
        if self.negative_cache.lock().contains(&key) {
            return None;
        }

        let local_filename = format!("{}_{}.png", system, game);
        let local_path = self.cache_dir.join(&local_filename);

        if local_path.exists() {
            return Some(local_path);
        }
        None
    }

    pub fn download_marquee(&self, system: &str, game: &str) -> Option<PathBuf> {
        let key = format!("{}/{}", system, game);
        if self.negative_cache.lock().contains(&key) {
            return None;
        }

        let local_filename = format!("{}_{}.png", system, game);
        let local_path = self.cache_dir.join(&local_filename);

        if local_path.exists() {
            return Some(local_path);
        }

        let pixelcade_system = match system {
            "mame" | "fbneo" => "mame",
            "neogeo" => "neogeo",
            "nes" => "nes",
            "snes" => "snes",
            "n64" => "n64",
            "gb" => "gb",
            "gba" => "gba",
            "gbc" => "gbc",
            "megadrive" | "genesis" => "genesis",
            "mastersystem" => "mastersystem",
            "gamegear" => "gamegear",
            "psx" => "psx",
            "dreamcast" => "dreamcast",
            "pcengine" => "pcengine",
            "atari2600" => "atari2600",
            _ => system,
        };

        // Try exact name, then clean name (without region/tags)
        let mut clean_name = game.to_string();
        if let Some(idx) = clean_name.find(" (") {
            clean_name.truncate(idx);
        }
        if let Some(idx) = clean_name.find(" [") {
            clean_name.truncate(idx);
        }
        let clean_name = clean_name.trim().to_string();

        let mut names_to_try = vec![game.to_string()];
        if !clean_name.is_empty() && clean_name != game {
            names_to_try.push(clean_name);
        }

        for name_variant in names_to_try {
            let safe_name = name_variant
                .replace(' ', "%20")
                .replace('!', "%21")
                .replace('\'', "%27")
                .replace('(', "%28")
                .replace(')', "%29")
                .replace('&', "%26");

            for ext in &[".png", ".gif"] {
                let url = format!(
                    "https://raw.githubusercontent.com/alinke/pixelcade/master/{}/{}{}",
                    pixelcade_system, safe_name, ext
                );

                match self
                    .http_client
                    .get(&url)
                    .header("User-Agent", "ArcadeMatrix")
                    .send()
                {
                    Ok(resp) if resp.status().is_success() => {
                        if let Ok(bytes) = resp.bytes() {
                            let tmp_path = self.cache_dir.join(format!("{}.tmp", local_filename));
                            if let Ok(mut file) = File::create(&tmp_path) {
                                if file.write_all(&bytes).is_ok()
                                    && fs::rename(&tmp_path, &local_path).is_ok()
                                {
                                    info!("Downloaded marquee for {}", key);
                                    return Some(local_path);
                                }
                            }
                        }
                    }
                    _ => continue,
                }
            }
        }

        self.negative_cache.lock().insert(key);
        None
    }
}

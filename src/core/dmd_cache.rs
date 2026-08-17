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

    pub fn get_cached_system_path(&self, system: &str) -> Option<PathBuf> {
        let key = format!("system/{}", system);
        if self.negative_cache.lock().contains(&key) {
            return None;
        }

        let local_png = self.cache_dir.join(format!("system_{}.png", system));
        if local_png.exists() {
            return Some(local_png);
        }
        let local_gif = self.cache_dir.join(format!("system_{}.gif", system));
        if local_gif.exists() {
            return Some(local_gif);
        }
        None
    }

    pub fn download_system_marquee(&self, system: &str) -> Option<PathBuf> {
        let key = format!("system/{}", system);
        if self.negative_cache.lock().contains(&key) {
            return None;
        }

        if let Some(path) = self.get_cached_system_path(system) {
            return Some(path);
        }

        let variants = get_system_name_variants(system);

        for (folder, name_variant) in variants {
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
                    folder, safe_name, ext
                );

                match self
                    .http_client
                    .get(&url)
                    .header("User-Agent", "ArcadeMatrix")
                    .send()
                {
                    Ok(resp) if resp.status().is_success() => {
                        if let Ok(bytes) = resp.bytes() {
                            let local_filename = format!("system_{}{}", system, ext);
                            let local_path = self.cache_dir.join(&local_filename);
                            let tmp_path = self.cache_dir.join(format!("{}.tmp", local_filename));
                            if let Ok(mut file) = File::create(&tmp_path) {
                                if file.write_all(&bytes).is_ok()
                                    && fs::rename(&tmp_path, &local_path).is_ok()
                                {
                                    info!("Downloaded system marquee for {}", key);
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

pub fn get_system_name_variants(system: &str) -> Vec<(&'static str, String)> {
    let sys_lower = system.to_lowercase();
    let sys_upper = system.to_uppercase();
    let sys_space = system.replace('_', " ");

    let mut names: Vec<String> = Vec::new();

    // 1. Direct name variants
    names.push(system.to_string());
    if sys_lower != system {
        names.push(sys_lower.clone());
    }
    if sys_upper != system && sys_upper != sys_lower {
        names.push(sys_upper.clone());
    }
    if sys_space != system && sys_space != sys_lower && sys_space != sys_upper {
        names.push(sys_space);
    }

    // 2. Common aliases
    match sys_lower.as_str() {
        "snes" | "supernintendo" => {
            names.push("Super Nintendo".into());
            names.push("Super Nintendo Entertainment System".into());
            names.push("- Super Nintendo".into());
        }
        "nes" | "famicom" => {
            names.push("Nintendo Entertainment System".into());
            names.push("3dnes".into());
        }
        "megadrive" | "genesis" => {
            names.push("genesis".into());
            names.push("Genesis".into());
            names.push("Mega Drive".into());
            names.push("SEGA Genesis".into());
            names.push("- Genesis".into());
        }
        "mame" | "arcade" | "fbneo" | "fba" => {
            names.push("arcade".into());
            names.push("Arcade".into());
            names.push("- Arcade".into());
            names.push("MAME".into());
            names.push("mame".into());
        }
        "n64" => {
            names.push("Nintendo 64".into());
        }
        "gb" | "gameboy" => {
            names.push("Game Boy".into());
        }
        "gba" => {
            names.push("Game Boy Advance".into());
        }
        "gbc" => {
            names.push("Game Boy Color".into());
        }
        "psx" | "ps1" => {
            names.push("PlayStation".into());
            names.push("Sony PlayStation".into());
        }
        "dreamcast" => {
            names.push("Dreamcast".into());
            names.push("SEGA Dreamcast".into());
        }
        "neogeo" => {
            names.push("Neo Geo".into());
            names.push("SNK Neo Geo".into());
        }
        "atari2600" => {
            names.push("Atari_2600".into());
            names.push("Atari 2600".into());
        }
        "mastersystem" => {
            names.push("Master System".into());
            names.push("SEGA Master System".into());
        }
        "gamegear" => {
            names.push("Game Gear".into());
            names.push("SEGA Game Gear".into());
        }
        "pcengine" | "tg16" => {
            names.push("NEC PC Engine".into());
            names.push("PC Engine".into());
        }
        "amiga" => {
            names.push("Commodore Amiga".into());
            names.push("Amiga".into());
        }
        "c64" => {
            names.push("COMMODORE_64".into());
            names.push("Commodore 64".into());
        }
        _ => {}
    }

    // Deduplicate names preserving order
    let mut unique_names: Vec<String> = Vec::new();
    for n in names {
        if !unique_names.contains(&n) {
            unique_names.push(n);
        }
    }

    // Prioritize system folder first (arcade/borne priority), then console folder
    let mut list: Vec<(&'static str, String)> = Vec::new();
    for folder in &["system", "console"] {
        for name in &unique_names {
            list.push((*folder, name.clone()));
        }
    }

    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_system_name_variants_priority() {
        let snes = get_system_name_variants("snes");
        assert!(snes.contains(&("system", "snes".to_string())));
        assert!(snes.contains(&("console", "snes".to_string())));
        assert!(snes.contains(&("system", "Super Nintendo".to_string())));
        assert!(snes.contains(&("console", "Super Nintendo".to_string())));

        // System folder entries must come BEFORE console folder entries
        let first_system_idx = snes.iter().position(|&(f, _)| f == "system").unwrap();
        let first_console_idx = snes.iter().position(|&(f, _)| f == "console").unwrap();
        assert!(first_system_idx < first_console_idx);
    }

    #[test]
    fn test_arcade_aliases() {
        let mame = get_system_name_variants("mame");
        assert!(mame.contains(&("system", "arcade".to_string())));
        assert!(mame.contains(&("console", "arcade".to_string())));
        assert!(mame.contains(&("system", "MAME".to_string())));

        let fbneo = get_system_name_variants("fbneo");
        assert!(fbneo.contains(&("system", "arcade".to_string())));
        assert!(fbneo.contains(&("console", "Arcade".to_string())));
    }

    #[test]
    fn test_custom_system() {
        let unknown = get_system_name_variants("my_custom_system");
        assert!(unknown.contains(&("system", "my_custom_system".to_string())));
        assert!(unknown.contains(&("console", "my_custom_system".to_string())));
        assert!(unknown.contains(&("system", "MY_CUSTOM_SYSTEM".to_string())));
        assert!(unknown.contains(&("console", "my custom system".to_string())));
    }
}

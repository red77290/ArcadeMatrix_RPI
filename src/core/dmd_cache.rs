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

pub fn clean_system_name(system: &str) -> String {
    let mut s = system.trim();
    let prefixes = [
        "arcade manufacturer ",
        "arcade system ",
        "arcade genre ",
        "arcade collection ",
        "manufacturer ",
        "system ",
        "genre ",
        "collection ",
    ];
    for prefix in &prefixes {
        if s.to_lowercase().starts_with(prefix) {
            s = &s[prefix.len()..];
            s = s.trim();
            break;
        }
    }
    s.to_string()
}

pub fn get_system_name_variants(raw_system: &str) -> Vec<(&'static str, String)> {
    let clean = clean_system_name(raw_system);
    let sys_lower = clean.to_lowercase();
    let sys_upper = clean.to_uppercase();
    let sys_space = clean.replace('_', " ");
    let sys_title = clean
        .split(|c| c == '_' || c == ' ')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    let sys_nospace = sys_lower.replace(' ', "");
    let sys_underscore = sys_lower.replace(' ', "_");

    let mut base_names: Vec<String> = Vec::new();

    // 1. Direct name variants
    base_names.push(clean.clone());
    if sys_lower != clean {
        base_names.push(sys_lower.clone());
    }
    if sys_nospace != clean && sys_nospace != sys_lower {
        base_names.push(sys_nospace.clone());
    }
    if sys_underscore != clean && sys_underscore != sys_lower && sys_underscore != sys_nospace {
        base_names.push(sys_underscore.clone());
    }
    if sys_upper != clean && sys_upper != sys_lower {
        base_names.push(sys_upper.clone());
    }
    if sys_title != clean && sys_title != sys_lower && sys_title != sys_upper {
        base_names.push(sys_title);
    }
    if sys_space != clean && sys_space != sys_lower && sys_space != sys_upper {
        base_names.push(sys_space);
    }

    // 2. Common aliases
    match sys_lower.as_str() {
        "snes" | "supernintendo" => {
            base_names.push("Super Nintendo".into());
            base_names.push("Super Nintendo Entertainment System".into());
            base_names.push("- Super Nintendo".into());
        }
        "nes" | "famicom" => {
            base_names.push("Nintendo Entertainment System".into());
            base_names.push("3dnes".into());
        }
        "megadrive" | "genesis" => {
            base_names.push("genesis".into());
            base_names.push("Genesis".into());
            base_names.push("Mega Drive".into());
            base_names.push("SEGA Genesis".into());
            base_names.push("- Genesis".into());
        }
        "mame" | "arcade" | "fbneo" | "fba" => {
            base_names.push("arcade".into());
            base_names.push("Arcade".into());
            base_names.push("- Arcade".into());
            base_names.push("MAME".into());
            base_names.push("mame".into());
        }
        "n64" => {
            base_names.push("Nintendo 64".into());
        }
        "gb" | "gameboy" => {
            base_names.push("Game Boy".into());
        }
        "gba" => {
            base_names.push("Game Boy Advance".into());
        }
        "gbc" => {
            base_names.push("Game Boy Color".into());
        }
        "psx" | "ps1" => {
            base_names.push("PlayStation".into());
            base_names.push("Sony PlayStation".into());
        }
        "dreamcast" => {
            base_names.push("Dreamcast".into());
            base_names.push("SEGA Dreamcast".into());
        }
        "neogeo" => {
            base_names.push("Neo Geo".into());
            base_names.push("SNK Neo Geo".into());
        }
        "atari2600" => {
            base_names.push("Atari_2600".into());
            base_names.push("Atari 2600".into());
        }
        "mastersystem" => {
            base_names.push("Master System".into());
            base_names.push("SEGA Master System".into());
        }
        "gamegear" => {
            base_names.push("Game Gear".into());
            base_names.push("SEGA Game Gear".into());
        }
        "pcengine" | "tg16" => {
            base_names.push("NEC PC Engine".into());
            base_names.push("PC Engine".into());
        }
        "amiga" => {
            base_names.push("Commodore Amiga".into());
            base_names.push("Amiga".into());
        }
        "c64" => {
            base_names.push("COMMODORE_64".into());
            base_names.push("Commodore 64".into());
        }
        _ => {}
    }

    // Deduplicate base names
    let mut unique_base: Vec<String> = Vec::new();
    for n in base_names {
        if !unique_base.contains(&n) {
            unique_base.push(n);
        }
    }

    // Generate prefixed variants in strict priority order:
    // 1. default-{clean} (exact name as received from EmulationStation)
    // 2. default-_{clean} (exact category/collection name)
    // 3. default-z{clean_lower} & z{clean_lower} (Pixelcade convention for arcade board systems like zcps1, zkonami, zcapcom)
    // 4. default-arcade_{b}_classics / default-arcade{b}classics / default-manufacture_{b} (Pixelcade publisher collections)
    // 5. default-{variant} & default-_{variant} for other base variants
    // 6. {variant} direct names
    let mut name_variants: Vec<String> = Vec::new();
    let clean_lower = clean.to_lowercase();
    let clean_nospace = clean_lower.replace(' ', "");
    let clean_underscore = clean_lower.replace(' ', "_");

    name_variants.push(format!("default-{}", clean));
    name_variants.push(format!("default-_{}", clean));
    name_variants.push(format!("default-z{}", clean_lower));
    name_variants.push(format!("default-z{}", clean_nospace));
    name_variants.push(format!("z{}", clean_lower));
    name_variants.push(format!("z{}", clean_nospace));

    for b in &unique_base {
        let b_lower = b.to_lowercase();
        let b_nospace = b_lower.replace(' ', "");
        let b_underscore = b_lower.replace(' ', "_");
        name_variants.push(format!("default-{}", b));
        name_variants.push(format!("default-_{}", b));
        name_variants.push(format!("default-z{}", b_lower));
        name_variants.push(format!("default-z{}", b_nospace));
        name_variants.push(format!("default-arcade_{}_classics", b_underscore));
        name_variants.push(format!("default-arcade{}classics", b_nospace));
        name_variants.push(format!("default-manufacture_{}", b_underscore));
        name_variants.push(format!("default-manufacture_{}", b_lower));
    }
    for b in &unique_base {
        name_variants.push(b.clone());
    }

    let mut final_names: Vec<String> = Vec::new();
    for n in name_variants {
        if !final_names.contains(&n) {
            final_names.push(n);
        }
    }

    // Folder search order: console first, then system
    let mut list: Vec<(&'static str, String)> = Vec::new();
    for folder in &["console", "system"] {
        for name in &final_names {
            list.push((*folder, name.clone()));
        }
    }

    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_system_name() {
        assert_eq!(clean_system_name("Arcade manufacturer Toaplan"), "Toaplan");
        assert_eq!(clean_system_name("Arcade manufacturer NeoGeo"), "NeoGeo");
        assert_eq!(clean_system_name("Arcade System CPS1"), "CPS1");
        assert_eq!(clean_system_name("snes"), "snes");
    }

    #[test]
    fn test_get_system_name_variants_priority() {
        let snes = get_system_name_variants("snes");
        assert!(snes.contains(&("console", "default-snes".to_string())));
        assert!(snes.contains(&("console", "default-_snes".to_string())));
        assert!(snes.contains(&("console", "snes".to_string())));
        assert!(snes.contains(&("system", "default-snes".to_string())));

        // Console folder entries must come BEFORE system folder entries
        let first_console_idx = snes.iter().position(|&(f, _)| f == "console").unwrap();
        let first_system_idx = snes.iter().position(|&(f, _)| f == "system").unwrap();
        assert!(first_console_idx < first_system_idx);

        // default-snes must come before snes
        let default_idx = snes
            .iter()
            .position(|&(_, ref n)| n == "default-snes")
            .unwrap();
        let exact_idx = snes.iter().position(|&(_, ref n)| n == "snes").unwrap();
        assert!(default_idx < exact_idx);
    }

    #[test]
    fn test_arcade_manufacturer_cleaning() {
        let toaplan = get_system_name_variants("Arcade manufacturer Toaplan");
        assert!(toaplan.contains(&("console", "default-Toaplan".to_string())));
        assert!(toaplan.contains(&("console", "default-toaplan".to_string())));
        assert!(toaplan.contains(&("console", "Toaplan".to_string())));
    }

    #[test]
    fn test_arcade_aliases() {
        let mame = get_system_name_variants("mame");
        assert!(mame.contains(&("console", "default-arcade".to_string())));
        assert!(mame.contains(&("console", "arcade".to_string())));
        assert!(mame.contains(&("system", "arcade".to_string())));
    }
}

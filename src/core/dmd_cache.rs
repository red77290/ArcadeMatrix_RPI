use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use tracing::info;

pub struct DmdCache {
    cache_dir: PathBuf,
    negative_cache: Mutex<HashSet<String>>,
    http_client: reqwest::blocking::Client,
    system_mappings: HashMap<String, Vec<String>>,
}

impl DmdCache {
    pub fn new<P: AsRef<Path>>(cache_dir: P) -> Self {
        let path = cache_dir.as_ref().to_path_buf();
        let _ = fs::create_dir_all(&path);
        let http_client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());

        let mut system_mappings: HashMap<String, Vec<String>> = HashMap::new();

        // 1. Pre-populate with compiled-in embedded default mappings
        let embedded_json = include_str!("../../resources/systems.json");
        if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(embedded_json) {
            for (k, v) in map {
                let key = k.to_lowercase();
                if let Some(arr) = v.as_array() {
                    let targets: Vec<String> = arr
                        .iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect();
                    system_mappings
                        .entry(key)
                        .or_insert_with(Vec::new)
                        .extend(targets);
                } else if let Some(s) = v.as_str() {
                    system_mappings
                        .entry(key)
                        .or_insert_with(Vec::new)
                        .push(s.to_string());
                }
            }
        }

        // 2. Overlay from external systems.json if present
        let json_paths = [
            "resources/systems.json",
            "systems.json",
            "config/systems.json",
            "data/systems.json",
            "/usr/local/share/arcadematrix/systems.json",
            "/etc/arcadematrix/systems.json",
        ];
        let mut loaded_json = false;
        for jp in &json_paths {
            if Path::new(jp).exists() {
                if let Ok(f) = File::open(jp) {
                    let reader = BufReader::new(f);
                    if let Ok(serde_json::Value::Object(map)) = serde_json::from_reader(reader) {
                        for (k, v) in map {
                            let key = k.to_lowercase();
                            if let Some(arr) = v.as_array() {
                                let targets: Vec<String> = arr
                                    .iter()
                                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                                    .collect();
                                system_mappings
                                    .entry(key)
                                    .or_insert_with(Vec::new)
                                    .extend(targets);
                            } else if let Some(s) = v.as_str() {
                                system_mappings
                                    .entry(key)
                                    .or_insert_with(Vec::new)
                                    .push(s.to_string());
                            }
                        }
                        info!("Loaded system mappings override from JSON: {}", jp);
                        loaded_json = true;
                        break;
                    }
                }
            }
        }

        if !loaded_json {
            let map_paths = [
                "resources/console.csv",
                "console.csv",
                "data/console.csv",
                "/usr/local/share/arcadematrix/console.csv",
                "/etc/arcadematrix/console.csv",
            ];
            for mp in &map_paths {
                if Path::new(mp).exists() {
                    if let Ok(f) = File::open(mp) {
                        let reader = BufReader::new(f);
                        for line in reader.lines().flatten() {
                            let trimmed = line.trim();
                            if trimmed.is_empty() || trimmed.starts_with('#') {
                                continue;
                            }
                            let parts: Vec<&str> = trimmed.split(',').map(|s| s.trim()).collect();
                            if parts.len() >= 2 {
                                let key = parts[0].to_lowercase();
                                let targets: Vec<String> =
                                    parts[1..].iter().map(|s| s.to_string()).collect();
                                system_mappings
                                    .entry(key)
                                    .or_insert_with(Vec::new)
                                    .extend(targets);
                            }
                        }
                        info!("Loaded system mappings from CSV: {}", mp);
                        break;
                    }
                }
            }
        }

        Self {
            cache_dir: path,
            negative_cache: Mutex::new(HashSet::new()),
            http_client,
            system_mappings,
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

        let clean_sys = clean_system_name(system);
        let sys_lower = clean_sys.to_lowercase();
        let pixelcade_system = match sys_lower.as_str() {
            "mame" | "fbneo" | "fba" | "arcade" | "cave" | "capcom" | "cps1" | "cps2" | "cps3"
            | "konami" | "taito" | "dataeast" | "data east" | "midway" | "irem" | "namco"
            | "toaplan" | "technos" | "sammy" | "atomiswave" | "naomi" | "snk" => "mame",
            "neogeo" => "neogeo",
            "nes" | "famicom" => "console/nes",
            "snes" | "supernintendo" => "console/snes",
            "n64" | "nintendo64" => "console/n64",
            "gb" | "gameboy" => "console/gb",
            "gba" | "gameboyadvance" => "console/gba",
            "gbc" | "gameboycolor" => "console/gbc",
            "megadrive" | "genesis" => "console/genesis",
            "mastersystem" => "console/mastersystem",
            "gamegear" => "console/gamegear",
            "psx" | "ps1" | "playstation" => "console/psx",
            "dreamcast" => "console/dreamcast",
            "saturn" => "console/saturn",
            "pcengine" | "tg16" => "console/pcengine",
            "atari2600" => "console/atari2600",
            "atari5200" => "console/atari5200",
            "atari7800" => "console/atari7800",
            _ => &clean_sys,
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

        let variants = get_system_name_variants_mapped(&self.system_mappings, system);

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
    let lower = s.to_lowercase().replace('_', " ").replace('-', " ");
    for p in prefixes.iter() {
        if lower.starts_with(p) {
            if s.len() >= p.len() {
                s = &s[p.len()..];
                if s.starts_with('_') || s.starts_with('-') {
                    s = &s[1..];
                }
            }
            break;
        }
    }
    s.trim().to_string()
}

pub fn get_system_name_variants(raw_system: &str) -> Vec<(&'static str, String)> {
    let empty_map = HashMap::new();
    get_system_name_variants_mapped(&empty_map, raw_system)
}

pub fn get_system_name_variants_mapped(
    mappings: &HashMap<String, Vec<String>>,
    raw_system: &str,
) -> Vec<(&'static str, String)> {
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
    let raw_lower = raw_system.trim().to_lowercase();

    let mut name_variants: Vec<String> = Vec::new();

    // 1. First priority: Check custom mapping table (systems.json / console.csv)
    let lookup_keys = [&raw_lower, &sys_lower, &sys_nospace, &sys_underscore];
    for key in lookup_keys {
        if let Some(targets) = mappings.get(key) {
            for t in targets {
                if !name_variants.contains(t) {
                    name_variants.push(t.clone());
                }
            }
        }
    }

    // Check embedded keywords in multi-word names (e.g., "Capcom cps1" -> "cps1", "capcom")
    let embedded_keywords = ["cps1", "cps2", "cps3", "atomiswave", "naomi", "neogeo"];
    for kw in &embedded_keywords {
        if sys_nospace.contains(kw) {
            let kw_str = kw.to_string();
            if let Some(targets) = mappings.get(&kw_str) {
                for t in targets {
                    if !name_variants.contains(t) {
                        name_variants.push(t.clone());
                    }
                }
            }
            let def_z = format!("default-z{}", kw_str);
            if !name_variants.contains(&def_z) {
                name_variants.push(def_z);
            }
            let def = format!("default-{}", kw_str);
            if !name_variants.contains(&def) {
                name_variants.push(def);
            }
        }
    }

    // Extract individual words in reverse order (e.g. "cps1" before "capcom")
    for word in sys_lower.split_whitespace().rev() {
        if word != "arcade" && word != "manufacturer" && word != "system" && word != "genre" {
            let w_str = word.to_string();
            if let Some(targets) = mappings.get(&w_str) {
                for t in targets {
                    if !name_variants.contains(t) {
                        name_variants.push(t.clone());
                    }
                }
            }
            let def_z = format!("default-z{}", w_str);
            if !name_variants.contains(&def_z) {
                name_variants.push(def_z);
            }
            let def = format!("default-{}", w_str);
            if !name_variants.contains(&def) {
                name_variants.push(def);
            }
        }
    }

    let mut base_names: Vec<String> = Vec::new();

    // 2. Direct name variants
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

    // 3. Common aliases
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
        "atari" | "atari2600" | "atari7800" | "atari5200" | "atari800" | "atarilynx"
        | "atarijaguar" | "atarist" => {
            base_names.push("atari".into());
            base_names.push("Atari".into());
            base_names.push("Atari_2600".into());
            base_names.push("Atari 2600".into());
            base_names.push("Atari_7800".into());
            base_names.push("Atari 7800".into());
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
    let clean_lower = clean.to_lowercase();
    let clean_nospace = clean_lower.replace(' ', "");
    let clean_underscore = clean_lower.replace(' ', "_");
    let clean_kebab = clean_lower.replace(' ', "-");

    // 1. Direct name variants (e.g. cave, atari, capcom, data_east, dataeast)
    name_variants.push(clean_lower.clone());
    name_variants.push(clean.clone());
    name_variants.push(clean_underscore.clone());
    name_variants.push(clean_nospace.clone());
    name_variants.push(clean_kebab.clone());

    // 2. default- prefixed variants (e.g. default-cave, default-Cave, default-data_east)
    name_variants.push(format!("default-{}", clean));
    name_variants.push(format!("default-{}", clean_lower));
    name_variants.push(format!("default-{}", clean_underscore));
    name_variants.push(format!("default-{}", clean_nospace));
    name_variants.push(format!("default-{}", clean_kebab));
    name_variants.push(format!("default-_{}", clean));
    name_variants.push(format!("default-_{}", clean_lower));
    name_variants.push(format!("default-_{}", clean_underscore));

    // 3. Pixelcade z-prefixed board/publisher conventions (e.g. default-zatari, default-zcave, zcave)
    name_variants.push(format!("default-z{}", clean_lower));
    name_variants.push(format!("default-z{}", clean_nospace));
    name_variants.push(format!("z{}", clean_lower));
    name_variants.push(format!("z{}", clean_nospace));

    // 4. Publisher classic collections
    name_variants.push(format!("default-arcade_{}_classics", clean_underscore));
    name_variants.push(format!("default-arcade{}classics", clean_nospace));
    name_variants.push(format!("default-manufacture_{}", clean_underscore));
    name_variants.push(format!("default-manufacture_{}", clean_lower));

    for b in &unique_base {
        let b_lower = b.to_lowercase();
        let b_nospace = b_lower.replace(' ', "");
        let b_underscore = b_lower.replace(' ', "_");
        name_variants.push(b.clone());
        name_variants.push(b_lower.clone());
        name_variants.push(b_underscore.clone());
        name_variants.push(format!("default-{}", b));
        name_variants.push(format!("default-{}", b_lower));
        name_variants.push(format!("default-_{}", b));
        name_variants.push(format!("default-z{}", b_lower));
        name_variants.push(format!("default-z{}", b_nospace));
        name_variants.push(format!("default-arcade_{}_classics", b_underscore));
        name_variants.push(format!("default-arcade{}classics", b_nospace));
        name_variants.push(format!("default-manufacture_{}", b_underscore));
        name_variants.push(format!("default-manufacture_{}", b_lower));
    }

    let mut final_names: Vec<String> = Vec::new();
    for n in name_variants {
        if !final_names.contains(&n) {
            final_names.push(n);
        }
    }

    // Folder search is exclusively /console/
    let mut list: Vec<(&'static str, String)> = Vec::new();
    for name in &final_names {
        list.push(("console", name.clone()));
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

        // All entries must be in "console" folder
        for (folder, _) in &snes {
            assert_eq!(*folder, "console");
        }
    }

    #[test]
    fn test_arcade_manufacturer_cleaning() {
        let toaplan = get_system_name_variants("Arcade manufacturer Toaplan");
        assert!(toaplan.contains(&("console", "default-Toaplan".to_string())));
        assert!(toaplan.contains(&("console", "default-toaplan".to_string())));
        assert!(toaplan.contains(&("console", "Toaplan".to_string())));

        let atari = get_system_name_variants("Arcade Manufacturer Atari");
        assert!(atari.contains(&("console", "default-zatari".to_string())));
        assert!(atari.contains(&("console", "default-Atari".to_string())));
        assert!(atari.contains(&("console", "atari".to_string())));

        let capcom = get_system_name_variants("Arcade Manufacturer Capcom");
        assert!(capcom.contains(&("console", "default-zcapcom".to_string())));
        assert!(capcom.contains(&("console", "default-Capcom".to_string())));

        let dataeast = get_system_name_variants("Arcade Manufacturer Data East");
        assert!(dataeast.contains(&("console", "default-zdataeast".to_string())));
        assert!(dataeast.contains(&("console", "default-arcade_data_east_classics".to_string())));

        let cps1 = get_system_name_variants("Arcade System CPS1");
        assert!(cps1.contains(&("console", "default-zcps1".to_string())));
        assert!(cps1.contains(&("console", "default-CPS1".to_string())));
    }

    #[test]
    fn test_arcade_aliases() {
        let mame = get_system_name_variants("mame");
        assert!(mame.contains(&("console", "default-arcade".to_string())));
        assert!(mame.contains(&("console", "arcade".to_string())));

        let cave = get_system_name_variants("Arcade Manufacturer Cave");
        assert!(cave.contains(&("console", "cave".to_string())));
        assert!(cave.contains(&("console", "default-cave".to_string())));
        assert!(cave.contains(&("console", "default-zcave".to_string())));
    }

    #[test]
    fn test_custom_console_csv_mappings() {
        let mut custom_map = HashMap::new();
        custom_map.insert(
            "my_custom_system".to_string(),
            vec!["custom_art_1".to_string(), "custom_art_2".to_string()],
        );
        let variants = get_system_name_variants_mapped(&custom_map, "my_custom_system");
        assert_eq!(variants[0], ("console", "custom_art_1".to_string()));
        assert_eq!(variants[1], ("console", "custom_art_2".to_string()));
    }
}

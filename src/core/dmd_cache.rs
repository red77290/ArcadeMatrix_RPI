use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

const NEGATIVE_CACHE_FILENAME: &str = ".negative_cache_v2";
const MAX_CATALOG_BYTES: usize = 512 * 1024;
const MAX_MARQUEE_BYTES: usize = 2 * 1024 * 1024;

enum CatalogLoadResult {
    Hit(Arc<Vec<String>>),
    NotFound,
    TransientError,
}

pub struct DmdCache {
    cache_dir: PathBuf,
    negative_cache: Mutex<HashSet<String>>,
    catalog_cache: Mutex<HashMap<String, Arc<Vec<String>>>>,
    missing_catalogs: Mutex<HashSet<String>>,
    http_client: reqwest::blocking::Client,
    system_mappings: HashMap<String, Vec<String>>,
}

impl DmdCache {
    pub fn new<P: AsRef<Path>>(cache_dir: P) -> Self {
        let path = cache_dir.as_ref().to_path_buf();
        let _ = fs::create_dir_all(&path);
        let http_client = crate::core::memory::fast_http_client().clone();

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
                    system_mappings.entry(key).or_default().extend(targets);
                } else if let Some(s) = v.as_str() {
                    system_mappings.entry(key).or_default().push(s.to_string());
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
                                system_mappings.entry(key).or_default().extend(targets);
                            } else if let Some(s) = v.as_str() {
                                system_mappings.entry(key).or_default().push(s.to_string());
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
                        for line in reader.lines().map_while(Result::ok) {
                            let trimmed = line.trim();
                            if trimmed.is_empty() || trimmed.starts_with('#') {
                                continue;
                            }
                            let parts: Vec<&str> = trimmed.split(',').map(|s| s.trim()).collect();
                            if parts.len() >= 2 {
                                let key = parts[0].to_lowercase();
                                let targets: Vec<String> =
                                    parts[1..].iter().map(|s| s.to_string()).collect();
                                system_mappings.entry(key).or_default().extend(targets);
                            }
                        }
                        info!("Loaded system mappings from CSV: {}", mp);
                        break;
                    }
                }
            }
        }

        // Remove legacy pre-catalog `.negative_cache` so upgraded devices automatically retry
        // games that previously missed due to region tags, missing folder aliases, or EU titles.
        let legacy_neg_path = path.join(".negative_cache");
        if legacy_neg_path.exists() {
            let _ = fs::remove_file(&legacy_neg_path);
        }

        let mut negative_cache = HashSet::new();
        let neg_path = path.join(NEGATIVE_CACHE_FILENAME);
        if let Ok(file) = File::open(&neg_path) {
            for line in BufReader::new(file).lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    negative_cache.insert(trimmed.to_string());
                }
            }
        }

        Self {
            cache_dir: path,
            negative_cache: Mutex::new(negative_cache),
            catalog_cache: Mutex::new(HashMap::new()),
            missing_catalogs: Mutex::new(HashSet::new()),
            http_client,
            system_mappings,
        }
    }

    pub fn is_negative_cached(&self, system: &str, game: &str) -> bool {
        let safe_sys = sanitize_cache_component(system);
        let safe_game = sanitize_cache_component(game);
        let key = format!("{}/{}", safe_sys, safe_game);
        self.negative_cache.lock().contains(&key)
    }

    pub fn is_system_negative_cached(&self, system: &str) -> bool {
        let safe_sys = sanitize_cache_component(system);
        let key = format!("system/{}", safe_sys);
        self.negative_cache.lock().contains(&key)
    }

    fn insert_negative_cache(&self, key: String) {
        let mut lock = self.negative_cache.lock();
        if lock.insert(key.clone()) {
            let neg_path = self.cache_dir.join(NEGATIVE_CACHE_FILENAME);
            if let Ok(mut file) = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(neg_path)
            {
                let _ = writeln!(file, "{}", key);
            }
        }
    }

    pub fn get_cached_path(&self, system: &str, game: &str) -> Option<PathBuf> {
        let safe_sys = sanitize_cache_component(system);
        let safe_game = sanitize_cache_component(game);
        let key = format!("{}/{}", safe_sys, safe_game);
        if self.negative_cache.lock().contains(&key) {
            return None;
        }

        for ext in &[".png", ".gif", ".bmp"] {
            let local_filename = format!("{}_{}{}", safe_sys, safe_game, ext);
            let local_path = self.cache_dir.join(&local_filename);
            if local_path.exists() {
                return Some(local_path);
            }
        }
        None
    }

    pub fn download_marquee(&self, system: &str, game: &str) -> Option<PathBuf> {
        self.download_marquee_with_title(system, game, None)
    }

    pub fn download_marquee_with_title(
        &self,
        system: &str,
        game: &str,
        title: Option<&str>,
    ) -> Option<PathBuf> {
        let safe_sys = sanitize_cache_component(system);
        let safe_game = sanitize_cache_component(game);
        let key = format!("{}/{}", safe_sys, safe_game);
        if self.negative_cache.lock().contains(&key) {
            return None;
        }

        if let Some(existing) = self.get_cached_path(system, game) {
            return Some(existing);
        }

        let primary_folder = map_system_to_pixelcade_folder(system);
        let secondary_folder = get_secondary_pixelcade_folder(system);

        let mut folders_to_search: Vec<String> = Vec::with_capacity(2);
        if !is_known_empty_pixelcade_folder(&primary_folder) {
            folders_to_search.push(primary_folder);
        }
        if let Some(sec) = secondary_folder {
            if !is_known_empty_pixelcade_folder(sec)
                && !folders_to_search
                    .iter()
                    .any(|f| f.eq_ignore_ascii_case(sec))
            {
                folders_to_search.push(sec.to_string());
            }
        }

        if folders_to_search.is_empty() {
            self.insert_negative_cache(key);
            return None;
        }

        let title_str = title.unwrap_or("").trim();
        let norm_game = normalize_marquee_key(game);
        let norm_title = if !title_str.is_empty() {
            Some(normalize_marquee_key(title_str))
        } else {
            None
        };

        let mut all_confirmed_miss = true;
        let local_prefix = format!("{}_{}", safe_sys, safe_game);

        for (fi, folder) in folders_to_search.iter().enumerate() {
            match self.ensure_folder_catalog(folder) {
                CatalogLoadResult::Hit(entries) => {
                    let mut best_score = 0;
                    let mut best_entry: Option<&str> = None;
                    for entry in entries.iter() {
                        let sc = score_catalog_entry(
                            entry,
                            game,
                            title_str,
                            Some(&norm_game),
                            norm_title.as_deref(),
                        );
                        if sc > best_score {
                            best_score = sc;
                            best_entry = Some(entry.as_str());
                            if best_score >= 1028 {
                                break;
                            }
                        }
                    }

                    if let Some(matched_file) = best_entry {
                        let mut file_404 = false;
                        if let Some(saved) = self.download_exact_pixelcade_file(
                            folder,
                            matched_file,
                            &local_prefix,
                            &mut file_404,
                        ) {
                            info!(
                                "Downloaded marquee for {} ({}/{})",
                                key, folder, matched_file
                            );
                            return Some(saved);
                        }
                        if !file_404 {
                            all_confirmed_miss = false;
                        }
                    }
                    // When `CatalogLoadResult::Hit` has `best_entry == None`, the folder definitively
                    // does NOT contain this game — zero 404 HTTP probes needed!
                }
                CatalogLoadResult::NotFound => {
                    // Folder has no catalog upstream -> definitively missing
                }
                CatalogLoadResult::TransientError => {
                    all_confirmed_miss = false;
                    // Bounded 2-candidate direct fallback when catalog fetch hit a transient error
                    if fi == 0 && !game.is_empty() {
                        let stripped = strip_rom_or_art_extension_and_tags(game);
                        let base = if !stripped.is_empty() {
                            stripped.as_str()
                        } else {
                            game
                        };
                        let cand_png = format!("{}.png", base);
                        let mut dummy_404 = false;
                        if let Some(saved) = self.download_exact_pixelcade_file(
                            folder,
                            &cand_png,
                            &local_prefix,
                            &mut dummy_404,
                        ) {
                            info!("Downloaded marquee (direct fallback) for {}", key);
                            return Some(saved);
                        }
                    }
                }
            }
        }

        if all_confirmed_miss {
            self.insert_negative_cache(key);
        } else {
            tracing::warn!(
                "[Pixelcade] Transient error resolving marquee for '{}' ({}); skipping negative cache",
                game,
                system
            );
        }
        None
    }

    pub fn get_cached_system_path(&self, system: &str) -> Option<PathBuf> {
        let safe_sys = sanitize_cache_component(system);
        let key = format!("system/{}", safe_sys);
        if self.negative_cache.lock().contains(&key) {
            return None;
        }

        for ext in &[".png", ".gif", ".bmp"] {
            let candidate = self.cache_dir.join(format!("system_{}{}", safe_sys, ext));
            if candidate.exists() {
                return Some(candidate);
            }
        }
        None
    }

    pub fn download_system_marquee(&self, system: &str) -> Option<PathBuf> {
        let safe_sys = sanitize_cache_component(system);
        let key = format!("system/{}", safe_sys);
        if self.negative_cache.lock().contains(&key) {
            return None;
        }

        if let Some(path) = self.get_cached_system_path(system) {
            return Some(path);
        }

        let variants = get_system_name_variants_mapped(&self.system_mappings, system);
        let local_prefix = format!("system_{}", safe_sys);

        match self.ensure_folder_catalog("console") {
            CatalogLoadResult::Hit(entries) => {
                let mut best_score = 0;
                let mut best_entry: Option<&str> = None;
                for entry in entries.iter() {
                    let sc = score_system_catalog_entry(entry, &variants);
                    if sc > best_score {
                        best_score = sc;
                        best_entry = Some(entry.as_str());
                        if best_score >= 1998 {
                            break;
                        }
                    }
                }

                if let Some(matched_file) = best_entry {
                    let mut file_404 = false;
                    if let Some(saved) = self.download_exact_pixelcade_file(
                        "console",
                        matched_file,
                        &local_prefix,
                        &mut file_404,
                    ) {
                        info!(
                            "Downloaded system marquee for {} (console/{})",
                            key, matched_file
                        );
                        return Some(saved);
                    }
                    if !file_404 {
                        return None;
                    }
                }

                self.insert_negative_cache(key);
                None
            }
            CatalogLoadResult::NotFound => {
                self.insert_negative_cache(key);
                None
            }
            CatalogLoadResult::TransientError => {
                // Bounded 2-candidate direct fallback without poisoning negative cache on outage
                for (_, name_variant) in variants.iter().take(2) {
                    let cand_png = format!("{}.png", name_variant);
                    let mut dummy_404 = false;
                    if let Some(saved) = self.download_exact_pixelcade_file(
                        "console",
                        &cand_png,
                        &local_prefix,
                        &mut dummy_404,
                    ) {
                        info!("Downloaded system marquee (direct fallback) for {}", key);
                        return Some(saved);
                    }
                }
                tracing::warn!(
                    "[Pixelcade] Could not reach raw.githubusercontent.com for system '{}' (network/DNS/TLS?)",
                    system
                );
                None
            }
        }
    }

    fn ensure_folder_catalog(&self, folder: &str) -> CatalogLoadResult {
        if folder.is_empty() || is_known_empty_pixelcade_folder(folder) {
            return CatalogLoadResult::NotFound;
        }

        let folder_key = folder.to_ascii_lowercase();
        if let Some(existing) = self.catalog_cache.lock().get(&folder_key).cloned() {
            return CatalogLoadResult::Hit(existing);
        }
        if self.missing_catalogs.lock().contains(&folder_key) {
            return CatalogLoadResult::NotFound;
        }

        let catalog_dir = self.cache_dir.join("_catalog");
        let safe_folder = sanitize_cache_component(folder);
        let disk_path = catalog_dir.join(format!("{}.txt", safe_folder));

        if disk_path.exists() {
            if let Ok(raw) = fs::read_to_string(&disk_path) {
                let parsed = parse_catalog_lines(&raw);
                if !parsed.is_empty() {
                    let arc = Arc::new(parsed);
                    self.catalog_cache
                        .lock()
                        .insert(folder_key, Arc::clone(&arc));
                    return CatalogLoadResult::Hit(arc);
                }
            }
            let _ = fs::remove_file(&disk_path);
        }

        let enc_folder = url_encode_segment(folder);
        let url = format!(
            "https://raw.githubusercontent.com/red77290/pixelcade/master/_catalog/{}.txt",
            enc_folder
        );

        match self
            .http_client
            .get(&url)
            .header("User-Agent", "ArcadeMatrix-RPI")
            .send()
        {
            Ok(resp) if resp.status().is_success() => match resp.bytes() {
                Ok(bytes) if bytes.len() <= MAX_CATALOG_BYTES => {
                    let text = String::from_utf8_lossy(&bytes);
                    let parsed = parse_catalog_lines(&text);
                    if parsed.is_empty() {
                        self.missing_catalogs.lock().insert(folder_key);
                        return CatalogLoadResult::NotFound;
                    }
                    let _ = fs::create_dir_all(&catalog_dir);
                    let tmp_path = catalog_dir.join(format!("{}.tmp", safe_folder));
                    if let Ok(mut f) = File::create(&tmp_path) {
                        if f.write_all(&bytes).is_ok() {
                            let _ = fs::rename(&tmp_path, &disk_path);
                        } else {
                            let _ = fs::remove_file(&tmp_path);
                        }
                    }
                    let arc = Arc::new(parsed);
                    self.catalog_cache
                        .lock()
                        .insert(folder_key, Arc::clone(&arc));
                    CatalogLoadResult::Hit(arc)
                }
                _ => CatalogLoadResult::TransientError,
            },
            Ok(resp) if resp.status().as_u16() == 404 => {
                self.missing_catalogs.lock().insert(folder_key);
                CatalogLoadResult::NotFound
            }
            Ok(_) => CatalogLoadResult::TransientError,
            Err(e) => {
                tracing::debug!("[Pixelcade] Network error fetching catalog {}: {}", url, e);
                CatalogLoadResult::TransientError
            }
        }
    }

    fn download_exact_pixelcade_file(
        &self,
        folder: &str,
        remote_filename: &str,
        local_prefix: &str,
        out_is_404: &mut bool,
    ) -> Option<PathBuf> {
        *out_is_404 = false;
        if folder.is_empty() || remote_filename.is_empty() || local_prefix.is_empty() {
            return None;
        }

        let ext = if remote_filename
            .get(remote_filename.len().saturating_sub(4)..)
            .map(|s| s.eq_ignore_ascii_case(".gif"))
            .unwrap_or(false)
        {
            ".gif"
        } else {
            ".png"
        };

        let enc_folder = url_encode_segment(folder);
        let enc_file = url_encode_segment(remote_filename);
        let url = format!(
            "https://raw.githubusercontent.com/red77290/pixelcade/master/{}/{}",
            enc_folder, enc_file
        );

        match self
            .http_client
            .get(&url)
            .header("User-Agent", "ArcadeMatrix-RPI")
            .send()
        {
            Ok(resp) if resp.status().is_success() => {
                let Ok(bytes) = resp.bytes() else {
                    return None;
                };
                if !validate_marquee_image_bytes(&bytes) {
                    tracing::warn!(
                        "[Pixelcade] Rejected invalid or oversized image payload from {}",
                        url
                    );
                    *out_is_404 = true;
                    return None;
                }
                let local_filename = format!("{}{}", local_prefix, ext);
                let local_path = self.cache_dir.join(&local_filename);
                let tmp_path = self.cache_dir.join(format!("{}.tmp", local_filename));
                if let Ok(mut file) = File::create(&tmp_path) {
                    if file.write_all(&bytes).is_ok() && fs::rename(&tmp_path, &local_path).is_ok()
                    {
                        return Some(local_path);
                    }
                    let _ = fs::remove_file(&tmp_path);
                }
                None
            }
            Ok(resp) if resp.status().as_u16() == 404 => {
                *out_is_404 = true;
                None
            }
            Ok(_) => None,
            Err(e) => {
                tracing::debug!("[Pixelcade] Network error fetching {}: {}", url, e);
                None
            }
        }
    }
}

fn parse_catalog_lines(raw: &str) -> Vec<String> {
    raw.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect()
}

pub fn validate_marquee_image_bytes(bytes: &[u8]) -> bool {
    if bytes.len() < 10 || bytes.len() > MAX_MARQUEE_BYTES {
        return false;
    }
    if bytes.len() >= 24 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        return w > 0 && w <= 512 && h > 0 && h <= 512;
    }
    if &bytes[0..6] == b"GIF87a" || &bytes[0..6] == b"GIF89a" {
        let w = u16::from_le_bytes([bytes[6], bytes[7]]);
        let h = u16::from_le_bytes([bytes[8], bytes[9]]);
        return w > 0 && w <= 512 && h > 0 && h <= 512;
    }
    false
}

pub fn sanitize_cache_component(s: &str) -> String {
    let mut cleaned = s.replace("..", "_");
    cleaned = cleaned
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_start_matches('.');
    if trimmed.is_empty() {
        "_".to_string()
    } else {
        trimmed.to_string()
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
    let lower = s.to_lowercase().replace(['_', '-'], " ");
    for p in prefixes.iter() {
        if lower.starts_with(p) {
            if let Some(rest) = s.get(p.len()..) {
                s = rest.trim_start_matches(['_', '-']);
            }
            break;
        }
    }
    s.trim().to_string()
}

fn compact_system_key(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

pub fn map_system_to_pixelcade_folder(system_id: &str) -> String {
    let cleaned = clean_system_name(system_id);
    let key = compact_system_key(&cleaned);
    if key.is_empty() {
        return system_id.trim().to_string();
    }

    let mapped = match key.as_str() {
        // Arcade / MAME / FBNeo / Publishers / Boards
        "mame" | "mame2003" | "mame2003plus" | "mame2010" | "fbneo" | "fba" | "fbalibretro"
        | "arcade" | "hbmame" | "cps" | "cps1" | "cps2" | "cps3" | "capcom" | "konami"
        | "taito" | "dataeast" | "midway" | "irem" | "namco" | "namcosystem22" | "toaplan"
        | "technos" | "psikyo" | "kaneko" | "visco" | "raizing" | "eighting" | "banpresto"
        | "jaleco" | "nichibutsu" | "seibu" | "snk" | "sammy" | "gaelco" | "igs" | "pgm"
        | "williams" | "bally" | "gottlieb" | "stern" | "universal" | "exidy" | "cinematronics"
        | "atari" | "sega" | "model3" | "segamodel3" | "hikaru" | "chihiro" => Some("mame"),
        "nintendoarcade" => Some("nintendo_arcade_systems"),
        "cave" | "cave3rd" => Some("cave"),
        "neogeo" => Some("neogeo"),
        "neogeomvs" => Some("snk_neo_geo_mvs"),
        "neogeoaes" => Some("snk_neo_geo_aes"),
        "neogeocd" | "neocdz" => Some("snk_neo_geo_cd"),
        "naomi" | "naomi2" | "naomigd" | "seganaomi" => Some("sega_naomi"),
        "atomiswave" | "sammyatomiswave" => Some("sammy_atomiswave"),
        "model2" | "segamodel2" => Some("sega_model_2"),
        "stv" | "segastv" => Some("sega_st-v"),
        "triforce" | "segatriforce" => Some("sega_triforce"),
        "lindbergh" | "teknoparrot" => Some("teknoparrot"),
        "ringedge" | "ringwide" => Some("sega_ring"),
        "taitotypex" | "typex" => Some("taito_type_x"),
        "zinc" => Some("zinc"),
        "aae" => Some("aae"),
        "daphne" | "hypseus" => Some("daphne"),
        "singe" | "daphnesinge" => Some("daphne_singe"),
        "actionmax" => Some("wow_action_max"),
        "americanlasergames" | "alg" => Some("american_laser_games"),
        "lightgun" | "gungames" => Some("gun_games"),

        // Nintendo Consoles & Handhelds
        "nes" | "nintendoentertainmentsystem" => Some("nes"),
        "famicom" | "fc" | "nintendofamicom" => Some("nintendo_famicom"),
        "fds" | "famicomdisksystem" => Some("nintendo_famicom_disk_system"),
        "snes" | "supernintendo" | "snesmsu1" | "snesturbo" => Some("snes"),
        "sfc" | "superfamicom" => Some("nintendo_super_famicom"),
        "satellaview" | "bsx" => Some("nintendo_satellaview"),
        "sufami" | "sufamiturbo" => Some("nintendo_sufami_turbo"),
        "n64" | "nintendo64" | "n64dd" => Some("n64"),
        "gc" | "gamecube" | "ngc" | "nintendogamecube" => Some("nintendo_gamecube"),
        "wii" | "nintendowii" | "wiiware" => Some("nintendo_wii"),
        "wiiu" | "nintendowiiu" => Some("nintendo_wii_u"),
        "switch" | "nintendoswitch" => Some("nintendo_switch"),
        "gb" | "gameboy" | "sgb" | "supergameboy" => Some("gb"),
        "gbc" | "gameboycolor" => Some("gbc"),
        "gba" | "gameboyadvance" => Some("gba"),
        "nds" | "ds" | "nintendods" => Some("nintendo_ds"),
        "3ds" | "n3ds" | "nintendo3ds" => Some("nintendo_3ds"),
        "virtualboy" | "vb" => Some("virtualboy"),

        // SEGA Consoles & Handhelds
        "sg1000" | "segasg1000" => Some("sega_sg-1000"),
        "mastersystem" | "sms" | "segamastersystem" => Some("mastersystem"),
        "megadrive" | "genesis" | "segamegadrive" | "segagenesis" => Some("genesis"),
        "megadrivejp" => Some("megadrive"),
        "segacd" | "megacd" | "mcd" => Some("segacd"),
        "sega32x" | "32x" | "mega32x" => Some("sega32x"),
        "saturn" | "segasaturn" => Some("sega_saturn"),
        "dreamcast" | "dc" | "segadreamcast" => Some("dreamcast"),
        "gamegear" | "gg" | "segagamegear" => Some("gamegear"),

        // Sony PlayStation
        "psx" | "ps1" | "playstation" | "sonyplaystation" => Some("psx"),
        "ps2" | "playstation2" | "sonyplaystation2" => Some("ps2"),
        "ps3" | "playstation3" | "sonyplaystation3" => Some("sony_playstation_3"),
        "psp" | "playstationportable" => Some("psp"),
        "pspminis" | "psminis" => Some("sony_playstation_minis"),

        // Microsoft
        "xbox" | "microsoftxbox" => Some("microsoft_xbox"),

        // NEC
        "pcengine" | "pce" => Some("pcengine"),
        "tg16" | "turbografx16" | "turbografx" => Some("nec_turbografx-16"),
        "pcenginecd" | "pcecd" => Some("nec_pc_engine-cd"),
        "tg16cd" | "turbografxcd" | "cdrom2" => Some("nec_turbografx_cd"),
        "supergrafx" | "sgx" => Some("supergrafx"),
        "necsupergrafx" => Some("nec_supergrafx"),
        "pcfx" | "necpcfx" => Some("nec_pc_fx"),

        // SNK / Bandai / Other Handhelds
        "ngp" | "ngpc" | "neogeopocket" | "neogeopocketcolor" => Some("ngpc"),
        "wswan" | "wonderswan" | "ws" => Some("wonderswan"),
        "wswanc" | "wonderswancolor" | "wsc" => Some("wonderswancolor"),
        "supervision" | "watarasupervision" => Some("watara_supervision"),
        "megaduck" => Some("megaduck"),
        "casio" | "pv1000" | "casiopv1000" => Some("casio_pv-1000"),
        "adventurevision" => Some("entex_adventure_vision"),

        // Atari / Classic Consoles / Computers / Pinball
        "atari2600" | "a2600" => Some("atari2600"),
        "atari5200" | "a5200" => Some("atari5200"),
        "atari7800" | "a7800" => Some("atari7800"),
        "atari800" | "atarixe" | "xegs" => Some("atari800"),
        "atarist" | "atariste" => Some("atari_st"),
        "atarilynx" | "lynx" => Some("atarilynx"),
        "atarijaguar" | "jaguar" | "atarijaguarcd" => Some("atarijaguar"),
        "colecovision" | "coleco" => Some("coleco"),
        "intellivision" | "intv" => Some("intellivision"),
        "vectrex" => Some("vectrex"),
        "odyssey2" | "videopac" | "odyssey" => Some("odyssey"),
        "astrocade" | "ballyastrocade" => Some("bally_astrocade"),
        "arcadia" | "arcadia2001" => Some("emerson_arcadia_2001"),
        "3do" | "panasonic3do" => Some("3do"),
        "amiga" | "amiga500" | "amiga600" | "amiga1200" | "amigacd32" | "cd32" => Some("amiga"),
        "c64" | "commodore64" => Some("c64"),
        "amstradcpc" | "cpc" => Some("amstradcpc"),
        "gx4000" | "amstradgx4000" => Some("amstrad_gx4000"),
        "dos" | "msdos" => Some("dos"),
        "pc" => Some("pc"),
        "scummvm" => Some("scummvm"),
        "ports" => Some("ports"),
        "windows" => Some("windows"),
        "openbor" => Some("openbor"),
        "mugen" | "ikemen" => Some("mugen"),
        "vpinball" | "visualpinball" => Some("visualpinball"),
        "fpinball" | "futurepinball" => Some("futurepinball"),
        "pinballfx" => Some("pinball_fx"),
        "pinballfx2" => Some("pinball_fx2"),
        "pinballfx3" => Some("pinball_fx3"),
        "zaccaria" => Some("zaccaria"),
        _ => None,
    };

    if let Some(folder) = mapped {
        folder.to_string()
    } else if !cleaned.is_empty() {
        cleaned
    } else {
        system_id.trim().to_string()
    }
}

pub fn get_secondary_pixelcade_folder(system_id: &str) -> Option<&'static str> {
    let cleaned = clean_system_name(system_id);
    let key = compact_system_key(&cleaned);
    match key.as_str() {
        "neogeo" | "neogeomvs" | "neogeoaes" | "cave" | "cave3rd" | "naomi" | "naomi2"
        | "naomigd" | "seganaomi" | "atomiswave" | "sammyatomiswave" | "model2" | "segamodel2"
        | "stv" | "segastv" | "zinc" | "aae" | "nintendoarcade" => Some("mame"),
        "neogeocd" | "neocdz" => Some("neogeo"),
        "cps1" => Some("capcom_play_system"),
        "cps2" => Some("capcom_play_system_ii"),
        "cps3" => Some("capcom_play_system_iii"),
        "triforce" | "segatriforce" => Some("triforce"),
        "singe" | "daphnesinge" => Some("daphne"),
        "sfc" | "superfamicom" => Some("snes"),
        "snes" | "supernintendo" => Some("nintendo_super_famicom"),
        "famicom" | "fc" | "nintendofamicom" | "fds" | "famicomdisksystem" => Some("nes"),
        "nes" => Some("nintendo_famicom"),
        "megadrive" | "genesis" => Some("megadrive"),
        "megadrivejp" => Some("genesis"),
        "tg16" | "turbografx16" => Some("pcengine"),
        "pcengine" | "pce" => Some("nec_turbografx-16"),
        "pcenginecd" | "pcecd" => Some("nec_turbografx_cd"),
        "tg16cd" | "turbografxcd" => Some("nec_pc_engine-cd"),
        "supergrafx" => Some("nec_supergrafx"),
        "gx4000" => Some("amstradcpc"),
        "dos" => Some("pc"),
        "pc" => Some("dos"),
        "ports" => Some("windows"),
        "windows" => Some("pc"),
        _ => None,
    }
}

pub fn is_known_empty_pixelcade_folder(folder: &str) -> bool {
    let trimmed = folder.trim();
    if trimmed.is_empty() {
        return true;
    }
    const KNOWN_EMPTY: &[&str] = &[
        "apple2",
        "commodore_cdtv",
        "commodore_vic_20",
        "doflinx",
        "driver",
        "fairchild_channel_f",
        "fba",
        "fds",
        "fonts",
        "fujitsu_fm_towns",
        "gamepark_gp32",
        "interton_electronic_vc_4000",
        "kodi",
        "macintosh",
        "magnavox_odyssey_2",
        "mattel_aquarius",
        "metadata",
        "mgt_sam_coupe",
        "microsoft_ms-dos",
        "microsoft_msx2",
        "misfit_mame",
        "msx",
        "msx1",
        "msx2",
        "msx2+",
        "msxturbor",
        "nds",
        "ngp",
        "nintendo_pokemon_mini",
        "pokemini",
        "philips_cd-i",
        "cdi",
        "pinball",
        "saturn",
        "search",
        "sega_pico",
        "pico",
        "sega_sc_3000",
        "sc-3000",
        "sc3000",
        "sega_vmu",
        "sg-1000",
        "sidekickwidgetart",
        "sinclair_zx81",
        "zx81",
        "snk_neo_geo_pocket",
        "sony_pocketstation",
        "steam",
        "system",
        "ti99",
        "ti994a",
        "toolkit",
        "user",
        "zmachine",
        "zxspectrum",
        "thomson",
        "to8",
        "mo5",
        "oric",
        "atmos",
        "samcoupe",
        "channelf",
        "vectrex2",
        "uzebox",
        "tic80",
        "pico8",
        "lowresnx",
        "wasm4",
        "lutro",
        "solarus",
        "easyrpg",
        "x68000",
        "pc88",
        "pc98",
        "fmtowns",
        "x1",
        "favorites",
        "allgames",
        "lastplayed",
        "imageviewer",
        "screenshots",
    ];
    KNOWN_EMPTY.iter().any(|e| trimmed.eq_ignore_ascii_case(e))
}

pub fn is_virtual_collection_system(sys: &str) -> bool {
    let trimmed = sys.trim();
    if trimmed.is_empty() {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("auto-")
        || lower.starts_with("auto_")
        || lower.starts_with("custom-")
        || lower.starts_with("custom_")
        || lower.starts_with("collection")
    {
        return true;
    }
    let key = compact_system_key(trimmed);
    matches!(
        key.as_str(),
        "favorites"
            | "favourite"
            | "favourites"
            | "fav"
            | "favs"
            | "all"
            | "allgames"
            | "lastplayed"
            | "recent"
            | "recentlyplayed"
            | "neverplayed"
            | "multiplayer"
            | "2players"
            | "3players"
            | "4players"
            | "retroachievements"
            | "lightgun"
            | "vertical"
            | "tate"
            | "shmups"
            | "fighting"
            | "beatemup"
            | "platform"
            | "puzzle"
            | "racing"
            | "rpg"
            | "sports"
    )
}

pub fn extract_system_from_rom_path(current_sys: &str, rom_path: &str) -> String {
    if !is_virtual_collection_system(current_sys) || rom_path.trim().is_empty() {
        return current_sys.to_string();
    }
    let norm = rom_path.replace('\\', "/");
    let lower = norm.to_ascii_lowercase();
    if let Some(idx) = lower.find("/roms/") {
        let after = &norm[(idx + 6)..];
        let after_trimmed = after.trim_start_matches('/');
        if let Some(slash_idx) = after_trimmed.find('/') {
            let candidate = &after_trimmed[..slash_idx];
            if !candidate.is_empty()
                && candidate.len() < 48
                && !is_virtual_collection_system(candidate)
            {
                return candidate.to_string();
            }
        }
    }
    current_sys.to_string()
}

fn strip_known_rom_or_image_ext(raw: &str) -> &str {
    const EXTS: &[&str] = &[
        ".png", ".gif", ".zip", ".7z", ".sfc", ".smc", ".nes", ".fds", ".md", ".gen", ".smd",
        ".bin", ".cue", ".iso", ".chd", ".gba", ".gbc", ".gb", ".nds", ".3ds", ".cci", ".z64",
        ".n64", ".v64", ".pce", ".cdi", ".gdi", ".pbp", ".cso", ".wad", ".rvz", ".wbfs", ".xci",
        ".nsp", ".dsk", ".adf", ".ipf", ".a26", ".a52", ".a78", ".lnx", ".j64", ".col", ".vec",
        ".ws", ".wsc", ".ngp", ".ngc",
    ];
    for ext in EXTS {
        if raw.len() > ext.len() {
            let split_at = raw.len() - ext.len();
            if raw
                .get(split_at..)
                .map(|tail| tail.eq_ignore_ascii_case(ext))
                .unwrap_or(false)
            {
                return &raw[..split_at];
            }
        }
    }
    raw
}

pub fn strip_rom_or_art_extension_and_tags(raw: &str) -> String {
    let base = strip_known_rom_or_image_ext(raw);
    let mut out = String::with_capacity(base.len());
    let mut paren_depth = 0usize;
    let mut brack_depth = 0usize;
    let mut prev_space = true;

    for ch in base.chars() {
        if ch == '(' {
            paren_depth += 1;
            continue;
        }
        if ch == ')' && paren_depth > 0 {
            paren_depth -= 1;
            continue;
        }
        if ch == '[' {
            brack_depth += 1;
            continue;
        }
        if ch == ']' && brack_depth > 0 {
            brack_depth -= 1;
            continue;
        }
        if paren_depth > 0 || brack_depth > 0 {
            continue;
        }

        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }

    out.trim().to_string()
}

fn fold_char_to_ascii_lower(ch: char, out: &mut String) {
    if ch.is_ascii() {
        out.push(ch.to_ascii_lowercase());
        return;
    }
    match ch {
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => {
            out.push('a')
        }
        'Æ' | 'æ' => out.push_str("ae"),
        'Ç' | 'ç' => out.push('c'),
        'È' | 'É' | 'Ê' | 'Ë' | 'è' | 'é' | 'ê' | 'ë' => out.push('e'),
        'Ì' | 'Í' | 'Î' | 'Ï' | 'ì' | 'í' | 'î' | 'ï' => out.push('i'),
        'Ñ' | 'ñ' => out.push('n'),
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => {
            out.push('o')
        }
        'Œ' | 'œ' => out.push_str("oe"),
        'Ù' | 'Ú' | 'Û' | 'Ü' | 'ù' | 'ú' | 'û' | 'ü' => out.push('u'),
        'Ý' | 'Ÿ' | 'ý' | 'ÿ' => out.push('y'),
        'ß' => out.push_str("ss"),
        _ => {}
    }
}

fn is_leading_article(word: &str) -> bool {
    matches!(
        word,
        "the" | "les" | "le" | "la" | "die" | "der" | "das" | "el" | "los" | "las"
    )
}

fn is_inverted_article(word: &str) -> bool {
    word == "a" || word == "an" || is_leading_article(word)
}

pub fn normalize_marquee_key(raw: &str) -> String {
    let stripped = strip_rom_or_art_extension_and_tags(raw);
    if stripped.is_empty() {
        return String::new();
    }

    // 1. Fold UTF-8 Latin accents to lowercase ASCII
    let mut folded = String::with_capacity(stripped.len());
    for ch in stripped.chars() {
        fold_char_to_ascii_lower(ch, &mut folded);
    }

    // 2. Strip inverted articles (`, the`, `, a`, `, an`, `, les`, `, die`, `, el`) before EOS or ` - `
    let fb = folded.as_bytes();
    let mut cleaned = String::with_capacity(fb.len());
    let mut i = 0usize;
    while i < fb.len() {
        if fb[i] == b',' {
            let mut j = i + 1;
            while j < fb.len() && fb[j] == b' ' {
                j += 1;
            }
            let w_start = j;
            while j < fb.len() && fb[j].is_ascii_lowercase() {
                j += 1;
            }
            if j > w_start {
                let word = &folded[w_start..j];
                if is_inverted_article(word) {
                    let mut after = j;
                    while after < fb.len() && fb[after] == b' ' {
                        after += 1;
                    }
                    if after == fb.len()
                        || (after + 2 <= fb.len() && fb[after] == b'-' && fb[after + 1] == b' ')
                    {
                        i = j;
                        continue;
                    }
                }
            }
        }
        cleaned.push(fb[i] as char);
        i += 1;
    }

    // 3. Strip leading article (`the `, `les `, `le `, `la `, etc.) when followed by another word
    let trimmed = cleaned.trim_start();
    let start_slice = if let Some((first_word, rest)) = trimmed.split_once(' ') {
        let rest_trimmed = rest.trim_start();
        if !rest_trimmed.is_empty()
            && first_word.bytes().all(|b| b.is_ascii_lowercase())
            && is_leading_article(first_word)
        {
            rest_trimmed
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    // 4. Compact to alphanumeric key, expanding '&' -> "and"
    let mut compacted = String::with_capacity(start_slice.len());
    for ch in start_slice.chars() {
        if ch.is_ascii_alphanumeric() {
            compacted.push(ch);
        } else if ch == '&' {
            compacted.push_str("and");
        }
    }

    // 5. Normalize European TMNT censor rename ("heroturtles" -> "ninjaturtles")
    if compacted.contains("heroturtles") {
        compacted = compacted.replace("heroturtles", "ninjaturtles");
    }

    compacted
}

fn resolve_regional_alias_norm(norm_key: &str) -> Option<&'static str> {
    if norm_key.is_empty() {
        return None;
    }
    const ALIASES: &[(&str, &str)] = &[
        ("starwing", "starfox"),
        ("lylatwars", "starfox64"),
        ("castlevaniavampireskiss", "castlevaniadraculax"),
        ("castlevaniathenewgeneration", "castlevaniabloodlines"),
        ("castlevanianewgeneration", "castlevaniabloodlines"),
        ("superprobotectoralienrebels", "contraiiithealienwars"),
        ("probotector2returnoftheevilforces", "superc"),
        ("mysticquestlegend", "finalfantasymysticquest"),
        ("kirbysfunpak", "kirbysuperstar"),
        ("kirbysghosttrap", "kirbysavalanche"),
        ("illusionoftime", "illusionofgaia"),
        ("anotherworld", "outofthisworld"),
        ("jetsetradio", "jetgrindradio"),
        ("bareknuckle", "streetsofrage"),
        ("bareknuckleii", "streetsofrage2"),
        ("bareknuckle2", "streetsofrage2"),
        ("bareknuckleiii", "streetsofrage3"),
        ("bareknuckle3", "streetsofrage3"),
        ("pokemonversionrouge", "pokemonredversion"),
        ("pokemonversionbleue", "pokemonblueversion"),
        ("pokemonversionjaune", "pokemonyellowversion"),
        ("pokemonversionor", "pokemongoldversion"),
        ("pokemonversionargent", "pokemonsilverversion"),
        ("pokemonversioncristal", "pokemoncrystalversion"),
        ("pokemonversionrubis", "pokemonrubyversion"),
        ("pokemonversionsaphir", "pokemonsapphireversion"),
        ("pokemonversionemeraude", "pokemonemeraldversion"),
        ("pokemonversionrougefeu", "pokemonfireredversion"),
        ("pokemonversionvertfeuille", "pokemonleafgreenversion"),
        ("pokemonversiondiamant", "pokemondiamondversion"),
        ("pokemonversionperle", "pokemonpearlversion"),
        ("pokemonversionplatine", "pokemonplatinumversion"),
        ("pokemonversionorheartgold", "pokemonheartgoldversion"),
        ("pokemonversionamesoulsilver", "pokemonsoulsilverversion"),
        ("pokemonversionargentsoulsilver", "pokemonsoulsilverversion"),
        ("pokemonversionnoire", "pokemonblackversion"),
        ("pokemonversionblanche", "pokemonwhiteversion"),
        ("pokemonversionnoire2", "pokemonblackversion2"),
        ("pokemonversionblanche2", "pokemonwhiteversion2"),
    ];
    for &(regional, canonical) in ALIASES {
        if norm_key == regional {
            return Some(canonical);
        }
        if norm_key == canonical {
            return Some(regional);
        }
    }
    None
}

fn extract_main_title_before_subtitle(stripped: &str) -> Option<&str> {
    let mut best_pos: Option<usize> = None;
    for sep in &[" - ", ": ", " : "] {
        if let Some(pos) = stripped.find(sep) {
            if pos > 0 && best_pos.map_or(true, |cur| pos < cur) {
                best_pos = Some(pos);
            }
        }
    }
    let pos = best_pos?;
    let head = stripped[..pos].trim_end();
    if head.is_empty() {
        None
    } else {
        Some(head)
    }
}

fn skip_four_digit_release_prefix(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    if bytes.len() > 5
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
        && bytes[3].is_ascii_digit()
    {
        let rest = s[4..].trim_start();
        if let Some(after_dash) = rest.strip_prefix('-') {
            let trimmed = after_dash.trim_start();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

fn contains_ascii_case_insensitive(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let hb = haystack.as_bytes();
    let nb = needle.as_bytes();
    if nb.len() > hb.len() {
        return false;
    }
    hb.windows(nb.len()).any(|win| win.eq_ignore_ascii_case(nb))
}

pub fn score_catalog_entry(
    candidate_file: &str,
    raw_game: &str,
    raw_title: &str,
    norm_game: Option<&str>,
    norm_title: Option<&str>,
) -> i32 {
    if candidate_file.len() < 5 {
        return 0;
    }
    let split_ext = candidate_file.len() - 4;
    let Some(ext) = candidate_file.get(split_ext..) else {
        return 0;
    };
    let is_png = if ext.eq_ignore_ascii_case(".png") {
        true
    } else if ext.eq_ignore_ascii_case(".gif") {
        false
    } else {
        return 0;
    };

    let cand_stem = &candidate_file[..split_ext];
    let g_stem = if !raw_game.is_empty() {
        strip_known_rom_or_image_ext(raw_game)
    } else {
        ""
    };
    let t_stem = if !raw_title.is_empty() {
        strip_known_rom_or_image_ext(raw_title)
    } else {
        ""
    };

    let cand_stripped = strip_rom_or_art_extension_and_tags(cand_stem);
    let g_stripped = if !g_stem.is_empty() {
        strip_rom_or_art_extension_and_tags(g_stem)
    } else {
        String::new()
    };
    let t_stripped = if !t_stem.is_empty() {
        strip_rom_or_art_extension_and_tags(t_stem)
    } else {
        String::new()
    };

    let owned_ng;
    let ng = match norm_game {
        Some(n) => n,
        None if !g_stripped.is_empty() => {
            owned_ng = normalize_marquee_key(&g_stripped);
            owned_ng.as_str()
        }
        None => "",
    };

    let owned_nt;
    let nt = match norm_title {
        Some(n) => n,
        None if !t_stripped.is_empty() => {
            owned_nt = normalize_marquee_key(&t_stripped);
            owned_nt.as_str()
        }
        None => "",
    };

    let cand_norm = normalize_marquee_key(&cand_stripped);
    let after_num_pfx = skip_four_digit_release_prefix(&cand_stripped);
    let cand_norm_no_num = after_num_pfx.map(normalize_marquee_key).unwrap_or_default();

    let ng_alias = resolve_regional_alias_norm(ng);
    let nt_alias = resolve_regional_alias_norm(nt);

    let mut base_score = if !g_stem.is_empty() && cand_stem.eq_ignore_ascii_case(g_stem) {
        1000
    } else if !t_stem.is_empty() && cand_stem.eq_ignore_ascii_case(t_stem) {
        980
    } else if !g_stripped.is_empty() && cand_stripped.eq_ignore_ascii_case(&g_stripped) {
        880
    } else if !t_stripped.is_empty() && cand_stripped.eq_ignore_ascii_case(&t_stripped) {
        860
    } else if !ng.is_empty()
        && (cand_norm == ng || (!cand_norm_no_num.is_empty() && cand_norm_no_num == ng))
    {
        760
    } else if !nt.is_empty()
        && (cand_norm == nt || (!cand_norm_no_num.is_empty() && cand_norm_no_num == nt))
    {
        750
    } else if let Some(alias) = ng_alias
        .filter(|a| cand_norm == *a || (!cand_norm_no_num.is_empty() && cand_norm_no_num == *a))
    {
        let _ = alias;
        720
    } else if let Some(alias) = nt_alias
        .filter(|a| cand_norm == *a || (!cand_norm_no_num.is_empty() && cand_norm_no_num == *a))
    {
        let _ = alias;
        710
    } else {
        0
    };

    if base_score == 0 {
        // Subtitle-aware matching (strictly bounded at explicit ` - ` / `: ` delimiter when one side
        // omits the subtitle, checking both `cand_stripped` and `after_num_pfx` so 4-digit year games
        // like `1943 (Europe)` -> `1943 - The Battle of Midway (USA).png` AND NDS numbered releases work).
        let cand_main_norm = extract_main_title_before_subtitle(&cand_stripped)
            .map(normalize_marquee_key)
            .unwrap_or_default();
        let cand_main_after_num_norm = after_num_pfx
            .and_then(extract_main_title_before_subtitle)
            .map(normalize_marquee_key)
            .unwrap_or_default();

        let g_main_norm = extract_main_title_before_subtitle(&g_stripped)
            .map(normalize_marquee_key)
            .unwrap_or_default();
        let t_main_norm = extract_main_title_before_subtitle(&t_stripped)
            .map(normalize_marquee_key)
            .unwrap_or_default();

        let matches_main =
            |m: &str, target: &str| -> bool { m.len() >= 4 && !target.is_empty() && m == target };

        if matches_main(&cand_main_norm, ng)
            || matches_main(&cand_main_norm, nt)
            || matches_main(&cand_main_after_num_norm, ng)
            || matches_main(&cand_main_after_num_norm, nt)
        {
            base_score = 540;
        } else if cand_norm.len() >= 5 && ((g_main_norm == cand_norm) || (t_main_norm == cand_norm))
        {
            base_score = 530;
        }
    }

    if base_score == 0 {
        return 0;
    }

    let mut bonus = if is_png { 4 } else { 0 };
    if !cand_stem.contains('(') {
        bonus += 28;
    } else if contains_ascii_case_insensitive(cand_stem, "(World)") {
        bonus += 30;
    } else if contains_ascii_case_insensitive(cand_stem, "(Europe)")
        || contains_ascii_case_insensitive(cand_stem, "(USA, Europe)")
        || contains_ascii_case_insensitive(cand_stem, "(France)")
    {
        bonus += 26;
    } else if contains_ascii_case_insensitive(cand_stem, "(USA)")
        || contains_ascii_case_insensitive(cand_stem, "(US)")
    {
        bonus += 24;
    } else if contains_ascii_case_insensitive(cand_stem, "(Japan)")
        || contains_ascii_case_insensitive(cand_stem, "(JP)")
    {
        bonus += 10;
    }

    const PENALTY_TAGS: &[&str] = &[
        "(Proto", "(Beta", "(Demo", "(Sample", "(Pirate", "(Unl)", "(Hack", "[b", "[h", "(Track",
    ];
    if PENALTY_TAGS
        .iter()
        .any(|tag| contains_ascii_case_insensitive(cand_stem, tag))
    {
        bonus -= 25;
    }

    if contains_ascii_case_insensitive(cand_stem, "(Disc 1)")
        || contains_ascii_case_insensitive(cand_stem, "(Disk 1)")
    {
        bonus += 2;
    } else if contains_ascii_case_insensitive(cand_stem, "(Disc ")
        || contains_ascii_case_insensitive(cand_stem, "(Disk ")
    {
        bonus -= 5;
    }

    (base_score + bonus).max(1)
}

pub fn score_system_catalog_entry<S: AsRef<str>>(
    candidate_file: &str,
    sys_variants: &[(&str, S)],
) -> i32 {
    if candidate_file.len() < 5 {
        return 0;
    }
    let split_ext = candidate_file.len() - 4;
    let Some(ext) = candidate_file.get(split_ext..) else {
        return 0;
    };
    let is_png = if ext.eq_ignore_ascii_case(".png") {
        true
    } else if ext.eq_ignore_ascii_case(".gif") {
        false
    } else {
        return 0;
    };

    let stem = &candidate_file[..split_ext];
    for (idx, (_, target)) in sys_variants.iter().enumerate() {
        if stem.eq_ignore_ascii_case(target.as_ref()) {
            let rank_score = 2000 - (idx as i32 * 4) + if is_png { 2 } else { 0 };
            return rank_score.max(10);
        }
    }
    0
}

pub fn url_encode_segment(raw: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(raw.len() + 8);
    for &b in raw.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0F) as usize] as char);
        }
    }
    out
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
        .split(['_', ' '])
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

    let sys_nospace = sys_lower.replace([' ', '_', '-'], "");
    let sys_underscore = sys_lower.replace(' ', "_");
    let raw_lower = raw_system.trim().to_lowercase();

    let mut name_variants: Vec<String> = Vec::new();

    // 1. First priority: Check custom / embedded mapping table (systems.json / console.csv)
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
            base_names.push("mame".into());
            base_names.push("- Arcade".into());
            base_names.push("arcade".into());
            base_names.push("Arcade".into());
            base_names.push("MAME".into());
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
        "saturn" | "segasaturn" => {
            base_names.push("sega_saturn".into());
            base_names.push("Saturn".into());
            base_names.push("SEGA Saturn".into());
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

    let mut unique_base: Vec<String> = Vec::new();
    for n in base_names {
        if !unique_base.contains(&n) {
            unique_base.push(n);
        }
    }

    let clean_lower = clean.to_lowercase();
    let clean_nospace = clean_lower.replace(' ', "");
    let clean_underscore = clean_lower.replace(' ', "_");
    let clean_kebab = clean_lower.replace(' ', "-");

    name_variants.push(clean_lower.clone());
    name_variants.push(clean.clone());
    name_variants.push(clean_underscore.clone());
    name_variants.push(clean_nospace.clone());
    name_variants.push(clean_kebab.clone());

    name_variants.push(format!("default-{}", clean));
    name_variants.push(format!("default-{}", clean_lower));
    name_variants.push(format!("default-{}", clean_underscore));
    name_variants.push(format!("default-{}", clean_nospace));
    name_variants.push(format!("default-{}", clean_kebab));
    name_variants.push(format!("default-_{}", clean));
    name_variants.push(format!("default-_{}", clean_lower));
    name_variants.push(format!("default-_{}", clean_underscore));

    name_variants.push(format!("default-z{}", clean_lower));
    name_variants.push(format!("default-z{}", clean_nospace));
    name_variants.push(format!("z{}", clean_lower));
    name_variants.push(format!("z{}", clean_nospace));

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

    final_names.into_iter().map(|n| ("console", n)).collect()
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

    #[test]
    fn test_sanitize_cache_component() {
        assert_eq!(
            sanitize_cache_component("../../etc/passwd"),
            "____etc_passwd"
        );
        assert_eq!(sanitize_cache_component("snes\ngame"), "snes_game");
        assert_eq!(sanitize_cache_component(".."), "_");
        assert_eq!(sanitize_cache_component("sf2"), "sf2");
    }

    #[test]
    fn test_folder_mapping_and_virtual_collection_extraction() {
        assert_eq!(map_system_to_pixelcade_folder("saturn"), "sega_saturn");
        assert_eq!(map_system_to_pixelcade_folder("nds"), "nintendo_ds");
        assert_eq!(map_system_to_pixelcade_folder("naomi"), "sega_naomi");
        assert_eq!(
            map_system_to_pixelcade_folder("atomiswave"),
            "sammy_atomiswave"
        );
        assert_eq!(map_system_to_pixelcade_folder("model3"), "mame");
        assert_eq!(
            map_system_to_pixelcade_folder("fds"),
            "nintendo_famicom_disk_system"
        );
        assert_eq!(
            map_system_to_pixelcade_folder("sfc"),
            "nintendo_super_famicom"
        );
        assert_eq!(map_system_to_pixelcade_folder("colecovision"), "coleco");
        assert_eq!(map_system_to_pixelcade_folder("ps3"), "sony_playstation_3");
        assert_eq!(map_system_to_pixelcade_folder("xbox"), "microsoft_xbox");

        assert_eq!(get_secondary_pixelcade_folder("neogeo"), Some("mame"));
        assert_eq!(
            get_secondary_pixelcade_folder("snes"),
            Some("nintendo_super_famicom")
        );
        assert_eq!(get_secondary_pixelcade_folder("naomi"), Some("mame"));

        assert!(is_known_empty_pixelcade_folder("msx"));
        assert!(is_known_empty_pixelcade_folder("zxspectrum"));
        assert!(!is_known_empty_pixelcade_folder("sega_saturn"));

        assert_eq!(
            extract_system_from_rom_path(
                "favorites",
                "/recalbox/share/roms/snes/Chrono Trigger (France).sfc"
            ),
            "snes"
        );
        assert_eq!(
            extract_system_from_rom_path("auto-lastplayed", "/userdata/roms/neogeo/mslugx.zip"),
            "neogeo"
        );
        assert_eq!(
            extract_system_from_rom_path("snes", "/userdata/roms/megadrive/sonic.bin"),
            "snes"
        );
    }

    #[test]
    fn test_catalog_scoring_regional_eu_fr_and_false_positive_guards() {
        // French / European region dump -> USA Pixelcade catalog entry
        let chrono = score_catalog_entry(
            "Chrono Trigger (USA).png",
            "Chrono Trigger (France)",
            "Chrono Trigger",
            None,
            None,
        );
        assert!(chrono >= 880, "expected >= 880, got {}", chrono);

        // Inverted article + accents
        let zelda = score_catalog_entry(
            "Legend of Zelda, The - A Link to the Past (USA).png",
            "The Legend of Zelda - A Link to the Past (Europe) (Fr,De)",
            "The Legend of Zelda: A Link to the Past",
            None,
            None,
        );
        assert!(zelda >= 760, "expected >= 760, got {}", zelda);

        // French Pokémon version -> USA version without cross-matching Ruby!
        let emeraude = score_catalog_entry(
            "Pokemon - Emerald Version (USA, Europe).png",
            "Pokemon - Version Emeraude (France)",
            "Pokémon : Version Émeraude",
            None,
            None,
        );
        assert!(emeraude >= 720, "expected >= 720, got {}", emeraude);

        let ruby_false = score_catalog_entry(
            "Pokemon - Ruby Version (USA, Europe).png",
            "Pokemon - Version Emeraude (France)",
            "Pokémon : Version Émeraude",
            None,
            None,
        );
        assert_eq!(
            ruby_false, 0,
            "Pokemon Emeraude must NEVER match Pokemon Ruby"
        );

        // European Hero Turtles -> USA Ninja Turtles
        let tmnt = score_catalog_entry(
            "Teenage Mutant Ninja Turtles IV - Turtles in Time (USA).png",
            "Teenage Mutant Hero Turtles IV - Turtles in Time (Europe)",
            "",
            None,
            None,
        );
        assert!(tmnt >= 760, "expected >= 760, got {}", tmnt);

        // Starwing (Europe) -> Star Fox (USA)
        let starwing =
            score_catalog_entry("Star Fox (USA).png", "Starwing (Europe)", "", None, None);
        assert!(starwing >= 720, "expected >= 720, got {}", starwing);

        // 4-digit year title (`1943`) AND NDS release numbering (`0168 - Mario Kart DS`)
        let m1943 = score_catalog_entry(
            "1943 - The Battle of Midway (USA).png",
            "1943 (Europe)",
            "",
            None,
            None,
        );
        assert!(m1943 >= 540, "expected >= 540, got {}", m1943);

        let mkds = score_catalog_entry(
            "0168 - Mario Kart DS (USA).png",
            "Mario Kart DS (Europe) (En,Fr,De,Es,It)",
            "Mario Kart DS",
            None,
            None,
        );
        assert!(mkds >= 760, "expected >= 760, got {}", mkds);
    }

    #[test]
    fn test_zero_404_catalog_miss_marks_negative_cache_locally() {
        let tmp_dir = std::env::temp_dir().join(format!(
            "arcadematrix_dmd_cache_catalog_miss_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp_dir);
        let cat_dir = tmp_dir.join("_catalog");
        fs::create_dir_all(&cat_dir).unwrap();
        fs::write(
            cat_dir.join("snes.txt"),
            "Chrono Trigger (USA).png\nSuper Mario World (USA).png\n",
        )
        .unwrap();
        fs::write(
            cat_dir.join("nintendo_super_famicom.txt"),
            "Bahamut Lagoon (Japan).png\n",
        )
        .unwrap();

        let cache = DmdCache::new(&tmp_dir);
        // Homebrew ROM is absent from both `snes.txt` and `nintendo_super_famicom.txt` -> 0 HTTP requests!
        assert!(cache
            .download_marquee_with_title("snes", "my_unreleased_homebrew_2026", None)
            .is_none());
        assert!(cache.is_negative_cached("snes", "my_unreleased_homebrew_2026"));

        let _ = fs::remove_dir_all(&tmp_dir);
    }
}

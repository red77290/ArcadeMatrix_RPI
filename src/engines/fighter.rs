use crate::core::matrix::MatrixBackend;
use byteorder::{LittleEndian, ReadBytesExt};
use image::{Rgb, RgbImage};
use rand::Rng;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

fn get_available_memory_bytes() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
            for line in content.lines() {
                if line.starts_with("MemAvailable:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<u64>() {
                            return kb * 1024;
                        }
                    }
                }
            }
        }
    }
    // Fallback default: 256 MB
    256 * 1024 * 1024
}

#[derive(Clone)]
pub struct FighterSprite {
    pub width: u32,
    pub height: u32,
    pub frames: Vec<RgbImage>,
    pub delays: Vec<u16>,
}

impl FighterSprite {
    pub fn load_fgt<P: AsRef<Path>>(path: P, max_frames: usize) -> Option<Self> {
        let file = File::open(path.as_ref()).ok()?;
        let is_gz = path.as_ref().extension().and_then(|e| e.to_str()) == Some("gz");

        let reader: Box<dyn std::io::Read> = if is_gz {
            Box::new(flate2::read::GzDecoder::new(file))
        } else {
            Box::new(file)
        };
        let mut reader = BufReader::new(reader);

        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic).ok()?;
        if magic != *b"FGT\x01" {
            tracing::warn!("FGT bad magic: {:?}", magic);
            return None;
        }

        let width = reader.read_u16::<LittleEndian>().ok()? as u32;
        let height = reader.read_u16::<LittleEndian>().ok()? as u32;
        let frame_count = reader.read_u16::<LittleEndian>().ok()? as usize;
        let _trans = reader.read_u16::<LittleEndian>().ok()?; // Transparent color (unused in this loop)

        // 1. Sanity check against corrupted file headers
        if width == 0 || height == 0 || width > 1024 || height > 1024 {
            tracing::warn!(
                "FGT invalid frame dimensions: {}x{} in {}",
                width,
                height,
                path.as_ref().display()
            );
            return None;
        }

        let frames_to_load = frame_count.min(max_frames);
        let pixel_count = (width * height) as usize;
        let frame_size = pixel_count * 3; // 3 bytes per pixel in RgbImage
        let total_projected_bytes = (frames_to_load * frame_size) as u64;

        // 2. RAM guard (proportional to available system RAM, similar to ESP32 PSRAM headroom)
        let avail_mem = get_available_memory_bytes();
        let safety_headroom = 64 * 1024 * 1024; // 64 MB OS/Display safety reserve
        let max_anim_bytes = 20 * 1024 * 1024; // 20 MB max per single animation file

        if total_projected_bytes > max_anim_bytes
            || avail_mem <= safety_headroom
            || total_projected_bytes > (avail_mem - safety_headroom)
        {
            tracing::warn!(
                "FGT animation too large ({} bytes, available RAM: {} bytes, headroom: {} bytes) for {}. Skipping.",
                total_projected_bytes,
                avail_mem,
                safety_headroom,
                path.as_ref().display()
            );
            return None;
        }

        // 1. Read all frame delays (frame_count * 2 bytes)
        let mut delays = Vec::with_capacity(frames_to_load);
        for i in 0..frame_count {
            let delay = reader.read_u16::<LittleEndian>().ok()?;
            if i < frames_to_load {
                delays.push(delay);
            }
        }

        // 2. Read frame pixel buffers
        let mut frames = Vec::with_capacity(frames_to_load);
        for i in 0..frame_count {
            if i < frames_to_load {
                let mut img = RgbImage::new(width, height);
                for p in 0..pixel_count {
                    let bgr565 = reader.read_u16::<LittleEndian>().ok()?;
                    let r = (((bgr565 >> 11) & 0x1F) as u32 * 255 / 31) as u8;
                    let g = (((bgr565 >> 5) & 0x3F) as u32 * 255 / 63) as u8;
                    let b = ((bgr565 & 0x1F) as u32 * 255 / 31) as u8;
                    let x = (p as u32) % width;
                    let y = (p as u32) / width;
                    img.put_pixel(x, y, Rgb([r, g, b]));
                }
                frames.push(img);
            } else {
                // Discard frames beyond frames_to_load without allocating memory
                let skip_bytes = (pixel_count * 2) as u64;
                let mut skipped = 0u64;
                while skipped < skip_bytes {
                    let chunk = (skip_bytes - skipped).min(65536);
                    let mut buf = [0u8; 65536];
                    let r = reader.read(&mut buf[..chunk as usize]).ok()?;
                    if r == 0 {
                        break;
                    }
                    skipped += r as u64;
                }
            }
        }

        Some(Self {
            width,
            height,
            frames,
            delays,
        })
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct FighterIndexMeta {
    pub height: i32,
    pub ground_y: i32,
    #[serde(default)]
    pub head_y: i32,
    #[serde(default)]
    pub origin_x: i32,
    pub width: i32,
    #[serde(default)]
    pub has_special: bool,
    #[serde(default)]
    pub has_super: bool,
}

#[derive(Clone)]
pub struct FighterChar {
    pub name: String,
    pub meta: FighterIndexMeta,
    pub anims: HashMap<String, FighterSprite>,
}

impl FighterChar {
    pub fn load_char<P: AsRef<Path>>(dir: P, meta: FighterIndexMeta) -> Option<Self> {
        let dir = dir.as_ref();
        let name = dir.file_name()?.to_str()?.to_string();
        let max_frames = 40;

        let mut anims = HashMap::new();

        let mut try_load = |state: &str, files: &[&str]| {
            for f in files {
                let path = dir.join(f);
                if let Some(sprite) = FighterSprite::load_fgt(&path, max_frames) {
                    tracing::debug!(
                        "FGT loaded {}: {}x{}, {} frames",
                        path.display(),
                        sprite.width,
                        sprite.height,
                        sprite.frames.len()
                    );
                    anims.insert(state.to_string(), sprite);
                    return true;
                }

                let gz_path = dir.join(format!("{}.gz", f));
                if let Some(sprite) = FighterSprite::load_fgt(&gz_path, max_frames) {
                    tracing::debug!(
                        "FGT loaded {}: {}x{}, {} frames",
                        gz_path.display(),
                        sprite.width,
                        sprite.height,
                        sprite.frames.len()
                    );
                    anims.insert(state.to_string(), sprite);
                    return true;
                }
            }
            false
        };

        // Mandatory states with robust fallbacks
        for action in &["walk", "attack", "hit", "win"] {
            if !try_load(action, &[&format!("{}.fgt", action)]) {
                // Fallbacks
                match *action {
                    "walk" => {
                        if !try_load("walk", &["stand.fgt"]) {
                            tracing::warn!("Failed to load walk.fgt or stand.fgt for {}", name);
                            return None;
                        }
                    }
                    "attack" => {
                        if !try_load(
                            "attack",
                            &[
                                "special1.fgt",
                                "super1.fgt",
                                "special2.fgt",
                                "super2.fgt",
                                "stand.fgt",
                                "walk.fgt",
                            ],
                        ) {
                            tracing::warn!("Failed to load attack fallback for {}", name);
                            return None;
                        }
                    }
                    "hit" => {
                        if !try_load("hit", &["fall.fgt", "dead.fgt", "stand.fgt", "walk.fgt"]) {
                            tracing::warn!("Failed to load hit fallback for {}", name);
                            return None;
                        }
                    }
                    "win" => {
                        if !try_load("win", &["stand.fgt", "walk.fgt"]) {
                            tracing::warn!("Failed to load win fallback for {}", name);
                            return None;
                        }
                    }
                    _ => {}
                }
            }
        }

        // Optional states
        for action in &[
            "special1", "special2", "special3", "super1", "super2", "super3", "fall", "dead",
        ] {
            try_load(action, &[&format!("{}.fgt", action)]);
        }

        Some(Self { name, meta, anims })
    }

    pub fn get_sprite(&self, state: &str) -> Option<&FighterSprite> {
        self.anims.get(state).or_else(|| self.anims.get("walk"))
    }
}

struct Player {
    character: FighterChar,
    x: f32,
    y: i32,
    state: String,
    frame_idx: usize,
    last_f: u128,
    dead: bool,
    dir: f32, // 1.0 (right) or -1.0 (left)
}

pub struct FighterEngine {
    matrix_width: u32,
    matrix_height: u32,
    p1: Option<Player>,
    p2: Option<Player>,
    active: bool,
    sprite_dir: String,
    last_move: u128,
    fight_end: u128,
    next_fight_time: u128,
    interval_sec: u32,
    loading: bool,
    rx: Option<Receiver<(Player, Player)>>,
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

impl FighterEngine {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            matrix_width: width,
            matrix_height: height,
            p1: None,
            p2: None,
            active: false,
            sprite_dir: String::new(),
            last_move: 0,
            fight_end: 0,
            next_fight_time: 0,
            interval_sec: 10,
            loading: false,
            rx: None,
        }
    }

    pub fn set_interval(&mut self, interval_sec: u32) {
        self.interval_sec = interval_sec.max(1);
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn stop(&mut self) {
        self.active = false;
        self.p1 = None;
        self.p2 = None;
        self.rx = None;
        self.loading = false;
    }

    fn load_index(dir: &str) -> HashMap<String, FighterIndexMeta> {
        let index_path = Path::new(dir).join("index.json");
        match std::fs::read_to_string(&index_path) {
            Ok(content) => {
                match serde_json::from_str::<HashMap<String, FighterIndexMeta>>(&content) {
                    Ok(index) => return index,
                    Err(e) => tracing::error!("Failed to parse {}: {}", index_path.display(), e),
                }
            }
            Err(e) => tracing::error!("Failed to read {}: {}", index_path.display(), e),
        }
        HashMap::new()
    }

    pub fn init_fight(&mut self, matrix_height: u32, interval_sec: u32) {
        if self.loading {
            return;
        }
        self.loading = true;
        self.matrix_height = matrix_height;
        self.set_interval(interval_sec);

        let (tx, rx) = channel();
        self.rx = Some(rx);
        let matrix_width = self.matrix_width;
        let matrix_height = self.matrix_height;

        thread::spawn(move || {
            let dir64 = "fighters_64";
            let dir32 = "fighters_32";

            // Pick the asset set that matches the panel height. A 64px (or taller)
            // panel prefers fighters_64; anything shorter uses fighters_32. We only
            // touch the other resolution as a last-resort fallback, so a 32px panel
            // never logs spurious errors about the fighters_64 assets.
            let (preferred, fallback) = if matrix_height >= 64 {
                (dir64, dir32)
            } else {
                (dir32, dir64)
            };

            let mut dir = preferred;
            let mut index = Self::load_index(preferred);

            if index.is_empty() {
                let alt = Self::load_index(fallback);
                if !alt.is_empty() {
                    index = alt;
                    dir = fallback;
                }
            }

            if index.is_empty() {
                tracing::warn!(
                    "FighterEngine: No valid index.json found (looked in '{}' for a \
                     {}px panel). Are the fighter sprite assets deployed?",
                    preferred,
                    matrix_height
                );
                return;
            }

            let fighters: Vec<(String, FighterIndexMeta)> = index.into_iter().collect();
            if fighters.len() < 2 {
                tracing::warn!("FighterEngine: Not enough characters in index.json");
                return;
            }

            let mut rng = rand::thread_rng();

            let idx1 = rng.gen_range(0..fighters.len());
            let (name1, meta1) = fighters[idx1].clone();

            let h1 = if meta1.height > 0 {
                meta1.height as f32
            } else if (meta1.ground_y - meta1.head_y) > 5 {
                (meta1.ground_y - meta1.head_y) as f32
            } else {
                32.0
            };

            let valid_opponents: Vec<&(String, FighterIndexMeta)> = fighters
                .iter()
                .filter(|(name, meta)| {
                    if name == &name1 {
                        return false;
                    }
                    let h2 = if meta.height > 0 {
                        meta.height as f32
                    } else if (meta.ground_y - meta.head_y) > 5 {
                        (meta.ground_y - meta.head_y) as f32
                    } else {
                        32.0
                    };
                    // P2 must be same height or up to 20% smaller (never taller than P1)
                    h2 <= h1 && (h2 / h1) >= 0.80
                })
                .collect();

            let (name2, meta2) = if !valid_opponents.is_empty() {
                let idx2 = rng.gen_range(0..valid_opponents.len());
                (*valid_opponents[idx2]).clone()
            } else {
                // Find opponent smaller than or closest to P1
                let mut candidates: Vec<&(String, FighterIndexMeta)> =
                    fighters.iter().filter(|(name, _)| name != &name1).collect();
                candidates.sort_by(|(_, m_a), (_, m_b)| {
                    let h_a = if m_a.height > 0 {
                        m_a.height as f32
                    } else {
                        32.0
                    };
                    let h_b = if m_b.height > 0 {
                        m_b.height as f32
                    } else {
                        32.0
                    };
                    // Prioritize candidates <= h1, closest to h1
                    let score_a = if h_a <= h1 {
                        h1 - h_a
                    } else {
                        (h_a - h1) + 1000.0
                    };
                    let score_b = if h_b <= h1 {
                        h1 - h_b
                    } else {
                        (h_b - h1) + 1000.0
                    };
                    score_a
                        .partial_cmp(&score_b)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                if !candidates.is_empty() {
                    (*candidates[0]).clone()
                } else {
                    fighters[0].clone()
                }
            };

            tracing::info!(
                "🥊 FighterEngine: Match -> [P1: {}] (H:{}) vs [P2: {}] (H:{})",
                name1,
                meta1.height,
                name2,
                meta2.height
            );

            let char1_dir = Path::new(dir).join(&name1);
            let char2_dir = Path::new(dir).join(&name2);

            let c1 = FighterChar::load_char(&char1_dir, meta1.clone());
            let c2 = FighterChar::load_char(&char2_dir, meta2.clone());

            if let (Some(c1), Some(c2)) = (c1, c2) {
                // Place P1 at 1 pixel from the top of the screen
                let y1 = 1 - c1.meta.head_y as i32;
                // Align ground line to P1's physical feet
                let ground_y_screen = y1 + c1.meta.ground_y as i32;
                // Place P2 on the exact same ground line
                let y2 = ground_y_screen - c2.meta.ground_y as i32;

                let mut mirrored_c2 = c2.clone();
                for anim in mirrored_c2.anims.values_mut() {
                    for frame in &mut anim.frames {
                        let w = frame.width();
                        let h = frame.height();
                        let mut flipped = RgbImage::new(w, h);
                        for y in 0..h {
                            for x in 0..w {
                                flipped.put_pixel(w - 1 - x, y, *frame.get_pixel(x, y));
                            }
                        }
                        *frame = flipped;
                    }
                }

                let p1 = Player {
                    character: c1,
                    x: -(meta1.width as f32),
                    y: y1,
                    state: "walk".to_string(),
                    frame_idx: 0,
                    last_f: now_ms(),
                    dead: false,
                    dir: 1.0,
                };

                let p2 = Player {
                    character: mirrored_c2,
                    x: matrix_width as f32,
                    y: y2,
                    state: "walk".to_string(),
                    frame_idx: 0,
                    last_f: now_ms(),
                    dead: false,
                    dir: -1.0,
                };

                let _ = tx.send((p1, p2));
            } else {
                tracing::warn!(
                    "FighterEngine: Skipping fight between {} and {} (failed safety limits or missing sprites)",
                    name1,
                    name2
                );
            }
        });
    }

    pub fn composite(&mut self, matrix: &mut dyn MatrixBackend) {
        let now = now_ms();

        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok((p1, p2)) => {
                    self.p1 = Some(p1);
                    self.p2 = Some(p2);
                    self.active = true;
                    self.loading = false;
                    self.rx = None;
                    self.last_move = now;
                    self.fight_end = 0;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    tracing::error!(
                        "FighterEngine: background thread died or failed to load fighters"
                    );
                    self.loading = false;
                    self.active = false;
                    self.rx = None;
                    // Wait at least 10s before trying again to avoid spamming thread spawn
                    self.next_fight_time = now + (self.interval_sec as u128 * 1000).max(10000);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {} // Still loading
            }
        }

        if !self.active {
            if !self.loading && now >= self.next_fight_time {
                let h = self.matrix_height;
                let i = self.interval_sec;
                self.init_fight(h, i);
            }
            return;
        }

        // Update animation frames
        if let Some(ref mut p1) = self.p1 {
            Self::update_anim(p1, now);
        }
        if let Some(ref mut p2) = self.p2 {
            Self::update_anim(p2, now);
        }

        // Movement & Logic
        if let (Some(ref mut p1), Some(ref mut p2)) = (&mut self.p1, &mut self.p2) {
            if p1.state == "walk" && p2.state == "walk" {
                let elapsed = now.saturating_sub(self.last_move);
                if elapsed >= 35 {
                    let px_move = (elapsed / 35) as f32;
                    p1.x += px_move;
                    p2.x -= px_move;
                    self.last_move += (px_move as u128) * 35;

                    let p1_world_origin = p1.x + p1.character.meta.origin_x as f32;
                    let p2_world_origin =
                        p2.x + (p2.character.meta.width as f32 - p2.character.meta.origin_x as f32);
                    let dist = p2_world_origin - p1_world_origin;

                    let scale = if self.matrix_height >= 64 { 2.0 } else { 1.0 };
                    let engage_dist = 18.0 * scale;

                    if dist <= engage_dist {
                        let mut rng = rand::thread_rng();
                        let p1_attacks = rng.gen_bool(0.5);

                        let (attacker, target) = if p1_attacks {
                            (&mut *p1, &mut *p2)
                        } else {
                            (&mut *p2, &mut *p1)
                        };

                        let mut atk_state = "attack".to_string();
                        let mut tgt_state = "hit".to_string();

                        let r: f32 = rng.gen();
                        if attacker.character.meta.has_super && r < 0.50 {
                            let supers: Vec<String> = attacker
                                .character
                                .anims
                                .keys()
                                .filter(|k| k.starts_with("super"))
                                .cloned()
                                .collect();
                            if !supers.is_empty() {
                                atk_state = supers[rng.gen_range(0..supers.len())].clone();
                                tgt_state = if target.character.anims.contains_key("fall") {
                                    "fall".to_string()
                                } else {
                                    "hit".to_string()
                                };
                            }
                        } else if attacker.character.meta.has_special && r < 0.80 {
                            let specials: Vec<String> = attacker
                                .character
                                .anims
                                .keys()
                                .filter(|k| k.starts_with("special"))
                                .cloned()
                                .collect();
                            if !specials.is_empty() {
                                atk_state = specials[rng.gen_range(0..specials.len())].clone();
                                tgt_state = if target.character.anims.contains_key("fall") {
                                    "fall".to_string()
                                } else {
                                    "hit".to_string()
                                };
                            }
                        }

                        attacker.state = atk_state;
                        target.state = tgt_state;

                        attacker.frame_idx = 0;
                        target.frame_idx = 0;
                        attacker.last_f = now;
                        target.last_f = now;
                    }
                }
            }

            if p1.state == "fall" {
                p1.x += p1.dir * -2.0;
            }
            if p2.state == "fall" {
                p2.x += p2.dir * -2.0;
            }

            if self.fight_end == 0 && (p1.dead || p2.dead) {
                self.fight_end = now;
            }

            if self.fight_end > 0 && now.saturating_sub(self.fight_end) > 2000 {
                self.active = false;
                self.next_fight_time = now + (self.interval_sec as u128 * 1000);
            }
        }

        // Draw (loser behind, winner in front)
        if let (Some(p1), Some(p2)) = (&self.p1, &self.p2) {
            if p1.dead || p1.state == "hit" || p1.state == "fall" {
                Self::draw_player(matrix, p1);
                Self::draw_player(matrix, p2);
            } else {
                Self::draw_player(matrix, p2);
                Self::draw_player(matrix, p1);
            }
        }
    }

    fn update_anim(p: &mut Player, now: u128) {
        if let Some(anim) = p.character.get_sprite(&p.state) {
            let mut delay =
                anim.delays[p.frame_idx.min(anim.delays.len().saturating_sub(1))] as u128;
            if delay < 30 {
                delay = 30;
            }

            if now.saturating_sub(p.last_f) > delay {
                p.frame_idx += 1;
                p.last_f = now;

                if p.frame_idx >= anim.frames.len() {
                    let s = p.state.as_str();
                    if s == "walk" {
                        p.frame_idx = 0;
                    } else if s.starts_with("attack")
                        || s.starts_with("special")
                        || s.starts_with("super")
                    {
                        p.state = "win".to_string();
                        p.frame_idx = 0;
                    } else if s == "hit" || s == "fall" {
                        p.frame_idx = anim.frames.len().saturating_sub(1);
                        p.dead = true;
                    } else if s == "win" {
                        p.frame_idx = anim.frames.len().saturating_sub(1);
                    }
                }

                if p.state.starts_with("special") || p.state.starts_with("super") {
                    p.x += p.dir * 2.0;
                }
            }
        }
    }

    fn draw_player(matrix: &mut dyn MatrixBackend, p: &Player) {
        if let Some(sprite) = p.character.get_sprite(&p.state) {
            let frame = &sprite.frames[p.frame_idx.min(sprite.frames.len().saturating_sub(1))];
            let start_x = p.x as i32;
            let start_y = p.y;

            let w = frame.width() as i32;
            let h = frame.height() as i32;
            let mat_w = matrix.width() as i32;
            let mat_h = matrix.height() as i32;

            let y_min = (0 - start_y).max(0);
            let y_max = (mat_h - start_y).min(h);
            let x_min = (0 - start_x).max(0);
            let x_max = (mat_w - start_x).min(w);

            for y in y_min..y_max {
                for x in x_min..x_max {
                    let px = frame.get_pixel(x as u32, y as u32);
                    if px[0] > 0 || px[1] > 0 || px[2] > 0 {
                        matrix.set_pixel(start_x + x, start_y + y, px[0], px[1], px[2]);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fighter_index_parsing() {
        let json = r#"{
            "test_char": {
                "height": 144,
                "ground_y": 126,
                "head_y": -30,
                "origin_x": -15,
                "width": 211,
                "has_special": true,
                "has_super": false
            }
        }"#;

        let parsed: Result<HashMap<String, FighterIndexMeta>, _> = serde_json::from_str(json);
        assert!(parsed.is_ok());
        let map = parsed.unwrap();

        let meta = map.get("test_char").unwrap();
        assert_eq!(meta.head_y, -30);
        assert_eq!(meta.origin_x, -15);
        assert_eq!(meta.has_special, true);
        assert_eq!(meta.has_super, false);
    }
}

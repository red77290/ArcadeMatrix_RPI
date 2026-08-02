use crate::core::matrix::MatrixBackend;
use byteorder::{LittleEndian, ReadBytesExt};
use image::{Rgb, RgbImage};
use rand::Rng;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FighterState {
    Walk,
    Attack,
    Hit,
    Win,
    Dead,
}

#[derive(Clone)]
pub struct FighterSprite {
    pub width: u32,
    pub height: u32,
    pub frames: Vec<RgbImage>,
}

impl FighterSprite {
    pub fn load_fgt<P: AsRef<Path>>(path: P) -> Option<Self> {
        let file = File::open(path).ok()?;
        let mut reader = BufReader::new(file);

        let width = reader.read_u16::<LittleEndian>().ok()? as u32;
        let height = reader.read_u16::<LittleEndian>().ok()? as u32;
        let frame_count = reader.read_u16::<LittleEndian>().ok()? as usize;

        let mut frames = Vec::new();
        let pixel_count = (width * height) as usize;

        for _ in 0..frame_count {
            let mut img = RgbImage::new(width, height);
            for i in 0..pixel_count {
                let bgr565 = reader.read_u16::<LittleEndian>().ok()?;
                let r = (((bgr565 >> 11) & 0x1F) as u32 * 255 / 31) as u8;
                let g = (((bgr565 >> 5) & 0x3F) as u32 * 255 / 63) as u8;
                let b = ((bgr565 & 0x1F) as u32 * 255 / 31) as u8;
                let x = (i as u32) % width;
                let y = (i as u32) / width;
                img.put_pixel(x, y, Rgb([r, g, b]));
            }
            frames.push(img);
        }

        Some(Self {
            width,
            height,
            frames,
        })
    }
}

struct Player {
    sprite: FighterSprite,
    x: f32,
    state: FighterState,
    frame_idx: usize,
    state_timer: u32,
    flip: bool, // Mirror sprite horizontally
}

pub struct FighterEngine {
    matrix_width: u32,
    matrix_height: u32,
    p1: Option<Player>,
    p2: Option<Player>,
    fight_cooldown: u32,
    active: bool,
    sprite_dir: String,
}

impl FighterEngine {
    pub fn new(width: u32) -> Self {
        Self {
            matrix_width: width,
            matrix_height: 64,
            p1: None,
            p2: None,
            fight_cooldown: 0,
            active: false,
            sprite_dir: String::new(),
        }
    }

    fn scan_fighters(dir: &str) -> Vec<String> {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) == Some("fgt") {
                    p.to_str().map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    fn load_random_pair(dir: &str) -> Option<(FighterSprite, FighterSprite)> {
        let files = Self::scan_fighters(dir);
        if files.len() < 2 {
            return None;
        }
        let mut rng = rand::thread_rng();
        let idx1 = rng.gen_range(0..files.len());
        let mut idx2 = rng.gen_range(0..files.len());
        while idx2 == idx1 {
            idx2 = rng.gen_range(0..files.len());
        }
        let s1 = FighterSprite::load_fgt(&files[idx1])?;
        let s2 = FighterSprite::load_fgt(&files[idx2])?;
        Some((s1, s2))
    }

    pub fn init_fight(&mut self, matrix_height: u32) {
        self.matrix_height = matrix_height;

        // Choose sprite directory based on matrix height
        let dir64 = "fighters_64";
        let dir32 = "fighters_32";
        let dir = if matrix_height >= 64 && std::path::Path::new(dir64).exists() {
            dir64
        } else {
            dir32
        };
        self.sprite_dir = dir.to_string();

        if let Some((s1, s2)) = Self::load_random_pair(dir) {
            let w = self.matrix_width as f32;
            self.p1 = Some(Player {
                x: 4.0,
                state: FighterState::Walk,
                frame_idx: 0,
                state_timer: 0,
                flip: false,
                sprite: s1,
            });
            self.p2 = Some(Player {
                x: w - 24.0,
                state: FighterState::Walk,
                frame_idx: 0,
                state_timer: 0,
                flip: true,
                sprite: s2,
            });
            self.active = true;
            self.fight_cooldown = 0;
        }
    }

    /// Overlay fighters on top of the current matrix frame.
    /// Call this after every other engine renders.
    pub fn composite(&mut self, matrix: &mut dyn MatrixBackend) {
        if !self.active {
            return;
        }

        let w = self.matrix_width as f32;
        let h = self.matrix_height as i32;

        let mut rng = rand::thread_rng();

        // Advance P1 AI
        if let Some(ref mut p1) = self.p1 {
            let p2_x = self.p2.as_ref().map(|p| p.x).unwrap_or(w / 2.0);
            Self::advance_ai(p1, p2_x, true, &mut rng);
        }
        if let Some(ref mut p2) = self.p2 {
            let p1_x = self.p1.as_ref().map(|p| p.x).unwrap_or(w / 2.0);
            Self::advance_ai(p2, p1_x, false, &mut rng);
        }

        // Check hitbox: if close enough and attacking, trigger hit on opponent
        if let (Some(ref mut p1), Some(ref mut p2)) = (&mut self.p1, &mut self.p2) {
            let dist = (p1.x - p2.x).abs();
            if dist < (p1.sprite.width + 4) as f32 {
                if p1.state == FighterState::Attack && p2.state == FighterState::Walk {
                    p2.state = FighterState::Hit;
                    p2.state_timer = 0;
                }
                if p2.state == FighterState::Attack && p1.state == FighterState::Walk {
                    p1.state = FighterState::Hit;
                    p1.state_timer = 0;
                }
            }

            // Check win/dead
            let p1_dead = p1.state == FighterState::Dead;
            let p2_dead = p2.state == FighterState::Dead;
            if p1_dead || p2_dead {
                self.fight_cooldown += 1;
                if self.fight_cooldown > 60 {
                    // Reset with new fighters
                    let dir = self.sprite_dir.clone();
                    let height = self.matrix_height;
                    self.p1 = None;
                    self.p2 = None;
                    self.active = false;
                    self.init_fight(height);
                    return;
                }
            }
        }

        // Draw P1
        if let Some(ref mut p1) = self.p1 {
            let frame = &p1.sprite.frames[p1.frame_idx % p1.sprite.frames.len().max(1)];
            let y = h - p1.sprite.height as i32;
            draw_sprite(matrix, frame, p1.x as i32, y, p1.flip);
            p1.frame_idx = p1.frame_idx.wrapping_add(1);
        }

        // Draw P2
        if let Some(ref mut p2) = self.p2 {
            let frame = &p2.sprite.frames[p2.frame_idx % p2.sprite.frames.len().max(1)];
            let y = h - p2.sprite.height as i32;
            draw_sprite(matrix, frame, p2.x as i32, y, p2.flip);
            p2.frame_idx = p2.frame_idx.wrapping_add(1);
        }
    }

    fn advance_ai(player: &mut Player, opponent_x: f32, is_p1: bool, rng: &mut impl Rng) {
        player.state_timer += 1;
        let speed = 1.2f32;

        match player.state {
            FighterState::Walk => {
                // Walk toward opponent
                let dir = if is_p1 {
                    if opponent_x > player.x {
                        1.0
                    } else {
                        -1.0
                    }
                } else {
                    if opponent_x < player.x {
                        -1.0
                    } else {
                        1.0
                    }
                };
                player.x += dir * speed;

                // Attack if close enough
                let dist = (player.x - opponent_x).abs();
                if dist < 20.0 && rng.gen_bool(0.05) {
                    player.state = FighterState::Attack;
                    player.state_timer = 0;
                }
            }
            FighterState::Attack => {
                if player.state_timer > 20 {
                    player.state = FighterState::Walk;
                    player.state_timer = 0;
                }
            }
            FighterState::Hit => {
                if player.state_timer > 15 {
                    player.state = if rng.gen_bool(0.1) {
                        FighterState::Dead
                    } else {
                        FighterState::Walk
                    };
                    player.state_timer = 0;
                }
            }
            FighterState::Win => {}
            FighterState::Dead => {}
        }
    }
}

fn draw_sprite(matrix: &mut dyn MatrixBackend, img: &RgbImage, x: i32, y: i32, flip: bool) {
    let (w, h) = img.dimensions();
    for iy in 0..h {
        for ix in 0..w {
            let px = img.get_pixel(ix, iy);
            // Treat pure black as transparent
            if px[0] == 0 && px[1] == 0 && px[2] == 0 {
                continue;
            }
            let draw_x = if flip {
                x + (w - 1 - ix) as i32
            } else {
                x + ix as i32
            };
            matrix.set_pixel(draw_x, y + iy as i32, px[0], px[1], px[2]);
        }
    }
}

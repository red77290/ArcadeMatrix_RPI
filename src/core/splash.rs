use crate::core::matrix::MatrixBackend;
use crate::engines::renderers::BaseRenderer;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ============================================================================
// AUTHENTIC 1981 DONKEY KONG ARCADE JUMPMAN SPRITES (ARGB 0xAARRGGBB)
// ============================================================================

const N: u32 = 0x00000000; // Transparent
const R: u32 = 0xFFF02020; // 1981 DK Jumpman Red (Cap & Overalls)
const B: u32 = 0xFF1048E8; // 1981 DK Jumpman Blue (Shirt & Shoes)
const S: u32 = 0xFFFFA070; // 1981 DK Jumpman Skin Peach
const D: u32 = 0xFF381800; // 1981 DK Dark Brown Hair & Mustache
const H: u32 = 0xFFD8E0F0; // 1981 DK Steel Hammer Head (Highlight)
const M: u32 = 0xFF303848; // 1981 DK Steel Hammer Head (Border/Shade)
const W: u32 = 0xFF984810; // 1981 DK Wooden Hammer Handle

// Donkey Kong 1981 Arcade: Jumpman with Hammer UP (16 wide x 22 tall)
const JUMPMAN_UP_WIDTH: u32 = 16;
const JUMPMAN_UP_HEIGHT: u32 = 22;
const JUMPMAN_HAMMER_UP: [u32; 352] = [
    N, N, N, N, M, M, M, M, M, M, M, M, N, N, N, N, N, N, N, M, H, H, H, H, H, H, H, H, M, N, N, N,
    N, N, N, M, H, H, H, H, H, H, H, H, M, N, N, N, N, N, N, M, H, H, H, H, H, H, H, H, M, N, N, N,
    N, N, N, N, M, M, M, W, W, M, M, M, N, N, N, N, N, N, N, N, N, N, N, W, W, N, N, N, N, N, N, N,
    N, N, N, N, N, N, N, W, W, N, N, N, N, N, N, N, N, N, N, N, N, R, R, R, R, R, R, N, N, N, N, N,
    N, N, N, N, R, R, R, R, R, R, R, R, R, N, N, N, N, N, N, N, D, D, D, S, S, D, S, N, N, N, N, N,
    N, N, N, D, S, D, S, S, S, D, S, S, S, N, N, N, N, N, N, D, S, D, D, S, S, S, D, S, S, S, N, N,
    N, N, N, D, D, S, S, S, S, D, D, D, D, N, N, N, N, N, N, N, N, S, S, S, S, S, S, S, N, N, N, N,
    N, N, N, N, B, B, R, B, B, B, N, N, N, N, N, N, N, N, N, B, B, B, R, B, B, R, B, B, B, N, N, N,
    N, N, B, B, B, B, R, R, R, R, B, B, B, B, N, N, N, N, S, S, B, R, S, R, R, S, R, B, S, S, N, N,
    N, N, N, N, N, R, R, R, R, R, R, N, N, N, N, N, N, N, N, N, R, R, R, N, N, R, R, R, N, N, N, N,
    N, N, N, B, B, B, B, N, N, B, B, B, B, N, N, N, N, N, B, B, B, B, B, N, N, B, B, B, B, B, N, N,
];

// Donkey Kong 1981 Arcade: Jumpman Hammer SMASH DOWN (24 wide x 16 tall)
const JUMPMAN_DOWN_WIDTH: u32 = 24;
const JUMPMAN_DOWN_HEIGHT: u32 = 16;
const JUMPMAN_HAMMER_DOWN: [u32; 384] = [
    N, N, N, N, N, N, N, N, N, N, N, N, R, R, R, R, R, R, N, N, N, N, N, N, N, N, N, N, N, N, N, N,
    N, N, N, R, R, R, R, R, R, R, R, R, N, N, N, N, N, N, N, N, N, N, N, N, N, N, N, D, D, D, S, S,
    D, S, N, N, N, N, N, N, N, N, N, N, N, N, N, N, N, N, D, S, D, S, S, S, D, S, S, S, N, N, N, N,
    N, N, N, N, N, N, N, N, N, N, D, S, D, D, S, S, S, D, S, S, S, N, N, N, N, N, N, N, N, N, N, N,
    N, N, D, D, S, S, S, S, D, D, D, D, N, N, N, N, N, N, N, N, N, N, N, N, N, N, N, N, S, S, S, S,
    S, S, S, N, N, N, N, N, N, N, N, N, N, N, N, N, N, N, N, B, B, R, B, B, B, N, N, N, N, N, N, N,
    N, N, N, N, N, N, N, N, N, N, B, B, B, R, B, B, R, B, B, B, N, N, N, N, N, N, N, N, N, N, N, N,
    N, B, B, B, B, R, R, R, R, B, B, B, B, N, N, N, N, N, N, N, N, N, N, N, N, S, S, B, R, S, R, R,
    S, R, B, S, S, N, N, N, N, N, N, N, N, N, N, N, N, N, N, R, R, R, R, R, R, W, W, W, W, W, W, N,
    N, N, N, N, N, N, N, N, N, N, R, R, R, N, N, R, M, M, M, M, M, M, M, M, N, N, N, N, N, N, N, N,
    N, B, B, B, B, N, N, M, H, H, H, H, H, H, H, M, N, N, N, N, N, N, N, N, B, B, B, B, B, N, N, M,
    H, H, H, H, H, H, H, M, N, N, N, N, N, N, N, N, N, N, N, N, N, N, N, M, M, M, M, M, M, M, M, M,
];

// ============================================================================
// AUTHENTIC 1978 SPACE INVADERS ARCADE SPRITES (ARGB 0xAARRGGBB)
// ============================================================================

const IR: u32 = 0xFFE52525; // 1978 UFO Red
const IY: u32 = 0xFFFFD020; // 1978 UFO Yellow Cockpit Lights
const IG: u32 = 0xFF00FF70; // 1978 Space Invaders Phosphor Green

// Space Invaders 1978 Arcade: Mystery Flying Saucer UFO (16 wide x 7 tall)
const INVADER_UFO_WIDTH: u32 = 16;
const INVADER_UFO_HEIGHT: u32 = 7;
const INVADER_UFO_SPRITE: [u32; 112] = [
    N, N, N, N, N, IR, IR, IR, IR, IR, IR, N, N, N, N, N, N, N, N, IR, IR, IR, IR, IR, IR, IR, IR,
    IR, N, N, N, N, N, N, IR, IR, IR, IR, IR, IR, IR, IR, IR, IR, IR, IR, N, N, N, IR, IR, IY, IY,
    IR, IY, IY, IR, IY, IY, IR, IR, IY, IR, N, IR, IR, IR, IR, IR, IR, IR, IR, IR, IR, IR, IR, IR,
    IR, IR, IR, N, N, N, IR, IR, IR, N, N, N, N, IR, IR, IR, N, N, N, N, N, IR, N, N, N, N, N, N,
    N, N, N, N, IR, N, N,
];

// Space Invaders 1978 Arcade: Crab Alien (11 wide x 8 tall) - Frame 0 (Tentacles Up)
const INVADER_CRAB_WIDTH: u32 = 11;
const INVADER_CRAB_HEIGHT: u32 = 8;
const INVADER_CRAB_FRAME0: [u32; 88] = [
    N, N, IG, N, N, N, N, N, IG, N, N, N, N, N, IG, N, N, N, IG, N, N, N, N, N, IG, IG, IG, IG, IG,
    IG, IG, N, N, N, IG, IG, N, IG, IG, IG, N, IG, IG, N, IG, IG, IG, IG, IG, IG, IG, IG, IG, IG,
    IG, IG, N, IG, IG, IG, IG, IG, IG, IG, N, IG, IG, N, IG, N, N, N, N, N, IG, N, IG, N, N, N, IG,
    IG, N, IG, IG, N, N, N,
];

// Space Invaders 1978 Arcade: Crab Alien (11 wide x 8 tall) - Frame 1 (Tentacles Down)
const INVADER_CRAB_FRAME1: [u32; 88] = [
    N, N, IG, N, N, N, N, N, IG, N, N, IG, N, N, IG, N, N, N, IG, N, N, IG, IG, N, IG, IG, IG, IG,
    IG, IG, IG, N, IG, IG, IG, IG, N, IG, IG, IG, N, IG, IG, IG, IG, IG, IG, IG, IG, IG, IG, IG,
    IG, IG, IG, N, IG, IG, IG, IG, IG, IG, IG, IG, IG, N, N, N, IG, N, N, N, N, N, IG, N, N, N, IG,
    N, N, N, N, N, N, N, IG, N,
];

// ============================================================================
// PARTICLE PHYSICS & VISUAL EFFECTS
// ============================================================================

#[derive(Debug, Clone)]
struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    r: u8,
    g: u8,
    b: u8,
    floor_y: f32,
    bounces: u8,
}

#[derive(Debug, Clone)]
struct Spark {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    r: u8,
    g: u8,
    b: u8,
    life: f32,
}

#[derive(Debug, Clone)]
struct Shockwave {
    cx: f32,
    cy: f32,
    radius: f32,
    max_radius: f32,
    r: u8,
    g: u8,
    b: u8,
    life: f32,
}

#[derive(Debug, Clone)]
struct ScorePopup {
    x: f32,
    y: f32,
    vy: f32,
    life: f32,
    r: u8,
    g: u8,
    b: u8,
}

#[derive(Debug, Clone)]
struct LaserBolt {
    x: f32,
    y: f32,
    vy: f32,
    r: u8,
    g: u8,
    b: u8,
}

#[derive(Debug, Clone)]
struct LetterInfo {
    char_val: char,
    center_x: f32,
    base_center_x: f32,
    top_y: f32,
    width: f32,
    height: f32,
    pixels: Vec<(i32, i32, (u8, u8, u8))>,
    rel_pixels: Vec<(i32, i32, (u8, u8, u8))>,
    broken: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SplashPhase {
    IntroGlow,
    ActiveAction,
}

pub struct SplashScreen {
    pub width: u32,
    pub height: u32,
    pub is_vertical: bool,
    ground_y: f32,
    two_line_layout: bool,
    letters: Vec<LetterInfo>,
    particles: Vec<Particle>,
    sparks: Vec<Spark>,
    shockwaves: Vec<Shockwave>,
    score_popups: Vec<ScorePopup>,
    // Horizontal Donkey Kong Jumpman splash state:
    pub jumpman_x: f32,
    pub jumpman_y: f32,
    jumpman_hammer_down: bool,
    // Vertical Space Invaders splash state (Patrols at bottom, shoots UP):
    invader_x: f32,
    invader_y: f32,
    invader_vx: f32,
    invader_shoot_timer: f32,
    invader_departed: bool,
    laser_bolts: Vec<LaserBolt>,
    title_scroll_x: f32,
    // Shared / 2D Pacman state:
    pacman_x: f32,
    pacman_y: f32,
    pacman_radius: i32,
    pacman_mouth_angle: i32,
    pacman_facing_dir: u8, // 0: Right, 1: Down, 2: Left, 3: Up
    pacman_active: bool,
    chomp_timer: f32,
    ghost_colors: [(u8, u8, u8); 4],
    tick: u32,
    screen_shake: f32,
    phase: SplashPhase,
    pub start_time: Instant,
    last_network_check: Instant,
    resolved_ip: String,
    is_connected: bool,
}

/// Checks local IP address non-blockingly via UDP routing socket and Linux network tools
pub fn resolve_local_ip() -> String {
    // 1. Direct UDP route inspection (instantaneous across all OSes)
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                let ip = addr.ip().to_string();
                if ip != "127.0.0.1" && !ip.is_empty() {
                    return ip;
                }
            }
        }
    }
    // 2. Linux / Raspberry Pi OS fallback: inspect local assigned IPv4 (works even without WAN access)
    #[cfg(target_os = "linux")]
    {
        if let Ok(out) = std::process::Command::new("hostname").arg("-I").output() {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout);
                for token in s.split_whitespace() {
                    if token != "127.0.0.1" && token.contains('.') {
                        return token.to_string();
                    }
                }
            }
        }
    }
    "127.0.0.1".to_string()
}

impl SplashScreen {
    pub fn new(width: u32, height: u32) -> Self {
        let base_renderer = BaseRenderer::new();
        let font = base_renderer.font();

        let is_vertical = height >= width;
        let mut letters = Vec::new();

        let ground_y = if height >= 64 {
            height as f32 - 12.0
        } else if height >= 48 {
            height as f32 - 8.0
        } else {
            height as f32 - 4.0
        };

        let mut two_line_layout = false;

        if is_vertical {
            // ================================================================
            // VERTICAL MODE: Scrolling "ARCADE MATRIX" in upper arena
            // ================================================================
            let title = "ARCADE MATRIX";
            let scale = 1.0f32;
            let (p, w, _) = font.get_pixel_map(title, scale);
            let start_y = if height >= 64 { 14 } else { 10 };
            let initial_x = (width as f32) + 4.0;

            for (idx, char_val) in title.chars().enumerate() {
                if let Some(char_pixels) = p.get(idx) {
                    let mut min_x = i32::MAX;
                    let mut max_x = i32::MIN;
                    let mut min_y = i32::MAX;
                    let mut max_y = i32::MIN;
                    let mut px_list = Vec::new();
                    let mut rel_list = Vec::new();

                    for &(px, py) in char_pixels {
                        min_x = min_x.min(px);
                        max_x = max_x.max(px);
                        min_y = min_y.min(py);
                        max_y = max_y.max(py);

                        let t = (px as f32 / w as f32).clamp(0.0, 1.0);
                        let r = (255.0 * t) as u8;
                        let g = (230.0 * (1.0 - t * 0.4)) as u8;
                        let b = (255.0 * (1.0 - t * 0.3)) as u8;
                        px_list.push((px, start_y + py, (r, g, b)));
                    }

                    if !px_list.is_empty() {
                        let letter_w = (max_x - min_x + 1).max(1) as f32;
                        let letter_h = (max_y - min_y + 1).max(1) as f32;
                        let base_cx = (min_x + max_x) as f32 / 2.0;

                        for &(px, py) in char_pixels {
                            let t = (px as f32 / w as f32).clamp(0.0, 1.0);
                            let r = (255.0 * t) as u8;
                            let g = (230.0 * (1.0 - t * 0.4)) as u8;
                            let b = (255.0 * (1.0 - t * 0.3)) as u8;
                            rel_list.push((px - base_cx as i32, py, (r, g, b)));
                        }

                        letters.push(LetterInfo {
                            char_val,
                            center_x: initial_x + base_cx,
                            base_center_x: initial_x + base_cx,
                            top_y: start_y as f32,
                            width: letter_w,
                            height: letter_h,
                            pixels: px_list,
                            rel_pixels: rel_list,
                            broken: false,
                        });
                    }
                }
            }
        } else {
            // ================================================================
            // HORIZONTAL MODE: Classic Donkey Kong Jumpman layout
            // ================================================================
            two_line_layout = height >= 52;
            if two_line_layout {
                let line1 = "ARCADE";
                let line2 = "MATRIX";
                let scale = if height >= 96 { 1.5f32 } else { 1.0f32 };

                let (p1, w1, _) = font.get_pixel_map(line1, scale);
                let start_x1 = (width as i32 - w1) / 2;
                let start_y1 = if height >= 64 { 8 } else { 4 };

                for (idx, char_val) in line1.chars().enumerate() {
                    if let Some(char_pixels) = p1.get(idx) {
                        let mut min_x = i32::MAX;
                        let mut max_x = i32::MIN;
                        let mut min_y = i32::MAX;
                        let mut max_y = i32::MIN;
                        let mut px_list = Vec::new();
                        let mut rel_list = Vec::new();

                        for &(px, py) in char_pixels {
                            let gx = start_x1 + px;
                            let gy = start_y1 + py;
                            min_x = min_x.min(gx);
                            max_x = max_x.max(gx);
                            min_y = min_y.min(gy);
                            max_y = max_y.max(gy);

                            let t = (gx as f32 / width as f32).clamp(0.0, 1.0);
                            let r = (255.0 * t) as u8;
                            let g = (220.0 * (1.0 - t * 0.4)) as u8;
                            let b = (255.0 * (1.0 - t * 0.2)) as u8;
                            px_list.push((gx, gy, (r, g, b)));
                            rel_list.push((px, py, (r, g, b)));
                        }

                        if !px_list.is_empty() {
                            letters.push(LetterInfo {
                                char_val,
                                center_x: (min_x + max_x) as f32 / 2.0,
                                base_center_x: (min_x + max_x) as f32 / 2.0,
                                top_y: min_y as f32,
                                width: (max_x - min_x + 1).max(1) as f32,
                                height: (max_y - min_y + 1).max(1) as f32,
                                pixels: px_list,
                                rel_pixels: rel_list,
                                broken: false,
                            });
                        }
                    }
                }

                let (p2, w2, _) = font.get_pixel_map(line2, scale);
                let start_x2 = (width as i32 - w2) / 2;
                let start_y2 = if height >= 64 { 22 } else { 16 };

                for (idx, char_val) in line2.chars().enumerate() {
                    if let Some(char_pixels) = p2.get(idx) {
                        let mut min_x = i32::MAX;
                        let mut max_x = i32::MIN;
                        let mut min_y = i32::MAX;
                        let mut max_y = i32::MIN;
                        let mut px_list = Vec::new();
                        let mut rel_list = Vec::new();

                        for &(px, py) in char_pixels {
                            let gx = start_x2 + px;
                            let gy = start_y2 + py;
                            min_x = min_x.min(gx);
                            max_x = max_x.max(gx);
                            min_y = min_y.min(gy);
                            max_y = max_y.max(gy);

                            let t = (gx as f32 / width as f32).clamp(0.0, 1.0);
                            let r = (255.0 * t * 0.8) as u8;
                            let g = 255;
                            let b = (120.0 * (1.0 - t)) as u8;
                            px_list.push((gx, gy, (r, g, b)));
                            rel_list.push((px, py, (r, g, b)));
                        }

                        if !px_list.is_empty() {
                            letters.push(LetterInfo {
                                char_val,
                                center_x: (min_x + max_x) as f32 / 2.0,
                                base_center_x: (min_x + max_x) as f32 / 2.0,
                                top_y: min_y as f32,
                                width: (max_x - min_x + 1).max(1) as f32,
                                height: (max_y - min_y + 1).max(1) as f32,
                                pixels: px_list,
                                rel_pixels: rel_list,
                                broken: false,
                            });
                        }
                    }
                }
            } else {
                let title = if width >= 80 {
                    "ARCADE MATRIX"
                } else {
                    "ARCADEMATRIX"
                };
                let scale = 1.0f32;
                let (p, w, _) = font.get_pixel_map(title, scale);
                let start_x = ((width as i32 - w) / 2).max(1);
                let start_y = 3;

                for (idx, char_val) in title.chars().enumerate() {
                    if let Some(char_pixels) = p.get(idx) {
                        let mut min_x = i32::MAX;
                        let mut max_x = i32::MIN;
                        let mut min_y = i32::MAX;
                        let mut max_y = i32::MIN;
                        let mut px_list = Vec::new();
                        let mut rel_list = Vec::new();

                        for &(px, py) in char_pixels {
                            let gx = start_x + px;
                            let gy = start_y + py;
                            min_x = min_x.min(gx);
                            max_x = max_x.max(gx);
                            min_y = min_y.min(gy);
                            max_y = max_y.max(gy);

                            let t = (gx as f32 / width as f32).clamp(0.0, 1.0);
                            let r = (255.0 * t) as u8;
                            let g = (230.0 * (1.0 - t * 0.4)) as u8;
                            let b = (255.0 * (1.0 - t * 0.3)) as u8;
                            px_list.push((gx, gy, (r, g, b)));
                            rel_list.push((px, py, (r, g, b)));
                        }

                        if !px_list.is_empty() {
                            letters.push(LetterInfo {
                                char_val,
                                center_x: (min_x + max_x) as f32 / 2.0,
                                base_center_x: (min_x + max_x) as f32 / 2.0,
                                top_y: min_y as f32,
                                width: (max_x - min_x + 1).max(1) as f32,
                                height: (max_y - min_y + 1).max(1) as f32,
                                pixels: px_list,
                                rel_pixels: rel_list,
                                broken: false,
                            });
                        }
                    }
                }
            }
        }

        let jumpman_y = ground_y - 22.0;
        let pacman_radius = if height >= 64 { 5 } else { 4 };

        let (pac_init_x, pac_init_y) = if is_vertical {
            (-24.0, (height as f32) * 0.46)
        } else {
            (-54.0, ground_y - pacman_radius as f32 - 1.0)
        };

        let invader_init_y = if is_vertical {
            (height as f32) - 10.0
        } else {
            4.0
        };

        Self {
            width,
            height,
            is_vertical,
            ground_y,
            two_line_layout,
            letters,
            particles: Vec::with_capacity(384),
            sparks: Vec::with_capacity(96),
            shockwaves: Vec::with_capacity(16),
            score_popups: Vec::with_capacity(16),
            jumpman_x: -28.0,
            jumpman_y,
            jumpman_hammer_down: false,
            invader_x: ((width as f32) * 0.5) - 8.0,
            invader_y: invader_init_y,
            invader_vx: 24.0,
            invader_shoot_timer: 0.0,
            invader_departed: false,
            laser_bolts: Vec::with_capacity(16),
            title_scroll_x: 0.0,
            pacman_x: pac_init_x,
            pacman_y: pac_init_y,
            pacman_radius,
            pacman_mouth_angle: 35,
            pacman_facing_dir: 0,
            pacman_active: !is_vertical,
            chomp_timer: 0.0,
            ghost_colors: [
                (255, 0, 0),     // Blinky (Red)
                (255, 184, 255), // Pinky (Pink)
                (0, 255, 255),   // Inky (Cyan)
                (255, 184, 82),  // Clyde (Orange)
            ],
            tick: 0,
            screen_shake: 0.0,
            phase: SplashPhase::IntroGlow,
            start_time: Instant::now(),
            last_network_check: Instant::now(),
            resolved_ip: resolve_local_ip(),
            is_connected: false,
        }
    }

    /// Renders the complete splash sequence until network is resolved or timeout reached
    pub fn run(
        matrix: &mut dyn MatrixBackend,
        running: &Arc<AtomicBool>,
        min_duration: Duration,
        max_timeout: Duration,
    ) -> String {
        let mut splash = Self::new(matrix.width(), matrix.height());
        let frame_duration = Duration::from_millis(25); // ~40 FPS arcade rendering

        tracing::info!(
            "Starting ArcadeMatrix Splash Screen (matrix {}x{}, vertical={})",
            matrix.width(),
            matrix.height(),
            splash.is_vertical
        );

        while running.load(Ordering::SeqCst) {
            let frame_start = Instant::now();

            splash.update(0.025);
            splash.render(matrix);
            matrix.update();

            let elapsed = splash.start_time.elapsed();

            // Periodic non-blocking network check
            if splash.last_network_check.elapsed() >= Duration::from_millis(300) {
                splash.last_network_check = Instant::now();
                let cur_ip = resolve_local_ip();
                if cur_ip != "127.0.0.1" {
                    if !splash.is_connected {
                        tracing::info!("Network online detected during splash screen: {}", cur_ip);
                    }
                    splash.resolved_ip = cur_ip;
                    splash.is_connected = true;
                }
            }

            // Completion check
            let animation_done = if splash.is_vertical {
                splash.invader_departed
                    && splash.pacman_active
                    && splash.particles.is_empty()
                    && splash.pacman_x > (splash.width as f32 + 16.0)
            } else {
                splash.jumpman_x > (splash.width as f32 + 28.0)
                    && splash.pacman_x > (splash.width as f32 + 16.0)
            };

            if elapsed >= min_duration {
                if splash.is_connected && animation_done {
                    tracing::info!(
                        "Splash animation complete with connected IP: {}",
                        splash.resolved_ip
                    );
                    break;
                }
                if elapsed >= max_timeout {
                    tracing::warn!(
                        "Splash screen reached timeout ({}s). Proceeding with IP: {}",
                        elapsed.as_secs(),
                        splash.resolved_ip
                    );
                    break;
                }
            }

            let frame_time = frame_start.elapsed();
            if frame_time < frame_duration {
                std::thread::sleep(frame_duration - frame_time);
            }
        }

        splash.resolved_ip
    }

    /// Advances physics, sprites, shockwaves, and destruction states
    pub fn update(&mut self, dt: f32) {
        self.tick = self.tick.wrapping_add(1);
        let elapsed = self.start_time.elapsed().as_secs_f32();

        // Screen shake decay
        self.screen_shake *= 0.82;

        if self.is_vertical {
            // ================================================================
            // VERTICAL MODE: SPACE INVADERS AT BOTTOM SHOOTING UP & 2D PACMAN
            // ================================================================
            if elapsed < 0.6 {
                self.phase = SplashPhase::IntroGlow;
                return;
            }
            self.phase = SplashPhase::ActiveAction;

            // 2. Title Horizontal Scrolling across the upper arena
            self.title_scroll_x += 16.0 * dt;
            for letter in &mut self.letters {
                letter.center_x = letter.base_center_x - self.title_scroll_x;
                if !letter.broken && letter.center_x < -12.0 {
                    letter.broken = true; // Auto-break letters that scrolled past
                }
            }

            let all_letters_broken = self.letters.iter().all(|l| l.broken);

            // 1. Space Invaders Patrol & Target Tracking at the bottom
            if !all_letters_broken {
                let target_letter = self.letters.iter().find(|l| {
                    !l.broken && l.center_x >= 0.0 && l.center_x <= (self.width as f32 + 6.0)
                });
                if let Some(tl) = target_letter {
                    let target_x =
                        (tl.center_x - 8.0).clamp(2.0, (self.width as f32 - 18.0).max(3.0));
                    let dx = target_x - self.invader_x;
                    if dx.abs() > 1.0 {
                        self.invader_vx = dx.signum() * 32.0;
                    }
                } else if self.invader_vx.abs() < 12.0 {
                    self.invader_vx = 24.0;
                }

                self.invader_x += self.invader_vx * dt;
                let min_invader_x = 2.0;
                let max_invader_x = (self.width as f32 - 18.0).max(min_invader_x + 1.0);
                if self.invader_x <= min_invader_x {
                    self.invader_x = min_invader_x;
                    self.invader_vx = self.invader_vx.abs();
                } else if self.invader_x >= max_invader_x {
                    self.invader_x = max_invader_x;
                    self.invader_vx = -self.invader_vx.abs();
                }

                // Rapid laser bursts to destroy letters quickly
                self.invader_shoot_timer += dt;
                if self.invader_shoot_timer >= 0.22 {
                    self.invader_shoot_timer = 0.0;
                    let ship_center_x = self.invader_x + 8.0;

                    self.laser_bolts.push(LaserBolt {
                        x: ship_center_x,
                        y: self.invader_y - 2.0,
                        vy: -68.0, // Negative velocity: travels UP
                        r: 255,
                        g: 35,
                        b: 35,
                    });
                }
            } else if !self.invader_departed {
                // "le vaisseau doit partir": Ship flies away off-screen at high speed!
                self.invader_x += 65.0 * dt;
                if self.invader_x > (self.width as f32 + 18.0) {
                    self.invader_departed = true;
                    // Trigger Pacman and trailing ghosts entry!
                    self.pacman_active = true;
                    self.pacman_x = -14.0;
                    self.pacman_y = (self.height as f32) * 0.46;
                }
            }

            // 4. Laser Bolt Collision & Letter Destruction
            for bolt in &mut self.laser_bolts {
                bolt.y += bolt.vy * dt;

                for letter in &mut self.letters {
                    if !letter.broken {
                        let dx = (bolt.x - letter.center_x).abs();
                        let dy = (bolt.y - (letter.top_y + letter.height * 0.5)).abs();

                        if dx < (letter.width / 2.0 + 3.0) && dy < 5.0 {
                            letter.broken = true;
                            self.screen_shake = 1.8;

                            // Energy shockwave ring
                            self.shockwaves.push(Shockwave {
                                cx: bolt.x,
                                cy: bolt.y,
                                radius: 1.0,
                                max_radius: (self.height as f32 * 0.28).clamp(8.0, 18.0),
                                r: 255,
                                g: 60,
                                b: 40,
                                life: 1.0,
                            });

                            // Sparks burst
                            for s in 0..10 {
                                let angle = (s as f32 * 36.0).to_radians();
                                let spd = 1.8 + (s % 3) as f32 * 0.8;
                                self.sparks.push(Spark {
                                    x: bolt.x,
                                    y: bolt.y,
                                    vx: angle.cos() * spd,
                                    vy: angle.sin() * spd + 0.8,
                                    r: 255,
                                    g: 220,
                                    b: 80,
                                    life: 1.0,
                                });
                            }

                            // Shatter into physical particles with 2D explosion velocity
                            for &(rx, ry, color) in &letter.rel_pixels {
                                let px = letter.center_x + rx as f32;
                                let py = letter.top_y + ry as f32;
                                let vx = (rx as f32 * 0.32) + ((rx % 3) as f32 - 1.0) * 0.6;
                                let vy = 0.8 + (ry as f32 * 0.22) + ((ry % 3) as f32 - 1.0) * 0.4;

                                self.particles.push(Particle {
                                    x: px,
                                    y: py,
                                    vx,
                                    vy,
                                    r: color.0,
                                    g: color.1,
                                    b: color.2,
                                    floor_y: self.invader_y - 2.0,
                                    bounces: 0,
                                });
                            }
                            bolt.y = -20.0; // Consume bolt
                            break;
                        }
                    }
                }
            }
            self.laser_bolts
                .retain(|b| b.y >= 0.0 && b.y < self.height as f32);

            // 5. Particle Physics with Left/Right Wall Bounces
            let gravity = 0.20;
            let max_w = self.width as f32 - 2.0;
            for p in &mut self.particles {
                p.x += p.vx;
                p.y += p.vy;
                p.vy += gravity;

                // Wall reflections
                if p.x <= 1.0 {
                    p.x = 1.0;
                    p.vx = -p.vx * 0.72;
                } else if p.x >= max_w {
                    p.x = max_w;
                    p.vx = -p.vx * 0.72;
                }

                // Floor bounce & friction
                if p.y >= p.floor_y {
                    p.y = p.floor_y;
                    p.vy = -p.vy * 0.48;
                    p.vx *= 0.86;
                    p.bounces = p.bounces.saturating_add(1);
                }
            }

            // 6. Pacman & Trailing Ghosts: run along bottom floor from left to right eating particles
            if self.pacman_active {
                self.chomp_timer += dt * 8.5;
                self.pacman_mouth_angle = ((self.chomp_timer.sin().abs()) * 45.0) as i32;
                self.pacman_facing_dir = 0; // Running Left -> Right

                let pac_speed = (self.width as f32 / 2.2).clamp(18.0, 32.0);
                self.pacman_x += pac_speed * dt;
                self.pacman_y = self.ground_y - (self.pacman_radius as f32) - 1.0;

                // 7. Pacman Eats Particles on the floor & Spawns Floating Scores
                let pac_cx = self.pacman_x;
                let pac_cy = self.pacman_y;
                let eat_rad = self.pacman_radius as f32 + 3.5;
                let mut eaten = 0;

                self.particles.retain(|p| {
                    if (p.x - pac_cx).abs() < eat_rad && (p.y - pac_cy).abs() < 10.0 {
                        eaten += 1;
                        false
                    } else {
                        true
                    }
                });

                if eaten > 0 && self.score_popups.len() < 8 {
                    self.score_popups.push(ScorePopup {
                        x: pac_cx,
                        y: pac_cy - 5.0,
                        vy: -15.0,
                        life: 1.0,
                        r: 255,
                        g: 255,
                        b: 100,
                    });
                }
            }
        } else {
            // ================================================================
            // HORIZONTAL MODE: JUMPMAN HAMMER SMASH & 1D PACMAN FLOOR RUNNER
            // ================================================================
            if elapsed < 1.0 {
                self.phase = SplashPhase::IntroGlow;
                return;
            }
            self.phase = SplashPhase::ActiveAction;

            let jumpman_speed = (self.width as f32 / 4.8).clamp(14.0, 32.0);
            self.jumpman_x += jumpman_speed * dt;

            let hammer_cycle = (((elapsed - 1.0) * 4.8) as usize) % 2;
            self.jumpman_hammer_down = hammer_cycle == 1;

            let pacman_target_x = self.jumpman_x - (self.width as f32 * 0.32).clamp(28.0, 48.0);
            self.pacman_x += (pacman_target_x - self.pacman_x) * (dt * 4.5);

            self.chomp_timer += dt * 8.0;
            self.pacman_mouth_angle = ((self.chomp_timer.sin().abs()) * 45.0) as i32;

            let hammer_impact_x = self.jumpman_x + 16.0;

            for letter in &mut self.letters {
                if !letter.broken {
                    if (hammer_impact_x - letter.center_x).abs() < (letter.width / 2.0 + 3.5) {
                        letter.broken = true;
                        self.jumpman_hammer_down = true;
                        self.screen_shake = 1.6;

                        self.shockwaves.push(Shockwave {
                            cx: hammer_impact_x,
                            cy: letter.top_y + letter.height * 0.5,
                            radius: 1.0,
                            max_radius: (self.height as f32 * 0.35).clamp(8.0, 22.0),
                            r: 255,
                            g: 220,
                            b: 100,
                            life: 1.0,
                        });

                        for s in 0..12 {
                            let angle = (s as f32 * 30.0).to_radians();
                            let spd = 1.6 + (s % 3) as f32 * 0.9;
                            self.sparks.push(Spark {
                                x: hammer_impact_x,
                                y: letter.top_y + letter.height * 0.4,
                                vx: angle.cos() * spd,
                                vy: angle.sin() * spd - 1.8,
                                r: 255,
                                g: 240,
                                b: 120,
                                life: 1.0,
                            });
                        }

                        for &(px, py, color) in &letter.pixels {
                            let vx = ((px as f32 - letter.center_x) * 0.28)
                                + ((px % 3) as f32 - 1.0) * 0.5;
                            let vy = -1.4 - ((py % 4) as f32 * 0.42);

                            self.particles.push(Particle {
                                x: px as f32,
                                y: py as f32,
                                vx,
                                vy,
                                r: color.0,
                                g: color.1,
                                b: color.2,
                                floor_y: self.ground_y + 2.0,
                                bounces: 0,
                            });
                        }
                    }
                }
            }

            let gravity = 0.28;
            for p in &mut self.particles {
                p.x += p.vx;
                p.y += p.vy;
                p.vy += gravity;

                if p.y >= p.floor_y {
                    p.y = p.floor_y;
                    p.vy = -p.vy * 0.44;
                    p.vx *= 0.84;
                    p.bounces = p.bounces.saturating_add(1);
                }
            }

            let pac_center_x = self.pacman_x;
            let pac_center_y = self.pacman_y;
            let eat_rad_sq = (self.pacman_radius as f32 + 3.5).powi(2);
            let mut eaten_count = 0;

            self.particles.retain(|p| {
                let dx = p.x - pac_center_x;
                let dy = p.y - pac_center_y;
                let dist_sq = dx * dx + dy * dy;

                if dist_sq < eat_rad_sq {
                    eaten_count += 1;
                    false
                } else {
                    true
                }
            });

            if eaten_count > 0 && self.score_popups.len() < 8 {
                self.score_popups.push(ScorePopup {
                    x: pac_center_x,
                    y: pac_center_y - 6.0,
                    vy: -14.0,
                    life: 1.0,
                    r: 255,
                    g: 255,
                    b: 100,
                });
            }
        }

        // Common effects update
        self.sparks.retain_mut(|s| {
            s.x += s.vx;
            s.y += s.vy;
            s.vy += 0.14;
            s.life -= dt * 3.4;
            s.life > 0.0
        });

        self.shockwaves.retain_mut(|sw| {
            sw.radius += dt * 42.0;
            sw.life = (1.0 - sw.radius / sw.max_radius).max(0.0);
            sw.life > 0.0
        });

        self.score_popups.retain_mut(|sp| {
            sp.y += sp.vy * dt;
            sp.life -= dt * 2.2;
            sp.life > 0.0
        });
    }

    /// Draws the complete frame onto the matrix backend with rich visual effects
    pub fn render(&self, matrix: &mut dyn MatrixBackend) {
        matrix.clear();

        let elapsed = self.start_time.elapsed().as_secs_f32();
        let shake_y = if self.screen_shake > 0.5 {
            ((self.tick % 2) as f32 * 2.0 - 1.0) * self.screen_shake
        } else {
            0.0
        };

        // 1. Dynamic Arcade Starfield
        let num_stars = (self.width / 8).clamp(8, 32) as usize;
        let star_max_y = if self.is_vertical {
            self.height as usize - 12
        } else {
            self.ground_y as usize
        };
        for i in 0..num_stars {
            let dot_x = ((i * 37 + 11) % (self.width as usize)) as i32;
            let dot_y = ((i * 23 + 7) % star_max_y) as i32;
            let pulse = ((elapsed * 3.2 + i as f32 * 1.3).sin() * 25.0) as i32 + 35;
            let b = pulse.clamp(10, 65) as u8;
            matrix.set_pixel(dot_x, dot_y, 0, b / 3, b);
        }

        if !self.is_vertical {
            // 2a. Donkey Kong Girder Platform (Horizontal mode)
            let girder_top = self.ground_y as i32 + 3;
            for x in 0..self.width as i32 {
                matrix.set_pixel(x, girder_top, 240, 45, 45);
                if girder_top + 1 < self.height as i32 {
                    matrix.set_pixel(x, girder_top + 1, 160, 25, 25);
                }
                if girder_top + 2 < self.height as i32 {
                    let is_rivet = (x % 8 == 0) || ((x + girder_top) % 6 == 0);
                    if is_rivet {
                        matrix.set_pixel(x, girder_top + 2, 255, 200, 100);
                    } else {
                        matrix.set_pixel(x, girder_top + 2, 100, 15, 15);
                    }
                }
            }
        }

        // 3. Shockwave Rings
        for sw in &self.shockwaves {
            let r_int = sw.radius as i32;
            let intensity = sw.life.clamp(0.0, 1.0);
            let cr = (sw.r as f32 * intensity) as u8;
            let cg = (sw.g as f32 * intensity) as u8;
            let cb = (sw.b as f32 * intensity) as u8;
            self.draw_circle(matrix, sw.cx as i32, sw.cy as i32, r_int, cr, cg, cb);
        }

        // 4. Unbroken Letters with Pulsing Neon Glow
        let glow = (elapsed * 4.5).sin() * 0.2 + 0.8;
        for letter in &self.letters {
            if !letter.broken {
                if self.is_vertical {
                    for &(rx, ry, (r, g, b)) in &letter.rel_pixels {
                        let px = (letter.center_x + rx as f32) as i32;
                        let py = (letter.top_y + ry as f32 + shake_y) as i32;
                        if px >= 0 && px < self.width as i32 && py >= 0 && py < self.height as i32 {
                            let gr = (r as f32 * glow).clamp(0.0, 255.0) as u8;
                            let gg = (g as f32 * glow).clamp(0.0, 255.0) as u8;
                            let gb = (b as f32 * glow).clamp(0.0, 255.0) as u8;
                            matrix.set_pixel(px, py, gr, gg, gb);
                        }
                    }
                } else {
                    for &(px, py, (r, g, b)) in &letter.pixels {
                        let draw_y = (py as f32 + shake_y) as i32;
                        let gr = (r as f32 * glow).clamp(0.0, 255.0) as u8;
                        let gg = (g as f32 * glow).clamp(0.0, 255.0) as u8;
                        let gb = (b as f32 * glow).clamp(0.0, 255.0) as u8;
                        matrix.set_pixel(px, draw_y, gr, gg, gb);
                    }
                }
            }
        }

        // 5. Letter Debris Particles
        for p in &self.particles {
            let px = p.x as i32;
            let py = p.y as i32;
            if px >= 0 && px < self.width as i32 && py >= 0 && py < self.height as i32 {
                matrix.set_pixel(px, py, p.r, p.g, p.b);
            }
        }

        // 6. Impact Sparks
        for s in &self.sparks {
            let px = s.x as i32;
            let py = s.y as i32;
            if px >= 0 && px < self.width as i32 && py >= 0 && py < self.height as i32 {
                let intensity = s.life.clamp(0.0, 1.0);
                matrix.set_pixel(
                    px,
                    py,
                    (s.r as f32 * intensity) as u8,
                    (s.g as f32 * intensity) as u8,
                    (s.b as f32 * intensity) as u8,
                );
            }
        }

        // 7. Score Popups
        for sp in &self.score_popups {
            let px = sp.x as i32;
            let py = sp.y as i32;
            let val = (255.0 * sp.life) as u8;
            if px >= 0 && px < self.width as i32 - 1 && py >= 0 && py < self.height as i32 {
                matrix.set_pixel(px, py, val, val, val / 2);
                matrix.set_pixel(px + 1, py, val, val, 0);
            }
        }

        if self.is_vertical {
            // ================================================================
            // 8a. VERTICAL: Laser Bolts from Space Invaders (Moving UPWARDS)
            // ================================================================
            for b in &self.laser_bolts {
                let bx = b.x as i32;
                let by = b.y as i32;
                if bx >= 0 && bx < self.width as i32 {
                    if by >= 0 && by < self.height as i32 {
                        matrix.set_pixel(bx, by, 255, 255, 255);
                    }
                    if by + 1 >= 0 && by + 1 < self.height as i32 {
                        matrix.set_pixel(bx, by + 1, 255, 200, 50);
                    }
                    if by + 2 >= 0 && by + 2 < self.height as i32 {
                        matrix.set_pixel(bx, by + 2, b.r, b.g, b.b);
                    }
                }
            }

            // 8b. Space Invaders Armada at the bottom (departs once letters are shattered)
            if !self.invader_departed && self.invader_x < (self.width as f32 + 20.0) {
                let crab_frame = (self.tick / 10) % 2 == 1;
                let crab_sprite = if crab_frame {
                    &INVADER_CRAB_FRAME1
                } else {
                    &INVADER_CRAB_FRAME0
                };

                // Command UFO (16x7) at the bottom
                self.draw_sprite(
                    matrix,
                    &INVADER_UFO_SPRITE,
                    INVADER_UFO_WIDTH,
                    INVADER_UFO_HEIGHT,
                    self.invader_x as i32,
                    self.invader_y as i32,
                );

                // Flanking Crab Aliens if screen is wide enough (>= 48px)
                if self.width >= 48 {
                    let left_crab_x = (self.invader_x - 14.0) as i32;
                    let right_crab_x = (self.invader_x + 19.0) as i32;
                    self.draw_sprite(
                        matrix,
                        crab_sprite,
                        INVADER_CRAB_WIDTH,
                        INVADER_CRAB_HEIGHT,
                        left_crab_x,
                        self.invader_y as i32,
                    );
                    self.draw_sprite(
                        matrix,
                        crab_sprite,
                        INVADER_CRAB_WIDTH,
                        INVADER_CRAB_HEIGHT,
                        right_crab_x,
                        self.invader_y as i32,
                    );
                }
            }

            // 8c. Pacman & Trailing Ghosts along the bottom floor (running Left -> Right)
            if self.pacman_active {
                let ghost_spacing = (self.pacman_radius * 2 + 3) as i32;
                for (i, &gc) in self.ghost_colors.iter().enumerate() {
                    let gx = self.pacman_x as i32
                        - (self.pacman_radius * 3)
                        - (i as i32 * ghost_spacing);
                    let gy_offset = ((self.tick as f32 * 0.25 + i as f32 * 1.2).sin()
                        * (self.pacman_radius as f32 * 0.35))
                        as i32;
                    self.draw_ghost(
                        matrix,
                        gx,
                        self.pacman_y as i32 + gy_offset,
                        self.pacman_radius - 1,
                        gc,
                        self.tick,
                    );
                }

                self.draw_pacman_directional(
                    matrix,
                    self.pacman_x as i32,
                    self.pacman_y as i32,
                    self.pacman_radius,
                    self.pacman_mouth_angle,
                    0, // Facing Right
                );
            }
        } else {
            // ================================================================
            // 8b. HORIZONTAL: Donkey Kong Jumpman & 1D Pacman
            // ================================================================
            let ghost_spacing = (self.pacman_radius * 2 + 3) as i32;
            for (i, &gc) in self.ghost_colors.iter().enumerate() {
                let gx =
                    self.pacman_x as i32 - (self.pacman_radius * 3) - (i as i32 * ghost_spacing);
                let gy_offset = ((self.tick as f32 * 0.25 + i as f32 * 1.2).sin()
                    * (self.pacman_radius as f32 * 0.35)) as i32;
                self.draw_ghost(
                    matrix,
                    gx,
                    self.pacman_y as i32 + gy_offset,
                    self.pacman_radius - 1,
                    gc,
                    self.tick,
                );
            }

            self.draw_pacman_directional(
                matrix,
                self.pacman_x as i32,
                self.pacman_y as i32,
                self.pacman_radius,
                self.pacman_mouth_angle,
                0, // Rightward
            );

            if self.jumpman_hammer_down {
                self.draw_sprite(
                    matrix,
                    &JUMPMAN_HAMMER_DOWN,
                    JUMPMAN_DOWN_WIDTH,
                    JUMPMAN_DOWN_HEIGHT,
                    self.jumpman_x as i32,
                    (self.ground_y - 16.0) as i32,
                );
            } else {
                self.draw_sprite(
                    matrix,
                    &JUMPMAN_HAMMER_UP,
                    JUMPMAN_UP_WIDTH,
                    JUMPMAN_UP_HEIGHT,
                    self.jumpman_x as i32,
                    (self.ground_y - 22.0) as i32,
                );
            }
        }

        // 9. Cyber Network Status Indicator (Top right in vertical, bottom right in horizontal)
        let (status_x, status_y) = if self.is_vertical {
            (self.width as i32 - 5, 2)
        } else {
            (self.width as i32 - 6, self.height as i32 - 3)
        };

        if self.is_connected {
            matrix.set_pixel(status_x, status_y, 0, 255, 120);
            matrix.set_pixel(status_x + 1, status_y, 0, 255, 120);
            matrix.set_pixel(status_x + 2, status_y, 0, 255, 120);
        } else {
            let scan = (self.tick / 4) % 3;
            for dot in 0..3 {
                if dot == scan {
                    matrix.set_pixel(status_x + dot as i32, status_y, 255, 180, 0);
                } else {
                    matrix.set_pixel(status_x + dot as i32, status_y, 80, 50, 0);
                }
            }
        }
    }

    /// Draws a circle shockwave ring
    fn draw_circle(
        &self,
        matrix: &mut dyn MatrixBackend,
        cx: i32,
        cy: i32,
        r: i32,
        cr: u8,
        cg: u8,
        cb: u8,
    ) {
        if r <= 0 {
            return;
        }
        let mut x = r;
        let mut y = 0;
        let mut err = 0;

        while x >= y {
            let pts = [
                (cx + x, cy + y),
                (cx + y, cy + x),
                (cx - y, cy + x),
                (cx - x, cy + y),
                (cx - x, cy - y),
                (cx - y, cy - x),
                (cx + y, cy - x),
                (cx + x, cy - y),
            ];
            for &(px, py) in &pts {
                if px >= 0 && px < self.width as i32 && py >= 0 && py < self.height as i32 {
                    matrix.set_pixel(px, py, cr, cg, cb);
                }
            }
            y += 1;
            err += 1 + 2 * y;
            if 2 * (err - x) + 1 > 0 {
                x -= 1;
                err += 1 - 2 * x;
            }
        }
    }

    /// Blits an ARGB sprite onto the matrix backend with transparent pixel skipping
    #[inline(always)]
    fn draw_sprite(
        &self,
        matrix: &mut dyn MatrixBackend,
        sprite: &[u32],
        w: u32,
        h: u32,
        offset_x: i32,
        offset_y: i32,
    ) {
        if offset_x + w as i32 <= 0 || offset_x >= self.width as i32 {
            return;
        }

        for y in 0..h as i32 {
            let py = offset_y + y;
            if py < 0 || py >= self.height as i32 {
                continue;
            }

            let row_idx = (y as usize) * (w as usize);
            for x in 0..w as i32 {
                let px = offset_x + x;
                if px < 0 || px >= self.width as i32 {
                    continue;
                }

                let color = sprite[row_idx + x as usize];
                if color != 0 {
                    let r = ((color >> 16) & 0xFF) as u8;
                    let g = ((color >> 8) & 0xFF) as u8;
                    let b = (color & 0xFF) as u8;
                    matrix.set_pixel(px, py, r, g, b);
                }
            }
        }
    }

    /// Draws Pacman with directional mouth opening (0: Right, 1: Down, 2: Left, 3: Up)
    fn draw_pacman_directional(
        &self,
        matrix: &mut dyn MatrixBackend,
        cx: i32,
        cy: i32,
        r: i32,
        mouth_deg: i32,
        direction: u8,
    ) {
        if cx + r < 0 || cx - r >= self.width as i32 || cy + r < 0 || cy - r >= self.height as i32 {
            return;
        }
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy > r * r {
                    continue;
                }
                let in_mouth = match direction {
                    0 => dx > 0 && dy.abs() * 45 < dx * mouth_deg,
                    1 => dy > 0 && dx.abs() * 45 < dy * mouth_deg,
                    2 => dx < 0 && dy.abs() * 45 < (-dx) * mouth_deg,
                    3 => dy < 0 && dx.abs() * 45 < (-dy) * mouth_deg,
                    _ => dx > 0 && dy.abs() * 45 < dx * mouth_deg,
                };
                if !in_mouth {
                    matrix.set_pixel(cx + dx, cy + dy, 255, 255, 0);
                }
            }
        }
    }

    /// Draws Ghost with animated tentacles and eye tracking
    fn draw_ghost(
        &self,
        matrix: &mut dyn MatrixBackend,
        cx: i32,
        cy: i32,
        r: i32,
        color: (u8, u8, u8),
        tick: u32,
    ) {
        if cx + r < 0 || cx - r >= self.width as i32 || cy + r < 0 || cy - r >= self.height as i32 {
            return;
        }
        for dy in -r..=0i32 {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r {
                    matrix.set_pixel(cx + dx, cy + dy, color.0, color.1, color.2);
                }
            }
        }
        for dy in 0..=r {
            for dx in -r..=r {
                matrix.set_pixel(cx + dx, cy + dy, color.0, color.1, color.2);
            }
        }
        let wave = (tick / 3) % 2 == 0;
        for i in 0..3i32 {
            let tx = cx - r + i * (r * 2 / 3) + r / 3;
            let bottom_y = cy + r;
            if (i % 2 == 0) == wave {
                matrix.set_pixel(tx, bottom_y + 1, 0, 0, 0);
            }
        }
        matrix.set_pixel(cx - r / 2, cy - 1, 255, 255, 255);
        matrix.set_pixel(cx + r / 2, cy - 1, 255, 255, 255);
        matrix.set_pixel(cx - r / 2 + 1, cy - 1, 0, 0, 200);
        matrix.set_pixel(cx + r / 2 + 1, cy - 1, 0, 0, 200);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::matrix::MockMatrix;

    #[test]
    fn test_splash_screen_initialization_64x64() {
        let splash = SplashScreen::new(64, 64);
        assert_eq!(splash.width, 64);
        assert_eq!(splash.height, 64);
        assert!(splash.is_vertical);
        assert!(!splash.letters.is_empty());
        assert_eq!(splash.particles.len(), 0);
    }

    #[test]
    fn test_splash_screen_initialization_64x32() {
        let splash = SplashScreen::new(64, 32);
        assert_eq!(splash.width, 64);
        assert_eq!(splash.height, 32);
        assert!(!splash.is_vertical);
        assert!(!splash.letters.is_empty());
    }

    #[test]
    fn test_splash_screen_particle_physics() {
        let mut splash = SplashScreen::new(64, 32);
        splash.start_time = Instant::now() - Duration::from_millis(1500);
        let first_letter_x = splash.letters.first().map(|l| l.center_x).unwrap_or(20.0);
        splash.jumpman_x = first_letter_x - 12.0;
        splash.update(0.1);

        assert!(splash.particles.len() > 0 || splash.letters.iter().any(|l| l.broken));
    }

    #[test]
    fn test_splash_screen_vertical_space_invader_and_pacman_2d() {
        let mut splash = SplashScreen::new(64, 64);
        assert!(splash.is_vertical);
        assert_eq!(splash.invader_y, 54.0); // At the bottom on 64x64
        assert!(!splash.pacman_active); // Pacman waits off-screen until letters are destroyed

        splash.start_time = Instant::now() - Duration::from_millis(1000);

        // Run update cycles while letters are intact: Space Invader fires upwards
        for _ in 0..10 {
            splash.update(0.05);
        }
        assert!(splash.invader_x >= 0.0 && splash.invader_x <= 64.0);
        assert_eq!(splash.invader_y, 54.0);

        // Break all letters -> Ship departs off-screen, then Pacman and ghosts enter
        for l in &mut splash.letters {
            l.broken = true;
        }
        for _ in 0..30 {
            splash.update(0.05);
        }
        assert!(splash.invader_departed);
        assert!(splash.pacman_active);

        for _ in 0..20 {
            splash.update(0.05);
        }
        assert!(splash.pacman_x >= -20.0 && splash.pacman_x <= 80.0);
        assert!(splash.pacman_y >= 0.0 && splash.pacman_y <= 64.0);
    }

    #[test]
    fn test_splash_screen_render() {
        let splash = SplashScreen::new(64, 64);
        let mut mock = MockMatrix::new(64, 64);
        splash.render(&mut mock);
        let non_black = mock
            .canvas
            .pixels()
            .any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0);
        assert!(non_black);
    }
}

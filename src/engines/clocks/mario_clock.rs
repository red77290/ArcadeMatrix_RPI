//! Super Mario Bros clock face with full multi-resolution and orientation parity with ESP32.
//!
//! Features:
//! - Full support for wide/tall panels, Tate/portrait orientation, and compact 128x32 panels (Zero "NEEDS 256x64" notices).
//! - Portrait Mode (h > w): Stepped brick platforms at y=70, 130, 190 with a 7-stage climbing state machine.
//! - Compact 128x32 Mode: Mario idle on left (x=14), Green Koopa shell kick (280 px/s) + gold coin pop on minute change.
//! - Wide/Tall Mode (h >= 48): Mario 3-frame run cycle, jump attack with dynamic parabolic arc, clouds, hill, bush.
//! - Canonical `IDrawingSurface` integration.

use crate::core::matrix::IDrawingSurface;
use crate::engines::clocks::cwassets::mario::{
    BLOCK, BUSH, CLOUD1, CLOUD2, CLOUD_FULL, GOLD_COIN, GROUND, HILL, KOOPA_H, KOOPA_SHELL,
    KOOPA_SHELL_H, KOOPA_SHELL_W, KOOPA_W, KOOPA_WALK1, KOOPA_WALK2, MARIO_IDLE, MARIO_JUMP,
    MARIO_RUN1, MARIO_RUN2, MARIO_RUN3, MARIO_RUN_LEFT, MARIO_RUN_W, SKY_COLOR,
};
use crate::engines::clocks::cwscene;
use crate::engines::renderers::base_renderer::ArcadeFont;
use crate::engines::renderers::BaseRenderer;
use std::time::Instant;

const BLOCK_W: i32 = 19;
const BLOCK_H: i32 = 19;
const GROUND_W: i32 = 8;
const GROUND_H: i32 = 8;
const HILL_W: i32 = 20;
const HILL_H: i32 = 22;
const BUSH_W: i32 = 21;
const BUSH_H: i32 = 9;
const CLOUD_W: i32 = 13;
const CLOUD_H: i32 = 12;
const CLOUD_FULL_W: i32 = 19;
const CLOUD_FULL_H: i32 = 12;
const MARIO_W: i32 = 13;
const MARIO_H: i32 = 16;
const MARIO_JUMP_W: i32 = 17;
const SCENE: i32 = 64;

#[derive(PartialEq, Clone, Copy, Debug)]
enum Phase {
    Waiting,
    RunIn,
    Jump,
    RunOut,
}

pub struct MarioClock {
    phase: Phase,
    runner_x: f32,
    runner_y: f32,
    jump_t: f32,
    jump_target: usize,
    climb_stage: u8,
    block_bounce: [f32; 2],
    pending_digits: bool,
    shown: [String; 2],
    last_minute: i32,
    last_frame: Instant,
    /// `clock_speed` percentage (25-300): scales the run and the jump.
    speed_pct: i32,
    /// Set when the face comes back on screen: the next frame shows the
    /// current time straight away instead of keeping the digits from the
    /// last time it was visible (same as the ESP32 `onActivated()`).
    snap: bool,
    shell_active: bool,
    shell_x: f32,
    coin_pop: bool,
    dirty: u8,
}

/// Walk-cycle frame for Mario at `runner_x`: one frame per 6 px travelled.
/// The +1200 keeps the division positive while he enters from off screen
/// (truncation toward zero, as the ESP32 `(int)` cast does).
pub fn run_frame(runner_x: f32) -> usize {
    ((((runner_x as i32) + 1200) / 6) % 3) as usize
}

impl MarioClock {
    pub fn new() -> Self {
        Self {
            phase: Phase::Waiting,
            runner_x: -(MARIO_JUMP_W as f32),
            runner_y: 0.0,
            jump_t: 0.0,
            jump_target: 1,
            climb_stage: 0,
            block_bounce: [0.0, 0.0],
            pending_digits: false,
            shown: [String::from("--"), String::from("--")],
            last_minute: -1,
            last_frame: Instant::now(),
            speed_pct: 100,
            snap: false,
            shell_active: false,
            shell_x: 0.0,
            coin_pop: false,
            dirty: 3,
        }
    }

    /// Called when the clock becomes active again (rotation, end of a
    /// preemption). Mario only runs for minutes that change while the face is
    /// on screen; time that passed while it was hidden is shown at once.
    pub fn on_activated(&mut self) {
        self.snap = true;
    }

    /// The hour and minute digits currently drawn on the blocks.
    pub fn shown_digits(&self) -> (&str, &str) {
        (&self.shown[0], &self.shown[1])
    }

    /// Whether Mario is on his way to (or back from) a block.
    pub fn is_running(&self) -> bool {
        self.phase != Phase::Waiting
    }

    /// Applies the instance's `clock_speed` (percent, clamped to 25-300).
    pub fn configure(&mut self, speed_pct: i32) {
        self.speed_pct = speed_pct.clamp(25, 300);
    }

    fn blit(
        matrix: &mut dyn IDrawingSurface,
        data: &[u16],
        w: i32,
        h: i32,
        x: i32,
        y: i32,
        transparent: bool,
    ) {
        Self::blit_ex(matrix, data, w, h, x, y, transparent, false);
    }

    fn blit_ex(
        matrix: &mut dyn IDrawingSurface,
        data: &[u16],
        w: i32,
        h: i32,
        x: i32,
        y: i32,
        transparent: bool,
        flip_h: bool,
    ) {
        for row in 0..h {
            let py = y + row;
            if py < 0 || py >= matrix.height() as i32 {
                continue;
            }
            for col in 0..w {
                let px = x + if flip_h { w - 1 - col } else { col };
                if px < 0 || px >= matrix.width() as i32 {
                    continue;
                }
                let c = data[(row * w + col) as usize];
                if transparent && c == SKY_COLOR {
                    continue;
                }
                cwscene::put(matrix, px, py, c);
            }
        }
    }

    fn draw_scene(matrix: &mut dyn IDrawingSurface, w: i32, h: i32) {
        // Sky fill
        for y in 0..h {
            for x in 0..w {
                cwscene::put(matrix, x, y, SKY_COLOR);
            }
        }

        let is_portrait = h > w;
        let ground_top = h - GROUND_H;

        let mut x = 0;
        while x < w {
            Self::blit(matrix, &GROUND, GROUND_W, GROUND_H, x, ground_top, false);
            x += GROUND_W;
        }

        if is_portrait {
            if w >= 64 {
                // Stepped brick platforms in vertical mode (64x256 / 64x128)
                let mut px = 8;
                while px < 56 {
                    Self::blit(matrix, &GROUND, GROUND_W, GROUND_H, px, 70, false);
                    px += GROUND_W;
                }
                px = 16;
                while px < 48 {
                    Self::blit(matrix, &GROUND, GROUND_W, GROUND_H, px, 130, false);
                    px += GROUND_W;
                }
                px = 8;
                while px < 56 {
                    Self::blit(matrix, &GROUND, GROUND_W, GROUND_H, px, 190, false);
                    px += GROUND_W;
                }
                Self::blit(matrix, &CLOUD1, CLOUD_W, CLOUD_H, 4, 4, true);
                Self::blit(matrix, &CLOUD2, CLOUD_W, CLOUD_H, w - CLOUD_W - 4, 10, true);
            } else {
                // 32-wide Tate (128x32 rotated: w=32, h=128)
                let mut px = 0;
                while px < w {
                    Self::blit(matrix, &GROUND, GROUND_W, GROUND_H, px, 80, false);
                    px += GROUND_W;
                }
                Self::blit(
                    matrix,
                    &CLOUD1,
                    CLOUD_W,
                    CLOUD_H,
                    (w - CLOUD_W) / 2,
                    2,
                    true,
                );
            }
            return;
        }

        if h >= 48 {
            Self::blit(
                matrix,
                &HILL,
                HILL_W,
                HILL_H,
                0,
                ground_top - HILL_H + 2,
                true,
            );
            Self::blit(
                matrix,
                &BUSH,
                BUSH_W,
                BUSH_H,
                w - BUSH_W,
                ground_top - BUSH_H,
                true,
            );
        }

        Self::blit(
            matrix,
            &CLOUD1,
            CLOUD_W,
            CLOUD_H,
            0,
            if h >= 48 { 14 } else { 2 },
            true,
        );
        Self::blit(
            matrix,
            &CLOUD2,
            CLOUD_W,
            CLOUD_H,
            w - CLOUD_W,
            if h >= 48 { 6 } else { 2 },
            true,
        );

        if w >= 128 && h >= 48 {
            Self::blit(matrix, &CLOUD_FULL, CLOUD_FULL_W, CLOUD_FULL_H, 40, 6, true);
            Self::blit(
                matrix,
                &CLOUD_FULL,
                CLOUD_FULL_W,
                CLOUD_FULL_H,
                72,
                14,
                true,
            );
        }
        if w >= 256 && h >= 48 {
            Self::blit(
                matrix,
                &CLOUD_FULL,
                CLOUD_FULL_W,
                CLOUD_FULL_H,
                165,
                6,
                true,
            );
            Self::blit(
                matrix,
                &CLOUD_FULL,
                CLOUD_FULL_W,
                CLOUD_FULL_H,
                198,
                14,
                true,
            );
        }
    }

    fn draw_block_at(
        matrix: &mut dyn IDrawingSurface,
        x: i32,
        y: i32,
        text: &str,
        font: &ArcadeFont<'_>,
    ) {
        Self::blit(matrix, &BLOCK, BLOCK_W, BLOCK_H, x, y, false);
        let text_offset_x = if text.len() == 1 { 6 } else { 2 };
        BaseRenderer::draw_text_at(
            matrix,
            text,
            font,
            1.0,
            x + text_offset_x,
            y + 4,
            (0, 0, 0),
            (0, 0, 0),
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        matrix: &mut dyn IDrawingSurface,
        hours: u32,
        minutes: u32,
        font: &ArcadeFont<'_>,
        _scale: u32,
    ) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;

        let is_portrait = h > w;
        let is_wide_tall = h >= 48 && !is_portrait;
        let ground_top = if is_portrait { 70 } else { h - GROUND_H };

        let (hour_x, minute_x, block_y) = if is_portrait {
            if w >= 64 {
                (10, 35, 24)
            } else {
                ((w - BLOCK_W) / 2, (w - BLOCK_W) / 2, 16)
            }
        } else if is_wide_tall {
            let scene_left = (w - SCENE) / 2;
            (scene_left + 13, scene_left + 32, 8)
        } else {
            // 128x32 compact mode
            (w - 46, w - 24, 3)
        };

        let hh = format!("{}", hours);
        let mm = format!("{:02}", minutes);
        if self.shown[0] == "--" {
            self.shown = [hh.clone(), mm.clone()];
            self.dirty = 3;
        }
        if self.snap {
            // Back on screen: show the current time now, no run-in for it.
            self.snap = false;
            self.shown = [hh.clone(), mm.clone()];
            self.last_minute = minutes as i32;
            self.phase = Phase::Waiting;
            self.pending_digits = false;
            self.block_bounce = [0.0, 0.0];
            self.runner_x = -(MARIO_JUMP_W as f32);
            self.jump_t = 0.0;
            self.shell_active = false;
            self.coin_pop = false;
            self.dirty = 3;
        }

        let dt = self.last_frame.elapsed().as_secs_f32().min(0.2);
        self.last_frame = Instant::now();

        // Minute change trigger
        if minutes as i32 != self.last_minute {
            let new_hour = self.last_minute >= 0 && minutes == 0;
            self.last_minute = minutes as i32;
            self.jump_target = if new_hour { 0 } else { 1 };
            self.pending_digits = true;
            self.dirty = 3;

            if is_wide_tall || is_portrait {
                if self.phase == Phase::Waiting {
                    self.phase = Phase::RunIn;
                    self.runner_x = -(MARIO_JUMP_W as f32);
                    self.climb_stage = 0;
                    self.runner_y = if is_portrait {
                        (h - GROUND_H - MARIO_H) as f32
                    } else {
                        (ground_top - MARIO_H) as f32
                    };
                }
            } else {
                // 128x32 compact mode: trigger green Koopa shell kick
                self.shell_active = true;
                self.shell_x = 26.0;
                self.phase = Phase::RunIn;
            }
        }

        let speed = self.speed_pct as f32 / 100.0;
        let pace = 85.0 * speed;

        if is_portrait {
            if w >= 64 {
                let target_x = ((if self.jump_target == 0 {
                    hour_x
                } else {
                    minute_x
                }) + BLOCK_W / 2
                    - MARIO_W / 2) as f32;

                match self.phase {
                    Phase::Waiting => {}
                    Phase::RunIn => {
                        let bottom_ground_y = (h - GROUND_H - MARIO_H) as f32;
                        match self.climb_stage {
                            0 => {
                                self.runner_x += pace * dt;
                                self.runner_y = bottom_ground_y;
                                if self.runner_x >= 14.0 {
                                    self.climb_stage = 1;
                                    self.jump_t = 0.0;
                                }
                            }
                            1 => {
                                self.jump_t += dt * 2.6;
                                let t = self.jump_t.min(1.0);
                                self.runner_x = 14.0 + 22.0 * t;
                                let base_y = bottom_ground_y
                                    + (190.0 - MARIO_H as f32 - bottom_ground_y) * t;
                                let arc = 4.0 * 14.0 * t * (1.0 - t);
                                self.runner_y = base_y - arc;
                                if self.jump_t >= 1.0 {
                                    self.jump_t = 0.0;
                                    self.runner_y = (190 - MARIO_H) as f32;
                                    self.climb_stage = 2;
                                }
                            }
                            2 => {
                                self.runner_x -= pace * dt;
                                self.runner_y = (190 - MARIO_H) as f32;
                                if self.runner_x <= 30.0 {
                                    self.climb_stage = 3;
                                    self.jump_t = 0.0;
                                }
                            }
                            3 => {
                                self.jump_t += dt * 2.8;
                                let t = self.jump_t.min(1.0);
                                self.runner_x = 30.0 + (18.0 - 30.0) * t;
                                let base_y = (190.0 - MARIO_H as f32)
                                    + ((130.0 - MARIO_H as f32) - (190.0 - MARIO_H as f32)) * t;
                                let arc = 4.0 * 14.0 * t * (1.0 - t);
                                self.runner_y = base_y - arc;
                                if self.jump_t >= 1.0 {
                                    self.jump_t = 0.0;
                                    self.runner_y = (130 - MARIO_H) as f32;
                                    self.climb_stage = 4;
                                }
                            }
                            4 => {
                                self.runner_x += pace * dt;
                                self.runner_y = (130 - MARIO_H) as f32;
                                if self.runner_x >= 24.0 {
                                    self.climb_stage = 5;
                                    self.jump_t = 0.0;
                                }
                            }
                            5 => {
                                self.jump_t += dt * 2.8;
                                let t = self.jump_t.min(1.0);
                                self.runner_x = 24.0 + (32.0 - 24.0) * t;
                                let base_y = (130.0 - MARIO_H as f32)
                                    + ((70.0 - MARIO_H as f32) - (130.0 - MARIO_H as f32)) * t;
                                let arc = 4.0 * 14.0 * t * (1.0 - t);
                                self.runner_y = base_y - arc;
                                if self.jump_t >= 1.0 {
                                    self.jump_t = 0.0;
                                    self.runner_y = (70 - MARIO_H) as f32;
                                    self.climb_stage = 6;
                                }
                            }
                            6 => {
                                if self.runner_x < target_x {
                                    self.runner_x += pace * dt;
                                    if self.runner_x >= target_x {
                                        self.runner_x = target_x;
                                        self.phase = Phase::Jump;
                                        self.jump_t = 0.0;
                                    }
                                } else {
                                    self.runner_x -= pace * dt;
                                    if self.runner_x <= target_x {
                                        self.runner_x = target_x;
                                        self.phase = Phase::Jump;
                                        self.jump_t = 0.0;
                                    }
                                }
                                self.runner_y = (70 - MARIO_H) as f32;
                            }
                            _ => {}
                        }
                    }
                    Phase::Jump => {
                        self.jump_t += dt * 4.0 * speed;
                        if self.jump_t >= 0.5 && self.pending_digits {
                            self.pending_digits = false;
                            self.block_bounce[self.jump_target] = 0.001;
                            self.shown[0] = hh.clone();
                            self.shown[1] = mm.clone();
                        }
                        if self.jump_t >= 1.0 {
                            self.jump_t = 0.0;
                            self.phase = Phase::RunOut;
                        }
                    }
                    Phase::RunOut => {
                        self.runner_x += pace * dt;
                        self.runner_y = (70 - MARIO_H) as f32;
                        if self.runner_x > w as f32 + MARIO_JUMP_W as f32 + 16.0 {
                            self.phase = Phase::Waiting;
                            self.dirty = 3;
                        }
                    }
                }
            } else {
                // 32-wide Tate (128x32 rotated: w=32, h=128)
                let target_x = ((w - MARIO_W) / 2) as f32;
                let plat_ground_y = (80 - MARIO_H) as f32;

                match self.phase {
                    Phase::Waiting => {}
                    Phase::RunIn => {
                        self.runner_x += pace * dt;
                        self.runner_y = plat_ground_y;
                        if self.runner_x >= target_x {
                            self.runner_x = target_x;
                            self.phase = Phase::Jump;
                            self.jump_t = 0.0;
                        }
                    }
                    Phase::Jump => {
                        self.jump_t += dt * 3.5 * speed;
                        let t = self.jump_t.min(1.0);
                        let arc_height = if self.jump_target == 0 { 26.0 } else { 10.0 };
                        let arc = (t * std::f32::consts::PI).sin() * arc_height;
                        self.runner_y = plat_ground_y - arc;
                        if self.jump_t >= 0.5 && self.pending_digits {
                            self.pending_digits = false;
                            self.block_bounce[self.jump_target] = 0.001;
                            self.shown[0] = hh.clone();
                            self.shown[1] = mm.clone();
                            self.coin_pop = true;
                        }
                        if self.jump_t >= 1.0 {
                            self.jump_t = 0.0;
                            self.runner_y = plat_ground_y;
                            self.phase = Phase::RunOut;
                        }
                    }
                    Phase::RunOut => {
                        self.runner_x += pace * dt;
                        self.runner_y = plat_ground_y;
                        if self.runner_x > w as f32 + 16.0 {
                            self.phase = Phase::Waiting;
                            self.dirty = 3;
                        }
                    }
                }
            }
        } else if is_wide_tall {
            let target_x = ((if self.jump_target == 0 {
                hour_x
            } else {
                minute_x
            }) + BLOCK_W / 2
                - MARIO_W / 2) as f32;

            match self.phase {
                Phase::Waiting => {}
                Phase::RunIn => {
                    self.runner_x += pace * dt;
                    if self.runner_x >= target_x {
                        self.runner_x = target_x;
                        self.phase = Phase::Jump;
                        self.jump_t = 0.0;
                    }
                }
                Phase::Jump => {
                    self.jump_t += dt * 4.0 * speed;
                    if self.jump_t >= 0.5 && self.pending_digits {
                        self.pending_digits = false;
                        self.block_bounce[self.jump_target] = 0.001;
                        self.shown[0] = hh.clone();
                        self.shown[1] = mm.clone();
                    }
                    if self.jump_t >= 1.0 {
                        self.jump_t = 0.0;
                        self.phase = Phase::RunOut;
                    }
                }
                Phase::RunOut => {
                    self.runner_x += pace * dt;
                    if self.runner_x > w as f32 + MARIO_JUMP_W as f32 + 16.0 {
                        self.phase = Phase::Waiting;
                        self.dirty = 3;
                    }
                }
            }
        } else {
            // 128x32 compact mode: shell physics
            if self.shell_active {
                let speed = self.speed_pct as f32 / 100.0;
                let shell_speed = 280.0f32 * speed;
                self.shell_x += shell_speed * dt;
                let target_x = (if self.jump_target == 0 {
                    hour_x
                } else {
                    minute_x
                }) as f32;
                if self.shell_x >= target_x - 6.0 {
                    self.shell_active = false;
                    self.block_bounce[self.jump_target] = 0.001;
                    self.shown[0] = hh.clone();
                    self.shown[1] = mm.clone();
                    self.pending_digits = false;
                    self.coin_pop = true;
                    self.phase = Phase::Waiting;
                    self.dirty = 3;
                }
            }
        }

        // Bouncing blocks animation
        for i in 0..2 {
            if self.block_bounce[i] > 0.0 {
                self.block_bounce[i] += dt * 3.2;
                if self.block_bounce[i] >= 1.0 {
                    self.block_bounce[i] = 0.0;
                    if !is_wide_tall && !is_portrait {
                        self.coin_pop = false;
                    }
                }
            }
        }

        let animating = self.phase != Phase::Waiting
            || self.block_bounce[0] > 0.0
            || self.block_bounce[1] > 0.0
            || self.shell_active;

        if !animating && self.dirty == 0 {
            return;
        }
        if self.dirty > 0 {
            self.dirty -= 1;
        }

        Self::draw_scene(matrix, w, h);

        // Draw blocks with bouncy lift
        for i in 0..2 {
            let b = self.block_bounce[i];
            let lift = if b > 0.0 {
                ((b * std::f32::consts::PI).sin() * 4.0) as i32
            } else {
                0
            };
            let by = if is_portrait && w < 64 {
                if i == 0 {
                    16
                } else {
                    48
                }
            } else {
                block_y
            };
            Self::draw_block_at(
                matrix,
                if i == 0 { hour_x } else { minute_x },
                by - lift,
                &self.shown[i],
                font,
            );
        }

        if is_portrait && w < 64 && self.coin_pop && self.block_bounce[self.jump_target] > 0.0 {
            let b = self.block_bounce[self.jump_target];
            let coin_lift = ((b * std::f32::consts::PI).sin() * 6.0) as i32;
            let target_by = if self.jump_target == 0 { 16 } else { 48 };
            Self::blit(
                matrix,
                &GOLD_COIN,
                8,
                8,
                (w - 8) / 2,
                target_by - 5 - coin_lift,
                true,
            );
        }

        if is_wide_tall || is_portrait {
            if self.phase != Phase::Waiting {
                let (airborne, draw_y) = if is_portrait {
                    if w >= 64 {
                        if self.phase == Phase::Jump {
                            let lift = ((self.jump_t * std::f32::consts::PI).sin()
                                * (70 - block_y - BLOCK_H - 2) as f32)
                                as i32;
                            (true, (70 - MARIO_H) - lift)
                        } else if self.phase == Phase::RunIn
                            && (self.climb_stage == 1
                                || self.climb_stage == 3
                                || self.climb_stage == 5)
                        {
                            (true, self.runner_y as i32)
                        } else {
                            (false, self.runner_y as i32)
                        }
                    } else {
                        (self.phase == Phase::Jump, self.runner_y as i32)
                    }
                } else {
                    let airborne = self.phase == Phase::Jump;
                    let lift = if airborne {
                        ((self.jump_t * std::f32::consts::PI).sin()
                            * (ground_top - block_y - BLOCK_H - 2) as f32)
                            as i32
                    } else {
                        0
                    };
                    (airborne, ground_top - MARIO_H - lift)
                };

                if airborne {
                    Self::blit(
                        matrix,
                        &MARIO_JUMP,
                        MARIO_JUMP_W,
                        MARIO_H,
                        self.runner_x as i32,
                        draw_y,
                        true,
                    );
                } else {
                    let f = run_frame(self.runner_x);
                    let frame: &[u16] = match f {
                        0 => &MARIO_RUN1,
                        1 => &MARIO_RUN2,
                        _ => &MARIO_RUN3,
                    };
                    Self::blit(
                        matrix,
                        frame,
                        MARIO_RUN_W[f],
                        MARIO_H,
                        self.runner_x as i32 - 2 + MARIO_RUN_LEFT[f],
                        draw_y,
                        true,
                    );
                }
            }
        } else {
            // 128x32 compact mode: Mario stands at x=14
            Self::blit(
                matrix,
                &MARIO_IDLE,
                MARIO_W,
                MARIO_H,
                14,
                ground_top - MARIO_H,
                true,
            );
            if self.shell_active {
                Self::blit(
                    matrix,
                    &KOOPA_SHELL,
                    KOOPA_SHELL_W,
                    KOOPA_SHELL_H,
                    self.shell_x as i32,
                    ground_top - KOOPA_SHELL_H,
                    true,
                );
            } else {
                let kf = (((self.shell_x as i32 + 500) / 4) % 2) as usize;
                let walk_data = if kf == 0 { &KOOPA_WALK1 } else { &KOOPA_WALK2 };
                Self::blit_ex(
                    matrix,
                    walk_data,
                    KOOPA_W,
                    KOOPA_H,
                    self.shell_x as i32,
                    ground_top - KOOPA_H,
                    true,
                    false,
                );
            }
            if self.coin_pop && self.block_bounce[self.jump_target] > 0.0 {
                let target_x = if self.jump_target == 0 {
                    hour_x
                } else {
                    minute_x
                };
                let b = self.block_bounce[self.jump_target];
                let coin_lift = ((b * std::f32::consts::PI).sin() * 6.0) as i32;
                Self::blit(
                    matrix,
                    &GOLD_COIN,
                    8,
                    8,
                    target_x + 5,
                    block_y - 4 - coin_lift,
                    true,
                );
            }
        }
    }
}

impl Default for MarioClock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::matrix::MockSurface;
    use crate::engines::clocks::cwassets::mario::{MARIO_IDLE, M_RED, M_SHIRT, M_SKIN};

    #[test]
    fn run_frame_follows_distance() {
        // ((x + 1200) / 6) % 3, x truncated toward zero.
        assert_eq!(run_frame(0.0), 2); // 1200 / 6 = 200
        assert_eq!(run_frame(6.0), 0);
        assert_eq!(run_frame(12.0), 1);
        assert_eq!(run_frame(17.9), 1);
        assert_eq!(run_frame(18.0), 2);
        assert_eq!(run_frame(-17.5), 2); // -17 -> 1183 / 6 = 197
    }

    /// RGB565 to RGB888, as `cwscene::put` converts it.
    fn rgb(c: u16) -> (u8, u8, u8) {
        let (r, g, b) = ((c >> 11) & 0x1F, (c >> 5) & 0x3F, c & 0x1F);
        (
            ((r << 3) | (r >> 2)) as u8,
            ((g << 2) | (g >> 4)) as u8,
            ((b << 3) | (b >> 2)) as u8,
        )
    }

    /// Bounding box (x0, y0, x1, y1) of Mario's body colours in rows y0..y1.
    fn mario_box(m: &MockSurface, y0: u32, y1: u32) -> Option<(u32, u32, u32, u32)> {
        let want: Vec<(u8, u8, u8)> = [M_RED, M_SKIN, M_SHIRT].iter().map(|&c| rgb(c)).collect();
        let mut b: Option<(u32, u32, u32, u32)> = None;
        for y in y0..y1 {
            for x in 0..80 {
                let p = m.canvas.get_pixel(x, y);
                if want.contains(&(p[0], p[1], p[2])) {
                    b = Some(match b {
                        None => (x, y, x, y),
                        Some((a, bb, c, d)) => (a.min(x), bb.min(y), c.max(x), d.max(y)),
                    });
                }
            }
        }
        b
    }

    #[test]
    fn running_mario_draws_the_walk_cycle_frame() {
        let base = BaseRenderer::new();
        let font = base.font();
        // 256x64: ground top = 56, Mario rows 40..55. The 64 px scene starts at
        // x = 96, so with Mario at x < 60 only his pixels are in columns 0..80.
        for (x, frame) in [(18.0f32, 2usize), (24.0, 0), (30.0, 1), (36.0, 2)] {
            assert_eq!(run_frame(x), frame);
            for phase in [Phase::RunIn, Phase::RunOut] {
                let mut mario = MarioClock::new();
                let mut m = MockSurface::new(256, 64);
                mario.render(&mut m, 7, 46, &font, 1); // settle on 7:46
                mario.phase = phase;
                mario.runner_x = x;
                mario.jump_target = 1;
                mario.last_frame = Instant::now();
                mario.render(&mut m, 7, 46, &font, 1);
                let x_now = mario.runner_x; // moved by at most a few px*dt
                assert_eq!(run_frame(x_now), frame, "moved across a frame boundary");
                // Expected: that frame alone at x - 2 + left, rows 40..55.
                let data: &[u16] = match frame {
                    0 => &MARIO_RUN1,
                    1 => &MARIO_RUN2,
                    _ => &MARIO_RUN3,
                };
                let mut solo = MockSurface::new(256, 64);
                let left = x_now as i32 - 2 + MARIO_RUN_LEFT[frame];
                MarioClock::blit(&mut solo, data, MARIO_RUN_W[frame], MARIO_H, left, 40, true);
                let want = mario_box(&solo, 40, 56).unwrap();
                assert_eq!(mario_box(&m, 40, 56), Some(want), "x={} frame={}", x, frame);
                // ... which is not the idle pose he used to glide in.
                let mut idle = MockSurface::new(256, 64);
                MarioClock::blit(
                    &mut idle,
                    &MARIO_IDLE,
                    MARIO_W,
                    MARIO_H,
                    x_now as i32,
                    40,
                    true,
                );
                assert_ne!(mario_box(&idle, 40, 56), Some(want));
            }
        }
    }

    /// How far Mario runs in one 100 ms frame at `speed_pct`.
    fn run_distance(speed_pct: i32) -> f32 {
        let base = BaseRenderer::new();
        let font = base.font();
        let mut m = MockSurface::new(256, 64);
        let mut mario = MarioClock::new();
        mario.configure(speed_pct);
        mario.render(&mut m, 7, 46, &font, 1);
        mario.phase = Phase::RunIn;
        mario.runner_x = 0.0;
        mario.jump_target = 1;
        mario.last_frame = Instant::now() - std::time::Duration::from_millis(100);
        mario.render(&mut m, 7, 46, &font, 1);
        mario.runner_x
    }

    #[test]
    fn pace_is_smb_walking_speed_scaled_by_clock_speed() {
        // 100 % = 85 px/s (the old face ran 34 px/s, Erik's 250 %).
        let d100 = run_distance(100);
        assert!((d100 - 8.5).abs() < 0.3, "100 %: {} px in 100 ms", d100);
        let d50 = run_distance(50);
        assert!((d50 - 4.25).abs() < 0.2, "50 %: {}", d50);
        let d300 = run_distance(300);
        assert!((d300 - 25.5).abs() < 0.8, "300 %: {}", d300);
        // Out-of-range settings are clamped like the other faces (25-300).
        assert!((run_distance(1000) - d300).abs() < 0.8);
        assert!((run_distance(1) - run_distance(25)).abs() < 0.2);
    }

    #[test]
    fn jump_rate_follows_clock_speed() {
        let base = BaseRenderer::new();
        let font = base.font();
        for (pct, want) in [(100, 0.4f32), (200, 0.8)] {
            let mut m = MockSurface::new(256, 64);
            let mut mario = MarioClock::new();
            mario.configure(pct);
            mario.render(&mut m, 7, 46, &font, 1);
            mario.phase = Phase::Jump;
            mario.jump_t = 0.0;
            mario.pending_digits = false;
            mario.last_frame = Instant::now() - std::time::Duration::from_millis(100);
            mario.render(&mut m, 7, 46, &font, 1);
            assert!(
                (mario.jump_t - want).abs() < 0.03,
                "{} %: {}",
                pct,
                mario.jump_t
            );
        }
    }

    #[test]
    fn jump_keeps_the_jump_sprite() {
        let base = BaseRenderer::new();
        let font = base.font();
        let mut mario = MarioClock::new();
        let mut m = MockSurface::new(256, 64);
        mario.render(&mut m, 7, 46, &font, 1);
        mario.phase = Phase::Jump;
        mario.runner_x = 30.0;
        mario.jump_t = 0.0;
        mario.last_frame = Instant::now();
        mario.render(&mut m, 7, 46, &font, 1);
        let mut solo = MockSurface::new(256, 64);
        let y = 56 - MARIO_H; // jump_t ~ 0: lift ~ 0
        MarioClock::blit(&mut solo, &MARIO_JUMP, MARIO_JUMP_W, MARIO_H, 30, y, true);
        assert_eq!(mario_box(&m, 0, 56), mario_box(&solo, 0, 56));
    }

    #[test]
    fn returning_face_shows_current_time_without_run_in() {
        let base = BaseRenderer::new();
        let font = base.font();
        let mut m = MockSurface::new(256, 64);
        let mut mario = MarioClock::new();

        // Shown at 7:46 (the engine always activates before the first frame).
        mario.on_activated();
        mario.render(&mut m, 7, 46, &font, 1);
        assert_eq!(mario.shown_digits(), ("7", "46"));
        assert!(!mario.is_running());

        // Off screen for ten minutes, then back: the first frame shows 7:56.
        mario.on_activated();
        mario.render(&mut m, 7, 56, &font, 1);
        assert_eq!(mario.shown_digits(), ("7", "56"));
        assert!(
            !mario.is_running(),
            "no run-in for time that passed off screen"
        );

        // A minute that changes while on screen still gets Mario's run.
        mario.render(&mut m, 7, 57, &font, 1);
        assert!(mario.is_running());
        assert_eq!(
            mario.shown_digits(),
            ("7", "56"),
            "until he strikes the block"
        );
    }

    #[test]
    fn snap_also_cancels_a_run_in_progress() {
        let base = BaseRenderer::new();
        let font = base.font();
        let mut m = MockSurface::new(256, 64);
        let mut mario = MarioClock::new();
        mario.on_activated();
        mario.render(&mut m, 7, 46, &font, 1);
        mario.render(&mut m, 7, 47, &font, 1); // run starts
        assert!(mario.is_running());
        mario.on_activated(); // hidden mid-run, back at 8:00
        mario.render(&mut m, 8, 0, &font, 1);
        assert_eq!(mario.shown_digits(), ("8", "00"));
        assert!(!mario.is_running());
    }

    #[test]
    fn snap_waits_for_a_wide_panel() {
        // Multi-resolution support: 128x32 renders Mario clock with compact shell kick
        let base = BaseRenderer::new();
        let font = base.font();
        let mut mario = MarioClock::new();
        mario.render(&mut MockSurface::new(256, 64), 7, 46, &font, 1);
        mario.on_activated();
        mario.render(&mut MockSurface::new(128, 32), 7, 56, &font, 1);
        assert_eq!(mario.shown_digits(), ("7", "56"));
        assert!(!mario.is_running());
    }
}

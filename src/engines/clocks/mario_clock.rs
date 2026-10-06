//! Mario clock, adapted from the 64x64 "Clockwise" clockface cw-cf-0x01.
//!
//! The square scene keeps its place in the middle of the panel and the ground and clouds carry on
//! to both edges, so a wide sign reads as more of the same level. The hill and the bush are each cut
//! down one side to sit against a frame edge, so there is one of each, at the edge it was drawn for.
//! Once a minute Mario runs in from the left, jumps to bump a block, which flips its digits, and
//! runs off right.
//! Laid out for 256x64; smaller panels get the notice instead. Matches the ESP32 face.

use crate::core::matrix::MatrixBackend;
use crate::engines::clocks::cwassets::mario::{
    BLOCK, BUSH, CLOUD1, CLOUD2, GROUND, HILL, MARIO_JUMP, MARIO_RUN1, MARIO_RUN2, MARIO_RUN3,
    MARIO_RUN_LEFT, MARIO_RUN_W, SKY_COLOR,
};
use crate::engines::clocks::cwscene;
use crate::engines::renderers::base_renderer::ArcadeFont;
use crate::engines::renderers::BaseRenderer;
use std::time::Instant;

const BLOCK_W: i32 = 19;
const GROUND_W: i32 = 8;
const GROUND_H: i32 = 8;
const HILL_W: i32 = 20;
const HILL_H: i32 = 22;
const BUSH_W: i32 = 21;
const BUSH_H: i32 = 9;
const CLOUD_W: i32 = 13;
const CLOUD_H: i32 = 12;
const MARIO_W: i32 = 13;
const MARIO_H: i32 = 16;
const MARIO_JUMP_W: i32 = 17;
const SCENE: i32 = 64;

#[derive(PartialEq, Clone, Copy)]
enum Phase {
    Waiting,
    RunIn,
    Jump,
    RunOut,
}

pub struct MarioClock {
    phase: Phase,
    runner_x: f32,
    jump_t: f32,
    jump_target: usize,
    block_bounce: [f32; 2],
    pending_digits: bool,
    shown: [String; 2],
    last_minute: u32,
    last_frame: Instant,
    /// `clock_speed` percentage (25-300): scales the run and the jump.
    speed_pct: i32,
    /// Set when the face comes back on screen: the next frame shows the
    /// current time straight away instead of keeping the digits from the
    /// last time it was visible (same as the ESP32 `onActivated()`).
    snap: bool,
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
            jump_t: 0.0,
            jump_target: 1,
            block_bounce: [0.0, 0.0],
            pending_digits: false,
            shown: [String::from("--"), String::from("--")],
            last_minute: 99,
            last_frame: Instant::now(),
            speed_pct: 100,
            snap: false,
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
        matrix: &mut dyn MatrixBackend,
        data: &[u16],
        w: i32,
        h: i32,
        x: i32,
        y: i32,
        transparent: bool,
    ) {
        for row in 0..h {
            for col in 0..w {
                let c = data[(row * w + col) as usize];
                if transparent && c == SKY_COLOR {
                    continue;
                }
                cwscene::put(matrix, x + col, y + row, c);
            }
        }
    }

    fn draw_scene(matrix: &mut dyn MatrixBackend, w: i32, h: i32) {
        for y in 0..h {
            for x in 0..w {
                cwscene::put(matrix, x, y, SKY_COLOR);
            }
        }
        let scene_left = (w - SCENE) / 2;
        let ground_top = h - GROUND_H;
        let mut x = 0;
        while x < w {
            Self::blit(matrix, &GROUND, GROUND_W, GROUND_H, x, ground_top, false);
            x += GROUND_W;
        }
        // The hill is half a hill: its left side is a sheer vertical cut, drawn to sit flush against
        // the frame edge so it reads as a slope running on past it. Tiled across a wide panel that
        // cut lands in open sky and looks like a hill sliced off, so there is one, against the left
        // edge, as in the original.
        Self::blit(
            matrix,
            &HILL,
            HILL_W,
            HILL_H,
            0,
            ground_top - HILL_H + 2,
            true,
        );

        // The bush is the hill's mirror: cut down its right side, drawn to sit flush against the
        // other frame edge (x 43 of 64, so its right edge lands exactly on it). One, on the right.
        Self::blit(
            matrix,
            &BUSH,
            BUSH_W,
            BUSH_H,
            w - BUSH_W,
            ground_top - BUSH_H,
            true,
        );
        let mut cx = scene_left % 64 - 64;
        while cx < w {
            Self::blit(matrix, &CLOUD1, CLOUD_W, CLOUD_H, cx, 8, true);
            Self::blit(matrix, &CLOUD2, CLOUD_W, CLOUD_H, cx + 25, 2, true);
            cx += 51;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        matrix: &mut dyn MatrixBackend,
        hours: u32,
        minutes: u32,
        font: &ArcadeFont<'_>,
        scale: u32,
    ) {
        let w = matrix.width() as i32;
        let h = matrix.height() as i32;
        if w < 192 || h < 64 {
            super::wide_only_notice(matrix, font, scale);
            return;
        }

        let dt = self.last_frame.elapsed().as_secs_f32().min(0.2);
        self.last_frame = Instant::now();

        let scene_left = (w - SCENE) / 2;
        let ground_top = h - GROUND_H;
        let hour_x = scene_left + 13;
        let minute_x = scene_left + 32;
        let block_y = 8;

        let hh = format!("{}", hours);
        let mm = format!("{:02}", minutes);
        if self.shown[0] == "--" {
            self.shown = [hh.clone(), mm.clone()];
        }
        if self.snap {
            // Back on screen: show the current time now, no run-in for it.
            self.snap = false;
            self.shown = [hh.clone(), mm.clone()];
            self.last_minute = minutes;
            self.phase = Phase::Waiting;
            self.pending_digits = false;
            self.block_bounce = [0.0, 0.0];
            self.runner_x = -(MARIO_JUMP_W as f32);
            self.jump_t = 0.0;
        }

        if minutes != self.last_minute {
            let new_hour = self.last_minute != 99 && minutes == 0;
            self.last_minute = minutes;
            self.jump_target = if new_hour { 0 } else { 1 };
            self.pending_digits = true;
            if self.phase == Phase::Waiting {
                self.phase = Phase::RunIn;
                self.runner_x = -(MARIO_JUMP_W as f32);
            }
        }

        let target_x = (if self.jump_target == 0 {
            hour_x
        } else {
            minute_x
        }) + BLOCK_W / 2
            - MARIO_W / 2;
        // 100 % = Super Mario Bros walking pace (about 85 px/s; the NES screen
        // is 256 px wide, like the 256x64 panel). Same as the ESP32 face.
        let speed = self.speed_pct as f32 / 100.0;
        let pace = 85.0 * speed;
        match self.phase {
            Phase::Waiting => {}
            Phase::RunIn => {
                self.runner_x += pace * dt;
                if self.runner_x >= target_x as f32 {
                    self.runner_x = target_x as f32;
                    self.phase = Phase::Jump;
                    self.jump_t = 0.0;
                }
            }
            Phase::Jump => {
                self.jump_t += dt * 4.0 * speed; // jump timed to the walking pace
                if self.jump_t >= 0.5 && self.pending_digits {
                    self.pending_digits = false;
                    self.block_bounce[self.jump_target] = 0.001;
                    self.shown = [hh.clone(), mm.clone()];
                }
                if self.jump_t >= 1.0 {
                    self.jump_t = 0.0;
                    self.phase = Phase::RunOut;
                }
            }
            Phase::RunOut => {
                self.runner_x += pace * dt;
                if self.runner_x > w as f32 {
                    self.phase = Phase::Waiting;
                }
            }
        }
        for b in self.block_bounce.iter_mut() {
            if *b > 0.0 {
                *b += dt * 3.2;
                if *b >= 1.0 {
                    *b = 0.0;
                }
            }
        }

        Self::draw_scene(matrix, w, h);

        for i in 0..2 {
            let b = self.block_bounce[i];
            let lift = if b > 0.0 {
                (std::f32::consts::PI * b).sin() * 4.0
            } else {
                0.0
            } as i32;
            let bx = if i == 0 { hour_x } else { minute_x };
            Self::blit(matrix, &BLOCK, BLOCK_W, BLOCK_W, bx, block_y - lift, false);
            let text = &self.shown[i];
            let offset = if text.chars().count() == 1 { 6 } else { 2 };
            BaseRenderer::draw_text_at(
                matrix,
                text,
                font,
                scale.max(1) as f32,
                bx + offset,
                block_y - lift + 4,
                (0, 0, 0),
                (0, 0, 0),
            );
        }

        if self.phase != Phase::Waiting {
            let airborne = self.phase == Phase::Jump;
            let lift = if airborne {
                ((std::f32::consts::PI * self.jump_t).sin()
                    * (ground_top - block_y - BLOCK_W - 2) as f32) as i32
            } else {
                0
            };
            let y = ground_top - MARIO_H - lift;
            if airborne {
                Self::blit(
                    matrix,
                    &MARIO_JUMP,
                    MARIO_JUMP_W,
                    MARIO_H,
                    self.runner_x as i32,
                    y,
                    true,
                );
            } else {
                // Running in or out: the NES walk cycle, one frame per 6 px
                // travelled so the legs keep pace with the speed. The run
                // frames sit in a 16 px cell whose column 2 lines up with the
                // idle sprite (same as the ESP32 face).
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
                    y,
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
    use crate::core::matrix::MockMatrix;
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
    fn mario_box(m: &MockMatrix, y0: u32, y1: u32) -> Option<(u32, u32, u32, u32)> {
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
                let mut m = MockMatrix::new(256, 64);
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
                let mut solo = MockMatrix::new(256, 64);
                let left = x_now as i32 - 2 + MARIO_RUN_LEFT[frame];
                MarioClock::blit(&mut solo, data, MARIO_RUN_W[frame], MARIO_H, left, 40, true);
                let want = mario_box(&solo, 40, 56).unwrap();
                assert_eq!(mario_box(&m, 40, 56), Some(want), "x={} frame={}", x, frame);
                // ... which is not the idle pose he used to glide in.
                let mut idle = MockMatrix::new(256, 64);
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
        let mut m = MockMatrix::new(256, 64);
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
            let mut m = MockMatrix::new(256, 64);
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
        let mut m = MockMatrix::new(256, 64);
        mario.render(&mut m, 7, 46, &font, 1);
        mario.phase = Phase::Jump;
        mario.runner_x = 30.0;
        mario.jump_t = 0.0;
        mario.last_frame = Instant::now();
        mario.render(&mut m, 7, 46, &font, 1);
        let mut solo = MockMatrix::new(256, 64);
        let y = 56 - MARIO_H; // jump_t ~ 0: lift ~ 0
        MarioClock::blit(&mut solo, &MARIO_JUMP, MARIO_JUMP_W, MARIO_H, 30, y, true);
        assert_eq!(mario_box(&m, 0, 56), mario_box(&solo, 0, 56));
    }

    #[test]
    fn returning_face_shows_current_time_without_run_in() {
        let base = BaseRenderer::new();
        let font = base.font();
        let mut m = MockMatrix::new(256, 64);
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
        let mut m = MockMatrix::new(256, 64);
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
        // On 128x32 the face only draws a notice; the snap is kept until it
        // is drawn on a panel wide enough.
        let base = BaseRenderer::new();
        let font = base.font();
        let mut mario = MarioClock::new();
        mario.render(&mut MockMatrix::new(256, 64), 7, 46, &font, 1);
        mario.on_activated();
        mario.render(&mut MockMatrix::new(128, 32), 7, 56, &font, 1);
        assert_eq!(mario.shown_digits(), ("7", "46"));
        mario.render(&mut MockMatrix::new(256, 64), 7, 56, &font, 1);
        assert_eq!(mario.shown_digits(), ("7", "56"));
        assert!(!mario.is_running());
    }
}

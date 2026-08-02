use crate::core::matrix::MatrixBackend;
use rand::Rng;

pub struct PongClock {
    ball_x: f32,
    ball_y: f32,
    dx: f32,
    dy: f32,
    p1_y: f32,
    p2_y: f32,
    pad_h: f32,
    pad_w: i32,
    ball_size: i32,
    score_left: u32,  // Displays hours
    score_right: u32, // Displays minutes
    last_hour: i32,
    last_minute: i32,
    force_miss_left: bool,
    force_miss_right: bool,
}

impl PongClock {
    pub fn new(w: u32, h: u32) -> Self {
        let pad_h = (h as f32 / 3.0).max(8.0);
        let pad_w = ((w as f32 / 32.0) as i32).max(2);
        let ball_size = ((h as f32 / 16.0) as i32).max(2);
        let dx = (w as f32 / 40.0).max(1.5);
        let dy = (h as f32 / 30.0).max(1.0);
        Self {
            ball_x: w as f32 / 2.0,
            ball_y: h as f32 / 2.0,
            dx,
            dy,
            p1_y: h as f32 / 2.0,
            p2_y: h as f32 / 2.0,
            pad_h,
            pad_w,
            ball_size,
            score_left: 0,
            score_right: 0,
            last_hour: -1,
            last_minute: -1,
            force_miss_left: false,
            force_miss_right: false,
        }
    }

    fn reset_ball(&mut self, w: f32, h: f32, left_served: bool) {
        let mut rng = rand::thread_rng();
        self.ball_y = h / 2.0;
        self.dy = rng.gen_range(-1.5f32..1.5f32);
        if left_served {
            self.ball_x = self.pad_w as f32 + 2.0;
            self.dx = (w / 40.0).max(1.5);
        } else {
            self.ball_x = w - self.pad_w as f32 - 3.0;
            self.dx = -(w / 40.0).max(1.5);
        }
    }

    pub fn update_and_render(&mut self, matrix: &mut dyn MatrixBackend, hours: u32, minutes: u32) {
        let w = matrix.width() as f32;
        let h = matrix.height() as f32;

        // Sync score with actual time
        if self.last_hour == -1 {
            self.last_hour = hours as i32;
            self.last_minute = minutes as i32;
            self.score_left = hours;
            self.score_right = minutes;
        }

        // Detect minute change → force a miss on the left paddle
        if minutes as i32 != self.last_minute {
            self.force_miss_left = true;
            self.last_minute = minutes as i32;
        }
        if hours as i32 != self.last_hour {
            self.last_hour = hours as i32;
        }

        // Physics update
        self.ball_x += self.dx;
        self.ball_y += self.dy;

        // Vertical wall bounce
        if self.ball_y <= 0.0 || self.ball_y >= h - 1.0 {
            self.dy = -self.dy;
            self.ball_y = self.ball_y.clamp(0.0, h - 1.0);
        }

        // AI: move paddles toward ball (with optional deliberate miss)
        let tracking_speed = h / 20.0;
        if self.force_miss_left {
            // Deliberate miss: move away from ball
            if self.ball_y < h / 2.0 {
                self.p1_y = (self.p1_y + tracking_speed).min(h - self.pad_h / 2.0);
            } else {
                self.p1_y = (self.p1_y - tracking_speed).max(self.pad_h / 2.0);
            }
        } else {
            let diff = self.ball_y - self.p1_y;
            self.p1_y += diff.signum() * tracking_speed.min(diff.abs());
        }

        let diff2 = self.ball_y - self.p2_y;
        self.p2_y += diff2.signum() * tracking_speed.min(diff2.abs());

        // Left paddle collision
        if self.ball_x <= self.pad_w as f32 + self.ball_size as f32 {
            let top = self.p1_y - self.pad_h / 2.0;
            let bot = self.p1_y + self.pad_h / 2.0;
            if self.force_miss_left || self.ball_y < top || self.ball_y > bot {
                // Miss! Point to right
                self.score_right = minutes;
                self.force_miss_left = false;
                self.reset_ball(w, h, false);
            } else {
                self.dx = self.dx.abs();
                self.ball_x = self.pad_w as f32 + self.ball_size as f32 + 1.0;
            }
        }

        // Right paddle collision
        if self.ball_x >= w - self.pad_w as f32 - self.ball_size as f32 {
            let top = self.p2_y - self.pad_h / 2.0;
            let bot = self.p2_y + self.pad_h / 2.0;
            if self.ball_y < top || self.ball_y > bot {
                // Miss! Point to left
                self.score_left = hours;
                self.reset_ball(w, h, true);
            } else {
                self.dx = -self.dx.abs();
                self.ball_x = w - self.pad_w as f32 - self.ball_size as f32 - 1.0;
            }
        }

        // ── Draw ──────────────────────────────────────────────────────────────

        // Dotted center line
        for y in (0..h as i32).step_by(2) {
            matrix.set_pixel((w / 2.0) as i32, y, 80, 80, 80);
        }

        // Left paddle
        let p1_top = (self.p1_y - self.pad_h / 2.0) as i32;
        for dy in 0..self.pad_h as i32 {
            matrix.set_pixel(
                self.pad_w,
                (p1_top + dy).clamp(0, h as i32 - 1),
                255,
                255,
                255,
            );
        }

        // Right paddle
        let p2_top = (self.p2_y - self.pad_h / 2.0) as i32;
        for dy in 0..self.pad_h as i32 {
            matrix.set_pixel(
                (w as i32) - self.pad_w - 1,
                (p2_top + dy).clamp(0, h as i32 - 1),
                255,
                255,
                255,
            );
        }

        // Ball (square, ball_size × ball_size)
        for dy in 0..self.ball_size {
            for dx in 0..self.ball_size {
                matrix.set_pixel(
                    self.ball_x as i32 + dx,
                    self.ball_y as i32 + dy,
                    255,
                    255,
                    0,
                );
            }
        }

        // Scores drawn as digit dots at top center
        // Left score (hours) — left of center
        self.draw_score_digit(matrix, self.score_left, (w / 2.0 - 12.0) as i32, 1);
        // Separator colon
        matrix.set_pixel((w / 2.0) as i32, 2, 200, 200, 200);
        matrix.set_pixel((w / 2.0) as i32, 4, 200, 200, 200);
        // Right score (minutes) — right of center
        self.draw_score_digit(matrix, self.score_right, (w / 2.0 + 4.0) as i32, 1);
    }

    /// Draws a small 2-digit number at (x, y) using 3×5 pixel font segments.
    fn draw_score_digit(&self, matrix: &mut dyn MatrixBackend, value: u32, x: i32, y: i32) {
        let tens = value / 10;
        let units = value % 10;
        self.draw_tiny_digit(matrix, tens, x, y);
        self.draw_tiny_digit(matrix, units, x + 4, y);
    }

    fn draw_tiny_digit(&self, matrix: &mut dyn MatrixBackend, d: u32, x: i32, y: i32) {
        // 3×5 bitmaps for 0–9 (rows top to bottom, cols left to right)
        let segments: [[u8; 15]; 10] = [
            [1, 1, 1, 1, 0, 1, 1, 0, 1, 1, 0, 1, 1, 1, 1], // 0
            [0, 1, 0, 1, 1, 0, 0, 1, 0, 0, 1, 0, 1, 1, 1], // 1
            [1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1], // 2
            [1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1], // 3
            [1, 0, 1, 1, 0, 1, 1, 1, 1, 0, 0, 1, 0, 0, 1], // 4
            [1, 1, 1, 1, 0, 0, 1, 1, 1, 0, 0, 1, 1, 1, 1], // 5
            [1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 1, 1, 1, 1], // 6
            [1, 1, 1, 0, 0, 1, 0, 0, 1, 0, 1, 0, 0, 1, 0], // 7
            [1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1], // 8
            [1, 1, 1, 1, 0, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1], // 9
        ];
        let d = (d % 10) as usize;
        for row in 0..5i32 {
            for col in 0..3i32 {
                if segments[d][(row * 3 + col) as usize] == 1 {
                    matrix.set_pixel(x + col, y + row, 200, 200, 200);
                }
            }
        }
    }
}

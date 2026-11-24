//! Pure zero-allocation renderers for FrontendSync companion cards and Priority 55 celebrations (ADR-0004).
//!
//! Renders [`HallOfFameCard`], [`RetroAchievementsCard`], and [`SyncCelebrationRenderer`]
//! across all Yoko, Tate, and Square matrix geometries without heap allocations or I/O.

use crate::core::i18n::{self, Lang};
use crate::core::matrix::IDrawingSurface;
use crate::core::types::{SyncCelebrationPayload, SyncHiscoreEntryPod, SyncMetaPayload};
use crate::engines::clocks::sprite_bank::{self, CelebrationHeroFamily, SpriteId};
use crate::engines::dashboard::font::{draw_text_clipped, measure_text as measure_small_text};
use crate::engines::renderers::fireworks_lut::{FireworksLutRenderer, BOUNCE_LUT};
use crate::engines::renderers::primitives::{draw_fast_hline, draw_round_rect, fill_round_rect};
use crate::engines::renderers::tiny_font::{draw_tiny_text, tiny_text_width as measure_tiny_text};

struct StackStrBuf<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> StackStrBuf<N> {
    #[inline]
    const fn new() -> Self {
        Self {
            buf: [0u8; N],
            len: 0,
        }
    }

    #[inline]
    fn push_str(&mut self, s: &str) {
        let bytes = s.as_bytes();
        let avail = N.saturating_sub(self.len);
        let n = bytes.len().min(avail);
        if n > 0 {
            self.buf[self.len..self.len + n].copy_from_slice(&bytes[..n]);
            self.len += n;
        }
    }

    #[inline]
    fn push_u32(&mut self, mut val: u32) {
        if val == 0 {
            self.push_str("0");
            return;
        }
        let mut tmp = [0u8; 10];
        let mut idx = 10usize;
        while val > 0 && idx > 0 {
            idx -= 1;
            tmp[idx] = b'0' + (val % 10) as u8;
            val /= 10;
        }
        if let Ok(s) = std::str::from_utf8(&tmp[idx..]) {
            self.push_str(s);
        }
    }

    #[inline]
    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

#[inline]
fn rank_color_rgb(rank: u8) -> (u8, u8, u8) {
    match rank {
        1 => (255, 215, 0),  // Gold
        2 => (0, 229, 255),  // Cyan / Silver
        3 => (255, 140, 0),  // Bronze / Orange
        4 => (255, 80, 220), // Neon Magenta
        _ => (80, 255, 120), // Neon Green
    }
}

#[inline]
fn rank_label_short(rank: u8) -> &'static str {
    match rank {
        1 => "1ST",
        2 => "2ND",
        3 => "3RD",
        4 => "4TH",
        5 => "5TH",
        _ => "TOP",
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_tiny_text_clipped_bounds(
    surface: &mut dyn IDrawingSurface,
    x: i32,
    y: i32,
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
    text: &str,
    color: (u8, u8, u8),
) {
    if y < min_y || y + 4 > max_y {
        return;
    }
    draw_tiny_text(surface, text, x, y, min_x, max_x + 1, color);
}

#[allow(clippy::too_many_arguments)]
fn draw_small_text_clipped_bounds(
    surface: &mut dyn IDrawingSurface,
    x: i32,
    y: i32,
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
    text: &str,
    color: (u8, u8, u8),
) {
    draw_text_clipped(
        surface,
        text,
        x,
        y,
        min_x,
        max_x + 1,
        min_y,
        max_y + 1,
        color,
    );
}

// ----------------------------------------------------------------------------
// HallOfFameCard — Responsive Yoko (128x32, 256x64) & Tate (32x64, 64x128, 64x256)
// ----------------------------------------------------------------------------

pub struct HallOfFameCard;

impl HallOfFameCard {
    /// Renders the arcade Hall of Fame leaderboard card with zero heap allocations.
    pub fn render(
        surface: &mut dyn IDrawingSurface,
        meta: &SyncMetaPayload,
        lang: Lang,
        elapsed_ms: u64,
    ) {
        let w = surface.width() as i32;
        let h = surface.height() as i32;
        if w <= 0 || h <= 0 {
            return;
        }

        // Deep arcade attract-mode navy background
        surface.fill_rect(0, 0, w, h, 4, 6, 16);
        let pulse = ((elapsed_ms / 250) & 1) == 0;
        let title_col = if pulse {
            (255, 215, 0)
        } else {
            (255, 245, 150)
        };
        let count = (meta.hiscore_count as usize).min(meta.hiscores.len());

        // 1. Tate / Portrait layouts (32x64, 64x128, 64x256)
        if h > w {
            surface.fill_rect(0, 0, w, 12, 16, 12, 32);
            draw_fast_hline(surface, 0, 12, w, (255, 180, 0));

            if w >= 64 {
                let crown = sprite_bank::get_sprite_frame(SpriteId::CrownRecord, 0);
                sprite_bank::blit_sprite(surface, &crown, 2, 0, false);
                let title = i18n::sync_high_scores_title(lang);
                draw_tiny_text_clipped_bounds(surface, 16, 4, 16, w - 2, 0, 11, title, title_col);
            } else {
                draw_tiny_text_clipped_bounds(
                    surface,
                    2,
                    4,
                    0,
                    w - 2,
                    0,
                    11,
                    "HIGH SCORES",
                    title_col,
                );
            }

            let mut y = 15;
            let game = meta.game_title_str();
            if !game.is_empty() && h >= 64 {
                draw_tiny_text_clipped_bounds(
                    surface,
                    2,
                    y,
                    2,
                    w - 2,
                    13,
                    h - 2,
                    game,
                    (0, 229, 255),
                );
                y += 8;
                draw_fast_hline(surface, 2, y - 2, w - 4, (24, 34, 60));
            }

            let max_rows = if h >= 128 { 5 } else { 3 };
            let row_h = if w >= 64 && h >= 128 { 18 } else { 12 };
            for idx in 0..count.min(max_rows) {
                if y + 6 > h {
                    break;
                }
                let entry = &meta.hiscores[idx];
                let r_col = rank_color_rgb(entry.rank);
                let r_lbl = rank_label_short(entry.rank);

                if w >= 64 {
                    draw_tiny_text_clipped_bounds(surface, 2, y, 2, w - 2, 13, h - 1, r_lbl, r_col);
                    draw_tiny_text_clipped_bounds(
                        surface,
                        18,
                        y,
                        18,
                        w - 2,
                        13,
                        h - 1,
                        entry.player_str(),
                        (255, 255, 255),
                    );
                    let score_s = entry.score_str();
                    let sw = measure_tiny_text(score_s);
                    let sx = (w - 2 - sw).max(2);
                    draw_tiny_text_clipped_bounds(
                        surface,
                        sx,
                        y + 7,
                        2,
                        w - 2,
                        13,
                        h - 1,
                        score_s,
                        r_col,
                    );
                } else {
                    let mut top_line = StackStrBuf::<16>::new();
                    top_line.push_str("#");
                    top_line.push_u32(entry.rank as u32);
                    top_line.push_str(" ");
                    top_line.push_str(entry.player_str());
                    draw_tiny_text_clipped_bounds(
                        surface,
                        1,
                        y,
                        1,
                        w - 1,
                        13,
                        h - 1,
                        top_line.as_str(),
                        r_col,
                    );
                    draw_tiny_text_clipped_bounds(
                        surface,
                        1,
                        y + 6,
                        1,
                        w - 1,
                        13,
                        h - 1,
                        entry.score_str(),
                        (240, 240, 255),
                    );
                }
                y += row_h;
            }
            return;
        }

        // 2. Compact 32px height (64x32, 128x32) -> CPS1 smooth vertical scroll
        if h <= 32 {
            surface.fill_rect(0, 0, w, 8, 18, 12, 34);
            draw_fast_hline(surface, 0, 8, w, (255, 180, 0));
            let title = i18n::sync_high_scores_title(lang);
            let tw = measure_tiny_text(title);
            let tx = ((w - tw) / 2).max(2);
            draw_tiny_text_clipped_bounds(surface, tx, 2, 1, w - 2, 0, 7, title, title_col);

            let view_top = 10;
            let view_bot = h - 1;
            let row_pitch = 8;

            if count <= 2 {
                for idx in 0..count {
                    let ry = view_top + (idx as i32) * 10;
                    Self::draw_yoko_row_compact(
                        surface,
                        &meta.hiscores[idx],
                        w,
                        ry,
                        view_top,
                        view_bot,
                    );
                }
            } else {
                let cycle_px = (count as i32) * row_pitch;
                let scroll_px = ((elapsed_ms / 70) as i32) % cycle_px.max(1);
                for pass in 0..2 {
                    for idx in 0..count {
                        let ry = view_top + (idx as i32) * row_pitch + pass * cycle_px - scroll_px;
                        if ry + 5 >= view_top && ry <= view_bot {
                            Self::draw_yoko_row_compact(
                                surface,
                                &meta.hiscores[idx],
                                w,
                                ry,
                                view_top,
                                view_bot,
                            );
                        }
                    }
                }
            }
            return;
        }

        // 3. HD Yoko / Square (64x64, 128x64, 256x64) -> Full Arcade Podium
        surface.fill_rect(0, 0, w, 12, 18, 12, 36);
        draw_fast_hline(surface, 0, 12, w, (255, 190, 0));
        let crown = sprite_bank::get_sprite_frame(SpriteId::CrownRecord, 0);
        sprite_bank::blit_sprite(surface, &crown, 3, 0, false);

        let title = i18n::sync_high_scores_title(lang);
        if w >= 192 {
            draw_small_text_clipped_bounds(surface, 18, 3, 18, w - 4, 0, 11, title, title_col);
            let game = meta.game_title_str();
            if !game.is_empty() {
                let gw = measure_tiny_text(game);
                let gx = (w - 4 - gw).max(130);
                draw_tiny_text_clipped_bounds(
                    surface,
                    gx,
                    4,
                    130,
                    w - 4,
                    0,
                    11,
                    game,
                    (0, 229, 255),
                );
            }
        } else {
            draw_tiny_text_clipped_bounds(surface, 17, 4, 17, w - 2, 0, 11, title, title_col);
        }

        let start_y = 15;
        let row_h = 9;
        for idx in 0..count.min(5) {
            let ry = start_y + (idx as i32) * row_h;
            if ry + 6 > h {
                break;
            }
            let entry = &meta.hiscores[idx];
            let r_col = rank_color_rgb(entry.rank);
            let r_lbl = rank_label_short(entry.rank);

            if idx == 0 {
                surface.fill_rect(2, ry - 1, w - 4, 8, 28, 22, 10);
            }

            if w >= 128 {
                draw_small_text_clipped_bounds(surface, 4, ry, 2, w - 2, 13, h - 1, r_lbl, r_col);
                draw_small_text_clipped_bounds(
                    surface,
                    28,
                    ry,
                    28,
                    w / 2,
                    13,
                    h - 1,
                    entry.player_str(),
                    (255, 255, 255),
                );
                let score_s = entry.score_str();
                let sw = measure_small_text(score_s);
                let sx = (w - 4 - sw).max(w / 2 + 4);
                if w >= 192 {
                    let dot_start = 28 + measure_small_text(entry.player_str()) + 6;
                    let mut dx = dot_start;
                    while dx + 4 < sx {
                        surface.set_pixel(dx, ry + 5, 50, 60, 90);
                        dx += 4;
                    }
                }
                draw_small_text_clipped_bounds(
                    surface,
                    sx,
                    ry,
                    28,
                    w - 2,
                    13,
                    h - 1,
                    score_s,
                    r_col,
                );
            } else {
                draw_tiny_text_clipped_bounds(surface, 2, ry, 2, w - 2, 13, h - 1, r_lbl, r_col);
                draw_tiny_text_clipped_bounds(
                    surface,
                    16,
                    ry,
                    16,
                    w - 2,
                    13,
                    h - 1,
                    entry.player_str(),
                    (255, 255, 255),
                );
                let score_s = entry.score_str();
                let sw = measure_tiny_text(score_s);
                let sx = (w - 2 - sw).max(30);
                draw_tiny_text_clipped_bounds(
                    surface,
                    sx,
                    ry,
                    30,
                    w - 2,
                    13,
                    h - 1,
                    score_s,
                    r_col,
                );
            }
        }
    }

    fn draw_yoko_row_compact(
        surface: &mut dyn IDrawingSurface,
        entry: &SyncHiscoreEntryPod,
        w: i32,
        ry: i32,
        min_y: i32,
        max_y: i32,
    ) {
        let r_col = rank_color_rgb(entry.rank);
        let r_lbl = rank_label_short(entry.rank);
        draw_tiny_text_clipped_bounds(surface, 2, ry, 2, w - 2, min_y, max_y, r_lbl, r_col);
        draw_tiny_text_clipped_bounds(
            surface,
            18,
            ry,
            18,
            w / 2 + 8,
            min_y,
            max_y,
            entry.player_str(),
            (255, 255, 255),
        );
        let score_s = entry.score_str();
        let sw = measure_tiny_text(score_s);
        let sx = (w - 2 - sw).max(34);
        draw_tiny_text_clipped_bounds(surface, sx, ry, 34, w - 2, min_y, max_y, score_s, r_col);
    }
}

// ----------------------------------------------------------------------------
// RetroAchievementsCard — Responsive Yoko & Tate with Beveled RPG XP Bar
// ----------------------------------------------------------------------------

pub struct RetroAchievementsCard;

impl RetroAchievementsCard {
    /// Renders the RetroAchievements progress card with zero heap allocations.
    pub fn render(
        surface: &mut dyn IDrawingSurface,
        meta: &SyncMetaPayload,
        lang: Lang,
        elapsed_ms: u64,
    ) {
        let w = surface.width() as i32;
        let h = surface.height() as i32;
        if w <= 0 || h <= 0 {
            return;
        }

        let ra = &meta.ra;
        surface.fill_rect(0, 0, w, h, 6, 8, 18);

        let border_col = if ra.hardcore {
            if ((elapsed_ms / 300) & 1) == 0 {
                (255, 205, 0)
            } else {
                (255, 235, 110)
            }
        } else {
            (0, 150, 210)
        };
        draw_round_rect(surface, 0, 0, w, h, 2, border_col);

        let mut progress_buf = StackStrBuf::<24>::new();
        progress_buf.push_u32(ra.unlocked as u32);
        progress_buf.push_str("/");
        progress_buf.push_u32(ra.total.max(1) as u32);

        let pct = ((ra.unlocked as u32).saturating_mul(100)) / (ra.total.max(1) as u32);
        let mut pts_buf = StackStrBuf::<24>::new();
        pts_buf.push_u32(ra.points_earned);
        pts_buf.push_str(" PTS");

        let trophy = sprite_bank::get_sprite_frame(SpriteId::TrophyCup, 0);
        if h > w {
            sprite_bank::blit_sprite(surface, &trophy, (w - 12) / 2, 3, false);
            let hdr = if ra.hardcore {
                "RA HARDCORE"
            } else {
                "RETROACHIEV."
            };
            let hw = measure_tiny_text(hdr);
            draw_tiny_text_clipped_bounds(
                surface,
                ((w - hw) / 2).max(2),
                17,
                2,
                w - 3,
                2,
                h - 2,
                hdr,
                border_col,
            );

            let bar_x = 3;
            let bar_w = (w - 6).max(8);
            let bar_y = 25;
            let bar_h = if h >= 128 { 8 } else { 6 };
            Self::draw_beveled_xp_bar(
                surface,
                bar_x,
                bar_y,
                bar_w,
                bar_h,
                ra.unlocked,
                ra.total,
                ra.hardcore,
            );

            let pw = measure_tiny_text(progress_buf.as_str());
            draw_tiny_text_clipped_bounds(
                surface,
                ((w - pw) / 2).max(2),
                bar_y + bar_h + 3,
                2,
                w - 3,
                2,
                h - 2,
                progress_buf.as_str(),
                (255, 255, 255),
            );

            let ptw = measure_tiny_text(pts_buf.as_str());
            draw_tiny_text_clipped_bounds(
                surface,
                ((w - ptw) / 2).max(2),
                bar_y + bar_h + 11,
                2,
                w - 3,
                2,
                h - 2,
                pts_buf.as_str(),
                (0, 229, 255),
            );

            let badge = ra.recent_badge_str();
            if !badge.is_empty() && h >= 64 {
                draw_tiny_text_clipped_bounds(
                    surface,
                    3,
                    bar_y + bar_h + 20,
                    3,
                    w - 3,
                    2,
                    h - 3,
                    badge,
                    (255, 220, 120),
                );
            }
            return;
        }

        if h <= 32 {
            sprite_bank::blit_sprite(surface, &trophy, 3, 3, false);
            let title = if w >= 128 {
                if ra.hardcore {
                    "RETROACHIEVEMENTS [HC]"
                } else {
                    "RETROACHIEVEMENTS"
                }
            } else if ra.hardcore {
                "RA [HC]"
            } else {
                "RA"
            };
            draw_tiny_text_clipped_bounds(surface, 17, 4, 17, w - 3, 2, 10, title, border_col);

            let bar_x = 17;
            let bar_w = (w - 21).max(12);
            Self::draw_beveled_xp_bar(
                surface,
                bar_x,
                12,
                bar_w,
                6,
                ra.unlocked,
                ra.total,
                ra.hardcore,
            );

            draw_tiny_text_clipped_bounds(
                surface,
                4,
                22,
                2,
                w - 3,
                18,
                h - 2,
                progress_buf.as_str(),
                (255, 255, 255),
            );
            let ptw = measure_tiny_text(pts_buf.as_str());
            draw_tiny_text_clipped_bounds(
                surface,
                (w - 4 - ptw).max(32),
                22,
                32,
                w - 3,
                18,
                h - 2,
                pts_buf.as_str(),
                (0, 229, 255),
            );
            return;
        }

        sprite_bank::blit_sprite(surface, &trophy, 4, 4, false);
        let hdr = if w >= 192 {
            if ra.hardcore {
                "RETROACHIEVEMENTS • HARDCORE"
            } else {
                "RETROACHIEVEMENTS"
            }
        } else {
            i18n::sync_achievement_title(lang)
        };
        if w >= 192 {
            draw_small_text_clipped_bounds(surface, 20, 5, 20, w - 4, 2, 14, hdr, border_col);
        } else {
            draw_tiny_text_clipped_bounds(surface, 18, 6, 18, w - 4, 2, 14, hdr, border_col);
        }

        let game = if !ra.game_title_str().is_empty() {
            ra.game_title_str()
        } else {
            meta.game_title_str()
        };
        if !game.is_empty() {
            draw_tiny_text_clipped_bounds(
                surface,
                20,
                16,
                20,
                w - 4,
                14,
                24,
                game,
                (220, 235, 255),
            );
        }

        let bar_x = 6;
        let bar_w = (w - 12).max(16);
        Self::draw_beveled_xp_bar(
            surface,
            bar_x,
            25,
            bar_w,
            10,
            ra.unlocked,
            ra.total,
            ra.hardcore,
        );

        let mut stats_line = StackStrBuf::<32>::new();
        stats_line.push_str(progress_buf.as_str());
        stats_line.push_str(" (");
        stats_line.push_u32(pct);
        stats_line.push_str("%)");
        draw_small_text_clipped_bounds(
            surface,
            6,
            39,
            4,
            w - 4,
            36,
            50,
            stats_line.as_str(),
            (255, 255, 255),
        );

        let ptw = measure_small_text(pts_buf.as_str());
        draw_small_text_clipped_bounds(
            surface,
            (w - 6 - ptw).max(w / 2),
            39,
            w / 2,
            w - 4,
            36,
            50,
            pts_buf.as_str(),
            (255, 215, 0),
        );

        let badge = ra.recent_badge_str();
        if !badge.is_empty() {
            draw_tiny_text_clipped_bounds(
                surface,
                6,
                51,
                4,
                w - 4,
                48,
                h - 3,
                badge,
                (0, 229, 255),
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_beveled_xp_bar(
        surface: &mut dyn IDrawingSurface,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        unlocked: u16,
        total: u16,
        hardcore: bool,
    ) {
        if w <= 4 || h <= 3 {
            return;
        }
        draw_round_rect(surface, x, y, w, h, 1, (70, 85, 120));
        fill_round_rect(surface, x + 1, y + 1, w - 2, h - 2, 1, (14, 18, 30));

        let inner_w = (w - 2).max(0);
        let fill_w =
            ((inner_w as u32).saturating_mul(unlocked as u32) / (total.max(1) as u32)) as i32;
        let fill_w = fill_w.clamp(0, inner_w);
        if fill_w > 0 {
            let (top_col, bot_col) = if hardcore {
                ((255, 235, 90), (215, 145, 0))
            } else {
                ((80, 255, 170), (0, 185, 100))
            };
            let top_h = ((h - 2) / 2).max(1);
            let bot_h = (h - 2 - top_h).max(0);
            surface.fill_rect(x + 1, y + 1, fill_w, top_h, top_col.0, top_col.1, top_col.2);
            if bot_h > 0 {
                surface.fill_rect(
                    x + 1,
                    y + 1 + top_h,
                    fill_w,
                    bot_h,
                    bot_col.0,
                    bot_col.1,
                    bot_col.2,
                );
            }
        }
    }
}

// ----------------------------------------------------------------------------
// SyncCelebrationRenderer — Priority 55 60 FPS Fireworks + Hero Cameo (0 Alloc)
// ----------------------------------------------------------------------------

pub struct SyncCelebrationRenderer;

impl SyncCelebrationRenderer {
    /// Renders celebratory night sky, `Q8.8` integer fireworks, bouncing header,
    /// system-matched arcade hero cameo, and footer onto `surface`.
    pub fn render(
        surface: &mut dyn IDrawingSurface,
        fireworks: &FireworksLutRenderer,
        payload: &SyncCelebrationPayload,
        lang: Lang,
        frame_tick: u32,
    ) {
        let w = surface.width() as i32;
        let h = surface.height() as i32;
        if w <= 0 || h <= 0 {
            return;
        }

        // Layer 1: Celebratory night sky + Q8.8 integer fireworks
        surface.fill_rect(0, 0, w, h, 4, 4, 14);
        fireworks.render(surface);

        // Layer 2: Bouncing arcade header via BOUNCE_LUT
        let bounce_idx = ((frame_tick as usize) / 3) % BOUNCE_LUT.len();
        let bounce_dy = BOUNCE_LUT[bounce_idx] as i32;
        let header_y = (3 + bounce_dy / 2).max(1);
        let flash = ((frame_tick / 4) & 1) == 0;
        let hdr_col = if flash {
            (255, 230, 60)
        } else {
            (255, 255, 255)
        };

        let header_str = if payload.kind == 1 {
            i18n::sync_achievement_title(lang)
        } else {
            i18n::sync_new_record_title(lang)
        };
        let hw = measure_tiny_text(header_str);
        let hx = ((w - hw) / 2).max(2);
        draw_tiny_text_clipped_bounds(
            surface,
            hx,
            header_y,
            1,
            w - 2,
            0,
            h - 1,
            header_str,
            hdr_col,
        );

        // Layer 3: System-matched Hero Cameo from sprite_bank
        let hero_family = CelebrationHeroFamily::from_system(payload.system_str());
        let anim_step = (frame_tick / 4) as usize;
        let stage_y = if h <= 32 { 9 } else { (h / 2) - 8 };

        match hero_family {
            CelebrationHeroFamily::NintendoMario => {
                let is_jumping = (anim_step % 8) >= 5;
                let mario_frame = if is_jumping {
                    sprite_bank::mario_jump_frame()
                } else {
                    sprite_bank::mario_run_frame(anim_step)
                };
                let mario_y = if is_jumping { stage_y - 3 } else { stage_y };
                let mario_x = (w / 2) - 14;
                sprite_bank::blit_sprite(surface, &mario_frame, mario_x, mario_y, false);
                let coin_frame = sprite_bank::mario_gold_coin_frame();
                sprite_bank::blit_sprite(surface, &coin_frame, mario_x + 18, stage_y - 2, false);
            }
            CelebrationHeroFamily::SegaSonic => {
                let sonic_x = (w / 2) - 16;
                let sonic_frame = if h <= 32 {
                    sprite_bank::sonic_sms_run_frame(anim_step)
                } else if (anim_step % 8) >= 5 {
                    sprite_bank::sonic_ball_frame(anim_step)
                } else {
                    sprite_bank::sonic_md_run_frame(anim_step)
                };
                sprite_bank::blit_sprite(surface, &sonic_frame, sonic_x, stage_y - 4, false);
                let ring_frame = sprite_bank::sonic_ring_frame(anim_step);
                sprite_bank::blit_sprite(surface, &ring_frame, sonic_x + 24, stage_y + 2, false);
            }
            CelebrationHeroFamily::CapcomMegaMan => {
                let mm_x = (w / 2) - 18;
                let mm_frame = if (anim_step % 8) >= 5 {
                    sprite_bank::megaman_jump_shoot_frame()
                } else {
                    sprite_bank::megaman_run_frame(anim_step)
                };
                sprite_bank::blit_sprite(surface, &mm_frame, mm_x, stage_y - 4, false);
                let bullet_dx = ((frame_tick as i32) * 2) % 20;
                let bullet_frame = sprite_bank::megaman_bullet_frame();
                sprite_bank::blit_sprite(
                    surface,
                    &bullet_frame,
                    mm_x + 24 + bullet_dx,
                    stage_y + 4,
                    false,
                );
                if bullet_dx >= 14 {
                    let spark_frame = sprite_bank::megaman_spark_frame();
                    sprite_bank::blit_sprite(
                        surface,
                        &spark_frame,
                        mm_x + 26 + bullet_dx,
                        stage_y + 2,
                        false,
                    );
                }
            }
            CelebrationHeroFamily::SnkMetalSlug => {
                let marco_x = (w / 2) - 14;
                let marco_frame = if (anim_step % 8) >= 4 {
                    sprite_bank::marco_victory_frame()
                } else {
                    sprite_bank::marco_walk_frame(anim_step)
                };
                sprite_bank::blit_sprite(surface, &marco_frame, marco_x, stage_y - 6, false);
            }
            CelebrationHeroFamily::ArcadePacMan => {
                let pac_x = (w / 2) - 14;
                let pac_frame = sprite_bank::pacman_frame(anim_step);
                sprite_bank::blit_sprite(surface, &pac_frame, pac_x, stage_y, false);
                sprite_bank::draw_frightened_ghost(
                    surface,
                    pac_x + 16,
                    stage_y,
                    anim_step,
                    (anim_step & 2) != 0,
                );
            }
        }

        // Layer 4: Score / Achievement title & points footer
        let title_s = payload.title_str();
        let score_s = payload.score_or_pts_str();
        if h <= 32 {
            let bot_y = h - 6;
            let mut footer = StackStrBuf::<48>::new();
            if !title_s.is_empty() {
                footer.push_str(title_s);
                if !score_s.is_empty() {
                    footer.push_str(" ");
                    footer.push_str(score_s);
                }
            } else {
                footer.push_str(score_s);
            }
            let fw = measure_tiny_text(footer.as_str());
            let fx = ((w - fw) / 2).max(2);
            draw_tiny_text_clipped_bounds(
                surface,
                fx,
                bot_y,
                1,
                w - 2,
                0,
                h - 1,
                footer.as_str(),
                (0, 229, 255),
            );
        } else {
            if !title_s.is_empty() {
                let tw = measure_tiny_text(title_s);
                let tx = ((w - tw) / 2).max(2);
                draw_tiny_text_clipped_bounds(
                    surface,
                    tx,
                    h - 15,
                    2,
                    w - 2,
                    0,
                    h - 1,
                    title_s,
                    (255, 255, 255),
                );
            }
            if !score_s.is_empty() {
                let sw = measure_small_text(score_s);
                let sx = ((w - sw) / 2).max(2);
                draw_small_text_clipped_bounds(
                    surface,
                    sx,
                    h - 8,
                    2,
                    w - 2,
                    0,
                    h - 1,
                    score_s,
                    (0, 229, 255),
                );
            }
        }
    }
}

/// Pure zero-allocation renderer for pre-decoded 24-bit `RGB888` Offline In-Game Showcase `.gif` frames (ADR-0004).
pub struct SyncOfflineGifRenderer;

impl SyncOfflineGifRenderer {
    #[inline]
    pub fn render(
        surface: &mut dyn IDrawingSurface,
        frames: &[(image::RgbImage, std::time::Duration)],
        frame_index: usize,
    ) {
        if let Some((ref img, _)) = frames.get(frame_index) {
            surface.draw_image(img, 0, 0);
        }
    }
}

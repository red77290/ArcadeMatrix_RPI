//! Pure zero-allocation renderer for [`crate::engines::agenda::AgendaEngine`] (ADR-0004).
//!
//! Draws Chill `FREE PLAY` scenes, Multi-Day vertical scroll feeds, Horizontal tickers,
//! and responsive Yoko/Tate/Square event decks onto any [`IDrawingSurface`] without
//! owning business state, network sockets, or disk caches.

use crate::core::i18n::{
    agenda_free_play_label, agenda_next_prefix, agenda_now_active_label, agenda_today_label,
    agenda_tomorrow_label, format_agenda_day_banner_buf, format_agenda_imminent_buf, Lang,
};
use crate::core::matrix::IDrawingSurface;
use crate::core::types::{DisplayGeometry, LayoutClass};
use crate::engines::agenda::{AgendaTheme, AgendaViewMode};
use crate::engines::agenda_ics::AgendaEventPod;
use crate::engines::clocks::sprite_bank::{
    blit_sprite_clipped, get_sprite_frame, metool_sleep_frame, rgb565_to_rgb888, SpriteId,
};
use crate::engines::dashboard::font::{draw_text_clipped, measure_text};
use crate::engines::renderers::tiny_font::{draw_tiny_text, tiny_text_width};
use chrono::{Datelike, TimeZone, Timelike};
use chrono_tz::Tz;

#[derive(Clone, Copy)]
pub struct AgendaThemeColors {
    pub header_bg: (u8, u8, u8),
    pub header_accent: (u8, u8, u8),
    pub header_text: (u8, u8, u8),
    pub title_text: (u8, u8, u8),
    pub dim_text: (u8, u8, u8),
    pub divider: (u8, u8, u8),
    pub card_bg: Option<(u8, u8, u8)>,
    pub override_calendar: Option<(u8, u8, u8)>,
}

impl AgendaThemeColors {
    pub const fn from_theme(theme: AgendaTheme) -> Self {
        match theme {
            AgendaTheme::AmberDmd => Self {
                header_bg: (40, 18, 0),
                header_accent: (255, 165, 0),
                header_text: (255, 220, 40),
                title_text: (255, 175, 20),
                dim_text: (190, 110, 10),
                divider: (90, 45, 0),
                card_bg: None,
                override_calendar: Some((255, 165, 0)),
            },
            AgendaTheme::MatrixGreen => Self {
                header_bg: (0, 28, 8),
                header_accent: (0, 255, 90),
                header_text: (140, 255, 170),
                title_text: (40, 255, 110),
                dim_text: (0, 160, 60),
                divider: (0, 75, 25),
                card_bg: None,
                override_calendar: Some((0, 255, 90)),
            },
            AgendaTheme::SnesRpg => Self {
                header_bg: (20, 36, 110),
                header_accent: (255, 215, 0),
                header_text: (255, 255, 255),
                title_text: (245, 248, 255),
                dim_text: (170, 195, 235),
                divider: (100, 130, 190),
                card_bg: Some((8, 18, 56)),
                override_calendar: None,
            },
            AgendaTheme::MinimalClean => Self {
                header_bg: (18, 22, 28),
                header_accent: (180, 195, 210),
                header_text: (255, 255, 255),
                title_text: (245, 248, 255),
                dim_text: (140, 155, 175),
                divider: (45, 55, 70),
                card_bg: None,
                override_calendar: None,
            },
            AgendaTheme::CyberArcade => Self {
                header_bg: (10, 18, 36),
                header_accent: (0, 180, 255),
                header_text: (0, 235, 255),
                title_text: (245, 248, 255),
                dim_text: (150, 180, 210),
                divider: (25, 45, 75),
                card_bg: None,
                override_calendar: None,
            },
        }
    }
}

/// Zero-allocation stack snapshot passed from [`crate::engines::agenda::AgendaEngine`] to [`AgendaRenderer`].
pub struct AgendaRenderParams<'a> {
    pub events: &'a [AgendaEventPod],
    pub event_count: usize,
    pub events_shown: usize,
    pub current_slide_idx: usize,
    pub view_mode: AgendaViewMode,
    pub theme: AgendaTheme,
    pub lang: Lang,
    pub tz: Tz,
    pub now_epoch: u32,
    pub imminent_min: u32,
    pub anim_tick: u32,
    pub scroll_offset: i32,
    pub show_relative_time: bool,
    pub show_badges: bool,
    pub fun_sprites: bool,
    pub has_secondary_calendar: bool,
    pub label_1_buf: &'a [u8; 20],
    pub color_1_565: u16,
    pub label_2_buf: &'a [u8; 20],
    pub color_2_565: u16,
}

pub struct AgendaRenderer;

impl AgendaRenderer {
    pub fn render(surface: &mut dyn IDrawingSurface, p: &AgendaRenderParams<'_>) {
        let w = surface.width() as i32;
        let h = surface.height() as i32;
        if w <= 0 || h <= 0 {
            return;
        }
        let layout = DisplayGeometry::classify(w as u32, h as u32);

        if p.event_count == 0 {
            Self::render_chill_mode(surface, p, w, h);
            return;
        }

        let tc = AgendaThemeColors::from_theme(p.theme);
        if let Some((br, bg, bb)) = tc.card_bg {
            surface.fill_rect(0, 0, w, h, br, bg, bb);
        }

        match p.view_mode {
            AgendaViewMode::MultiDayScroll => {
                Self::render_multiday_vertical_scroll(surface, p, &tc, w, h);
                return;
            }
            AgendaViewMode::Ticker => {
                Self::render_multiday_ticker(surface, p, &tc, w, h);
                return;
            }
            _ => {}
        }

        let shown = p.event_count.min(p.events_shown).max(1);
        let start_idx = p.current_slide_idx % shown;

        match layout {
            LayoutClass::Portrait | LayoutClass::Tall => {
                surface.fill_rect(0, 0, w, 9, tc.header_bg.0, tc.header_bg.1, tc.header_bg.2);
                surface.fill_rect(
                    0,
                    9,
                    w,
                    1,
                    tc.header_accent.0,
                    tc.header_accent.1,
                    tc.header_accent.2,
                );
                let mut hdr_buf = [0u8; 32];
                let hdr = Self::format_event_day_header(
                    p,
                    p.events[start_idx].start_epoch,
                    w < 64,
                    &mut hdr_buf,
                );
                draw_tiny_text(surface, hdr, 2, 2, 0, w, tc.header_text);

                let max_cards = ((h - 12) / 18).clamp(1, shown as i32) as usize;
                let mut cy = 12;
                for row in 0..max_cards {
                    let ev = &p.events[(start_idx + row) % shown];
                    Self::render_event_row(surface, p, &tc, ev, 1, cy, w - 2, 16, row == 0);
                    cy += 18;
                }
            }
            LayoutClass::Square => {
                surface.fill_rect(0, 0, w, 9, tc.header_bg.0, tc.header_bg.1, tc.header_bg.2);
                surface.fill_rect(
                    0,
                    9,
                    w,
                    1,
                    tc.header_accent.0,
                    tc.header_accent.1,
                    tc.header_accent.2,
                );
                let mut hdr_buf = [0u8; 32];
                let hdr = Self::format_event_day_header(
                    p,
                    p.events[start_idx].start_epoch,
                    true,
                    &mut hdr_buf,
                );
                draw_tiny_text(surface, hdr, 2, 2, 0, w, tc.header_text);

                let cards = shown.min(3);
                let mut cy = 12;
                for row in 0..cards {
                    let ev = &p.events[(start_idx + row) % shown];
                    Self::render_event_row(surface, p, &tc, ev, 1, cy, w - 2, 15, row == 0);
                    cy += 17;
                }
            }
            LayoutClass::Wide | LayoutClass::Landscape => {
                if h <= 32 || p.view_mode == AgendaViewMode::Cards {
                    let ev0 = &p.events[start_idx];
                    Self::render_event_row(surface, p, &tc, ev0, 1, 2, w - 2, 16, true);
                    surface.fill_rect(0, 19, w, 1, tc.divider.0, tc.divider.1, tc.divider.2);

                    let mut hdr_buf = [0u8; 32];
                    let day_hdr =
                        Self::format_event_day_header(p, ev0.start_epoch, true, &mut hdr_buf);
                    draw_tiny_text(surface, day_hdr, 2, 23, 0, w, tc.header_text);
                    let nx = 2 + tiny_text_width(day_hdr) + 4;

                    if shown >= 2 {
                        let ev1 = &p.events[(start_idx + 1) % shown];
                        let next_lbl = agenda_next_prefix(p.lang);
                        draw_tiny_text(surface, next_lbl, nx, 23, 0, w, tc.dim_text);
                        let nx2 = nx + tiny_text_width(next_lbl) + 3;
                        Self::render_event_row(
                            surface,
                            p,
                            &tc,
                            ev1,
                            nx2,
                            21,
                            (w - nx2 - 1).max(8),
                            10,
                            false,
                        );
                    } else if p.show_badges {
                        let badge = ev0.badge_label_str();
                        let cal_rgb = tc
                            .override_calendar
                            .unwrap_or_else(|| rgb565_to_rgb888(ev0.calendar_color_565));
                        draw_tiny_text(surface, badge, nx, 23, 0, w, cal_rgb);
                    }
                } else {
                    surface.fill_rect(0, 0, w, 10, tc.header_bg.0, tc.header_bg.1, tc.header_bg.2);
                    surface.fill_rect(
                        0,
                        10,
                        w,
                        1,
                        tc.header_accent.0,
                        tc.header_accent.1,
                        tc.header_accent.2,
                    );
                    let mut hdr_buf = [0u8; 32];
                    let hdr = Self::format_event_day_header(
                        p,
                        p.events[start_idx].start_epoch,
                        false,
                        &mut hdr_buf,
                    );
                    draw_tiny_text(surface, hdr, 3, 3, 0, w, tc.header_text);

                    if p.show_badges {
                        let l1 = core::str::from_utf8(p.label_1_buf)
                            .unwrap_or("Perso")
                            .trim_end_matches('\0');
                        let (r1, g1, b1) = tc
                            .override_calendar
                            .unwrap_or_else(|| rgb565_to_rgb888(p.color_1_565));
                        let x_l1 = (w - 64).max(58);
                        surface.fill_rect(x_l1 - 4, 4, 2, 3, r1, g1, b1);
                        draw_tiny_text(surface, l1, x_l1, 3, 0, w, (r1, g1, b1));

                        if p.has_secondary_calendar {
                            let l2 = core::str::from_utf8(p.label_2_buf)
                                .unwrap_or("Pro")
                                .trim_end_matches('\0');
                            let (r2, g2, b2) = tc
                                .override_calendar
                                .unwrap_or_else(|| rgb565_to_rgb888(p.color_2_565));
                            let x_l2 = (w - 28).max(x_l1 + 24);
                            surface.fill_rect(x_l2 - 4, 4, 2, 3, r2, g2, b2);
                            draw_tiny_text(surface, l2, x_l2, 3, 0, w, (r2, g2, b2));
                        }
                    }

                    let page_rows = shown.min(4);
                    let row_h = ((h - 13) / (page_rows.max(1) as i32)).clamp(12, 18);
                    let mut cy = 13;
                    for row in 0..page_rows {
                        let ev = &p.events[(start_idx + row) % shown];
                        Self::render_event_row(
                            surface,
                            p,
                            &tc,
                            ev,
                            2,
                            cy,
                            w - 4,
                            row_h - 1,
                            row == 0,
                        );
                        cy += row_h;
                    }
                }
            }
        }
    }

    fn render_chill_mode(
        surface: &mut dyn IDrawingSurface,
        p: &AgendaRenderParams<'_>,
        w: i32,
        h: i32,
    ) {
        let ground_y = h - 5;
        surface.fill_rect(0, ground_y, w, 1, 0, 140, 210);
        surface.fill_rect(0, ground_y + 1, w, 4, 12, 22, 42);

        let peek = (p.anim_tick / 60).is_multiple_of(8);
        let metool = metool_sleep_frame(peek);
        let mx = if w >= 128 {
            12
        } else if h > w {
            (w - metool.width) / 2
        } else {
            4
        };
        let my = (ground_y - metool.height).max(2);
        blit_sprite_clipped(surface, &metool, mx, my, false, 0, w, 0, h);

        if !peek {
            let phase = ((p.anim_tick / 8) % 12) as i32;
            let zx = mx + metool.width + 1 + (phase / 4);
            let zy = (my - 2 - (phase / 2)).max(1);
            draw_tiny_text(surface, "Z", zx, zy, 0, w, (120, 220, 255));
            if zy >= 6 {
                draw_tiny_text(surface, "Z", zx + 4, zy - 4, 0, w, (80, 160, 220));
            }
        }

        let label = agenda_free_play_label(p.lang);
        let text_w = measure_text(label);
        let text_x = if h > w {
            2
        } else {
            (mx + metool.width + 6).min((w - 16).max(0))
        };
        let avail_w = (w - text_x - 2).max(16);
        let text_y = if h > w {
            (my - 12).max(4)
        } else {
            (h / 2 - 4).max(2)
        };

        if text_w <= avail_w {
            draw_text_clipped(
                surface,
                label,
                text_x,
                text_y,
                text_x,
                w - 1,
                0,
                h,
                (0, 235, 255),
            );
        } else {
            let span = text_w + 24;
            let sx = text_x - (p.scroll_offset % span.max(1));
            draw_text_clipped(
                surface,
                label,
                sx,
                text_y,
                text_x,
                w - 1,
                0,
                h,
                (0, 235, 255),
            );
            draw_text_clipped(
                surface,
                label,
                sx + span,
                text_y,
                text_x,
                w - 1,
                0,
                h,
                (0, 235, 255),
            );
        }
    }

    fn render_multiday_vertical_scroll(
        surface: &mut dyn IDrawingSurface,
        p: &AgendaRenderParams<'_>,
        tc: &AgendaThemeColors,
        w: i32,
        h: i32,
    ) {
        let shown = p.event_count.min(p.events_shown);
        if shown == 0 {
            return;
        }

        let mut total_h = 0i32;
        let mut prev_day = i64::MIN;
        for ev in p.events.iter().take(shown) {
            let day_key = (ev.start_epoch as i64) / 86400;
            if day_key != prev_day {
                prev_day = day_key;
                total_h += 11;
            }
            total_h += 14;
        }

        let max_scroll = (total_h - h + 8).max(0);
        let y_off = if max_scroll > 0 {
            let cycle = max_scroll + 24;
            (p.scroll_offset % cycle.max(1)).min(max_scroll)
        } else {
            0
        };

        let mut cy = 1 - y_off;
        prev_day = i64::MIN;
        for ev in p.events.iter().take(shown) {
            let day_key = (ev.start_epoch as i64) / 86400;
            if day_key != prev_day {
                prev_day = day_key;
                if cy + 9 > 0 && cy < h {
                    surface.fill_rect(
                        1,
                        cy.max(0),
                        w - 2,
                        (cy + 9).min(h) - cy.max(0),
                        tc.header_bg.0,
                        tc.header_bg.1,
                        tc.header_bg.2,
                    );
                    let mut day_buf = [0u8; 32];
                    let day_hdr =
                        Self::format_event_day_header(p, ev.start_epoch, w < 96, &mut day_buf);
                    if cy >= 0 && cy + 7 <= h {
                        draw_tiny_text(surface, day_hdr, 3, cy + 2, 1, w - 1, tc.header_text);
                    }
                }
                cy += 11;
            }

            if cy + 13 > 0 && cy < h {
                Self::render_event_row(surface, p, tc, ev, 1, cy, w - 2, 12, false);
            }
            cy += 14;
        }
    }

    fn render_multiday_ticker(
        surface: &mut dyn IDrawingSurface,
        p: &AgendaRenderParams<'_>,
        tc: &AgendaThemeColors,
        w: i32,
        h: i32,
    ) {
        let shown = p.event_count.min(p.events_shown);
        if shown == 0 {
            return;
        }

        let lead_ev = &p.events[0];
        surface.fill_rect(0, 0, w, 10, tc.header_bg.0, tc.header_bg.1, tc.header_bg.2);
        surface.fill_rect(
            0,
            10,
            w,
            1,
            tc.header_accent.0,
            tc.header_accent.1,
            tc.header_accent.2,
        );
        let mut hdr_buf = [0u8; 32];
        let hdr = Self::format_event_day_header(p, lead_ev.start_epoch, false, &mut hdr_buf);
        draw_tiny_text(surface, hdr, 3, 3, 0, w, tc.header_text);

        let row_y = if h >= 48 { (h + 10) / 2 - 6 } else { 15 };
        let item_w = 140i32;
        let total_span = (shown as i32 * item_w).max(w);
        let base_x = (w / 3) - ((p.scroll_offset * 2) % total_span.max(1));

        for pass in 0..2 {
            let mut cx = base_x + pass * total_span;
            for ev in p.events.iter().take(shown) {
                if cx + item_w > 0 && cx < w {
                    Self::render_event_row(surface, p, tc, ev, cx, row_y, item_w - 8, 14, false);
                }
                cx += item_w;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_event_row(
        surface: &mut dyn IDrawingSurface,
        p: &AgendaRenderParams<'_>,
        tc: &AgendaThemeColors,
        ev: &AgendaEventPod,
        x: i32,
        y: i32,
        row_w: i32,
        row_h: i32,
        scroll_title: bool,
    ) {
        let (cr, cg, cb) = tc
            .override_calendar
            .unwrap_or_else(|| rgb565_to_rgb888(ev.calendar_color_565));
        let mut time_buf = [0u8; 24];
        let (time_str, is_active, is_imminent) =
            Self::format_event_time_badge(p, ev, &mut time_buf);

        let pulse_on = (p.anim_tick / 4).is_multiple_of(2);
        let bar_color = if is_imminent && pulse_on {
            (255, 230, 50)
        } else if is_active && pulse_on {
            (50, 255, 120)
        } else {
            (cr, cg, cb)
        };
        surface.fill_rect(x, y, 2, row_h.max(7), bar_color.0, bar_color.1, bar_color.2);

        let mut cur_x = x + 4;
        if p.fun_sprites && row_w >= 48 && row_h >= 11 {
            let sprite = get_sprite_frame(SpriteId::from_u8(ev.sprite_id), p.anim_tick);
            let sy = y + (row_h - sprite.height) / 2;
            blit_sprite_clipped(
                surface,
                &sprite,
                cur_x,
                sy,
                false,
                cur_x,
                x + row_w,
                y,
                y + row_h,
            );
            cur_x += sprite.width.min(14) + 2;
        }

        let badge_col = if is_active {
            (80, 255, 140)
        } else if is_imminent {
            (255, 210, 40)
        } else {
            (cr, cg, cb)
        };
        let ty = y + (row_h - 5).max(0) / 2;
        draw_tiny_text(surface, time_str, cur_x, ty, cur_x, x + row_w, badge_col);
        cur_x += tiny_text_width(time_str) + 4;

        let title = ev.title_str();
        let avail_w = (x + row_w - cur_x - 1).max(8);
        let title_w = measure_text(title);
        let title_y = y + (row_h - 7).max(0) / 2;

        let sh = surface.height() as i32;
        if !scroll_title || title_w <= avail_w {
            draw_text_clipped(
                surface,
                title,
                cur_x,
                title_y,
                cur_x,
                x + row_w - 1,
                0,
                sh,
                tc.title_text,
            );
        } else {
            let span = title_w + 20;
            let sx = cur_x - (p.scroll_offset % span.max(1));
            draw_text_clipped(
                surface,
                title,
                sx,
                title_y,
                cur_x,
                x + row_w - 1,
                0,
                sh,
                tc.title_text,
            );
            draw_text_clipped(
                surface,
                title,
                sx + span,
                title_y,
                cur_x,
                x + row_w - 1,
                0,
                sh,
                tc.title_text,
            );
        }
    }

    fn format_event_day_header<'a>(
        p: &AgendaRenderParams<'_>,
        start_ep: u32,
        short_form: bool,
        out_buf: &'a mut [u8; 32],
    ) -> &'a str {
        let Some(ev_dt) = p.tz.timestamp_opt(start_ep as i64, 0).single() else {
            return agenda_today_label(p.lang);
        };
        let Some(now_dt) = p.tz.timestamp_opt(p.now_epoch as i64, 0).single() else {
            return agenda_today_label(p.lang);
        };
        let ev_date = ev_dt.date_naive();
        let today = now_dt.date_naive();
        let diff_days = (ev_date - today).num_days();
        let weekday_sun0 = ev_dt.weekday().num_days_from_sunday() as usize;
        format_agenda_day_banner_buf(
            p.lang,
            diff_days,
            weekday_sun0,
            ev_dt.day() as u8,
            ev_dt.month() as u8,
            p.show_relative_time,
            short_form,
            out_buf,
        )
    }

    fn format_event_time_badge<'a>(
        p: &AgendaRenderParams<'_>,
        ev: &AgendaEventPod,
        out_buf: &'a mut [u8; 24],
    ) -> (&'a str, bool, bool) {
        let start_ep = ev.start_epoch;
        let end_ep = ev.end_epoch;
        let is_active = p.now_epoch >= start_ep && p.now_epoch < end_ep;
        if is_active && !ev.is_all_day() {
            return (agenda_now_active_label(p.lang), true, true);
        }

        if start_ep > p.now_epoch {
            let diff_sec = start_ep - p.now_epoch;
            let diff_min = diff_sec.div_ceil(60);
            if diff_min <= p.imminent_min && p.imminent_min > 0 && !ev.is_all_day() {
                let s = format_agenda_imminent_buf(p.lang, diff_min.max(1), out_buf);
                return (s, false, true);
            }
        }

        let Some(ev_dt) = p.tz.timestamp_opt(start_ep as i64, 0).single() else {
            return ("--:--", false, false);
        };
        let Some(now_dt) = p.tz.timestamp_opt(p.now_epoch as i64, 0).single() else {
            return ("--:--", false, false);
        };

        let ev_date = ev_dt.date_naive();
        let today = now_dt.date_naive();
        let tomorrow = today.succ_opt().unwrap_or(today);

        if ev.is_all_day() {
            if ev_date <= today {
                return (agenda_today_label(p.lang), true, false);
            } else if ev_date == tomorrow {
                return (agenda_tomorrow_label(p.lang), false, false);
            } else {
                let d = ev_dt.day().min(31) as u8;
                let m = ev_dt.month().min(12) as u8;
                out_buf[0] = b'0' + (d / 10);
                out_buf[1] = b'0' + (d % 10);
                out_buf[2] = b'/';
                out_buf[3] = b'0' + (m / 10);
                out_buf[4] = b'0' + (m % 10);
                let s = core::str::from_utf8(&out_buf[..5]).unwrap_or("DATE");
                return (s, false, false);
            }
        }

        let h = ev_dt.hour().min(23) as u8;
        let m = ev_dt.minute().min(59) as u8;
        if ev_date == today {
            out_buf[0] = b'0' + (h / 10);
            out_buf[1] = b'0' + (h % 10);
            out_buf[2] = b':';
            out_buf[3] = b'0' + (m / 10);
            out_buf[4] = b'0' + (m % 10);
            let s = core::str::from_utf8(&out_buf[..5]).unwrap_or("00:00");
            (s, false, false)
        } else if ev_date == tomorrow {
            let tom = agenda_tomorrow_label(p.lang).as_bytes();
            let prefix_len = tom.len().min(3);
            out_buf[..prefix_len].copy_from_slice(&tom[..prefix_len]);
            out_buf[prefix_len] = b' ';
            out_buf[prefix_len + 1] = b'0' + (h / 10);
            out_buf[prefix_len + 2] = b'0' + (h % 10);
            out_buf[prefix_len + 3] = b':';
            out_buf[prefix_len + 4] = b'0' + (m / 10);
            out_buf[prefix_len + 5] = b'0' + (m % 10);
            let s = core::str::from_utf8(&out_buf[..prefix_len + 6]).unwrap_or("TOM");
            (s, false, false)
        } else {
            let day = ev_dt.day().min(31) as u8;
            let mon = ev_dt.month().min(12) as u8;
            out_buf[0] = b'0' + (day / 10);
            out_buf[1] = b'0' + (day % 10);
            out_buf[2] = b'/';
            out_buf[3] = b'0' + (mon / 10);
            out_buf[4] = b'0' + (mon % 10);
            out_buf[5] = b' ';
            out_buf[6] = b'0' + (h / 10);
            out_buf[7] = b'0' + (h % 10);
            out_buf[8] = b':';
            out_buf[9] = b'0' + (m / 10);
            out_buf[10] = b'0' + (m % 10);
            let s = core::str::from_utf8(&out_buf[..11]).unwrap_or("DATE");
            (s, false, false)
        }
    }
}

use crate::core::engine_contract::{
    Capabilities, ConfigField, ConfigOption, ConfigSchema, ConfigType, Engine, EngineConfig,
    EngineContext, EngineDescriptor, EngineError, EngineMetadata, Requirements, ValidationPolicy,
};
use crate::core::i18n::{self, GNewsStatus, Lang};
use crate::core::matrix::IDrawingSurface;
use crate::core::types::DisplayGeometry;
use crate::engines::dashboard::font::{
    draw_char_clipped, draw_text_clipped, draw_text_scaled, measure_text,
};
use linkme::distributed_slice;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollState {
    PauseStart,
    Scrolling,
    PauseEnd,
}

#[derive(Clone, Debug)]
pub struct GNewsArticle {
    pub title: String,
    pub description: String,
    pub content: String,
    pub source: String,
    pub category: String,
    pub published_epoch: u64,
    pub badge_color: (u8, u8, u8),
}

impl GNewsArticle {
    pub fn compose_text(
        &self,
        show_title: bool,
        show_description: bool,
        show_content: bool,
    ) -> String {
        let title = self.title.trim();
        let desc = self.description.trim();
        let content = self.content.trim();

        let mut parts: Vec<&str> = Vec::new();
        if show_title && !title.is_empty() {
            parts.push(title);
        }
        if show_description && !desc.is_empty() {
            if !parts.iter().any(|p| p.contains(desc)) {
                parts.push(desc);
            }
        }
        if show_content && !content.is_empty() {
            if let Some(pos) = parts.iter().position(|p| content.starts_with(p)) {
                parts[pos] = content;
            } else if !parts.iter().any(|p| p.contains(content)) {
                parts.push(content);
            }
        }

        if parts.is_empty() {
            if !content.is_empty() {
                return content.to_string();
            }
            if !desc.is_empty() {
                return desc.to_string();
            }
            return title.to_string();
        }
        parts.join(" — ")
    }
}

pub struct GNewsEngine {
    // Config fields
    api_key: String,
    category: String,
    keywords: String,
    lang: String,
    country: String,
    max_articles: usize,
    articles_per_rotation: usize,
    cache_ttl_min: u64,
    requests_per_day: u32,
    force_refresh: bool,
    display_mode: String,
    scroll_speed: u32,
    scroll_pause_start_ms: u64,
    scroll_pause_end_ms: u64,
    article_duration_sec: u64,
    theme: String,
    show_title: bool,
    show_description: bool,
    show_content: bool,
    show_category_badge: bool,
    show_source: bool,
    show_time_ago: bool,
    show_beacon: bool,
    show_progress_dots: bool,

    // Thread-safe Async Shared State
    shared_articles: Arc<Mutex<Vec<GNewsArticle>>>,
    shared_status: Arc<Mutex<GNewsStatus>>,
    shared_quota: Arc<Mutex<(usize, Vec<u32>)>>,
    is_fetching: Arc<AtomicBool>,

    // Runtime state
    articles: Vec<GNewsArticle>,
    status: GNewsStatus,
    current_index: usize,
    shuffled_order: Vec<usize>,
    shuffle_pos: usize,
    articles_shown_this_cycle: usize,
    activate_time: Instant,
    current_page_index: usize,
    total_pages: usize,
    last_page_switch: Instant,
    cat_round_robin_idx: usize,
    active_key_idx: usize,
    key_usages: Vec<u32>,
    last_fetch_epoch: u64,
    last_fetch_day: u32,
    scroll_pixel_offset: i32,
    source_marquee_offset: u32,
    scroll_state: ScrollState,
    state_start: Instant,
    last_update: Instant,
    last_scroll_tick: Instant,
    last_source_tick: Instant,
    last_article_switch: Instant,
    last_fetch: Option<Instant>,
    last_fetched_lang: String,
    geometry: DisplayGeometry,
    cached_display_lines: Vec<String>,
    cached_title_chars: Vec<char>,
    cached_max_scroll: i32,
    cached_article_index: usize,
    cached_article_title: String,
    cached_display_mode: String,
}

impl GNewsEngine {
    pub fn new(_w: u32, _h: u32) -> Self {
        let now = Instant::now();
        let now_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let cur_day = (now_epoch / 86400) as u32;

        let mut loaded_articles = Vec::new();
        let mut loaded_epoch = 0;
        let mut loaded_cat_idx = 0;
        let mut loaded_key_idx = 0;
        let mut loaded_usages = Vec::new();
        let mut loaded_status = GNewsStatus::EmptyKey;
        let mut last_fetch_inst = None;

        if let Some((arts, ep, c_idx, k_idx, usages, st_num)) =
            crate::api::gnews::GNewsProvider::load_cache_file("gnews_cache.json")
        {
            if !arts.is_empty() {
                for a in arts {
                    let badge_color = Self::get_category_color(&a.category);
                    loaded_articles.push(GNewsArticle {
                        title: a.title,
                        description: a.description,
                        content: a.content,
                        source: a.source,
                        category: a.category,
                        published_epoch: a.published_epoch,
                        badge_color,
                    });
                }
                loaded_epoch = ep;
                loaded_cat_idx = c_idx;
                loaded_key_idx = k_idx;
                loaded_usages = usages;
                loaded_status = match st_num {
                    2 => GNewsStatus::InvalidKey,
                    3 => GNewsStatus::RateLimited,
                    4 => GNewsStatus::NetworkError,
                    _ => GNewsStatus::Ok,
                };
                let has_content = loaded_articles.iter().any(|a| !a.content.is_empty());
                if has_content && now_epoch >= ep && (now_epoch - ep) < 86400 {
                    let elapsed_sec = now_epoch - ep;
                    last_fetch_inst = now.checked_sub(Duration::from_secs(elapsed_sec));
                }
            }
        }

        let mut engine = Self {
            api_key: String::new(),
            category: "technology".to_string(),
            keywords: String::new(),
            lang: "auto".to_string(),
            country: "auto".to_string(),
            max_articles: 5,
            articles_per_rotation: 2,
            cache_ttl_min: 30,
            requests_per_day: 10,
            force_refresh: false,
            display_mode: "smooth_scroll".to_string(),
            scroll_speed: 3,
            scroll_pause_start_ms: 1200,
            scroll_pause_end_ms: 1000,
            article_duration_sec: 12,
            theme: "category_dynamic".to_string(),
            show_title: false,
            show_description: false,
            show_content: true,
            show_category_badge: true,
            show_source: true,
            show_time_ago: true,
            show_beacon: true,
            show_progress_dots: true,

            shared_articles: Arc::new(Mutex::new(loaded_articles.clone())),
            shared_status: Arc::new(Mutex::new(loaded_status)),
            shared_quota: Arc::new(Mutex::new((loaded_key_idx, loaded_usages.clone()))),
            is_fetching: Arc::new(AtomicBool::new(false)),

            articles: loaded_articles,
            status: loaded_status,
            current_index: 0,
            shuffled_order: Vec::new(),
            shuffle_pos: 0,
            articles_shown_this_cycle: 0,
            activate_time: now,
            current_page_index: 0,
            total_pages: 1,
            last_page_switch: now,
            cat_round_robin_idx: loaded_cat_idx,
            active_key_idx: loaded_key_idx,
            key_usages: loaded_usages,
            last_fetch_epoch: loaded_epoch,
            last_fetch_day: cur_day,
            scroll_pixel_offset: 0,
            source_marquee_offset: 0,
            scroll_state: ScrollState::PauseStart,
            state_start: now,
            last_update: now,
            last_scroll_tick: now,
            last_source_tick: now,
            last_article_switch: now,
            last_fetch: last_fetch_inst,
            last_fetched_lang: String::new(),
            geometry: DisplayGeometry::new(64, 32, 0, 0),
            cached_display_lines: Vec::new(),
            cached_title_chars: Vec::new(),
            cached_max_scroll: 0,
            cached_article_index: usize::MAX,
            cached_article_title: String::new(),
            cached_display_mode: String::new(),
        };
        if !engine.articles.is_empty() {
            engine.pick_next_random_article();
        }
        engine
    }

    pub fn wrap_text_to_lines(text: &str, max_w: i32) -> Vec<String> {
        let mut lines = Vec::new();
        let max_w = max_w.max(12);
        let mut cur_line = String::new();

        for word in text.split_whitespace() {
            let w_word = measure_text(word);
            if w_word > max_w {
                // Flush previous line if not empty
                if !cur_line.is_empty() {
                    lines.push(cur_line);
                    cur_line = String::new();
                }
                // Break long word across lines by characters
                let mut chunk = String::new();
                for ch in word.chars() {
                    let mut test_chunk = chunk.clone();
                    test_chunk.push(ch);
                    if measure_text(&test_chunk) <= max_w {
                        chunk = test_chunk;
                    } else {
                        if !chunk.is_empty() {
                            lines.push(chunk);
                        }
                        chunk = ch.to_string();
                    }
                }
                if !chunk.is_empty() {
                    cur_line = chunk;
                }
            } else {
                let candidate = if cur_line.is_empty() {
                    word.to_string()
                } else {
                    format!("{} {}", cur_line, word)
                };
                if measure_text(&candidate) <= max_w {
                    cur_line = candidate;
                } else {
                    if !cur_line.is_empty() {
                        lines.push(cur_line);
                    }
                    cur_line = word.to_string();
                }
            }
        }
        if !cur_line.is_empty() {
            lines.push(cur_line);
        }
        lines
    }

    pub fn distribute_text_to_rows(text: &str, num_rows: usize) -> Vec<String> {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            return Vec::new();
        }
        let num_rows = num_rows.clamp(1, 16);
        if words.len() <= num_rows {
            return words.into_iter().map(|w| w.to_string()).collect();
        }

        let total_chars: usize =
            words.iter().map(|w| w.chars().count()).sum::<usize>() + words.len() - 1;
        let target_per_row = (total_chars / num_rows).max(8);

        let mut rows = Vec::new();
        let mut cur_row = String::new();

        for word in words {
            if rows.len() + 1 < num_rows {
                let cur_len = cur_row.chars().count();
                if !cur_row.is_empty() && cur_len + word.chars().count() + 1 > target_per_row {
                    rows.push(cur_row);
                    cur_row = word.to_string();
                    continue;
                }
            }
            if cur_row.is_empty() {
                cur_row = word.to_string();
            } else {
                cur_row.push(' ');
                cur_row.push_str(word);
            }
        }
        if !cur_row.is_empty() {
            rows.push(cur_row);
        }
        rows
    }

    pub fn get_category_short(category: &str) -> &'static str {
        i18n::gnews_category_short(Lang::En, category)
    }

    pub fn get_category_color(category: &str) -> (u8, u8, u8) {
        let cat = category.to_lowercase();
        if cat.contains("world") || cat.contains("nation") || cat.contains("break") {
            (255, 42, 77) // Crimson Red
        } else if cat.contains("tech") {
            (0, 229, 255) // Electric Cyan
        } else if cat.contains("bus") || cat.contains("fin") || cat.contains("econ") {
            (0, 230, 118) // Emerald Green
        } else if cat.contains("sport") {
            (255, 145, 0) // Amber Orange
        } else if cat.contains("sci") {
            (213, 0, 249) // Cosmic Purple
        } else if cat.contains("ent") || cat.contains("art") {
            (255, 64, 129) // Hot Pink
        } else if cat.contains("heal") {
            (29, 233, 182) // Seafoam Teal
        } else {
            (224, 230, 237) // Crisp Cool White
        }
    }

    /// Computes (x, y) on the S-curve serpentine track for a given distance along the track.
    ///
    /// - Tier 0: (x_max -> x_min) at body_y (Right -> Left, Even)
    /// - Turn 0: at x_min from body_y to body_y + line_spacing (Down)
    /// - Tier 1: (x_min -> x_max) at body_y + line_spacing (Left -> Right, Odd)
    /// - Turn 1: at x_max from body_y + line_spacing to body_y + 2 * line_spacing (Down)
    /// - Tier 2: (x_max -> x_min) at body_y + 2 * line_spacing (Right -> Left, Even)
    /// ...
    #[inline]
    pub fn serpentine_track_pos(
        dist: i32,
        x_min: i32,
        x_max: i32,
        body_y: i32,
        line_spacing: i32,
    ) -> (i32, i32) {
        let leg_len = (x_max - x_min).max(1);
        let turn_len = line_spacing;
        let tier_len = leg_len + turn_len;

        let tier = dist / tier_len;
        let rem = dist % tier_len;

        let y_tier = body_y + tier * line_spacing;

        if rem < leg_len {
            let is_odd = (tier % 2) != 0;
            let x = if !is_odd {
                // Even tier: Right -> Left
                x_max - rem
            } else {
                // Odd tier: Left -> Right
                x_min + rem
            };
            (x, y_tier)
        } else {
            let turn_progress = rem - leg_len;
            let is_odd = (tier % 2) != 0;
            let x = if !is_odd {
                // Turn at left edge
                x_min
            } else {
                // Turn at right edge
                x_max
            };
            let y = y_tier + turn_progress;
            (x, y)
        }
    }

    /// Computes (x, y) on the mental serpentine track where each row alternates direction
    /// and characters organically enter and exit off-frame before wrapping to the row above.
    ///
    /// - Tier 0 (Row 0): moves Right -> Left as u decreases
    /// - Tier 1 (Row 1): moves Left -> Right as u decreases
    /// - Tier 2 (Row 2): moves Right -> Left as u decreases
    /// - Tier 3 (Row 3): moves Left -> Right as u decreases
    #[inline]
    pub fn serpentine_track_pos_mental(
        u: i32,
        clip_min_x: i32,
        clip_max_x: i32,
        body_y: i32,
        line_spacing: i32,
    ) -> (i32, i32) {
        let char_w = 6; // 5px glyph + 1px spacing
        let x_left = clip_min_x - char_w;
        let x_right = clip_max_x;
        let leg_len = (x_right - x_left).max(1);

        let tier = u / leg_len;
        let rem = u % leg_len;

        let y = body_y + tier * line_spacing;
        let is_odd = (tier % 2) != 0;

        let x = if !is_odd {
            // Even tier (Row 0, 2...): moves Right -> Left as u decreases
            x_left + rem
        } else {
            // Odd tier (Row 1, 3...): moves Left -> Right as u decreases
            x_right - rem
        };

        (x, y)
    }

    /// Renders a headline flowing in continuous mental serpentine motion where letters
    /// slide organically into and out of the matrix frame (column-by-column clipping).
    pub fn render_serpentine_headline(
        matrix: &mut dyn IDrawingSurface,
        title_chars: &[char],
        scroll_offset: i32,
        _mw: i32,
        body_y: i32,
        clip_min_x: i32,
        clip_max_x: i32,
        clip_min_y: i32,
        clip_max_y: i32,
        line_spacing: i32,
    ) {
        let t_len = title_chars.len();
        if t_len == 0 {
            return;
        }

        let char_w = 6;
        let w = (clip_max_x - clip_min_x).max(1);
        let num_rows = if clip_max_y <= 32 {
            2
        } else {
            (((clip_max_y - body_y) / line_spacing).max(2)) as usize
        };

        let chars_per_row = ((w / char_w).max(1)) as usize;
        let needed_rows = (t_len + chars_per_row - 1) / chars_per_row;
        let active_rows = needed_rows.min(num_rows).max(1);

        let s = scroll_offset;
        let char_step = s.div_euclid(char_w);
        let pixel_shift = s.rem_euclid(char_w);

        for r in 0..active_rows {
            let cy = body_y + (r as i32 * line_spacing);
            if cy + 7 <= clip_min_y || cy >= clip_max_y {
                continue;
            }

            let row_base_char = (r * chars_per_row) as i32 + char_step;

            for col in 0..=(chars_per_row as i32) {
                let cx = clip_min_x + col * char_w - pixel_shift;
                if cx + 5 < clip_min_x || cx >= clip_max_x {
                    continue;
                }

                let char_idx = row_base_char + col;
                if char_idx >= 0 && (char_idx as usize) < t_len {
                    let c = title_chars[char_idx as usize];
                    if c != ' ' {
                        draw_char_clipped(
                            matrix,
                            c,
                            cx,
                            cy,
                            clip_min_x,
                            clip_max_x,
                            clip_min_y,
                            clip_max_y,
                            (255, 255, 255),
                        );
                    }
                }
            }
        }
    }

    fn apply_config(&mut self, config: &dyn EngineConfig) {
        self.api_key = config.get_string("api_key", "");
        self.category = config.get_string("category", "technology");
        self.keywords = config.get_string("keywords", "");
        self.lang = config.get_string("lang", "auto");
        self.country = config.get_string("country", "auto");
        self.max_articles = config.get_int("max_articles", 5).clamp(3, 15) as usize;
        self.articles_per_rotation =
            config.get_int("articles_per_rotation", 2).clamp(1, 15) as usize;
        self.cache_ttl_min = config.get_int("cache_ttl_min", 30).clamp(5, 120) as u64;
        self.requests_per_day = config.get_int("requests_per_day", 10).clamp(1, 100) as u32;
        self.force_refresh = config.get_bool("force_refresh", false);

        let prev_mode = self.display_mode.clone();
        let prev_title = self.show_title;
        let prev_desc = self.show_description;
        let prev_content = self.show_content;

        self.display_mode = config.get_string("display_mode", "smooth_scroll");
        self.show_title = config.get_bool("show_title", false);
        self.show_description = config.get_bool("show_description", false);
        self.show_content = config.get_bool("show_content", true);

        if self.display_mode != prev_mode
            || self.show_title != prev_title
            || self.show_description != prev_desc
            || self.show_content != prev_content
        {
            self.scroll_pixel_offset = 0;
            self.scroll_state = ScrollState::PauseStart;
            self.state_start = Instant::now();
            self.current_page_index = 0;
            self.total_pages = 1;
            self.last_page_switch = Instant::now();
            self.cached_display_lines.clear();
            self.cached_title_chars.clear();
            self.cached_max_scroll = 0;
            self.cached_article_index = usize::MAX;
            self.cached_article_title.clear();
            self.cached_display_mode.clear();
        }

        self.scroll_speed = config.get_int("scroll_speed", 3).clamp(1, 10) as u32;
        self.scroll_pause_start_ms =
            config.get_int("scroll_pause_start_ms", 1200).clamp(0, 4000) as u64;
        self.scroll_pause_end_ms =
            config.get_int("scroll_pause_end_ms", 1000).clamp(0, 4000) as u64;
        self.article_duration_sec = config.get_int("article_duration_sec", 12).clamp(5, 60) as u64;
        self.theme = config.get_string("theme", "category_dynamic");
        self.show_category_badge = config.get_bool("show_category_badge", true);
        self.show_source = config.get_bool("show_source", true);
        self.show_time_ago = config.get_bool("show_time_ago", true);
        self.show_beacon = config.get_bool("show_beacon", true);
        self.show_progress_dots = config.get_bool("show_progress_dots", true);
    }

    fn trigger_background_fetch(&mut self, system_lang: &str) {
        if self.is_fetching.load(Ordering::Relaxed) {
            return;
        }

        let effective_lang = if self.lang.is_empty() || self.lang == "auto" || self.lang == "system"
        {
            if system_lang.is_empty() {
                "fr".to_string()
            } else {
                system_lang.to_string()
            }
        } else {
            self.lang.clone()
        };

        let keys: Vec<String> = self
            .api_key
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if keys.is_empty() {
            tracing::warn!("[GNewsEngine] No API key configured. Please provide a GNews API key to receive live news.");
            self.status = GNewsStatus::EmptyKey;
            if let Ok(mut st) = self.shared_status.lock() {
                *st = GNewsStatus::EmptyKey;
            }
            self.last_fetch = Some(Instant::now());
            self.last_fetched_lang = effective_lang;
            return;
        }

        let shared = Arc::clone(&self.shared_articles);
        let shared_st = Arc::clone(&self.shared_status);
        let flag = Arc::clone(&self.is_fetching);
        flag.store(true, Ordering::Relaxed);

        if self.articles.is_empty() {
            self.status = GNewsStatus::Loading;
            if let Ok(mut st) = self.shared_status.lock() {
                *st = GNewsStatus::Loading;
            }
        }

        let categories: Vec<String> = self
            .category
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let target_category = if categories.is_empty() {
            "general".to_string()
        } else {
            let idx = self.cat_round_robin_idx % categories.len();
            self.cat_round_robin_idx = (self.cat_round_robin_idx + 1) % categories.len();
            categories[idx].clone()
        };

        let keywords = self.keywords.clone();
        let country = self.country.clone();
        let max_articles = self.max_articles;
        let req_lang = effective_lang.clone();
        let cur_cat_idx = self.cat_round_robin_idx;
        let mut active_k_idx = self.active_key_idx;
        let mut usages = self.key_usages.clone();

        thread::spawn(move || {
            let (status, fetched_opt) = crate::api::gnews::GNewsProvider::fetch_articles_multi_key(
                &keys,
                &mut active_k_idx,
                &mut usages,
                &target_category,
                &keywords,
                &req_lang,
                &country,
                max_articles,
            );

            if let Ok(mut st) = shared_st.lock() {
                *st = status;
            }

            let now_epoch = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            if let Some(fetched) = fetched_opt {
                if let Ok(mut guard) = shared.lock() {
                    let mut existing = guard.clone();
                    for a in fetched {
                        if let Some(existing_art) = existing.iter_mut().find(|x| x.title == a.title)
                        {
                            if existing_art.content.is_empty() && !a.content.is_empty() {
                                existing_art.content = a.content;
                            }
                            if existing_art.description.is_empty() && !a.description.is_empty() {
                                existing_art.description = a.description;
                            }
                        } else {
                            let badge_color = Self::get_category_color(&a.category);
                            existing.insert(
                                0,
                                GNewsArticle {
                                    title: a.title,
                                    description: a.description,
                                    content: a.content,
                                    source: a.source,
                                    category: a.category,
                                    published_epoch: a.published_epoch,
                                    badge_color,
                                },
                            );
                        }
                    }
                    existing.truncate(10);
                    *guard = existing.clone();

                    let fetched_save: Vec<crate::api::gnews::FetchedArticle> = existing
                        .iter()
                        .map(|art| crate::api::gnews::FetchedArticle {
                            title: art.title.clone(),
                            description: art.description.clone(),
                            content: art.content.clone(),
                            source: art.source.clone(),
                            category: art.category.clone(),
                            published_epoch: art.published_epoch,
                        })
                        .collect();
                    let st_num = match status {
                        GNewsStatus::InvalidKey => 2,
                        GNewsStatus::RateLimited => 3,
                        GNewsStatus::NetworkError => 4,
                        _ => 0,
                    };
                    crate::api::gnews::GNewsProvider::save_cache_file(
                        "gnews_cache.json",
                        &fetched_save,
                        now_epoch,
                        cur_cat_idx,
                        active_k_idx,
                        &usages,
                        st_num,
                    );
                }
            } else if let Ok(guard) = shared.lock() {
                let fetched_save: Vec<crate::api::gnews::FetchedArticle> = guard
                    .iter()
                    .map(|art| crate::api::gnews::FetchedArticle {
                        title: art.title.clone(),
                        description: art.description.clone(),
                        content: art.content.clone(),
                        source: art.source.clone(),
                        category: art.category.clone(),
                        published_epoch: art.published_epoch,
                    })
                    .collect();
                let st_num = match status {
                    GNewsStatus::InvalidKey => 2,
                    GNewsStatus::RateLimited => 3,
                    GNewsStatus::NetworkError => 4,
                    _ => 0,
                };
                crate::api::gnews::GNewsProvider::save_cache_file(
                    "gnews_cache.json",
                    &fetched_save,
                    now_epoch,
                    cur_cat_idx,
                    active_k_idx,
                    &usages,
                    st_num,
                );
            }
            flag.store(false, Ordering::Relaxed);
        });

        self.last_fetch = Some(Instant::now());
        self.last_fetched_lang = effective_lang;
    }

    fn pick_next_random_article(&mut self) {
        let n = self.articles.len();
        if n == 0 {
            self.current_index = 0;
            self.shuffled_order.clear();
            self.shuffle_pos = 0;
            return;
        }
        if n == 1 {
            self.current_index = 0;
            return;
        }
        if self.shuffled_order.len() != n || self.shuffle_pos >= self.shuffled_order.len() {
            use rand::seq::SliceRandom;
            let prev_idx = self.current_index;
            self.shuffled_order = (0..n).collect();
            self.shuffled_order.shuffle(&mut rand::thread_rng());
            if self.shuffled_order[0] == prev_idx && n > 1 {
                self.shuffled_order.swap(0, n - 1);
            }
            self.shuffle_pos = 0;
        }
        self.current_index = self.shuffled_order[self.shuffle_pos];
        self.shuffle_pos += 1;
    }

    fn advance_to_next_article(&mut self) {
        self.articles_shown_this_cycle = self.articles_shown_this_cycle.saturating_add(1);
        self.pick_next_random_article();
        let now = Instant::now();
        self.scroll_pixel_offset = 0;
        self.scroll_state = ScrollState::PauseStart;
        self.state_start = now;
        self.last_scroll_tick = now;
        self.last_article_switch = now;
        self.current_page_index = 0;
        self.total_pages = 1;
        self.last_page_switch = now;
        self.cached_display_lines.clear();
        self.cached_max_scroll = 0;
        self.cached_article_index = usize::MAX;
        self.cached_article_title.clear();
        self.cached_display_mode.clear();
    }

    fn fill_rect_util(
        matrix: &mut dyn IDrawingSurface,
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        color: (u8, u8, u8),
    ) {
        for dy in 0..h as i32 {
            for dx in 0..w as i32 {
                matrix.set_pixel(x + dx, y + dy, color.0, color.1, color.2);
            }
        }
    }

    fn draw_rect_util(
        matrix: &mut dyn IDrawingSurface,
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        color: (u8, u8, u8),
    ) {
        if w == 0 || h == 0 {
            return;
        }
        for dx in 0..w as i32 {
            matrix.set_pixel(x + dx, y, color.0, color.1, color.2);
            matrix.set_pixel(x + dx, y + h as i32 - 1, color.0, color.1, color.2);
        }
        for dy in 0..h as i32 {
            matrix.set_pixel(x, y + dy, color.0, color.1, color.2);
            matrix.set_pixel(x + w as i32 - 1, y + dy, color.0, color.1, color.2);
        }
    }

    fn draw_hline_util(
        matrix: &mut dyn IDrawingSurface,
        x: i32,
        y: i32,
        w: u32,
        color: (u8, u8, u8),
    ) {
        for dx in 0..w as i32 {
            matrix.set_pixel(x + dx, y, color.0, color.1, color.2);
        }
    }
}

impl Engine for GNewsEngine {
    fn initialize(
        &mut self,
        _ctx: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        self.apply_config(config);
        Ok(())
    }

    fn activate(&mut self) {
        self.articles_shown_this_cycle = 0;
        self.pick_next_random_article();
        self.scroll_pixel_offset = 0;
        self.source_marquee_offset = 0;
        self.scroll_state = ScrollState::PauseStart;
        let now = Instant::now();
        self.activate_time = now;
        self.state_start = now;
        self.last_update = now;
        self.last_scroll_tick = now;
        self.last_source_tick = now;
        self.last_article_switch = now;
        self.current_page_index = 0;
        self.total_pages = 1;
        self.last_page_switch = now;
        self.cached_display_lines.clear();
        self.cached_max_scroll = 0;
        self.cached_article_index = usize::MAX;
        self.cached_article_title.clear();
        self.cached_display_mode.clear();
    }

    fn self_paced(&self) -> bool {
        true
    }

    fn is_finished(&self) -> bool {
        if self.articles.is_empty() {
            return self.activate_time.elapsed()
                >= Duration::from_secs(self.article_duration_sec.max(5));
        }
        self.articles_shown_this_cycle >= self.articles_per_rotation.max(1)
    }

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        let prev_force = self.force_refresh;
        self.apply_config(config);
        if self.force_refresh && !prev_force {
            // Force refresh requested -> clear stale articles and trigger immediate fetch
            self.articles.clear();
            self.shuffled_order.clear();
            self.shuffle_pos = 0;
            if let Ok(mut g) = self.shared_articles.lock() {
                g.clear();
            }
            self.last_fetch = None;
            self.last_fetched_lang.clear();
        }
        // If force_refresh is false, changes will automatically take effect at the next scheduled cycle
    }

    fn on_display_geometry_changed(&mut self, geometry: &DisplayGeometry) {
        self.geometry = *geometry;
        self.cached_display_lines.clear();
        self.cached_max_scroll = 0;
        self.cached_article_index = usize::MAX;
        self.cached_article_title.clear();
        self.cached_display_mode.clear();
    }

    fn is_realtime(&self) -> bool {
        true
    }

    fn update(&mut self, ctx: &mut EngineContext) {
        let now = Instant::now();
        self.last_update = now;

        // 1. Ingest freshly fetched background articles & status without locking main thread
        if let Ok(guard) = self.shared_status.try_lock() {
            self.status = *guard;
        }
        let fresh_articles = if let Ok(guard) = self.shared_articles.try_lock() {
            if !guard.is_empty()
                && (self.articles.is_empty()
                    || guard.len() != self.articles.len()
                    || guard[0].title != self.articles[0].title
                    || guard[0].content != self.articles[0].content)
            {
                Some(guard.clone())
            } else {
                None
            }
        } else {
            None
        };
        if let Some(new_articles) = fresh_articles {
            let was_empty = self.articles.is_empty();
            self.articles = new_articles;
            self.shuffled_order.clear();
            self.shuffle_pos = 0;
            if was_empty || self.current_index >= self.articles.len() {
                self.pick_next_random_article();
            }
            self.cached_display_lines.clear();
            self.cached_max_scroll = 0;
            self.cached_article_index = usize::MAX;
            self.cached_article_title.clear();
            self.cached_display_mode.clear();
        }

        // 2. Midnight quota reset detector (00:00 UTC / 12:00 AM UTC daily rollover)
        let now_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let cur_day = (now_epoch / 86400) as u32;
        if cur_day != self.last_fetch_day {
            tracing::info!(
                "[GNewsEngine] UTC Midnight rollover detected (day {} -> {}). Resetting daily quota counters.",
                self.last_fetch_day,
                cur_day
            );
            self.last_fetch_day = cur_day;
            for u in &mut self.key_usages {
                *u = 0;
            }
            if self.status == GNewsStatus::RateLimited {
                self.status = GNewsStatus::Ok;
                if let Ok(mut st) = self.shared_status.lock() {
                    *st = GNewsStatus::Ok;
                }
            }
        }

        // 3. Background periodic fetch trigger based on daily request budget
        let sys_lang = ctx.config.settings.read().system.lang.clone();
        let effective_lang = if self.lang.is_empty() || self.lang == "auto" || self.lang == "system"
        {
            if sys_lang.is_empty() {
                "fr".to_string()
            } else {
                sys_lang.clone()
            }
        } else {
            self.lang.clone()
        };

        let interval_sec = 86400 / self.requests_per_day.clamp(1, 100) as u64;
        let lang_changed =
            !self.last_fetched_lang.is_empty() && self.last_fetched_lang != effective_lang;
        let should_fetch = lang_changed
            || self.last_fetch.map_or(true, |t| {
                t.elapsed() >= std::time::Duration::from_secs(interval_sec)
            });
        if should_fetch {
            self.trigger_background_fetch(&sys_lang);
        }

        if self.articles.is_empty() {
            return;
        }
        if self.current_index >= self.articles.len() {
            self.current_index = 0;
        }

        // 4. Smooth discrete source marquee advancement (~35ms per pixel)
        let elapsed_src = self.last_source_tick.elapsed().as_millis() as u64;
        if elapsed_src >= 35 {
            let steps = (elapsed_src / 35) as u32;
            self.source_marquee_offset = self.source_marquee_offset.wrapping_add(steps);
            self.last_source_tick += Duration::from_millis(steps as u64 * 35);
        }

        let article = &self.articles[self.current_index];
        let headline =
            article.compose_text(self.show_title, self.show_description, self.show_content);
        let mw = ctx.surface.width();
        let mh = ctx.surface.height();
        let is_vertical = mh > mw || mw < 48 || mh > (mw * 3) / 2;

        let cache_invalid = self.cached_article_index != self.current_index
            || self.cached_article_title != headline
            || self.cached_display_mode != self.display_mode;

        if cache_invalid {
            self.cached_title_chars = headline.chars().collect();
            let mode = self.display_mode.as_str();
            let line_spacing: i32 = 9;

            if is_vertical {
                let body_y: i32 = 24;
                let num_rows = (((mh as i32) - body_y) / line_spacing).max(1) as usize;
                let max_w = (mw as i32) - 4;
                let viewport_h = ((mh as i32) - body_y).max(10);

                match mode {
                    "serpentine" => {
                        let t_len = self.cached_title_chars.len();
                        let avail_w = ((mw as i32) - 4).max(1) as usize;
                        let chars_per_row = (avail_w / 6).max(1);
                        let needed_rows = if t_len == 0 {
                            1
                        } else {
                            (t_len + chars_per_row - 1) / chars_per_row
                        };
                        if needed_rows <= num_rows {
                            self.cached_max_scroll = ((t_len as i32) * 6).max(6);
                        } else {
                            let overflow = (t_len - (num_rows * chars_per_row) + 3) as i32;
                            self.cached_max_scroll = overflow * 6;
                        }
                        self.cached_display_lines.clear();
                    }
                    "vertical_crawl" => {
                        let lines = Self::wrap_text_to_lines(&headline, max_w);
                        let total_h = lines.len() as i32 * line_spacing;
                        self.cached_max_scroll = if total_h > viewport_h {
                            (total_h - viewport_h) + 12
                        } else {
                            (total_h + 4).max(6)
                        };
                        self.cached_display_lines = lines;
                    }
                    "static_paged" => {
                        let lines = Self::wrap_text_to_lines(&headline, max_w);
                        self.cached_max_scroll = 0;
                        self.total_pages = ((lines.len() + num_rows - 1) / num_rows.max(1)).max(1);
                        self.current_page_index = 0;
                        self.last_page_switch = now;
                        self.cached_display_lines = lines;
                    }
                    _ => {
                        // Default: "smooth_scroll"
                        // Multi-line block filling entire height, scrolling horizontally Right -> Left
                        let lines = Self::distribute_text_to_rows(&headline, num_rows);
                        let max_line_w = lines.iter().map(|l| measure_text(l)).max().unwrap_or(0);
                        self.cached_max_scroll = (mw as i32) + max_line_w + 12;
                        self.cached_display_lines = lines;
                    }
                }
            } else {
                // Horizontal display
                let (_body_y, _clip_max_y, num_rows) = if mh >= 64 {
                    (19, mh as i32, 4usize)
                } else {
                    (13, mh as i32, 2usize)
                };

                match mode {
                    "serpentine" => {
                        let t_len = self.cached_title_chars.len();
                        let avail_w = ((mw as i32) - 4).max(1) as usize;
                        let chars_per_row = (avail_w / 6).max(1);
                        let needed_rows = if t_len == 0 {
                            1
                        } else {
                            (t_len + chars_per_row - 1) / chars_per_row
                        };
                        if needed_rows <= num_rows {
                            self.cached_max_scroll = ((t_len as i32) * 6).max(6);
                        } else {
                            let overflow = (t_len - (num_rows * chars_per_row) + 3) as i32;
                            self.cached_max_scroll = overflow * 6;
                        }
                        self.cached_display_lines.clear();
                    }
                    "vertical_crawl" => {
                        let max_w = (mw as i32) - 8;
                        let lines = Self::wrap_text_to_lines(&headline, max_w);
                        let total_h = lines.len() as i32 * line_spacing;
                        let viewport_h = ((mh as i32) - 16).max(10);
                        self.cached_max_scroll = if total_h > viewport_h {
                            (total_h - viewport_h) + 12
                        } else {
                            (total_h + 4).max(6)
                        };
                        self.cached_display_lines = lines;
                    }
                    "static_paged" => {
                        let max_w = (mw as i32) - 8;
                        let lines = Self::wrap_text_to_lines(&headline, max_w);
                        self.cached_max_scroll = 0;
                        self.total_pages = ((lines.len() + num_rows - 1) / num_rows.max(1)).max(1);
                        self.current_page_index = 0;
                        self.last_page_switch = now;
                        self.cached_display_lines = lines;
                    }
                    _ => {
                        // "smooth_scroll" on horizontal display
                        let scale = if mh >= 64 && mw >= 256 { 2 } else { 1 };
                        let text_w = measure_text(&headline) * scale;
                        self.cached_max_scroll = text_w + 16;
                        self.cached_display_lines.clear();
                    }
                }
            }

            self.cached_article_index = self.current_index;
            self.cached_article_title = headline;
            self.cached_display_mode = self.display_mode.clone();
        }

        let max_scroll = self.cached_max_scroll;
        let base_tick_ms: u64 = match self.scroll_speed {
            1 => 100,
            2 => 75,
            3 => 55,
            4 => 42,
            5 => 32,
            6 => 24,
            7 => 18,
            8 => 13,
            9 => 9,
            _ => 6,
        };

        if self.display_mode == "static_paged" {
            let base_page_ms = ((self.article_duration_sec * 1000)
                / self.total_pages.max(1) as u64)
                .clamp(2000, 12000);
            let page_dur_ms = ((base_page_ms * base_tick_ms) / 55).clamp(1000, 20000);
            if now.duration_since(self.last_page_switch).as_millis() as u64 >= page_dur_ms {
                self.last_page_switch = now;
                if self.current_page_index + 1 < self.total_pages {
                    self.current_page_index += 1;
                } else {
                    self.advance_to_next_article();
                }
            }
        } else if max_scroll == 0 {
            if now.duration_since(self.last_article_switch).as_secs() >= self.article_duration_sec {
                self.advance_to_next_article();
            }
        } else {
            let tick_ms = if self.display_mode == "vertical_crawl" {
                base_tick_ms * 4
            } else {
                base_tick_ms
            };

            match self.scroll_state {
                ScrollState::PauseStart => {
                    if now.duration_since(self.state_start).as_millis() as u64
                        >= self.scroll_pause_start_ms
                    {
                        self.scroll_state = ScrollState::Scrolling;
                        self.state_start = now;
                        self.last_scroll_tick = now;
                    }
                }
                ScrollState::Scrolling => {
                    let elapsed_scroll = self.last_scroll_tick.elapsed().as_millis() as u64;
                    if elapsed_scroll >= tick_ms {
                        let steps = (elapsed_scroll / tick_ms) as i32;
                        self.scroll_pixel_offset += steps;
                        self.last_scroll_tick += Duration::from_millis(steps as u64 * tick_ms);
                        if self.scroll_pixel_offset >= max_scroll {
                            self.scroll_state = ScrollState::PauseEnd;
                            self.state_start = now;
                        }
                    }
                }
                ScrollState::PauseEnd => {
                    if now.duration_since(self.state_start).as_millis() as u64
                        >= self.scroll_pause_end_ms
                    {
                        self.advance_to_next_article();
                    }
                }
            }
        }
    }

    fn render(&mut self, ctx: &mut EngineContext) {
        let mw = ctx.surface.width();
        let mh = ctx.surface.height();
        ctx.surface.clear();

        if self.articles.is_empty() {
            let sys_lang = ctx.config.settings.read().system.lang.clone();
            let lang_enum = Lang::from_code(&sys_lang);
            let msg = i18n::gnews_status_label(lang_enum, self.status);
            let color = match self.status {
                GNewsStatus::InvalidKey => (255, 50, 50),
                GNewsStatus::EmptyKey => (255, 170, 0),
                GNewsStatus::RateLimited | GNewsStatus::NetworkError => (255, 140, 0),
                _ => (0, 229, 255),
            };

            let matrix = &mut *ctx.surface;
            let text_w = measure_text(msg);
            let x = ((mw as i32) - text_w) / 2;
            let y = ((mh as i32) - 7) / 2;
            draw_text_clipped(matrix, msg, x.max(2), y, 0, mw as i32, 0, mh as i32, color);

            if self.show_beacon {
                let beacon_pulse = ((self.source_marquee_offset as f32 * 0.1).sin() + 1.0) * 0.5;
                let br = (beacon_pulse * 255.0) as u8;
                let b_col = if self.status == GNewsStatus::InvalidKey {
                    (br, 20, 20)
                } else {
                    (br, 30, 30)
                };
                let bx = (mw as i32) - 6;
                let by = (mh as i32) / 2;
                matrix.set_pixel(bx, by, b_col.0, b_col.1, b_col.2);
                matrix.set_pixel(bx + 1, by, b_col.0, b_col.1, b_col.2);
                matrix.set_pixel(bx, by + 1, b_col.0, b_col.1, b_col.2);
                matrix.set_pixel(bx + 1, by + 1, b_col.0, b_col.1, b_col.2);
            }
            return;
        }

        if self.current_index >= self.articles.len() {
            self.current_index = 0;
        }
        let article = self.articles[self.current_index].clone();
        let total_count = self.articles.len();

        let beacon_pulse = ((self.source_marquee_offset as f32 * 0.1).sin() + 1.0) * 0.5;

        let mut cat_color = article.badge_color;
        if self.theme == "breaking_crimson" {
            cat_color = (255, 42, 77);
        } else if self.theme == "cyberpunk" {
            cat_color = (0, 229, 255);
        } else if self.theme == "monochrome_paper" {
            cat_color = (224, 230, 237);
        }

        let matrix = &mut *ctx.surface;
        let is_vertical = mh > mw || mw < 48 || mh > (mw * 3) / 2;

        if is_vertical {
            // ==========================================
            // Portrait / Tate Layout (Matching ESP32)
            // ==========================================
            // 1. Category Tag (Top)
            let cat_short = Self::get_category_short(&article.category);
            draw_text_clipped(
                matrix, cat_short, 2, 2, 0, mw as i32, 0, mh as i32, cat_color,
            );

            // 2. Pulsing Live Beacon (Top Right)
            if self.show_beacon {
                let br = 120 + (beacon_pulse * 135.0) as u8;
                Self::fill_rect_util(matrix, (mw as i32) - 5, 4, 3, 3, (br, 20, 20));
            }

            // 3. Divider line
            Self::draw_hline_util(matrix, 2, 11, mw.saturating_sub(4), (40, 45, 55));

            // 4. News Source Name (Scrolling if longer than width)
            if self.show_source {
                let src_w = measure_text(&article.source);
                let avail_w = (mw as i32) - 4;
                if src_w <= avail_w {
                    draw_text_clipped(
                        matrix,
                        &article.source,
                        2,
                        14,
                        0,
                        mw as i32,
                        0,
                        mh as i32,
                        (160, 175, 195),
                    );
                } else {
                    let gap = 16;
                    let total_src_w = src_w + gap;
                    let dx = (self.source_marquee_offset as i32).rem_euclid(total_src_w);
                    let draw_x1 = 2 - dx;
                    let clip_max_x = mw as i32 - 2;

                    draw_text_clipped(
                        matrix,
                        &article.source,
                        draw_x1,
                        14,
                        2,
                        clip_max_x,
                        0,
                        mh as i32,
                        (160, 175, 195),
                    );
                    let draw_x2 = draw_x1 + total_src_w;
                    if draw_x2 < clip_max_x {
                        draw_text_clipped(
                            matrix,
                            &article.source,
                            draw_x2,
                            14,
                            2,
                            clip_max_x,
                            0,
                            mh as i32,
                            (160, 175, 195),
                        );
                    }
                }
            }

            // 5. Headline rendering based on display_mode
            let div_y = if mh >= 64 { 16 } else { 11 };
            let body_y: i32 = if mh >= 64 { 19 } else { 13 };
            let line_spacing: i32 = if mh >= 64 { 10 } else { 9 };
            let clip_min_x: i32 = 0;
            let clip_max_x: i32 = mw as i32;
            let clip_min_y: i32 = div_y + 1;
            let clip_max_y: i32 = mh as i32;

            match self.display_mode.as_str() {
                "serpentine" => {
                    Self::render_serpentine_headline(
                        matrix,
                        &self.cached_title_chars,
                        self.scroll_pixel_offset,
                        mw as i32,
                        body_y,
                        clip_min_x,
                        clip_max_x,
                        clip_min_y,
                        clip_max_y,
                        line_spacing,
                    );
                }
                "vertical_crawl" => {
                    let inner_min_x = 2;
                    let inner_max_x = (mw as i32) - 2;
                    for (idx, line) in self.cached_display_lines.iter().enumerate() {
                        let y = body_y + (idx as i32 * line_spacing) - self.scroll_pixel_offset;
                        if y + line_spacing > clip_min_y && y < clip_max_y {
                            draw_text_clipped(
                                matrix,
                                line,
                                inner_min_x,
                                y,
                                inner_min_x,
                                inner_max_x,
                                clip_min_y,
                                clip_max_y,
                                (255, 255, 255),
                            );
                        }
                    }
                }
                "static_paged" => {
                    let inner_min_x = 2;
                    let inner_max_x = (mw as i32) - 2;
                    let lines_per_page = (((mh as i32) - body_y) / line_spacing).max(1) as usize;
                    let start_idx = self.current_page_index * lines_per_page;
                    for (row, line) in self
                        .cached_display_lines
                        .iter()
                        .skip(start_idx)
                        .take(lines_per_page)
                        .enumerate()
                    {
                        let y = body_y + (row as i32 * line_spacing);
                        if y + line_spacing > clip_min_y && y < clip_max_y {
                            draw_text_clipped(
                                matrix,
                                line,
                                inner_min_x,
                                y,
                                inner_min_x,
                                inner_max_x,
                                clip_min_y,
                                clip_max_y,
                                (255, 255, 255),
                            );
                        }
                    }
                }
                _ => {
                    // Default: "smooth_scroll"
                    // Multi-line block filling entire height, scrolling horizontally Right -> Left
                    let start_x = (mw as i32) - self.scroll_pixel_offset;
                    for (idx, line) in self.cached_display_lines.iter().enumerate() {
                        let y = body_y + (idx as i32 * line_spacing);
                        if y + line_spacing > clip_min_y && y < clip_max_y {
                            draw_text_clipped(
                                matrix,
                                line,
                                start_x,
                                y,
                                clip_min_x,
                                clip_max_x,
                                clip_min_y,
                                clip_max_y,
                                (255, 255, 255),
                            );
                        }
                    }
                }
            }
        } else if mw >= 128 {
            // ==========================================
            // Wide Layout (128x32, 256x64)
            // ==========================================
            let mut cur_x: i32 = 4;
            let header_y: i32 = if mh >= 64 { 4 } else { 2 };

            // 1. Live Pulsing Beacon
            if self.show_beacon {
                let br = (120.0 + beacon_pulse * 135.0) as u8;
                Self::fill_rect_util(matrix, cur_x, header_y + 2, 3, 3, (br, 15, 25));
                cur_x += 7;
            }

            // 2. Compact Category Pill Badge (e.g. "TECH", "SCI", "NEWS")
            if self.show_category_badge {
                let cat_short = Self::get_category_short(&article.category);
                let cat_text_w = measure_text(cat_short);
                let cat_w = (cat_text_w + 5) as u32;
                Self::fill_rect_util(matrix, cur_x, header_y, cat_w, 9, (20, 25, 35));
                Self::draw_rect_util(matrix, cur_x, header_y, cat_w, 9, cat_color);
                draw_text_clipped(
                    matrix,
                    cat_short,
                    cur_x + 3,
                    header_y + 1,
                    0,
                    mw as i32,
                    0,
                    mh as i32,
                    cat_color,
                );
                cur_x += (cat_w + 5) as i32;
            }

            // Progress Dots reservation (compact 5px per dot)
            let dots_count = total_count.min(6);
            let dots_start_x = if self.show_progress_dots && total_count > 1 {
                (mw as i32) - ((dots_count * 5 + 3) as i32)
            } else {
                (mw as i32) - 3
            };

            // 3. News Source Name with Marquee if long (generous ~60-80px window!)
            if self.show_source {
                let max_src_w = (dots_start_x - cur_x - 3).max(20);
                let src_w = measure_text(&article.source);

                if src_w <= max_src_w {
                    draw_text_clipped(
                        matrix,
                        &article.source,
                        cur_x,
                        header_y + 1,
                        cur_x,
                        dots_start_x,
                        0,
                        mh as i32,
                        (200, 210, 225),
                    );
                    cur_x += (src_w + 5) as i32;
                } else {
                    let gap = 16;
                    let total_src_w = src_w + gap;
                    let dx = (self.source_marquee_offset as i32).rem_euclid(total_src_w);
                    let draw_x1 = cur_x - dx;
                    let clip_max_x = dots_start_x.min(mw as i32 - 2);

                    draw_text_clipped(
                        matrix,
                        &article.source,
                        draw_x1,
                        header_y + 1,
                        cur_x,
                        clip_max_x,
                        0,
                        mh as i32,
                        (200, 210, 225),
                    );
                    let draw_x2 = draw_x1 + total_src_w;
                    if draw_x2 < clip_max_x {
                        draw_text_clipped(
                            matrix,
                            &article.source,
                            draw_x2,
                            header_y + 1,
                            cur_x,
                            clip_max_x,
                            0,
                            mh as i32,
                            (200, 210, 225),
                        );
                    }
                    cur_x = dots_start_x;
                }
            }

            // 4. Progress Dots
            if self.show_progress_dots && total_count > 1 {
                if dots_start_x > cur_x {
                    for i in 0..dots_count {
                        let dx = dots_start_x + (i * 5) as i32;
                        if i == self.current_index {
                            Self::fill_rect_util(matrix, dx, header_y + 3, 3, 3, cat_color);
                        } else {
                            Self::draw_rect_util(matrix, dx, header_y + 3, 3, 3, (70, 75, 85));
                        }
                    }
                }
            }

            // Divider Line
            let div_y = if mh >= 64 { 16 } else { 12 };
            Self::draw_hline_util(matrix, 2, div_y, mw.saturating_sub(4), (40, 45, 55));

            // Headline Content Area
            let body_y = div_y + if mh >= 64 { 8 } else { 4 };
            if self.display_mode == "vertical_crawl" {
                for (idx, line) in self.cached_display_lines.iter().enumerate() {
                    let y = body_y + (idx as i32 * 9) - self.scroll_pixel_offset;
                    if y + 9 > div_y && y < mh as i32 {
                        draw_text_clipped(
                            matrix,
                            line,
                            4,
                            y,
                            4,
                            mw as i32 - 4,
                            div_y + 1,
                            mh as i32,
                            (255, 255, 255),
                        );
                    }
                }
            } else if self.display_mode == "static_paged" {
                let lines_per_page = if mh >= 64 { 4usize } else { 2usize };
                let start_idx = self.current_page_index * lines_per_page;
                for (row, line) in self
                    .cached_display_lines
                    .iter()
                    .skip(start_idx)
                    .take(lines_per_page)
                    .enumerate()
                {
                    let y = body_y + (row as i32 * 9);
                    if y + 9 > div_y && y < mh as i32 {
                        draw_text_clipped(
                            matrix,
                            line,
                            4,
                            y,
                            4,
                            mw as i32 - 4,
                            div_y + 1,
                            mh as i32,
                            (255, 255, 255),
                        );
                    }
                }
            } else if self.display_mode == "serpentine" {
                Self::render_serpentine_headline(
                    matrix,
                    &self.cached_title_chars,
                    self.scroll_pixel_offset,
                    mw as i32,
                    body_y,
                    0,
                    mw as i32,
                    div_y + 1,
                    mh as i32,
                    9,
                );
            } else {
                let start_x = 4 - self.scroll_pixel_offset;
                if mh >= 64 && mw >= 256 {
                    draw_text_scaled(
                        matrix,
                        &self.cached_article_title,
                        start_x,
                        body_y,
                        0,
                        mw as i32,
                        0,
                        mh as i32,
                        2,
                        (255, 255, 255),
                    );
                } else {
                    draw_text_clipped(
                        matrix,
                        &self.cached_article_title,
                        start_x,
                        body_y,
                        0,
                        mw as i32,
                        0,
                        mh as i32,
                        (255, 255, 255),
                    );
                }
            }
        } else {
            // Compact Layout (64x32)
            if self.show_beacon {
                let br = (120.0 + beacon_pulse * 135.0) as u8;
                Self::fill_rect_util(matrix, 2, 2, 3, 3, (br, 20, 20));
            }

            let cat_short = Self::get_category_short(&article.category);
            draw_text_clipped(
                matrix, cat_short, 8, 1, 0, mw as i32, 0, mh as i32, cat_color,
            );

            let idx_str = format!("{}/{}", self.current_index + 1, total_count);
            let idx_x = (mw as i32).saturating_sub(18);
            draw_text_clipped(
                matrix,
                &idx_str,
                idx_x,
                1,
                0,
                mw as i32,
                0,
                mh as i32,
                (140, 150, 160),
            );

            Self::draw_hline_util(matrix, 0, 10, mw, (35, 40, 50));

            if self.display_mode == "vertical_crawl" {
                for (idx, line) in self.cached_display_lines.iter().enumerate() {
                    let y = 14 + (idx as i32 * 9) - self.scroll_pixel_offset;
                    if y + 9 > 10 && y < mh as i32 {
                        draw_text_clipped(
                            matrix,
                            line,
                            2,
                            y,
                            2,
                            mw as i32 - 2,
                            11,
                            mh as i32,
                            (255, 255, 255),
                        );
                    }
                }
            } else if self.display_mode == "static_paged" {
                let lines_per_page = if mh >= 64 { 4usize } else { 2usize };
                let start_idx = self.current_page_index * lines_per_page;
                for (row, line) in self
                    .cached_display_lines
                    .iter()
                    .skip(start_idx)
                    .take(lines_per_page)
                    .enumerate()
                {
                    let y = 14 + (row as i32 * 9);
                    if y + 9 > 10 && y < mh as i32 {
                        draw_text_clipped(
                            matrix,
                            line,
                            2,
                            y,
                            2,
                            mw as i32 - 2,
                            11,
                            mh as i32,
                            (255, 255, 255),
                        );
                    }
                }
            } else if self.display_mode == "serpentine" {
                Self::render_serpentine_headline(
                    matrix,
                    &self.cached_title_chars,
                    self.scroll_pixel_offset,
                    mw as i32,
                    13,
                    0,
                    mw as i32,
                    11,
                    mh as i32,
                    9,
                );
            } else {
                let start_x = 2 - self.scroll_pixel_offset;
                draw_text_clipped(
                    matrix,
                    &self.cached_article_title,
                    start_x,
                    15,
                    0,
                    mw as i32,
                    0,
                    mh as i32,
                    (255, 255, 255),
                );
            }
        }
    }

    fn deactivate(&mut self) {}
}

#[distributed_slice(crate::core::registry::ENGINES)]
fn register_gnews_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "gnews",
            name: "GNews Live Feed",
            category: "news",
            version: crate::core::build_info::VERSION,
        },
        capabilities: Capabilities {
            supports_128x32: true,
            supports_256x64: true,
            realtime: true,
            interruptible: true,
            allows_overlay: true,
            allow_rotation: true,
        },
        requirements: Requirements {
            needs_audio: false,
            needs_network: true,
            needs_sd: false,
        },
        available: true,
        unavailable_reason: None,
        schema: ConfigSchema {
            fields: vec![
                ConfigField {
                    id: "api_key",
                    field_type: ConfigType::String,
                    label: "API Key",
                    description: "GNews.io API key (uses demo articles if empty)",
                    default_value: "",
                    validation_policy: ValidationPolicy::Accept,
                    ..Default::default()
                },
                ConfigField {
                    id: "category",
                    field_type: ConfigType::Options,
                    label: "Category",
                    description: "News topic category",
                    default_value: "technology",
                    options: Some(vec![
                        ConfigOption {
                            label: "General",
                            value: "general",
                        },
                        ConfigOption {
                            label: "World",
                            value: "world",
                        },
                        ConfigOption {
                            label: "Nation",
                            value: "nation",
                        },
                        ConfigOption {
                            label: "Business",
                            value: "business",
                        },
                        ConfigOption {
                            label: "Technology",
                            value: "technology",
                        },
                        ConfigOption {
                            label: "Entertainment",
                            value: "entertainment",
                        },
                        ConfigOption {
                            label: "Sports",
                            value: "sports",
                        },
                        ConfigOption {
                            label: "Science",
                            value: "science",
                        },
                        ConfigOption {
                            label: "Health",
                            value: "health",
                        },
                    ]),
                    multiple: true,
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "keywords",
                    field_type: ConfigType::String,
                    label: "Keywords",
                    description: "Custom search keywords or filter tags",
                    default_value: "",
                    validation_policy: ValidationPolicy::Accept,
                    ..Default::default()
                },
                ConfigField {
                    id: "lang",
                    field_type: ConfigType::Options,
                    label: "Language",
                    description: "Article language (auto matches system)",
                    default_value: "auto",
                    options: Some(vec![
                        ConfigOption {
                            label: "System Default (Auto)",
                            value: "auto",
                        },
                        ConfigOption {
                            label: "English (en)",
                            value: "en",
                        },
                        ConfigOption {
                            label: "Français (fr)",
                            value: "fr",
                        },
                        ConfigOption {
                            label: "Español (es)",
                            value: "es",
                        },
                        ConfigOption {
                            label: "Deutsch (de)",
                            value: "de",
                        },
                        ConfigOption {
                            label: "Italiano (it)",
                            value: "it",
                        },
                        ConfigOption {
                            label: "Português (pt)",
                            value: "pt",
                        },
                        ConfigOption {
                            label: "Nederlands (nl)",
                            value: "nl",
                        },
                        ConfigOption {
                            label: "Русский (ru)",
                            value: "ru",
                        },
                        ConfigOption {
                            label: "中文 (zh)",
                            value: "zh",
                        },
                        ConfigOption {
                            label: "日本語 (ja)",
                            value: "ja",
                        },
                    ]),
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "country",
                    field_type: ConfigType::Options,
                    label: "Country",
                    description: "Country edition",
                    default_value: "auto",
                    options: Some(vec![
                        ConfigOption {
                            label: "Local / Auto",
                            value: "auto",
                        },
                        ConfigOption {
                            label: "United States (us)",
                            value: "us",
                        },
                        ConfigOption {
                            label: "France (fr)",
                            value: "fr",
                        },
                        ConfigOption {
                            label: "United Kingdom (gb)",
                            value: "gb",
                        },
                        ConfigOption {
                            label: "Spain (es)",
                            value: "es",
                        },
                        ConfigOption {
                            label: "Germany (de)",
                            value: "de",
                        },
                        ConfigOption {
                            label: "Canada (ca)",
                            value: "ca",
                        },
                        ConfigOption {
                            label: "Italy (it)",
                            value: "it",
                        },
                        ConfigOption {
                            label: "Japan (jp)",
                            value: "jp",
                        },
                    ]),
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "max_articles",
                    field_type: ConfigType::Integer,
                    label: "Max Articles",
                    description: "Headlines count per cycle",
                    default_value: "5",
                    min_val: Some("3"),
                    max_val: Some("15"),
                    validation_policy: ValidationPolicy::Clamp,
                    ..Default::default()
                },
                ConfigField {
                    id: "articles_per_rotation",
                    field_type: ConfigType::Integer,
                    label: "Articles per Rotation",
                    description: "Number of random articles to scroll before rotating to the next screen (overrides slot timer)",
                    default_value: "2",
                    min_val: Some("1"),
                    max_val: Some("15"),
                    validation_policy: ValidationPolicy::Clamp,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_title",
                    field_type: ConfigType::Boolean,
                    label: "Show Title",
                    description: "Include article title in the scrolling feed",
                    default_value: "false",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_description",
                    field_type: ConfigType::Boolean,
                    label: "Show Description",
                    description: "Include article summary description in the scrolling feed",
                    default_value: "false",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_content",
                    field_type: ConfigType::Boolean,
                    label: "Show Content",
                    description: "Include full article body content in the scrolling feed",
                    default_value: "true",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "requests_per_day",
                    field_type: ConfigType::Integer,
                    label: "Daily Requests Budget",
                    description: "Total API requests per 24 hours (Free tier: max 100)",
                    default_value: "10",
                    min_val: Some("1"),
                    max_val: Some("100"),
                    validation_policy: ValidationPolicy::Clamp,
                    ..Default::default()
                },
                ConfigField {
                    id: "force_refresh",
                    field_type: ConfigType::Boolean,
                    label: "Force Refresh Now",
                    description: "Purge cached news of obsolete language and immediately query API",
                    default_value: "false",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "cache_ttl_min",
                    field_type: ConfigType::Integer,
                    label: "Cache TTL (min)",
                    description: "Minutes between fresh API requests",
                    default_value: "30",
                    min_val: Some("5"),
                    max_val: Some("120"),
                    validation_policy: ValidationPolicy::Clamp,
                    ..Default::default()
                },
                ConfigField {
                    id: "display_mode",
                    field_type: ConfigType::Options,
                    label: "Display Mode",
                    description: "Animation style",
                    default_value: "smooth_scroll",
                    options: Some(vec![
                        ConfigOption {
                            label: "Smooth Horizontal Scroll",
                            value: "smooth_scroll",
                        },
                        ConfigOption {
                            label: "Serpentine Snake Flow",
                            value: "serpentine",
                        },
                        ConfigOption {
                            label: "Vertical Crawl",
                            value: "vertical_crawl",
                        },
                        ConfigOption {
                            label: "Static Word-Wrapped Paging",
                            value: "static_paged",
                        },
                    ]),
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "scroll_speed",
                    field_type: ConfigType::Integer,
                    label: "Scroll Speed",
                    description: "Ticker speed (1=Slow, 10=Fast)",
                    default_value: "3",
                    min_val: Some("1"),
                    max_val: Some("10"),
                    validation_policy: ValidationPolicy::Clamp,
                    ..Default::default()
                },
                ConfigField {
                    id: "scroll_pause_start_ms",
                    field_type: ConfigType::Integer,
                    label: "Start Pause (ms)",
                    description: "Initial dwell before scrolling",
                    default_value: "1200",
                    min_val: Some("0"),
                    max_val: Some("4000"),
                    validation_policy: ValidationPolicy::Clamp,
                    ..Default::default()
                },
                ConfigField {
                    id: "scroll_pause_end_ms",
                    field_type: ConfigType::Integer,
                    label: "End Pause (ms)",
                    description: "End dwell before switching",
                    default_value: "1000",
                    min_val: Some("0"),
                    max_val: Some("4000"),
                    validation_policy: ValidationPolicy::Clamp,
                    ..Default::default()
                },
                ConfigField {
                    id: "article_duration_sec",
                    field_type: ConfigType::Integer,
                    label: "Article Duration (s)",
                    description: "Seconds per article in paged mode",
                    default_value: "12",
                    min_val: Some("5"),
                    max_val: Some("60"),
                    validation_policy: ValidationPolicy::Clamp,
                    ..Default::default()
                },
                ConfigField {
                    id: "theme",
                    field_type: ConfigType::Options,
                    label: "Theme",
                    description: "Color palette scheme",
                    default_value: "category_dynamic",
                    options: Some(vec![
                        ConfigOption {
                            label: "Dynamic Category Colors",
                            value: "category_dynamic",
                        },
                        ConfigOption {
                            label: "Breaking Crimson",
                            value: "breaking_crimson",
                        },
                        ConfigOption {
                            label: "Cyberpunk Neo",
                            value: "cyberpunk",
                        },
                        ConfigOption {
                            label: "Monochrome Paper",
                            value: "monochrome_paper",
                        },
                    ]),
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_category_badge",
                    field_type: ConfigType::Boolean,
                    label: "Show Category Badge",
                    description: "Display colored topic badge",
                    default_value: "true",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_source",
                    field_type: ConfigType::Boolean,
                    label: "Show Source",
                    description: "Display news source name",
                    default_value: "true",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_time_ago",
                    field_type: ConfigType::Boolean,
                    label: "Show Time Ago",
                    description: "Display relative time badge",
                    default_value: "true",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_beacon",
                    field_type: ConfigType::Boolean,
                    label: "Show Live Beacon",
                    description: "Display live pulsing broadcast dot",
                    default_value: "true",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_progress_dots",
                    field_type: ConfigType::Boolean,
                    label: "Show Progress Dots",
                    description: "Display headline index dots",
                    default_value: "true",
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
            ],
        },
        factory: || -> Box<dyn Engine> { Box::new(GNewsEngine::new(64, 32)) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_gnews_random_article_selection_cycles_all_cached_articles() {
        let mut engine = GNewsEngine::new(64, 32);
        engine.articles = (0..6)
            .map(|i| GNewsArticle {
                title: format!("Headline {i}"),
                description: format!("Description {i}"),
                content: format!("Full content body {i}"),
                source: "TestSource".to_string(),
                category: "technology".to_string(),
                published_epoch: 0,
                badge_color: (0, 229, 255),
            })
            .collect();
        engine.shuffled_order.clear();
        engine.shuffle_pos = 0;

        // First permutation of 6 articles must visit every cached article index 0..6 without repetition
        let mut first_cycle = Vec::new();
        for _ in 0..6 {
            engine.activate();
            first_cycle.push(engine.current_index);
        }
        let unique_first: HashSet<usize> = first_cycle.iter().copied().collect();
        assert_eq!(unique_first.len(), 6);

        // Next permutation must not repeat the last article of the previous cycle on boundary
        let last_of_first_cycle = *first_cycle.last().unwrap();
        engine.advance_to_next_article();
        assert_ne!(engine.current_index, last_of_first_cycle);
    }

    #[test]
    fn test_gnews_compose_text_and_self_paced_rotation() {
        let art = GNewsArticle {
            title: "Clickbait Title".to_string(),
            description: "Summary description".to_string(),
            content: "Full article content body".to_string(),
            source: "Source".to_string(),
            category: "technology".to_string(),
            published_epoch: 0,
            badge_color: (0, 229, 255),
        };

        // Default: only content enabled
        assert_eq!(
            art.compose_text(false, false, true),
            "Full article content body"
        );
        // Title + Content
        assert_eq!(
            art.compose_text(true, false, true),
            "Clickbait Title — Full article content body"
        );
        // Fallback when content is empty
        let art_no_content = GNewsArticle {
            content: String::new(),
            ..art.clone()
        };
        assert_eq!(
            art_no_content.compose_text(false, false, true),
            "Summary description"
        );

        let mut engine = GNewsEngine::new(64, 32);
        engine.articles = vec![art];
        engine.articles_per_rotation = 2;
        assert!(engine.self_paced());

        engine.activate();
        assert!(!engine.is_finished());
        engine.advance_to_next_article();
        assert!(!engine.is_finished());
        engine.advance_to_next_article();
        assert!(engine.is_finished());
    }
}

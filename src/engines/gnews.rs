use crate::core::engine_contract::{
    Capabilities, ConfigField, ConfigOption, ConfigSchema, ConfigType, Engine, EngineConfig,
    EngineContext, EngineDescriptor, EngineError, EngineMetadata, Requirements, ValidationPolicy,
};
use crate::core::matrix::MatrixBackend;
use crate::core::types::DisplayGeometry;
use crate::engines::renderers::BaseRenderer;
use linkme::distributed_slice;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollState {
    PauseStart,
    Scrolling,
    PauseEnd,
}

#[derive(Clone, Debug)]
pub struct GNewsArticle {
    pub title: String,
    pub source: String,
    pub category: String,
    pub published_epoch: u64,
    pub badge_color: (u8, u8, u8),
}

pub struct GNewsEngine {
    base_renderer: BaseRenderer,

    // Config fields
    api_key: String,
    category: String,
    keywords: String,
    lang: String,
    country: String,
    max_articles: usize,
    cache_ttl_min: u64,
    display_mode: String,
    scroll_speed: u32,
    scroll_pause_start_ms: u64,
    scroll_pause_end_ms: u64,
    article_duration_sec: u64,
    theme: String,
    show_category_badge: bool,
    show_source: bool,
    show_time_ago: bool,
    show_beacon: bool,
    show_progress_dots: bool,

    // Runtime state
    articles: Vec<GNewsArticle>,
    current_index: usize,
    scroll_offset: f32,
    scroll_state: ScrollState,
    state_start: Instant,
    last_update: Instant,
    last_article_switch: Instant,
    last_fetch: Option<Instant>,
    geometry: DisplayGeometry,
}

impl GNewsEngine {
    pub fn new(_w: u32, _h: u32) -> Self {
        let now = Instant::now();
        let mut engine = Self {
            base_renderer: BaseRenderer::new(),
            api_key: String::new(),
            category: "technology".to_string(),
            keywords: String::new(),
            lang: "auto".to_string(),
            country: "auto".to_string(),
            max_articles: 5,
            cache_ttl_min: 30,
            display_mode: "smooth_scroll".to_string(),
            scroll_speed: 3,
            scroll_pause_start_ms: 1200,
            scroll_pause_end_ms: 1000,
            article_duration_sec: 12,
            theme: "category_dynamic".to_string(),
            show_category_badge: true,
            show_source: true,
            show_time_ago: true,
            show_beacon: true,
            show_progress_dots: true,

            articles: Vec::new(),
            current_index: 0,
            scroll_offset: 0.0,
            scroll_state: ScrollState::PauseStart,
            state_start: now,
            last_update: now,
            last_article_switch: now,
            last_fetch: None,
            geometry: DisplayGeometry::new(64, 32, 0, 0),
        };
        engine.populate_demo_articles("technology");
        engine
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

    fn populate_demo_articles(&mut self, category: &str) {
        self.articles.clear();
        let cat = if category.is_empty() {
            "General"
        } else {
            category
        };
        let color = Self::get_category_color(cat);

        let demos = match cat.to_lowercase().as_str() {
            c if c.contains("tech") => vec![
                (
                    "Quantum computing milestone achieved with 1,000-qubit coherence",
                    "TechCrunch",
                ),
                (
                    "Next-generation neural architecture boosts edge efficiency by 40%",
                    "The Verge",
                ),
                (
                    "Retro arcade preservation initiative restores classic raster titles",
                    "Ars Technica",
                ),
            ],
            c if c.contains("sci") => vec![
                (
                    "James Webb Space Telescope detects organic molecules in distant galaxy",
                    "Nature",
                ),
                (
                    "New fusion containment record sets path for clean grid power",
                    "Science Daily",
                ),
            ],
            c if c.contains("world") => vec![
                (
                    "International summit reaches landmark agreement on clean energy standards",
                    "Reuters",
                ),
                (
                    "Global maritime corridor introduces automated zero-emission transit",
                    "BBC News",
                ),
            ],
            _ => vec![
                (
                    "Global technology summit unveils revolutionary advancements in AI & robotics",
                    "BBC News",
                ),
                (
                    "Autonomous exploration vessel reaches uncharted deep-sea ecosystem",
                    "Reuters",
                ),
                (
                    "Historic vintage gaming tournament attracts worldwide championship players",
                    "IGN",
                ),
            ],
        };

        for (title, source) in demos {
            self.articles.push(GNewsArticle {
                title: title.to_string(),
                source: source.to_string(),
                category: cat.to_string(),
                published_epoch: 0,
                badge_color: color,
            });
        }
    }

    fn apply_config(&mut self, config: &dyn EngineConfig) {
        self.api_key = config.get_string("api_key", "");
        self.category = config.get_string("category", "technology");
        self.keywords = config.get_string("keywords", "");
        self.lang = config.get_string("lang", "auto");
        self.country = config.get_string("country", "auto");
        self.max_articles = config.get_int("max_articles", 5).clamp(3, 15) as usize;
        self.cache_ttl_min = config.get_int("cache_ttl_min", 30).clamp(5, 120) as u64;
        self.display_mode = config.get_string("display_mode", "smooth_scroll");
        self.scroll_speed = config.get_int("scroll_speed", 3).clamp(1, 5) as u32;
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

        if self.articles.is_empty() {
            self.populate_demo_articles(&self.category.clone());
        }
    }

    fn fetch_news(&mut self) {
        if self.api_key.trim().is_empty() {
            if self.articles.is_empty() {
                self.populate_demo_articles(&self.category.clone());
            }
            return;
        }

        if let Some(fetched) = crate::api::gnews::GNewsProvider::fetch_articles(
            &self.api_key,
            &self.category,
            &self.keywords,
            &self.lang,
            &self.country,
            self.max_articles,
        ) {
            self.articles.clear();
            for a in fetched {
                let badge_color = Self::get_category_color(&a.category);
                self.articles.push(GNewsArticle {
                    title: a.title,
                    source: a.source,
                    category: a.category,
                    published_epoch: a.published_epoch,
                    badge_color,
                });
            }
            self.last_fetch = Some(Instant::now());
        } else if self.articles.is_empty() {
            self.populate_demo_articles(&self.category.clone());
        }
    }

    fn advance_to_next_article(&mut self) {
        if !self.articles.is_empty() {
            self.current_index = (self.current_index + 1) % self.articles.len();
        } else {
            self.current_index = 0;
        }
        let now = Instant::now();
        self.scroll_offset = 0.0;
        self.scroll_state = ScrollState::PauseStart;
        self.state_start = now;
        self.last_article_switch = now;
    }

    fn fill_rect_util(
        matrix: &mut dyn MatrixBackend,
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
        matrix: &mut dyn MatrixBackend,
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
        matrix: &mut dyn MatrixBackend,
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
        let should_fetch = self.last_fetch.map_or(true, |t| {
            t.elapsed() >= std::time::Duration::from_secs(self.cache_ttl_min * 60)
        });
        if should_fetch {
            self.fetch_news();
        }
        self.current_index = 0;
        self.scroll_offset = 0.0;
        self.scroll_state = ScrollState::PauseStart;
        let now = Instant::now();
        self.state_start = now;
        self.last_update = now;
        self.last_article_switch = now;
    }

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        self.apply_config(config);
        self.fetch_news();
    }

    fn on_display_geometry_changed(&mut self, geometry: &DisplayGeometry) {
        self.geometry = *geometry;
    }

    fn is_realtime(&self) -> bool {
        true
    }

    fn update(&mut self, _ctx: &mut EngineContext) {
        let now = Instant::now();
        let dt = now
            .duration_since(self.last_update)
            .as_secs_f32()
            .clamp(0.001, 0.1);
        self.last_update = now;

        if self.articles.is_empty() {
            return;
        }
        if self.current_index >= self.articles.len() {
            self.current_index = 0;
        }

        let article = &self.articles[self.current_index];

        if self.display_mode == "static_paged" {
            if now.duration_since(self.last_article_switch).as_secs() >= self.article_duration_sec {
                self.advance_to_next_article();
            }
        } else {
            // Smooth horizontal scrolling ticker
            let speed_pps = (self.scroll_speed as f32) * 12.0 + 6.0;
            let text_len = article.title.len();
            let text_width = (text_len * 6) as f32;

            match self.scroll_state {
                ScrollState::PauseStart => {
                    if now.duration_since(self.state_start).as_millis() as u64
                        >= self.scroll_pause_start_ms
                    {
                        self.scroll_state = ScrollState::Scrolling;
                        self.state_start = now;
                    }
                }
                ScrollState::Scrolling => {
                    self.scroll_offset += speed_pps * dt;
                    if self.scroll_offset >= (text_width + 8.0) {
                        self.scroll_state = ScrollState::PauseEnd;
                        self.state_start = now;
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
        let mw = ctx.matrix.width();
        let mh = ctx.matrix.height();
        ctx.matrix.clear();

        if self.articles.is_empty() {
            let matrix = &mut *ctx.matrix;
            self.base_renderer.render_text(
                matrix,
                "GNEWS LIVE",
                -1,
                1,
                2,
                (mh / 2).saturating_sub(4) as i32,
                Some((0, 229, 255)),
                None,
            );
            return;
        }

        if self.current_index >= self.articles.len() {
            self.current_index = 0;
        }
        let article = self.articles[self.current_index].clone();
        let total_count = self.articles.len();

        let beacon_pulse = ((self.scroll_offset * 0.1).sin() + 1.0) * 0.5;

        let mut cat_color = article.badge_color;
        if self.theme == "breaking_crimson" {
            cat_color = (255, 42, 77);
        } else if self.theme == "cyberpunk" {
            cat_color = (0, 229, 255);
        } else if self.theme == "monochrome_paper" {
            cat_color = (224, 230, 237);
        }

        let matrix = &mut *ctx.matrix;

        if mw >= 128 {
            // Wide Layout
            let mut cur_x: i32 = 4;
            let header_y: i32 = if mh >= 64 { 4 } else { 2 };

            // 1. Live Pulsing Beacon
            if self.show_beacon {
                let br = (120.0 + beacon_pulse * 135.0) as u8;
                Self::fill_rect_util(matrix, cur_x, header_y + 2, 3, 3, (br, 15, 25));
                cur_x += 8;
            }

            // 2. Category Pill Badge
            if self.show_category_badge {
                let cat_upper = article.category.to_uppercase();
                let cat_w = (cat_upper.len() * 6 + 4) as u32;
                Self::fill_rect_util(matrix, cur_x, header_y, cat_w, 9, (20, 25, 35));
                Self::draw_rect_util(matrix, cur_x, header_y, cat_w, 9, cat_color);
                self.base_renderer.render_text(
                    matrix,
                    &cat_upper,
                    -1,
                    1,
                    cur_x + 2,
                    header_y + 1,
                    Some(cat_color),
                    None,
                );
                cur_x += (cat_w + 6) as i32;
            }

            // 3. News Source Name
            if self.show_source {
                self.base_renderer.render_text(
                    matrix,
                    &article.source,
                    -1,
                    1,
                    cur_x,
                    header_y + 1,
                    Some((200, 210, 225)),
                    None,
                );
                cur_x += (article.source.len() * 6 + 6) as i32;
            }

            // 4. Progress Dots
            if self.show_progress_dots && total_count > 1 {
                let dots_start_x = (mw as i32) - ((total_count * 6 + 4) as i32);
                if dots_start_x > cur_x {
                    for i in 0..total_count.min(8) {
                        let dx = dots_start_x + (i * 6) as i32;
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

            // Headline Text Ticker
            let body_y = div_y + if mh >= 64 { 8 } else { 3 };
            let start_x = 4 - (self.scroll_offset as i32);
            self.base_renderer.render_text(
                matrix,
                &article.title,
                -1,
                1,
                start_x,
                body_y,
                Some((255, 255, 255)),
                None,
            );
        } else {
            // Compact Layout (64x32 or vertical)
            if self.show_beacon {
                let br = (120.0 + beacon_pulse * 135.0) as u8;
                Self::fill_rect_util(matrix, 2, 2, 3, 3, (br, 20, 20));
            }

            let cat_upper = article.category.to_uppercase();
            let cat_short = if cat_upper.len() > 6 {
                &cat_upper[..6]
            } else {
                &cat_upper
            };
            self.base_renderer
                .render_text(matrix, cat_short, -1, 1, 8, 1, Some(cat_color), None);

            let idx_str = format!("{}/{}", self.current_index + 1, total_count);
            let idx_x = (mw as i32).saturating_sub(18);
            self.base_renderer.render_text(
                matrix,
                &idx_str,
                -1,
                1,
                idx_x,
                1,
                Some((140, 150, 160)),
                None,
            );

            Self::draw_hline_util(matrix, 0, 10, mw, (35, 40, 50));

            // Headline ticker
            let start_x = 2 - (self.scroll_offset as i32);
            self.base_renderer.render_text(
                matrix,
                &article.title,
                -1,
                1,
                start_x,
                15,
                Some((255, 255, 255)),
                None,
            );
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
                    description: "Ticker speed (1=Slow, 5=Turbo)",
                    default_value: "3",
                    min_val: Some("1"),
                    max_val: Some("5"),
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

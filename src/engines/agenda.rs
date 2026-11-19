//! `agenda` Engine — Dual-Calendar RFC 5545 `.ics` Agenda with Binary POD Cache (`AGND`),
//! Header-Only Reboot Fast-Path, Semantic Arcade Sprites, and Zero-Allocation Rendering.

use crate::core::engine_contract::{
    Capabilities, ConfigField, ConfigOption, ConfigSchema, ConfigType, Engine, EngineConfig,
    EngineContext, EngineDescriptor, EngineError, EngineMetadata, Requirements, ValidationPolicy,
};
use crate::core::i18n::Lang;
use crate::core::registry::ENGINES;
use crate::core::types::DisplayGeometry;
use crate::engines::agenda_ics::{
    compute_agenda_config_crc32, normalize_ics_url, parse_hex_color_565, parse_ics_stream,
    read_cache_header_only, read_cache_records, write_cache_records_atomic, AgendaEventPod,
    MAX_AGENDA_EVENTS,
};
use crate::engines::renderers::agenda::{AgendaRenderParams, AgendaRenderer};
use chrono_tz::Tz;
use linkme::distributed_slice;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmptyAgendaBehavior {
    Chill,
    Skip,
}

impl EmptyAgendaBehavior {
    pub fn from_str_opt(s: &str) -> Self {
        if s.trim().eq_ignore_ascii_case("skip") {
            Self::Skip
        } else {
            Self::Chill
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgendaViewMode {
    Auto,
    MultiDayScroll,
    Cards,
    SplitDeck,
    Ticker,
}

impl AgendaViewMode {
    pub fn from_str_opt(s: &str) -> Self {
        let t = s.trim();
        if t.eq_ignore_ascii_case("multiday_scroll") {
            Self::MultiDayScroll
        } else if t.eq_ignore_ascii_case("cards") {
            Self::Cards
        } else if t.eq_ignore_ascii_case("split_deck") {
            Self::SplitDeck
        } else if t.eq_ignore_ascii_case("ticker") {
            Self::Ticker
        } else {
            Self::Auto
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgendaTheme {
    CyberArcade,
    AmberDmd,
    MatrixGreen,
    SnesRpg,
    MinimalClean,
}

impl AgendaTheme {
    pub fn from_str_opt(s: &str) -> Self {
        let t = s.trim();
        if t.eq_ignore_ascii_case("amber_dmd") {
            Self::AmberDmd
        } else if t.eq_ignore_ascii_case("matrix_green") {
            Self::MatrixGreen
        } else if t.eq_ignore_ascii_case("snes_rpg") {
            Self::SnesRpg
        } else if t.eq_ignore_ascii_case("minimal_clean") {
            Self::MinimalClean
        } else {
            Self::CyberArcade
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgendaScrollSpeed {
    Slow,
    Normal,
    Fast,
}

impl AgendaScrollSpeed {
    pub fn from_str_opt(s: &str) -> Self {
        let t = s.trim();
        if t.eq_ignore_ascii_case("slow") {
            Self::Slow
        } else if t.eq_ignore_ascii_case("fast") {
            Self::Fast
        } else {
            Self::Normal
        }
    }

    pub const fn step_ms(self) -> u64 {
        match self {
            Self::Slow => 95,
            Self::Normal => 60,
            Self::Fast => 35,
        }
    }
}

pub struct AgendaEngine {
    instance_id: String,
    custom_cache_path: Option<PathBuf>,

    // Config fields
    ics_url_1: String,
    ics_label_1: String,
    ics_color_1: String,
    color_1_565: u16,
    label_1_buf: [u8; 20],

    ics_url_2: String,
    ics_label_2: String,
    ics_color_2: String,
    color_2_565: u16,
    label_2_buf: [u8; 20],

    view_mode: AgendaViewMode,
    theme: AgendaTheme,
    scroll_speed: AgendaScrollSpeed,
    slide_duration_sec: u32,
    show_relative_time: bool,
    show_badges: bool,
    cache_ttl_min: u32,
    horizon_days: u32,
    events_shown: usize,
    empty_behavior: EmptyAgendaBehavior,
    imminent_min: u32,
    fun_sprites: bool,

    // Pre-computed CRC32 & cache metadata
    config_crc32: u32,
    last_fetch_epoch: u32,
    shared_last_fetch_epoch: Arc<AtomicU32>,
    cache_updated_flag: Arc<AtomicBool>,
    is_fetching: Arc<AtomicBool>,
    last_fetch_attempt_epoch: u32,

    // Active fixed-capacity POD event table (0 heap allocation in update/render)
    events: [AgendaEventPod; MAX_AGENDA_EVENTS],
    event_count: usize,

    // Display & animation state
    geometry: DisplayGeometry,
    cached_tz: Tz,
    cached_lang: Lang,
    anim_tick: u32,
    scroll_offset: i32,
    current_slide_idx: usize,
    last_slide_instant: Instant,
    last_anim_instant: Instant,
    last_clock_sec: u32,
    pending_draw: bool,
    skip_finished: bool,
}

impl Default for AgendaEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AgendaEngine {
    pub fn new() -> Self {
        let mut label_1_buf = [0u8; 20];
        label_1_buf[..5].copy_from_slice(b"Perso");
        let mut label_2_buf = [0u8; 20];
        label_2_buf[..3].copy_from_slice(b"Pro");

        Self {
            instance_id: "agenda_1".to_string(),
            custom_cache_path: None,
            ics_url_1: String::new(),
            ics_label_1: "Perso".to_string(),
            ics_color_1: "#00E5FF".to_string(),
            color_1_565: parse_hex_color_565("#00E5FF", (0, 229, 255)),
            label_1_buf,
            ics_url_2: String::new(),
            ics_label_2: "Pro".to_string(),
            ics_color_2: "#FFB300".to_string(),
            color_2_565: parse_hex_color_565("#FFB300", (255, 179, 0)),
            label_2_buf,
            view_mode: AgendaViewMode::Auto,
            theme: AgendaTheme::CyberArcade,
            scroll_speed: AgendaScrollSpeed::Normal,
            slide_duration_sec: 5,
            show_relative_time: true,
            show_badges: true,
            cache_ttl_min: 60,
            horizon_days: 7,
            events_shown: 5,
            empty_behavior: EmptyAgendaBehavior::Chill,
            imminent_min: 15,
            fun_sprites: true,
            config_crc32: compute_agenda_config_crc32("", "", 7),
            last_fetch_epoch: 0,
            shared_last_fetch_epoch: Arc::new(AtomicU32::new(0)),
            cache_updated_flag: Arc::new(AtomicBool::new(false)),
            is_fetching: Arc::new(AtomicBool::new(false)),
            last_fetch_attempt_epoch: 0,
            events: [AgendaEventPod::empty(); MAX_AGENDA_EVENTS],
            event_count: 0,
            geometry: DisplayGeometry::new(64, 32, 0, 0),
            cached_tz: chrono_tz::Europe::Paris,
            cached_lang: Lang::Fr,
            anim_tick: 0,
            scroll_offset: 0,
            current_slide_idx: 0,
            last_slide_instant: Instant::now(),
            last_anim_instant: Instant::now(),
            last_clock_sec: 0,
            pending_draw: true,
            skip_finished: false,
        }
    }

    pub fn with_cache_path(path: impl Into<PathBuf>) -> Self {
        let mut eng = Self::new();
        eng.custom_cache_path = Some(path.into());
        eng
    }

    pub fn set_cache_path(&mut self, path: impl Into<PathBuf>) {
        self.custom_cache_path = Some(path.into());
    }

    pub fn cache_file_path(&self) -> PathBuf {
        if let Some(ref p) = self.custom_cache_path {
            return p.clone();
        }
        let safe_id: String = self
            .instance_id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let id = if safe_id.is_empty() {
            "agenda_1"
        } else {
            safe_id.as_str()
        };
        PathBuf::from(format!("data/cache/agenda_{}.bin", id))
    }

    #[inline]
    pub fn last_fetch_epoch(&self) -> u32 {
        self.last_fetch_epoch
    }

    #[inline]
    pub fn active_event_count(&self) -> usize {
        self.event_count
    }

    #[inline]
    pub fn active_events(&self) -> &[AgendaEventPod] {
        &self.events[..self.event_count]
    }

    /// Injects pre-built `AgendaEventPod` records directly (used by deterministic zero-alloc tests).
    pub fn set_events_pod(&mut self, pods: &[AgendaEventPod]) {
        let n = pods.len().min(MAX_AGENDA_EVENTS);
        self.events = [AgendaEventPod::empty(); MAX_AGENDA_EVENTS];
        self.events[..n].copy_from_slice(&pods[..n]);
        self.event_count = n;
        self.pending_draw = true;
    }

    fn apply_config(&mut self, config: &dyn EngineConfig, sys_tz: &str, sys_lang: &str) {
        let custom_cache = config.get_string("_cache_path", "");
        if !custom_cache.is_empty() {
            self.custom_cache_path = Some(PathBuf::from(custom_cache));
        }
        let cfg_inst_id = config.get_string("_instance_id", "");
        if !cfg_inst_id.is_empty() {
            self.instance_id = cfg_inst_id;
        }

        self.ics_url_1 = config.get_string("ics_url_1", "").trim().to_string();
        self.ics_label_1 = config.get_string("ics_label_1", "Perso");
        self.ics_color_1 = config.get_string("ics_color_1", "#00E5FF");
        self.color_1_565 = parse_hex_color_565(&self.ics_color_1, (0, 229, 255));
        crate::engines::agenda_ics::copy_utf8_truncated(&self.ics_label_1, &mut self.label_1_buf);

        self.ics_url_2 = config.get_string("ics_url_2", "").trim().to_string();
        self.ics_label_2 = config.get_string("ics_label_2", "Pro");
        self.ics_color_2 = config.get_string("ics_color_2", "#FFB300");
        self.color_2_565 = parse_hex_color_565(&self.ics_color_2, (255, 179, 0));
        crate::engines::agenda_ics::copy_utf8_truncated(&self.ics_label_2, &mut self.label_2_buf);

        self.view_mode = AgendaViewMode::from_str_opt(&config.get_string("view_mode", "auto"));
        self.theme = AgendaTheme::from_str_opt(&config.get_string("theme", "cyber_arcade"));
        self.scroll_speed =
            AgendaScrollSpeed::from_str_opt(&config.get_string("scroll_speed", "normal"));
        self.slide_duration_sec = (config.get_int("slide_duration_sec", 5) as u32).clamp(3, 20);
        self.show_relative_time = config.get_bool("show_relative_time", true);
        self.show_badges = config.get_bool("show_badges", true);
        self.cache_ttl_min = (config.get_int("cache_ttl_min", 60) as u32).clamp(15, 1440);
        self.horizon_days = (config.get_int("horizon_days", 7) as u32).clamp(1, 30);
        self.events_shown = (config.get_int("events_shown", 5) as usize).clamp(1, 8);
        self.empty_behavior =
            EmptyAgendaBehavior::from_str_opt(&config.get_string("empty_behavior", "chill"));
        self.imminent_min = (config.get_int("imminent_min", 15) as u32).clamp(0, 60);
        self.fun_sprites = config.get_bool("fun_sprites", true);

        self.config_crc32 =
            compute_agenda_config_crc32(&self.ics_url_1, &self.ics_url_2, self.horizon_days as i32);
        self.cached_tz = sys_tz.parse::<Tz>().unwrap_or(chrono_tz::Europe::Paris);
        self.cached_lang = Lang::from_code(sys_lang);
    }

    #[inline]
    pub fn current_unix_epoch() -> u32 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as u32)
            .unwrap_or(0)
    }

    #[inline]
    pub const fn config_crc32(&self) -> u32 {
        self.config_crc32
    }

    /// Reads ONLY the 24-byte `AgendaCacheHeader` during `initialize()` to restore `last_fetch_epoch`
    /// if `magic == 0x41474E44` and `config_crc32` matches, preventing redundant fetches after reboot.
    pub fn restore_header_from_cache(&mut self) -> bool {
        let path = self.cache_file_path();
        if let Some(hdr) = read_cache_header_only(&path) {
            if hdr.config_crc32 == self.config_crc32 {
                self.last_fetch_epoch = hdr.last_fetch_epoch;
                self.shared_last_fetch_epoch
                    .store(hdr.last_fetch_epoch, Ordering::Release);
                return true;
            }
        }
        self.last_fetch_epoch = 0;
        self.shared_last_fetch_epoch.store(0, Ordering::Release);
        false
    }

    /// Reads cached `AgendaEventPod` entries on `activate()` and filters `end_epoch > now_epoch`
    /// into the fixed `self.events` array.
    pub fn load_active_events_from_cache(&mut self, now_epoch: u32) -> usize {
        let path = self.cache_file_path();
        self.events = [AgendaEventPod::empty(); MAX_AGENDA_EVENTS];
        self.event_count = 0;
        if let Some((hdr, pods, count)) = read_cache_records(&path, self.config_crc32) {
            self.last_fetch_epoch = hdr.last_fetch_epoch;
            self.shared_last_fetch_epoch
                .store(hdr.last_fetch_epoch, Ordering::Release);
            for pod in pods.iter().take(count) {
                let end_ep = pod.end_epoch;
                if end_ep > now_epoch && self.event_count < MAX_AGENDA_EVENTS {
                    self.events[self.event_count] = *pod;
                    self.event_count += 1;
                }
            }
        }
        self.event_count
    }

    fn maybe_spawn_background_fetch(&mut self, now_epoch: u32, tz_str: &str) {
        if self.ics_url_1.is_empty() && self.ics_url_2.is_empty() {
            return;
        }
        let ttl_secs = self.cache_ttl_min.saturating_mul(60);
        let cache_fresh =
            self.last_fetch_epoch > 0 && now_epoch < self.last_fetch_epoch.saturating_add(ttl_secs);
        if cache_fresh {
            return;
        }
        if self.last_fetch_attempt_epoch > 0
            && now_epoch < self.last_fetch_attempt_epoch.saturating_add(30)
        {
            return;
        }
        if self
            .is_fetching
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }

        self.last_fetch_attempt_epoch = now_epoch;
        let url1 = self.ics_url_1.clone();
        let lbl1 = self.ics_label_1.clone();
        let col1 = self.color_1_565;
        let url2 = self.ics_url_2.clone();
        let lbl2 = self.ics_label_2.clone();
        let col2 = self.color_2_565;
        let horizon_days = self.horizon_days;
        let cfg_crc = self.config_crc32;
        let cache_path = self.cache_file_path();
        let tz_owned = tz_str.to_string();
        let is_fetching = Arc::clone(&self.is_fetching);
        let shared_epoch = Arc::clone(&self.shared_last_fetch_epoch);
        let updated_flag = Arc::clone(&self.cache_updated_flag);

        std::thread::spawn(move || {
            let client = crate::core::memory::api_http_client();
            let mut pods = [AgendaEventPod::empty(); MAX_AGENDA_EVENTS];
            let mut count = 0usize;
            let mut any_ok = false;

            for (idx, (raw_url, label, color_565)) in
                [(0u8, (&url1, &lbl1, col1)), (1u8, (&url2, &lbl2, col2))]
            {
                if raw_url.trim().is_empty() {
                    continue;
                }
                let fetch_url = normalize_ics_url(raw_url);
                if let Ok(resp) = client
                    .get(&fetch_url)
                    .timeout(Duration::from_secs(10))
                    .send()
                {
                    if resp.status().is_success() {
                        parse_ics_stream(
                            resp,
                            now_epoch,
                            horizon_days,
                            &tz_owned,
                            idx,
                            color_565,
                            label,
                            &mut pods,
                            &mut count,
                        );
                        any_ok = true;
                    }
                }
            }

            if any_ok {
                let fetch_ts = Self::current_unix_epoch().max(1);
                if write_cache_records_atomic(&cache_path, cfg_crc, fetch_ts, &pods[..count]) {
                    shared_epoch.store(fetch_ts, Ordering::Release);
                    updated_flag.store(true, Ordering::Release);
                }
            }

            crate::core::memory::trim_heap();
            is_fetching.store(false, Ordering::Release);
        });
    }
}

impl Engine for AgendaEngine {
    fn set_instance_name(&mut self, name: &str) {
        if !name.trim().is_empty() {
            self.instance_id = name.trim().to_string();
        }
    }

    fn initialize(
        &mut self,
        context: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        let (sys_tz, sys_lang) = {
            let s = context.config.settings.read();
            (s.system.timezone.clone(), s.system.lang.clone())
        };
        self.apply_config(config, &sys_tz, &sys_lang);
        let w = context.surface.width();
        let h = context.surface.height();
        self.geometry = DisplayGeometry::new(w, h, 0, 1);

        // Header-only reboot fast-path: reads strictly the 24-byte AgendaCacheHeader
        self.restore_header_from_cache();
        Ok(())
    }

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        let prev_crc = self.config_crc32;
        let tz_str = self.cached_tz.name().to_string();
        let lang_str = self.cached_lang.as_code().to_string();
        self.apply_config(config, &tz_str, &lang_str);
        if self.config_crc32 != prev_crc {
            self.restore_header_from_cache();
            let now_ep = Self::current_unix_epoch();
            self.load_active_events_from_cache(now_ep);
        }
        self.pending_draw = true;
    }

    fn activate(&mut self) {
        self.anim_tick = 0;
        self.scroll_offset = 0;
        self.last_anim_instant = Instant::now();
        self.pending_draw = true;
        self.skip_finished = false;

        let now_epoch = Self::current_unix_epoch();
        self.load_active_events_from_cache(now_epoch);

        let tz_name = self.cached_tz.name();
        self.maybe_spawn_background_fetch(now_epoch, tz_name);

        if self.event_count == 0
            && self.empty_behavior == EmptyAgendaBehavior::Skip
            && !self.is_fetching.load(Ordering::Acquire)
        {
            self.skip_finished = true;
        }
    }

    fn deactivate(&mut self) {
        self.pending_draw = false;
    }

    fn on_display_geometry_changed(&mut self, geometry: &DisplayGeometry) {
        self.geometry = *geometry;
        self.pending_draw = true;
    }

    fn update(&mut self, context: &mut EngineContext) {
        // Sync language via lock-free AtomicU8 (zero RwLock / zero heap allocation on H-Core)
        let new_lang = context.config.system_lang_fast();
        if new_lang != self.cached_lang {
            self.cached_lang = new_lang;
            self.pending_draw = true;
        }

        let w = context.surface.width();
        let h = context.surface.height();
        if w != self.geometry.width() || h != self.geometry.height() {
            self.geometry = DisplayGeometry::new(w, h, 0, self.geometry.version.wrapping_add(1));
            self.pending_draw = true;
        }

        // Check if background worker committed a new binary cache (cold event)
        if self.cache_updated_flag.swap(false, Ordering::AcqRel) {
            let now_ep = Self::current_unix_epoch();
            self.load_active_events_from_cache(now_ep);
            self.pending_draw = true;
        }

        let now_inst = Instant::now();
        let step_ms = self.scroll_speed.step_ms();
        if now_inst.duration_since(self.last_anim_instant) >= Duration::from_millis(step_ms) {
            self.last_anim_instant = now_inst;
            self.anim_tick = self.anim_tick.wrapping_add(1);
            self.scroll_offset = self.scroll_offset.wrapping_add(1);
            self.pending_draw = true;
        }

        let shown = self.event_count.min(self.events_shown);
        if shown > 1
            && now_inst.duration_since(self.last_slide_instant)
                >= Duration::from_secs(self.slide_duration_sec as u64)
        {
            self.last_slide_instant = now_inst;
            self.current_slide_idx = (self.current_slide_idx + 1) % shown;
            self.pending_draw = true;
        }

        let now_epoch = Self::current_unix_epoch();
        if now_epoch != self.last_clock_sec {
            self.last_clock_sec = now_epoch;
            // Prune expired events in-place without allocation
            let mut write_idx = 0usize;
            for read_idx in 0..self.event_count {
                let ev = self.events[read_idx];
                if ev.end_epoch > now_epoch {
                    self.events[write_idx] = ev;
                    write_idx += 1;
                }
            }
            if write_idx != self.event_count {
                self.event_count = write_idx;
                self.pending_draw = true;
            }
        }

        if self.event_count == 0
            && self.empty_behavior == EmptyAgendaBehavior::Skip
            && !self.is_fetching.load(Ordering::Acquire)
        {
            self.skip_finished = true;
        }
    }

    fn render(&mut self, context: &mut EngineContext) {
        let params = AgendaRenderParams {
            events: &self.events,
            event_count: self.event_count,
            events_shown: self.events_shown,
            current_slide_idx: self.current_slide_idx,
            view_mode: self.view_mode,
            theme: self.theme,
            lang: self.cached_lang,
            tz: self.cached_tz,
            now_epoch: Self::current_unix_epoch(),
            imminent_min: self.imminent_min,
            anim_tick: self.anim_tick,
            scroll_offset: self.scroll_offset,
            show_relative_time: self.show_relative_time,
            show_badges: self.show_badges,
            fun_sprites: self.fun_sprites,
            has_secondary_calendar: !self.ics_url_2.is_empty(),
            label_1_buf: &self.label_1_buf,
            color_1_565: self.color_1_565,
            label_2_buf: &self.label_2_buf,
            color_2_565: self.color_2_565,
        };
        AgendaRenderer::render(&mut *context.surface, &params);
        self.pending_draw = false;
    }

    fn has_new_frame(&self) -> bool {
        self.pending_draw
    }

    fn is_realtime(&self) -> bool {
        false
    }

    fn is_finished(&self) -> bool {
        self.skip_finished
    }
}

#[distributed_slice(ENGINES)]
pub fn register_agenda_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "agenda",
            name: "Smart Agenda (iCal / .ics)",
            category: "daily",
            version: "1.0",
        },
        capabilities: Capabilities {
            supports_128x32: true,
            supports_256x64: true,
            realtime: false,
            interruptible: true,
            allows_overlay: true,
            allow_rotation: true,
        },
        requirements: Requirements {
            needs_audio: false,
            needs_network: true,
            needs_sd: true,
        },
        available: true,
        unavailable_reason: None,
        schema: ConfigSchema {
            fields: vec![
                ConfigField {
                    id: "ics_url_1",
                    field_type: ConfigType::String,
                    label: "Primary Calendar URL (.ics)",
                    description:
                        "Private iCal / .ics URL (Google, Apple iCloud, Outlook, Nextcloud)",
                    default_value: "",
                    required: false,
                    ..Default::default()
                },
                ConfigField {
                    id: "ics_label_1",
                    field_type: ConfigType::String,
                    label: "Primary Calendar Label",
                    description: "Short badge name for the primary calendar",
                    default_value: "Perso",
                    required: false,
                    ..Default::default()
                },
                ConfigField {
                    id: "ics_color_1",
                    field_type: ConfigType::String,
                    label: "Primary Calendar Color",
                    description: "Accent hex color for the primary calendar (#RRGGBB)",
                    default_value: "#00E5FF",
                    required: false,
                    ..Default::default()
                },
                ConfigField {
                    id: "ics_url_2",
                    field_type: ConfigType::String,
                    label: "Secondary Calendar URL (.ics)",
                    description: "Optional second iCal / .ics URL merged chronologically",
                    default_value: "",
                    required: false,
                    ..Default::default()
                },
                ConfigField {
                    id: "ics_label_2",
                    field_type: ConfigType::String,
                    label: "Secondary Calendar Label",
                    description: "Short badge name for the secondary calendar",
                    default_value: "Pro",
                    required: false,
                    ..Default::default()
                },
                ConfigField {
                    id: "ics_color_2",
                    field_type: ConfigType::String,
                    label: "Secondary Calendar Color",
                    description: "Accent hex color for the secondary calendar (#RRGGBB)",
                    default_value: "#FFB300",
                    required: false,
                    ..Default::default()
                },
                ConfigField {
                    id: "view_mode",
                    field_type: ConfigType::Options,
                    label: "Display Layout Mode",
                    description: "Visual layout: Auto, Multi-Day Vertical Scroll, Sliding Cards, Split Calendar Deck, or Horizontal Ticker",
                    default_value: "auto",
                    required: false,
                    options: Some(vec![
                        ConfigOption {
                            label: "Auto Responsive",
                            value: "auto",
                        },
                        ConfigOption {
                            label: "Multi-Day Vertical Scroll",
                            value: "multiday_scroll",
                        },
                        ConfigOption {
                            label: "Sliding Event Cards",
                            value: "cards",
                        },
                        ConfigOption {
                            label: "Split Calendar Deck",
                            value: "split_deck",
                        },
                        ConfigOption {
                            label: "Continuous Horizontal Ticker",
                            value: "ticker",
                        },
                    ]),
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "theme",
                    field_type: ConfigType::Options,
                    label: "Visual Theme",
                    description: "Color palette and border style for the agenda display",
                    default_value: "cyber_arcade",
                    required: false,
                    options: Some(vec![
                        ConfigOption {
                            label: "Cyber Arcade",
                            value: "cyber_arcade",
                        },
                        ConfigOption {
                            label: "Pinball Amber DMD",
                            value: "amber_dmd",
                        },
                        ConfigOption {
                            label: "Matrix Phosphor",
                            value: "matrix_green",
                        },
                        ConfigOption {
                            label: "16-Bit RPG Quest Log",
                            value: "snes_rpg",
                        },
                        ConfigOption {
                            label: "Minimalist Clean",
                            value: "minimal_clean",
                        },
                    ]),
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "scroll_speed",
                    field_type: ConfigType::Options,
                    label: "Scroll Speed",
                    description: "Scrolling speed for multi-day feed, horizontal ticker, and long event titles",
                    default_value: "normal",
                    required: false,
                    options: Some(vec![
                        ConfigOption {
                            label: "Slow",
                            value: "slow",
                        },
                        ConfigOption {
                            label: "Normal",
                            value: "normal",
                        },
                        ConfigOption {
                            label: "Fast",
                            value: "fast",
                        },
                    ]),
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "slide_duration_sec",
                    field_type: ConfigType::Integer,
                    label: "Slide / Page Duration (sec)",
                    description: "Seconds per card or page when cycling through multi-day events (3 to 20)",
                    default_value: "5",
                    required: false,
                    min_val: Some("3"),
                    max_val: Some("20"),
                    validation_policy: ValidationPolicy::Clamp,
                    ..Default::default()
                },
                ConfigField {
                    id: "cache_ttl_min",
                    field_type: ConfigType::Integer,
                    label: "Refresh Interval (min)",
                    description: "Background .ics synchronization interval in minutes (15 to 1440)",
                    default_value: "60",
                    required: false,
                    min_val: Some("15"),
                    max_val: Some("1440"),
                    validation_policy: ValidationPolicy::Clamp,
                    ..Default::default()
                },
                ConfigField {
                    id: "horizon_days",
                    field_type: ConfigType::Integer,
                    label: "Lookahead Horizon (days)",
                    description: "Number of upcoming days to display (1 to 30)",
                    default_value: "7",
                    required: false,
                    min_val: Some("1"),
                    max_val: Some("30"),
                    validation_policy: ValidationPolicy::Clamp,
                    ..Default::default()
                },
                ConfigField {
                    id: "events_shown",
                    field_type: ConfigType::Integer,
                    label: "Max Events Shown",
                    description: "Maximum number of upcoming events shown across the multi-day horizon (1 to 8)",
                    default_value: "5",
                    required: false,
                    min_val: Some("1"),
                    max_val: Some("8"),
                    validation_policy: ValidationPolicy::Clamp,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_relative_time",
                    field_type: ConfigType::Boolean,
                    label: "Show Relative Countdown",
                    description: "Display relative day/minute countdowns (e.g. IN 12M, J+3 / +3D)",
                    default_value: "true",
                    required: false,
                    ..Default::default()
                },
                ConfigField {
                    id: "show_badges",
                    field_type: ConfigType::Boolean,
                    label: "Show Calendar Badges",
                    description: "Display calendar source badge pills ([Perso], [Pro]) next to events",
                    default_value: "true",
                    required: false,
                    ..Default::default()
                },
                ConfigField {
                    id: "empty_behavior",
                    field_type: ConfigType::Options,
                    label: "When No Events",
                    description: "Behavior when no upcoming events exist in the horizon",
                    default_value: "chill",
                    required: false,
                    options: Some(vec![
                        ConfigOption {
                            label: "Chill Free Play Scene (Sleeping Metool)",
                            value: "chill",
                        },
                        ConfigOption {
                            label: "Skip Rotation Slot Immediately",
                            value: "skip",
                        },
                    ]),
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
                ConfigField {
                    id: "imminent_min",
                    field_type: ConfigType::Integer,
                    label: "Imminent Alert Threshold (min)",
                    description:
                        "Minutes before event start to trigger imminent countdown alert (0 to 60)",
                    default_value: "15",
                    required: false,
                    min_val: Some("0"),
                    max_val: Some("60"),
                    validation_policy: ValidationPolicy::Clamp,
                    ..Default::default()
                },
                ConfigField {
                    id: "fun_sprites",
                    field_type: ConfigType::Boolean,
                    label: "Arcade Context Icons",
                    description:
                        "Display pixel-art context icons (Birthday, Call, Resto, Sport, Gaming)",
                    default_value: "true",
                    required: false,
                    ..Default::default()
                },
            ],
        },
        factory: || Box::new(AgendaEngine::new()),
    }
}

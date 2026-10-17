//! MQTT Data engine: shows pages that a publisher (a home automation system, a
//! script...) sends as retained MQTT messages, following the contract in
//! docs/MQTT_DATA_CONTRACT.md.
//!
//! The instance only lists topics (up to 6). Everything else comes from each
//! payload: its `type` picks the renderer (`value`, `table`, `graph`,
//! `weather`) and `seconds` its time on screen. A weather payload expands to
//! its NOW page plus one page per forecast day. The engine owns its slot's
//! duration: one full cycle of its pages (`self_paced` + `is_finished`).
//!
//! Nothing runs in the background. The engine connects when its slot becomes
//! active (credentials from the global `data_mqtt` settings), subscribes to its
//! topics, receives the retained snapshot, stays subscribed for live updates
//! while on screen, and on deactivate stops its thread, disconnects and frees
//! everything (Discussion #59). Payloads are parsed on the MQTT thread; the
//! render path only reads the parsed pages.

pub mod graph;
pub mod payload;
pub mod table;
pub mod weather;

use crate::api::DayForecast;
use crate::core::config::DataMqttConfig;
use crate::core::engine_contract::{
    Capabilities, ConfigField, ConfigSchema, ConfigType, Engine, EngineConfig, EngineContext,
    EngineDescriptor, EngineError, EngineMetadata, Requirements, ValidationPolicy,
};
use crate::core::i18n::{self, Lang};
use crate::core::matrix::IDrawingSurface;
use crate::engines::dashboard::font::{draw_text_scaled, measure_text};
use crate::engines::renderers::weather_page::draw_weather_page;
use arc_swap::ArcSwapOption;
use linkme::distributed_slice;
use payload::{parse_page, Page, PageData, DEFAULT_PAGE_SECONDS, MAX_PAYLOAD_BYTES, MAX_TOPICS};
use rumqttc::{Client, Event, MqttOptions, Packet, QoS};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// A page whose topic sent nothing this long after connecting shows NO DATA.
pub const NO_DATA_AFTER: Duration = Duration::from_secs(3);
/// How long the MQTT thread waits per poll before checking its stop flag.
const POLL_SLICE: Duration = Duration::from_millis(150);

const NOTICE_COLOR: (u8, u8, u8) = (0x78, 0x78, 0x78);
const NO_CONNECTION_COLOR: (u8, u8, u8) = (0xB4, 0x50, 0x50);
const UNSUPPORTED_COLOR: (u8, u8, u8) = (0xC0, 0x80, 0x00);

/// Connection state shared with the MQTT thread.
pub const STATE_CONNECTING: u8 = 0;
pub const STATE_CONNECTED: u8 = 1;
pub const STATE_FAILED: u8 = 2;

/// Splits the `topics` setting: comma, semicolon or newline separated, blanks
/// dropped, at most [`MAX_TOPICS`].
pub fn parse_topics(raw: &str) -> Vec<String> {
    raw.split([',', ';', '\n'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .take(MAX_TOPICS)
        .map(String::from)
        .collect()
}

/// MQTT client id for this sign's data connection (`<hostname>-data`).
pub fn data_client_id(hostname: &str, device_name: &str) -> String {
    let base = if !hostname.trim().is_empty() {
        hostname.trim()
    } else if !device_name.trim().is_empty() {
        device_name.trim()
    } else {
        "arcadematrix"
    };
    format!("{}-data", base)
}

/// State shared between the engine (render thread) and its MQTT thread.
/// Lives only while the slot is active.
pub struct Shared {
    stop: AtomicBool,
    state: AtomicU8,
    /// Milliseconds after session start when the broker first accepted us (0 = not yet).
    connected_at_ms: AtomicU64,
    /// Bumped on every page update so the engine knows to rebuild.
    generation: AtomicU64,
    /// One slot per topic, in topic order.
    pages: Vec<ArcSwapOption<Page>>,
}

impl Shared {
    pub fn new(topics: usize) -> Self {
        Self {
            stop: AtomicBool::new(false),
            state: AtomicU8::new(STATE_CONNECTING),
            connected_at_ms: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            pages: (0..topics).map(|_| ArcSwapOption::empty()).collect(),
        }
    }

    /// Stores (or with `None`, clears) the page of topic `idx`.
    pub fn set_page(&self, idx: usize, page: Option<Page>) {
        if let Some(slot) = self.pages.get(idx) {
            slot.store(page.map(Arc::new));
            self.generation.fetch_add(1, Ordering::Release);
        }
    }

    pub fn page(&self, idx: usize) -> Option<Arc<Page>> {
        self.pages.get(idx).and_then(|s| s.load_full())
    }

    pub fn state(&self) -> u8 {
        self.state.load(Ordering::Acquire)
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
}

/// The MQTT side of one activation: connect, subscribe, parse into `shared`,
/// until `shared.stop` is set; then disconnect.
fn run_session(
    shared: Arc<Shared>,
    cfg: DataMqttConfig,
    client_id: String,
    topics: Vec<String>,
    started: Instant,
) {
    let mut options = MqttOptions::new(client_id.clone(), cfg.broker.trim(), cfg.port);
    options.set_keep_alive(Duration::from_secs(30));
    options.set_max_packet_size(MAX_PAYLOAD_BYTES + 1024, 1024);
    if !cfg.user.is_empty() {
        options.set_credentials(cfg.user.clone(), cfg.pass.clone());
    }
    info!(
        "MQTT Data: connecting to {}:{} as '{}' for {} topic(s)",
        cfg.broker,
        cfg.port,
        client_id,
        topics.len()
    );
    let (client, mut connection) = Client::new(options, 16);
    let mut backoff = Duration::from_millis(500);
    let mut warned_large = false;

    while !shared.stop.load(Ordering::Acquire) {
        match connection.recv_timeout(POLL_SLICE) {
            Ok(Ok(Event::Incoming(Packet::ConnAck(_)))) => {
                shared.state.store(STATE_CONNECTED, Ordering::Release);
                let ms = started.elapsed().as_millis().max(1) as u64;
                let _ = shared.connected_at_ms.compare_exchange(
                    0,
                    ms,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );
                backoff = Duration::from_millis(500);
                // Clean session: (re)subscribe on every connect so the broker
                // sends the retained snapshot.
                for t in &topics {
                    if let Err(e) = client.try_subscribe(t.as_str(), QoS::AtMostOnce) {
                        warn!("MQTT Data: subscribe to '{}' failed: {}", t, e);
                    }
                }
            }
            Ok(Ok(Event::Incoming(Packet::Publish(p)))) => {
                let Some(idx) = topics.iter().position(|t| *t == p.topic) else {
                    continue;
                };
                if p.payload.len() > MAX_PAYLOAD_BYTES {
                    if !warned_large {
                        warned_large = true;
                        warn!(
                            "MQTT Data: payload on '{}' too large ({} > {} bytes)",
                            p.topic,
                            p.payload.len(),
                            MAX_PAYLOAD_BYTES
                        );
                    }
                    continue;
                }
                // Empty payload = retained message deleted: the page goes back to NO DATA.
                // Anything that isn't UTF-8 JSON shows UNSUPPORTED.
                let page = match std::str::from_utf8(&p.payload) {
                    Ok(text) => parse_page(text),
                    Err(_) => Some(Page {
                        data: PageData::Unsupported,
                        seconds: DEFAULT_PAGE_SECONDS,
                    }),
                };
                shared.set_page(idx, page);
            }
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                if shared.connected_at_ms.load(Ordering::Acquire) == 0 {
                    shared.state.store(STATE_FAILED, Ordering::Release);
                } else {
                    shared.state.store(STATE_CONNECTING, Ordering::Release);
                }
                warn!(
                    "MQTT Data: connection error: {} (retry in {:?})",
                    e, backoff
                );
                // Sleep in short steps so deactivation never waits long.
                let until = Instant::now() + backoff;
                while Instant::now() < until && !shared.stop.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(50));
                }
                backoff = (backoff * 2).min(Duration::from_secs(5));
            }
            Err(_timeout) => {}
        }
    }
    let _ = client.try_disconnect();
    let _ = connection.recv_timeout(Duration::from_millis(50));
    info!("MQTT Data: disconnected");
}

/// One displayed page: topic index and, for weather topics, the day slide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRef {
    pub topic: usize,
    pub sub: usize,
}

/// Everything allocated for one activation; dropped on deactivate.
struct Session {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    started: Instant,
    seen_generation: u64,
    seen_wday: u32,
    pages: Vec<PageRef>,
    /// Seconds of each entry of `pages`.
    page_seconds: Vec<u32>,
    /// When the current cycle of all pages started (the engine's slot time).
    cycle_started: Instant,
    /// Weather slides per topic (empty for non-weather topics).
    weather: Vec<Vec<DayForecast>>,
    current: usize,
    page_started: Instant,
    /// The current weather slide, pre-drawn (rebuilt only when it changes).
    weather_image: Option<(PageRef, u64, image::RgbImage)>,
    axis_scratch: (String, String),
}

impl Session {
    fn stop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}

/// What the current page shows when it has no data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    Connecting,
    NoConnection,
    NoData,
    /// The payload's `type` is missing or unknown, or it isn't JSON.
    Unsupported,
}

/// Which notice a page without data shows, from the connection state.
pub fn notice_for(state: u8, connected_for: Option<Duration>) -> Notice {
    match (state, connected_for) {
        (STATE_FAILED, None) => Notice::NoConnection,
        (_, Some(d)) if d >= NO_DATA_AFTER => Notice::NoData,
        _ => Notice::Connecting,
    }
}

/// Draws a notice centred in the 5x7 font.
pub fn draw_notice(m: &mut dyn IDrawingSurface, notice: Notice, lang: Lang) {
    let (text, color) = match notice {
        Notice::Connecting => (i18n::connecting_label(lang), NOTICE_COLOR),
        Notice::NoConnection => (i18n::no_connection_label(lang), NO_CONNECTION_COLOR),
        Notice::NoData => (i18n::no_data_label(lang), NOTICE_COLOR),
        Notice::Unsupported => (i18n::unsupported_label(lang), UNSUPPORTED_COLOR),
    };
    let (w, h) = (m.width() as i32, m.height() as i32);
    let x = ((w - measure_text(text)) / 2).max(0);
    let y = ((h - 7) / 2).max(0);
    m.clear();
    draw_text_scaled(m, text, x, y, 0, w, 0, h, 1, color);
}

/// Builds the page list: one page per topic, a weather topic expanding to one
/// page per slide (at least one).
pub fn build_pages(weather: &[Vec<DayForecast>], topics: usize) -> Vec<PageRef> {
    let mut pages = Vec::with_capacity(topics + 4);
    for topic in 0..topics {
        let n = weather.get(topic).map(|w| w.len()).unwrap_or(0).max(1);
        for sub in 0..n {
            pages.push(PageRef { topic, sub });
        }
    }
    pages
}

fn lang_from_code(code: &str) -> Lang {
    let c = code.trim();
    if c.eq_ignore_ascii_case("en") {
        Lang::En
    } else if c.eq_ignore_ascii_case("es") {
        Lang::Es
    } else {
        Lang::Fr
    }
}

fn weekday_now() -> u32 {
    use chrono::Datelike;
    chrono::Local::now().weekday().num_days_from_sunday()
}

/// Seconds per page for the page list: the payload's `seconds`, or the
/// default for a topic that hasn't sent anything yet.
pub fn page_durations(pages: &[PageRef], seconds_of_topic: impl Fn(usize) -> u32) -> Vec<u32> {
    pages.iter().map(|p| seconds_of_topic(p.topic)).collect()
}

pub struct MqttDataEngine {
    topics: Vec<String>,
    lang: Lang,
    lang_code: String,
    active: bool,
    session: Option<Session>,
}

impl Default for MqttDataEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MqttDataEngine {
    pub fn new() -> Self {
        Self {
            topics: Vec::new(),
            lang: Lang::En,
            lang_code: "en".to_string(),
            active: false,
            session: None,
        }
    }

    fn read_config(&mut self, config: &dyn EngineConfig) {
        self.topics = parse_topics(&config.get_string("topics", ""));
    }

    /// Whether a session (thread, client, buffers) currently exists.
    pub fn has_session(&self) -> bool {
        self.session.is_some()
    }

    /// Starts the connection for this activation (first update after activate).
    fn start_session(&mut self, ctx: &EngineContext) {
        let (cfg, client_id) = {
            let s = ctx.config.settings.read();
            (
                s.data_mqtt.clone(),
                data_client_id(&s.wifi.hostname, &s.mqtt.device_name),
            )
        };
        let shared = Arc::new(Shared::new(self.topics.len()));
        let started = Instant::now();
        let thread = if cfg.broker.trim().is_empty() || self.topics.is_empty() {
            // Nothing to connect to: show NO CONNECTION straight away.
            shared.state.store(STATE_FAILED, Ordering::Release);
            None
        } else {
            let (sh, topics) = (Arc::clone(&shared), self.topics.clone());
            std::thread::Builder::new()
                .name("mqttdata".to_string())
                .spawn(move || run_session(sh, cfg, client_id, topics, started))
                .map_err(|e| warn!("MQTT Data: failed to start MQTT thread: {}", e))
                .ok()
        };
        self.session = Some(Session {
            shared,
            thread,
            started,
            seen_generation: u64::MAX,
            seen_wday: u32::MAX,
            pages: build_pages(&[], self.topics.len()),
            page_seconds: vec![DEFAULT_PAGE_SECONDS; self.topics.len()],
            cycle_started: Instant::now(),
            weather: vec![Vec::new(); self.topics.len()],
            current: 0,
            page_started: Instant::now(),
            weather_image: None,
            axis_scratch: (String::with_capacity(16), String::with_capacity(16)),
        });
    }

    /// The page currently shown (for tests and diagnostics).
    pub fn current_page(&self) -> Option<PageRef> {
        self.session
            .as_ref()
            .and_then(|s| s.pages.get(s.current).copied())
    }

    /// Test hook: the session's shared state (to inject pages without a broker).
    pub fn shared(&self) -> Option<Arc<Shared>> {
        self.session.as_ref().map(|s| Arc::clone(&s.shared))
    }
}

impl Engine for MqttDataEngine {
    fn initialize(
        &mut self,
        ctx: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        self.read_config(config);
        self.lang_code = ctx.config.settings.read().system.lang.clone();
        self.lang = lang_from_code(&self.lang_code);
        Ok(())
    }

    fn activate(&mut self) {
        if self.active {
            // Re-activated without leaving the screen (the rotation came back to
            // this slot, e.g. it is the only one): keep the connection and
            // start a new cycle at page 1.
            if let Some(s) = self.session.as_mut() {
                s.current = 0;
                s.page_started = Instant::now();
                s.cycle_started = Instant::now();
                s.weather_image = None;
            }
            return;
        }
        self.active = true;
        self.session = None; // a fresh session starts on the next update()
    }

    fn update(&mut self, ctx: &mut EngineContext) {
        if !self.active {
            return;
        }
        let lang_code = ctx.config.settings.read().system.lang.clone();
        if lang_code != self.lang_code {
            self.lang = lang_from_code(&lang_code);
            self.lang_code = lang_code;
            if let Some(s) = self.session.as_mut() {
                s.seen_generation = u64::MAX; // relabel weather slides
            }
        }
        if self.session.is_none() {
            self.start_session(ctx);
        }
        let lang_code = self.lang_code.clone();
        let Some(s) = self.session.as_mut() else {
            return;
        };

        // Rebuild the page list when data (or the day) changed.
        let generation = s.shared.generation();
        let wday = weekday_now();
        if generation != s.seen_generation || wday != s.seen_wday {
            s.seen_generation = generation;
            s.seen_wday = wday;
            for (i, slides) in s.weather.iter_mut().enumerate() {
                *slides = match s.shared.page(i).as_deref() {
                    Some(Page {
                        data: PageData::Weather(w),
                        ..
                    }) => weather::weather_pages(w, &lang_code, wday),
                    _ => Vec::new(),
                };
            }
            let current = s.pages.get(s.current).copied();
            s.pages = build_pages(&s.weather, s.weather.len());
            let shared = &s.shared;
            s.page_seconds = page_durations(&s.pages, |t| {
                shared
                    .page(t)
                    .map(|p| p.seconds)
                    .unwrap_or(DEFAULT_PAGE_SECONDS)
            });
            s.current = current
                .and_then(|c| s.pages.iter().position(|p| *p == c))
                .unwrap_or(0);
            s.weather_image = None;
        }

        // Cycle pages, each for its own payload's seconds.
        let page_time = Duration::from_secs(
            s.page_seconds
                .get(s.current)
                .copied()
                .unwrap_or(DEFAULT_PAGE_SECONDS) as u64,
        );
        if s.pages.len() > 1 && s.page_started.elapsed() >= page_time {
            s.current = (s.current + 1) % s.pages.len();
            s.page_started = Instant::now();
        }

        // Pre-draw the current weather slide off the render path.
        if let Some(page) = s.pages.get(s.current).copied() {
            if let Some(slide) = s.weather.get(page.topic).and_then(|w| w.get(page.sub)) {
                let fresh = matches!(&s.weather_image,
                    Some((p, g, _)) if *p == page && *g == s.seen_generation);
                if !fresh {
                    let (w, h) = (ctx.surface.width(), ctx.surface.height());
                    let mut img = image::RgbaImage::new(w, h);
                    draw_weather_page(&mut img, slide, 0, w, h, 0, 0);
                    let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
                    s.weather_image = Some((page, s.seen_generation, rgb));
                }
            }
        }
    }

    fn render(&mut self, ctx: &mut EngineContext) {
        let lang = self.lang;
        let Some(s) = self.session.as_mut() else {
            draw_notice(ctx.surface, Notice::Connecting, lang);
            return;
        };
        let Some(page) = s.pages.get(s.current).copied() else {
            draw_notice(ctx.surface, Notice::NoConnection, lang);
            return;
        };
        let data = s.shared.page(page.topic);
        match data.as_deref().map(|p| &p.data) {
            Some(PageData::Graph(g)) => graph::render_graph_with(
                ctx.surface,
                Some(g),
                g.show_header,
                i18n::no_data_label(lang),
                &mut s.axis_scratch,
            ),
            Some(PageData::Table(t)) => table::render_table(
                ctx.surface,
                Some(t),
                t.show_title,
                i18n::no_data_label(lang),
            ),
            Some(PageData::Unsupported) => draw_notice(ctx.surface, Notice::Unsupported, lang),
            Some(PageData::Weather(_)) => {
                if let Some((_, _, img)) = &s.weather_image {
                    ctx.surface.draw_image(img, 0, 0);
                } else {
                    draw_notice(ctx.surface, Notice::NoData, lang);
                }
            }
            None => {
                let at = s.shared.connected_at_ms.load(Ordering::Acquire);
                let connected_for = (at > 0).then(|| {
                    s.started
                        .elapsed()
                        .saturating_sub(Duration::from_millis(at))
                });
                draw_notice(
                    ctx.surface,
                    notice_for(s.shared.state(), connected_for),
                    lang,
                );
            }
        }
    }

    /// The engine decides when its slot ends: after one full cycle of its pages.
    fn self_paced(&self) -> bool {
        true
    }

    /// True once the sum of the current pages' seconds has elapsed since the
    /// cycle started.
    fn is_finished(&self) -> bool {
        let Some(s) = self.session.as_ref() else {
            return false;
        };
        let total: u64 = s.page_seconds.iter().map(|&x| x as u64).sum();
        // No pages at all (no topics): behave like one default page.
        let total = if total == 0 {
            DEFAULT_PAGE_SECONDS as u64
        } else {
            total
        };
        s.cycle_started.elapsed() >= Duration::from_secs(total)
    }

    fn deactivate(&mut self) {
        self.active = false;
        // Stops and joins the MQTT thread, disconnects, frees every buffer.
        self.session = None;
    }

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        let before = self.topics.clone();
        self.read_config(config);
        if before != self.topics {
            // New topics: reconnect on the next update.
            self.session = None;
        }
    }
}

#[distributed_slice(crate::core::registry::ENGINES)]
fn register_mqttdata_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "mqttdata",
            name: "MQTT Data",
            category: "info",
            version: crate::core::build_info::VERSION,
        },
        capabilities: Capabilities::default(),
        requirements: Requirements {
            needs_network: true,
            ..Default::default()
        },
        available: true,
        unavailable_reason: None,
        schema: ConfigSchema {
            fields: vec![
                ConfigField {
                    id: "topics",
                    field_type: ConfigType::String,
                    label: "MQTT Topics",
                    description: "Up to 6 full MQTT topics, comma separated. Each payload sets its own type and time on screen (see the MQTT Data contract).",
                    default_value: "",
                    validation_policy: ValidationPolicy::Accept,
                    ..Default::default()
                },
            ],
        },
        factory: || Box::new(MqttDataEngine::new()),
    }
}

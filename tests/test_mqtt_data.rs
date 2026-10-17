//! MQTT Data engine: payload contract, topics, notices, page list, timing and the
//! activate / deactivate lifecycle (nothing may stay resident).

use arcadematrix::core::config::{Config, ConfigSettings, DataMqttConfig};
use arcadematrix::core::engine_contract::{Engine, EngineContext, HashConfig};
use arcadematrix::core::i18n::Lang;
use arcadematrix::core::matrix::MockSurface;
use arcadematrix::core::registry::EngineRegistry;
use arcadematrix::engines::mqtt_data::payload::{parse_page, PageData, DEFAULT_PAGE_SECONDS};
use arcadematrix::engines::mqtt_data::{
    build_pages, data_client_id, draw_notice, notice_for, parse_topics, MqttDataEngine, Notice,
    PageRef, NO_DATA_AFTER, STATE_CONNECTED, STATE_CONNECTING, STATE_FAILED,
};
use std::collections::HashMap;
use std::time::{Duration, Instant};

fn px(m: &MockSurface, x: u32, y: u32) -> (u8, u8, u8) {
    let p = m.canvas.get_pixel(x, y);
    (p[0], p[1], p[2])
}

fn bbox(m: &MockSurface, c: (u8, u8, u8)) -> Option<(u32, u32, u32, u32)> {
    let mut b: Option<(u32, u32, u32, u32)> = None;
    for y in 0..m.canvas.height() {
        for x in 0..m.canvas.width() {
            if px(m, x, y) == c {
                b = Some(match b {
                    None => (x, y, x, y),
                    Some((a, bb, cc, d)) => (a.min(x), bb.min(y), cc.max(x), d.max(y)),
                });
            }
        }
    }
    b
}

// ---------------------------------------------------------------------------
// Payload type
// ---------------------------------------------------------------------------

#[test]
fn type_is_required_and_selects_the_renderer() {
    let kind = |json: &str| parse_page(json).map(|p| p.data);
    assert!(matches!(
        kind(r#"{"type":"graph","series":[{"data":[1]}]}"#),
        Some(PageData::Graph(_))
    ));
    assert!(matches!(
        kind(r#"{"type":"table","tiles":[{"value":"1"}]}"#),
        Some(PageData::Table(_))
    ));
    assert!(matches!(
        kind(r#"{"type":" Weather ","current":{"temp":70}}"#),
        Some(PageData::Weather(_))
    ));
    // value = a one-tile table without a title.
    let Some(PageData::Table(v)) =
        kind(r##"{"type":"value","value":"89","unit":"°F","label":"OUTSIDE","color":"#FF6040"}"##)
    else {
        panic!("value page");
    };
    assert_eq!(v.tiles.len(), 1);
    assert!(!v.show_title);
    assert_eq!(v.tiles[0].value, "89");
    assert_eq!(v.tiles[0].color, (0xFF, 0x60, 0x40));
    assert_eq!(v.tiles[0].label_color, (0x80, 0x80, 0x80));
    // A numeric value is accepted too.
    assert!(matches!(
        kind(r#"{"type":"value","value":21.5}"#),
        Some(PageData::Table(_))
    ));
    // No shape fallback any more: missing or unknown type -> UNSUPPORTED.
    for bad in [
        r#"{"series":[{"data":[1]}]}"#,
        r#"{"tiles":[{"value":"1"}]}"#,
        r#"{"type":"values","tiles":[{"value":"1"}]}"#,
        r#"{"type":"future"}"#,
        r#"{"type":"graph","tiles":[]}"#,
        r#"{"type":"value"}"#,
        "not json",
        "[1,2,3]",
    ] {
        assert_eq!(kind(bad), Some(PageData::Unsupported), "{}", bad);
    }
    // Empty payload = cleared retained message -> no page (NO DATA).
    assert!(parse_page("").is_none());
    assert!(parse_page("  ").is_none());
}

#[test]
fn seconds_and_show_flags_come_from_the_payload() {
    let secs = |json: &str| parse_page(json).unwrap().seconds;
    assert_eq!(
        secs(r#"{"type":"value","value":"1"}"#),
        DEFAULT_PAGE_SECONDS
    );
    assert_eq!(secs(r#"{"type":"value","value":"1","seconds":25}"#), 25);
    assert_eq!(secs(r#"{"type":"value","value":"1","seconds":1}"#), 3); // minimum
    assert_eq!(
        secs(r#"{"type":"value","value":"1","seconds":99999}"#),
        3600
    );
    assert_eq!(secs(r#"{"type":"nope","seconds":20}"#), 20); // UNSUPPORTED honours it
    let Some(PageData::Table(t)) =
        parse_page(r#"{"type":"table","title":"HOME","show_title":false,"tiles":[{"value":"1"}]}"#)
            .map(|p| p.data)
    else {
        panic!()
    };
    assert!(!t.show_title);
    let Some(PageData::Graph(g)) =
        parse_page(r#"{"type":"graph","show_header":false,"series":[]}"#).map(|p| p.data)
    else {
        panic!()
    };
    assert!(!g.show_header);
    // Newer contract versions and unknown fields still render what is known.
    assert!(matches!(
        parse_page(r#"{"v":7,"type":"value","value":"1","sparkle":true}"#).map(|p| p.data),
        Some(PageData::Table(_))
    ));
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[test]
fn topics_setting_is_split_and_capped() {
    assert_eq!(
        parse_topics(" a/one, a/two ;a/three\na/four,,"),
        vec!["a/one", "a/two", "a/three", "a/four"]
    );
    let many: Vec<String> = (0..9).map(|i| format!("t/{}", i)).collect();
    assert_eq!(parse_topics(&many.join(",")).len(), 6);
    assert!(parse_topics("").is_empty());
}

#[test]
fn data_mqtt_is_credentials_only() {
    let d = DataMqttConfig::default();
    assert_eq!((d.broker.as_str(), d.port), ("", 1883));
    // Old keys (enabled / topic_prefix) from the data-feed design are ignored.
    let s: ConfigSettings = serde_json::from_str(
        r#"{"data_mqtt":{"enabled":true,"broker":"10.0.0.2","port":1884,"user":"u","pass":"p","topic_prefix":"x"}}"#,
    )
    .unwrap();
    assert_eq!(s.data_mqtt.broker, "10.0.0.2");
    assert_eq!(s.data_mqtt.port, 1884);
    let out = serde_json::to_value(&s).unwrap();
    assert!(out["data_mqtt"].get("enabled").is_none());
    assert!(out["data_mqtt"].get("topic_prefix").is_none());
    assert_eq!(data_client_id("clock1", "x"), "clock1-data");
    assert_eq!(data_client_id("", ""), "arcadematrix-data");
}

#[test]
fn engine_is_registered_with_topics_only() {
    let d = EngineRegistry::get_descriptor("mqttdata").expect("descriptor");
    assert_eq!(d.metadata.name, "MQTT Data");
    let ids: Vec<&str> = d.schema.fields.iter().map(|f| f.id).collect();
    assert_eq!(ids, vec!["topics"]);
    assert_eq!(EngineRegistry::engine_name_to_id("mqttdata"), 16);
    assert!(EngineRegistry::get_descriptor("homeassistant").is_none());
    assert!(EngineRegistry::get_descriptor("graph").is_none());
    assert!(EngineRegistry::get_descriptor("values").is_none());
    let w = EngineRegistry::get_descriptor("weather").unwrap();
    assert!(w
        .schema
        .fields
        .iter()
        .all(|f| f.id != "source" && f.id != "topic"));
}

// ---------------------------------------------------------------------------
// Notices and pages
// ---------------------------------------------------------------------------

#[test]
fn notice_follows_connection_state() {
    assert_eq!(notice_for(STATE_CONNECTING, None), Notice::Connecting);
    assert_eq!(notice_for(STATE_FAILED, None), Notice::NoConnection);
    assert_eq!(
        notice_for(STATE_CONNECTED, Some(Duration::from_millis(500))),
        Notice::Connecting
    );
    assert_eq!(
        notice_for(STATE_CONNECTED, Some(NO_DATA_AFTER)),
        Notice::NoData
    );
    // Reconnecting after a drop with data pending: NO DATA, not NO CONNECTION.
    assert_eq!(
        notice_for(STATE_CONNECTING, Some(Duration::from_secs(10))),
        Notice::NoData
    );
}

#[test]
fn notices_are_centred_on_128x32() {
    let grey = (0x78, 0x78, 0x78);
    let red = (0xB4, 0x50, 0x50);
    let mut m = MockSurface::new(128, 32);
    draw_notice(&mut m, Notice::Connecting, Lang::En);
    let (x0, y0, x1, y1) = bbox(&m, grey).unwrap();
    // "CONNECTING" = 59 px at x=34, rows 12..18.
    assert_eq!((x0, x1, y0, y1), (34, 34 + 58, 12, 18));
    draw_notice(&mut m, Notice::NoConnection, Lang::En);
    let (x0, _, x1, _) = bbox(&m, red).unwrap();
    assert_eq!((x0, x1), (25, 25 + 76));
    draw_notice(&mut m, Notice::NoConnection, Lang::Fr);
    let (x0, _, x1, _) = bbox(&m, red).unwrap();
    assert_eq!((x0, x1), (16, 16 + 94));
    let amber = (0xC0, 0x80, 0x00);
    draw_notice(&mut m, Notice::Unsupported, Lang::En);
    let (x0, y0, x1, _) = bbox(&m, amber).unwrap();
    assert_eq!((x0, y0, x1), (31, 12, 31 + 64)); // "UNSUPPORTED" = 65 px
    draw_notice(&mut m, Notice::Unsupported, Lang::Fr);
    let (x0, _, x1, _) = bbox(&m, amber).unwrap();
    assert_eq!((x0, x1), (10, 10 + 106)); // "NON PRIS EN CHARGE" = 107 px
}

#[test]
fn weather_topic_expands_to_its_slides() {
    use arcadematrix::api::DayForecast;
    let slide = |l: &str| DayForecast {
        label: l.into(),
        label_long: String::new(),
        temp: String::new(),
        temp_min: String::new(),
        temp_max: String::new(),
        condition: String::new(),
        condition_long: String::new(),
        icon: String::new(),
        is_now: false,
        now_line2_short: String::new(),
    };
    let weather = vec![
        vec![slide("NOW"), slide("TODAY"), slide("TOM.")],
        vec![],
        vec![],
    ];
    let pages = build_pages(&weather, 3);
    assert_eq!(
        pages,
        vec![
            PageRef { topic: 0, sub: 0 },
            PageRef { topic: 0, sub: 1 },
            PageRef { topic: 0, sub: 2 },
            PageRef { topic: 1, sub: 0 },
            PageRef { topic: 2, sub: 0 },
        ]
    );
    // Topics without data still get one page (it shows a notice).
    assert_eq!(build_pages(&[], 2).len(), 2);
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

fn test_config(broker: &str) -> (Config, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "am_ha_{}_{}",
        std::process::id(),
        broker.replace([':', '.'], "_")
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let config = Config::new(dir.join("config.json"));
    config.settings.write().data_mqtt.broker = broker.to_string();
    config.settings.write().system.lang = "en".to_string();
    (config, dir)
}

fn instance(topics: &str, _unused: &str) -> HashMap<String, String> {
    let mut cfg = HashMap::new();
    cfg.insert("topics".to_string(), topics.to_string());
    cfg
}

#[test]
fn no_broker_shows_no_connection_and_starts_no_thread() {
    let (config, dir) = test_config("");
    let cfg = instance("a/b", "10");
    let mut engine = MqttDataEngine::new();
    let mut m = MockSurface::new(128, 32);
    let mut ctx = EngineContext {
        surface: &mut m,
        config: &config,
    };
    engine
        .initialize(&mut ctx, &HashConfig { data: &cfg })
        .unwrap();
    assert!(!engine.has_session(), "nothing allocated before activation");
    engine.activate();
    engine.update(&mut ctx);
    assert!(engine.has_session());
    engine.render(&mut ctx);
    assert!(bbox(&m, (0xB4, 0x50, 0x50)).is_some(), "NO CONNECTION");
    let mut ctx = EngineContext {
        surface: &mut m,
        config: &config,
    };
    engine.deactivate();
    assert!(!engine.has_session());
    engine.render(&mut ctx); // must not panic after deactivate
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn twenty_activations_free_everything() {
    // A broker that refuses connections: the thread starts, fails, and must
    // still stop and drop all its state on every deactivate.
    let (config, dir) = test_config("127.0.0.1");
    config.settings.write().data_mqtt.port = 1; // nothing listens on port 1
    let cfg = instance("a/one,a/two", "10");
    let mut engine = MqttDataEngine::new();
    let mut m = MockSurface::new(128, 32);
    {
        let mut ctx = EngineContext {
            surface: &mut m,
            config: &config,
        };
        engine
            .initialize(&mut ctx, &HashConfig { data: &cfg })
            .unwrap();
    }
    for _ in 0..20 {
        engine.activate();
        {
            let mut ctx = EngineContext {
                surface: &mut m,
                config: &config,
            };
            engine.update(&mut ctx);
            engine.render(&mut ctx);
        }
        let shared = engine.shared().expect("session");
        let weak = std::sync::Arc::downgrade(&shared);
        drop(shared);
        let t = Instant::now();
        engine.deactivate();
        assert!(
            t.elapsed() < Duration::from_secs(2),
            "deactivate must not hang"
        );
        assert!(weak.upgrade().is_none(), "thread and buffers released");
        assert!(!engine.has_session());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pages_cycle_on_their_own_seconds_and_engine_owns_the_slot() {
    let (config, dir) = test_config("");
    let cfg = instance("a/graph,a/table,a/odd", "");
    let mut engine = MqttDataEngine::new();
    let mut m = MockSurface::new(128, 32);
    let mut ctx = EngineContext {
        surface: &mut m,
        config: &config,
    };
    engine
        .initialize(&mut ctx, &HashConfig { data: &cfg })
        .unwrap();
    assert!(engine.self_paced());
    engine.activate();
    engine.update(&mut ctx);
    let shared = engine.shared().unwrap();
    shared.set_page(
        0,
        parse_page(
            r##"{"type":"graph","seconds":1,"max":1,"series":[{"color":"#FF0000","data":[1]}]}"##,
        )
        .map(|mut p| {
            p.seconds = 1;
            p
        }),
    );
    shared.set_page(
        1,
        parse_page(
            r##"{"type":"table","seconds":1,"tiles":[{"label":"A","value":"7","color":"#00FF00"}]}"##,
        )
        .map(|mut p| {
            p.seconds = 1;
            p
        }),
    );
    shared.set_page(
        2,
        parse_page(r#"{"type":"nope","seconds":1}"#).map(|mut p| {
            p.seconds = 1;
            p
        }),
    );
    engine.update(&mut ctx);
    assert_eq!(engine.current_page(), Some(PageRef { topic: 0, sub: 0 }));
    ctx.surface.clear();
    engine.render(&mut ctx);
    drop(ctx);
    assert_eq!(px(&m, 64, 31), (255, 0, 0), "graph page drawn");
    assert!(!engine.is_finished(), "slot = 1 + 1 + 1 s");
    std::thread::sleep(Duration::from_millis(1100));
    let mut ctx = EngineContext {
        surface: &mut m,
        config: &config,
    };
    engine.update(&mut ctx);
    assert_eq!(engine.current_page(), Some(PageRef { topic: 1, sub: 0 }));
    ctx.surface.clear();
    engine.render(&mut ctx);
    drop(ctx);
    assert!(bbox(&m, (0, 255, 0)).is_some(), "table page drawn");
    std::thread::sleep(Duration::from_millis(1100));
    let mut ctx = EngineContext {
        surface: &mut m,
        config: &config,
    };
    engine.update(&mut ctx);
    assert_eq!(engine.current_page(), Some(PageRef { topic: 2, sub: 0 }));
    engine.render(&mut ctx);
    drop(ctx);
    assert!(bbox(&m, (0xC0, 0x80, 0x00)).is_some(), "UNSUPPORTED notice");
    std::thread::sleep(Duration::from_millis(1100));
    assert!(engine.is_finished(), "one full cycle done");
    // The rotation re-activates the same slot (e.g. it is the only one): the
    // connection is kept and a new cycle starts at page 1.
    engine.activate();
    assert!(engine.has_session());
    assert!(!engine.is_finished());
    assert_eq!(engine.current_page(), Some(PageRef { topic: 0, sub: 0 }));
    // An empty payload (retained message deleted) clears a page -> NO DATA.
    shared.set_page(1, parse_page(""));
    assert!(shared.page(1).is_none());
    drop(shared);
    engine.deactivate();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn value_page_matches_a_single_tile_table() {
    use arcadematrix::engines::mqtt_data::table::render_table;
    let Some(PageData::Table(v)) = parse_page(
        r##"{"type":"value","value":"89","unit":"°F","label":"OUTSIDE","color":"#FF6040"}"##,
    )
    .map(|p| p.data) else {
        panic!()
    };
    let Some(PageData::Table(t)) = parse_page(
        r##"{"type":"table","show_title":false,"tiles":[{"label":"OUTSIDE","value":"89","unit":"°F","color":"#FF6040"}]}"##,
    )
    .map(|p| p.data) else {
        panic!()
    };
    for (w, h) in [(128u32, 32u32), (256, 64)] {
        let (mut a, mut b) = (MockSurface::new(w, h), MockSurface::new(w, h));
        render_table(&mut a, Some(&v), v.show_title, "");
        render_table(&mut b, Some(&t), t.show_title, "");
        assert_eq!(a.canvas, b.canvas, "{}x{}", w, h);
    }
}

// ---------------------------------------------------------------------------
// Preview renders: `MQTTDATA_PREVIEW_DIR=out cargo test --test test_mqtt_data -- --ignored`
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn render_notice_previews() {
    let Ok(dir) = std::env::var("MQTTDATA_PREVIEW_DIR") else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    for (name, notice) in [
        ("mqttdata_connecting", Notice::Connecting),
        ("mqttdata_no_connection", Notice::NoConnection),
        ("mqttdata_no_data", Notice::NoData),
        ("mqttdata_unsupported", Notice::Unsupported),
    ] {
        for (w, h) in [(128u32, 32u32), (256, 64)] {
            let mut m = MockSurface::new(w, h);
            draw_notice(&mut m, notice, Lang::En);
            let big = image::imageops::resize(&m.canvas, w * 8, h * 8, image::imageops::Nearest);
            big.save(format!("{}/{}_{}x{}.png", dir, name, w, h))
                .unwrap();
        }
    }
    for (w, h) in [(128u32, 32u32), (256, 64)] {
        let save = |m: &MockSurface, name: &str| {
            let big = image::imageops::resize(&m.canvas, w * 8, h * 8, image::imageops::Nearest);
            big.save(format!("{}/{}_{}x{}.png", dir, name, w, h))
                .unwrap();
        };
        let mut m = MockSurface::new(w, h);
        draw_notice(&mut m, Notice::Unsupported, Lang::Fr);
        save(&m, "mqttdata_unsupported_fr");
        // The value page, with the payload the value blueprint publishes.
        use arcadematrix::engines::mqtt_data::table::render_table;
        if let Some(PageData::Table(v)) = parse_page(
            r##"{"v":1,"type":"value","value":"89","unit":"°F","label":"OUTSIDE","color":"#FF6040"}"##,
        )
        .map(|p| p.data)
        {
            let mut m = MockSurface::new(w, h);
            render_table(&mut m, Some(&v), v.show_title, "");
            save(&m, "mqttdata_value");
        }
    }
}

/// Runs the engine against a real broker and saves every page it shows:
/// `MQTTDATA_BROKER=host MQTTDATA_USER=u MQTTDATA_PASS=p MQTTDATA_TOPICS=a,b MQTTDATA_PREVIEW_DIR=out
///  cargo test --test test_mqtt_data live_broker -- --ignored`
#[test]
#[ignore]
fn live_broker_pages() {
    let (Ok(broker), Ok(dir)) = (
        std::env::var("MQTTDATA_BROKER"),
        std::env::var("MQTTDATA_PREVIEW_DIR"),
    ) else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    let topics = std::env::var("MQTTDATA_TOPICS").unwrap_or_default();
    for (w, h) in [(128u32, 32u32), (256, 64)] {
        let (config, tmp) = test_config(&broker);
        {
            let mut s = config.settings.write();
            s.data_mqtt.user = std::env::var("MQTTDATA_USER").unwrap_or_default();
            s.data_mqtt.pass = std::env::var("MQTTDATA_PASS").unwrap_or_default();
            s.wifi.hostname = "arcadematrix-mactest".into();
        }
        let cfg = instance(&topics, "3");
        let mut engine = MqttDataEngine::new();
        let mut m = MockSurface::new(w, h);
        let t0 = Instant::now();
        {
            let mut ctx = EngineContext {
                surface: &mut m,
                config: &config,
            };
            engine
                .initialize(&mut ctx, &HashConfig { data: &cfg })
                .unwrap();
            engine.activate();
            engine.update(&mut ctx);
            engine.render(&mut ctx);
        }
        let save = |m: &MockSurface, name: &str| {
            let big = image::imageops::resize(&m.canvas, w * 8, h * 8, image::imageops::Nearest);
            big.save(format!("{}/{}_{}x{}.png", dir, name, w, h))
                .unwrap();
        };
        save(&m, "live_00_first_frame");
        // Wait for the retained snapshot, then step through every page.
        std::thread::sleep(Duration::from_millis(1500));
        let mut seen = Vec::new();
        for _ in 0..80 {
            let mut ctx = EngineContext {
                surface: &mut m,
                config: &config,
            };
            engine.update(&mut ctx);
            let page = engine.current_page();
            ctx.surface.clear();
            engine.render(&mut ctx);
            drop(ctx);
            if let Some(p) = page {
                if !seen.contains(&p) {
                    seen.push(p);
                    save(&m, &format!("live_t{}_p{}", p.topic, p.sub));
                }
                if seen.len() > 1 && p == seen[0] {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        eprintln!("{}x{}: {} page(s) in {:?}", w, h, seen.len(), t0.elapsed());
        engine.deactivate();
        assert!(!engine.has_session());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

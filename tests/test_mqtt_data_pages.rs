//! MQTT Data engine: graph v2 features, table tiles, weather pages.

use arcadematrix::core::matrix::{IDrawingSurface, MockSurface};
use arcadematrix::engines::mqtt_data::graph::{render_graph, YScale};
use arcadematrix::engines::mqtt_data::payload::{
    parse_graph_payload, parse_table_payload, parse_weather_payload, MAX_PAYLOAD_BYTES,
};
use arcadematrix::engines::mqtt_data::table::{fit_value, render_table};
use arcadematrix::engines::mqtt_data::weather::weather_pages;
use arcadematrix::engines::renderers::tiny_font::{draw_tiny_text, tiny_text_width};
use arcadematrix::engines::renderers::weather_page::draw_weather_page;

fn px(m: &MockSurface, x: u32, y: u32) -> (u8, u8, u8) {
    let p = m.canvas.get_pixel(x, y);
    (p[0], p[1], p[2])
}

fn lit_in(
    m: &MockSurface,
    xs: std::ops::Range<u32>,
    ys: std::ops::Range<u32>,
    c: (u8, u8, u8),
) -> bool {
    xs.clone().any(|x| ys.clone().any(|y| px(m, x, y) == c))
}

/// The store has a single writer in production (the MQTT thread); parallel
/// tests must not publish at the same time or they can race for a free slot.
const RED: (u8, u8, u8) = (0xFF, 0x40, 0x20);
const BLUE: (u8, u8, u8) = (0x20, 0x60, 0xFF);

// ---------------------------------------------------------------------------
// Graph v2
// ---------------------------------------------------------------------------

#[test]
fn payload_limit_is_8k() {
    assert_eq!(MAX_PAYLOAD_BYTES, 8192);
}

#[test]
fn old_payloads_keep_v1_range() {
    let g = parse_graph_payload(r#"{"max":6,"series":[{"data":[1,2]}]}"#).unwrap();
    assert_eq!((g.lo, g.hi), (0.0, 6.0));
    let auto = parse_graph_payload(r#"{"series":[{"data":[1,3]},{"data":[2,2]}]}"#).unwrap();
    assert_eq!((auto.lo, auto.hi), (0.0, 5.0));
}

#[test]
fn explicit_min_max_and_auto_ranges() {
    let g = parse_graph_payload(r#"{"min":68,"max":90,"series":[{"style":"line","data":[70]}]}"#)
        .unwrap();
    assert_eq!((g.lo, g.hi), (68.0, 90.0));
    // max <= min -> auto max.
    let g = parse_graph_payload(r#"{"min":10,"max":5,"series":[{"data":[20]}]}"#).unwrap();
    assert_eq!((g.lo, g.hi), (10.0, 20.0));
    // Lines: data range padded by max(5 % of span, 0.5).
    let g = parse_graph_payload(r#"{"series":[{"style":"line","data":[70,null,90]}]}"#).unwrap();
    assert_eq!((g.lo, g.hi), (69.0, 91.0));
    let g = parse_graph_payload(r#"{"series":[{"style":"line","data":[72,72.2]}]}"#).unwrap();
    assert!((g.lo - 71.5).abs() < 1e-4 && (g.hi - 72.7).abs() < 1e-4);
    // Negative bars: range goes below 0.
    let g = parse_graph_payload(r#"{"series":[{"data":[2,-3]},{"data":[1,-1]}]}"#).unwrap();
    assert_eq!((g.lo, g.hi), (-4.0, 3.0));
}

#[test]
fn ninety_six_bands_render_and_extra_bands_keep_newest() {
    use arcadematrix::engines::mqtt_data::payload::MAX_BANDS;
    assert_eq!(MAX_BANDS, 96);
    // One band per 15 min slot, each with its own colour (cooling intensity).
    let bands: Vec<String> = (0..96)
        .map(|i| {
            format!(
                r##"{{"from":{},"to":{},"color":"#{:02X}0000"}}"##,
                i,
                i + 1,
                i + 100
            )
        })
        .collect();
    let json = format!(
        r#"{{"slots":96,"min":0,"max":10,"series":[{{"style":"line","data":[]}}],"bands":[{}]}}"#,
        bands.join(",")
    );
    let g = parse_graph_payload(&json).unwrap();
    assert_eq!(g.bands.len(), 96);
    for (w, h) in [(128u32, 32u32), (256, 64)] {
        let mut m = MockSurface::new(w, h);
        render_graph(&mut m, Some(&g), false, "");
        // Every column is covered by its own slot's band colour.
        for x in 0..w {
            let slot = (x as usize * 96) / w as usize;
            assert_eq!(
                px(&m, x, h - 1),
                ((slot + 100) as u8, 0, 0),
                "{}x{} col {}",
                w,
                h,
                x
            );
        }
    }

    // 100 bands, sent out of order: the 96 with the highest `from` survive.
    let many: Vec<String> = (0..100)
        .rev()
        .map(|i| format!(r##"{{"from":{},"to":{},"color":"#101010"}}"##, i, i + 1))
        .collect();
    let g = parse_graph_payload(&format!(
        r#"{{"slots":100,"series":[{{"data":[]}}],"bands":[{}]}}"#,
        many.join(",")
    ))
    .unwrap();
    assert_eq!(g.bands.len(), 96);
    assert_eq!(g.bands.first().unwrap().from, 4);
    assert_eq!(g.bands.last().unwrap().from, 99);
}

const LEGEND_CASE: &str = r##"{"title":"DOWNSTAIRS COOL","summary":"76°","summary_color":"#40A0FF",
  "min":68,"max":90,"yaxis":true,
  "series":[{"style":"line","color":"#2060FF","data":[90,90]}],
  "legend":[{"text":"INSIDE","color":"#40A0FF"},{"text":"OUTSIDE","color":"#FF4020"},
            {"text":"COOLING","color":"#5080C0"}]}"##;

#[test]
fn legend_row_and_summary_colour_on_tall_panels() {
    let g = parse_graph_payload(LEGEND_CASE).unwrap();
    assert_eq!(g.legend.len(), 3);
    assert_eq!(g.summary_color, Some((0x40, 0xA0, 0xFF)));
    let mut m = MockSurface::new(256, 64);
    render_graph(&mut m, Some(&g), true, "");
    // Legend at the bottom: px0 = 8 (labels "90"/"68"),
    // INSIDE x 8..30, OUTSIDE 39..65, COOLING 74..100, rows 58..62; row 63 blank.
    let blue = (0x40, 0xA0, 0xFF);
    let xs: Vec<u32> = (0..256)
        .filter(|&x| (58..63).any(|y| px(&m, x, y) == blue))
        .collect();
    assert_eq!((xs[0], *xs.last().unwrap()), (8, 30));
    assert!(lit_in(&m, 39..66, 58..63, (0xFF, 0x40, 0x20)));
    assert!(lit_in(&m, 74..101, 58..63, (0x50, 0x80, 0xC0)));
    assert!((0..256).all(|x| px(&m, x, 63) == (0, 0, 0)));
    // Summary uses summary_color (on the header rows, right side).
    assert!(lit_in(&m, 200..256, 0..14, blue));
    // Plot is back to rows 15..56: value 90 on row 15, axis max on 15..19, min on 52..56.
    assert_eq!(px(&m, 8, 15), (0x20, 0x60, 0xFF));
    assert!(lit_in(&m, 0..7, 15..20, (0x70, 0x70, 0x70)));
    assert!(lit_in(&m, 0..7, 52..57, (0x70, 0x70, 0x70)));
    assert!(!lit_in(&m, 0..7, 57..64, (0x70, 0x70, 0x70)));
}

#[test]
fn legend_ignored_on_32px_panels() {
    let with = parse_graph_payload(LEGEND_CASE).unwrap();
    let mut without = with.clone();
    without.legend.clear();
    let (mut a, mut b) = (MockSurface::new(128, 32), MockSurface::new(128, 32));
    render_graph(&mut a, Some(&with), true, "");
    render_graph(&mut b, Some(&without), true, "");
    assert_eq!(a.canvas, b.canvas);
}

#[test]
fn too_wide_legend_drops_items_from_the_end() {
    let g = parse_graph_payload(
        r##"{"series":[{"data":[]}],"legend":[
          {"text":"AAAAAAAAAAAAAAAAAAAA","color":"#FF0000"},
          {"text":"BBBBBBBBBBBBBBBBBBBB","color":"#00FF00"},
          {"text":"CCCCCCCCCCCCCCCCCCCC","color":"#0000FF"},
          {"text":"DD","color":"#FFFF00"},
          {"text":"IGNORED","color":"#FFFFFF"}]}"##,
    )
    .unwrap();
    assert_eq!(g.legend.len(), 4); // 5th item not read
    let mut m = MockSurface::new(256, 64);
    render_graph(&mut m, Some(&g), false, "");
    // 79 px each + 8 gap: A 0..78, B 87..165, C would end at 252 (fits), D at 261 (dropped).
    assert!(lit_in(&m, 0..79, 58..63, (255, 0, 0)));
    assert!(lit_in(&m, 87..166, 58..63, (0, 255, 0)));
    assert!(lit_in(&m, 174..253, 58..63, (0, 0, 255)));
    assert!(!lit_in(&m, 0..256, 0..64, (255, 255, 0)));
}

#[test]
fn null_is_gap_for_lines_and_zero_for_bars() {
    let g = parse_graph_payload(
        r#"{"series":[{"style":"line","data":[1,null,3]},{"data":[1,null,3]}]}"#,
    )
    .unwrap();
    assert!(g.series[0].line && !g.series[1].line);
    assert_eq!(g.value_opt(&g.series[0], 1), None);
    assert_eq!(g.value_at(&g.series[1], 1), 0.0);
}

#[test]
fn y_mapping_matches_layout() {
    let ys = YScale {
        lo: 68.0,
        hi: 90.0,
        gh: 24,
        h: 32,
    };
    assert_eq!(ys.line_y(90.0), 8);
    assert_eq!(ys.line_y(68.0), 31);
    assert_eq!(ys.line_y(200.0), 8); // clamps
    assert_eq!(ys.line_y(-5.0), 31);
    assert_eq!(ys.rows(68.0), 0);
    assert_eq!(ys.rows(90.0), 24);
}

#[test]
fn line_draws_steps_and_gaps() {
    // 4 slots on 128 columns -> 32 columns each, plot rows 0..31 (no header).
    let g = parse_graph_payload(
        r##"{"min":0,"max":31,"series":[{"style":"line","color":"#FF4020","data":[0,31,null,10]}]}"##,
    )
    .unwrap();
    let mut m = MockSurface::new(128, 32);
    render_graph(&mut m, Some(&g), false, "");
    assert_eq!(px(&m, 0, 31), RED); // slot 0 at value 0 -> bottom row
    assert_eq!(px(&m, 31, 31), RED);
    // Step at column 32: vertical run from row 31 up to row 0.
    for y in 0..32 {
        assert_eq!(px(&m, 32, y), RED, "row {}", y);
    }
    assert_eq!(px(&m, 40, 0), RED);
    assert_eq!(px(&m, 40, 10), (0, 0, 0));
    // Gap: slot 2 has nothing, and slot 3 starts with a single pixel (no join).
    assert!((64..96).all(|x| (0..32).all(|y| px(&m, x, y) == (0, 0, 0))));
    assert_eq!(px(&m, 96, 21), RED); // 10/31 of 31 rows -> 31-10 = 21
    assert_eq!(px(&m, 96, 22), (0, 0, 0));
}

#[test]
fn negative_bars_hang_below_zero_line() {
    let g =
        parse_graph_payload(r##"{"min":-2,"max":2,"series":[{"color":"#2060FF","data":[2,-2]}]}"##)
            .unwrap();
    let mut m = MockSurface::new(128, 32);
    render_graph(&mut m, Some(&g), false, "");
    // Zero line: row 31 - round(0.5*31) = 15 (16 -> rounds half away -> 15.5 -> 16? computed below).
    let zero = YScale {
        lo: -2.0,
        hi: 2.0,
        gh: 32,
        h: 32,
    }
    .line_y(0.0);
    // Positive bar fills the top half, negative bar the bottom half.
    assert_eq!(px(&m, 10, 0), BLUE);
    assert_eq!(px(&m, 10, 31), (0, 0, 0));
    assert_eq!(px(&m, 100, 31), BLUE);
    assert_eq!(px(&m, 100, 0), (0, 0, 0));
    // Zero line visible where no bar covers it? Both bars cover their half; the
    // line pixel is under one of them, so check the colour exists on its row.
    assert!(zero >= 15 && zero <= 16);
}

#[test]
fn zero_line_only_with_bars_crossing_zero() {
    let lines =
        parse_graph_payload(r#"{"min":-5,"max":5,"series":[{"style":"line","data":[]}]}"#).unwrap();
    let mut m = MockSurface::new(128, 32);
    render_graph(&mut m, Some(&lines), false, "");
    assert!(m
        .canvas
        .pixels()
        .all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0));

    let bars = parse_graph_payload(r#"{"min":-5,"max":5,"series":[{"data":[]}]}"#).unwrap();
    render_graph(&mut m, Some(&bars), false, "");
    let zy = YScale {
        lo: -5.0,
        hi: 5.0,
        gh: 32,
        h: 32,
    }
    .line_y(0.0) as u32;
    assert_eq!(px(&m, 50, zy), (0x30, 0x30, 0x30));
}

#[test]
fn yaxis_only_on_tall_panels() {
    let g = parse_graph_payload(
        r##"{"title":"T","min":68,"max":90,"yaxis":true,"series":[{"style":"line","color":"#FF4020","data":[90]}]}"##,
    )
    .unwrap();
    let mut big = MockSurface::new(256, 64);
    render_graph(&mut big, Some(&g), true, "");
    // Labels "90"/"68": width 7 -> plot starts at x=8.
    assert!(lit_in(&big, 0..7, 15..20, (0x70, 0x70, 0x70)));
    assert!(lit_in(&big, 0..7, 59..64, (0x70, 0x70, 0x70)));
    assert_eq!(px(&big, 7, 15), (0, 0, 0)); // gap column
    assert_eq!(px(&big, 8, 15), RED); // first plot column, value = max -> top row
    let mut small = MockSurface::new(128, 32);
    render_graph(&mut small, Some(&g), true, "");
    assert_eq!(px(&small, 0, 8), RED); // no axis on 32 px panels
}

#[test]
fn tiny_font_widths_and_pixels() {
    assert_eq!(tiny_text_width(""), 0);
    assert_eq!(tiny_text_width("90"), 7);
    assert_eq!(tiny_text_width("kWh"), 11);
    let mut m = MockSurface::new(16, 8);
    draw_tiny_text(&mut m, "1", 0, 0, 0, 16, (255, 255, 255));
    // '1' = 010 110 010 010 111
    assert_eq!(px(&m, 1, 0), (255, 255, 255));
    assert_eq!(px(&m, 0, 0), (0, 0, 0));
    assert_eq!(px(&m, 0, 1), (255, 255, 255));
    assert!((0..3).all(|x| px(&m, x, 4) == (255, 255, 255)));
}

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

const ENERGY: &str = r##"{"v":1,"title":"ENERGY","tiles":[
  {"label":"HOUSE","value":"2.8","unit":"kW","color":"#FFB000"},
  {"label":"TODAY","value":"31.2","unit":"kWh","color":"#FFFFFF"},
  {"label":"AC $","value":"0.35","unit":"","color":"#40C0FF"},
  {"label":"OUT","value":"75","unit":"°F","color":"#FF6040"}]}"##;

#[test]
fn values_parse_and_limits() {
    let d = parse_table_payload(ENERGY).unwrap();
    assert_eq!(d.title, "ENERGY");
    assert_eq!(d.tiles.len(), 4);
    assert_eq!(d.tiles[0].label_color, (0x80, 0x80, 0x80));
    assert_eq!(d.tiles[3].unit, "°F");
    let five =
        r#"{"tiles":[{"value":"1"},{"value":"2"},{"value":"3"},{"value":"4"},{"value":"5"}]}"#;
    assert_eq!(parse_table_payload(five).unwrap().tiles.len(), 4);
    let num = parse_table_payload(r#"{"tiles":[{"value":42}]}"#).unwrap();
    assert_eq!(num.tiles[0].value, "42");
    assert_eq!(num.tiles[0].color, (255, 255, 255));
    assert!(parse_table_payload(r#"{"title":"x"}"#).is_none());
}

#[test]
fn fit_rule_drops_unit_but_never_truncates_value() {
    // Fits big with unit.
    let f = fit_value("2.8", "kW", &[3, 2, 1], 128, 24);
    assert_eq!((f.scale, f.show_unit, f.chars), (3, true, 3));
    // Too tall for G3 in 16 rows -> G2.
    let f = fit_value("2.8", "kW", &[3, 2, 1], 128, 16);
    assert_eq!(f.scale, 2);
    // G1 only. "31.2"+gap+"kWh"(tiny) = 23+2+11 = 36.
    let f = fit_value("31.2", "kWh", &[1], 36, 16);
    assert!(f.show_unit);
    let f = fit_value("31.2", "kWh", &[1], 35, 16);
    assert_eq!((f.show_unit, f.chars), (false, 4));
    // Too wide even without the unit: drawn in full anyway.
    let f = fit_value("123456", "", &[1], 20, 16);
    assert_eq!((f.show_unit, f.chars), (false, 6));
}

const HOME: &str = r##"{"v":1,"title":"HOME","tiles":[
  {"label":"DOWNSTAIRS","label_short":"DOWN","value":"75","unit":"°F","color":"#FFFFFF"},
  {"label":"UPSTAIRS","label_short":"UP","value":"76","unit":"°F","color":"#FFFFFF"},
  {"label":"AC","label_short":"AC","value":"18.2","unit":"HOURS","color":"#40C0FF"},
  {"label":"OUTSIDE","label_short":"OUT","value":"90","unit":"°F","color":"#FF6040"}]}"##;

#[test]
fn values_grid_is_stacked_on_128x32() {
    let d = parse_table_payload(HOME).unwrap();
    assert_eq!(d.tiles[0].label_short, "DOWN");
    let mut m = MockSurface::new(128, 32);
    render_table(&mut m, Some(&d), true, "NO DATA");
    let grey = (0x80, 0x80, 0x80);
    // Block = 5 + 1 + 7 = 13 rows, top = cell_y + 1: labels on rows 1..5 / 17..21,
    // values on rows 7..13 / 23..29.
    assert!(lit_in(&m, 0..62, 1..6, grey));
    assert!(!lit_in(&m, 0..128, 0..1, grey));
    assert!(lit_in(&m, 0..62, 7..14, (255, 255, 255)));
    assert!(!lit_in(&m, 0..62, 1..6, (255, 255, 255)));
    assert!(lit_in(&m, 0..62, 23..30, (0x40, 0xC0, 0xFF)));
    assert!(lit_in(&m, 66..128, 23..30, (0xFF, 0x60, 0x40)));
    // "DOWNSTAIRS" (39 px) fits the 62 px cell, so the full label is used: centred at x = 11.
    assert!(lit_in(&m, 11..13, 1..6, grey));
    assert!(!lit_in(&m, 0..11, 1..6, grey));
    // Gutter columns 62..65 stay dark.
    assert!((62..66).all(|x| (0..32).all(|y| px(&m, x, y) == (0, 0, 0))));
}

#[test]
fn short_label_used_only_when_full_does_not_fit() {
    let d = parse_table_payload(
        r#"{"tiles":[{"label":"DOWNSTAIRS","label_short":"DOWN","value":"1"},{"value":"2"},{"value":"3"}]}"#,
    )
    .unwrap();
    // 3 columns on 128x32: inner width 40 >= 39 -> full label still fits.
    let mut m = MockSurface::new(128, 32);
    render_table(&mut m, Some(&d), false, "");
    let grey = (0x80, 0x80, 0x80);
    let (lo, hi) = (0..40)
        .filter(|&x| (0..32).any(|y| px(&m, x, y) == grey))
        .fold((u32::MAX, 0), |(a, b), x| (a.min(x), b.max(x)));
    assert_eq!(hi - lo + 1, 39);
    // A 30 px wide cell can't hold it -> "DOWN" (15 px).
    let d4 = parse_table_payload(
        r#"{"tiles":[{"label":"DOWNSTAIRS","label_short":"DOWN","value":"1"},{"value":"2"},{"value":"3"},{"value":"4"}]}"#,
    )
    .unwrap();
    let mut small = MockSurface::new(64, 32); // 2x2 cells of 30 px
    render_table(&mut small, Some(&d4), false, "");
    let (lo, hi) = (0..30)
        .filter(|&x| (0..16).any(|y| px(&small, x, y) == grey))
        .fold((u32::MAX, 0), |(a, b), x| (a.min(x), b.max(x)));
    assert_eq!(hi - lo + 1, 15);
}

#[test]
fn six_char_label_fits_grid_with_unit() {
    // "AC 24H" (23 px tiny) + 2 + "17.0" (23) + 2 + "h" (3) = 53 <= 62 px inner cell.
    let aw = 62 - tiny_text_width("AC 24H") - 2;
    let f = fit_value("17.0", "h", &[1], aw, 16);
    assert!(f.show_unit && f.chars == 4);
}

#[test]
fn values_single_tile_stacks_under_title() {
    let d = parse_table_payload(
        r##"{"title":"POWER","tiles":[{"label":"HOUSE","value":"2.8","unit":"kW","color":"#FFB000"}]}"##,
    )
    .unwrap();
    let mut m = MockSurface::new(128, 32);
    render_table(&mut m, Some(&d), true, "NO DATA");
    assert!(lit_in(&m, 0..30, 0..7, (255, 255, 255))); // title
                                                       // ch = 24: G3 doesn't fit (27), G2 block = 5 + 1 + 14 = 20, top = 8 + 2 = 10.
    assert!(lit_in(&m, 0..128, 10..15, (0x80, 0x80, 0x80))); // label rows 10..14
    assert!(lit_in(&m, 0..128, 16..30, (0xFF, 0xB0, 0x00))); // value rows 16..29
    assert!(!lit_in(&m, 0..128, 30..32, (0xFF, 0xB0, 0x00)));
}

#[test]
fn values_no_data() {
    let mut m = MockSurface::new(128, 32);
    render_table(&mut m, None, true, "NO DATA");
    assert!(m.canvas.pixels().any(|p| p[0] > 0));
    let empty = parse_table_payload(r#"{"tiles":[]}"#).unwrap();
    let mut m2 = MockSurface::new(128, 32);
    render_table(&mut m2, Some(&empty), true, "NO DATA");
    assert_eq!(m.canvas, m2.canvas);
}

// ---------------------------------------------------------------------------
// Weather HA source
// ---------------------------------------------------------------------------

const WEATHER: &str = r#"{"v":1,"units":"imperial",
 "current":{"temp":75.4,"condition":"clear-night","humidity":47,"wind":7,"wind_unit":"mph"},
 "days":[{"temp_max":89,"temp_min":70,"condition":"sunny","precip_prob":0},
         {"temp_max":90,"temp_min":71,"condition":"partlycloudy","precip_prob":5},
         {"temp_max":91,"temp_min":72,"condition":"lightning-rainy"},
         {"temp_max":88,"temp_min":69,"condition":"windy-variant"},
         {"temp_max":87,"temp_min":68,"condition":"weird"},
         {"temp_max":86,"temp_min":67,"condition":"sunny"}]}"#;

#[test]
fn weather_payload_to_day_forecasts() {
    let w = parse_weather_payload(WEATHER).unwrap();
    assert!(w.imperial);
    assert_eq!(w.days.len(), 5);
    let f = weather_pages(&w, "en", 5); // Friday
    assert_eq!(f.len(), 6); // NOW + 5 days
                            // NOW page: live reading (white line 1) + humidity / wind (grey line 2).
    assert!(f[0].is_now);
    assert_eq!(f[0].label, "NOW");
    assert_eq!(f[0].temp_min, "75°F");
    assert_eq!(f[0].temp_max, "47%  7mph");
    assert_eq!(f[0].icon, "01n"); // current.condition = clear-night
    assert!(f[0].condition.is_empty());
    assert!(!f[1].is_now);
    assert_eq!(f[1].label, "TODAY");
    assert_eq!(f[1].temp, "75°F"); // live station reading
    assert_eq!(
        (f[1].temp_min.as_str(), f[1].temp_max.as_str()),
        ("70°F", "89°F")
    );
    assert_eq!(f[1].condition, "Sunny");
    assert_eq!(f[1].icon, "01d");
    assert_eq!(f[2].icon, "02d");
    assert_eq!(f[3].condition, "Storm+rain");
    assert_eq!(f[3].icon, "11d");
    assert_eq!(f[4].icon, "03d");
    assert_eq!(f[5].condition, "weird");
    assert_eq!(f[5].icon, "03d");
    let fr = weather_pages(&w, "fr", 5);
    assert_eq!(fr[0].label, "ACTU.");
    assert_eq!(fr[1].condition, "Soleil");
    assert_eq!(weather_pages(&w, "es", 5)[0].label, "AHORA");
    let metric = parse_weather_payload(r#"{"units":"metric","current":{"temp":21}}"#).unwrap();
    let m = weather_pages(&metric, "en", 0);
    assert_eq!(m.len(), 1); // no forecast days: the NOW page stands alone (as on the ESP32)
    assert!(m[0].is_now);
    assert_eq!(m[0].temp_min, "21°C");
    // Long labels for wide panels.
    assert_eq!(f[1].label_long, "TODAY");
    assert_eq!(f[2].label_long, "TOMORROW");
    assert_eq!(f[3].label_long, "SUNDAY");
    assert_eq!(f[2].condition_long, "Partly cloudy");
    assert_eq!(f[0].label_long, "");
}

#[test]
fn now_page_wind_direction() {
    let w = parse_weather_payload(
        r#"{"current":{"temp":80,"humidity":41,"wind":9,"wind_unit":"mph","wind_dir":"NE"}}"#,
    )
    .unwrap();
    let f = weather_pages(&w, "en", 0);
    assert_eq!(f[0].temp_max, "41%  NE 9mph");
    assert_eq!(f[0].now_line2_short, "41%  9mph");
    // No speed: the direction never stands alone.
    let w =
        parse_weather_payload(r#"{"current":{"temp":80,"humidity":41,"wind_dir":"NE"}}"#).unwrap();
    assert_eq!(weather_pages(&w, "en", 0)[0].temp_max, "41%");
    // Too long for 128x32 (16 chars = 95 px > 92): the direction is dropped, numbers stay.
    let m = render_weather_day(
        r#"{"units":"imperial","current":{"temp":100,"humidity":100,"wind":100,"wind_unit":"km/h","wind_dir":"NW"}}"#,
        128,
        32,
        "dir_drop",
    );
    let grey: Vec<u32> = (0..128)
        .filter(|&x| (0..32).any(|y| px(&m, x, y) == (210, 210, 210)))
        .collect();
    // "100%  100km/h" = 13 chars = 77 px, right-aligned to x=123: it starts at x=47,
    // whose column is blank in the GLCD "1", so the first lit pixel is on x=48.
    assert_eq!((grey[0], *grey.last().unwrap()), (48, 123));
    if let Ok(out) = std::env::var("MQTTDATA_PREVIEW_DIR") {
        let live = render_weather_day(
            r#"{"units":"imperial","current":{"temp":80,"condition":"clear-night","humidity":41,"wind":9.0,"wind_unit":"mph","wind_dir":"NE"},"days":[{"temp_max":95,"temp_min":72,"condition":"sunny"}]}"#,
            128,
            32,
            "dir_live",
        );
        save(&live, &out, "weather_128_now_dir");
        let live64 = render_weather_day(
            r#"{"units":"imperial","current":{"temp":80,"condition":"clear-night","humidity":41,"wind":9.0,"wind_unit":"mph","wind_dir":"NE"},"days":[{"temp_max":95,"temp_min":72,"condition":"sunny"}]}"#,
            256,
            64,
            "dir_live64",
        );
        save(&live64, &out, "weather_256_now_dir");
        save(&m, &out, "weather_128_now_dir_dropped");
    }
}

#[test]
fn now_page_second_line_falls_back() {
    let only_hum =
        parse_weather_payload(r#"{"current":{"temp":60,"humidity":30.4,"condition":"rainy"}}"#)
            .unwrap();
    assert_eq!(weather_pages(&only_hum, "en", 0)[0].temp_max, "30%");
    let only_wind =
        parse_weather_payload(r#"{"current":{"temp":60,"wind":12,"wind_unit":"km/h"}}"#).unwrap();
    assert_eq!(weather_pages(&only_wind, "en", 0)[0].temp_max, "12km/h");
    let neither = parse_weather_payload(r#"{"current":{"temp":60,"condition":"rainy"}}"#).unwrap();
    assert_eq!(weather_pages(&neither, "en", 0)[0].temp_max, "Rain");
    // No live temperature -> no NOW page.
    let no_temp = parse_weather_payload(r#"{"days":[{"temp_max":70,"temp_min":50}]}"#).unwrap();
    let f = weather_pages(&no_temp, "en", 0);
    assert_eq!(f.len(), 1);
    assert!(!f[0].is_now);
}

/// First page of a weather payload (NOW when it has a live temperature),
/// drawn with the shared weather page renderer.
fn render_weather_day(payload: &str, w: u32, h: u32, _name: &str) -> MockSurface {
    let wx = parse_weather_payload(payload).expect("weather payload");
    let slides = weather_pages(&wx, "en", 0);
    let mut img = image::RgbaImage::new(w, h);
    draw_weather_page(&mut img, &slides[0], 0, w, h, 0, 0);
    let mut m = MockSurface::new(w, h);
    m.draw_image(&image::DynamicImage::ImageRgba8(img).to_rgb8(), 0, 0);
    m
}

/// Leftmost / rightmost columns and top / bottom rows painted in colour `c`.
#[allow(dead_code)]
fn bbox(m: &MockSurface, c: (u8, u8, u8)) -> Option<(u32, u32, u32, u32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for y in 0..m.canvas.height() {
        for x in 0..m.canvas.width() {
            if px(m, x, y) == c {
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
    }
    (x0 != u32::MAX).then_some((x0, y0, x1, y1))
}

#[test]
fn day0_condition_falls_back_to_current() {
    let w = parse_weather_payload(
        r#"{"current":{"temp":60,"condition":"rainy"},"days":[{"temp_max":70,"temp_min":50}]}"#,
    )
    .unwrap();
    let f = weather_pages(&w, "en", 1);
    assert_eq!(f[1].condition, "Rain");
    assert_eq!(f[1].icon, "10d");
}

// ---------------------------------------------------------------------------
// Preview renders
// `MQTTDATA_PREVIEW_DIR=out cargo test --test test_ha_screens -- --ignored`
// ---------------------------------------------------------------------------

fn save(m: &MockSurface, dir: &str, name: &str) {
    let (w, h) = (m.canvas.width(), m.canvas.height());
    let big = image::imageops::resize(&m.canvas, w * 8, h * 8, image::imageops::Nearest);
    big.save(format!("{}/{}_{}x{}.png", dir, name, w, h))
        .unwrap();
}

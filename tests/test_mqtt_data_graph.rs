//! MQTT Data engine: payload parsing and the graph page renderer.

use arcadematrix::core::matrix::MockSurface;
use arcadematrix::engines::mqtt_data::graph::{layout_for, render_graph, slot_for_column};
use arcadematrix::engines::mqtt_data::payload::{
    parse_graph_payload, parse_hex_color, MAX_POINTS, MAX_SERIES,
};
fn px(m: &MockSurface, x: u32, y: u32) -> (u8, u8, u8) {
    let p = m.canvas.get_pixel(x, y);
    (p[0], p[1], p[2])
}

const UP: (u8, u8, u8) = (0x20, 0x80, 0xFF);
const DN: (u8, u8, u8) = (0x00, 0xD0, 0xA0);

// ---------------------------------------------------------------------------
// Payload parser
// ---------------------------------------------------------------------------

#[test]
fn parses_full_payload() {
    let g = parse_graph_payload(
        r##"{"v":1,"title":"AC 24H","summary":"12.3kWh","unit":"kW","slots":96,"max":6,
            "stack":true,
            "series":[{"label":"UP","color":"#2080FF","data":[0,1.25,2]},
                      {"label":"DN","color":"#00D0A0","data":[1,1]}],
            "bands":[{"from":60,"to":72,"color":"#301800"}],
            "marks":[{"at":28,"color":"#404040"}]}"##,
    )
    .expect("valid payload");
    assert_eq!(g.title, "AC 24H");
    assert_eq!(g.summary, "12.3kWh");
    assert_eq!(g.unit, "kW");
    assert_eq!(g.slots, 96);
    assert_eq!(g.max, Some(6.0));
    assert_eq!(g.hi, 6.0);
    assert!(g.stack);
    assert_eq!(g.series.len(), 2);
    assert_eq!(g.series[0].color, UP);
    assert_eq!(g.series[1].label, "DN");
    assert_eq!(g.bands.len(), 1);
    assert_eq!((g.bands[0].from, g.bands[0].to), (60, 72));
    assert_eq!(g.marks[0].at, 28);
    assert_eq!(g.marks[0].color, (0x40, 0x40, 0x40));
}

#[test]
fn data_is_right_aligned() {
    let g = parse_graph_payload(r#"{"slots":5,"series":[{"data":[1,2,3]}]}"#).unwrap();
    let s = &g.series[0];
    let vals: Vec<f32> = (0..5).map(|i| g.value_at(s, i)).collect();
    assert_eq!(vals, vec![0.0, 0.0, 1.0, 2.0, 3.0]);
}

#[test]
fn longer_series_than_slots_keeps_newest_points() {
    let g = parse_graph_payload(r#"{"slots":2,"series":[{"data":[1,2,3,4]}]}"#).unwrap();
    let s = &g.series[0];
    assert_eq!(g.value_at(s, 0), 3.0);
    assert_eq!(g.value_at(s, 1), 4.0);
}

#[test]
fn slots_default_to_longest_series() {
    let g = parse_graph_payload(r#"{"series":[{"data":[1,2]},{"data":[1,2,3,4,5]}]}"#).unwrap();
    assert_eq!(g.slots, 5);
}

#[test]
fn auto_max_uses_stacked_sum_or_single_peak() {
    let stacked = parse_graph_payload(r#"{"series":[{"data":[1,3]},{"data":[2,2]}]}"#).unwrap();
    assert_eq!(stacked.hi, 5.0);
    let overlay =
        parse_graph_payload(r#"{"stack":false,"series":[{"data":[1,3]},{"data":[2,2]}]}"#).unwrap();
    assert_eq!(overlay.hi, 3.0);
    let zeros = parse_graph_payload(r#"{"max":0,"series":[{"data":[0,0]}]}"#).unwrap();
    assert!((zeros.hi - 0.001).abs() < 1e-6);
    let negative_max = parse_graph_payload(r#"{"max":-4,"series":[{"data":[2]}]}"#).unwrap();
    assert_eq!(negative_max.hi, 2.0);
}

#[test]
fn bad_points_become_zero_and_limits_apply() {
    let g = parse_graph_payload(r#"{"series":[{"data":[1,null,"x",true,2]}]}"#).unwrap();
    let vals: Vec<f32> = (0..5).map(|i| g.value_at(&g.series[0], i)).collect();
    assert_eq!(vals, vec![1.0, 0.0, 0.0, 0.0, 2.0]);
    assert_eq!(g.value_opt(&g.series[0], 1), None);

    let many_series = format!(
        r#"{{"series":[{}]}}"#,
        vec![r#"{"data":[1]}"#; MAX_SERIES + 2].join(",")
    );
    assert_eq!(
        parse_graph_payload(&many_series).unwrap().series.len(),
        MAX_SERIES
    );

    let points: Vec<String> = (0..MAX_POINTS + 10).map(|i| i.to_string()).collect();
    let many_points = format!(r#"{{"series":[{{"data":[{}]}}]}}"#, points.join(","));
    assert_eq!(
        parse_graph_payload(&many_points).unwrap().series[0]
            .data
            .len(),
        MAX_POINTS
    );
}

#[test]
fn rejects_invalid_payloads() {
    assert!(parse_graph_payload("not json").is_none());
    assert!(parse_graph_payload(r#"{"title":"no series"}"#).is_none());
    assert!(parse_graph_payload(r#"{"series":"nope"}"#).is_none());
}

#[test]
fn bad_colors_fall_back_and_bad_bands_are_skipped() {
    let g = parse_graph_payload(
        r##"{"series":[{"color":"blue","data":[1]}],
            "bands":[{"from":5,"to":2,"color":"#111111"},{"from":1,"to":3,"color":"zz"}],
            "marks":[{"at":-1,"color":"#ffffff"}]}"##,
    )
    .unwrap();
    assert_eq!(g.series[0].color, UP); // first default colour
    assert!(g.bands.is_empty());
    assert!(g.marks.is_empty());
    assert_eq!(parse_hex_color("#A0b0C0"), Some((0xA0, 0xB0, 0xC0)));
    assert_eq!(parse_hex_color("#12345"), None);
}

// ---------------------------------------------------------------------------
// Engine registration + rendering
// ---------------------------------------------------------------------------

#[test]
fn layouts_match_spec() {
    let small = layout_for(32, true);
    assert_eq!(
        (small.text_scale, small.graph_top, small.graph_height),
        (1, 8, 24)
    );
    assert!(!small.gridline);
    let big = layout_for(64, true);
    assert_eq!(
        (big.text_scale, big.graph_top, big.graph_height),
        (2, 15, 49)
    );
    assert!(big.gridline);
    let no_header = layout_for(32, false);
    assert_eq!((no_header.graph_top, no_header.graph_height), (0, 32));
    let tiny = layout_for(16, true);
    assert!(!tiny.header);
}

#[test]
fn columns_map_to_slots() {
    assert_eq!(slot_for_column(0, 128, 96), 0);
    assert_eq!(slot_for_column(127, 128, 96), 95);
    assert_eq!(slot_for_column(4, 128, 96), 3);
    assert_eq!(slot_for_column(255, 256, 96), 95);
}

#[test]
fn no_data_notice_is_drawn() {
    let mut m = MockSurface::new(128, 32);
    render_graph(&mut m, None, true, "NO DATA");
    let lit = m.canvas.pixels().filter(|p| p[0] > 0).count();
    assert!(lit > 0);
    // Centered: nothing in the outer columns.
    for y in 0..32 {
        assert_eq!(px(&m, 0, y), (0, 0, 0));
        assert_eq!(px(&m, 127, y), (0, 0, 0));
    }
}

#[test]
fn stacked_bars_have_expected_heights_on_128x32() {
    // 4 slots over 128 columns -> 32 columns per slot. max 4 -> 6 rows per unit.
    let g = parse_graph_payload(
        r##"{"max":4,"series":[{"color":"#2080FF","data":[0,1,2,4]},
                               {"color":"#00D0A0","data":[0,1,1,0]}]}"##,
    )
    .unwrap();
    let mut m = MockSurface::new(128, 32);
    // Header on (empty title): plot rows 8..31 = 24 rows.
    render_graph(&mut m, Some(&g), true, "NO DATA");
    // Slot 0: empty.
    assert_eq!(px(&m, 10, 31), (0, 0, 0));
    // Slot 1: UP rows 26..31 (6 rows), DN rows 20..25.
    assert_eq!(px(&m, 40, 31), UP);
    assert_eq!(px(&m, 40, 26), UP);
    assert_eq!(px(&m, 40, 25), DN);
    assert_eq!(px(&m, 40, 20), DN);
    assert_eq!(px(&m, 40, 19), (0, 0, 0));
    // Slot 3: UP fills the whole plot (value == max).
    assert_eq!(px(&m, 120, 8), UP);
    assert_eq!(px(&m, 120, 31), UP);
    assert_eq!(px(&m, 120, 7), (0, 0, 0));
}

#[test]
fn overlay_draws_later_series_on_top() {
    let g = parse_graph_payload(
        r##"{"max":2,"stack":false,"series":[{"color":"#2080FF","data":[2]},
                                             {"color":"#00D0A0","data":[1]}]}"##,
    )
    .unwrap();
    let mut m = MockSurface::new(128, 32);
    render_graph(&mut m, Some(&g), false, "");
    assert_eq!(px(&m, 5, 31), DN); // overlay on top at the bottom half
    assert_eq!(px(&m, 5, 15), UP); // only the taller series above
    assert_eq!(px(&m, 5, 0), UP);
}

#[test]
fn bands_marks_and_header_render() {
    let g = parse_graph_payload(
        r##"{"title":"AC","summary":"5kWh","slots":4,"max":10,
            "series":[{"color":"#2080FF","data":[0,0,0,0]}],
            "bands":[{"from":2,"to":3,"color":"#301800"}],
            "marks":[{"at":1,"color":"#404040"}]}"##,
    )
    .unwrap();
    let mut m = MockSurface::new(128, 32);
    render_graph(&mut m, Some(&g), true, "");
    // Band covers slot 2 = columns 64..95, plot rows 8..31 only.
    assert_eq!(px(&m, 70, 8), (0x30, 0x18, 0x00));
    assert_eq!(px(&m, 70, 31), (0x30, 0x18, 0x00));
    assert_eq!(px(&m, 96, 20), (0, 0, 0));
    // Mark at the left edge of slot 1 (column 32), dotted from the plot top.
    assert_eq!(px(&m, 32, 8), (0x40, 0x40, 0x40));
    assert_eq!(px(&m, 32, 9), (0, 0, 0));
    assert_eq!(px(&m, 33, 8), (0, 0, 0));
    // Title in white somewhere in the top-left, summary in the series colour on the right.
    let title_lit = (0..20).any(|x| (0..7).any(|y| px(&m, x, y) == (255, 255, 255)));
    assert!(title_lit);
    let summary_lit = (100..128).any(|x| (0..7).any(|y| px(&m, x, y) == UP));
    assert!(summary_lit);
    // Row 7 (gap between header and plot) stays dark.
    assert!((0..128).all(|x| px(&m, x, 7) == (0, 0, 0)));
}

#[test]
fn tall_panel_uses_big_header_and_gridline() {
    let g = parse_graph_payload(
        r##"{"title":"AC","slots":96,"max":6,"series":[{"color":"#2080FF","data":[]}]}"##,
    )
    .unwrap();
    let mut m = MockSurface::new(256, 64);
    render_graph(&mut m, Some(&g), true, "");
    // Gridline at 50 %: 49-row plot -> 25 rows up from the bottom = row 39.
    assert_eq!(px(&m, 100, 39), (0x20, 0x20, 0x20));
    // 2x title reaches row 13 but not 14.
    let row13 = (0..30).any(|x| px(&m, x, 13) != (0, 0, 0));
    assert!(row13);
    assert!((0..256).all(|x| px(&m, x, 14) == (0, 0, 0)));
}

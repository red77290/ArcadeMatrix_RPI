use arcadematrix::core::config::parse_symbols_string;
use arcadematrix::core::rotation::{is_night_time, RotationState};

#[test]
fn test_night_mode_range() {
    assert!(!is_night_time(false, "23:00", "07:00"));
}

#[test]
fn test_rotation_state() {
    let mut rot = RotationState::new();
    let modes = vec![
        arcadematrix::core::config::RotationEntry::new("clock", 10),
        arcadematrix::core::config::RotationEntry::new("date", 10),
        arcadematrix::core::config::RotationEntry::new("weather", 10),
    ];

    assert_eq!(rot.next_mode(&modes).unwrap().instance_id, "date");
    assert_eq!(rot.next_mode(&modes).unwrap().instance_id, "weather");
    assert_eq!(rot.next_mode(&modes).unwrap().instance_id, "clock");
}

#[test]
fn test_parse_symbols_string() {
    assert_eq!(parse_symbols_string("BTC"), vec!["BTC"]);
    assert_eq!(parse_symbols_string("BTC,ETH"), vec!["BTC", "ETH"]);
    assert_eq!(
        parse_symbols_string("BTC, ETH, SOL"),
        vec!["BTC", "ETH", "SOL"]
    );
    assert_eq!(
        parse_symbols_string("BTC, ETH,,SOL,"),
        vec!["BTC", "ETH", "SOL"]
    );
    assert_eq!(parse_symbols_string(""), Vec::<String>::new());
    assert_eq!(parse_symbols_string(" , "), Vec::<String>::new());
}

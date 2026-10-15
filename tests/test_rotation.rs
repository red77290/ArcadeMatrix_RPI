use arcadematrix::core::config::parse_symbols_string;
use arcadematrix::core::rotation::{is_night_time, RotationState};

#[test]
fn test_night_mode_range() {
    // Disabled always returns false regardless of range
    assert!(!is_night_time(false, "00:00", "23:59"));
    assert!(!is_night_time(false, "23:00", "07:00"));

    // Invalid time format strings return false safely
    assert!(!is_night_time(true, "invalid", "07:00"));
    assert!(!is_night_time(true, "23:00", "invalid"));
    assert!(!is_night_time(true, "", ""));

    // Empty same-day window (turn_off == wake_up): now >= t && now < t is always false
    assert!(!is_night_time(true, "00:00", "00:00"));
    assert!(!is_night_time(true, "12:00", "12:00"));

    // Deterministic same-day (turn_off <= wake_up) and overnight (turn_off > wake_up) checks
    // anchored around the current local hour
    use chrono::Timelike;
    let hour = chrono::Local::now().time().hour();
    if hour < 12 {
        // Morning (00:00..11:59):
        // 1. Same-day range covering morning ("00:00" to "23:59") -> active (true)
        assert!(is_night_time(true, "00:00", "23:59"));
        // 2. Same-day range in afternoon ("13:00" to "23:59") -> inactive (false)
        assert!(!is_night_time(true, "13:00", "23:59"));
        // 3. Overnight range wrapping past morning ("23:00" to "12:30") -> active (true)
        assert!(is_night_time(true, "23:00", "12:30"));
        // 4. Overnight range ending at midnight ("23:00" to "00:00") -> inactive (false)
        assert!(!is_night_time(true, "23:00", "00:00"));
    } else {
        // Afternoon/Evening (12:00..23:59):
        // 1. Same-day range covering afternoon ("12:00" to "23:59" or overnight "12:00" to "06:00")
        assert!(is_night_time(true, "00:00", "11:59") == false);
        // 2. Overnight range starting at noon ("12:00" to "06:00") -> active (true)
        assert!(is_night_time(true, "12:00", "06:00"));
        // 3. Overnight range starting after current hour or ending before noon ("23:59" to "06:00" if hour < 23)
        if hour < 23 {
            assert!(is_night_time(true, "00:00", "23:59"));
            assert!(!is_night_time(true, "23:30", "06:00"));
        } else {
            assert!(is_night_time(true, "23:00", "06:00"));
            assert!(!is_night_time(true, "01:00", "11:00"));
        }
    }
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

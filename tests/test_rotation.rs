use arcadematrix::core::rotation::{is_night_time, RotationState};

#[test]
fn test_night_mode_range() {
    assert_eq!(is_night_time(false, "23:00", "07:00"), false);
}

#[test]
fn test_rotation_state() {
    let mut rot = RotationState::new();
    let modes = vec![
        "clock".to_string(),
        "date".to_string(),
        "weather".to_string(),
    ];

    assert_eq!(rot.next_mode(&modes), Some("date"));
    assert_eq!(rot.next_mode(&modes), Some("weather"));
    assert_eq!(rot.next_mode(&modes), Some("clock"));
}

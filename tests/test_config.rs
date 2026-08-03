use arcadematrix::core::config::{Config, ConfigSettings};
use tempfile::NamedTempFile;

#[test]
fn test_default_config_settings() {
    let settings = ConfigSettings::default();
    assert_eq!(settings.matrix_rows, 32);
    assert_eq!(settings.matrix_cols, 64);
    assert_eq!(settings.matrix_chain, 1);
    assert_eq!(settings.matrix_parallel, 1);
    assert_eq!(settings.time_format, "%H:%M:%S");
    assert_eq!(settings.time_font, "PressStart2P.ttf");
    assert_eq!(settings.time_size, 2);
    assert_eq!(settings.time_theme, 0);
    assert_eq!(settings.standby_enabled, false);
    assert_eq!(settings.standby_turn_off, "23:00");
    assert_eq!(settings.standby_wake_up, "07:00");
}

#[test]
fn test_config_ini_save_and_reload() {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let path = temp_file.path().to_path_buf();

    let config = Config::new(&path);
    {
        let mut s = config.settings.write();
        s.matrix_brightness = 75;
        s.time_theme = 18; // Cyberpunk
        s.weather_city = "Paris".to_string();
        s.standby_enabled = true;
    }
    assert!(config.save());

    // Reload from disk in a fresh Config instance
    let reloaded_config = Config::new(&path);
    let s = reloaded_config.settings.read();
    assert_eq!(s.matrix_brightness, 75);
    assert_eq!(s.time_theme, 18);
    assert_eq!(s.weather_city, "Paris");
    assert_eq!(s.standby_enabled, true);
}

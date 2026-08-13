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
fn test_config_ini_save_and_reload_exhaustive() {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let path = temp_file.path().to_path_buf();

    let config = Config::new(&path);
    {
        let mut s = config.settings.write();
        // MATRIX
        s.matrix_rows = 64;
        s.matrix_cols = 128;
        s.matrix_chain = 2;
        s.matrix_parallel = 2;
        s.matrix_multiplexing = 1;
        s.matrix_row_addr_type = 2;
        s.matrix_mapping = "adafruit-hat".to_string();
        s.matrix_slowdown = 4;
        s.matrix_brightness = 85;
        s.matrix_rgb_sequence = "BGR".to_string();
        s.matrix_pwm_bits = 12;
        s.matrix_pwm_lsb_nanoseconds = 150;
        s.matrix_limit_refresh_rate_hz = 100;
        s.matrix_disable_hardware_pulsing = true;
        s.matrix_driver_chip = "FM6126A".to_string();

        // TIME
        s.time_format = "%H:%M".to_string();
        s.time_font = "custom.ttf".to_string();
        s.time_size = 3;
        s.time_theme = 18;
        s.clock_color_1 = "#123456".to_string();
        s.clock_color_2 = "#654321".to_string();
        s.time_offset_x = -2;
        s.time_offset_y = 5;
        s.ntp_server = "time.cloudflare.com".to_string();
        s.timezone = "EST5EDT".to_string();

        // DATE
        s.date_format = "%Y/%m/%d".to_string();
        s.date_font = "date.ttf".to_string();
        s.date_size = 2;
        s.date_theme = 5;
        s.date_color_1 = "#ABCDEF".to_string();
        s.date_color_2 = "#FEDCBA".to_string();
        s.date_offset_x = 3;
        s.date_offset_y = -1;

        // WEATHER
        s.weather_api_key = "test_key_xyz".to_string();
        s.weather_city = "Lyon,FR".to_string();
        s.weather_lang = "fr".to_string();
        s.weather_offset_x = 1;
        s.weather_offset_y = -3;

        // IDLE
        s.idle_rotation = vec![
            "clock".to_string(),
            "crypto".to_string(),
            "stocks".to_string(),
        ];
        s.idle_clock_duration_sec = 45;
        s.idle_date_duration_sec = 12;
        s.idle_weather_duration_sec = 20;
        s.idle_gifs_count = 8;
        s.idle_fighter_enabled = false;
        s.idle_fighter_interval = 25;
        s.selected_gifs = vec!["gif1.gif".to_string(), "gif2.gif".to_string()];
        s.selected_sprites = vec!["sprite1.png".to_string()];

        // CRYPTO & STOCKS
        s.crypto_symbols = vec!["BTC".to_string(), "ETH".to_string(), "SOL".to_string()];
        s.crypto_cache_ttl_min = 5;
        s.stock_symbols = vec!["AAPL".to_string(), "MSFT".to_string()];
        s.stock_cache_ttl_min = 10;

        // STANDBY
        s.standby_enabled = true;
        s.standby_turn_off = "22:00".to_string();
        s.standby_wake_up = "06:00".to_string();
        s.standby_night_brightness = 5;

        // MQTT
        s.mqtt_enabled = true;
        s.mqtt_broker = "10.0.0.10".to_string();
        s.mqtt_port = 8883;
        s.mqtt_user = "mqttuser".to_string();
        s.mqtt_pass = "mqttpass".to_string();

        // API
        s.api_auth_enabled = true;
        s.api_token = "custom_secret_token_123".to_string();

        // WIFI
        s.wifi_ssid = "TestWiFi".to_string();
        s.wifi_pass = "TestPass".to_string();
        s.wifi_configured = true;
        s.wifi_disable_internal = true;
    }
    assert!(config.save());

    // Reload from disk in a fresh Config instance
    let reloaded_config = Config::new(&path);
    let s = reloaded_config.settings.read();

    // Assert every single field across all 11 sections
    // MATRIX
    assert_eq!(s.matrix_rows, 64);
    assert_eq!(s.matrix_cols, 128);
    assert_eq!(s.matrix_chain, 2);
    assert_eq!(s.matrix_parallel, 2);
    assert_eq!(s.matrix_multiplexing, 1);
    assert_eq!(s.matrix_row_addr_type, 2);
    assert_eq!(s.matrix_mapping, "adafruit-hat");
    assert_eq!(s.matrix_slowdown, 4);
    assert_eq!(s.matrix_brightness, 85);
    assert_eq!(s.matrix_rgb_sequence, "BGR");
    assert_eq!(s.matrix_pwm_bits, 12);
    assert_eq!(s.matrix_pwm_lsb_nanoseconds, 150);
    assert_eq!(s.matrix_limit_refresh_rate_hz, 100);
    assert_eq!(s.matrix_disable_hardware_pulsing, true);
    assert_eq!(s.matrix_driver_chip, "FM6126A");

    // TIME
    assert_eq!(s.time_format, "%H:%M");
    assert_eq!(s.time_font, "custom.ttf");
    assert_eq!(s.time_size, 3);
    assert_eq!(s.time_theme, 18);
    assert_eq!(s.clock_color_1, "#123456");
    assert_eq!(s.clock_color_2, "#654321");
    assert_eq!(s.time_offset_x, -2);
    assert_eq!(s.time_offset_y, 5);
    assert_eq!(s.ntp_server, "time.cloudflare.com");
    assert_eq!(s.timezone, "EST5EDT");

    // DATE
    assert_eq!(s.date_format, "%Y/%m/%d");
    assert_eq!(s.date_font, "date.ttf");
    assert_eq!(s.date_size, 2);
    assert_eq!(s.date_theme, 5);
    assert_eq!(s.date_color_1, "#ABCDEF");
    assert_eq!(s.date_color_2, "#FEDCBA");
    assert_eq!(s.date_offset_x, 3);
    assert_eq!(s.date_offset_y, -1);

    // WEATHER
    assert_eq!(s.weather_api_key, "test_key_xyz");
    assert_eq!(s.weather_city, "Lyon,FR");
    assert_eq!(s.weather_lang, "fr");
    assert_eq!(s.weather_offset_x, 1);
    assert_eq!(s.weather_offset_y, -3);

    // IDLE
    assert_eq!(s.idle_rotation, vec!["clock", "crypto", "stocks"]);
    assert_eq!(s.idle_clock_duration_sec, 45);
    assert_eq!(s.idle_date_duration_sec, 12);
    assert_eq!(s.idle_weather_duration_sec, 20);
    assert_eq!(s.idle_gifs_count, 8);
    assert_eq!(s.idle_fighter_enabled, false);
    assert_eq!(s.idle_fighter_interval, 25);
    assert_eq!(s.selected_gifs, vec!["gif1.gif", "gif2.gif"]);
    assert_eq!(s.selected_sprites, vec!["sprite1.png"]);

    // CRYPTO & STOCKS
    assert_eq!(s.crypto_symbols, vec!["BTC", "ETH", "SOL"]);
    assert_eq!(s.crypto_cache_ttl_min, 5);
    assert_eq!(s.stock_symbols, vec!["AAPL", "MSFT"]);
    assert_eq!(s.stock_cache_ttl_min, 10);

    // STANDBY
    assert_eq!(s.standby_enabled, true);
    assert_eq!(s.standby_turn_off, "22:00");
    assert_eq!(s.standby_wake_up, "06:00");
    assert_eq!(s.standby_night_brightness, 5);

    // MQTT
    assert_eq!(s.mqtt_enabled, true);
    assert_eq!(s.mqtt_broker, "10.0.0.10");
    assert_eq!(s.mqtt_port, 8883);
    assert_eq!(s.mqtt_user, "mqttuser");
    assert_eq!(s.mqtt_pass, "mqttpass");

    // API
    assert_eq!(s.api_auth_enabled, true);
    assert_eq!(s.api_token, "custom_secret_token_123");

    // WIFI
    assert_eq!(s.wifi_ssid, "TestWiFi");
    assert_eq!(s.wifi_pass, "TestPass");
    assert_eq!(s.wifi_configured, true);
    assert_eq!(s.wifi_disable_internal, true);
}

#[test]
fn test_config_malformed_ini_fallback() {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let path = temp_file.path().to_path_buf();

    // Write malformed INI
    std::fs::write(
        &path,
        "[MATRIX]\nROWS = invalid_number\nBRIGHTNESS = 999999999999999999999\n[IDLE]\nROTATION = ",
    )
    .unwrap();

    let mut settings = ConfigSettings::default();
    Config::load_from_ini(&path, &mut settings);

    // Fallbacks to default values without panic
    assert_eq!(settings.matrix_rows, 32);
    assert_eq!(settings.matrix_cols, 64);
    assert_eq!(
        settings.idle_rotation,
        vec!["clock", "date", "weather", "gifs"]
    );
}

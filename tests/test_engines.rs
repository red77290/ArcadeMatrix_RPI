use arcadematrix::core::config::Config;
use arcadematrix::core::dmd_cache::DmdCache;
use arcadematrix::core::matrix::{MatrixBackend, MockMatrix};
use arcadematrix::engines::clock::ClockEngine;
use arcadematrix::engines::date::DateEngine;
use arcadematrix::engines::fighter::FighterEngine;
use arcadematrix::engines::gif::GifEngine;
use arcadematrix::engines::marquee::MarqueeEngine;
use arcadematrix::engines::message::{MessageEngine, MessagePayload};
use image::RgbImage;
use tempfile::NamedTempFile;

#[test]
fn test_clock_engine_render_themes() {
    let temp_file = NamedTempFile::new().unwrap();
    let config = Config::new(temp_file.path());
    let mut matrix = MockMatrix::new(64, 32);
    let mut clock_engine = ClockEngine::new(64, 32);

    for theme_id in 0..30 {
        config.settings.write().time_theme = theme_id;
        matrix.clear();
        clock_engine.render(&mut matrix, &config);
    }
}

#[test]
fn test_date_engine_themes() {
    let temp_file = NamedTempFile::new().unwrap();
    let config = Config::new(temp_file.path());
    let mut matrix = MockMatrix::new(64, 32);
    let mut date_engine = DateEngine::new(64, 32);

    for theme_id in [0, 18, 20, 21] {
        config.settings.write().date_theme = theme_id;
        matrix.clear();
        date_engine.render(&mut matrix, &config);
    }
}

#[test]
fn test_weather_engine_init() {
    let temp_file = NamedTempFile::new().unwrap();
    let config = Config::new(temp_file.path());
    let mut matrix = MockMatrix::new(64, 32);
    let mut engine = arcadematrix::engines::weather::WeatherEngine::new();

    // Render without API key (should draw "No API key")
    engine.render(&mut matrix, &config);
}

#[test]
fn test_message_engine() {
    let mut matrix = MockMatrix::new(64, 32);
    let mut engine = MessageEngine::new();
    let payload = MessagePayload {
        text: "Test Msg".to_string(),
        color: "#ff0000".to_string(), // Red string
        size: 1,
        direction: "left".to_string(),
        speed: 2,
        timeout_seconds: 5,
    };

    engine.render(&mut matrix, &payload);
}

#[test]
fn test_marquee_engine() {
    let mut matrix = MockMatrix::new(64, 32);
    let engine = MarqueeEngine::new();
    let img = RgbImage::new(64, 32);

    engine.render(&mut matrix, &img);
}

#[test]
fn test_gif_engine_init() {
    let mut gif_engine = GifEngine::new(64, 32);
    let mut matrix = MockMatrix::new(64, 32);

    gif_engine.render_next_frame(&mut matrix, std::time::Duration::from_millis(50));
    assert!(!gif_engine.load_gif("non_existent.gif"));
}

#[test]
fn test_fighter_engine_initialization() {
    let mut engine = FighterEngine::new(64);
    let mut matrix = MockMatrix::new(64, 32);
    engine.init_fight(32, 10);
    engine.composite(&mut matrix);
}

#[test]
fn test_dmd_cache_lookup() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = DmdCache::new(temp_dir.path());
    assert!(cache
        .get_marquee_path("invalid_sys", "invalid_game")
        .is_none());
}

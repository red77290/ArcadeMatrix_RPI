use arcadematrix::core::config::{Config, ConfigSettings};
use arcadematrix::engines::gif::GifEngine;
use std::fs;

fn setup_test_env() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let gifs_dir = dir.path().join("gifs");
    let data_dir = dir.path().join("data").join("gifs");

    fs::create_dir_all(&data_dir.join("Sonic")).unwrap();
    fs::create_dir_all(&data_dir.join("Mario")).unwrap();

    fs::write(data_dir.join("Sonic").join("run.gif"), b"fake_gif").unwrap();
    fs::write(data_dir.join("Sonic").join("idle.gif"), b"fake_gif").unwrap();
    fs::write(data_dir.join("Mario").join("jump.gif"), b"fake_gif").unwrap();

    #[cfg(unix)]
    std::os::unix::fs::symlink(&data_dir, &gifs_dir).unwrap();

    dir
}

#[test]
fn test_conf_ini_to_selected_gifs() {
    let dir = setup_test_env();
    let conf_path = dir.path().join("conf.ini");

    fs::write(
        &conf_path,
        "[IDLE]\nSELECTED_GIFS = gifs/Sonic, gifs/Mario\n",
    )
    .unwrap();

    let mut settings = ConfigSettings::default();
    Config::load_from_ini(&conf_path, &mut settings);

    assert_eq!(settings.selected_gifs.len(), 2);
    assert_eq!(settings.selected_gifs[0], "gifs/Sonic");
    assert_eq!(settings.selected_gifs[1], "gifs/Mario");
}

#[test]
fn test_gif_engine_playlist_selection() {
    let dir = setup_test_env();
    std::env::set_current_dir(dir.path()).unwrap();
    let mut engine = GifEngine::new(64, 32);

    let selected = vec!["gifs/Sonic".to_string()];
    let res = engine.play_random_playlist_gif(&selected);
    assert!(!res);

    let selected_empty = vec![];
    let res_fallback = engine.play_random_playlist_gif(&selected_empty);
    assert!(!res_fallback);

    // Test sanitization with weird quotes
    let selected_quotes = vec!["\"gifs/Mario\"".to_string()];
    let res_quotes = engine.play_random_playlist_gif(&selected_quotes);
    assert!(!res_quotes);

    // Test prepending gifs/
    let selected_no_prefix = vec!["Sonic".to_string()];
    let res_no_prefix = engine.play_random_playlist_gif(&selected_no_prefix);
    assert!(!res_no_prefix);
}

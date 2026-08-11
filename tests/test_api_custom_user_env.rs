use actix_web::{test, web, App};
use arcadematrix::api::server::{get_settings, post_settings};
use arcadematrix::core::config::Config;
use serde_json::json;
use std::sync::Arc;

// We mimic the path resolution logic from main.rs to test it here
pub fn resolve_working_dir(home_path: &str) -> bool {
    let mut dir_set = false;
    let dynamic_home_path = format!("{}/ArcadeMatrix_RPi", home_path);

    let common_paths = [
        dynamic_home_path.as_str(),
        "/home/pi/ArcadeMatrix_RPI",
        "/home/pi/ArcadeMatrix_RPi",
        "/opt/arcadematrix",
    ];
    for path in common_paths.iter() {
        let p = std::path::Path::new(path);
        if p.join("gifs").exists() && p.join("fonts").exists() {
            if std::env::set_current_dir(p).is_ok() {
                dir_set = true;
                break;
            }
        }
    }
    dir_set
}

#[actix_web::test]
async fn test_api_settings_custom_user_env() {
    // 1. Setup mock directory structure for a custom user
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let mock_home = temp_dir.path().join("home").join("customuser");
    let proj_dir = mock_home.join("ArcadeMatrix_RPi");

    std::fs::create_dir_all(&proj_dir).unwrap();
    std::fs::create_dir_all(proj_dir.join("gifs")).unwrap();
    std::fs::create_dir_all(proj_dir.join("fonts")).unwrap();
    std::fs::create_dir_all(proj_dir.join("data")).unwrap();

    let conf_path = proj_dir.join("data").join("conf.ini");
    std::fs::write(
        &conf_path,
        "[MATRIX]\nBRIGHTNESS = 50\n[API]\nAUTH_ENABLED = false\n",
    )
    .unwrap();

    // 2. Resolve working directory (this must succeed and point to our proj_dir)
    let home_str = mock_home.to_str().unwrap();
    assert!(
        resolve_working_dir(home_str),
        "Path resolution failed for custom user"
    );

    let current_dir = std::env::current_dir().unwrap();
    assert_eq!(
        current_dir,
        proj_dir.canonicalize().unwrap(),
        "Did not cd into custom user project directory"
    );

    // 3. Initialize Config and AppState
    let config = Arc::new(Config::new(&conf_path));
    let state = arcadematrix::api::server::AppState {
        config: config.clone(),
    };

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .service(get_settings)
            .service(post_settings),
    )
    .await;

    // 4. Test GET /api/settings
    let req = test::TestRequest::get().uri("/api/settings").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    // 5. Test POST /api/settings
    let payload = json!({
        "brightness_limit": 75,
        "clock_theme": 1,
        "weather_city": "Paris",
        "night_mode_enabled": true
    });

    let req = test::TestRequest::post()
        .uri("/api/settings")
        .set_json(&payload)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    // 6. Verify filesystem write
    let updated_conf = std::fs::read_to_string(&conf_path).unwrap();
    println!("UPDATED CONF INI: {}", updated_conf);

    assert!(
        updated_conf.contains("brightness=75"),
        "conf.ini not updated correctly for BRIGHTNESS!"
    );
    assert!(
        updated_conf.contains("theme=1"),
        "conf.ini not updated correctly for THEME!"
    );
    assert!(
        updated_conf.contains("city=Paris"),
        "conf.ini not updated correctly for CITY!"
    );

    println!("SUCCESS: API endpoints correctly resolve and mutate conf.ini in a custom user environment.");
}

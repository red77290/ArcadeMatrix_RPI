use crate::api::ota::{get_version, handle_update};
use crate::core::config::Config;
use actix_web::{get, post, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use serde_json::json;
use std::sync::Arc;
use sysinfo::System;

struct AppState {
    config: Arc<Config>,
}

fn check_auth(_req: &HttpRequest, _config: &Config) -> Result<(), HttpResponse> {
    // Authentication temporarily disabled per user request
    Ok(())
}

#[get("/api/fonts")]
async fn api_fonts() -> impl Responder {
    let mut fonts = Vec::new();
    if let Ok(entries) = std::fs::read_dir("fonts") {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                let lower_name = name.to_lowercase();
                if lower_name.ends_with(".ttf")
                    || lower_name.ends_with(".otf")
                    || lower_name.ends_with(".bdf")
                {
                    fonts.push(name);
                }
            }
        }
    }
    HttpResponse::Ok().json(fonts)
}

#[get("/api/settings")]
async fn get_settings(data: web::Data<AppState>) -> impl Responder {
    let s = data.config.settings.read();
    HttpResponse::Ok().json(json!({
        "brightness_limit": s.matrix_brightness,
        "color_depth": 24,
        "rotation": s.idle_rotation.join(","),
        "clock_offset_x": s.time_offset_x,
        "clock_offset_y": s.time_offset_y,
        "date_offset_x": s.date_offset_x,
        "date_offset_y": s.date_offset_y,
        "weather_offset_x": s.weather_offset_x,
        "weather_offset_y": s.weather_offset_y,
        "clock_size": s.time_size,
        "clock_font": s.time_font,
        "clock_theme": s.time_theme,
        "clock_color_1": s.clock_color_1,
        "clock_color_2": s.clock_color_2,
        "time_format": s.time_format,
        "date_size": s.date_size,
        "date_font": s.date_font,
        "date_theme": s.date_theme,
        "date_format": s.date_format,
        "date_color_1": s.date_color_1,
        "date_color_2": s.date_color_2,
        "night_mode_enabled": s.standby_enabled,
        "turn_off_at": s.standby_turn_off,
        "wake_up_at": s.standby_wake_up,
        "matrix_brightness_night": s.standby_night_brightness,
        "matrix_power": data.config.matrix_power.load(std::sync::atomic::Ordering::Relaxed),
        "matrix_brightness": s.matrix_brightness,
        "matrix_slowdown": s.matrix_slowdown,
        "matrix_rows": s.matrix_rows,
        "matrix_cols": s.matrix_cols,
        "matrix_chain": s.matrix_chain,
        "matrix_parallel": s.matrix_parallel,
        "matrix_mapping": s.matrix_mapping,
        "matrix_rgb_sequence": s.matrix_rgb_sequence,
        "matrix_pwm_bits": s.matrix_pwm_bits,
        "matrix_pwm_lsb_nanoseconds": s.matrix_pwm_lsb_nanoseconds,
        "mqtt_enabled": s.mqtt_enabled,
        "mqtt_broker": s.mqtt_broker,
        "mqtt_port": s.mqtt_port,
        "mqtt_user": s.mqtt_user,
        "clock_duration_sec": s.idle_clock_duration_sec,
        "date_duration_sec": s.idle_date_duration_sec,
        "weather_duration_sec": s.idle_weather_duration_sec,
        "gifs_count": s.idle_gifs_count,
        "sprite_count": s.idle_sprite_count,
        "fighter_interval_sec": s.idle_fighter_interval,
        "weather_api_key": s.weather_api_key,
        "weather_city": s.weather_city,
        "weather_lang": s.weather_lang,
    }))
}

#[post("/api/settings")]
async fn post_settings(
    data: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    let mut s = data.config.settings.write();
    let old_s = s.clone();

    // Matrix settings
    if let Some(v) = body.get("brightness_limit").and_then(|v| v.as_u64()) {
        s.matrix_brightness = v as u32;
        data.config
            .matrix_brightness
            .store(v as u32, std::sync::atomic::Ordering::Relaxed);
    }
    if let Some(v) = body.get("matrix_slowdown").and_then(|v| v.as_u64()) {
        s.matrix_slowdown = v as u32;
    }
    if let Some(v) = body.get("matrix_rows").and_then(|v| v.as_u64()) {
        s.matrix_rows = v as u32;
    }
    if let Some(v) = body.get("matrix_cols").and_then(|v| v.as_u64()) {
        s.matrix_cols = v as u32;
    }
    if let Some(v) = body.get("matrix_chain").and_then(|v| v.as_u64()) {
        s.matrix_chain = v as u32;
    }
    if let Some(v) = body.get("matrix_parallel").and_then(|v| v.as_u64()) {
        s.matrix_parallel = v as u32;
    }
    if let Some(v) = body.get("matrix_mapping").and_then(|v| v.as_str()) {
        s.matrix_mapping = v.to_string();
    }
    if let Some(v) = body.get("matrix_rgb_sequence").and_then(|v| v.as_str()) {
        s.matrix_rgb_sequence = v.to_string();
    }
    if let Some(v) = body.get("matrix_pwm_bits").and_then(|v| v.as_u64()) {
        s.matrix_pwm_bits = v as u32;
    }
    if let Some(v) = body
        .get("matrix_pwm_lsb_nanoseconds")
        .and_then(|v| v.as_u64())
    {
        s.matrix_pwm_lsb_nanoseconds = v as u32;
    }
    if let Some(v) = body
        .get("matrix_disable_hardware_pulsing")
        .and_then(|v| v.as_bool())
    {
        s.matrix_disable_hardware_pulsing = v;
    }

    // Clock settings
    if let Some(v) = body.get("clock_theme").and_then(|v| v.as_i64()) {
        s.time_theme = v as i32;
    }
    if let Some(v) = body.get("clock_font").and_then(|v| v.as_str()) {
        s.time_font = v.to_string();
    }
    if let Some(v) = body.get("time_size").and_then(|v| v.as_u64()) {
        s.time_size = v as u32;
    }
    if let Some(v) = body.get("ntp_server").and_then(|v| v.as_str()) {
        s.ntp_server = v.to_string();
    }
    if let Some(v) = body.get("timezone").and_then(|v| v.as_str()) {
        s.timezone = v.to_string();
    }
    if let Some(v) = body.get("clock_color_1").and_then(|v| v.as_str()) {
        s.clock_color_1 = v.to_string();
    }
    if let Some(v) = body.get("clock_color_2").and_then(|v| v.as_str()) {
        s.clock_color_2 = v.to_string();
    }
    if let Some(v) = body.get("clock_offset_x").and_then(|v| v.as_i64()) {
        s.time_offset_x = v as i32;
    }
    if let Some(v) = body.get("clock_offset_y").and_then(|v| v.as_i64()) {
        s.time_offset_y = v as i32;
    }
    if let Some(v) = body.get("time_format").and_then(|v| v.as_str()) {
        s.time_format = v.to_string();
    }

    // Date settings
    if let Some(v) = body.get("date_theme").and_then(|v| v.as_i64()) {
        s.date_theme = v as i32;
    }
    if let Some(v) = body.get("date_font").and_then(|v| v.as_str()) {
        s.date_font = v.to_string();
    }
    if let Some(v) = body.get("date_size").and_then(|v| v.as_u64()) {
        s.date_size = v as u32;
    }
    if let Some(v) = body.get("date_format").and_then(|v| v.as_str()) {
        s.date_format = v.to_string();
    }
    if let Some(v) = body.get("date_color_1").and_then(|v| v.as_str()) {
        s.date_color_1 = v.to_string();
    }
    if let Some(v) = body.get("date_color_2").and_then(|v| v.as_str()) {
        s.date_color_2 = v.to_string();
    }
    if let Some(v) = body.get("date_offset_x").and_then(|v| v.as_i64()) {
        s.date_offset_x = v as i32;
    }
    if let Some(v) = body.get("date_offset_y").and_then(|v| v.as_i64()) {
        s.date_offset_y = v as i32;
    }

    // Weather settings
    if let Some(v) = body.get("weather_api_key").and_then(|v| v.as_str()) {
        s.weather_api_key = v.to_string();
    }
    if let Some(v) = body.get("weather_city").and_then(|v| v.as_str()) {
        s.weather_city = v.to_string();
    }
    if let Some(v) = body.get("weather_lang").and_then(|v| v.as_str()) {
        s.weather_lang = v.to_string();
    }
    if let Some(v) = body.get("weather_offset_x").and_then(|v| v.as_i64()) {
        s.weather_offset_x = v as i32;
    }
    if let Some(v) = body.get("weather_offset_y").and_then(|v| v.as_i64()) {
        s.weather_offset_y = v as i32;
    }

    // Rotation & Idle settings
    if let Some(v) = body.get("rotation").and_then(|v| v.as_str()) {
        s.idle_rotation = v
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect();
    }
    if let Some(v) = body.get("clock_duration_sec").and_then(|v| v.as_u64()) {
        s.idle_clock_duration_sec = v as u32;
    }
    if let Some(v) = body.get("date_duration_sec").and_then(|v| v.as_u64()) {
        s.idle_date_duration_sec = v as u32;
    }
    if let Some(v) = body.get("weather_duration_sec").and_then(|v| v.as_u64()) {
        s.idle_weather_duration_sec = v as u32;
    }
    if let Some(v) = body.get("gifs_count").and_then(|v| v.as_u64()) {
        s.idle_gifs_count = v as u32;
    }
    if let Some(v) = body.get("idle_fighter_interval").and_then(|v| v.as_u64()) {
        s.idle_fighter_interval = v as u32;
    }
    if let Some(v) = body.get("idle_sprite_count").and_then(|v| v.as_u64()) {
        s.idle_sprite_count = v as u32;
    }

    // Standby / Night mode
    if let Some(v) = body.get("night_mode_enabled").and_then(|v| v.as_bool()) {
        s.standby_enabled = v;
    }
    if let Some(v) = body.get("turn_off_at").and_then(|v| v.as_str()) {
        s.standby_turn_off = v.to_string();
    }
    if let Some(v) = body.get("wake_up_at").and_then(|v| v.as_str()) {
        s.standby_wake_up = v.to_string();
    }
    if let Some(v) = body.get("matrix_brightness_night").and_then(|v| v.as_u64()) {
        s.standby_night_brightness = v as u32;
    }

    // MQTT settings
    if let Some(v) = body.get("mqtt_enable").and_then(|v| v.as_bool()) {
        s.mqtt_enabled = v;
    }
    if let Some(v) = body.get("mqtt_broker").and_then(|v| v.as_str()) {
        s.mqtt_broker = v.to_string();
    }
    if let Some(v) = body.get("mqtt_port").and_then(|v| v.as_u64()) {
        s.mqtt_port = v as u16;
    }
    if let Some(v) = body.get("mqtt_user").and_then(|v| v.as_str()) {
        s.mqtt_user = v.to_string();
    }
    if let Some(v) = body.get("mqtt_pass").and_then(|v| v.as_str()) {
        s.mqtt_pass = v.to_string();
    }

    // API settings
    if let Some(v) = body.get("api_auth_enabled").and_then(|v| v.as_bool()) {
        s.api_auth_enabled = v;
    }
    if let Some(v) = body.get("api_token").and_then(|v| v.as_str()) {
        s.api_token = v.to_string();
    }

    let needs_restart = old_s.matrix_slowdown != s.matrix_slowdown
        || old_s.matrix_rows != s.matrix_rows
        || old_s.matrix_cols != s.matrix_cols
        || old_s.matrix_chain != s.matrix_chain
        || old_s.matrix_parallel != s.matrix_parallel
        || old_s.matrix_mapping != s.matrix_mapping
        || old_s.matrix_rgb_sequence != s.matrix_rgb_sequence
        || old_s.matrix_pwm_bits != s.matrix_pwm_bits
        || old_s.matrix_pwm_lsb_nanoseconds != s.matrix_pwm_lsb_nanoseconds
        || old_s.matrix_disable_hardware_pulsing != s.matrix_disable_hardware_pulsing
        || old_s.mqtt_enabled != s.mqtt_enabled
        || old_s.mqtt_broker != s.mqtt_broker
        || old_s.mqtt_port != s.mqtt_port
        || old_s.mqtt_user != s.mqtt_user
        || old_s.mqtt_pass != s.mqtt_pass;

    drop(s);
    data.config.save();

    if needs_restart {
        data.config
            .reload_flag
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    HttpResponse::Ok().json(json!({"status": "success"}))
}

#[post("/api/clock")]
async fn post_clock(
    data: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    if let Some(theme) = body.get("clock_theme").and_then(|v| v.as_i64()) {
        let mut s = data.config.settings.write();
        s.time_theme = theme as i32;
        drop(s);
        data.config.save();
    }
    HttpResponse::Ok().json(json!({"status": "success"}))
}

#[post("/api/message")]
async fn post_message(
    data: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    *data.config.message_payload.lock() = Some(body.into_inner());
    *data.config.force_engine.lock() = Some("message".to_string());
    HttpResponse::Ok().json(json!({"status": "success"}))
}

#[get("/api/playlists")]
async fn get_playlists() -> impl Responder {
    let mut playlists = serde_json::Map::new();
    let gifs_dir = std::path::Path::new("gifs");
    if let Ok(entries) = std::fs::read_dir(gifs_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let path_str = entry.path().to_string_lossy().to_string();
                let mut count = 0;
                if let Ok(files) = std::fs::read_dir(entry.path()) {
                    count = files
                        .flatten()
                        .filter(|f| {
                            let name = f.file_name().to_string_lossy().to_string().to_lowercase();
                            name.ends_with(".gif") && !name.starts_with("._")
                        })
                        .count();
                }
                playlists.insert(
                    path_str.clone(),
                    json!({
                        "path": path_str,
                        "count": count
                    }),
                );
            }
        }
    }
    HttpResponse::Ok().json(playlists)
}

#[get("/api/playlists/selected")]
async fn get_selected_playlists(data: web::Data<AppState>) -> impl Responder {
    let s = data.config.settings.read();
    HttpResponse::Ok().json(json!({
        "playlists": s.selected_gifs
    }))
}

#[post("/api/playlists/save")]
async fn save_selected_playlists(
    data: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    if let Some(arr) = body.get("playlists").and_then(|v| v.as_array()) {
        let selected: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        let mut s = data.config.settings.write();
        s.selected_gifs = selected;
        drop(s);
        data.config.save();
    }
    HttpResponse::Ok().json(json!({"status": "success"}))
}

#[post("/api/playlists/play")]
async fn play_selected_playlists(
    data: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    if let Some(arr) = body.get("playlists").and_then(|v| v.as_array()) {
        let selected: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        let mut s = data.config.settings.write();
        s.selected_gifs = selected;
        drop(s);
        data.config.save();
        *data.config.force_engine.lock() = Some("gifs".to_string());
    }
    HttpResponse::Ok().json(json!({"status": "success"}))
}

#[get("/api/system_info")]
async fn api_system_info() -> impl Responder {
    let mut sys = System::new_all();
    sys.refresh_all();

    let cpu_load = sys.global_cpu_usage();
    let ram_used = sys.used_memory() / (1024 * 1024);
    let ram_total = sys.total_memory() / (1024 * 1024);

    let temp = sysinfo::Components::new_with_refreshed_list()
        .iter()
        .next()
        .map(|c| c.temperature())
        .unwrap_or(42.0);

    HttpResponse::Ok().json(json!({
        "cpu_load": cpu_load,
        "ram_used_mb": ram_used,
        "ram_total_mb": ram_total,
        "ram_percent": (ram_used as f32 / ram_total as f32 * 100.0) as u32,
        "disk_free_gb": 10.5,
        "disk_total_gb": 16.0,
        "disk_percent": 35,
        "temperature_c": temp,
    }))
}

#[post("/api/system/reboot")]
async fn api_reboot(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    std::process::Command::new("sh")
        .arg("-c")
        .arg("sleep 1 && sudo /sbin/reboot")
        .spawn()
        .ok();
    HttpResponse::Ok().json(json!({"status": "success", "message": "Rebooting..."}))
}

#[post("/api/system/shutdown")]
async fn api_shutdown(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    std::process::Command::new("sh")
        .arg("-c")
        .arg("sleep 1 && sudo /sbin/shutdown -h now")
        .spawn()
        .ok();
    HttpResponse::Ok().json(json!({"status": "success", "message": "Shutting down..."}))
}

#[post("/api/system/restart_app")]
async fn api_restart_app(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }

    // Set the reload flag to true, triggering a safe matrix drop and process exec in app.rs
    data.config
        .reload_flag
        .store(true, std::sync::atomic::Ordering::Relaxed);

    HttpResponse::Ok().json(json!({"status": "success", "message": "Restarting application..."}))
}

#[post("/api/wifi")]
async fn api_wifi(
    req: HttpRequest,
    data: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    let ssid = body.get("ssid").and_then(|v| v.as_str()).unwrap_or("");
    let pass = body.get("password").and_then(|v| v.as_str()).unwrap_or("");
    if ssid.is_empty() || pass.is_empty() {
        return HttpResponse::BadRequest()
            .json(json!({"status": "error", "message": "Missing ssid or password"}));
    }

    let output = tokio::process::Command::new("sudo")
        .args(["nmcli", "dev", "wifi", "connect", ssid, "password", pass])
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {
            let mut ws = data.config.settings.write();
            ws.wifi_ssid = ssid.to_string();
            ws.wifi_pass = pass.to_string();
            ws.wifi_configured = true;
            drop(ws);
            data.config.save();
            HttpResponse::Ok()
                .json(json!({"status": "success", "message": "Connected to Wi-Fi successfully!"}))
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            HttpResponse::InternalServerError().json(
                json!({"status": "error", "message": format!("Failed to connect: {}", stderr)}),
            )
        }
        Err(e) => HttpResponse::InternalServerError()
            .json(json!({"status": "error", "message": format!("Failed to execute nmcli: {}", e)})),
    }
}

#[post("/api/mqtt/install")]
async fn api_mqtt_install(
    req: HttpRequest,
    data: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    let target_ip = body.get("ip").and_then(|v| v.as_str()).unwrap_or("");
    if target_ip.is_empty() {
        return HttpResponse::BadRequest()
            .json(json!({"status": "error", "message": "No IP provided"}));
    }
    let matrix_ip = std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("10.255.255.255:1")?;
            s.local_addr()
        })
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string());

    match crate::core::ssh_installer::install_sync_script(target_ip, &matrix_ip) {
        Ok(msg) => HttpResponse::Ok().json(json!({"status": "success", "message": msg})),
        Err(msg) => HttpResponse::BadRequest().json(json!({"status": "error", "message": msg})),
    }
}

#[post("/api/marquee")]
async fn api_marquee(
    mut payload: actix_multipart::Multipart,
    data: web::Data<AppState>,
) -> impl Responder {
    use futures_util::stream::StreamExt;
    let mut image_data = Vec::new();
    while let Some(item) = payload.next().await {
        if let Ok(mut field) = item {
            if field.name() == Some("image") {
                while let Some(chunk) = field.next().await {
                    if let Ok(bytes) = chunk {
                        image_data.extend_from_slice(&bytes);
                    }
                }
                break;
            }
        }
    }

    if image_data.is_empty() {
        return HttpResponse::BadRequest()
            .json(json!({"status": "error", "message": "No image provided"}));
    }

    match image::load_from_memory(&image_data) {
        Ok(img) => {
            *data.config.image_obj.lock() = Some(img.to_rgb8());
            *data.config.force_engine.lock() = Some("marquee".to_string());
            data.config
                .reload_flag
                .store(true, std::sync::atomic::Ordering::Relaxed);
            HttpResponse::Ok().json(
                json!({"status": "success", "message": "Marquee image received and displayed"}),
            )
        }
        Err(e) => HttpResponse::BadRequest()
            .json(json!({"status": "error", "message": format!("Invalid image: {}", e)})),
    }
}

#[get("/api/sprites/playlists")]
async fn api_sprites_playlists() -> impl Responder {
    HttpResponse::Ok().json(json!({}))
}

#[get("/api/sprites/playlists/selected")]
async fn api_sprites_playlists_selected() -> impl Responder {
    HttpResponse::Ok().json(json!({"playlists": []}))
}

#[post("/api/sprites/playlists/save")]
async fn api_sprites_playlists_save() -> impl Responder {
    HttpResponse::Ok().json(json!({"status": "success"}))
}

#[post("/api/system/power")]
async fn api_power(
    data: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    if let Some(state) = body.get("state").and_then(|v| v.as_bool()) {
        data.config
            .matrix_power
            .store(state, std::sync::atomic::Ordering::Relaxed);
        data.config
            .reload_flag
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let p = data
        .config
        .matrix_power
        .load(std::sync::atomic::Ordering::Relaxed);
    HttpResponse::Ok().json(json!({"status": "success", "matrix_power": p}))
}

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "api/www/"]
struct WebAssets;

async fn serve_static(req: actix_web::HttpRequest) -> impl actix_web::Responder {
    let mut p = req.path().trim_start_matches('/').to_string();
    if p.is_empty() {
        p = "index.html".to_string();
    }
    match WebAssets::get(&p) {
        Some(content) => {
            let mime = mime_guess::from_path(&p).first_or_octet_stream();
            actix_web::HttpResponse::Ok()
                .content_type(mime.as_ref())
                .body(content.data.into_owned())
        }
        None => actix_web::HttpResponse::NotFound().body("404 Not Found"),
    }
}

pub async fn run_server(config: Arc<Config>, port: u16) -> std::io::Result<()> {
    let state = web::Data::new(AppState { config });
    let serve_dir = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("api/www");
    tracing::info!(
        "Starting web server on port {}, serving files from: {:?}",
        port,
        serve_dir
    );

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .service(api_fonts)
            .service(get_settings)
            .service(post_settings)
            .service(post_clock)
            .service(post_message)
            .service(get_playlists)
            .service(get_selected_playlists)
            .service(save_selected_playlists)
            .service(play_selected_playlists)
            .service(api_system_info)
            .service(api_reboot)
            .service(api_shutdown)
            .service(api_restart_app)
            .service(api_sprites_playlists)
            .service(api_sprites_playlists_selected)
            .service(api_sprites_playlists_save)
            .service(api_wifi)
            .service(api_mqtt_install)
            .service(api_marquee)
            .service(api_power)
            .service(get_version)
            .service(handle_update)
            .route("/", web::get().to(serve_static))
            .route("/{_:.*}", web::get().to(serve_static))
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}

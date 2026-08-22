use crate::core::config::{Config, EngineInstance, RotationEntry};
use crate::core::config_sanitizer::ConfigSanitizer;
use crate::core::registry::EngineRegistry;
use actix_multipart::Multipart;
use actix_web::{delete, get, post, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use futures_util::StreamExt;
use rust_embed::RustEmbed;
use serde_json::json;
use std::net::UdpSocket;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use sysinfo::System;

pub struct AppState {
    pub config: Arc<Config>,
}

/// Best-effort local IPv4 discovery (used to tell a game console where to push
/// MQTT marquee events). Falls back to loopback when offline.
fn get_local_ip() -> String {
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                return addr.ip().to_string();
            }
        }
    }
    "127.0.0.1".to_string()
}

fn check_auth(req: &HttpRequest, config: &Config) -> Result<(), HttpResponse> {
    let s = config.settings.read();
    if !s.api_auth_enabled {
        return Ok(());
    }

    let supplied = req
        .headers()
        .get("X-API-Token")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if supplied.is_empty() || supplied != s.api_token {
        return Err(HttpResponse::Unauthorized().json(json!({
            "status": "error",
            "message": "Missing or invalid X-API-Token header"
        })));
    }
    Ok(())
}

#[get("/api/system")]
async fn get_system(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    let s = data.config.settings.read();
    HttpResponse::Ok().json(json!({
        "system": s.system,
        "matrix": s.matrix,
        "mqtt": s.mqtt,
        "wifi": s.wifi,
        "api_auth_enabled": s.api_auth_enabled,
        "api_token": s.api_token
    }))
}

#[post("/api/system")]
async fn post_system(
    req: HttpRequest,
    data: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    let mut s = data.config.settings.write();
    // Snapshot the fields that require a hardware restart to take effect.
    let prev_matrix = serde_json::to_value(&s.matrix).ok();
    let prev_disable_wifi = s.wifi.disable_internal;
    // Simplified update (normally you'd merge the fields)
    if let Some(sys) = body.get("system") {
        if let Ok(sys_val) = serde_json::from_value(sys.clone()) {
            s.system = sys_val;
        }
    }
    if let Some(mat) = body.get("matrix") {
        if let Ok(mat_val) = serde_json::from_value(mat.clone()) {
            s.matrix = mat_val;
        }
    }
    if let Some(mq) = body.get("mqtt") {
        if let Ok(mq_val) = serde_json::from_value(mq.clone()) {
            s.mqtt = mq_val;
        }
    }
    if let Some(wf) = body.get("wifi") {
        if let Ok(wf_val) = serde_json::from_value(wf.clone()) {
            s.wifi = wf_val;
        }
    }
    // Top-level (non-nested) settings sent directly by the web UI.
    if let Some(v) = body.get("api_auth_enabled").and_then(|v| v.as_bool()) {
        s.api_auth_enabled = v;
    }
    if let Some(v) = body.get("api_token").and_then(|v| v.as_str()) {
        s.api_token = v.to_string();
    }
    // Fighter overlay toggle/interval (media page). Handled as top-level keys so
    // the UI can patch them without replacing the whole `system` object.
    if let Some(v) = body.get("idle_fighter_enabled").and_then(|v| v.as_bool()) {
        s.system.idle_fighter_enabled = v;
    }
    if let Some(v) = body.get("idle_fighter_interval").and_then(|v| v.as_u64()) {
        s.system.idle_fighter_interval = (v.max(1)) as u32;
    }
    // Live daytime brightness (0-100), applied immediately without a restart.
    if let Some(v) = body
        .get("brightness_limit")
        .or_else(|| body.get("brightness"))
        .and_then(|v| v.as_u64())
    {
        let clamped = v.min(100) as u32;
        data.config
            .matrix_brightness
            .store(clamped, Ordering::Relaxed);
        // Persist it too, so the saved brightness survives a restart instead of
        // reverting to the default when the config is reloaded from disk.
        s.system.day_brightness = clamped;
    }
    // Hardware-affecting settings only take effect after a full restart of the
    // render loop, so flag it when matrix params or the Wi-Fi radio state change.
    let new_matrix = serde_json::to_value(&s.matrix).ok();
    let needs_reload = prev_matrix != new_matrix || prev_disable_wifi != s.wifi.disable_internal;
    drop(s);
    data.config.save();
    if needs_reload {
        data.config.reload_flag.store(true, Ordering::Relaxed);
    }
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

#[get("/api/instances")]
async fn get_instances(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    let s = data.config.settings.read();
    HttpResponse::Ok().json(&s.instances)
}

#[post("/api/instances")]
async fn post_instances(
    req: HttpRequest,
    data: web::Data<AppState>,
    body: web::Json<EngineInstance>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    let new_inst = body.into_inner();
    // Reject instances referencing an engine that isn't registered in the
    // auto-discovery registry: they would silently never render.
    if EngineRegistry::get_descriptor(&new_inst.engine_id).is_none() {
        return HttpResponse::BadRequest().json(json!({
            "status": "error",
            "message": format!("Unknown engine_id '{}'", new_inst.engine_id)
        }));
    }
    let mut s = data.config.settings.write();
    if let Some(existing) = s
        .instances
        .iter_mut()
        .find(|i| i.instance_id == new_inst.instance_id)
    {
        *existing = new_inst;
    } else {
        s.instances.push(new_inst);
    }
    // Self-heal at write time: inject defaults, clamp/fallback out-of-range values.
    ConfigSanitizer::sanitize_instances(&mut s);
    drop(s);
    data.config.save();
    // Make the runtime pick up the new/edited instance immediately.
    data.config.reset_rotation.store(true, Ordering::Relaxed);
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

#[delete("/api/instances/{id}")]
async fn delete_instance(
    req: HttpRequest,
    data: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    let mut s = data.config.settings.write();
    s.instances.retain(|i| i.instance_id != *path);
    // Drop any rotation entries that referenced the removed instance so the
    // render loop never points at a dangling instance_id.
    s.rotation.retain(|r| r.instance_id != *path);
    drop(s);
    data.config.save();
    data.config.reset_rotation.store(true, Ordering::Relaxed);
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

#[get("/api/rotation")]
async fn get_rotation(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    let s = data.config.settings.read();
    HttpResponse::Ok().json(&s.rotation)
}

#[post("/api/rotation")]
async fn post_rotation(
    req: HttpRequest,
    data: web::Data<AppState>,
    body: web::Json<Vec<RotationEntry>>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    let mut s = data.config.settings.write();
    s.rotation = body.into_inner();
    drop(s);
    data.config.save();
    data.config.reset_rotation.store(true, Ordering::Relaxed);
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

#[get("/api/engines")]
async fn get_engines(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    HttpResponse::Ok().json(crate::core::registry::EngineRegistry::get_all_descriptors())
}

/// Runs a privileged power command, trying several candidates so it works
/// whether or not `sudo` is needed and regardless of the systemd PATH. Logs the
/// outcome instead of silently swallowing failures (the previous `.spawn().ok()`
/// hid the reason reboot/shutdown appeared to do nothing).
fn run_power_command(candidates: &[&[&str]]) -> bool {
    for cand in candidates {
        let (bin, args) = cand.split_first().expect("non-empty command");
        match std::process::Command::new(bin).args(args).spawn() {
            Ok(_) => {
                tracing::info!("Power command dispatched: {} {:?}", bin, args);
                return true;
            }
            Err(e) => {
                tracing::warn!("Power command '{}' failed: {}", bin, e);
            }
        }
    }
    tracing::error!("All power command candidates failed");
    false
}

#[post("/api/system/restart")]
async fn post_restart(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    // Gracefully stop the render loop; the process then exits and systemd
    // (Restart=always) brings the app back in a couple of seconds. This applies
    // hardware-level settings without a full OS reboot.
    tracing::info!("Application restart requested via API.");
    data.config.reload_flag.store(true, Ordering::Relaxed);
    HttpResponse::Ok().json(json!({"status": "restarting"}))
}

#[get("/api/action/reboot")]
async fn reboot(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    let ok = run_power_command(&[&["systemctl", "reboot"], &["reboot"], &["sudo", "reboot"]]);
    if ok {
        HttpResponse::Ok().json(json!({"status": "rebooting"}))
    } else {
        HttpResponse::InternalServerError().json(json!({"status": "error"}))
    }
}

#[post("/api/system/reboot")]
async fn post_reboot(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    let ok = run_power_command(&[&["systemctl", "reboot"], &["reboot"], &["sudo", "reboot"]]);
    if ok {
        HttpResponse::Ok().json(json!({"status": "rebooting"}))
    } else {
        HttpResponse::InternalServerError().json(json!({"status": "error"}))
    }
}

#[post("/api/system/shutdown")]
async fn post_shutdown(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    let ok = run_power_command(&[
        &["systemctl", "poweroff"],
        &["shutdown", "-h", "now"],
        &["sudo", "shutdown", "-h", "now"],
    ]);
    if ok {
        HttpResponse::Ok().json(json!({"status": "shutting_down"}))
    } else {
        HttpResponse::InternalServerError().json(json!({"status": "error"}))
    }
}

#[post("/api/system/power")]
async fn post_power(
    req: HttpRequest,
    data: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    // `state = true` means the panel should be ON.
    let state = body.get("state").and_then(|v| v.as_bool()).unwrap_or(true);
    data.config.matrix_power.store(state, Ordering::Relaxed);
    HttpResponse::Ok().json(json!({"status": "ok", "power": state}))
}

#[get("/api/stats")]
async fn get_stats(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }

    // Gather CPU/memory/disk/temperature off the async reactor thread.
    let stats = web::block(|| {
        let mut sys = System::new_all();
        // Two samples spaced by the minimum interval for a meaningful CPU load.
        sys.refresh_cpu_all();
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        sys.refresh_cpu_all();
        sys.refresh_memory();

        let cpu_load = sys.global_cpu_usage();
        let ram_used_mb = (sys.used_memory() / (1024 * 1024)) as u64;
        let ram_total_mb = (sys.total_memory() / (1024 * 1024)) as u64;

        // RPi exposes the SoC temperature via the thermal sysfs entry.
        let temperature_c = std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp")
            .ok()
            .and_then(|s| s.trim().parse::<f32>().ok())
            .map(|milli| milli / 1000.0)
            .unwrap_or(0.0);

        let disks = sysinfo::Disks::new_with_refreshed_list();
        let disk_free_gb = disks
            .list()
            .iter()
            .find(|d| d.mount_point() == std::path::Path::new("/"))
            .or_else(|| disks.list().first())
            .map(|d| d.available_space() as f64 / (1024.0 * 1024.0 * 1024.0))
            .unwrap_or(0.0);

        (
            cpu_load,
            ram_used_mb,
            ram_total_mb,
            temperature_c,
            disk_free_gb,
        )
    })
    .await
    .unwrap_or((0.0, 0, 0, 0.0, 0.0));

    HttpResponse::Ok().json(json!({
        "version": crate::core::build_info::VERSION,
        "arch": crate::core::build_info::ARCH,
        "uptime": sysinfo::System::uptime(),
        "cpu_load": stats.0,
        "ram_used_mb": stats.1,
        "ram_total_mb": stats.2,
        "memory_used": stats.1,
        "memory_total": stats.2,
        "temperature_c": stats.3,
        "disk_free_gb": (stats.4 * 10.0).round() / 10.0
    }))
}

#[post("/api/wifi")]
async fn post_wifi(
    req: HttpRequest,
    data: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    let ssid = body.get("ssid").and_then(|v| v.as_str()).unwrap_or("");
    let password = body.get("password").and_then(|v| v.as_str()).unwrap_or("");
    if ssid.is_empty() {
        return HttpResponse::BadRequest()
            .json(json!({"status": "error", "message": "SSID is required"}));
    }
    {
        let mut s = data.config.settings.write();
        s.wifi.ssid = ssid.to_string();
        s.wifi.password = password.to_string();
        // Force the startup routine to (re)apply the NetworkManager profile.
        s.wifi.configured = false;
    }
    data.config.save();
    // Restart so the boot Wi-Fi provisioning block re-runs with the new profile.
    data.config.reload_flag.store(true, Ordering::Relaxed);
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

#[post("/api/marquee")]
async fn post_marquee(
    req: HttpRequest,
    data: web::Data<AppState>,
    mut payload: Multipart,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    let mut bytes = Vec::new();
    while let Some(item) = payload.next().await {
        let mut field = match item {
            Ok(f) => f,
            Err(e) => {
                return HttpResponse::BadRequest()
                    .json(json!({"status": "error", "message": format!("Upload error: {}", e)}))
            }
        };
        while let Some(chunk) = field.next().await {
            match chunk {
                Ok(d) => bytes.extend_from_slice(&d),
                Err(e) => {
                    return HttpResponse::BadRequest()
                        .json(json!({"status": "error", "message": format!("Chunk error: {}", e)}))
                }
            }
        }
    }
    match image::load_from_memory(&bytes) {
        Ok(img) => {
            *data.config.image_obj.lock() = Some(img.to_rgb8());
            *data.config.force_engine.lock() = Some("marquee".to_string());
            HttpResponse::Ok().json(json!({"status": "ok"}))
        }
        Err(e) => HttpResponse::BadRequest()
            .json(json!({"status": "error", "message": format!("Invalid image: {}", e)})),
    }
}

#[post("/api/mqtt/install")]
async fn post_mqtt_install(
    req: HttpRequest,
    data: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    let ip = body
        .get("ip")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if ip.is_empty() {
        return HttpResponse::BadRequest()
            .json(json!({"status": "error", "message": "Console IP is required"}));
    }
    let user = body
        .get("user")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let pass = body
        .get("pass")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let matrix_ip = get_local_ip();

    let res = web::block(move || {
        crate::core::ssh_installer::install_sync_script(&ip, &matrix_ip, user, pass)
    })
    .await;

    match res {
        Ok(Ok(log)) => HttpResponse::Ok().json(json!({"status": "ok", "log": log})),
        Ok(Err(msg)) => HttpResponse::BadRequest().json(json!({"status": "error", "message": msg})),
        Err(e) => HttpResponse::InternalServerError()
            .json(json!({"status": "error", "message": e.to_string()})),
    }
}

#[post("/api/mqtt/logs")]
async fn post_mqtt_logs(
    req: HttpRequest,
    data: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    let ip = body
        .get("ip")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if ip.is_empty() {
        return HttpResponse::BadRequest()
            .json(json!({"status": "error", "message": "Console IP is required"}));
    }
    let user = body
        .get("user")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let pass = body
        .get("pass")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let res =
        web::block(move || crate::core::ssh_installer::fetch_sync_logs(&ip, user, pass)).await;

    match res {
        Ok(Ok(logs)) => HttpResponse::Ok().json(json!({"status": "ok", "logs": logs})),
        Ok(Err(msg)) => HttpResponse::BadRequest().json(json!({"status": "error", "message": msg})),
        Err(e) => HttpResponse::InternalServerError()
            .json(json!({"status": "error", "message": e.to_string()})),
    }
}

#[get("/api/fonts")]
async fn get_fonts(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }

    let mut fonts = Vec::new();
    if let Ok(entries) = std::fs::read_dir("fonts") {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_file() {
                    if let Some(ext) = entry.path().extension() {
                        if ext == "ttf" || ext == "bdf" {
                            if let Some(name) = entry.file_name().to_str() {
                                fonts.push(json!({"value": name, "label": name}));
                            }
                        }
                    }
                }
            }
        }
    }
    HttpResponse::Ok().json(fonts)
}

#[get("/api/playlists")]
async fn get_playlists(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }

    let mut playlists = Vec::new();
    if let Ok(entries) = std::fs::read_dir("gifs") {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        playlists.push(json!({"value": name, "label": name}));
                    }
                }
            }
        }
    }
    HttpResponse::Ok().json(playlists)
}

#[get("/api/themes")]
async fn get_themes(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    // Built automatically from the single theme source of truth: adding a new
    // ThemeInfo entry in core::theme makes it appear in the UI dropdown with no
    // other change required here.
    let themes: Vec<_> = crate::core::theme::all_themes()
        .iter()
        .map(|t| json!({"value": t.id.to_string(), "label": t.name}))
        .collect();
    HttpResponse::Ok().json(themes)
}

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
    let app_state = web::Data::new(AppState { config });
    HttpServer::new(move || {
        let cors = actix_cors::Cors::permissive();
        App::new()
            .wrap(cors)
            .app_data(app_state.clone())
            .service(get_system)
            .service(post_system)
            .service(get_instances)
            .service(post_instances)
            .service(delete_instance)
            .service(get_rotation)
            .service(post_rotation)
            .service(get_engines)
            .service(reboot)
            .service(post_reboot)
            .service(post_restart)
            .service(post_shutdown)
            .service(post_power)
            .service(get_stats)
            .service(get_fonts)
            .service(get_playlists)
            .service(get_themes)
            .service(post_wifi)
            .service(post_marquee)
            .service(post_mqtt_install)
            .service(post_mqtt_logs)
            .service(crate::api::ota::get_version)
            .service(crate::api::ota::handle_update)
            .route("/", web::get().to(serve_static))
            .route("/{_:.*}", web::get().to(serve_static))
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}

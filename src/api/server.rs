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
    // Snapshot the fields that require a hardware/service restart to take effect.
    let prev_matrix = serde_json::to_value(&s.matrix).ok();
    let prev_mqtt = serde_json::to_value(&s.mqtt).ok();
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
    if let Some(v) = body.get("lang").and_then(|v| v.as_str()) {
        s.system.lang = v.to_string();
    }
    // Fighter overlay toggle/interval (media page). Handled as top-level keys so
    // the UI can patch them without replacing the whole `system` object.
    if let Some(v) = body.get("idle_fighter_enabled").and_then(|v| v.as_bool()) {
        s.system.idle_fighter_enabled = v;
    }
    if let Some(v) = body.get("idle_fighter_interval").and_then(|v| v.as_u64()) {
        s.system.idle_fighter_interval = (v.max(1)) as u32;
    }
    if let Some(v) = body.get("night_mode_enabled").and_then(|v| v.as_bool()) {
        s.system.night_mode_enabled = v;
    }
    if let Some(v) = body.get("turn_off_at").and_then(|v| v.as_str()) {
        s.system.turn_off_at = v.to_string();
    }
    if let Some(v) = body.get("wake_up_at").and_then(|v| v.as_str()) {
        s.system.wake_up_at = v.to_string();
    }
    if let Some(v) = body.get("night_brightness").and_then(|v| v.as_u64()) {
        s.system.night_brightness = (v.min(100)) as u32;
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
    // Hardware & MQTT-affecting settings only take effect after a restart of the
    // render/network loops, so flag it when matrix params, MQTT broker, or Wi-Fi state change.
    let new_matrix = serde_json::to_value(&s.matrix).ok();
    let new_mqtt = serde_json::to_value(&s.mqtt).ok();
    let needs_reload = prev_matrix != new_matrix
        || prev_mqtt != new_mqtt
        || prev_disable_wifi != s.wifi.disable_internal;
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
    let mut new_inst = body.into_inner();
    if new_inst.engine_id == "sysinfo" || new_inst.engine_id == "sys_info" {
        new_inst.engine_id = "system_info".to_string();
    } else if new_inst.engine_id == "gif" {
        new_inst.engine_id = "gifs".to_string();
    } else if new_inst.engine_id == "cast" {
        new_inst.engine_id = "google_cast".to_string();
    }
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
    let mut rot = body.into_inner();
    for entry in &mut rot {
        entry.normalize();
    }
    let mut s = data.config.settings.write();
    s.rotation = rot;
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
/// whether or not `sudo` is needed and regardless of the systemd PATH. Verifies
/// command exit status and logs stderr before falling back to the next candidate.
fn run_power_command(candidates: &[&[&str]]) -> bool {
    for cand in candidates {
        let (bin, args) = cand.split_first().expect("non-empty command");
        match std::process::Command::new(bin).args(args).output() {
            Ok(output) => {
                if output.status.success() {
                    tracing::info!("Power command executed successfully: {} {:?}", bin, args);
                    return true;
                } else {
                    let err = String::from_utf8_lossy(&output.stderr);
                    tracing::warn!(
                        "Power command '{} {:?}' exited with status {}: {}",
                        bin,
                        args,
                        output.status,
                        err.trim()
                    );
                }
            }
            Err(e) => {
                tracing::warn!("Power command '{}' failed to start: {}", bin, e);
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
    let ok = run_power_command(&[
        &["systemctl", "reboot"],
        &["sudo", "systemctl", "reboot"],
        &["/bin/systemctl", "reboot"],
        &["/usr/bin/systemctl", "reboot"],
        &["reboot"],
        &["sudo", "reboot"],
        &["/sbin/reboot"],
        &["sudo", "/sbin/reboot"],
        &["shutdown", "-r", "now"],
        &["sudo", "shutdown", "-r", "now"],
        &["/sbin/shutdown", "-r", "now"],
        &["sudo", "/sbin/shutdown", "-r", "now"],
    ]);
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
    let ok = run_power_command(&[
        &["systemctl", "reboot"],
        &["sudo", "systemctl", "reboot"],
        &["/bin/systemctl", "reboot"],
        &["/usr/bin/systemctl", "reboot"],
        &["reboot"],
        &["sudo", "reboot"],
        &["/sbin/reboot"],
        &["sudo", "/sbin/reboot"],
        &["shutdown", "-r", "now"],
        &["sudo", "shutdown", "-r", "now"],
        &["/sbin/shutdown", "-r", "now"],
        &["sudo", "/sbin/shutdown", "-r", "now"],
    ]);
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
        &["sudo", "systemctl", "poweroff"],
        &["/bin/systemctl", "poweroff"],
        &["/usr/bin/systemctl", "poweroff"],
        &["poweroff"],
        &["sudo", "poweroff"],
        &["/sbin/poweroff"],
        &["sudo", "/sbin/poweroff"],
        &["shutdown", "-h", "now"],
        &["sudo", "shutdown", "-h", "now"],
        &["/sbin/shutdown", "-h", "now"],
        &["sudo", "/sbin/shutdown", "-h", "now"],
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

fn count_gifs_in_dir(path: &std::path::Path) -> usize {
    if let Ok(entries) = std::fs::read_dir(path) {
        entries
            .flatten()
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.to_lowercase().ends_with(".gif") && !name.starts_with("._")
            })
            .count()
    } else {
        0
    }
}

#[get("/api/playlists")]
async fn get_playlists(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }

    let mut playlists = Vec::new();
    // 1. Scan horizontal / yoko folders in gifs/
    if let Ok(entries) = std::fs::read_dir("gifs") {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        if !name.starts_with('.') && !name.starts_with('_') {
                            playlists.push(json!({
                                "value": format!("/gifs/{}", name),
                                "label": name,
                                "orientation": "yoko"
                            }));
                        }
                    }
                }
            }
        }
    }

    // 2. Scan vertical / tate folders in gifs_tate/
    if let Ok(entries) = std::fs::read_dir("gifs_tate") {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        if !name.starts_with('.') && !name.starts_with('_') {
                            playlists.push(json!({
                                "value": format!("/gifs_tate/{}", name),
                                "label": name,
                                "orientation": "tate"
                            }));
                        }
                    }
                }
            }
        }
    }

    HttpResponse::Ok().json(playlists)
}

#[get("/api/gifs/playlists")]
async fn get_gifs_playlists(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }

    let mut yoko_map = serde_json::Map::new();
    if let Ok(entries) = std::fs::read_dir("gifs") {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        if !name.starts_with('.') && !name.starts_with('_') {
                            let count = count_gifs_in_dir(&entry.path());
                            yoko_map.insert(
                                name.to_string(),
                                json!({"path": format!("/gifs/{}", name), "count": count}),
                            );
                        }
                    }
                }
            }
        }
    }

    let mut tate_map = serde_json::Map::new();
    if let Ok(entries) = std::fs::read_dir("gifs_tate") {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        if !name.starts_with('.') && !name.starts_with('_') {
                            let count = count_gifs_in_dir(&entry.path());
                            tate_map.insert(
                                name.to_string(),
                                json!({"path": format!("/gifs_tate/{}", name), "count": count}),
                            );
                        }
                    }
                }
            }
        }
    }

    HttpResponse::Ok().json(json!({
        "yoko": yoko_map,
        "tate": tate_map
    }))
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

#[get("/api/timezones")]
async fn get_timezones() -> impl actix_web::Responder {
    let zones = [
        ("Europe/Paris", "Europe/Paris (UTC+1/+2)"),
        ("Europe/London", "Europe/London (UTC+0/+1)"),
        ("Europe/Dublin", "Europe/Dublin (UTC+0/+1)"),
        ("Europe/Lisbon", "Europe/Lisbon (UTC+0/+1)"),
        ("Europe/Berlin", "Europe/Berlin (UTC+1/+2)"),
        ("Europe/Madrid", "Europe/Madrid (UTC+1/+2)"),
        ("Europe/Rome", "Europe/Rome (UTC+1/+2)"),
        ("Europe/Brussels", "Europe/Brussels (UTC+1/+2)"),
        ("Europe/Amsterdam", "Europe/Amsterdam (UTC+1/+2)"),
        ("Europe/Zurich", "Europe/Zurich (UTC+1/+2)"),
        ("Europe/Vienna", "Europe/Vienna (UTC+1/+2)"),
        ("Europe/Warsaw", "Europe/Warsaw (UTC+1/+2)"),
        ("Europe/Prague", "Europe/Prague (UTC+1/+2)"),
        ("Europe/Stockholm", "Europe/Stockholm (UTC+1/+2)"),
        ("Europe/Oslo", "Europe/Oslo (UTC+1/+2)"),
        ("Europe/Copenhagen", "Europe/Copenhagen (UTC+1/+2)"),
        ("Europe/Athens", "Europe/Athens (UTC+2/+3)"),
        ("Europe/Helsinki", "Europe/Helsinki (UTC+2/+3)"),
        ("Europe/Bucharest", "Europe/Bucharest (UTC+2/+3)"),
        ("Europe/Kyiv", "Europe/Kyiv (UTC+2/+3)"),
        ("Europe/Moscow", "Europe/Moscow (UTC+3)"),
        ("Europe/Istanbul", "Europe/Istanbul (UTC+3)"),
        ("Atlantic/Reykjavik", "Atlantic/Reykjavik (UTC+0)"),
        ("Atlantic/Azores", "Atlantic/Azores (UTC-1/+0)"),
        ("America/New_York", "America/New_York (EST/EDT, UTC-5/-4)"),
        ("America/Detroit", "America/Detroit (EST/EDT, UTC-5/-4)"),
        (
            "America/Indiana/Indianapolis",
            "America/Indiana/Indianapolis (EST/EDT, UTC-5/-4)",
        ),
        ("America/Montreal", "America/Montreal (EST/EDT, UTC-5/-4)"),
        ("America/Toronto", "America/Toronto (EST/EDT, UTC-5/-4)"),
        ("America/Chicago", "America/Chicago (CST/CDT, UTC-6/-5)"),
        ("America/Mexico_City", "America/Mexico_City (CST, UTC-6)"),
        ("America/Denver", "America/Denver (MST/MDT, UTC-7/-6)"),
        ("America/Boise", "America/Boise (MST/MDT, UTC-7/-6)"),
        ("America/Phoenix", "America/Phoenix (MST, UTC-7, no DST)"),
        (
            "America/Los_Angeles",
            "America/Los_Angeles (PST/PDT, UTC-8/-7)",
        ),
        ("America/Vancouver", "America/Vancouver (PST/PDT, UTC-8/-7)"),
        (
            "America/Anchorage",
            "America/Anchorage (AKST/AKDT, UTC-9/-8)",
        ),
        ("America/Halifax", "America/Halifax (AST/ADT, UTC-4/-3)"),
        (
            "America/St_Johns",
            "America/St_Johns (NST/NDT, UTC-3:30/-2:30)",
        ),
        ("Pacific/Honolulu", "Pacific/Honolulu (HST, UTC-10)"),
        ("America/Sao_Paulo", "America/Sao_Paulo (BRT, UTC-3)"),
        ("America/Buenos_Aires", "America/Buenos_Aires (ART, UTC-3)"),
        ("America/Santiago", "America/Santiago (CLT/CLST, UTC-4/-3)"),
        ("America/Bogota", "America/Bogota (COT, UTC-5)"),
        ("America/Lima", "America/Lima (PET, UTC-5)"),
        ("Africa/Casablanca", "Africa/Casablanca (WEST, UTC+1)"),
        ("Africa/Cairo", "Africa/Cairo (EET/EEST, UTC+2/+3)"),
        ("Africa/Johannesburg", "Africa/Johannesburg (SAST, UTC+2)"),
        ("Africa/Nairobi", "Africa/Nairobi (EAT, UTC+3)"),
        ("Africa/Lagos", "Africa/Lagos (WAT, UTC+1)"),
        ("Asia/Jerusalem", "Asia/Jerusalem (IST/IDT, UTC+2/+3)"),
        ("Asia/Riyadh", "Asia/Riyadh (AST, UTC+3)"),
        ("Asia/Dubai", "Asia/Dubai (GST, UTC+4)"),
        ("Asia/Tehran", "Asia/Tehran (IRST, UTC+3:30)"),
        ("Asia/Karachi", "Asia/Karachi (PKT, UTC+5)"),
        ("Asia/Kolkata", "Asia/Kolkata (IST, UTC+5:30)"),
        ("Asia/Dhaka", "Asia/Dhaka (BST, UTC+6)"),
        ("Asia/Bangkok", "Asia/Bangkok (ICT, UTC+7)"),
        ("Asia/Jakarta", "Asia/Jakarta (WIB, UTC+7)"),
        ("Asia/Singapore", "Asia/Singapore (SGT, UTC+8)"),
        ("Asia/Hong_Kong", "Asia/Hong_Kong (HKT, UTC+8)"),
        ("Asia/Shanghai", "Asia/Shanghai (CST, UTC+8)"),
        ("Asia/Taipei", "Asia/Taipei (CST, UTC+8)"),
        ("Asia/Manila", "Asia/Manila (PST, UTC+8)"),
        ("Asia/Tokyo", "Asia/Tokyo (JST, UTC+9)"),
        ("Asia/Seoul", "Asia/Seoul (KST, UTC+9)"),
        (
            "Australia/Sydney",
            "Australia/Sydney (AEST/AEDT, UTC+10/+11)",
        ),
        (
            "Australia/Melbourne",
            "Australia/Melbourne (AEST/AEDT, UTC+10/+11)",
        ),
        ("Australia/Brisbane", "Australia/Brisbane (AEST, UTC+10)"),
        (
            "Australia/Adelaide",
            "Australia/Adelaide (ACST/ACDT, UTC+9:30/+10:30)",
        ),
        ("Australia/Perth", "Australia/Perth (AWST, UTC+8)"),
        ("Pacific/Guam", "Pacific/Guam (ChST, UTC+10)"),
        (
            "Pacific/Auckland",
            "Pacific/Auckland (NZST/NZDT, UTC+12/+13)",
        ),
        ("Pacific/Fiji", "Pacific/Fiji (FJT, UTC+12)"),
        ("UTC", "UTC (Coordinated Universal Time)"),
    ];
    let res: Vec<serde_json::Value> = zones
        .iter()
        .map(|(val, lbl)| json!({"value": val, "label": lbl}))
        .collect();
    HttpResponse::Ok().json(res)
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
            .service(get_gifs_playlists)
            .service(get_themes)
            .service(get_timezones)
            .service(post_wifi)
            .service(post_marquee)
            .service(post_mqtt_install)
            .service(post_mqtt_logs)
            .service(crate::api::ota::get_version)
            .service(crate::api::ota::check_update)
            .service(crate::api::ota::auto_update)
            .service(crate::api::ota::handle_update)
            .route("/", web::get().to(serve_static))
            .route("/{_:.*}", web::get().to(serve_static))
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}

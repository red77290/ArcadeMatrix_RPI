use crate::core::config::{Config, EngineInstance, RotationEntry};
use crate::core::config_sanitizer::ConfigSanitizer;
use crate::core::registry::EngineRegistry;
use actix_multipart::Multipart;
use actix_web::{delete, get, post, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use futures_util::StreamExt;
use rust_embed::RustEmbed;
use serde_json::json;
use std::net::UdpSocket;
use std::path::Path;
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

const REDACTED_SECRET: &str = "••••••••";
const MAX_UPLOAD_BYTES: usize = 25 * 1024 * 1024;
const UPLOAD_EXT_ALLOWLIST: &[&str] = &[
    "gif", "png", "jpg", "jpeg", "bmp", "webp", "raw", "ttf", "bdf", "fgt", "gz",
];

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn generate_fallback_api_token() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id() as u128;
    format!("{:016x}{:016x}", nanos, pid ^ 0xa5a5_5a5a_f0f0_0f0f)
}

pub(crate) fn check_auth(req: &HttpRequest, config: &Config) -> Result<(), HttpResponse> {
    let s = config.settings.read();
    if !s.api_auth_enabled || s.api_token.is_empty() {
        return Ok(());
    }

    let supplied = req
        .headers()
        .get("X-API-Token")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if supplied.is_empty() || !constant_time_eq(supplied.as_bytes(), s.api_token.as_bytes()) {
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
        "data_mqtt": s.data_mqtt,
        "showcase": s.showcase,
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
    // Snapshot the fields that require a hardware/service restart to take effect,
    // and preserve existing secrets when a client submits masked placeholders.
    let prev_matrix = serde_json::to_value(&s.matrix).ok();
    let prev_mqtt = serde_json::to_value(&s.mqtt).ok();
    let prev_showcase = serde_json::to_value(&s.showcase).ok();
    let prev_disable_wifi = s.wifi.disable_internal;
    let prev_mqtt_pass = s.mqtt.pass.clone();
    let prev_data_mqtt_pass = s.data_mqtt.pass.clone();
    let prev_wifi_pass = s.wifi.password.clone();
    let prev_api_token = s.api_token.clone();

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
        if let Ok(mut mq_val) =
            serde_json::from_value::<crate::core::config::MqttConfig>(mq.clone())
        {
            if mq_val.pass == REDACTED_SECRET {
                mq_val.pass = prev_mqtt_pass;
            }
            s.mqtt = mq_val;
        }
    }
    // Data MQTT credentials: read by the MQTT Data engine on its next
    // activation, so no restart is needed.
    if let Some(dm) = body.get("data_mqtt") {
        if let Ok(mut dm_val) =
            serde_json::from_value::<crate::core::config::DataMqttConfig>(dm.clone())
        {
            if dm_val.pass == REDACTED_SECRET {
                dm_val.pass = prev_data_mqtt_pass;
            }
            s.data_mqtt = dm_val;
        }
    }
    if let Some(sc) = body.get("showcase") {
        if let Ok(sc_val) =
            serde_json::from_value::<crate::core::config::ShowcaseConfig>(sc.clone())
        {
            s.showcase = sc_val;
        }
    }
    if let Some(wf) = body.get("wifi") {
        if let Ok(mut wf_val) =
            serde_json::from_value::<crate::core::config::WifiConfig>(wf.clone())
        {
            if wf_val.password == REDACTED_SECRET {
                wf_val.password = prev_wifi_pass;
            }
            s.wifi = wf_val;
        }
    }
    // Top-level (non-nested) settings sent directly by the web UI.
    if let Some(v) = body.get("api_auth_enabled").and_then(|v| v.as_bool()) {
        s.api_auth_enabled = v;
    }
    if let Some(v) = body.get("api_token").and_then(|v| v.as_str()) {
        if v != REDACTED_SECRET {
            s.api_token = v.to_string();
        } else {
            s.api_token = prev_api_token;
        }
    }
    if s.api_auth_enabled && s.api_token.trim().is_empty() {
        s.api_token = generate_fallback_api_token();
    }
    if let Some(v) = body.get("lang").and_then(|v| v.as_str()) {
        s.system.lang = v.to_string();
    }
    if let Some(v) = body.get("timezone").and_then(|v| v.as_str()) {
        s.system.timezone = v.to_string();
    }
    if let Some(v) = body.get("format_24h").and_then(|v| v.as_bool()) {
        s.system.format_24h = v;
    }
    if let Some(v) = body.get("temp_unit").and_then(|v| v.as_str()) {
        s.system.temp_unit = v.to_string();
    }
    // Fighter overlay toggle/interval (media page). Handled as top-level keys so
    // the UI can patch them without replacing the whole `system` object.
    if let Some(v) = body.get("idle_fighter_enabled").and_then(|v| v.as_bool()) {
        s.system.idle_fighter_enabled = v;
    }
    if let Some(v) = body.get("idle_fighter_interval").and_then(|v| v.as_u64()) {
        s.system.idle_fighter_interval = (v.max(1)) as u32;
    }
    if let Some(v) = body.get("idle_fighter_speed").and_then(|v| v.as_u64()) {
        s.system.idle_fighter_speed = (v.clamp(25, 200)) as u32;
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
    // Real-time display orientation & transitions (applied immediately without a restart).
    if let Some(v) = body.get("rotation").and_then(|v| v.as_u64()) {
        if s.matrix.rotation != v as u32 {
            s.matrix.rotation = v as u32;
        }
    }
    if let Some(v) = body.get("transition_effect").and_then(|v| v.as_str()) {
        s.matrix.transition_effect = v.to_string();
    }
    if let Some(v) = body.get("transition_duration_ms").and_then(|v| v.as_u64()) {
        s.matrix.transition_duration_ms = v as u32;
    }
    if let Some(v) = body.get("slot_transition").and_then(|v| v.as_str()) {
        s.matrix.slot_transition = v.to_string();
    }
    if let Some(v) = body
        .get("slot_transition_duration_ms")
        .and_then(|v| v.as_u64())
    {
        s.matrix.slot_transition_duration_ms = v as u32;
    }
    // Hardware & MQTT-affecting settings only take effect after a restart of the
    // render/network loops, so flag it when physical matrix params, MQTT broker, or Wi-Fi state change.
    let prev_matrix_cfg: Option<crate::core::config::MatrixConfig> =
        prev_matrix.and_then(|v| serde_json::from_value(v).ok());
    let hw_matrix_changed = if let Some(ref p) = prev_matrix_cfg {
        p.width != s.matrix.width
            || p.height != s.matrix.height
            || p.chain_length != s.matrix.chain_length
            || p.driver_chip != s.matrix.driver_chip
            || p.mapping != s.matrix.mapping
            || p.rgb_sequence != s.matrix.rgb_sequence
            || p.row_address_mode != s.matrix.row_address_mode
            || p.multiplexing != s.matrix.multiplexing
            || p.slowdown != s.matrix.slowdown
            || p.pwm_bits != s.matrix.pwm_bits
            || p.pwm_lsb_nanoseconds != s.matrix.pwm_lsb_nanoseconds
            || p.limit_refresh_rate_hz != s.matrix.limit_refresh_rate_hz
            || p.disable_hardware_pulsing != s.matrix.disable_hardware_pulsing
            || p.render_pipeline != s.matrix.render_pipeline
    } else {
        false
    };
    let new_mqtt = serde_json::to_value(&s.mqtt).ok();
    let new_showcase = serde_json::to_value(&s.showcase).ok();
    let mqtt_or_showcase_changed = prev_mqtt != new_mqtt || prev_showcase != new_showcase;
    let needs_reload = hw_matrix_changed || prev_disable_wifi != s.wifi.disable_internal;
    drop(s);
    if !data.config.save() {
        return HttpResponse::InternalServerError().json(json!({
            "status": "error",
            "message": "Failed to persist configuration to disk"
        }));
    }
    if mqtt_or_showcase_changed {
        crate::engines::frontend_sync::notify_mqtt_config_changed();
    }
    // Always notify the render loop to refresh its runtime snapshot (night mode,
    // language, units, fighter overlay, slot transition, display orientation).
    data.config.reset_rotation.store(true, Ordering::Relaxed);
    if needs_reload {
        data.config.reload_flag.store(true, Ordering::Relaxed);
    }
    HttpResponse::Ok().json(json!({"status": if needs_reload { "rebooting" } else { "ok" }}))
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
        for (k, v) in new_inst.config.iter_mut() {
            if *v == REDACTED_SECRET {
                if let Some(old_val) = existing.config.get(k) {
                    *v = old_val.clone();
                }
            }
        }
        *existing = new_inst;
    } else {
        let same_engine_cfg = s
            .instances
            .iter()
            .find(|i| i.engine_id == new_inst.engine_id)
            .map(|i| i.config.clone());
        if let Some(fallback_cfg) = same_engine_cfg {
            for (k, v) in new_inst.config.iter_mut() {
                if *v == REDACTED_SECRET {
                    if let Some(old_val) = fallback_cfg.get(k) {
                        *v = old_val.clone();
                    }
                }
            }
        }
        s.instances.push(new_inst);
    }
    // Self-heal at write time: inject defaults, clamp/fallback out-of-range values.
    ConfigSanitizer::sanitize_instances(&mut s);
    drop(s);
    if !data.config.save() {
        return HttpResponse::InternalServerError().json(json!({
            "status": "error",
            "message": "Failed to persist configuration to disk"
        }));
    }
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
    if !data.config.save() {
        return HttpResponse::InternalServerError().json(json!({
            "status": "error",
            "message": "Failed to persist configuration to disk"
        }));
    }
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
    ConfigSanitizer::sanitize_instances(&mut s);
    drop(s);
    if !data.config.save() {
        return HttpResponse::InternalServerError().json(json!({
            "status": "error",
            "message": "Failed to persist configuration to disk"
        }));
    }
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
    // Avoid `System::new_all()` which enumerates every process, thread, user, and network
    // interface in `/proc` on every poll; only refresh CPU and memory specifics.
    let stats = web::block(|| {
        let mut sys = System::new_with_specifics(
            sysinfo::RefreshKind::new()
                .with_cpu(sysinfo::CpuRefreshKind::everything())
                .with_memory(sysinfo::MemoryRefreshKind::everything()),
        );
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
    if ssid.contains(['\r', '\n', '\0'])
        || password.contains(['\r', '\n', '\0'])
        || ssid.starts_with('-')
        || password.starts_with('-')
    {
        return HttpResponse::BadRequest().json(json!({
            "status": "error",
            "message": "Invalid characters in SSID or password"
        }));
    }
    {
        let mut s = data.config.settings.write();
        s.wifi.ssid = ssid.to_string();
        if password != REDACTED_SECRET {
            s.wifi.password = password.to_string();
        }
        // Force the startup routine to (re)apply the NetworkManager profile.
        s.wifi.configured = false;
    }
    data.config.save();
    // Restart so the boot Wi-Fi provisioning block re-runs with the new profile.
    data.config.reload_flag.store(true, Ordering::Relaxed);
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

#[post("/api/message")]
async fn post_message(
    req: HttpRequest,
    body: web::Json<serde_json::Value>,
    data: web::Data<AppState>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    let text = body
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if text.is_empty() {
        data.config.set_message_payload(None);
        return HttpResponse::Ok().json(json!({"status": "ok", "cleared": true}));
    }
    let color = body
        .get("color")
        .and_then(|v| v.as_str())
        .unwrap_or("#ffffff")
        .to_string();
    let size = body
        .get("size")
        .or_else(|| body.get("font_size"))
        .and_then(|v| v.as_u64())
        .unwrap_or(1)
        .clamp(1, 5) as u32;
    let direction = body
        .get("direction")
        .and_then(|v| v.as_str())
        .unwrap_or("rtl")
        .to_string();
    let speed = body
        .get("speed")
        .and_then(|v| v.as_u64())
        .unwrap_or(40)
        .clamp(1, 500) as u32;
    let timeout_seconds = body
        .get("timeout")
        .or_else(|| body.get("timeout_seconds"))
        .or_else(|| body.get("timeoutSeconds"))
        .or_else(|| body.get("duration"))
        .and_then(|v| v.as_u64())
        .unwrap_or(10) as u32;

    let payload = crate::engines::message::MessagePayload {
        text,
        color,
        size,
        direction,
        speed,
        timeout_seconds,
    };
    data.config.set_message_payload(Some(payload));
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

async fn handle_file_upload(
    req: HttpRequest,
    data: web::Data<AppState>,
    mut payload: Multipart,
) -> HttpResponse {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    let mut bytes = Vec::new();
    let mut file_ext = String::new();

    while let Some(item) = payload.next().await {
        let mut field = match item {
            Ok(f) => f,
            Err(e) => {
                return HttpResponse::BadRequest()
                    .json(json!({"status": "error", "message": format!("Upload error: {}", e)}));
            }
        };

        if file_ext.is_empty() {
            if let Some(cd) = field.content_disposition() {
                if let Some(fname) = cd.get_filename() {
                    let p = std::path::Path::new(fname);
                    if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
                        let ext_lower = ext.to_lowercase();
                        if !UPLOAD_EXT_ALLOWLIST.contains(&ext_lower.as_str()) {
                            return HttpResponse::BadRequest().json(json!({
                                "status": "error",
                                "message": format!("Unsupported file extension '.{}'", ext_lower)
                            }));
                        }
                        file_ext = format!(".{}", ext_lower);
                    }
                }
            }
        }

        while let Some(chunk) = field.next().await {
            match chunk {
                Ok(d) => {
                    if bytes.len() + d.len() > MAX_UPLOAD_BYTES {
                        return HttpResponse::PayloadTooLarge().json(json!({
                            "status": "error",
                            "message": "Uploaded file exceeds maximum allowed size (25 MB)"
                        }));
                    }
                    bytes.extend_from_slice(&d);
                }
                Err(e) => {
                    return HttpResponse::BadRequest().json(
                        json!({"status": "error", "message": format!("Chunk error: {}", e)}),
                    );
                }
            }
        }
    }

    if bytes.is_empty() {
        return HttpResponse::BadRequest()
            .json(json!({"status": "error", "message": "No file received"}));
    }

    if file_ext.is_empty() {
        if bytes.starts_with(b"GIF8") {
            file_ext = ".gif".to_string();
        } else if bytes.starts_with(b"\x89PNG") {
            file_ext = ".png".to_string();
        } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            file_ext = ".jpg".to_string();
        } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            file_ext = ".webp".to_string();
        } else if bytes.starts_with(b"BM") {
            file_ext = ".bmp".to_string();
        } else if bytes.starts_with(&[0x1F, 0x8B]) {
            file_ext = ".gz".to_string();
        } else {
            return HttpResponse::BadRequest().json(json!({
                "status": "error",
                "message": "Unrecognized or unsupported file format"
            }));
        }
    }

    let is_marquee =
        req.path().contains("marquee") || req.query_string().contains("target=marquee");
    let target_dir = if is_marquee {
        std::path::Path::new("data/marquees")
    } else {
        std::path::Path::new("data/uploads")
    };
    if let Err(e) = std::fs::create_dir_all(target_dir) {
        return HttpResponse::InternalServerError().json(
            json!({"status": "error", "message": format!("Failed to create directory: {}", e)}),
        );
    }

    let dest_path = if is_marquee {
        target_dir.join(format!("marquee{}", file_ext))
    } else {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        target_dir.join(format!("asset_{}{}", ts, file_ext))
    };

    let tmp_path = dest_path.with_extension(format!("{}.tmp", file_ext.trim_start_matches('.')));
    if let Err(e) = std::fs::write(&tmp_path, &bytes) {
        let _ = std::fs::remove_file(&tmp_path);
        return HttpResponse::InternalServerError()
            .json(json!({"status": "error", "message": format!("Failed to save file: {}", e)}));
    }
    if let Err(e) = std::fs::rename(&tmp_path, &dest_path) {
        let _ = std::fs::remove_file(&tmp_path);
        return HttpResponse::InternalServerError()
            .json(json!({"status": "error", "message": format!("Failed to commit file: {}", e)}));
    }

    if is_marquee {
        // Remove any previous marquee files with different extensions after the new file is committed
        if let Ok(entries) = std::fs::read_dir(target_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p != dest_path {
                    let _ = std::fs::remove_file(p);
                }
            }
        }
        // Clear stale static image buffer if marquee
        data.config.set_marquee_image(None);
        // Do NOT set ForcedEngineMode::Marquee: Marquee runs normally in the rotation loop
    }

    HttpResponse::Ok().json(json!({
        "status": "ok",
        "success": true,
        "path": dest_path.to_string_lossy().to_string(),
        "message": "File uploaded successfully"
    }))
}

#[post("/api/marquee")]
async fn post_marquee(
    req: HttpRequest,
    data: web::Data<AppState>,
    payload: Multipart,
) -> impl Responder {
    handle_file_upload(req, data, payload).await
}

#[post("/api/upload")]
async fn post_upload(
    req: HttpRequest,
    data: web::Data<AppState>,
    payload: Multipart,
) -> impl Responder {
    handle_file_upload(req, data, payload).await
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
    let target_os = body
        .get("os")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let matrix_ip = get_local_ip();

    let res = web::block(move || {
        crate::core::ssh_installer::install_sync_script(&ip, &matrix_ip, user, pass, target_os)
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
    let target_os = body
        .get("os")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let res =
        web::block(move || crate::core::ssh_installer::fetch_sync_logs(&ip, user, pass, target_os))
            .await;

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

pub(crate) fn find_gif_candidate_roots(orientation: &str) -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/pi".to_string());

    let subs: &[&str] = if orientation == "tate" {
        &["gifs_tate", "gifs/tate", "tate"]
    } else {
        &["gifs", "gifs/yoko", "yoko"]
    };

    let base_prefixes = [
        "",
        "/",
        "data/",
        "release/sdCard/",
        "../",
        "../ArcadeMatrix/release/sdCard/",
        "/opt/arcadematrix/",
        &format!("{}/", home),
        &format!("{}/ArcadeMatrix_RPi/", home),
        &format!("{}/ArcadeMatrix/", home),
        &format!("{}/ArcadeMatrix/release/sdCard/", home),
        "/media/",
        "/mnt/",
    ];

    for base in &base_prefixes {
        for sub in subs {
            let candidate = if base.is_empty() {
                std::path::PathBuf::from(sub)
            } else if base.starts_with('/') {
                std::path::Path::new(base).join(sub)
            } else {
                std::path::PathBuf::from(format!("{}{}", base, sub))
            };

            if candidate.is_dir() && !roots.contains(&candidate) {
                roots.push(candidate);
            }
        }
    }

    roots
}

fn count_gifs_in_dir(path: &std::path::Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if entry.file_type().map(|ft| ft.is_symlink()).unwrap_or(false) {
                continue;
            }
            if let Ok(ft) = entry.file_type() {
                if ft.is_dir() {
                    count += count_gifs_in_dir(&entry.path());
                } else {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.to_lowercase().ends_with(".gif") && !name.starts_with("._") {
                        count += 1;
                    }
                }
            }
        }
    }
    count
}

#[get("/api/playlists")]
async fn get_playlists(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }

    let mut playlists = Vec::new();
    let mut seen_yoko = std::collections::HashSet::new();
    let mut seen_tate = std::collections::HashSet::new();

    // 1. Scan horizontal / yoko folders across candidate roots (live directories first)
    for root in find_gif_candidate_roots("yoko") {
        let mut has_live_subdirs = false;
        if let Ok(entries) = std::fs::read_dir(&root) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() {
                        if let Some(name) = entry.file_name().to_str() {
                            if !name.starts_with('.') && !name.starts_with('_') {
                                has_live_subdirs = true;
                                if seen_yoko.insert(name.to_string()) {
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
        }

        let pl_json = root.join("playlists.json");
        if !has_live_subdirs && pl_json.is_file() {
            if let Ok(content) = std::fs::read_to_string(&pl_json) {
                if let Ok(doc) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(obj) = doc.as_object() {
                        for (k, v) in obj {
                            if !k.starts_with('.')
                                && !k.starts_with('_')
                                && seen_yoko.insert(k.clone())
                            {
                                let path = v.get("path").and_then(|p| p.as_str()).unwrap_or(k);
                                playlists.push(json!({
                                    "value": if path.starts_with('/') { path.to_string() } else { format!("/gifs/{}", path) },
                                    "label": k,
                                    "orientation": "yoko"
                                }));
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Scan vertical / tate folders across candidate roots (live directories first)
    for root in find_gif_candidate_roots("tate") {
        let mut has_live_subdirs = false;
        if let Ok(entries) = std::fs::read_dir(&root) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() {
                        if let Some(name) = entry.file_name().to_str() {
                            if !name.starts_with('.') && !name.starts_with('_') {
                                has_live_subdirs = true;
                                if seen_tate.insert(name.to_string()) {
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
        }

        let pl_json = root.join("playlists.json");
        if !has_live_subdirs && pl_json.is_file() {
            if let Ok(content) = std::fs::read_to_string(&pl_json) {
                if let Ok(doc) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(obj) = doc.as_object() {
                        for (k, v) in obj {
                            if !k.starts_with('.')
                                && !k.starts_with('_')
                                && seen_tate.insert(k.clone())
                            {
                                let path = v.get("path").and_then(|p| p.as_str()).unwrap_or(k);
                                playlists.push(json!({
                                    "value": if path.starts_with('/') { path.to_string() } else { format!("/gifs_tate/{}", path) },
                                    "label": k,
                                    "orientation": "tate"
                                }));
                            }
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
    for root in find_gif_candidate_roots("yoko") {
        let mut has_live_subdirs = false;
        if let Ok(entries) = std::fs::read_dir(&root) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() {
                        if let Some(name) = entry.file_name().to_str() {
                            if !name.starts_with('.') && !name.starts_with('_') {
                                has_live_subdirs = true;
                                if !yoko_map.contains_key(name) {
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
        }

        let pl_json = root.join("playlists.json");
        if !has_live_subdirs && pl_json.is_file() {
            if let Ok(content) = std::fs::read_to_string(&pl_json) {
                if let Ok(doc) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(obj) = doc.as_object() {
                        for (k, v) in obj {
                            if !k.starts_with('.')
                                && !k.starts_with('_')
                                && !yoko_map.contains_key(k)
                            {
                                yoko_map.insert(k.clone(), v.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    let mut tate_map = serde_json::Map::new();
    for root in find_gif_candidate_roots("tate") {
        let mut has_live_subdirs = false;
        if let Ok(entries) = std::fs::read_dir(&root) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() {
                        if let Some(name) = entry.file_name().to_str() {
                            if !name.starts_with('.') && !name.starts_with('_') {
                                has_live_subdirs = true;
                                if !tate_map.contains_key(name) {
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
        }

        let pl_json = root.join("playlists.json");
        if !has_live_subdirs && pl_json.is_file() {
            if let Ok(content) = std::fs::read_to_string(&pl_json) {
                if let Ok(doc) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(obj) = doc.as_object() {
                        for (k, v) in obj {
                            if !k.starts_with('.')
                                && !k.starts_with('_')
                                && !tate_map.contains_key(k)
                            {
                                tate_map.insert(k.clone(), v.clone());
                            }
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
    let (w, h) = {
        let mat = &data.config.settings.read().matrix;
        let phys_w = (mat.width * mat.chain_length).max(32) as i32;
        let phys_h = (mat.height * mat.parallel.max(1)).max(32) as i32;
        if mat.rotation == 90 || mat.rotation == 270 {
            (phys_h, phys_w)
        } else {
            (phys_w, phys_h)
        }
    };
    // Built automatically from the single theme source of truth, filtering out
    // themes whose declarative resolution constraints are not met by the panel.
    let themes: Vec<_> = crate::core::theme::all_themes()
        .iter()
        .filter(|t| t.is_compatible(w as u32, h as u32))
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

#[get("/api/hardware")]
async fn get_hardware_compat(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    let s = data.config.settings.read();
    HttpResponse::Ok().json(&s.matrix)
}

#[get("/api/gyro")]
async fn get_gyro_compat() -> impl Responder {
    HttpResponse::Ok().json(json!({
        "present": false,
        "pitch": 0.0,
        "roll": 0.0,
        "orientation": 0
    }))
}

#[post("/api/display/orientation")]
async fn post_orientation_compat(
    req: HttpRequest,
    data: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    let mut s = data.config.settings.write();
    if let Some(v) = body
        .get("rotation")
        .or_else(|| body.get("screen_rotation"))
        .and_then(|v| v.as_u64())
    {
        s.matrix.rotation = v as u32;
    }
    if let Some(v) = body
        .get("transition_effect")
        .or_else(|| body.get("rotation_transition"))
        .and_then(|v| v.as_str())
    {
        s.matrix.transition_effect = v.to_string();
    }
    if let Some(v) = body.get("transition_duration_ms").and_then(|v| v.as_u64()) {
        s.matrix.transition_duration_ms = v as u32;
    }
    drop(s);
    if !data.config.save() {
        return HttpResponse::InternalServerError().json(json!({
            "status": "error",
            "message": "Failed to persist orientation to disk"
        }));
    }
    data.config.reset_rotation.store(true, Ordering::Relaxed);
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

#[post("/api/gyro/calibrate")]
async fn post_gyro_calibrate_compat() -> impl Responder {
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

#[derive(serde::Deserialize, Default)]
struct ShowcaseStatusQuery {
    url: Option<String>,
    enabled: Option<bool>,
    force: Option<bool>,
    hint_ip: Option<String>,
}

#[get("/api/showcase/status")]
async fn get_showcase_status(
    req: HttpRequest,
    data: web::Data<AppState>,
    query: web::Query<ShowcaseStatusQuery>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    if let Some(peer) = req.peer_addr() {
        let ip_str = match peer.ip() {
            std::net::IpAddr::V4(v4) => v4.to_string(),
            std::net::IpAddr::V6(v6) => v6
                .to_ipv4_mapped()
                .map(|v4| v4.to_string())
                .unwrap_or_default(),
        };
        if !ip_str.is_empty() {
            crate::engines::renderers::media_showcase::register_showcase_candidate_ip(&ip_str);
        }
    }
    if let Some(ref hint) = query.hint_ip {
        crate::engines::renderers::media_showcase::register_showcase_candidate_ip(hint);
    }
    let (cfg_enabled, cfg_url, mqtt_broker, data_mqtt_broker) = {
        let s = data.config.settings.read();
        (
            s.showcase.enabled,
            s.showcase.server_url.clone(),
            s.mqtt.broker.clone(),
            s.data_mqtt.broker.clone(),
        )
    };
    if !mqtt_broker.is_empty() {
        crate::engines::renderers::media_showcase::register_showcase_candidate_ip(&mqtt_broker);
    }
    if !data_mqtt_broker.is_empty() {
        crate::engines::renderers::media_showcase::register_showcase_candidate_ip(
            &data_mqtt_broker,
        );
    }
    let enabled = query.enabled.unwrap_or(cfg_enabled);
    let raw_url = query
        .url
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .unwrap_or(cfg_url.as_str())
        .trim_end_matches('/')
        .to_string();
    let force_probe = query.force.unwrap_or(false);

    if (!enabled && !force_probe) || raw_url.is_empty() {
        return HttpResponse::Ok().json(json!({
            "enabled": enabled,
            "online": false,
            "server_url": raw_url,
            "resolved_url": "",
            "version": "",
            "mqtt_port": 1883
        }));
    }

    let probe_res = web::block(move || {
        if force_probe {
            crate::engines::renderers::media_showcase::invalidate_showcase_host_cache();
        }
        let mut resolved =
            crate::engines::renderers::media_showcase::resolve_showcase_server_url(&raw_url);
        let probe_once = |target_url: &str| -> Option<(String, u64)> {
            if let Some((h, p)) =
                crate::engines::renderers::media_showcase::parse_host_and_port(target_url, 8890)
            {
                if let Some(res) =
                    crate::engines::renderers::media_showcase::probe_showcase_health(&h, p)
                {
                    return Some(res);
                }
            }
            let health_url = format!("{}/api/v1/health", target_url.trim_end_matches('/'));
            let client = crate::core::memory::stream_http_client();
            let resp = client
                .get(&health_url)
                .timeout(std::time::Duration::from_millis(1200))
                .send()
                .ok()?;
            if !resp.status().is_success() {
                return None;
            }
            let v = resp.json::<serde_json::Value>().ok()?;
            let version = v
                .get("version")
                .and_then(|s| s.as_str())
                .unwrap_or("3.0.0")
                .to_string();
            let mqtt_port = v.get("mqtt_port").and_then(|p| p.as_u64()).unwrap_or(1883);
            Some((version, mqtt_port))
        };

        if let Some((version, mqtt_port)) = probe_once(&resolved) {
            return (true, resolved, version, mqtt_port);
        }
        if !force_probe && raw_url.to_ascii_lowercase().contains(".local") {
            crate::engines::renderers::media_showcase::invalidate_showcase_host_cache();
            resolved =
                crate::engines::renderers::media_showcase::resolve_showcase_server_url(&raw_url);
            if let Some((version, mqtt_port)) = probe_once(&resolved) {
                return (true, resolved, version, mqtt_port);
            }
        }
        (false, resolved, String::new(), 1883)
    })
    .await;

    match probe_res {
        Ok((online, resolved_url, version, mqtt_port)) => HttpResponse::Ok().json(json!({
            "enabled": enabled,
            "online": online,
            "server_url": query.url.as_deref().unwrap_or(cfg_url.as_str()),
            "resolved_url": resolved_url,
            "version": version,
            "mqtt_port": mqtt_port
        })),
        Err(_) => HttpResponse::Ok().json(json!({
            "enabled": enabled,
            "online": false,
            "server_url": query.url.as_deref().unwrap_or(cfg_url.as_str()),
            "resolved_url": "",
            "version": "",
            "mqtt_port": 1883
        })),
    }
}

#[get("/api/mqtt/broker-target")]
async fn get_mqtt_broker_target(data: web::Data<AppState>) -> impl Responder {
    let cfg = Arc::clone(&data.config);
    let res =
        web::block(move || crate::engines::frontend_sync::resolve_broker_target(&cfg, false)).await;
    match res {
        Ok(target) => HttpResponse::Ok().json(json!({
            "host": target.lan_host,
            "port": target.port,
            "source": target.source,
            "mqtt_enabled": target.mqtt_enabled,
        })),
        Err(_) => {
            let fallback_ip = get_local_ip();
            HttpResponse::Ok().json(json!({
                "host": fallback_ip,
                "port": 1883,
                "source": "local",
                "mqtt_enabled": true,
            }))
        }
    }
}

#[post("/api/showcase/cache/evict")]
async fn post_showcase_cache_evict(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) {
        return e;
    }
    let raw_url = data.config.settings.read().showcase.server_url.clone();
    if raw_url.trim().is_empty() {
        return HttpResponse::BadRequest().json(json!({
            "status": "error",
            "message": "Showcase server URL is empty"
        }));
    }
    let res = web::block(move || -> Result<serde_json::Value, String> {
        let resolved =
            crate::engines::renderers::media_showcase::resolve_showcase_server_url(&raw_url);
        let evict_url = format!(
            "{}/api/v1/cache/on-demand/evict",
            resolved.trim_end_matches('/')
        );
        let client = crate::core::memory::api_http_client();
        let resp = client
            .post(&evict_url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .map_err(|e| format!("Showcase unreachable: {}", e))?;
        if !resp.status().is_success() {
            return Err(format!("Showcase returned HTTP {}", resp.status()));
        }
        resp.json::<serde_json::Value>()
            .map_err(|e| format!("Invalid response from Showcase: {}", e))
    })
    .await;

    match res {
        Ok(Ok(payload)) => HttpResponse::Ok().json(payload),
        Ok(Err(msg)) => HttpResponse::BadGateway().json(json!({
            "status": "error",
            "message": msg,
        })),
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "status": "error",
            "message": e.to_string(),
        })),
    }
}

#[derive(RustEmbed)]
#[folder = "api/www/"]
struct WebAssets;

async fn serve_static(req: actix_web::HttpRequest) -> impl actix_web::Responder {
    let mut p = req.path().trim_start_matches('/').to_string();
    if p.is_empty() {
        p = "index.html".to_string();
    }

    if p.contains('\\') || p.contains('\0') {
        return actix_web::HttpResponse::BadRequest().body("400 Bad Request");
    }

    let rel_path = Path::new(&p);
    for comp in rel_path.components() {
        match comp {
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                return actix_web::HttpResponse::BadRequest().body("400 Bad Request");
            }
            _ => {}
        }
    }

    // Check disk first for real-time web development updates, verifying canonical path containment
    let base_dir = Path::new("api/www");
    let disk_path = base_dir.join(rel_path);
    if let (Ok(canon_base), Ok(canon_file)) = (base_dir.canonicalize(), disk_path.canonicalize()) {
        if canon_file.starts_with(&canon_base) && canon_file.is_file() {
            if let Ok(bytes) = std::fs::read(&canon_file) {
                let mime = mime_guess::from_path(&canon_file).first_or_octet_stream();
                return actix_web::HttpResponse::Ok()
                    .insert_header(("Cache-Control", "no-cache, no-store, must-revalidate"))
                    .content_type(mime.as_ref())
                    .body(bytes);
            }
        }
    }

    match WebAssets::get(&p) {
        Some(content) => {
            let mime = mime_guess::from_path(&p).first_or_octet_stream();
            actix_web::HttpResponse::Ok()
                .insert_header(("Cache-Control", "no-cache, no-store, must-revalidate"))
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
            .service(get_hardware_compat)
            .service(get_gyro_compat)
            .service(post_orientation_compat)
            .service(post_gyro_calibrate_compat)
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
            .service(post_message)
            .service(post_marquee)
            .service(post_upload)
            .service(crate::api::gifs::get_gifs_library)
            .service(crate::api::gifs::post_gifs_upload)
            .service(crate::api::gifs::post_gifs_reindex)
            .service(crate::api::gifs::get_gifs_reindex_status)
            .service(crate::api::gifs::delete_gifs_reindex)
            .service(crate::api::gifs::delete_gifs_file)
            .service(crate::api::gifs::get_gifs_files)
            .service(crate::api::gifs::delete_gifs_folder)
            .service(crate::api::gifs::post_gifs_mkdir)
            .service(crate::api::gifs::post_gifs_rename)
            .service(crate::api::gifs::get_gifs_file)
            .service(post_mqtt_install)
            .service(post_mqtt_logs)
            .service(get_mqtt_broker_target)
            .service(get_showcase_status)
            .service(post_showcase_cache_evict)
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

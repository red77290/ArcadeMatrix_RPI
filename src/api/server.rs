use crate::core::config::{Config, EngineInstance, RotationEntry};
use actix_web::{delete, get, post, put, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use serde_json::json;
use std::sync::Arc;
use sysinfo::System;
use crate::api::ota::{get_version, handle_update};

pub struct AppState {
    pub config: Arc<Config>,
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
    if let Err(e) = check_auth(&req, &data.config) { return e; }
    let s = data.config.settings.read();
    HttpResponse::Ok().json(json!({
        "system": s.system,
        "matrix": s.matrix,
        "mqtt": s.mqtt,
        "wifi": s.wifi
    }))
}

#[post("/api/system")]
async fn post_system(req: HttpRequest, data: web::Data<AppState>, body: web::Json<serde_json::Value>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) { return e; }
    let mut s = data.config.settings.write();
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
    drop(s);
    data.config.save();
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

#[get("/api/instances")]
async fn get_instances(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) { return e; }
    let s = data.config.settings.read();
    HttpResponse::Ok().json(&s.instances)
}

#[post("/api/instances")]
async fn post_instances(req: HttpRequest, data: web::Data<AppState>, body: web::Json<EngineInstance>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) { return e; }
    let mut s = data.config.settings.write();
    s.instances.push(body.into_inner());
    drop(s);
    data.config.save();
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

#[delete("/api/instances/{id}")]
async fn delete_instance(req: HttpRequest, data: web::Data<AppState>, path: web::Path<String>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) { return e; }
    let mut s = data.config.settings.write();
    s.instances.retain(|i| i.instance_id != *path);
    drop(s);
    data.config.save();
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

#[get("/api/rotation")]
async fn get_rotation(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) { return e; }
    let s = data.config.settings.read();
    HttpResponse::Ok().json(&s.rotation)
}

#[post("/api/rotation")]
async fn post_rotation(req: HttpRequest, data: web::Data<AppState>, body: web::Json<Vec<RotationEntry>>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) { return e; }
    let mut s = data.config.settings.write();
    s.rotation = body.into_inner();
    drop(s);
    data.config.save();
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

#[get("/api/engines")]
async fn get_engines(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) { return e; }
    // Fetch from EngineRegistry (dummy for now)
    HttpResponse::Ok().json(json!([]))
}

#[get("/api/action/reboot")]
async fn reboot(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) { return e; }
    std::process::Command::new("sudo").arg("reboot").spawn().ok();
    HttpResponse::Ok().json(json!({"status": "rebooting"}))
}

#[get("/api/stats")]
async fn get_stats(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) { return e; }
    let mut sys = System::new_all();
    sys.refresh_all();
    HttpResponse::Ok().json(json!({
        "uptime": sysinfo::System::uptime(),
        "memory_used": sys.used_memory(),
        "memory_total": sys.total_memory()
    }))
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
            .service(get_stats)
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}

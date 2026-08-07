use actix_multipart::Multipart;
use actix_web::{get, post, HttpResponse, Responder};
use futures_util::StreamExt;
use tokio::fs;
use tokio::process::Command;
use tracing::{info, warn};

const TEMP_PATH: &str = "/tmp/arcadematrix_new";
const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];

const EM_AARCH64: u16 = 183;
const EM_ARM: u16 = 40;

#[get("/api/version")]
pub async fn get_version() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "platform": "rpi",
        "arch": env!("BUILD_TARGET"),
        "build_date": env!("BUILD_TIMESTAMP"),
    }))
}

#[post("/api/update")]
pub async fn handle_update(mut payload: Multipart) -> impl Responder {
    let mut firmware_bytes = Vec::new();

    while let Some(item) = payload.next().await {
        let mut field = match item {
            Ok(f) => f,
            Err(e) => return HttpResponse::BadRequest().json(serde_json::json!({"status": "error", "message": format!("Upload payload error: {}", e)})),
        };

        while let Some(chunk) = field.next().await {
            let data = match chunk {
                Ok(d) => d,
                Err(e) => return HttpResponse::BadRequest().json(serde_json::json!({"status": "error", "message": format!("Chunk read error: {}", e)})),
            };
            firmware_bytes.extend_from_slice(&data);
        }
    }

    if firmware_bytes.len() < 20 {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "status": "error",
            "message": "Uploaded file too small to be a valid firmware binary"
        }));
    }

    if firmware_bytes[..4] != ELF_MAGIC {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "status": "error",
            "message": "Invalid firmware: missing ELF magic header"
        }));
    }

    let e_machine = u16::from_le_bytes([firmware_bytes[18], firmware_bytes[19]]);
    let current_target = env!("BUILD_TARGET");
    let valid_arch = match e_machine {
        EM_AARCH64 => current_target.contains("aarch64"),
        EM_ARM => current_target.contains("arm"),
        _ => false,
    };

    if !valid_arch && !cfg!(debug_assertions) {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "status": "error",
            "message": format!("Architecture mismatch: firmware is for e_machine {}, expected target {}", e_machine, current_target)
        }));
    }

    if let Err(e) = fs::write(TEMP_PATH, &firmware_bytes).await {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to write temporary binary: {}", e)
        }));
    }

    let binary_path = std::env::current_exe()
        .unwrap_or_else(|_| std::path::PathBuf::from("/usr/local/bin/arcadematrix"));
    let backup_path = binary_path.with_extension("bak");

    if binary_path.exists() {
        if let Err(e) = fs::copy(&binary_path, &backup_path).await {
            warn!("Could not backup current binary: {}", e);
        }
    }

    if let Err(_e) = fs::rename(TEMP_PATH, &binary_path).await {
        if let Err(e2) = fs::copy(TEMP_PATH, &binary_path).await {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "status": "error",
                "message": format!("Failed to install firmware binary: {}", e2)
            }));
        }
        let _ = fs::remove_file(TEMP_PATH).await;
    }

    let _ = Command::new("chmod")
        .args(["+x"])
        .arg(&binary_path)
        .status()
        .await;

    info!("Firmware update successful. Scheduling systemd service restart...");

    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let _ = Command::new("sudo")
            .args(["systemctl", "restart", "arcadematrix"])
            .status()
            .await;
    });

    HttpResponse::Ok().json(serde_json::json!({
        "status": "success",
        "message": "Firmware updated successfully. Service restarting in 1 second...",
        "old_version": env!("CARGO_PKG_VERSION")
    }))
}

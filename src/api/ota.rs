use actix_multipart::Multipart;
use actix_web::{get, post, HttpResponse, Responder};
use futures_util::StreamExt;
use tokio::fs;
use tokio::process::Command;
use tracing::info;

const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];

const EM_AARCH64: u16 = 183;
const EM_ARM: u16 = 40;

#[get("/api/version")]
pub async fn get_version() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "version": crate::core::build_info::VERSION,
        "platform": "rpi",
        "arch": crate::core::build_info::ARCH,
        "build_date": crate::core::build_info::BUILD_TIMESTAMP,
        "git_commit": crate::core::build_info::GIT_COMMIT,
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

    if let Err(msg) = validate_firmware(&firmware_bytes, crate::core::build_info::ARCH) {
        // If debug assertions are on, we bypass architecture mismatch for local testing
        if msg.starts_with("Architecture mismatch") && cfg!(debug_assertions) {
            // Ignore for debug builds
        } else {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "status": "error",
                "message": msg
            }));
        }
    }

    let binary_path = std::env::current_exe()
        .unwrap_or_else(|_| std::path::PathBuf::from("/usr/local/bin/arcadematrix"));
    let temp_path = std::path::PathBuf::from("/tmp/arcadematrix_update");

    if let Err(e) = fs::write(&temp_path, &firmware_bytes).await {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to write temporary binary: {}", e)
        }));
    }

    let script_content = format!(
        "#!/bin/bash\n\
        sleep 2\n\
        systemctl stop arcadematrix\n\
        rm -f \"{}\"\n\
        mv /tmp/arcadematrix_update \"{}\"\n\
        chmod +x \"{}\"\n\
        systemctl start arcadematrix\n",
        binary_path.display(),
        binary_path.display(),
        binary_path.display()
    );

    let script_path = std::path::PathBuf::from("/tmp/update_am.sh");
    if let Err(e) = fs::write(&script_path, script_content).await {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to write updater script: {}", e)
        }));
    }

    let _ = Command::new("chmod")
        .args(["+x"])
        .arg(&script_path)
        .status()
        .await;

    info!("Firmware update staged. Executing background updater script...");

    tokio::spawn(async move {
        // Use systemd-run to escape the service cgroup, otherwise systemctl stop will kill this script!
        let _ = Command::new("sudo")
            .args([
                "systemd-run",
                "--unit=arcadematrix-updater",
                "/tmp/update_am.sh",
            ])
            .spawn();
    });

    HttpResponse::Ok().json(serde_json::json!({
        "status": "success",
        "message": "Firmware updated successfully. Service restarting in 1 second...",
        "old_version": crate::core::build_info::VERSION
    }))
}

pub fn validate_firmware(firmware_bytes: &[u8], current_target: &str) -> Result<(), String> {
    if firmware_bytes.len() < 20 {
        return Err("Uploaded file too small to be a valid firmware binary".to_string());
    }

    if firmware_bytes[..4] != ELF_MAGIC {
        return Err("Invalid firmware: missing ELF magic header".to_string());
    }

    let e_machine = u16::from_le_bytes([firmware_bytes[18], firmware_bytes[19]]);
    let valid_arch = match e_machine {
        EM_AARCH64 => current_target.contains("aarch64"),
        EM_ARM => current_target.contains("arm"),
        _ => false,
    };

    if !valid_arch {
        return Err(format!(
            "Architecture mismatch: firmware is for e_machine {}, expected target {}",
            e_machine, current_target
        ));
    }

    Ok(())
}

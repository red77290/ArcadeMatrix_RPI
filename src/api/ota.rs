use actix_multipart::Multipart;
use actix_web::{get, post, web, HttpResponse, Responder};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::process::Command;
use tracing::info;

const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];

const EM_AARCH64: u16 = 183;
const EM_ARM: u16 = 40;

const GITHUB_RELEASES_API: &str = "https://api.github.com/repos/red77290/ArcadeMatrix_RPI/releases";

#[derive(Debug, PartialEq, Eq)]
pub struct ParsedVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub prerelease: Option<(String, u32)>, // (tag, number), e.g. ("beta", 2)
}

impl ParsedVersion {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches(|c| c == 'v' || c == 'V');
        let parts: Vec<&str> = s.splitn(2, '-').collect();
        let main_part = parts[0];
        let pre_part = if parts.len() > 1 {
            Some(parts[1])
        } else {
            None
        };

        let num_parts: Vec<&str> = main_part.split('.').collect();
        if num_parts.is_empty() {
            return None;
        }
        let major = num_parts.get(0)?.parse::<u32>().ok()?;
        let minor = num_parts
            .get(1)
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(0);
        let patch = num_parts
            .get(2)
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(0);

        let prerelease = pre_part.map(|p| {
            let p_lower = p.to_lowercase();
            let p_num = p_lower
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u32>()
                .unwrap_or(0);
            let p_name = p_lower
                .chars()
                .filter(|c| c.is_alphabetic())
                .collect::<String>();
            (p_name, p_num)
        });

        Some(Self {
            major,
            minor,
            patch,
            prerelease,
        })
    }

    /// Returns true ONLY if `self` is strictly lower than `other` (i.e. self < other).
    pub fn is_lower_than(&self, other: &Self) -> bool {
        if self.major != other.major {
            return self.major < other.major;
        }
        if self.minor != other.minor {
            return self.minor < other.minor;
        }
        if self.patch != other.patch {
            return self.patch < other.patch;
        }

        match (&self.prerelease, &other.prerelease) {
            // Both are stable releases -> equal, not lower
            (None, None) => false,
            // Self is pre-release, other is stable release -> self is lower (e.g. 3.0.0-beta.3 < 3.0.0)
            (Some(_), None) => true,
            // Self is stable release, other is pre-release -> self is higher (e.g. 3.0.0 > 3.0.0-beta.3)
            (None, Some(_)) => false,
            // Both are pre-releases -> compare identifier and number (e.g. beta.2 < beta.3)
            (Some((name1, num1)), Some((name2, num2))) => {
                if name1 != name2 {
                    name1 < name2
                } else {
                    num1 < num2
                }
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub name: Option<String>,
    pub prerelease: bool,
    pub published_at: Option<String>,
    pub body: Option<String>,
    pub assets: Vec<GitHubAsset>,
}

#[derive(Debug, Serialize)]
pub struct CheckUpdateResponse {
    pub current_version: &'static str,
    pub current_arch: &'static str,
    pub latest_version: String,
    pub update_available: bool,
    pub release_name: String,
    pub release_notes: String,
    pub published_at: String,
    pub download_url: Option<String>,
    pub asset_name: Option<String>,
    pub asset_size: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct AutoUpdateRequest {
    pub download_url: Option<String>,
}

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

#[get("/api/ota/check")]
pub async fn check_update() -> impl Responder {
    let client = reqwest::Client::builder()
        .user_agent("ArcadeMatrix-RPi-OTA")
        .timeout(std::time::Duration::from_secs(10))
        .build();

    let client = match client {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "status": "error",
                "message": format!("HTTP client init failed: {}", e)
            }))
        }
    };

    let res = match client.get(GITHUB_RELEASES_API).send().await {
        Ok(r) => r,
        Err(e) => {
            return HttpResponse::BadGateway().json(serde_json::json!({
                "status": "error",
                "message": format!("Failed to reach GitHub API: {}", e)
            }))
        }
    };

    if !res.status().is_success() {
        return HttpResponse::BadGateway().json(serde_json::json!({
            "status": "error",
            "message": format!("GitHub API returned HTTP {}", res.status())
        }));
    }

    let releases: Vec<GitHubRelease> = match res.json().await {
        Ok(rel) => rel,
        Err(e) => {
            return HttpResponse::BadGateway().json(serde_json::json!({
                "status": "error",
                "message": format!("Failed to parse GitHub releases: {}", e)
            }))
        }
    };

    if releases.is_empty() {
        return HttpResponse::Ok().json(serde_json::json!({
            "status": "error",
            "message": "No releases found on GitHub repository."
        }));
    }

    // Pick the most recent release
    let latest_rel = &releases[0];
    let latest_version = latest_rel
        .tag_name
        .trim_start_matches(|c| c == 'v' || c == 'V')
        .to_string();
    let current_version = crate::core::build_info::VERSION;
    let current_arch = crate::core::build_info::ARCH;

    // Only report update_available if current version is strictly lower than latest release
    let curr_parsed = ParsedVersion::parse(current_version);
    let lat_parsed = ParsedVersion::parse(&latest_version);

    let update_available = match (&curr_parsed, &lat_parsed) {
        (Some(curr), Some(lat)) => curr.is_lower_than(lat),
        _ => false,
    };

    // Find matching asset for arch (aarch64 vs arm/armv7)
    let is_64bit = current_arch.contains("aarch64") || current_arch.contains("arm64");
    let mut matching_asset: Option<&GitHubAsset> = None;
    for asset in &latest_rel.assets {
        let name_lower = asset.name.to_lowercase();
        // Ignore OS SD card images (.img.xz) or zip archives
        if name_lower.ends_with(".img.xz")
            || name_lower.ends_with(".zip")
            || name_lower.ends_with(".tar.gz")
        {
            continue;
        }
        if is_64bit && (name_lower.contains("aarch64") || name_lower.contains("arm64")) {
            matching_asset = Some(asset);
            break;
        } else if !is_64bit
            && (name_lower.contains("armv7")
                || name_lower.contains("armhf")
                || name_lower.contains("arm"))
        {
            matching_asset = Some(asset);
            break;
        }
    }

    HttpResponse::Ok().json(CheckUpdateResponse {
        current_version,
        current_arch,
        latest_version,
        update_available,
        release_name: latest_rel
            .name
            .clone()
            .unwrap_or_else(|| latest_rel.tag_name.clone()),
        release_notes: latest_rel.body.clone().unwrap_or_default(),
        published_at: latest_rel.published_at.clone().unwrap_or_default(),
        download_url: matching_asset.map(|a| a.browser_download_url.clone()),
        asset_name: matching_asset.map(|a| a.name.clone()),
        asset_size: matching_asset.map(|a| a.size),
    })
}

#[post("/api/ota/auto-update")]
pub async fn auto_update(body: Option<web::Json<AutoUpdateRequest>>) -> impl Responder {
    let client = reqwest::Client::builder()
        .user_agent("ArcadeMatrix-RPi-OTA")
        .timeout(std::time::Duration::from_secs(120))
        .build();

    let client = match client {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "status": "error",
                "message": format!("HTTP client init failed: {}", e)
            }))
        }
    };

    let download_url = if let Some(b) = body.and_then(|b| b.download_url.clone()) {
        b
    } else {
        let res = match client.get(GITHUB_RELEASES_API).send().await {
            Ok(r) => r,
            Err(e) => {
                return HttpResponse::BadGateway().json(serde_json::json!({
                    "status": "error",
                    "message": format!("Failed to reach GitHub API: {}", e)
                }))
            }
        };
        let releases: Vec<GitHubRelease> = match res.json().await {
            Ok(rel) => rel,
            Err(e) => {
                return HttpResponse::BadGateway().json(serde_json::json!({
                    "status": "error",
                    "message": format!("Failed to parse releases: {}", e)
                }))
            }
        };
        if releases.is_empty() {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "status": "error",
                "message": "No releases found on GitHub."
            }));
        }

        let latest_rel = &releases[0];
        let latest_version = latest_rel
            .tag_name
            .trim_start_matches(|c| c == 'v' || c == 'V')
            .to_string();
        let current_version = crate::core::build_info::VERSION;

        let curr_parsed = ParsedVersion::parse(current_version);
        let lat_parsed = ParsedVersion::parse(&latest_version);

        if let (Some(curr), Some(lat)) = (curr_parsed, lat_parsed) {
            if !curr.is_lower_than(&lat) {
                return HttpResponse::BadRequest().json(serde_json::json!({
                    "status": "error",
                    "message": format!("Current version (v{}) is already equal or newer than latest release (v{}). Update cancelled.", current_version, latest_version)
                }));
            }
        }

        let current_arch = crate::core::build_info::ARCH;
        let is_64bit = current_arch.contains("aarch64") || current_arch.contains("arm64");
        let mut url: Option<String> = None;
        for asset in &latest_rel.assets {
            let name_lower = asset.name.to_lowercase();
            if name_lower.ends_with(".img.xz")
                || name_lower.ends_with(".zip")
                || name_lower.ends_with(".tar.gz")
            {
                continue;
            }
            if is_64bit && (name_lower.contains("aarch64") || name_lower.contains("arm64")) {
                url = Some(asset.browser_download_url.clone());
                break;
            } else if !is_64bit
                && (name_lower.contains("armv7")
                    || name_lower.contains("armhf")
                    || name_lower.contains("arm"))
            {
                url = Some(asset.browser_download_url.clone());
                break;
            }
        }
        match url {
            Some(u) => u,
            None => {
                return HttpResponse::BadRequest().json(serde_json::json!({
                    "status": "error",
                    "message": format!("No matching binary asset found for arch {}", current_arch)
                }))
            }
        }
    };

    info!("Downloading binary from: {}", download_url);
    let resp = match client.get(&download_url).send().await {
        Ok(r) => r,
        Err(e) => {
            return HttpResponse::BadGateway().json(serde_json::json!({
                "status": "error",
                "message": format!("Download request failed: {}", e)
            }))
        }
    };

    if !resp.status().is_success() {
        return HttpResponse::BadGateway().json(serde_json::json!({
            "status": "error",
            "message": format!("Download returned HTTP {}", resp.status())
        }));
    }

    let firmware_bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "status": "error",
                "message": format!("Failed reading downloaded bytes: {}", e)
            }))
        }
    };

    if let Err(msg) = validate_firmware(&firmware_bytes, crate::core::build_info::ARCH) {
        if !(msg.starts_with("Architecture mismatch") && cfg!(debug_assertions)) {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "status": "error",
                "message": msg
            }));
        }
    }

    let binary_path = std::env::current_exe()
        .and_then(|p| p.canonicalize().or(Ok(p)))
        .unwrap_or_else(|_| std::path::PathBuf::from("/usr/local/bin/arcadematrix"));
    let temp_path = std::path::PathBuf::from("/tmp/arcadematrix_update");

    if let Err(e) = fs::write(&temp_path, &firmware_bytes).await {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to write temporary binary: {}", e)
        }));
    }

    let _ = Command::new("chmod")
        .args(["+x"])
        .arg(&temp_path)
        .status()
        .await;

    let target_bin = binary_path.display().to_string();
    let script_content = format!(
        r#"#!/bin/bash
LOG_FILE="/tmp/arcadematrix_ota.log"
exec > "$LOG_FILE" 2>&1
echo "[$(date)] --- ArcadeMatrix Auto-OTA Starting ---"
echo "Target binary: {target_bin}"

sleep 1.5

if command -v systemctl &>/dev/null && systemctl is-active --quiet arcadematrix.service; then
    echo "Stopping arcadematrix.service..."
    systemctl stop arcadematrix.service
elif command -v systemctl &>/dev/null && systemctl is-active --quiet arcadematrix; then
    echo "Stopping arcadematrix unit..."
    systemctl stop arcadematrix
fi

CURRENT_PID=$$
for pid in $(pgrep -f "{target_bin}" 2>/dev/null); do
    if [ "$pid" != "$CURRENT_PID" ]; then
        echo "Terminating process $pid..."
        kill -15 "$pid" 2>/dev/null || true
    fi
done
sleep 1

for pid in $(pgrep -f "{target_bin}" 2>/dev/null); do
    if [ "$pid" != "$CURRENT_PID" ]; then
        echo "Force killing process $pid..."
        kill -9 "$pid" 2>/dev/null || true
    fi
done

if [ -f "/tmp/arcadematrix_update" ]; then
    echo "Copying new binary over {target_bin}..."
    cp -f /tmp/arcadematrix_update "{target_bin}"
    chmod +x "{target_bin}"
    rm -f /tmp/arcadematrix_update
    echo "Binary replaced successfully."
else
    echo "ERROR: /tmp/arcadematrix_update not found!"
    exit 1
fi

if command -v systemctl &>/dev/null && systemctl list-unit-files | grep -q "^arcadematrix.service"; then
    echo "Starting arcadematrix.service..."
    systemctl start arcadematrix.service
elif [ -x "{target_bin}" ]; then
    echo "Relaunching daemon in background..."
    nohup "{target_bin}" >/dev/null 2>&1 &
fi

echo "[$(date)] --- Auto-OTA Finished Successfully ---"
"#
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

    let unit_id = format!(
        "arcadematrix-ota-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );

    info!("Firmware auto-update staged. Executing background updater script...");

    tokio::spawn(async move {
        let mut ran = false;
        if let Ok(mut child) = Command::new("systemd-run")
            .args([
                &format!("--unit={}", unit_id),
                "--collect",
                "--remain-after-exit=no",
                "/tmp/update_am.sh",
            ])
            .spawn()
        {
            if let Ok(status) = child.wait().await {
                if status.success() {
                    ran = true;
                }
            }
        }

        if !ran {
            let _ = Command::new("nohup").args(["/tmp/update_am.sh"]).spawn();
        }
    });

    HttpResponse::Ok().json(serde_json::json!({
        "status": "success",
        "message": "Firmware downloaded and verified. Service restarting in 2 seconds...",
        "old_version": crate::core::build_info::VERSION
    }))
}

#[post("/api/update")]
pub async fn handle_update(mut payload: Multipart) -> impl Responder {
    let mut firmware_bytes = Vec::new();

    while let Some(item) = payload.next().await {
        let mut field = match item {
            Ok(f) => f,
            Err(e) => {
                return HttpResponse::BadRequest().json(serde_json::json!({
                    "status": "error",
                    "message": format!("Upload payload error: {}", e)
                }))
            }
        };

        while let Some(chunk) = field.next().await {
            let data = match chunk {
                Ok(d) => d,
                Err(e) => {
                    return HttpResponse::BadRequest().json(serde_json::json!({
                        "status": "error",
                        "message": format!("Chunk read error: {}", e)
                    }))
                }
            };
            firmware_bytes.extend_from_slice(&data);
        }
    }

    if let Err(msg) = validate_firmware(&firmware_bytes, crate::core::build_info::ARCH) {
        if !(msg.starts_with("Architecture mismatch") && cfg!(debug_assertions)) {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "status": "error",
                "message": msg
            }));
        }
    }

    let binary_path = std::env::current_exe()
        .and_then(|p| p.canonicalize().or(Ok(p)))
        .unwrap_or_else(|_| std::path::PathBuf::from("/usr/local/bin/arcadematrix"));
    let temp_path = std::path::PathBuf::from("/tmp/arcadematrix_update");

    if let Err(e) = fs::write(&temp_path, &firmware_bytes).await {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to write temporary binary: {}", e)
        }));
    }

    let _ = Command::new("chmod")
        .args(["+x"])
        .arg(&temp_path)
        .status()
        .await;

    let target_bin = binary_path.display().to_string();
    let script_content = format!(
        r#"#!/bin/bash
LOG_FILE="/tmp/arcadematrix_ota.log"
exec > "$LOG_FILE" 2>&1
echo "[$(date)] --- ArcadeMatrix OTA Update Starting ---"
echo "Target binary: {target_bin}"

# Wait 1.5s for Actix to flush HTTP 200 response
sleep 1.5

# Step 1: Stop systemd service if running
if command -v systemctl &>/dev/null && systemctl is-active --quiet arcadematrix.service; then
    echo "Stopping arcadematrix.service..."
    systemctl stop arcadematrix.service
elif command -v systemctl &>/dev/null && systemctl is-active --quiet arcadematrix; then
    echo "Stopping arcadematrix unit..."
    systemctl stop arcadematrix
fi

# Step 2: Ensure any lingering process holding the binary is stopped
CURRENT_PID=$$
for pid in $(pgrep -f "{target_bin}" 2>/dev/null); do
    if [ "$pid" != "$CURRENT_PID" ]; then
        echo "Terminating process $pid..."
        kill -15 "$pid" 2>/dev/null || true
    fi
done
sleep 1

for pid in $(pgrep -f "{target_bin}" 2>/dev/null); do
    if [ "$pid" != "$CURRENT_PID" ]; then
        echo "Force killing process $pid..."
        kill -9 "$pid" 2>/dev/null || true
    fi
done

# Step 3: Replace binary
if [ -f "/tmp/arcadematrix_update" ]; then
    echo "Copying new binary over {target_bin}..."
    cp -f /tmp/arcadematrix_update "{target_bin}"
    chmod +x "{target_bin}"
    rm -f /tmp/arcadematrix_update
    echo "Binary replaced successfully."
else
    echo "ERROR: /tmp/arcadematrix_update not found!"
    exit 1
fi

# Step 4: Restart systemd service or relaunch daemon
if command -v systemctl &>/dev/null && systemctl list-unit-files | grep -q "^arcadematrix.service"; then
    echo "Starting arcadematrix.service..."
    systemctl start arcadematrix.service
elif [ -x "{target_bin}" ]; then
    echo "Relaunching daemon in background..."
    nohup "{target_bin}" >/dev/null 2>&1 &
fi

echo "[$(date)] --- OTA Update Finished Successfully ---"
"#
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

    let unit_id = format!(
        "arcadematrix-ota-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );

    info!("Firmware update staged. Executing background updater script...");

    tokio::spawn(async move {
        let mut ran = false;
        if let Ok(mut child) = Command::new("systemd-run")
            .args([
                &format!("--unit={}", unit_id),
                "--collect",
                "--remain-after-exit=no",
                "/tmp/update_am.sh",
            ])
            .spawn()
        {
            if let Ok(status) = child.wait().await {
                if status.success() {
                    ran = true;
                }
            }
        }

        if !ran {
            let _ = Command::new("nohup").args(["/tmp/update_am.sh"]).spawn();
        }
    });

    HttpResponse::Ok().json(serde_json::json!({
        "status": "success",
        "message": "Firmware updated successfully. Service restarting in 2 seconds...",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_parsing() {
        let v1 = ParsedVersion::parse("v3.0.0").unwrap();
        assert_eq!(v1.major, 3);
        assert_eq!(v1.minor, 0);
        assert_eq!(v1.patch, 0);
        assert_eq!(v1.prerelease, None);

        let v2 = ParsedVersion::parse("3.0.0-beta.2").unwrap();
        assert_eq!(v2.major, 3);
        assert_eq!(v2.minor, 0);
        assert_eq!(v2.patch, 0);
        assert_eq!(v2.prerelease, Some(("beta".to_string(), 2)));
    }

    #[test]
    fn test_semver_comparison_lower() {
        let v_beta2 = ParsedVersion::parse("3.0.0-beta.2").unwrap();
        let v_beta3 = ParsedVersion::parse("3.0.0-beta.3").unwrap();
        let v_stable = ParsedVersion::parse("3.0.0").unwrap();
        let v_future = ParsedVersion::parse("3.1.0-dev").unwrap();

        // 3.0.0-beta.2 < 3.0.0-beta.3 -> true
        assert!(v_beta2.is_lower_than(&v_beta3));
        // 3.0.0-beta.3 < 3.0.0-beta.2 -> false
        assert!(!v_beta3.is_lower_than(&v_beta2));

        // 3.0.0-beta.3 < 3.0.0 (stable) -> true
        assert!(v_beta3.is_lower_than(&v_stable));
        // 3.0.0 (stable) < 3.0.0-beta.3 -> false
        assert!(!v_stable.is_lower_than(&v_beta3));

        // 3.0.0 == 3.0.0 -> false
        assert!(!v_stable.is_lower_than(&v_stable));

        // Developer future version: 3.1.0-dev > 3.0.0 -> false (don't downgrade!)
        assert!(!v_future.is_lower_than(&v_stable));
    }
}

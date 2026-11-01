use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

fn disable_wlan0_power_save() {
    for bin in ["/usr/sbin/iw", "/sbin/iw", "iw"] {
        if let Ok(st) = Command::new(bin)
            .args(["dev", "wlan0", "set", "power_save", "off"])
            .status()
        {
            if st.success() {
                info!("[Wi-Fi] Disabled brcmfmac power_save on wlan0 for low-latency streaming.");
                return;
            }
        }
    }
}

fn read_rpi_throttled_status() -> Option<String> {
    for bin in ["/usr/bin/vcgencmd", "/bin/vcgencmd", "vcgencmd"] {
        if let Ok(out) = Command::new(bin).arg("get_throttled").output() {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
    }
    None
}

fn attempt_wlan0_soft_recovery() {
    warn!("[Wi-Fi] Attempting non-blocking wlan0 link recovery...");
    for bin in ["/sbin/ip", "/usr/sbin/ip", "ip"] {
        if Command::new(bin)
            .args(["link", "set", "wlan0", "up"])
            .status()
            .map_or(false, |s| s.success())
        {
            break;
        }
    }
    for bin in ["/sbin/wpa_cli", "/usr/sbin/wpa_cli", "wpa_cli"] {
        if Command::new(bin)
            .args(["-i", "wlan0", "reconnect"])
            .status()
            .map_or(false, |s| s.success())
        {
            return;
        }
    }
    for bin in ["/usr/bin/nmcli", "/bin/nmcli", "nmcli"] {
        if Command::new(bin)
            .args(["device", "connect", "wlan0"])
            .status()
            .map_or(false, |s| s.success())
        {
            return;
        }
    }
}

pub fn start_wifi_monitor() {
    std::thread::Builder::new()
        .name("wifi-watchdog".to_string())
        .spawn(|| {
            let operstate_path = Path::new("/sys/class/net/wlan0/operstate");
            if !operstate_path.exists() {
                return;
            }

            disable_wlan0_power_save();
            let mut was_down = false;
            let mut last_recovery_at: Option<Instant> = None;

            loop {
                if !operstate_path.exists() {
                    std::thread::sleep(Duration::from_secs(30));
                    continue;
                }

                let state = std::fs::read_to_string(operstate_path)
                    .unwrap_or_else(|_| "unknown".to_string())
                    .trim()
                    .to_string();

                if state != "up" {
                    let throttled =
                        read_rpi_throttled_status().unwrap_or_else(|| "unavailable".to_string());
                    error!(
                        "Wi-Fi ALARM: wlan0 is DOWN! State: '{}' (throttled: {})",
                        state, throttled
                    );

                    if !was_down {
                        if let Ok(dmesg) = Command::new("dmesg").output() {
                            let logs = String::from_utf8_lossy(&dmesg.stdout);
                            let relevant: Vec<&str> = logs
                                .lines()
                                .filter(|l| {
                                    let low = l.to_ascii_lowercase();
                                    low.contains("brcmfmac") || low.contains("wlan0")
                                })
                                .collect();

                            if !relevant.is_empty() {
                                let tail = relevant.into_iter().rev().take(5).collect::<Vec<_>>();
                                error!("--- Kernel Wi-Fi Crash Logs (last 5) ---");
                                for log in tail.iter().rev() {
                                    error!("{}", log);
                                }
                            }
                        }
                    }
                    was_down = true;

                    let should_recover =
                        last_recovery_at.map_or(true, |ts| ts.elapsed() >= Duration::from_secs(30));
                    if should_recover {
                        last_recovery_at = Some(Instant::now());
                        attempt_wlan0_soft_recovery();
                    }
                } else if was_down {
                    was_down = false;
                    info!("[Wi-Fi] wlan0 recovered to 'up'; re-applying power_save off.");
                    disable_wlan0_power_save();
                }

                std::thread::sleep(Duration::from_secs(10));
            }
        })
        .ok();
}

use std::process::Command;
use std::time::Duration;
use tracing::error;

pub fn start_wifi_monitor() {
    std::thread::spawn(|| {
        loop {
            // Check if wlan0 interface state is up
            let state = std::fs::read_to_string("/sys/class/net/wlan0/operstate")
                .unwrap_or_else(|_| "unknown".to_string())
                .trim()
                .to_string();

            if state != "up" {
                error!("Wi-Fi ALARM: wlan0 is DOWN! State: '{}'", state);

                // Fetch recent brcmfmac dmesg logs for debugging kernel crashes
                if let Ok(dmesg) = Command::new("dmesg").output() {
                    let logs = String::from_utf8_lossy(&dmesg.stdout);
                    let relevant: Vec<&str> = logs
                        .lines()
                        .filter(|l| {
                            l.to_lowercase().contains("brcmfmac")
                                || l.to_lowercase().contains("wlan0")
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

            std::thread::sleep(Duration::from_secs(10));
        }
    });
}

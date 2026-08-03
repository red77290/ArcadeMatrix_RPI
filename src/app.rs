use crate::api::run_server;
use crate::core::config::Config;
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
use crate::core::matrix::HardwareMatrix;
use crate::core::matrix::{MatrixBackend, MockMatrix};
use crate::core::rotation::RotationState;

use crate::engines::clock::ClockEngine;
use crate::engines::date::DateEngine;
use crate::engines::fighter::FighterEngine;
use crate::engines::gif::GifEngine;
use crate::engines::marquee::MarqueeEngine;
use crate::engines::message::{MessageEngine, MessagePayload};
use crate::engines::weather::WeatherEngine;

use std::net::UdpSocket;

use std::sync::atomic::Ordering;
use std::sync::Arc;
use tracing::info;

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

pub struct ArcadeMatrixApp {
    pub config: Arc<Config>,
}

impl ArcadeMatrixApp {
    pub fn new() -> Self {
        let config = Arc::new(Config::new("conf.ini"));
        Self { config }
    }

    pub async fn run(&self) -> std::io::Result<()> {
        info!("Starting ArcadeMatrix RPi v{}", env!("CARGO_PKG_VERSION"));

        #[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
        {
            // The rgb-led-matrix C++ library automatically drops privileges from root to the 'daemon' user.
            // Since the API requires running commands like nmcli, reboot, and shutdown, we must dynamically
            // allow the 'daemon' user to run these commands via sudo without a password.
            let sudoers_file = "/etc/sudoers.d/010_arcadematrix_daemon";
            let sudoers_content =
                "daemon ALL=(ALL) NOPASSWD: /usr/bin/nmcli, /sbin/shutdown, /sbin/reboot\n";
            if std::fs::read_to_string(sudoers_file).unwrap_or_default() != sudoers_content {
                info!("Granting sudo privileges to daemon user for system commands...");
                std::fs::write(sudoers_file, sudoers_content).ok();
                std::process::Command::new("chmod")
                    .args(["0440", sudoers_file])
                    .spawn()
                    .ok();
            }

            // Manage internal Wi-Fi state via config.txt
            let boot_config = if std::path::Path::new("/boot/firmware/config.txt").exists() {
                "/boot/firmware/config.txt"
            } else {
                "/boot/config.txt"
            };

            if let Ok(content) = std::fs::read_to_string(boot_config) {
                let disable_wifi_cmd = "dtoverlay=disable-wifi";
                let s = self.config.settings.read().clone();
                let should_be_disabled = s.wifi_disable_internal;
                let is_disabled = content.contains(disable_wifi_cmd);

                if should_be_disabled && !is_disabled {
                    info!(
                        "Disabling internal Wi-Fi in {} (Reboot required)",
                        boot_config
                    );
                    let new_content = format!("{}\n{}\n", content.trim(), disable_wifi_cmd);
                    if let Err(e) = std::fs::write(boot_config, new_content) {
                        tracing::error!("Failed to write to {}: {}", boot_config, e);
                    }
                } else if !should_be_disabled && is_disabled {
                    info!(
                        "Enabling internal Wi-Fi in {} (Reboot required)",
                        boot_config
                    );
                    let new_content = content
                        .lines()
                        .filter(|line| !line.contains(disable_wifi_cmd))
                        .collect::<Vec<_>>()
                        .join("\n");
                    if let Err(e) = std::fs::write(boot_config, new_content) {
                        tracing::error!("Failed to write to {}: {}", boot_config, e);
                    }
                }
            }
        }

        // 0. Setup Wi-Fi if needed (like the Python version did)
        {
            let s = self.config.settings.read().clone();
            if !s.wifi_ssid.is_empty() && !s.wifi_configured {
                info!("Attempting to configure Wi-Fi for SSID: {}", s.wifi_ssid);

                // Set country code and unblock wifi
                std::process::Command::new("sudo")
                    .args(["raspi-config", "nonint", "do_wifi_country", "FR"])
                    .output()
                    .ok();
                std::process::Command::new("sudo")
                    .args(["rfkill", "unblock", "wifi"])
                    .output()
                    .ok();
                std::thread::sleep(std::time::Duration::from_secs(2));

                let safe_ssid = s.wifi_ssid.replace(" ", "_").replace("/", "_");
                let nm_content = format!(
                    "[connection]\nid={safe_ssid}\ntype=wifi\nautoconnect=true\n\n\
                    [wifi]\nmode=infrastructure\nssid={ssid}\n\n\
                    [wifi-security]\nkey-mgmt=wpa-psk\npsk={pass}\n\n\
                    [ipv4]\nmethod=auto\n\n\
                    [ipv6]\naddr-gen-mode=default\nmethod=auto\n",
                    safe_ssid = safe_ssid,
                    ssid = s.wifi_ssid,
                    pass = s.wifi_pass
                );

                let profile_path = format!(
                    "/etc/NetworkManager/system-connections/{}.nmconnection",
                    safe_ssid
                );
                if let Err(e) = std::fs::write(&profile_path, nm_content) {
                    tracing::error!("Failed to write NetworkManager profile: {}", e);
                } else {
                    std::process::Command::new("sudo")
                        .args(["chmod", "600", &profile_path])
                        .output()
                        .ok();
                    std::process::Command::new("sudo")
                        .args(["nmcli", "connection", "reload"])
                        .output()
                        .ok();
                    let output = std::process::Command::new("sudo")
                        .args(["nmcli", "connection", "up", &safe_ssid])
                        .output()
                        .ok();

                    if let Some(out) = output {
                        if out.status.success() {
                            info!("Connected to Wi-Fi successfully!");
                            let mut ws = self.config.settings.write();
                            ws.wifi_configured = true;
                            drop(ws);
                            self.config.save();
                        } else {
                            let stderr = String::from_utf8_lossy(&out.stderr);
                            tracing::error!("Failed to bring up Wi-Fi connection: {}", stderr);
                        }
                    }
                }
            }
        }

        let local_ip = get_local_ip();
        info!("ArcadeMatrix RPi IP Address: {}", local_ip);

        let config_clone = Arc::clone(&self.config);
        std::thread::spawn(move || {
            let sys = actix_web::rt::System::new();
            if let Err(e) = sys.block_on(run_server(config_clone, 8080)) {
                tracing::error!("API Server crashed: {}", e);
            }
        });

        // Start MQTT client background worker
        crate::engines::network::start_mqtt_client(Arc::clone(&self.config));

        // Initialize hardware matrix on Linux target or MockMatrix fallback
        let mut matrix: Box<dyn MatrixBackend> = {
            #[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
            {
                let cfg = self.config.settings.read();
                match HardwareMatrix::new(
                    cfg.matrix_rows,
                    cfg.matrix_cols,
                    cfg.matrix_chain,
                    cfg.matrix_parallel,
                    &cfg.matrix_mapping,
                    &cfg.matrix_rgb_sequence,
                    cfg.matrix_slowdown,
                    cfg.matrix_pwm_bits,
                    cfg.matrix_pwm_lsb_nanoseconds,
                    cfg.matrix_disable_hardware_pulsing,
                    cfg.matrix_brightness as u8,
                ) {
                    Ok(hw) => Box::new(hw),
                    Err(e) => {
                        tracing::warn!(
                            "Failed to initialize HardwareMatrix, falling back to MockMatrix: {}",
                            e
                        );
                        Box::new(MockMatrix::new(
                            cfg.matrix_cols * cfg.matrix_chain,
                            cfg.matrix_rows,
                        ))
                    }
                }
            }
            #[cfg(not(all(
                target_os = "linux",
                any(target_arch = "arm", target_arch = "aarch64")
            )))]
            {
                let cfg = self.config.settings.read();
                Box::new(MockMatrix::new(
                    cfg.matrix_cols * cfg.matrix_chain,
                    cfg.matrix_rows,
                ))
            }
        };

        let width = matrix.width();
        let height = matrix.height();

        let mut clock_engine = ClockEngine::new(width, height);
        let mut date_engine = DateEngine::new(width, height);
        let mut weather_engine = WeatherEngine::new();
        let mut gif_engine = GifEngine::new(width, height);
        let mut fighter_engine = FighterEngine::new(width);
        let interval = self.config.settings.read().idle_fighter_interval;
        fighter_engine.init_fight(height, interval);
        let marquee_engine = MarqueeEngine::new();
        let mut message_engine = MessageEngine::new();

        let mut rotation_state = RotationState::new();
        let mut last_frame = std::time::Instant::now();
        let mut gifs_played = 0;

        // 1. Display startup IP Address banner on DMD matrix
        let startup_payload = MessagePayload {
            text: format!("IP: {}", local_ip),
            color: 0x07FF, // Cyan RGB565
            size: 1,
            direction: "left".to_string(),
            speed: 30,
            timeout_seconds: 4,
        };

        let start_time = std::time::Instant::now();
        while start_time.elapsed() < std::time::Duration::from_secs(4) {
            matrix.clear();
            message_engine.render(matrix.as_mut(), &startup_payload);
            matrix.update();
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        }

        // Force clear BOTH hardware buffers to permanently erase the startup IP
        matrix.clear();
        matrix.update();
        matrix.clear();
        matrix.update();

        let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r = running.clone();
        tokio::spawn(async move {
            let mut sigterm =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = sigterm.recv() => {},
            }
            tracing::info!("Received shutdown signal, stopping...");
            r.store(false, Ordering::SeqCst);
        });

        let mut last_mode = String::new();
        // 2. Main rotation and engine loop
        while running.load(Ordering::SeqCst) {
            matrix.clear();
            // Auto-restart if configuration requires hardware reload
            if self.config.reload_flag.swap(false, Ordering::Relaxed) {
                tracing::info!(
                    "Configuration changed! Restarting application to apply hardware settings..."
                );
                // Cleanly exit the loop. The process will drop the matrix and exit with code 0.
                // Systemd's Restart=always will relaunch the process cleanly after 3 seconds.
                break;
            }

            // Standby / Night mode check
            let (standby_enabled, turn_off_at, wake_up_at, night_bright) = {
                let s = self.config.settings.read();
                (
                    s.standby_enabled,
                    s.standby_turn_off.clone(),
                    s.standby_wake_up.clone(),
                    s.standby_night_brightness,
                )
            };

            let is_night =
                crate::core::rotation::is_night_time(standby_enabled, &turn_off_at, &wake_up_at);

            if is_night {
                if night_bright == 0 {
                    matrix.clear();
                    matrix.update();
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    continue;
                } else {
                    matrix.set_brightness(night_bright as u8);
                }
            } else {
                let bright = self.config.matrix_brightness.load(Ordering::Relaxed);
                matrix.set_brightness(bright as u8);
            }

            // Power check
            if !self.config.matrix_power.load(Ordering::Relaxed) {
                matrix.clear();
                matrix.update();
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                continue;
            }

            // Handle forced engine (Custom Message, Marquee Image)
            let forced = self.config.force_engine.lock().clone();
            if let Some(ref mode) = forced {
                // Clear the flag immediately so we don't infinitely trigger it
                *self.config.force_engine.lock() = None;

                if mode == "message" {
                    if let Some(ref payload_val) = *self.config.message_payload.lock() {
                        if let Ok(payload) =
                            serde_json::from_value::<MessagePayload>(payload_val.clone())
                        {
                            let start_time = std::time::Instant::now();
                            let timeout =
                                std::time::Duration::from_secs(payload.timeout_seconds as u64);

                            // Reset position to right edge
                            message_engine.reset(matrix.width() as f32);

                            // Block and render exclusively
                            while start_time.elapsed() < timeout {
                                if self.config.reload_flag.load(Ordering::Relaxed) {
                                    break;
                                }

                                // Check if another engine was forced while we were scrolling
                                if self.config.force_engine.lock().is_some() {
                                    break;
                                }

                                matrix.clear();
                                let finished = message_engine.render(matrix.as_mut(), &payload);
                                matrix.update();

                                if finished {
                                    break; // Message fully scrolled off screen
                                }

                                tokio::time::sleep(std::time::Duration::from_millis(33)).await;
                            }

                            // Resume normal flow after message completes
                            last_frame = std::time::Instant::now();
                            rotation_state.mode_start_time = std::time::Instant::now();
                            continue;
                        }
                    }
                } else if mode == "marquee" {
                    if let Some(ref img) = *self.config.image_obj.lock() {
                        while !self.config.reload_flag.load(Ordering::Relaxed) {
                            if self.config.force_engine.lock().is_some() {
                                break;
                            }

                            matrix.clear();
                            marquee_engine.render(matrix.as_mut(), img);
                            matrix.update();
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }

                        last_frame = std::time::Instant::now();
                        rotation_state.mode_start_time = std::time::Instant::now();
                        continue;
                    }
                }
            }

            // If MQTT is enabled, we act as a dedicated DMD for Recalbox and wait for games.
            let mqtt_enabled = self.config.settings.read().mqtt_enabled;
            if mqtt_enabled {
                matrix.clear();
                let payload = crate::engines::message::MessagePayload {
                    text: "Waiting for Recalbox...".to_string(),
                    color: 0xFD20, // Orange
                    size: if matrix.height() >= 64 { 2 } else { 1 },
                    direction: "left".to_string(),
                    speed: 40,
                    timeout_seconds: 60,
                };
                let _ = message_engine.render(matrix.as_mut(), &payload);
                matrix.update();
                last_frame = std::time::Instant::now();
                tokio::time::sleep(std::time::Duration::from_millis(33)).await;
                continue;
            }

            // Rotation sequence execution
            let (idle_list, clock_dur, date_dur, weather_dur) = {
                let s = self.config.settings.read();
                (
                    s.idle_rotation.clone(),
                    s.idle_clock_duration_sec,
                    s.idle_date_duration_sec,
                    s.idle_weather_duration_sec,
                )
            };

            if !idle_list.is_empty() {
                let current_mode = &idle_list[rotation_state.current_index % idle_list.len()];

                matrix.clear();
                match current_mode.as_str() {
                    "clock" => {
                        clock_engine.render(matrix.as_mut(), &self.config);
                        if rotation_state.mode_start_time.elapsed()
                            >= std::time::Duration::from_secs(clock_dur as u64)
                        {
                            rotation_state.next_mode(&idle_list);
                        }
                    }
                    "date" => {
                        date_engine.render(matrix.as_mut(), &self.config);
                        if rotation_state.mode_start_time.elapsed()
                            >= std::time::Duration::from_secs(date_dur as u64)
                        {
                            rotation_state.next_mode(&idle_list);
                        }
                    }
                    "weather" => {
                        weather_engine.render(matrix.as_mut(), &self.config);
                        if rotation_state.mode_start_time.elapsed()
                            >= std::time::Duration::from_secs(weather_dur as u64)
                        {
                            rotation_state.next_mode(&idle_list);
                        }
                    }
                    "gifs" => {
                        let gifs_count = self.config.settings.read().idle_gifs_count as u32;

                        if last_mode != "gifs" {
                            gifs_played = 0;
                            let selected = self.config.settings.read().selected_gifs.clone();
                            gif_engine.play_random_playlist_gif(&selected);
                        }

                        let dt = last_frame.elapsed();
                        gif_engine.render_next_frame(matrix.as_mut(), dt);

                        if gif_engine.has_finished_loops(1) {
                            gifs_played += 1;
                            if gifs_played >= gifs_count {
                                rotation_state.next_mode(&idle_list);
                            } else {
                                let selected = self.config.settings.read().selected_gifs.clone();
                                gif_engine.play_random_playlist_gif(&selected);
                            }
                        }
                    }
                    "network" => {
                        let ip = get_local_ip();
                        let payload = crate::engines::message::MessagePayload {
                            text: format!("IP: {}", ip),
                            color: 0x07FF,
                            size: 1,
                            direction: "left".to_string(),
                            speed: 30,
                            timeout_seconds: 10,
                        };

                        let _ = message_engine.render(matrix.as_mut(), &payload);
                        if rotation_state.mode_start_time.elapsed()
                            >= std::time::Duration::from_secs(10)
                        {
                            rotation_state.next_mode(&idle_list);
                        }
                    }
                    "message" => {
                        if let Some(ref payload_val) = *self.config.message_payload.lock() {
                            if let Ok(payload) = serde_json::from_value::<
                                crate::engines::message::MessagePayload,
                            >(payload_val.clone())
                            {
                                matrix.clear();
                                let finished = message_engine.render(matrix.as_mut(), &payload);
                                if finished
                                    || rotation_state.mode_start_time.elapsed()
                                        >= std::time::Duration::from_secs(
                                            payload.timeout_seconds as u64,
                                        )
                                {
                                    rotation_state.next_mode(&idle_list);
                                }
                            }
                        } else {
                            rotation_state.next_mode(&idle_list);
                        }
                    }
                    "marquee" => {
                        if let Some(ref img) = *self.config.image_obj.lock() {
                            marquee_engine.render(matrix.as_mut(), img);
                        }
                        if rotation_state.mode_start_time.elapsed()
                            >= std::time::Duration::from_secs(30)
                        {
                            rotation_state.next_mode(&idle_list);
                        }
                    }
                    _ => {
                        clock_engine.render(matrix.as_mut(), &self.config);
                    }
                }
                last_mode = current_mode.to_string();

                // Composite fighter overlay on every frame if sprites are loaded
                let settings = self.config.settings.read();
                let sprite_count = settings.idle_sprite_count;
                if sprite_count > 0 {
                    fighter_engine.set_interval(settings.idle_fighter_interval);
                    fighter_engine.composite(matrix.as_mut());
                }
                matrix.update();
            } else {
                clock_engine.render(matrix.as_mut(), &self.config);
                let settings = self.config.settings.read();
                let sprite_count = settings.idle_sprite_count;
                if sprite_count > 0 {
                    fighter_engine.set_interval(settings.idle_fighter_interval);
                    fighter_engine.composite(matrix.as_mut());
                }
                matrix.update();
            }

            // Adaptive sleep: animated modes at ~30fps, static at ~5fps
            let is_animated = matches!(
                idle_list
                    .get(rotation_state.current_index % idle_list.len().max(1))
                    .map(|s| s.as_str()),
                Some("clock") | Some("gifs") | Some("network") | Some("message")
            );
            let theme = self.config.settings.read().time_theme;
            let fast_theme = matches!(theme, 18 | 21 | 22 | 23 | 26 | 27 | 28 | 29);
            let sleep_ms = if is_animated || fast_theme { 33 } else { 200 };
            last_frame = std::time::Instant::now();
            tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)).await;
        }

        matrix.clear();
        matrix.update();
        Ok(())
    }
}

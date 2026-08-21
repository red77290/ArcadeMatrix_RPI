#![allow(unused_assignments)]
use crate::api::run_server;
use crate::core::config::Config;
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
use crate::core::matrix::HardwareMatrix;
use crate::core::matrix::{MatrixBackend, MockMatrix};
use crate::core::rotation::RotationState;

use crate::engines::message::MessagePayload;

use crate::core::arbiter::{DisplayArbiter, DisplayPriority, DisplayRequest, RequestLifecycle};

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
        let config = Arc::new(Config::new("config.json"));
        Self { config }
    }

    pub async fn run(&self) -> std::io::Result<()> {
        info!("Starting ArcadeMatrix RPi v{}", env!("CARGO_PKG_VERSION"));

        #[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
        {
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

            let boot_config = if std::path::Path::new("/boot/firmware/config.txt").exists() {
                "/boot/firmware/config.txt"
            } else {
                "/boot/config.txt"
            };

            if let Ok(content) = std::fs::read_to_string(boot_config) {
                let disable_wifi_cmd = "dtoverlay=disable-wifi";
                let s = self.config.settings.read().clone();
                let should_be_disabled = s.wifi.disable_internal;
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

        // 0. Setup Wi-Fi if needed
        {
            let s = self.config.settings.read().clone();
            if !s.wifi.ssid.is_empty() && !s.wifi.configured {
                info!("Attempting to configure Wi-Fi for SSID: {}", s.wifi.ssid);

                std::process::Command::new("sudo")
                    .args(["raspi-config", "nonint", "do_wifi_country", "FR"])
                    .output()
                    .ok();
                std::process::Command::new("sudo")
                    .args(["rfkill", "unblock", "wifi"])
                    .output()
                    .ok();
                std::thread::sleep(std::time::Duration::from_secs(2));

                let safe_ssid = s.wifi.ssid.replace(" ", "_").replace("/", "_");
                let nm_content = format!(
                    "[connection]\nid={safe_ssid}\ntype=wifi\nautoconnect=true\n\n\
                    [wifi]\nmode=infrastructure\nssid={ssid}\n\n\
                    [wifi-security]\nkey-mgmt=wpa-psk\npsk={pass}\n\n\
                    [ipv4]\nmethod=auto\n\n\
                    [ipv6]\naddr-gen-mode=default\nmethod=auto\n",
                    safe_ssid = safe_ssid,
                    ssid = s.wifi.ssid,
                    pass = s.wifi.password
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
                            ws.wifi.configured = true;
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

        // Start API server in its own thread, strictly single-threaded.
        // We use a custom current_thread Tokio runtime for Actix to prevent it from
        // spawning a multi-threaded reactor that spans all CPU cores and chokes Wi-Fi IRQs.
        // This perfectly matches the Python Flask behavior.
        let config_clone = Arc::clone(&self.config);
        std::thread::Builder::new()
            .name("api-server".to_string())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                let sys = actix_web::rt::System::with_tokio_rt(|| rt);
                if let Err(e) = sys.block_on(run_server(config_clone, 8080)) {
                    tracing::error!("API Server crashed: {}", e);
                }
            })
            .expect("Failed to spawn API thread");

        // Start MQTT client background worker
        crate::engines::frontend_sync::start_mqtt_client(Arc::clone(&self.config));

        // Graceful shutdown flag, shared with the render loop.
        let running = Arc::new(std::sync::atomic::AtomicBool::new(true));

        // Oneshot used by the render thread to notify the async runtime that it
        // exited on its own (e.g. a configuration change set reload_flag). Without
        // it, run() would await the shutdown signals forever after a self-restart.
        let (render_done_tx, render_done_rx) = tokio::sync::oneshot::channel::<()>();

        // Run the render loop in a dedicated OS thread, completely isolated from
        // the tokio async runtime. This is the same architecture as Python where
        // the matrix render loop runs in the main thread and the web server in a
        // daemon thread. This prevents tokio from scheduling async I/O tasks
        // (HTTP, MQTT, Wi-Fi IRQs) in the same OS thread as the LED DMA engine.
        let config_for_render = Arc::clone(&self.config);
        let running_for_render = Arc::clone(&running);
        let render_handle = std::thread::Builder::new()
            .name("matrix-render".to_string())
            .stack_size(8 * 1024 * 1024) // 8MB stack for render thread
            .spawn(move || {
                // No CPU pinning here: we rely on the kernel's isolcpus=3 (set by autoInstall.sh)
                // to reserve core 3 for the hzeller DMA thread, exactly like the Python version.
                // The OS scheduler freely distributes our render thread and other work across
                // the remaining cores (0, 1, 2).
                Self::render_loop(config_for_render, local_ip, running_for_render);
                let _ = render_done_tx.send(());
            })
            .expect("Failed to spawn render thread");

        // Await either an OS shutdown signal or the render loop self-exiting.
        //
        // We AWAIT here rather than blocking on render_handle.join(). This is
        // critical: main() uses a `current_thread` Tokio runtime, so blocking the
        // main thread would starve the signal futures and SIGTERM would never be
        // observed. systemd would then SIGKILL us after its stop timeout, skipping
        // the LED matrix destructor and leaving GPIO/DMA locked -> the next start
        // fails with "Couldn't create LedMatrix" and falls back to a black screen.
        #[cfg(unix)]
        {
            let mut sigterm =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
            let mut sigint =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
            tokio::select! {
                _ = sigterm.recv() => tracing::info!("SIGTERM received, shutting down."),
                _ = sigint.recv() => tracing::info!("SIGINT received, shutting down."),
                _ = render_done_rx => tracing::info!("Render loop exited on its own (self-restart)."),
            }
        }
        #[cfg(not(unix))]
        {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = render_done_rx => {}
            }
        }

        // Ask the render loop to stop and wait for it, so the LED matrix
        // destructor runs and releases GPIO/DMA cleanly before we exit.
        running.store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = render_handle.join();
        Ok(())
    }

    fn render_loop(
        config: Arc<Config>,
        local_ip: String,
        running: Arc<std::sync::atomic::AtomicBool>,
    ) {
        // Initialize hardware matrix
        let mut matrix: Box<dyn MatrixBackend> = {
            #[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
            {
                // Snapshot the matrix config, then release the settings lock so we
                // never hold it across the (potentially retried) hardware init.
                let (
                    rows,
                    cols,
                    chain,
                    parallel,
                    mapping,
                    rgb_sequence,
                    slowdown,
                    pwm_bits,
                    pwm_lsb,
                    disable_pulsing,
                    brightness,
                    limit_refresh,
                    driver_chip,
                    multiplexing,
                    row_addr_type,
                ) = {
                    let cfg = config.settings.read();
                    (
                        cfg.matrix.height,
                        cfg.matrix.width,
                        cfg.matrix.chain_length,
                        1,
                        cfg.matrix.mapping.clone(),
                        cfg.matrix.rgb_sequence.clone(),
                        cfg.matrix.slowdown,
                        cfg.matrix.pwm_bits,
                        cfg.matrix.pwm_lsb_nanoseconds,
                        cfg.matrix.disable_hardware_pulsing,
                        100 as u8,
                        cfg.matrix.limit_refresh_rate_hz,
                        cfg.matrix.driver_chip.clone(),
                        cfg.matrix.multiplexing,
                        cfg.matrix.row_address_mode,
                    )
                };

                // Retry hardware init instead of instantly falling back to a Mock
                // (black) matrix. After a "restart to apply settings" the previous
                // process can still hold the GPIO/DMA for a moment, which makes the
                // very next init fail with "Couldn't create LedMatrix". Retrying
                // until the hardware is released prevents a permanent black screen.
                const MAX_INIT_ATTEMPTS: u32 = 30;
                let mut attempt = 0u32;
                loop {
                    match HardwareMatrix::new(
                        rows,
                        cols,
                        chain,
                        parallel,
                        multiplexing,
                        row_addr_type,
                        &mapping,
                        &rgb_sequence,
                        slowdown,
                        pwm_bits,
                        pwm_lsb,
                        disable_pulsing,
                        brightness,
                        limit_refresh,
                        &driver_chip,
                    ) {
                        Ok(hw) => break Box::new(hw) as Box<dyn MatrixBackend>,
                        Err(e) => {
                            attempt += 1;
                            if attempt >= MAX_INIT_ATTEMPTS {
                                tracing::error!(
                                    "Failed to initialize hardware matrix after {} attempts: {} \u{2014} falling back to Mock (BLACK SCREEN)",
                                    attempt,
                                    e
                                );
                                break Box::new(MockMatrix::new(cols * chain, rows))
                                    as Box<dyn MatrixBackend>;
                            }
                            tracing::warn!(
                                "Hardware matrix init failed (attempt {}/{}): {} \u{2014} previous process may still hold the GPIO, retrying in 1s",
                                attempt,
                                MAX_INIT_ATTEMPTS,
                                e
                            );
                            std::thread::sleep(std::time::Duration::from_secs(1));
                        }
                    }
                }
            }
            #[cfg(not(all(
                target_os = "linux",
                any(target_arch = "arm", target_arch = "aarch64")
            )))]
            Box::new(MockMatrix::new(
                {
                    let cfg = config.settings.read();
                    cfg.matrix.width * cfg.matrix.chain_length
                },
                {
                    let cfg = config.settings.read();
                    cfg.matrix.height
                },
            ))
        };

        let width = matrix.width();
        let height = matrix.height();

        let mut runtime = crate::core::registry::EngineRuntime::new();
        let mut rotation_state = RotationState::new();
        let mut arbiter = DisplayArbiter::new();
        let mut last_frame = std::time::Instant::now();
        let gifs_played = 0;

        let marquee_engine = crate::engines::marquee::MarqueeEngine::new();
        let mut message_engine = crate::engines::message::MessageEngine::new();

        // Display startup IP Address banner
        let startup_payload =
            MessagePayload::new(format!("IP: {}", local_ip), "#00ffc8", 1, "left", 4);

        let start_time = std::time::Instant::now();
        while start_time.elapsed() < std::time::Duration::from_secs(4) {
            matrix.clear();
            message_engine.render_payload(matrix.as_mut(), &startup_payload);
            matrix.update();
            std::thread::sleep(std::time::Duration::from_millis(30));
        }

        matrix.clear();
        matrix.update();
        matrix.clear();
        matrix.update();

        // The actual SIGTERM is handled by tokio in the async context, which updates the 'running' atomic bool.
        let mut last_index = usize::MAX;
        let mut was_mqtt_waiting = false;
        let mut has_received_mqtt = false;

        while running.load(Ordering::SeqCst) {
            matrix.clear();

            // Auto-restart if configuration requires hardware reload
            if config.reload_flag.swap(false, Ordering::Relaxed) {
                tracing::info!(
                    "Configuration changed! Restarting application to apply hardware settings..."
                );
                break;
            }

            if config.reset_rotation.swap(false, Ordering::Relaxed) {
                rotation_state.current_index = 0;
                rotation_state.mode_start_time = std::time::Instant::now();
                last_index = usize::MAX; // Force mode_just_changed to be true
                tracing::info!("Display settings changed! Resetting rotation to index 0.");
            }

            // Standby / Night mode check
            let (standby_enabled, turn_off_at, wake_up_at, night_bright) = {
                let s = config.settings.read();
                (
                    s.system.night_mode_enabled,
                    s.system.turn_off_at.clone(),
                    s.system.wake_up_at.clone(),
                    s.system.night_brightness,
                )
            };

            let is_night =
                crate::core::rotation::is_night_time(standby_enabled, &turn_off_at, &wake_up_at);

            if is_night {
                if night_bright == 0 {
                    matrix.clear();
                    matrix.update();
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    continue;
                } else {
                    matrix.set_brightness(night_bright as u8);
                }
            } else {
                let bright = config.matrix_brightness.load(Ordering::Relaxed);
                matrix.set_brightness(bright as u8);
            }

            // Power check
            if !config.matrix_power.load(Ordering::Relaxed) {
                matrix.clear();
                matrix.update();
                std::thread::sleep(std::time::Duration::from_millis(100));
                continue;
            }

            // --- ARBITER INTEGRATION ---
            let forced_opt = config.force_engine.lock().clone();

            // 1. Submit MQTT/Message Request if active
            if let Some(ref forced_mode) = forced_opt {
                if forced_mode == "message" {
                    has_received_mqtt = true;
                    let payload_val_opt = config.message_payload.lock().clone();
                    if let Some(payload_val) = payload_val_opt {
                        if let Ok(payload) =
                            serde_json::from_value::<MessagePayload>(payload_val.clone())
                        {
                            let mut req = DisplayRequest::new(
                                "MESSAGE",
                                DisplayPriority::Mqtt,
                                RequestLifecycle::OneShot,
                            );
                            req.preemptive = true;
                            if payload.timeout_seconds > 0 {
                                req.lifecycle = RequestLifecycle::Timed;
                                req.timeout = Some(std::time::Duration::from_secs(
                                    payload.timeout_seconds as u64,
                                ));
                            }
                            req.instance_id = "mqtt_message".to_string();
                            arbiter.submit_request(req);
                        }
                    }
                } else if forced_mode == "marquee" {
                    has_received_mqtt = true;
                    let mut req = DisplayRequest::new(
                        "MARQUEE",
                        DisplayPriority::Marquee,
                        RequestLifecycle::UntilCancelled,
                    );
                    req.preemptive = true;
                    req.instance_id = "marquee".to_string();
                    arbiter.submit_request(req);
                }
            } else {
                arbiter.cancel_request("MESSAGE");
                arbiter.cancel_request("MARQUEE");
            }

            let mqtt_enabled = { config.settings.read().mqtt.enabled };
            if mqtt_enabled && !has_received_mqtt && forced_opt.is_none() {
                let req = DisplayRequest::new(
                    "WAITING_MARQUEE",
                    DisplayPriority::Marquee,
                    RequestLifecycle::UntilCancelled,
                );
                arbiter.submit_request(req);
            } else {
                arbiter.cancel_request("WAITING_MARQUEE");
            }

            // 2. Submit Rotation Request
            let idle_list = config.settings.read().rotation.clone();
            if !idle_list.is_empty() {
                let current_index = rotation_state.current_index;
                let current_mode = &idle_list[current_index % idle_list.len()];
                let mut req = DisplayRequest::new(
                    "ROTATION",
                    DisplayPriority::Rotation,
                    RequestLifecycle::Persistent,
                );
                req.preemptive = false;
                req.instance_id = current_mode.instance_id.clone();
                arbiter.submit_request(req);
            } else {
                arbiter.cancel_request("ROTATION");
            }

            // 3. Evaluate Arbiter
            let active_req = arbiter.evaluate();

            if let Some(req) = active_req {
                if req.source == "MESSAGE" {
                    let payload_val_opt = config.message_payload.lock().clone();
                    if let Some(payload_val) = payload_val_opt {
                        if let Ok(payload) = serde_json::from_value::<MessagePayload>(payload_val) {
                            matrix.clear();
                            let finished = message_engine.render_payload(matrix.as_mut(), &payload);
                            matrix.update();

                            if finished
                                && req.lifecycle != RequestLifecycle::UntilCancelled
                                && req.lifecycle != RequestLifecycle::Persistent
                            {
                                let mut f = config.force_engine.lock();
                                if f.as_deref() == Some("message") {
                                    *f = None;
                                }
                                arbiter.cancel_request("MESSAGE");
                            }

                            last_frame = std::time::Instant::now();
                            std::thread::sleep(std::time::Duration::from_millis(33));
                            continue;
                        }
                    }
                } else if req.source == "MARQUEE" {
                    let img_opt = config.image_obj.lock().clone();
                    if let Some(img) = img_opt {
                        matrix.clear();
                        marquee_engine.render_image(matrix.as_mut(), &img);
                        matrix.update();
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        last_frame = std::time::Instant::now();
                        continue;
                    }
                } else if req.source == "WAITING_MARQUEE" {
                    if !was_mqtt_waiting {
                        message_engine.reset_state(matrix.width() as f32);
                        was_mqtt_waiting = true;
                    }
                    let msg_payload = crate::engines::message::MessagePayload::new(
                        "WAITING FOR MARQUEE".to_string(),
                        "#ffffff",
                        2,
                        "rtl",
                        5,
                    );
                    matrix.clear();
                    message_engine.render_payload(matrix.as_mut(), &msg_payload);
                    matrix.update();
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    last_frame = std::time::Instant::now();
                    continue;
                }
            }

            // Rotation sequence execution
            let idle_list = config.settings.read().rotation.clone();
            let mut realtime_cadence = false;

            if !idle_list.is_empty() {
                let current_index = rotation_state.current_index;
                let current_mode = &idle_list[current_index % idle_list.len()];
                let mode_just_changed = current_index != last_index;

                // Find engine_id for this instance
                let engine_id = config
                    .settings
                    .read()
                    .instances
                    .iter()
                    .find(|i| i.instance_id == current_mode.instance_id)
                    .map(|i| i.engine_id.clone())
                    .unwrap_or_else(|| current_mode.instance_id.clone()); // fallback to legacy strings if no instance

                matrix.clear();
                let empty_map = std::collections::HashMap::new();
                let settings = config.settings.read();
                let inst_config = settings
                    .instances
                    .iter()
                    .find(|i| i.instance_id == current_mode.instance_id)
                    .map(|inst| &inst.config)
                    .unwrap_or(&empty_map);

                let engine_config = inst_config;

                // Whether the active engine needs a high frame rate. Derived from the
                // engine descriptor's `realtime` capability (Sprint 3 metadata) instead
                // of legacy hardcoded instance-id string matching.
                let mut current_realtime =
                    crate::core::registry::EngineRegistry::get_descriptor(&engine_id)
                        .map(|d| d.capabilities.realtime)
                        .unwrap_or(false);

                {
                    let mut ctx = crate::core::engine_contract::EngineContext {
                        matrix: matrix.as_mut(),
                        config: &config,
                    };

                    // get_instance handles initialization internally exactly once!
                    if let Some(engine) = runtime.get_instance(
                        &current_mode.instance_id,
                        &engine_id,
                        &mut ctx,
                        engine_config,
                    ) {
                        if mode_just_changed {
                            engine.activate();
                        }

                        engine.update(&mut ctx);
                        engine.render(&mut ctx);

                        // Advance rotation based on duration OR engine completion
                        if engine.is_finished()
                            || rotation_state.mode_start_time.elapsed()
                                >= std::time::Duration::from_secs(current_mode.duration_sec as u64)
                        {
                            rotation_state.next_mode(&idle_list);
                        }
                    } else {
                        // Fallback if engine fails to load
                        current_realtime = false;
                        if rotation_state.mode_start_time.elapsed()
                            >= std::time::Duration::from_secs(current_mode.duration_sec as u64)
                        {
                            rotation_state.next_mode(&idle_list);
                        }
                    }
                }

                last_index = current_index;
                realtime_cadence = current_realtime;
                matrix.update();
            } else {
                let dict = std::collections::HashMap::new();
                let mut ctx = crate::core::engine_contract::EngineContext {
                    matrix: matrix.as_mut(),
                    config: &config,
                };
                if let Some(engine) =
                    runtime.get_instance("fallback_clock", "clock", &mut ctx, &dict)
                {
                    engine.update(&mut ctx);
                    engine.render(&mut ctx);
                }
                realtime_cadence = false;
                matrix.update();
            }

            // Adaptive sleep: realtime engines (gif, scrolling text, spotify) run at
            // ~25fps; static engines (clock, date, weather) refresh once per second to
            // leave the CPU and Wi-Fi IRQs plenty of breathing room.
            let sleep_ms = if realtime_cadence { 40 } else { 1000 };
            last_frame = std::time::Instant::now();
            std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
        }

        matrix.clear();
        matrix.update();
        tracing::info!("Render loop exited cleanly.");
    }
}

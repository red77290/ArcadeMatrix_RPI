#![allow(unused_assignments)]
use crate::api::run_server;
use crate::core::arbiter::DisplayArbiter;
use crate::core::config::Config;
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
use crate::core::matrix::HardwareMatrix;
use crate::core::matrix::{MatrixBackend, MockMatrix};
use crate::core::registry::EngineRuntime;
use crate::core::rotation_manager::RotationManager;
use crate::core::runtime::DisplayRuntime;
use crate::core::types::{
    DisplayRequest, DisplaySourceId, EngineHandle, ProducerSyncState, RequestLifecycle,
};

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
        info!(
            "Starting ArcadeMatrix RPi v{} (build {} @ {})",
            crate::core::build_info::VERSION,
            crate::core::build_info::GIT_COMMIT,
            crate::core::build_info::BUILD_TIMESTAMP
        );

        #[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
        {
            let sudoers_file = "/etc/sudoers.d/010_arcadematrix_daemon";
            let sudoers_content =
                "ALL ALL=(ALL) NOPASSWD: /usr/bin/nmcli, /sbin/shutdown, /usr/sbin/shutdown, /sbin/reboot, /usr/sbin/reboot, /sbin/poweroff, /usr/sbin/poweroff, /bin/systemctl, /usr/bin/systemctl\n";
            if std::fs::read_to_string(sudoers_file).unwrap_or_default() != sudoers_content {
                info!("Granting sudo privileges for system power and network commands...");
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

        let mut arbiter = DisplayArbiter::new();
        let mut runtime = DisplayRuntime::new();
        let mut engine_runtime = EngineRuntime::new();
        let mut rotation_manager = RotationManager::new();
        let mut overlay_manager = crate::core::overlay_manager::OverlayManager::new(width, height);
        let mut message_engine = crate::engines::message::MessageEngine::new();

        let mut message_sync = ProducerSyncState::INIT;
        let mut marquee_sync = ProducerSyncState::INIT;
        let mut waiting_marquee_sync = ProducerSyncState::INIT;
        let mut rotation_sync = ProducerSyncState::INIT;
        let mut has_received_mqtt = false;

        // 1. Run interactive Startup Splash Screen (Mario hammer smash & Pacman eat particles)
        // while polling for real network IP in the background.
        let active_ip = crate::core::splash::SplashScreen::run(
            matrix.as_mut(),
            &running,
            std::time::Duration::from_millis(5500),
            std::time::Duration::from_secs(12),
        );
        info!("ArcadeMatrix RPi Active IP after splash: {}", active_ip);

        // 2. Display startup IP Address banner with the resolved network IP
        let startup_payload = crate::engines::message::MessagePayload::new(
            format!("IP: {}", active_ip),
            "#00ffc8",
            1,
            "left",
            4,
        );

        let start_time = std::time::Instant::now();
        while running.load(Ordering::SeqCst)
            && start_time.elapsed() < std::time::Duration::from_secs(4)
        {
            matrix.clear();
            message_engine.render_payload(matrix.as_mut(), &startup_payload);
            matrix.update();
            std::thread::sleep(std::time::Duration::from_millis(30));
        }

        matrix.clear();
        matrix.update();
        matrix.clear();
        matrix.update();

        while running.load(Ordering::SeqCst) {
            // Auto-restart if configuration requires hardware reload
            if config.reload_flag.swap(false, Ordering::Relaxed) {
                tracing::info!(
                    "Configuration changed! Restarting application to apply hardware settings..."
                );
                break;
            }

            if config.reset_rotation.swap(false, Ordering::Relaxed) {
                rotation_manager.reset();
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

            // --- PRODUCER SYNCHRONIZATION (EDGE-TRIGGERED) ---

            // 1. Sync MQTT/Message & Marquee Producers
            let forced_opt = config.force_engine.lock().clone();
            if let Some(ref forced_mode) = forced_opt {
                if forced_mode == "message" {
                    has_received_mqtt = true;
                    let payload_val_opt = config.message_payload.lock().clone();
                    if let Some(payload_val) = payload_val_opt {
                        if let Ok(payload) = serde_json::from_value::<
                            crate::engines::message::MessagePayload,
                        >(payload_val)
                        {
                            let handle =
                                engine_runtime.register_instance_handle("mqtt_message", "message");
                            let req_id = 100;
                            let duration_ms = if payload.timeout_seconds > 0 {
                                (payload.timeout_seconds * 1000) as u32
                            } else {
                                5000
                            };
                            let req = DisplayRequest::new(
                                DisplaySourceId::Mqtt,
                                req_id,
                                handle,
                                DisplaySourceId::Mqtt as u8,
                                RequestLifecycle::Transient,
                                true,
                                duration_ms,
                            );
                            if message_sync.has_changed(true, req_id, handle) {
                                arbiter.submit_request(req);
                                message_sync.update(true, req_id, handle);
                            }
                        }
                    }
                } else if forced_mode == "marquee" {
                    has_received_mqtt = true;
                    let handle = engine_runtime.register_instance_handle("marquee", "marquee");
                    let req_id = 200;
                    let req = DisplayRequest::new(
                        DisplaySourceId::Marquee,
                        req_id,
                        handle,
                        DisplaySourceId::Marquee as u8,
                        RequestLifecycle::Persistent,
                        true,
                        0,
                    );
                    if marquee_sync.has_changed(true, req_id, handle) {
                        arbiter.submit_request(req);
                        marquee_sync.update(true, req_id, handle);
                    }
                }
            } else {
                if message_sync.active {
                    arbiter.cancel_request(DisplaySourceId::Mqtt, 0);
                    message_sync.update(false, 0, EngineHandle::NULL);
                }
                if marquee_sync.active {
                    arbiter.cancel_request(DisplaySourceId::Marquee, 0);
                    marquee_sync.update(false, 0, EngineHandle::NULL);
                }
            }

            // 2. Sync Waiting Marquee
            let mqtt_enabled = { config.settings.read().mqtt.enabled };
            if mqtt_enabled && !has_received_mqtt && forced_opt.is_none() {
                let handle = engine_runtime.register_instance_handle("waiting_marquee", "message");
                let req_id = 50;
                let req = DisplayRequest::new(
                    DisplaySourceId::Marquee,
                    req_id,
                    handle,
                    DisplaySourceId::Marquee as u8,
                    RequestLifecycle::Persistent,
                    true,
                    0,
                );
                if waiting_marquee_sync.has_changed(true, req_id, handle) {
                    arbiter.submit_request(req);
                    waiting_marquee_sync.update(true, req_id, handle);
                }
            } else if waiting_marquee_sync.active {
                arbiter.cancel_request(DisplaySourceId::Marquee, 50);
                waiting_marquee_sync.update(false, 0, EngineHandle::NULL);
            }

            // 3. Sync Rotation Producer
            let idle_list = config.settings.read().rotation.clone();
            if !idle_list.is_empty() {
                if let Some(rot_req) = rotation_manager.build_rotation_request(
                    &idle_list,
                    &mut engine_runtime,
                    &config.settings.read(),
                ) {
                    if rotation_sync.has_changed(true, rot_req.request_id, rot_req.engine_handle) {
                        arbiter.submit_request(rot_req);
                        rotation_sync.update(true, rot_req.request_id, rot_req.engine_handle);
                    }
                }
            } else if rotation_sync.active {
                arbiter.cancel_request(DisplaySourceId::Rotation, 0);
                rotation_sync.update(false, 0, EngineHandle::NULL);
            }

            // 4. Evaluate Arbiter
            let decision = arbiter.evaluate(std::time::Instant::now());

            // 5. Resolve active config map
            let settings = config.settings.read().clone();
            let empty_map = std::collections::HashMap::new();
            let active_config_map = if let Some((inst_name, _)) =
                engine_runtime.handle_to_names(decision.engine_handle)
            {
                settings
                    .instances
                    .iter()
                    .find(|i| i.instance_id == inst_name)
                    .map(|i| &i.config)
                    .unwrap_or(&empty_map)
            } else {
                &empty_map
            };

            // 6 & 7. Transition, Update & Render Active Engine in scoped context
            let mut realtime_cadence = false;
            let mut is_finished = false;
            let mut self_paced = false;
            let mut allows_overlay = false;

            {
                let mut ctx = crate::core::engine_contract::EngineContext {
                    matrix: matrix.as_mut(),
                    config: &config,
                };
                runtime.transition_session(
                    decision,
                    &arbiter,
                    &mut engine_runtime,
                    &mut ctx,
                    active_config_map,
                );

                ctx.matrix.clear();
                runtime.update(&mut engine_runtime, &mut ctx);
                runtime.render(&mut engine_runtime, &mut ctx);

                if let Some(engine) =
                    engine_runtime.get_active_instance(runtime.active_session().engine_handle)
                {
                    realtime_cadence = engine.is_realtime();
                    is_finished = engine.is_finished();
                    self_paced = engine.self_paced();
                    allows_overlay = engine.allows_overlay();
                }
            }

            // 8. Configure & Composite Overlay
            if allows_overlay {
                if let Some(rot_entry) = rotation_manager.current_entry(&idle_list) {
                    overlay_manager.configure(&rot_entry.overlays, &settings.system);
                } else {
                    overlay_manager.configure(
                        &crate::core::config::OverlayConfig::default(),
                        &settings.system,
                    );
                }
            } else {
                overlay_manager.configure(
                    &crate::core::config::OverlayConfig::default(),
                    &settings.system,
                );
            }

            // Check rotation advance
            if runtime.active_session().source_id == DisplaySourceId::Rotation {
                let _ = rotation_manager.evaluate_advance(&idle_list, is_finished, self_paced);
            }

            overlay_manager.composite(matrix.as_mut());
            realtime_cadence = realtime_cadence || overlay_manager.is_active();
            matrix.update();

            // Adaptive sleep: realtime engines run at ~25fps (40ms), static at 1000ms
            let sleep_ms = if realtime_cadence { 40 } else { 1000 };
            std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
        }

        matrix.clear();
        matrix.update();
        tracing::info!("Render loop exited cleanly.");
    }
}

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

        // 0. Setup Wi-Fi if needed
        {
            let s = self.config.settings.read().clone();
            if !s.wifi_ssid.is_empty() && !s.wifi_configured {
                info!("Attempting to configure Wi-Fi for SSID: {}", s.wifi_ssid);

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
        crate::engines::network::start_mqtt_client(Arc::clone(&self.config));

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
                        cfg.matrix_rows,
                        cfg.matrix_cols,
                        cfg.matrix_chain,
                        cfg.matrix_parallel,
                        cfg.matrix_mapping.clone(),
                        cfg.matrix_rgb_sequence.clone(),
                        cfg.matrix_slowdown,
                        cfg.matrix_pwm_bits,
                        cfg.matrix_pwm_lsb_nanoseconds,
                        cfg.matrix_disable_hardware_pulsing,
                        cfg.matrix_brightness as u8,
                        cfg.matrix_limit_refresh_rate_hz,
                        cfg.matrix_driver_chip.clone(),
                        cfg.matrix_multiplexing,
                        cfg.matrix_row_addr_type,
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
                                break Box::new(MockMatrix::new(64, 32)) as Box<dyn MatrixBackend>;
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
                    cfg.matrix_cols * cfg.matrix_chain
                },
                {
                    let cfg = config.settings.read();
                    cfg.matrix_rows
                },
            ))
        };

        let width = matrix.width();
        let height = matrix.height();

        let mut clock_engine = ClockEngine::new(width, height);
        let mut date_engine = DateEngine::new(width, height);
        let mut weather_engine = WeatherEngine::new();
        weather_engine.add_provider(Box::new(crate::api::OpenWeatherMapProvider));

        let mut crypto_engine = crate::engines::crypto::CryptoEngine::new(width, height);
        crypto_engine.add_provider(Box::new(crate::api::CoinGeckoProvider));
        crypto_engine.add_provider(Box::new(crate::api::BinanceProvider));

        let mut stock_engine = crate::engines::stock::StockEngine::new(width, height);
        stock_engine.add_provider(Box::new(crate::api::YahooFinanceProvider));
        let mut gif_engine = GifEngine::new(width, height);
        let mut fighter_engine = FighterEngine::new(width, height);
        let marquee_engine = MarqueeEngine::new();
        let mut message_engine = MessageEngine::new();

        let mut rotation_state = RotationState::new();
        let mut last_frame = std::time::Instant::now();
        let mut gifs_played = 0;

        // Display startup IP Address banner
        let startup_payload =
            MessagePayload::new(format!("IP: {}", local_ip), "#00ffc8", 1, "left", 4);

        let start_time = std::time::Instant::now();
        while start_time.elapsed() < std::time::Duration::from_secs(4) {
            matrix.clear();
            message_engine.render(matrix.as_mut(), &startup_payload);
            matrix.update();
            std::thread::sleep(std::time::Duration::from_millis(30));
        }

        matrix.clear();
        matrix.update();
        matrix.clear();
        matrix.update();

        // The actual SIGTERM is handled by tokio in the async context, which updates the 'running' atomic bool.
        let mut last_index = usize::MAX;

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

            // Handle forced engine (Custom Message, Marquee Image)
            let forced = config.force_engine.lock().clone();
            if let Some(ref mode) = forced {
                *config.force_engine.lock() = None;

                if mode == "message" {
                    let payload_val_opt = config.message_payload.lock().clone();
                    if let Some(payload_val) = payload_val_opt {
                        if let Ok(payload) =
                            serde_json::from_value::<MessagePayload>(payload_val.clone())
                        {
                            let start_time = std::time::Instant::now();
                            let timeout =
                                std::time::Duration::from_secs(payload.timeout_seconds as u64);

                            message_engine.reset(matrix.width() as f32);

                            while start_time.elapsed() < timeout && running.load(Ordering::SeqCst) {
                                if config.reload_flag.load(Ordering::Relaxed) {
                                    break;
                                }
                                if config.force_engine.lock().is_some() {
                                    break;
                                }

                                matrix.clear();
                                let finished = message_engine.render(matrix.as_mut(), &payload);
                                matrix.update();

                                if finished {
                                    break;
                                }

                                std::thread::sleep(std::time::Duration::from_millis(33));
                            }

                            last_frame = std::time::Instant::now();
                            rotation_state.mode_start_time = std::time::Instant::now();
                            continue;
                        }
                    }
                } else if mode == "marquee" {
                    let img_opt = config.image_obj.lock().clone();
                    if let Some(img) = img_opt {
                        while !config.reload_flag.load(Ordering::Relaxed)
                            && running.load(Ordering::SeqCst)
                        {
                            if config.force_engine.lock().is_some() {
                                break;
                            }

                            matrix.clear();
                            marquee_engine.render(matrix.as_mut(), &img);
                            matrix.update();
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        }

                        last_frame = std::time::Instant::now();
                        rotation_state.mode_start_time = std::time::Instant::now();
                        continue;
                    }
                }
            }

            // MQTT mode: dedicated DMD for Recalbox/Batocera (only when no active game event)
            let mqtt_enabled = config.settings.read().mqtt_enabled;
            if mqtt_enabled {
                let current_force = config.force_engine.lock().clone();
                if current_force.is_none() {
                    matrix.clear();
                    let payload = crate::engines::message::MessagePayload::new(
                        "Waiting for Content...".to_string(),
                        "#ff8c00",
                        if matrix.height() >= 64 { 2 } else { 1 },
                        "left",
                        60, // Arbitrary for idle
                    );
                    let _ = message_engine.render(matrix.as_mut(), &payload);
                    matrix.update();
                    last_frame = std::time::Instant::now();
                    std::thread::sleep(std::time::Duration::from_millis(33));
                    continue;
                }
            }

            // Rotation sequence execution
            let (idle_list, clock_dur, date_dur, weather_dur) = {
                let s = config.settings.read();
                (
                    s.idle_rotation.clone(),
                    s.idle_clock_duration_sec,
                    s.idle_date_duration_sec,
                    s.idle_weather_duration_sec,
                )
            };

            if !idle_list.is_empty() {
                let current_index = rotation_state.current_index;
                let current_mode = &idle_list[current_index % idle_list.len()];
                let mode_just_changed = current_index != last_index;

                matrix.clear();
                let mut should_update = true;
                match current_mode.as_str() {
                    "clock" => {
                        clock_engine.render(matrix.as_mut(), &config);
                        if rotation_state.mode_start_time.elapsed()
                            >= std::time::Duration::from_secs(clock_dur as u64)
                        {
                            rotation_state.next_mode(&idle_list);
                        }
                    }
                    "date" => {
                        date_engine.render(matrix.as_mut(), &config);
                        if rotation_state.mode_start_time.elapsed()
                            >= std::time::Duration::from_secs(date_dur as u64)
                        {
                            rotation_state.next_mode(&idle_list);
                        }
                    }
                    "weather" => {
                        weather_engine.render(matrix.as_mut(), &config);
                        if rotation_state.mode_start_time.elapsed()
                            >= std::time::Duration::from_secs(weather_dur as u64)
                        {
                            rotation_state.next_mode(&idle_list);
                        }
                    }
                    "crypto" => {
                        crypto_engine.render(matrix.as_mut(), &config);
                        if rotation_state.mode_start_time.elapsed()
                            >= std::time::Duration::from_secs(10)
                        {
                            rotation_state.next_mode(&idle_list);
                        }
                    }
                    "stocks" | "stock" => {
                        stock_engine.render(matrix.as_mut(), &config);
                        if rotation_state.mode_start_time.elapsed()
                            >= std::time::Duration::from_secs(10)
                        {
                            rotation_state.next_mode(&idle_list);
                        }
                    }
                    "gifs" => {
                        let gifs_count = config.settings.read().idle_gifs_count as u32;

                        if mode_just_changed {
                            gifs_played = 0;
                            let selected = config.settings.read().selected_gifs.clone();
                            if !gif_engine.play_random_playlist_gif(&selected) {
                                gifs_played = gifs_count; // Force advance if failed
                            }
                        }

                        let dt = last_frame.elapsed();
                        let sprites_on = config.settings.read().idle_fighter_enabled;
                        let frame_changed = gif_engine.render_next_frame(matrix.as_mut(), dt);
                        if sprites_on {
                            // Fighter overlay animates every iteration: the gif image
                            // must be present on the freshly-cleared canvas.
                            if !frame_changed {
                                gif_engine.redraw_current(matrix.as_mut());
                            }
                        } else {
                            // Skip the (memory-bus heavy) canvas swap when the gif
                            // frame is unchanged: prevents DDR/SDIO DMA starvation
                            // that was dropping Wi-Fi packets during playback.
                            should_update = frame_changed;
                        }

                        if gif_engine.has_finished_loops(1) || gif_engine.is_empty() {
                            gifs_played += 1;
                            if gifs_played >= gifs_count {
                                gifs_played = 0;
                                let next_mode_opt = rotation_state.next_mode(&idle_list);
                                if next_mode_opt == Some("gifs") {
                                    let selected = config.settings.read().selected_gifs.clone();
                                    if !gif_engine.play_random_playlist_gif(&selected) {
                                        gifs_played = gifs_count; // Force advance if failed
                                    }
                                }
                            } else {
                                let selected = config.settings.read().selected_gifs.clone();
                                if !gif_engine.play_random_playlist_gif(&selected) {
                                    gifs_played = gifs_count; // Force advance if failed
                                }
                            }
                        }
                    }
                    "network" => {
                        let ip = get_local_ip();
                        let payload = crate::engines::message::MessagePayload::new(
                            format!("IP: {}", ip),
                            "#00ffc8",
                            1,
                            "left",
                            10,
                        );

                        let _ = message_engine.render(matrix.as_mut(), &payload);
                        if rotation_state.mode_start_time.elapsed()
                            >= std::time::Duration::from_secs(10)
                        {
                            rotation_state.next_mode(&idle_list);
                        }
                    }
                    "message" => {
                        let payload_val_opt = config.message_payload.lock().clone();
                        if let Some(payload_val) = payload_val_opt {
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
                        let img_opt = config.image_obj.lock().clone();
                        if let Some(img) = img_opt {
                            marquee_engine.render(matrix.as_mut(), &img);
                        }
                        if rotation_state.mode_start_time.elapsed()
                            >= std::time::Duration::from_secs(30)
                        {
                            rotation_state.next_mode(&idle_list);
                        }
                    }
                    _ => {
                        clock_engine.render(matrix.as_mut(), &config);
                    }
                }
                last_index = current_index;

                // Composite fighter overlay (strictly disabled during GIF rotation)
                let settings = config.settings.read();
                let fighter_enabled =
                    settings.idle_fighter_enabled && current_mode.as_str() != "gifs";
                if fighter_enabled {
                    fighter_engine.set_interval(settings.idle_fighter_interval);
                    fighter_engine.composite(matrix.as_mut());
                } else if fighter_engine.is_active() {
                    fighter_engine.stop();
                    matrix.clear();
                    should_update = true;
                }
                if should_update {
                    matrix.update();
                }
            } else {
                clock_engine.render(matrix.as_mut(), &config);
                let settings = config.settings.read();
                let fighter_enabled = settings.idle_fighter_enabled;
                if fighter_enabled {
                    fighter_engine.set_interval(settings.idle_fighter_interval);
                    fighter_engine.composite(matrix.as_mut());
                } else if fighter_engine.is_active() {
                    fighter_engine.stop();
                    matrix.clear();
                }
                matrix.update();
            }

            // Adaptive sleep: match Python timing exactly
            // - Fast/animated themes: 33ms (~30fps)
            // - Static themes (clock, date, weather): 1000ms like Python (time.sleep(1))
            // This gives the CPU and Wi-Fi IRQs plenty of breathing room.
            let current_mode_str = idle_list
                .get(rotation_state.current_index % idle_list.len().max(1))
                .map(|s| s.as_str())
                .unwrap_or("");
            let is_animated = matches!(current_mode_str, "gifs" | "network" | "message");

            let theme = if current_mode_str == "date" {
                config.settings.read().date_theme
            } else {
                config.settings.read().time_theme
            };

            let fast_theme = matches!(theme, 18 | 19 | 21 | 22 | 23 | 26 | 27 | 28 | 29);
            let sprite_active = config.settings.read().idle_fighter_enabled;

            let sleep_ms = if is_animated || fast_theme || sprite_active {
                40 // ~25fps, same as Python fast mode (time.sleep(0.04))
            } else {
                1000 // 1s for static modes, exactly like Python (time.sleep(1))
            };
            last_frame = std::time::Instant::now();
            std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
        }

        matrix.clear();
        matrix.update();
        tracing::info!("Render loop exited cleanly.");
    }
}

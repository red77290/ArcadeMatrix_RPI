use crate::core::config::Config;
use rumqttc::{Client, Event, MqttOptions, Packet, QoS};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

static MQTT_REQUEST_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn start_mqtt_client(config: Arc<Config>) {
    let (enabled, broker, port, user, pass) = {
        let s = config.settings.read();
        (
            s.mqtt_enabled,
            s.mqtt_broker.clone(),
            s.mqtt_port,
            s.mqtt_user.clone(),
            s.mqtt_pass.clone(),
        )
    };

    if !enabled {
        return;
    }

    std::thread::spawn(move || {
        // Instantiate the cache ONCE for the entire MQTT loop so that the negative_cache
        // (games that don't exist on Pixelcade) is preserved across multiple events!
        let dmd_cache = Arc::new(crate::core::dmd_cache::DmdCache::new("data/marquees"));

        loop {
            let mut mqttoptions = MqttOptions::new("arcadematrix_rpi", &broker, port);
            mqttoptions.set_keep_alive(Duration::from_secs(60));

            if !user.is_empty() {
                mqttoptions.set_credentials(&user, &pass);
            }

            let (client, mut connection) = Client::new(mqttoptions, 10);

            if let Err(e) = client.subscribe("recalbox/system/playing", QoS::AtMostOnce) {
                error!("Failed to subscribe to Recalbox MQTT topic: {}", e);
                std::thread::sleep(Duration::from_secs(5));
                continue;
            }

            for notification in connection.iter() {
                match notification {
                    Ok(Event::Incoming(Packet::Publish(publish))) => {
                        if let Ok(payload) = String::from_utf8(publish.payload.to_vec()) {
                            info!("MQTT Recalbox game payload: {}", payload);
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&payload) {
                                let status = json["status"].as_str().unwrap_or("stopped");
                                if status != "stopped" {
                                    let game =
                                        json["game"].as_str().unwrap_or("Unknown").to_string();
                                    let system =
                                        json["system"].as_str().unwrap_or("Unknown").to_string();

                                    // Format text like in Python: .replace("-", " ").replace("_", " ").to_title_case()
                                    // For simplicity we just replace and uppercase the first letter of each word (Title Case)
                                    let clean_name = game.replace("-", " ").replace("_", " ");
                                    let clean_name: String = clean_name
                                        .split_whitespace()
                                        .map(|word| {
                                            let mut c = word.chars();
                                            match c.next() {
                                                None => String::new(),
                                                Some(f) => {
                                                    f.to_uppercase().collect::<String>()
                                                        + c.as_str()
                                                }
                                            }
                                        })
                                        .collect::<Vec<_>>()
                                        .join(" ");

                                    // Check if we already have the image cached (instant display)
                                    let req_id = MQTT_REQUEST_ID
                                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                                        + 1;

                                    if let Some(path) = dmd_cache.get_cached_path(&system, &game) {
                                        if let Ok(img) = image::open(&path) {
                                            *config.image_obj.lock() = Some(img.to_rgb8());
                                            *config.force_engine.lock() =
                                                Some("marquee".to_string());
                                            continue;
                                        }
                                    }

                                    // 1. Instantly show fallback message (no scroll, instant feedback)
                                    let mut text_to_show = clean_name.clone();
                                    if text_to_show.len() > 10 {
                                        text_to_show = format!(" {} ", text_to_show);
                                    }
                                    let msg_payload = crate::engines::message::MessagePayload::new(
                                        text_to_show,
                                        "#00ffff", // Cyan in Python: 0x07FF
                                        1,
                                        if clean_name.len() > 8 { "left" } else { "none" }, // Scroll only if very long, as in python
                                        30, // 30 seconds should be enough for a fallback text if download fails
                                    );
                                    *config.message_payload.lock() =
                                        Some(serde_json::to_value(msg_payload).unwrap());
                                    *config.force_engine.lock() = Some("message".to_string());

                                    let config_clone = Arc::clone(&config);
                                    let cache_clone = Arc::clone(&dmd_cache);
                                    std::thread::spawn(move || {
                                        if let Some(path) =
                                            cache_clone.download_marquee(&system, &game)
                                        {
                                            if let Ok(img) = image::open(&path) {
                                                // Only apply if the user hasn't scrolled to another game since
                                                if MQTT_REQUEST_ID
                                                    .load(std::sync::atomic::Ordering::Relaxed)
                                                    == req_id
                                                {
                                                    *config_clone.image_obj.lock() =
                                                        Some(img.to_rgb8());
                                                    *config_clone.force_engine.lock() =
                                                        Some("marquee".to_string());
                                                }
                                            }
                                        }
                                    });
                                } else if status == "stopped" {
                                    *config.force_engine.lock() = None;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("MQTT connection error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }

            std::thread::sleep(Duration::from_secs(5));
        }
    });
}

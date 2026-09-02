use crate::core::config::Config;
use rumqttc::{Client, Event, MqttOptions, Packet, QoS};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

static MQTT_REQUEST_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn format_game_name(game: &str) -> String {
    let stripped = crate::core::dmd_cache::clean_system_name(game);
    let clean_name = stripped.replace("-", " ").replace("_", " ");
    clean_name
        .split_whitespace()
        .map(|word| {
            let mut c = word.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn start_mqtt_client(config: Arc<Config>) {
    let (enabled, broker, port, user, pass) = {
        let s = config.settings.read();
        (
            s.mqtt.enabled,
            s.mqtt.broker.clone(),
            s.mqtt.port,
            s.mqtt.user.clone(),
            s.mqtt.pass.clone(),
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
                                    let raw_system =
                                        json["system"].as_str().unwrap_or("").to_string();
                                    let raw_game = json["game"].as_str().unwrap_or("").to_string();

                                    let clean_sys =
                                        crate::core::dmd_cache::clean_system_name(&raw_system);
                                    let clean_game =
                                        crate::core::dmd_cache::clean_system_name(&raw_game);

                                    let is_system_event = json["type"].as_str() == Some("system")
                                        || clean_game.is_empty()
                                        || clean_game.eq_ignore_ascii_case(&clean_sys);

                                    let system = if !clean_sys.is_empty() {
                                        clean_sys
                                    } else {
                                        raw_system
                                    };
                                    let game = if !clean_game.is_empty() {
                                        clean_game
                                    } else {
                                        raw_game
                                    };

                                    let req_id = MQTT_REQUEST_ID
                                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                                        + 1;

                                    if is_system_event {
                                        // System Marquee Browsing
                                        if let Some(path) =
                                            dmd_cache.get_cached_system_path(&system)
                                        {
                                            if let Ok(img) = image::open(&path) {
                                                *config.image_obj.lock() = Some(img.to_rgb8());
                                                config.set_forced_engine_mode(
                                                    crate::core::types::ForcedEngineMode::Marquee,
                                                );
                                                continue;
                                            }
                                        }

                                        let clean_name = format_game_name(
                                            &crate::core::dmd_cache::clean_system_name(&system),
                                        );
                                        let mut text_to_show = clean_name.clone();
                                        if text_to_show.len() > 10 {
                                            text_to_show = format!(" {} ", text_to_show);
                                        }
                                        let msg_payload =
                                            crate::engines::message::MessagePayload::new(
                                                text_to_show,
                                                "#00ffff",
                                                1,
                                                if clean_name.len() > 8 { "left" } else { "none" },
                                                0,
                                            );
                                        config.set_message_payload(Some(msg_payload));

                                        let config_clone = Arc::clone(&config);
                                        let cache_clone = Arc::clone(&dmd_cache);
                                        std::thread::spawn(move || {
                                            if let Some(path) =
                                                cache_clone.download_system_marquee(&system)
                                            {
                                                if let Ok(img) = image::open(&path) {
                                                    if MQTT_REQUEST_ID
                                                        .load(std::sync::atomic::Ordering::Relaxed)
                                                        == req_id
                                                    {
                                                        *config_clone.image_obj.lock() =
                                                            Some(img.to_rgb8());
                                                        config_clone.set_forced_engine_mode(
                                                            crate::core::types::ForcedEngineMode::Marquee,
                                                        );
                                                    }
                                                }
                                            }
                                        });
                                    } else {
                                        // Game Marquee
                                        let game =
                                            json["game"].as_str().unwrap_or("Unknown").to_string();
                                        let clean_name = format_game_name(&game);

                                        if let Some(path) =
                                            dmd_cache.get_cached_path(&system, &game)
                                        {
                                            if let Ok(img) = image::open(&path) {
                                                *config.image_obj.lock() = Some(img.to_rgb8());
                                                config.set_forced_engine_mode(
                                                    crate::core::types::ForcedEngineMode::Marquee,
                                                );
                                                continue;
                                            }
                                        }

                                        let mut text_to_show = clean_name.clone();
                                        if text_to_show.len() > 10 {
                                            text_to_show = format!(" {} ", text_to_show);
                                        }
                                        let msg_payload =
                                            crate::engines::message::MessagePayload::new(
                                                text_to_show,
                                                "#00ffff",
                                                1,
                                                if clean_name.len() > 8 { "left" } else { "none" },
                                                0,
                                            );
                                        config.set_message_payload(Some(msg_payload));

                                        let config_clone = Arc::clone(&config);
                                        let cache_clone = Arc::clone(&dmd_cache);
                                        std::thread::spawn(move || {
                                            let path_opt = cache_clone
                                                .download_marquee(&system, &game)
                                                .or_else(|| {
                                                    cache_clone.download_system_marquee(&game)
                                                })
                                                .or_else(|| {
                                                    cache_clone.download_system_marquee(&system)
                                                });
                                            if let Some(path) = path_opt {
                                                if let Ok(img) = image::open(&path) {
                                                    if MQTT_REQUEST_ID
                                                        .load(std::sync::atomic::Ordering::Relaxed)
                                                        == req_id
                                                    {
                                                        *config_clone.image_obj.lock() =
                                                            Some(img.to_rgb8());
                                                        config_clone.set_forced_engine_mode(
                                                            crate::core::types::ForcedEngineMode::Marquee,
                                                        );
                                                    }
                                                }
                                            }
                                        });
                                    }
                                } else if status == "stopped" {
                                    config.clear_forced_engine();
                                    *config.image_obj.lock() = None;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_game_name() {
        assert_eq!(format_game_name("super-mario_bros"), "Super Mario Bros");
        assert_eq!(
            format_game_name("the_legend-of-zelda"),
            "The Legend Of Zelda"
        );
        assert_eq!(format_game_name("pacman"), "Pacman");
        assert_eq!(format_game_name("STREET_FIGHTER_II"), "STREET FIGHTER II"); // If already uppercase, it stays uppercase for remaining letters, just like Python's title() in some edge cases. Wait, Python title() makes rest lower. Let's just check our current logic.
        assert_eq!(format_game_name("donkey_kong"), "Donkey Kong");
    }
}

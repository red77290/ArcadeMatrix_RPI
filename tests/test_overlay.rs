use arcadematrix::core::config::{
    ConfigSettings, EngineInstance, OverlayConfig, RotationEntry, SystemConfig,
};
use arcadematrix::core::config_sanitizer::ConfigSanitizer;
use arcadematrix::core::engine_contract::Capabilities;
use arcadematrix::core::overlay_manager::OverlayManager;
use arcadematrix::core::registry::EngineRegistry;

#[test]
fn test_fighter_not_in_engine_registry() {
    let descriptors = EngineRegistry::get_all_descriptors();
    let fighter = descriptors.iter().find(|d| d.metadata.id == "fighter");
    assert!(
        fighter.is_none(),
        "Fighter must NOT be registered as an Engine in EngineRegistry"
    );
}

#[test]
fn test_capabilities_defaults_and_overrides() {
    let default_caps = Capabilities::default();
    assert!(default_caps.allows_overlay);
    assert!(default_caps.allow_rotation);

    let marquee_desc = EngineRegistry::get_descriptor("marquee");
    if let Some(desc) = marquee_desc {
        assert!(
            !desc.capabilities.allow_rotation,
            "MarqueeEngine must have allow_rotation = false"
        );
        assert!(
            !desc.capabilities.allows_overlay,
            "MarqueeEngine must have allows_overlay = false"
        );
    }
}

#[test]
fn test_canonical_overlays_schema_and_migration() {
    // 1. Canonical schema
    let json_canonical = r#"{
        "instance_id": "clock_main",
        "duration_sec": 15,
        "overlays": { "fighter": true }
    }"#;
    let mut entry: RotationEntry = serde_json::from_str(json_canonical).unwrap();
    entry.normalize();
    assert_eq!(entry.instance_id, "clock_main");
    assert_eq!(entry.duration_sec, 15);
    assert!(entry.overlays.fighter);

    // 2. Legacy schema with top-level fighter_overlay
    let json_legacy = r#"{
        "instance_id": "weather_main",
        "duration_sec": 10,
        "fighter_overlay": true
    }"#;
    let mut entry_legacy: RotationEntry = serde_json::from_str(json_legacy).unwrap();
    assert!(!entry_legacy.overlays.fighter); // Not yet normalized
    entry_legacy.normalize();
    assert!(entry_legacy.overlays.fighter); // Normalized

    // 3. Serialization outputs canonical overlays object
    let serialized = serde_json::to_string(&entry_legacy).unwrap();
    assert!(serialized.contains(r#""overlays":{"fighter":true}"#));
    assert!(!serialized.contains("fighter_overlay"));
}

#[test]
fn test_overlay_manager_3_tier_hierarchy() {
    let mut om = OverlayManager::new(64, 32);

    let mut system_config = SystemConfig::default();
    system_config.idle_fighter_enabled = true;
    system_config.idle_fighter_interval = 10;

    // Level 3: Per-slot toggle ON
    let overlay_on = OverlayConfig { fighter: true };
    om.configure(&overlay_on, &system_config);
    // Fighter should be configured and primed
    // (Note: is_active() becomes true when animating frames)

    // Level 3: Per-slot toggle OFF (user disabled on this specific rotation card)
    let overlay_off = OverlayConfig { fighter: false };
    om.configure(&overlay_off, &system_config);
    assert!(!om.is_active());

    // Level 2: Master switch OFF (user disabled globally)
    system_config.idle_fighter_enabled = false;
    om.configure(&overlay_on, &system_config);
    assert!(!om.is_active());

    // Level 1: Engine does not allow overlay (empty overlay config passed)
    system_config.idle_fighter_enabled = true;
    om.configure(&OverlayConfig::default(), &system_config);
    assert!(!om.is_active());
}

#[test]
fn test_sanitizer_purges_fighter_instance_and_non_rotatable() {
    let mut config = ConfigSettings::default();

    // Valid clock instance
    config.instances.push(EngineInstance {
        instance_id: "clock_1".to_string(),
        engine_id: "clock".to_string(),
        config: std::collections::HashMap::new(),
    });

    // Invalid fighter instance (Fighter is an overlay, not a rotatable engine)
    config.instances.push(EngineInstance {
        instance_id: "fighter_1".to_string(),
        engine_id: "fighter".to_string(),
        config: std::collections::HashMap::new(),
    });

    // Rotation entries
    config.rotation.push(RotationEntry {
        instance_id: "clock_1".to_string(),
        duration_sec: 15,
        overlays: OverlayConfig { fighter: true },
        fighter_overlay: None,
    });
    config.rotation.push(RotationEntry {
        instance_id: "fighter_1".to_string(),
        duration_sec: 15,
        overlays: OverlayConfig { fighter: false },
        fighter_overlay: None,
    });

    let res = ConfigSanitizer::sanitize_instances(&mut config);
    assert!(res.modified);
    assert_eq!(res.invalid_instances, 1);

    // Ensure fighter instance is removed
    assert_eq!(config.instances.len(), 1);
    assert_eq!(config.instances[0].instance_id, "clock_1");

    // Ensure rotation referencing fighter_1 was purged
    assert_eq!(config.rotation.len(), 1);
    assert_eq!(config.rotation[0].instance_id, "clock_1");
}

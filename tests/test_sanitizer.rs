use arcadematrix::core::config::{ConfigSettings, EngineInstance};
use arcadematrix::core::config_sanitizer::ConfigSanitizer;
use arcadematrix::core::engine_contract::*;
use arcadematrix::core::registry::ENGINES;
use linkme::distributed_slice;
use std::collections::HashMap;

struct SanitizerMockEngine;

impl Engine for SanitizerMockEngine {
    fn initialize(
        &mut self,
        _c: &mut EngineContext,
        _cfg: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        Ok(())
    }
    fn activate(&mut self) {}
    fn update(&mut self, _c: &mut EngineContext) {}
    fn render(&mut self, _c: &mut EngineContext) {}
    fn deactivate(&mut self) {}
}

// A synthetic engine exercising every ConfigType so the self-healing sanitizer
// can be validated end to end (no real engine uses Float/Boolean/Options yet).
#[distributed_slice(ENGINES)]
fn register_sanitizer_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "test.sanitizer",
            name: "Sanitizer Test Engine",
            category: "test",
            version: "1.0",
        },
        capabilities: Capabilities::default(),
        requirements: Requirements::default(),
        available: true,
        unavailable_reason: None,
        schema: ConfigSchema {
            fields: vec![
                ConfigField {
                    id: "count",
                    field_type: ConfigType::Integer,
                    default_value: "5",
                    min_val: Some("0"),
                    max_val: Some("10"),
                    validation_policy: ValidationPolicy::Clamp,
                    ..Default::default()
                },
                ConfigField {
                    id: "ratio",
                    field_type: ConfigType::Float,
                    default_value: "0.5",
                    min_val: Some("0.0"),
                    max_val: Some("1.0"),
                    validation_policy: ValidationPolicy::Clamp,
                    ..Default::default()
                },
                ConfigField {
                    id: "enabled",
                    field_type: ConfigType::Boolean,
                    default_value: "false",
                    ..Default::default()
                },
                ConfigField {
                    id: "mode",
                    field_type: ConfigType::Options,
                    default_value: "a",
                    options: Some(vec![
                        ConfigOption {
                            label: "A",
                            value: "a",
                        },
                        ConfigOption {
                            label: "B",
                            value: "b",
                        },
                    ]),
                    validation_policy: ValidationPolicy::FallbackDefault,
                    ..Default::default()
                },
            ],
        },
        factory: || Box::new(SanitizerMockEngine),
    }
}

fn settings_with(config: HashMap<String, String>) -> ConfigSettings {
    ConfigSettings {
        instances: vec![EngineInstance {
            instance_id: "test_inst".to_string(),
            engine_id: "test.sanitizer".to_string(),
            config,
        }],
        ..Default::default()
    }
}

#[test]
fn clamps_integer_and_float_out_of_range() {
    let mut cfg = HashMap::new();
    cfg.insert("count".to_string(), "999".to_string());
    cfg.insert("ratio".to_string(), "-2.0".to_string());
    let mut s = settings_with(cfg);

    let res = ConfigSanitizer::sanitize_instances(&mut s);
    let c = &s.instances[0].config;

    assert_eq!(c.get("count").unwrap(), "10", "integer clamped to max");
    assert_eq!(c.get("ratio").unwrap(), "0", "float clamped to min");
    assert!(res.values_clamped >= 2);
    assert!(res.modified);
}

#[test]
fn normalizes_boolean_values() {
    for (input, expected) in [
        ("on", "true"),
        ("1", "true"),
        ("no", "false"),
        ("OFF", "false"),
    ] {
        let mut cfg = HashMap::new();
        cfg.insert("enabled".to_string(), input.to_string());
        let mut s = settings_with(cfg);
        ConfigSanitizer::sanitize_instances(&mut s);
        assert_eq!(
            s.instances[0].config.get("enabled").unwrap(),
            expected,
            "boolean '{}' should normalize to '{}'",
            input,
            expected
        );
    }
}

#[test]
fn falls_back_invalid_boolean_and_option() {
    let mut cfg = HashMap::new();
    cfg.insert("enabled".to_string(), "maybe".to_string());
    cfg.insert("mode".to_string(), "zzz".to_string());
    let mut s = settings_with(cfg);

    let res = ConfigSanitizer::sanitize_instances(&mut s);
    let c = &s.instances[0].config;

    assert_eq!(c.get("enabled").unwrap(), "false", "bad bool -> default");
    assert_eq!(c.get("mode").unwrap(), "a", "bad option -> default");
    assert!(res.values_fallback >= 2);
}

#[test]
fn injects_missing_defaults() {
    let s0 = HashMap::new();
    let mut s = settings_with(s0);

    let res = ConfigSanitizer::sanitize_instances(&mut s);
    let c = &s.instances[0].config;

    assert_eq!(c.get("count").unwrap(), "5");
    assert_eq!(c.get("ratio").unwrap(), "0.5");
    assert_eq!(c.get("enabled").unwrap(), "false");
    assert_eq!(c.get("mode").unwrap(), "a");
    assert_eq!(res.defaults_injected, 4);
}

#[test]
fn prunes_keys_not_in_schema() {
    let mut cfg = HashMap::new();
    cfg.insert("count".to_string(), "5".to_string());
    cfg.insert("ratio".to_string(), "0.5".to_string());
    cfg.insert("enabled".to_string(), "false".to_string());
    cfg.insert("mode".to_string(), "a".to_string());
    cfg.insert("obsolete_key".to_string(), "stale".to_string());
    let mut s = settings_with(cfg);

    let res = ConfigSanitizer::sanitize_instances(&mut s);
    let c = &s.instances[0].config;

    assert!(!c.contains_key("obsolete_key"), "obsolete key pruned");
    assert_eq!(res.keys_pruned, 1);
}

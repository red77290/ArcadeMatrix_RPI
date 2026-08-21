use crate::core::config::ConfigSettings;
use crate::core::engine_contract::{ConfigType, ValidationPolicy};
use tracing::{info, warn};

pub struct SanitizeResult {
    pub modified: bool,
    pub defaults_injected: u16,
    pub values_clamped: u16,
    pub values_fallback: u16,
    pub invalid_instances: u16,
}

pub struct ConfigSanitizer;

impl ConfigSanitizer {
    pub fn sanitize_instances(config: &mut ConfigSettings) -> SanitizeResult {
        let mut result = SanitizeResult {
            modified: false,
            defaults_injected: 0,
            values_clamped: 0,
            values_fallback: 0,
            invalid_instances: 0,
        };

        let registry_engines = crate::core::registry::ENGINES;

        for inst in &mut config.instances {
            // Find engine schema
            let engine_id = inst.engine_id.as_str();
            let mut schema_opt = None;
            for desc_fn in registry_engines {
                let desc = desc_fn();
                if desc.metadata.id == engine_id {
                    schema_opt = Some(desc.schema);
                    break;
                }
            }

            let schema = match schema_opt {
                Some(s) => s,
                None => {
                    warn!(
                        "Sanitizer: Unknown engine_id '{}' for instance '{}'",
                        engine_id, inst.instance_id
                    );
                    result.invalid_instances += 1;
                    continue;
                }
            };

            for field in schema.fields {
                let key = field.id.to_string();
                if !inst.config.contains_key(&key) {
                    // Missing: inject default
                    inst.config
                        .insert(key.clone(), field.default_value.to_string());
                    result.defaults_injected += 1;
                    result.modified = true;
                    continue;
                }

                let val_str = inst.config.get(&key).unwrap().clone();
                let mut invalid = false;

                // Validate based on type and min/max
                match field.field_type {
                    ConfigType::Integer => {
                        if let Ok(val) = val_str.parse::<i32>() {
                            let mut new_val = val;
                            if let Some(min_str) = field.min_val {
                                if let Ok(min) = min_str.parse::<i32>() {
                                    if new_val < min {
                                        new_val = min;
                                        invalid = true;
                                    }
                                }
                            }
                            if let Some(max_str) = field.max_val {
                                if let Ok(max) = max_str.parse::<i32>() {
                                    if new_val > max {
                                        new_val = max;
                                        invalid = true;
                                    }
                                }
                            }

                            if invalid {
                                match field.validation_policy {
                                    ValidationPolicy::Clamp => {
                                        inst.config.insert(key.clone(), new_val.to_string());
                                        result.values_clamped += 1;
                                        result.modified = true;
                                    }
                                    ValidationPolicy::FallbackDefault => {
                                        inst.config
                                            .insert(key.clone(), field.default_value.to_string());
                                        result.values_fallback += 1;
                                        result.modified = true;
                                    }
                                    ValidationPolicy::Reject | ValidationPolicy::Accept => {
                                        // Usually shouldn't accept out of bounds if reject is set
                                    }
                                }
                            }
                        } else {
                            // Invalid parsing
                            if field.validation_policy == ValidationPolicy::FallbackDefault {
                                inst.config
                                    .insert(key.clone(), field.default_value.to_string());
                                result.values_fallback += 1;
                                result.modified = true;
                            }
                        }
                    }
                    _ => {
                        // For String, Options, Boolean, check Options if needed
                    }
                }
            }
        }

        if result.modified {
            info!("[CONFIG] Sanitization completed: {} defaults injected, {} clamped, {} fallbacks, {} invalid instances.",
                  result.defaults_injected, result.values_clamped, result.values_fallback, result.invalid_instances);
        }

        result
    }
}

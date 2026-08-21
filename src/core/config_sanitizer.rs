use crate::core::config::ConfigSettings;
use crate::core::engine_contract::{ConfigType, ValidationPolicy};
use tracing::{info, warn};

pub struct SanitizeResult {
    pub modified: bool,
    pub defaults_injected: u16,
    pub values_clamped: u16,
    pub values_fallback: u16,
    pub invalid_instances: u16,
    pub keys_pruned: u16,
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
            keys_pruned: 0,
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

            let valid_keys: std::collections::HashSet<String> =
                schema.fields.iter().map(|f| f.id.to_string()).collect();

            for field in &schema.fields {
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

                // Validate based on the declared field type.
                match &field.field_type {
                    ConfigType::Integer => {
                        if let Ok(val) = val_str.parse::<i32>() {
                            let mut new_val = val;
                            let mut invalid = false;
                            if let Some(min) = field.min_val.and_then(|s| s.parse::<i32>().ok()) {
                                if new_val < min {
                                    new_val = min;
                                    invalid = true;
                                }
                            }
                            if let Some(max) = field.max_val.and_then(|s| s.parse::<i32>().ok()) {
                                if new_val > max {
                                    new_val = max;
                                    invalid = true;
                                }
                            }
                            if invalid {
                                Self::apply_bounds_policy(
                                    field,
                                    &key,
                                    new_val.to_string(),
                                    inst,
                                    &mut result,
                                );
                            }
                        } else {
                            Self::apply_parse_fallback(field, &key, inst, &mut result);
                        }
                    }
                    ConfigType::Float => {
                        if let Ok(val) = val_str.parse::<f64>() {
                            let mut new_val = val;
                            let mut invalid = false;
                            if let Some(min) = field.min_val.and_then(|s| s.parse::<f64>().ok()) {
                                if new_val < min {
                                    new_val = min;
                                    invalid = true;
                                }
                            }
                            if let Some(max) = field.max_val.and_then(|s| s.parse::<f64>().ok()) {
                                if new_val > max {
                                    new_val = max;
                                    invalid = true;
                                }
                            }
                            if invalid {
                                Self::apply_bounds_policy(
                                    field,
                                    &key,
                                    new_val.to_string(),
                                    inst,
                                    &mut result,
                                );
                            }
                        } else {
                            Self::apply_parse_fallback(field, &key, inst, &mut result);
                        }
                    }
                    ConfigType::Boolean => match val_str.trim().to_ascii_lowercase().as_str() {
                        "true" | "1" | "yes" | "on" => {
                            if val_str != "true" {
                                inst.config.insert(key.clone(), "true".to_string());
                                result.modified = true;
                            }
                        }
                        "false" | "0" | "no" | "off" => {
                            if val_str != "false" {
                                inst.config.insert(key.clone(), "false".to_string());
                                result.modified = true;
                            }
                        }
                        _ => {
                            inst.config
                                .insert(key.clone(), field.default_value.to_string());
                            result.values_fallback += 1;
                            result.modified = true;
                        }
                    },
                    ConfigType::Options => {
                        // Only statically-declared options can be validated. Values
                        // fetched from a dynamic `options_endpoint` are left as-is.
                        if let Some(opts) = &field.options {
                            let allowed: std::collections::HashSet<&str> =
                                opts.iter().map(|o| o.value).collect();
                            let ok = if field.multiple {
                                // Multi-select is stored as a comma-separated list.
                                val_str
                                    .split(',')
                                    .map(|s| s.trim())
                                    .filter(|s| !s.is_empty())
                                    .all(|s| allowed.contains(s))
                            } else {
                                allowed.contains(val_str.as_str())
                            };
                            if !ok {
                                inst.config
                                    .insert(key.clone(), field.default_value.to_string());
                                result.values_fallback += 1;
                                result.modified = true;
                            }
                        }
                    }
                    ConfigType::String => {}
                }
            }

            // Prune keys that are no longer part of the engine schema (e.g. after an
            // OTA that removed or renamed a field). Keeps config.json aligned with the
            // current descriptor instead of carrying stale/obsolete values forever.
            let before = inst.config.len();
            inst.config.retain(|k, _| valid_keys.contains(k));
            let pruned = before - inst.config.len();
            if pruned > 0 {
                result.keys_pruned += pruned as u16;
                result.modified = true;
            }
        }

        if result.modified {
            info!("[CONFIG] Sanitization completed: {} defaults injected, {} clamped, {} fallbacks, {} pruned, {} invalid instances.",
                  result.defaults_injected, result.values_clamped, result.values_fallback, result.keys_pruned, result.invalid_instances);
        }

        result
    }

    /// Apply the field's out-of-bounds policy (clamp to `bounded`, fall back to the
    /// declared default, or leave untouched for Reject/Accept).
    fn apply_bounds_policy(
        field: &crate::core::engine_contract::ConfigField,
        key: &str,
        bounded: String,
        inst: &mut crate::core::config::EngineInstance,
        result: &mut SanitizeResult,
    ) {
        match field.validation_policy {
            ValidationPolicy::Clamp => {
                inst.config.insert(key.to_string(), bounded);
                result.values_clamped += 1;
                result.modified = true;
            }
            ValidationPolicy::FallbackDefault => {
                inst.config
                    .insert(key.to_string(), field.default_value.to_string());
                result.values_fallback += 1;
                result.modified = true;
            }
            ValidationPolicy::Reject | ValidationPolicy::Accept => {}
        }
    }

    /// When a numeric value cannot be parsed at all, only a FallbackDefault policy
    /// can recover it (clamping is meaningless on a non-number).
    fn apply_parse_fallback(
        field: &crate::core::engine_contract::ConfigField,
        key: &str,
        inst: &mut crate::core::config::EngineInstance,
        result: &mut SanitizeResult,
    ) {
        if field.validation_policy == ValidationPolicy::FallbackDefault {
            inst.config
                .insert(key.to_string(), field.default_value.to_string());
            result.values_fallback += 1;
            result.modified = true;
        }
    }
}

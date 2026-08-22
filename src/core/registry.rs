use crate::core::engine_contract::EngineDescriptor;
use linkme::distributed_slice;

#[distributed_slice]
pub static ENGINES: [fn() -> EngineDescriptor];

pub struct EngineRegistry;

impl EngineRegistry {
    pub fn get_all_descriptors() -> Vec<EngineDescriptor> {
        let mut descriptors = Vec::new();
        for engine_fn in ENGINES {
            descriptors.push(engine_fn());
        }
        descriptors
    }

    pub fn get_descriptor(id: &str) -> Option<EngineDescriptor> {
        for engine_fn in ENGINES {
            let desc = engine_fn();
            if desc.metadata.id == id {
                return Some(desc);
            }
        }
        None
    }
}

use crate::core::engine_contract::Engine;
use std::collections::HashMap;

pub struct EngineRuntime {
    instances: HashMap<String, Box<dyn Engine>>,
    // Snapshot of the config used to (re)configure each instance, so we can
    // detect live edits coming from the API and notify the engine via
    // `on_config_changed()` without recreating it (Lazy-Once, S11.2 / S13).
    configs: HashMap<String, HashMap<String, String>>,
}

use crate::core::engine_contract::{EngineContext, HashConfig};

impl EngineRuntime {
    pub fn new() -> Self {
        Self {
            instances: HashMap::new(),
            configs: HashMap::new(),
        }
    }

    pub fn get_instance(
        &mut self,
        instance_id: &str,
        engine_id: &str,
        context: &mut EngineContext,
        config_map: &HashMap<String, String>,
    ) -> Option<&mut Box<dyn Engine>> {
        let cfg = HashConfig { data: config_map };
        if !self.instances.contains_key(instance_id) {
            // First use: create + initialize exactly once, then cache.
            if let Some(desc) = EngineRegistry::get_descriptor(engine_id) {
                let mut engine = (desc.factory)();
                let _ = engine.initialize(context, &cfg);
                self.instances.insert(instance_id.to_string(), engine);
                self.configs
                    .insert(instance_id.to_string(), config_map.clone());
            } else {
                return None;
            }
        } else {
            // Already alive: if the config changed since last time, hot-reload it
            // in place instead of destroying/recreating the instance.
            let changed = self
                .configs
                .get(instance_id)
                .map(|prev| prev != config_map)
                .unwrap_or(true);
            if changed {
                if let Some(engine) = self.instances.get_mut(instance_id) {
                    engine.on_config_changed(&cfg);
                }
                self.configs
                    .insert(instance_id.to_string(), config_map.clone());
            }
        }
        self.instances.get_mut(instance_id)
    }
}

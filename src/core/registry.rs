use crate::core::engine_contract::{EngineDescriptor, EngineMetadata};
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

use std::collections::HashMap;
use crate::core::engine_contract::Engine;

pub struct EngineRuntime {
    instances: HashMap<String, Box<dyn Engine>>,
}

use crate::core::engine_contract::{EngineConfig, EngineContext};

impl EngineRuntime {
    pub fn new() -> Self {
        Self {
            instances: HashMap::new(),
        }
    }

    pub fn get_instance(
        &mut self,
        instance_id: &str,
        engine_id: &str,
        context: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Option<&mut Box<dyn Engine>> {
        if !self.instances.contains_key(instance_id) {
            if let Some(desc) = EngineRegistry::get_descriptor(engine_id) {
                let mut engine = (desc.factory)();
                let _ = engine.initialize(context, config);
                self.instances.insert(instance_id.to_string(), engine);
            } else {
                return None;
            }
        }
        self.instances.get_mut(instance_id)
    }
}

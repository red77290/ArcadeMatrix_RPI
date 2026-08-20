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

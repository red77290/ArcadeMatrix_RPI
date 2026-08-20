use arcadematrix::core::engine_contract::*;
use arcadematrix::core::registry::{EngineRegistry, ENGINES};
use linkme::distributed_slice;

struct MockEngine;

impl Engine for MockEngine {
    fn initialize(
        &mut self,
        _context: &mut EngineContext,
        _config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        Ok(())
    }
    fn activate(&mut self) {}
    fn update(&mut self, _context: &mut EngineContext) {}
    fn render(&mut self, _context: &mut EngineContext) {}
    fn deactivate(&mut self) {}
}

#[distributed_slice(ENGINES)]
fn register_mock_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "test.mock",
            name: "Mock Engine",
            category: "test",
            version: "1.0",
        },
        capabilities: Capabilities::default(),
        requirements: Requirements::default(),
        schema: ConfigSchema { fields: vec![] },
        factory: || Box::new(MockEngine),
    }
}

#[test]
fn test_engine_discovery() {
    let descriptors = EngineRegistry::get_all_descriptors();
    assert!(!descriptors.is_empty(), "Registry should not be empty");

    let found = descriptors.iter().find(|d| d.metadata.id == "test.mock");
    assert!(found.is_some(), "Mock engine should be discovered");
}

#[test]
fn test_get_descriptor() {
    let desc = EngineRegistry::get_descriptor("test.mock");
    assert!(desc.is_some());
    assert_eq!(desc.unwrap().metadata.name, "Mock Engine");

    let not_found = EngineRegistry::get_descriptor("non.existent");
    assert!(not_found.is_none());
}

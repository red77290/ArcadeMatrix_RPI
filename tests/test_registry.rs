use arcadematrix::core::engine_contract::*;
use arcadematrix::core::registry::{EngineRegistry, ENGINES};
use linkme::distributed_slice;


use std::sync::atomic::{AtomicUsize, Ordering};

pub static MOCK_INIT_COUNT: AtomicUsize = AtomicUsize::new(0);
pub static MOCK_ACTIVATE_COUNT: AtomicUsize = AtomicUsize::new(0);
pub static MOCK_UPDATE_COUNT: AtomicUsize = AtomicUsize::new(0);
pub static MOCK_RENDER_COUNT: AtomicUsize = AtomicUsize::new(0);
pub static MOCK_DEACTIVATE_COUNT: AtomicUsize = AtomicUsize::new(0);

struct MockEngine;

impl Engine for MockEngine {
    fn initialize(
        &mut self,
        _context: &mut EngineContext,
        _config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        MOCK_INIT_COUNT.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn activate(&mut self) {
        MOCK_ACTIVATE_COUNT.fetch_add(1, Ordering::SeqCst);
    }
    fn update(&mut self, _context: &mut EngineContext) {
        MOCK_UPDATE_COUNT.fetch_add(1, Ordering::SeqCst);
    }
    fn render(&mut self, _context: &mut EngineContext) {
        MOCK_RENDER_COUNT.fetch_add(1, Ordering::SeqCst);
    }
    fn deactivate(&mut self) {
        MOCK_DEACTIVATE_COUNT.fetch_add(1, Ordering::SeqCst);
    }
    fn on_config_changed(&mut self, _config: &dyn EngineConfig) {}
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
        schema: ConfigSchema {
            fields: vec![ConfigField {
                id: "theme",
                field_type: ConfigType::Options,
                label: "Clock Theme",
                description: "Select the visual theme",
                default_value: "0",
                options: Some(vec![
                    ConfigOption {
                        label: "Arcade",
                        value: "0",
                    },
                    ConfigOption {
                        label: "Cyberpunk",
                        value: "18",
                    },
                ]),
                min_val: None,
                max_val: None,
                required: false,
                step: None,
                visible_when: None,
            }],
        },
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

use arcadematrix::core::registry::EngineRuntime;
use arcadematrix::core::matrix::MockMatrix;
use arcadematrix::core::config::Config;

#[test]
fn test_engine_runtime_lifecycle() {
    MOCK_INIT_COUNT.store(0, Ordering::SeqCst);
    MOCK_ACTIVATE_COUNT.store(0, Ordering::SeqCst);
    
    let mut runtime = EngineRuntime::new();
    let mut matrix = MockMatrix::new(64, 64);
    let config = Config::new("conf.ini");
    let mut context = EngineContext {
        matrix: &mut matrix,
        config: &config,
    };
    
    let dummy_cfg = arcadematrix::core::engine_contract::HashConfig { data: &std::collections::HashMap::new() };

    // Get instance for the first time
    let inst = runtime.get_instance("inst1", "test.mock", &mut context, &dummy_cfg);
    assert!(inst.is_some());
    assert_eq!(MOCK_INIT_COUNT.load(Ordering::SeqCst), 1);
    
    let engine = inst.unwrap();
    engine.activate();
    assert_eq!(MOCK_ACTIVATE_COUNT.load(Ordering::SeqCst), 1);
    
    engine.update(&mut context);
    assert_eq!(MOCK_UPDATE_COUNT.load(Ordering::SeqCst), 1);

    // Get instance for the second time
    let inst2 = runtime.get_instance("inst1", "test.mock", &mut context, &dummy_cfg);
    assert_eq!(MOCK_INIT_COUNT.load(Ordering::SeqCst), 1); // Should STILL be 1, Lazy-Once
}

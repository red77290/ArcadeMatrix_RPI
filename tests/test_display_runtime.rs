use arcadematrix::core::arbiter::DisplayArbiter;
use arcadematrix::core::config::Config;
use arcadematrix::core::engine_contract::{
    Capabilities, ConfigSchema, Engine, EngineConfig, EngineContext, EngineDescriptor, EngineError,
    EngineMetadata, Requirements,
};
use arcadematrix::core::matrix::MockMatrix;
use arcadematrix::core::registry::{EngineRuntime, ENGINES};
use arcadematrix::core::runtime::DisplayRuntime;
use arcadematrix::core::types::{
    DisplayGeometry, DisplayRequest, DisplaySourceId, EngineHandle, FighterOverride,
    RequestLifecycle, TransitionMode,
};
use linkme::distributed_slice;
use std::collections::HashMap;
use std::time::Instant;

// --- Mock Engines for testing FSM lifecycle ---
#[derive(Default)]
struct MockRotationEngine {
    activated: bool,
    paused: bool,
    resumed: bool,
    deactivated: bool,
}

impl Engine for MockRotationEngine {
    fn initialize(
        &mut self,
        _c: &mut EngineContext,
        _cfg: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        Ok(())
    }
    fn activate(&mut self) {
        self.activated = true;
    }
    fn pause(&mut self) {
        self.paused = true;
    }
    fn resume(&mut self) {
        self.resumed = true;
    }
    fn deactivate(&mut self) {
        self.deactivated = true;
    }
    fn update(&mut self, _c: &mut EngineContext) {}
    fn render(&mut self, _c: &mut EngineContext) {}
}

#[derive(Default)]
struct MockMqttEngine {
    activated: bool,
    deactivated: bool,
    config_changed: bool,
}

impl Engine for MockMqttEngine {
    fn initialize(
        &mut self,
        _c: &mut EngineContext,
        _cfg: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        Ok(())
    }
    fn activate(&mut self) {
        self.activated = true;
    }
    fn deactivate(&mut self) {
        self.deactivated = true;
    }
    fn on_config_changed(&mut self, _cfg: &dyn EngineConfig) {
        self.config_changed = true;
    }
    fn update(&mut self, _c: &mut EngineContext) {}
    fn render(&mut self, _c: &mut EngineContext) {}
}

#[derive(Default)]
struct MockMarqueeEngine;

impl Engine for MockMarqueeEngine {
    fn initialize(
        &mut self,
        _c: &mut EngineContext,
        _cfg: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        Ok(())
    }
    fn activate(&mut self) {}
    fn deactivate(&mut self) {}
    fn update(&mut self, _c: &mut EngineContext) {}
    fn render(&mut self, _c: &mut EngineContext) {}
}

#[distributed_slice(ENGINES)]
fn register_mock_fsm_clock() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "fsm_clock",
            name: "FSM Clock",
            category: "test",
            version: "1.0",
        },
        capabilities: Capabilities::default(),
        requirements: Requirements::default(),
        available: true,
        unavailable_reason: None,
        schema: ConfigSchema { fields: vec![] },
        factory: || Box::new(MockRotationEngine::default()),
    }
}

#[distributed_slice(ENGINES)]
fn register_mock_fsm_mqtt() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "fsm_mqtt",
            name: "FSM Mqtt",
            category: "test",
            version: "1.0",
        },
        capabilities: Capabilities::default(),
        requirements: Requirements::default(),
        available: true,
        unavailable_reason: None,
        schema: ConfigSchema { fields: vec![] },
        factory: || Box::new(MockMqttEngine::default()),
    }
}

#[distributed_slice(ENGINES)]
fn register_mock_fsm_marquee() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "fsm_marquee",
            name: "FSM Marquee",
            category: "test",
            version: "1.0",
        },
        capabilities: Capabilities::default(),
        requirements: Requirements::default(),
        available: true,
        unavailable_reason: None,
        schema: ConfigSchema { fields: vec![] },
        factory: || Box::new(MockMarqueeEngine),
    }
}

fn create_test_context<'a>(matrix: &'a mut MockMatrix, config: &'a Config) -> EngineContext<'a> {
    EngineContext { matrix, config }
}

#[test]
fn test_t01_baseline_rotation() {
    let mut arbiter = DisplayArbiter::new();
    let mut runtime = DisplayRuntime::new();
    let mut engine_runtime = EngineRuntime::new();
    let mut matrix = MockMatrix::new(64, 64);
    let config = Config::new("config.json");
    let mut ctx = create_test_context(&mut matrix, &config);
    let cfg_map = HashMap::new();

    let rot_handle = engine_runtime.register_instance_handle("rot_inst", "fsm_clock");
    let rot_req = DisplayRequest::new(
        DisplaySourceId::Rotation,
        1,
        rot_handle,
        10,
        RequestLifecycle::Persistent,
        false,
        10000,
    );
    arbiter.submit_request(rot_req);

    let decision = arbiter.evaluate(Instant::now());
    assert_eq!(decision.source_id, DisplaySourceId::Rotation);

    let transition =
        runtime.transition_session(decision, &arbiter, &mut engine_runtime, &mut ctx, &cfg_map);
    assert_eq!(transition, TransitionMode::Replace);
    assert!(runtime.is_active());
    assert_eq!(
        runtime.active_session().source_id,
        DisplaySourceId::Rotation
    );
    assert_eq!(runtime.preemption_depth(), 0);
}

#[test]
fn test_t02_rotation_to_mqtt_preempt() {
    let mut arbiter = DisplayArbiter::new();
    let mut runtime = DisplayRuntime::new();
    let mut engine_runtime = EngineRuntime::new();
    let mut matrix = MockMatrix::new(64, 64);
    let config = Config::new("config.json");
    let mut ctx = create_test_context(&mut matrix, &config);
    let cfg_map = HashMap::new();

    let rot_handle = engine_runtime.register_instance_handle("rot_inst", "fsm_clock");
    let rot_req = DisplayRequest::new(
        DisplaySourceId::Rotation,
        1,
        rot_handle,
        10,
        RequestLifecycle::Persistent,
        false,
        10000,
    );
    arbiter.submit_request(rot_req);
    let dec1 = arbiter.evaluate(Instant::now());
    runtime.transition_session(dec1, &arbiter, &mut engine_runtime, &mut ctx, &cfg_map);

    let mqtt_handle = engine_runtime.register_instance_handle("mqtt_inst", "fsm_mqtt");
    let mqtt_req = DisplayRequest::new(
        DisplaySourceId::Mqtt,
        100,
        mqtt_handle,
        40,
        RequestLifecycle::Transient,
        true,
        5000,
    );
    arbiter.submit_request(mqtt_req);

    let dec2 = arbiter.evaluate(Instant::now());
    assert_eq!(dec2.source_id, DisplaySourceId::Mqtt);

    let transition =
        runtime.transition_session(dec2, &arbiter, &mut engine_runtime, &mut ctx, &cfg_map);
    assert_eq!(transition, TransitionMode::Preempt);
    assert_eq!(runtime.active_session().source_id, DisplaySourceId::Mqtt);
    assert_eq!(runtime.preemption_depth(), 1);
}

#[test]
fn test_t03_mqtt_refresh_same_request_noop() {
    let mut arbiter = DisplayArbiter::new();
    let mut runtime = DisplayRuntime::new();
    let mut engine_runtime = EngineRuntime::new();
    let mut matrix = MockMatrix::new(64, 64);
    let config = Config::new("config.json");
    let mut ctx = create_test_context(&mut matrix, &config);
    let cfg_map = HashMap::new();

    let mqtt_handle = engine_runtime.register_instance_handle("mqtt_inst", "fsm_mqtt");
    let req = DisplayRequest::new(
        DisplaySourceId::Mqtt,
        100,
        mqtt_handle,
        40,
        RequestLifecycle::Transient,
        true,
        5000,
    );
    arbiter.submit_request(req);
    let dec = arbiter.evaluate(Instant::now());
    runtime.transition_session(dec, &arbiter, &mut engine_runtime, &mut ctx, &cfg_map);

    // Resubmit same request_id
    arbiter.submit_request(req);
    let dec2 = arbiter.evaluate(Instant::now());
    let transition =
        runtime.transition_session(dec2, &arbiter, &mut engine_runtime, &mut ctx, &cfg_map);
    assert_eq!(transition, TransitionMode::None); // NOOP
}

#[test]
fn test_t04_mqtt_new_request_id_internal_refresh() {
    let mut arbiter = DisplayArbiter::new();
    let mut runtime = DisplayRuntime::new();
    let mut engine_runtime = EngineRuntime::new();
    let mut matrix = MockMatrix::new(64, 64);
    let config = Config::new("config.json");
    let mut ctx = create_test_context(&mut matrix, &config);
    let cfg_map = HashMap::new();

    let mqtt_handle = engine_runtime.register_instance_handle("mqtt_inst", "fsm_mqtt");
    let req1 = DisplayRequest::new(
        DisplaySourceId::Mqtt,
        100,
        mqtt_handle,
        40,
        RequestLifecycle::Transient,
        true,
        5000,
    );
    arbiter.submit_request(req1);
    let dec1 = arbiter.evaluate(Instant::now());
    runtime.transition_session(dec1, &arbiter, &mut engine_runtime, &mut ctx, &cfg_map);

    // Submit new request_id for same source and handle
    let req2 = DisplayRequest::new(
        DisplaySourceId::Mqtt,
        101,
        mqtt_handle,
        40,
        RequestLifecycle::Transient,
        true,
        5000,
    );
    arbiter.submit_request(req2);
    let dec2 = arbiter.evaluate(Instant::now());
    assert_eq!(dec2.request_id, 101);

    let transition =
        runtime.transition_session(dec2, &arbiter, &mut engine_runtime, &mut ctx, &cfg_map);
    assert_eq!(transition, TransitionMode::None); // REFRESH [Internal]
    assert_eq!(runtime.active_session().request_id, 101);
    assert_eq!(runtime.preemption_depth(), 0); // No extra preemption entry pushed
}

#[test]
fn test_t05_mqtt_dominates_lower_marquee() {
    let mut arbiter = DisplayArbiter::new();
    let mqtt_handle = EngineHandle::new(10, 1);
    let marq_handle = EngineHandle::new(7, 2);

    let mqtt_req = DisplayRequest::new(
        DisplaySourceId::Mqtt,
        1,
        mqtt_handle,
        40,
        RequestLifecycle::Persistent,
        true,
        0,
    );
    let marq_req = DisplayRequest::new(
        DisplaySourceId::Marquee,
        2,
        marq_handle,
        30,
        RequestLifecycle::Persistent,
        true,
        0,
    );

    arbiter.submit_request(mqtt_req);
    arbiter.submit_request(marq_req);

    let decision = arbiter.evaluate(Instant::now());
    assert_eq!(decision.source_id, DisplaySourceId::Mqtt);
    assert_eq!(decision.priority, 40);
}

#[test]
fn test_t06_mqtt_cancel_resumes_rotation() {
    let mut arbiter = DisplayArbiter::new();
    let mut runtime = DisplayRuntime::new();
    let mut engine_runtime = EngineRuntime::new();
    let mut matrix = MockMatrix::new(64, 64);
    let config = Config::new("config.json");
    let mut ctx = create_test_context(&mut matrix, &config);
    let cfg_map = HashMap::new();

    let rot_handle = engine_runtime.register_instance_handle("rot_inst", "fsm_clock");
    let rot_req = DisplayRequest::new(
        DisplaySourceId::Rotation,
        1,
        rot_handle,
        10,
        RequestLifecycle::Persistent,
        false,
        10000,
    );
    arbiter.submit_request(rot_req);
    let dec1 = arbiter.evaluate(Instant::now());
    runtime.transition_session(dec1, &arbiter, &mut engine_runtime, &mut ctx, &cfg_map);

    let mqtt_handle = engine_runtime.register_instance_handle("mqtt_inst", "fsm_mqtt");
    let mqtt_req = DisplayRequest::new(
        DisplaySourceId::Mqtt,
        100,
        mqtt_handle,
        40,
        RequestLifecycle::Transient,
        true,
        5000,
    );
    arbiter.submit_request(mqtt_req);
    let dec2 = arbiter.evaluate(Instant::now());
    runtime.transition_session(dec2, &arbiter, &mut engine_runtime, &mut ctx, &cfg_map);

    // Cancel MQTT
    arbiter.cancel_request(DisplaySourceId::Mqtt, 100);
    let dec3 = arbiter.evaluate(Instant::now());
    assert_eq!(dec3.source_id, DisplaySourceId::Rotation);

    let transition =
        runtime.transition_session(dec3, &arbiter, &mut engine_runtime, &mut ctx, &cfg_map);
    assert_eq!(transition, TransitionMode::Resume);
    assert_eq!(
        runtime.active_session().source_id,
        DisplaySourceId::Rotation
    );
    assert_eq!(runtime.preemption_depth(), 0);
}

#[test]
fn test_t07_marquee_preempted_by_mqtt_and_resumed() {
    let mut arbiter = DisplayArbiter::new();
    let mut runtime = DisplayRuntime::new();
    let mut engine_runtime = EngineRuntime::new();
    let mut matrix = MockMatrix::new(64, 64);
    let config = Config::new("config.json");
    let mut ctx = create_test_context(&mut matrix, &config);
    let cfg_map = HashMap::new();

    let marq_handle = engine_runtime.register_instance_handle("marq_inst", "fsm_marquee");
    let marq_req = DisplayRequest::new(
        DisplaySourceId::Marquee,
        10,
        marq_handle,
        30,
        RequestLifecycle::Persistent,
        true,
        0,
    );
    arbiter.submit_request(marq_req);
    let dec1 = arbiter.evaluate(Instant::now());
    runtime.transition_session(dec1, &arbiter, &mut engine_runtime, &mut ctx, &cfg_map);

    let mqtt_handle = engine_runtime.register_instance_handle("mqtt_inst", "fsm_mqtt");
    let mqtt_req = DisplayRequest::new(
        DisplaySourceId::Mqtt,
        20,
        mqtt_handle,
        40,
        RequestLifecycle::Persistent,
        true,
        0,
    );
    arbiter.submit_request(mqtt_req);
    let dec2 = arbiter.evaluate(Instant::now());
    runtime.transition_session(dec2, &arbiter, &mut engine_runtime, &mut ctx, &cfg_map);
    assert_eq!(runtime.active_session().source_id, DisplaySourceId::Mqtt);

    arbiter.cancel_request(DisplaySourceId::Mqtt, 20);
    let dec3 = arbiter.evaluate(Instant::now());
    assert_eq!(dec3.source_id, DisplaySourceId::Marquee);

    let transition =
        runtime.transition_session(dec3, &arbiter, &mut engine_runtime, &mut ctx, &cfg_map);
    assert_eq!(transition, TransitionMode::Resume);
    assert_eq!(runtime.active_session().source_id, DisplaySourceId::Marquee);
}

#[test]
fn test_t08_multi_level_preemption_stack_depth() {
    let mut arbiter = DisplayArbiter::new();
    let mut runtime = DisplayRuntime::new();
    let mut engine_runtime = EngineRuntime::new();
    let mut matrix = MockMatrix::new(64, 64);
    let config = Config::new("config.json");
    let mut ctx = create_test_context(&mut matrix, &config);
    let cfg_map = HashMap::new();

    let h_rot = engine_runtime.register_instance_handle("i_rot", "fsm_clock");
    let h_gif = engine_runtime.register_instance_handle("i_gif", "fsm_clock");
    let h_marq = engine_runtime.register_instance_handle("i_marq", "fsm_marquee");
    let h_mqtt = engine_runtime.register_instance_handle("i_mqtt", "fsm_mqtt");

    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Rotation,
        1,
        h_rot,
        10,
        RequestLifecycle::Persistent,
        false,
        0,
    ));
    runtime.transition_session(
        arbiter.evaluate(Instant::now()),
        &arbiter,
        &mut engine_runtime,
        &mut ctx,
        &cfg_map,
    );

    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Gif,
        2,
        h_gif,
        20,
        RequestLifecycle::Persistent,
        true,
        0,
    ));
    runtime.transition_session(
        arbiter.evaluate(Instant::now()),
        &arbiter,
        &mut engine_runtime,
        &mut ctx,
        &cfg_map,
    );
    assert_eq!(runtime.preemption_depth(), 1);

    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Marquee,
        3,
        h_marq,
        30,
        RequestLifecycle::Persistent,
        true,
        0,
    ));
    runtime.transition_session(
        arbiter.evaluate(Instant::now()),
        &arbiter,
        &mut engine_runtime,
        &mut ctx,
        &cfg_map,
    );
    assert_eq!(runtime.preemption_depth(), 2);

    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Mqtt,
        4,
        h_mqtt,
        40,
        RequestLifecycle::Persistent,
        true,
        0,
    ));
    runtime.transition_session(
        arbiter.evaluate(Instant::now()),
        &arbiter,
        &mut engine_runtime,
        &mut ctx,
        &cfg_map,
    );
    assert_eq!(runtime.preemption_depth(), 3);
}

#[test]
fn test_t09_t10_stack_saturation_at_depth_4() {
    let mut runtime = DisplayRuntime::new();
    let mut engine_runtime = EngineRuntime::new();
    let mut arbiter = DisplayArbiter::new();
    let mut matrix = MockMatrix::new(64, 64);
    let config = Config::new("config.json");
    let mut ctx = create_test_context(&mut matrix, &config);
    let cfg_map = HashMap::new();

    let h1 = engine_runtime.register_instance_handle("i1", "fsm_clock");
    let h2 = engine_runtime.register_instance_handle("i2", "fsm_clock");
    let h3 = engine_runtime.register_instance_handle("i3", "fsm_marquee");
    let h4 = engine_runtime.register_instance_handle("i4", "fsm_mqtt");

    // Level 1: Rotation
    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Rotation,
        1,
        h1,
        10,
        RequestLifecycle::Persistent,
        false,
        0,
    ));
    runtime.transition_session(
        arbiter.evaluate(Instant::now()),
        &arbiter,
        &mut engine_runtime,
        &mut ctx,
        &cfg_map,
    );
    assert_eq!(runtime.preemption_depth(), 0);

    // Level 2: GIF preemption -> depth 1
    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Gif,
        2,
        h2,
        20,
        RequestLifecycle::Persistent,
        true,
        0,
    ));
    runtime.transition_session(
        arbiter.evaluate(Instant::now()),
        &arbiter,
        &mut engine_runtime,
        &mut ctx,
        &cfg_map,
    );
    assert_eq!(runtime.preemption_depth(), 1);

    // Level 3: Marquee preemption -> depth 2
    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Marquee,
        3,
        h3,
        30,
        RequestLifecycle::Persistent,
        true,
        0,
    ));
    runtime.transition_session(
        arbiter.evaluate(Instant::now()),
        &arbiter,
        &mut engine_runtime,
        &mut ctx,
        &cfg_map,
    );
    assert_eq!(runtime.preemption_depth(), 2);

    // Level 4: MQTT preemption -> depth 3
    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Mqtt,
        4,
        h4,
        40,
        RequestLifecycle::Persistent,
        true,
        0,
    ));
    runtime.transition_session(
        arbiter.evaluate(Instant::now()),
        &arbiter,
        &mut engine_runtime,
        &mut ctx,
        &cfg_map,
    );
    assert_eq!(runtime.preemption_depth(), 3);
}

#[test]
fn test_t11_t12_resilient_stack_unwinding_skips_cancelled_intermediate() {
    let mut arbiter = DisplayArbiter::new();
    let mut runtime = DisplayRuntime::new();
    let mut engine_runtime = EngineRuntime::new();
    let mut matrix = MockMatrix::new(64, 64);
    let config = Config::new("config.json");
    let mut ctx = create_test_context(&mut matrix, &config);
    let cfg_map = HashMap::new();

    let h_rot = engine_runtime.register_instance_handle("i_rot", "fsm_clock");
    let h_marq = engine_runtime.register_instance_handle("i_marq", "fsm_marquee");
    let h_mqtt = engine_runtime.register_instance_handle("i_mqtt", "fsm_mqtt");

    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Rotation,
        1,
        h_rot,
        10,
        RequestLifecycle::Persistent,
        false,
        0,
    ));
    runtime.transition_session(
        arbiter.evaluate(Instant::now()),
        &arbiter,
        &mut engine_runtime,
        &mut ctx,
        &cfg_map,
    );

    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Marquee,
        2,
        h_marq,
        30,
        RequestLifecycle::Persistent,
        true,
        0,
    ));
    runtime.transition_session(
        arbiter.evaluate(Instant::now()),
        &arbiter,
        &mut engine_runtime,
        &mut ctx,
        &cfg_map,
    );

    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Mqtt,
        3,
        h_mqtt,
        40,
        RequestLifecycle::Persistent,
        true,
        0,
    ));
    runtime.transition_session(
        arbiter.evaluate(Instant::now()),
        &arbiter,
        &mut engine_runtime,
        &mut ctx,
        &cfg_map,
    );

    // Cancel intermediate Marquee while MQTT is dominant
    arbiter.cancel_request(DisplaySourceId::Marquee, 2);

    // Cancel MQTT -> Should skip cancelled Marquee and resume Rotation directly
    arbiter.cancel_request(DisplaySourceId::Mqtt, 3);
    let dec = arbiter.evaluate(Instant::now());
    assert_eq!(dec.source_id, DisplaySourceId::Rotation);

    let transition =
        runtime.transition_session(dec, &arbiter, &mut engine_runtime, &mut ctx, &cfg_map);
    assert_eq!(transition, TransitionMode::Resume);
    assert_eq!(
        runtime.active_session().source_id,
        DisplaySourceId::Rotation
    );
    assert_eq!(runtime.preemption_depth(), 0);
}

#[test]
fn test_t13_transactional_rejection_on_invalid_engine() {
    let mut arbiter = DisplayArbiter::new();
    let mut runtime = DisplayRuntime::new();
    let mut engine_runtime = EngineRuntime::new();
    let mut matrix = MockMatrix::new(64, 64);
    let config = Config::new("config.json");
    let mut ctx = create_test_context(&mut matrix, &config);
    let cfg_map = HashMap::new();

    let h_rot = engine_runtime.register_instance_handle("i_rot", "fsm_clock");
    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Rotation,
        1,
        h_rot,
        10,
        RequestLifecycle::Persistent,
        false,
        0,
    ));
    runtime.transition_session(
        arbiter.evaluate(Instant::now()),
        &arbiter,
        &mut engine_runtime,
        &mut ctx,
        &cfg_map,
    );

    // Submit request targeting an unresolvable engine ID
    let invalid_handle = EngineHandle::new(999, 999);
    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Mqtt,
        2,
        invalid_handle,
        40,
        RequestLifecycle::Persistent,
        true,
        0,
    ));
    let dec = arbiter.evaluate(Instant::now());

    let transition =
        runtime.transition_session(dec, &arbiter, &mut engine_runtime, &mut ctx, &cfg_map);
    assert_eq!(transition, TransitionMode::None); // Rejet transactionnel
    assert_eq!(
        runtime.active_session().source_id,
        DisplaySourceId::Rotation
    ); // Active session intact
}

#[test]
fn test_t15_fighter_master_switch_truth_table() {
    let global_off = false;
    let global_on = true;

    // Truth table: should_be_active = global_enabled && (override != Disabled)
    let check =
        |global: bool, ov: FighterOverride| -> bool { global && ov != FighterOverride::Disabled };

    assert_eq!(check(global_off, FighterOverride::Unspecified), false);
    assert_eq!(check(global_off, FighterOverride::Disabled), false);
    assert_eq!(check(global_off, FighterOverride::Enabled), false); // MASTER SWITCH CANNOT BE BYPASSED!

    assert_eq!(check(global_on, FighterOverride::Unspecified), true);
    assert_eq!(check(global_on, FighterOverride::Enabled), true);
    assert_eq!(check(global_on, FighterOverride::Disabled), false);
}

#[test]
fn test_t16_geometry_classification_and_version() {
    let g1 = DisplayGeometry::new(64, 64, 0, 1);
    assert_eq!(g1.logical_width, 64);
    assert_eq!(g1.logical_height, 64);
    assert_eq!(
        g1.layout_class,
        arcadematrix::core::types::LayoutClass::Square
    );

    // Tate 90° rotation swaps logical width and height
    let g2 = DisplayGeometry::new(128, 64, 1, 2);
    assert_eq!(g2.logical_width, 64);
    assert_eq!(g2.logical_height, 128);
    assert_eq!(
        g2.layout_class,
        arcadematrix::core::types::LayoutClass::Tall
    );
}

#[test]
fn test_t23_producer_sync_state_edge_triggering() {
    let mut sync = arcadematrix::core::types::ProducerSyncState::INIT;
    let h1 = EngineHandle::new(1, 1);

    assert!(sync.has_changed(true, 1, h1));
    sync.update(true, 1, h1);

    // 1000 frames with identical state -> NO CHANGE
    for _ in 0..1000 {
        assert!(!sync.has_changed(true, 1, h1));
    }

    // New request_id -> TRIGGER
    assert!(sync.has_changed(true, 2, h1));
}

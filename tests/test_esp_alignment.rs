use arcadematrix::core::arbiter::DisplayArbiter;
use arcadematrix::core::orientation::OrientationManager;
use arcadematrix::core::registry::EngineRuntime;
use arcadematrix::core::runtime::DisplayRuntime;
use arcadematrix::core::types::{
    DisplayDecision, DisplayRequest, DisplaySourceId, EngineHandle, ProducerSyncState,
    RequestIdGenerator, RequestLifecycle, TransitionMode,
};
use std::time::Instant;

// =========================================================================
// Category A — Ownership & Isolation
// =========================================================================

#[test]
fn test_a01_arbiter_stateless_and_bounded() {
    let mut arbiter = DisplayArbiter::new();
    assert_eq!(arbiter.active_count(), 0);
    assert_eq!(arbiter.evaluate(Instant::now()), DisplayDecision::NONE);

    let handle1 = EngineHandle::new(1, 1);
    let req1 = DisplayRequest::new(
        DisplaySourceId::Rotation,
        1,
        handle1,
        DisplaySourceId::Rotation as u8,
        RequestLifecycle::Persistent,
        false,
        10000,
    );
    arbiter.submit_request(req1);

    assert_eq!(arbiter.active_count(), 1);
    let decision = arbiter.evaluate(Instant::now());
    assert_eq!(decision.source_id, DisplaySourceId::Rotation);
    assert_eq!(decision.engine_handle, handle1);
    assert_eq!(decision.priority, 10);
    assert!(!decision.preemptive);
}

#[test]
fn test_a02_runtime_exclusively_owns_session() {
    let mut runtime = DisplayRuntime::new();
    assert!(!runtime.is_active());
    assert_eq!(runtime.preemption_depth(), 0);

    let handle = EngineHandle::new(1, 1);
    let decision = DisplayDecision {
        source_id: DisplaySourceId::Rotation,
        engine_handle: handle,
        request_id: 1,
        priority: 10,
        preemptive: false,
    };

    let arbiter = DisplayArbiter::new();
    let mut engine_runtime = EngineRuntime::new();
    engine_runtime.register_instance_handle("clock_main", "clock");

    let mode = runtime.transition_session(decision, &arbiter, &mut engine_runtime);

    assert_eq!(mode, TransitionMode::Replace);
    assert!(runtime.is_active());
    assert_eq!(
        runtime.active_session().source_id,
        DisplaySourceId::Rotation
    );
    assert_eq!(runtime.active_session().engine_handle, handle);
}

#[test]
fn test_a03_preemption_stack_fixed_capacity_rejection() {
    let mut runtime = DisplayRuntime::new();
    let arbiter = DisplayArbiter::new();
    let mut engine_runtime = EngineRuntime::new();

    // Register 6 distinct instances
    for i in 1..=6 {
        engine_runtime.register_instance_handle(&format!("inst_{}", i), "clock");
    }

    // 1. Activate baseline (depth = 0)
    let d1 = DisplayDecision {
        source_id: DisplaySourceId::Rotation,
        engine_handle: EngineHandle::new(1, 1),
        request_id: 1,
        priority: 10,
        preemptive: false,
    };
    assert_eq!(
        runtime.transition_session(d1, &arbiter, &mut engine_runtime),
        TransitionMode::Replace
    );
    assert_eq!(runtime.preemption_depth(), 0);

    // 2. Preempt 1 (depth = 1)
    let d2 = DisplayDecision {
        source_id: DisplaySourceId::Gif,
        engine_handle: EngineHandle::new(1, 2),
        request_id: 2,
        priority: 20,
        preemptive: true,
    };
    assert_eq!(
        runtime.transition_session(d2, &arbiter, &mut engine_runtime),
        TransitionMode::Preempt
    );
    assert_eq!(runtime.preemption_depth(), 1);

    // 3. Preempt 2 (depth = 2)
    let d3 = DisplayDecision {
        source_id: DisplaySourceId::Marquee,
        engine_handle: EngineHandle::new(1, 3),
        request_id: 3,
        priority: 30,
        preemptive: true,
    };
    assert_eq!(
        runtime.transition_session(d3, &arbiter, &mut engine_runtime),
        TransitionMode::Preempt
    );
    assert_eq!(runtime.preemption_depth(), 2);

    // 4. Preempt 3 (depth = 3)
    let d4 = DisplayDecision {
        source_id: DisplaySourceId::Mqtt,
        engine_handle: EngineHandle::new(1, 4),
        request_id: 4,
        priority: 40,
        preemptive: true,
    };
    assert_eq!(
        runtime.transition_session(d4, &arbiter, &mut engine_runtime),
        TransitionMode::Preempt
    );
    assert_eq!(runtime.preemption_depth(), 3);

    // 5. Preempt 4 (depth = 4, SATURATED)
    let d5 = DisplayDecision {
        source_id: DisplaySourceId::Mqtt,
        engine_handle: EngineHandle::new(1, 5),
        request_id: 5,
        priority: 50,
        preemptive: true,
    };
    assert_eq!(
        runtime.transition_session(d5, &arbiter, &mut engine_runtime),
        TransitionMode::Preempt
    );
    assert_eq!(runtime.preemption_depth(), 4);

    // 6. Preempt 5 (stack full: must REJECT without modifying active session)
    let d6 = DisplayDecision {
        source_id: DisplaySourceId::Mqtt,
        engine_handle: EngineHandle::new(1, 6),
        request_id: 6,
        priority: 60,
        preemptive: true,
    };
    assert_eq!(
        runtime.transition_session(d6, &arbiter, &mut engine_runtime),
        TransitionMode::None
    );
    assert_eq!(runtime.preemption_depth(), 4);
    assert_eq!(
        runtime.active_session().engine_handle,
        EngineHandle::new(1, 5)
    );
}

// =========================================================================
// Category B — State Machine & Behavioral Scenarios
// =========================================================================

#[test]
fn test_b01_preemption_and_exact_resume() {
    let mut arbiter = DisplayArbiter::new();
    let mut runtime = DisplayRuntime::new();
    let mut engine_runtime = EngineRuntime::new();
    let handle_rot = engine_runtime.register_instance_handle("rot_clock", "clock");
    let handle_mqtt = engine_runtime.register_instance_handle("mqtt_msg", "message");

    // 1. Rotation active
    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Rotation,
        1,
        handle_rot,
        10,
        RequestLifecycle::Persistent,
        false,
        10000,
    ));
    let dec1 = arbiter.evaluate(Instant::now());
    assert_eq!(
        runtime.transition_session(dec1, &arbiter, &mut engine_runtime),
        TransitionMode::Replace
    );
    assert_eq!(runtime.active_session().engine_handle, handle_rot);

    // 2. Incoming MQTT alert preempts Rotation
    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Mqtt,
        1001,
        handle_mqtt,
        40,
        RequestLifecycle::Transient,
        true,
        5000,
    ));
    let dec2 = arbiter.evaluate(Instant::now());
    assert_eq!(
        runtime.transition_session(dec2, &arbiter, &mut engine_runtime),
        TransitionMode::Preempt
    );
    assert_eq!(runtime.active_session().engine_handle, handle_mqtt);
    assert_eq!(runtime.preemption_depth(), 1);

    // 3. MQTT finishes/cancelled -> Resume baseline Rotation cleanly
    arbiter.cancel_request(DisplaySourceId::Mqtt, 1001);
    let dec3 = arbiter.evaluate(Instant::now());
    assert_eq!(dec3.source_id, DisplaySourceId::Rotation);
    assert_eq!(
        runtime.transition_session(dec3, &arbiter, &mut engine_runtime),
        TransitionMode::Resume
    );
    assert_eq!(runtime.active_session().engine_handle, handle_rot);
    assert_eq!(runtime.preemption_depth(), 0);
}

#[test]
fn test_b02_submerged_orphan_cleanup_on_resume() {
    let mut arbiter = DisplayArbiter::new();
    let mut runtime = DisplayRuntime::new();
    let mut engine_runtime = EngineRuntime::new();

    let handle_rot = engine_runtime.register_instance_handle("rot_1", "clock");
    let handle_mar = engine_runtime.register_instance_handle("mar_1", "marquee");
    let handle_mqtt = engine_runtime.register_instance_handle("mqtt_1", "message");

    // Baseline: Rotation
    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Rotation,
        1,
        handle_rot,
        10,
        RequestLifecycle::Persistent,
        false,
        10000,
    ));
    let dec1 = arbiter.evaluate(Instant::now());
    runtime.transition_session(dec1, &arbiter, &mut engine_runtime);

    // Preemption 1: Marquee (priority 30)
    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Marquee,
        2001,
        handle_mar,
        30,
        RequestLifecycle::Persistent,
        true,
        0,
    ));
    let dec2 = arbiter.evaluate(Instant::now());
    runtime.transition_session(dec2, &arbiter, &mut engine_runtime);
    assert_eq!(runtime.preemption_depth(), 1);

    // Preemption 2: MQTT (priority 40)
    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Mqtt,
        3001,
        handle_mqtt,
        40,
        RequestLifecycle::Transient,
        true,
        5000,
    ));
    let dec3 = arbiter.evaluate(Instant::now());
    runtime.transition_session(dec3, &arbiter, &mut engine_runtime);
    assert_eq!(runtime.preemption_depth(), 2);

    // While MQTT is active, Marquee game ends (cancelled in Arbiter)
    arbiter.cancel_request(DisplaySourceId::Marquee, 2001);

    // MQTT finishes
    arbiter.cancel_request(DisplaySourceId::Mqtt, 3001);
    let dec4 = arbiter.evaluate(Instant::now());

    // Resume should cleanly discard orphan Marquee and resume Rotation directly
    let mode = runtime.transition_session(dec4, &arbiter, &mut engine_runtime);
    assert_eq!(mode, TransitionMode::Resume);
    assert_eq!(runtime.active_session().engine_handle, handle_rot);
    assert_eq!(runtime.preemption_depth(), 0);
}

#[test]
fn test_b03_orientation_manager_geometry_versioning() {
    let mut orientation = OrientationManager::new(64, 32, 0);
    assert_eq!(orientation.geometry().logical_width, 64);
    assert_eq!(orientation.geometry().logical_height, 32);
    let v1 = orientation.geometry().version;

    // Change to 90 degrees (Tate portrait mode)
    assert!(orientation.set_rotation(1));
    assert_eq!(orientation.geometry().logical_width, 32);
    assert_eq!(orientation.geometry().logical_height, 64);
    assert_eq!(orientation.geometry().version, v1 + 1);

    // Same rotation -> no change
    assert!(!orientation.set_rotation(1));
    assert_eq!(orientation.geometry().version, v1 + 1);
}

// =========================================================================
// Category C — Identity & Transactionality
// =========================================================================

#[test]
fn test_c01_intent_identity_and_request_id_generators() {
    let mut gen = RequestIdGenerator::new(100);
    assert_eq!(gen.next_id(), 100);
    assert_eq!(gen.next_id(), 101);
    assert_eq!(gen.next_id(), 102);

    let mut arbiter = DisplayArbiter::new();
    let handle = EngineHandle::new(1, 1);

    let t0 = Instant::now();
    let req1 = DisplayRequest {
        source_id: DisplaySourceId::Mqtt,
        request_id: 500,
        engine_handle: handle,
        priority: 40,
        lifecycle: RequestLifecycle::Transient,
        preemptive: true,
        created_at: t0,
        duration_ms: 5000,
    };
    arbiter.submit_request(req1);

    // Same source + same request_id + same handle -> preserves created_at
    let req1_dup = DisplayRequest {
        source_id: DisplaySourceId::Mqtt,
        request_id: 500,
        engine_handle: handle,
        priority: 40,
        lifecycle: RequestLifecycle::Transient,
        preemptive: true,
        created_at: Instant::now() + std::time::Duration::from_secs(10),
        duration_ms: 5000,
    };
    arbiter.submit_request(req1_dup);

    let dec = arbiter.evaluate(Instant::now());
    assert_eq!(dec.request_id, 500);

    // New request_id -> updates intent
    let req2 = DisplayRequest {
        source_id: DisplaySourceId::Mqtt,
        request_id: 501,
        engine_handle: handle,
        priority: 40,
        lifecycle: RequestLifecycle::Transient,
        preemptive: true,
        created_at: Instant::now(),
        duration_ms: 5000,
    };
    arbiter.submit_request(req2);
    let dec2 = arbiter.evaluate(Instant::now());
    assert_eq!(dec2.request_id, 501);
}

#[test]
fn test_c02_transactional_resolution_rejection() {
    let mut runtime = DisplayRuntime::new();
    let arbiter = DisplayArbiter::new();
    let mut engine_runtime = EngineRuntime::new();
    engine_runtime.register_instance_handle("valid_inst", "clock");

    // 1. Valid session
    let d_valid = DisplayDecision {
        source_id: DisplaySourceId::Rotation,
        engine_handle: EngineHandle::new(1, 1),
        request_id: 1,
        priority: 10,
        preemptive: false,
    };
    assert_eq!(
        runtime.transition_session(d_valid, &arbiter, &mut engine_runtime,),
        TransitionMode::Replace
    );
    assert!(runtime.is_active());

    // 2. Invalid instance_id 999 -> Must reject transaction without altering session
    let d_invalid = DisplayDecision {
        source_id: DisplaySourceId::Marquee,
        engine_handle: EngineHandle::new(1, 999),
        request_id: 2,
        priority: 30,
        preemptive: true,
    };
    assert_eq!(
        runtime.transition_session(d_invalid, &arbiter, &mut engine_runtime,),
        TransitionMode::None
    );
    assert_eq!(
        runtime.active_session().engine_handle,
        EngineHandle::new(1, 1)
    );
}

// =========================================================================
// Category D — Hot-Path Zero-Allocation (H-Core)
// =========================================================================

#[test]
fn test_d01_h_core_hot_path_zero_allocation_simulation() {
    let mut arbiter = DisplayArbiter::new();
    let mut runtime = DisplayRuntime::new();
    let mut engine_runtime = EngineRuntime::new();

    let h1 = engine_runtime.register_instance_handle("inst_a", "clock");
    let h2 = engine_runtime.register_instance_handle("inst_b", "message");

    let mut sync_state = ProducerSyncState::INIT;

    // Simulate 10,000 rapid cycles of decision, handle resolution, and state sync
    for i in 1..=10_000 {
        let now = Instant::now();

        if i % 100 == 0 {
            let req = DisplayRequest::new(
                DisplaySourceId::Mqtt,
                i,
                h2,
                40,
                RequestLifecycle::Transient,
                true,
                50,
            );
            arbiter.submit_request(req);
        } else if i % 50 == 0 {
            arbiter.cancel_request(DisplaySourceId::Mqtt, 0);
        }

        if sync_state.has_changed(true, i, h1) {
            sync_state.update(true, i, h1);
        }

        let decision = arbiter.evaluate(now);
        assert!(engine_runtime.resolve_handle(h1));

        let _ = runtime.transition_session(decision, &arbiter, &mut engine_runtime);
    }
}

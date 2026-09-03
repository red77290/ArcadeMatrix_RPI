use arcadematrix::core::arbiter::DisplayArbiter;
use arcadematrix::core::engine_contract::EngineContext;
use arcadematrix::core::matrix::MockMatrix;
use arcadematrix::core::orientation::OrientationManager;
use arcadematrix::core::registry::EngineRuntime;
use arcadematrix::core::runtime::DisplayRuntime;
use arcadematrix::core::types::{
    DisplayDecision, DisplayRequest, DisplaySourceId, EngineHandle, ProducerSyncState,
    RequestIdGenerator, RequestLifecycle, TransitionMode,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::HashMap;
use std::time::Instant;

// =========================================================================
// Thread-Local Instrumented Allocator for Mechanical Zero-Allocation Verification
// =========================================================================

thread_local! {
    static THREAD_TRACKING: Cell<bool> = const { Cell::new(false) };
    static THREAD_ALLOC_COUNT: Cell<usize> = const { Cell::new(0) };
    static THREAD_DEALLOC_COUNT: Cell<usize> = const { Cell::new(0) };
}

struct TrackingAllocator;

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = THREAD_TRACKING.try_with(|tracking| {
            if tracking.get() {
                let _ = THREAD_ALLOC_COUNT.try_with(|count| {
                    count.set(count.get() + 1);
                });
            }
        });
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let _ = THREAD_TRACKING.try_with(|tracking| {
            if tracking.get() {
                let _ = THREAD_DEALLOC_COUNT.try_with(|count| {
                    count.set(count.get() + 1);
                });
            }
        });
        System.dealloc(ptr, layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = THREAD_TRACKING.try_with(|tracking| {
            if tracking.get() {
                let _ = THREAD_ALLOC_COUNT.try_with(|count| {
                    count.set(count.get() + 1);
                });
            }
        });
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static GLOBAL_TRACKER: TrackingAllocator = TrackingAllocator;

use arcadematrix::core::engine_contract::{
    Capabilities, ConfigSchema, Engine, EngineConfig, EngineDescriptor, EngineError,
    EngineMetadata, Requirements,
};
use arcadematrix::core::registry::ENGINES;
use linkme::distributed_slice;

#[derive(Default)]
struct TestHotPathEngine;

impl Engine for TestHotPathEngine {
    fn initialize(
        &mut self,
        _c: &mut EngineContext,
        _cfg: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        Ok(())
    }
    fn activate(&mut self) {}
    fn pause(&mut self) {}
    fn resume(&mut self) {}
    fn deactivate(&mut self) {}
    fn update(&mut self, _c: &mut EngineContext) {}
    fn render(&mut self, _c: &mut EngineContext) {}
}

#[distributed_slice(ENGINES)]
fn register_test_hotpath_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "hotpath_mock",
            name: "Hotpath Mock Engine",
            category: "test",
            version: "1.0",
        },
        capabilities: Capabilities::default(),
        requirements: Requirements::default(),
        available: true,
        unavailable_reason: None,
        schema: ConfigSchema { fields: vec![] },
        factory: || Box::new(TestHotPathEngine::default()),
    }
}

// =========================================================================
// Group 1 — Mechanical Zero-Allocation Proof (H-Core Invariant)
// =========================================================================

#[test]
fn test_h_core_zero_allocation_mechanical_proof() {
    let mut arbiter = DisplayArbiter::new();
    let mut runtime = DisplayRuntime::new();
    let mut engine_runtime = EngineRuntime::new();
    let mut matrix = MockMatrix::new(64, 32);
    let config = arcadematrix::core::config::Config::new("config.json");

    // Pre-publish a message payload via Control Plane
    let initial_payload = arcadematrix::engines::message::MessagePayload::new(
        "ArcadeMatrix".to_string(),
        "#00ff00",
        1,
        "left",
        10,
    );
    config.set_message_payload(Some(initial_payload));

    let mut ctx = EngineContext {
        matrix: &mut matrix,
        config: &config,
    };
    let empty_cfg = HashMap::new();

    // 1. Cold Path: Register and initialize instances (Heap allocation permitted here)
    let h1 = engine_runtime.register_instance_handle("inst_rot", "hotpath_mock");
    let h2 = engine_runtime.register_instance_handle("inst_marq", "hotpath_mock");
    let h3 = engine_runtime.register_instance_handle("inst_mqtt", "message");

    engine_runtime.init_instance(h1, &mut ctx, &empty_cfg);
    engine_runtime.init_instance(h2, &mut ctx, &empty_cfg);
    engine_runtime.init_instance(h3, &mut ctx, &empty_cfg);

    let mut message_sync = ProducerSyncState::INIT;
    let mut marquee_sync = ProducerSyncState::INIT;
    let mut rotation_sync = ProducerSyncState::INIT;
    let mut last_message_payload: Option<
        std::sync::Arc<arcadematrix::engines::message::MessagePayload>,
    > = None;
    let mut mqtt_req_gen = RequestIdGenerator::new(1001);
    let mut marquee_req_gen = RequestIdGenerator::new(2001);

    // 2. Warmup: Prime lazy glyph rendering before entering steady-state
    {
        let producer_snap = config.producer_snapshot.load();
        if let Some(ref payload) = producer_snap.message_payload {
            last_message_payload = Some(std::sync::Arc::clone(payload));
            let req_id = mqtt_req_gen.next_id();
            let req = DisplayRequest::new(
                DisplaySourceId::Mqtt,
                req_id,
                h3,
                40,
                RequestLifecycle::Transient,
                true,
                5000,
            );
            arbiter.submit_request(req);
            message_sync.update(true, req_id, h3);
        }
        let decision = arbiter.evaluate(Instant::now());
        let _mode = runtime.transition_session(decision, &arbiter, &mut engine_runtime);
        runtime.update(&mut engine_runtime, &mut ctx);
        runtime.render(&mut engine_runtime, &mut ctx);
    }

    // 3. Arm the thread-local allocation tracker
    THREAD_ALLOC_COUNT.with(|c| c.set(0));
    THREAD_DEALLOC_COUNT.with(|c| c.set(0));
    THREAD_TRACKING.with(|t| t.set(true));

    // 4. Execute 10,000 steady-state H-Core cycles covering full render loop logic
    for i in 1..=10_000 {
        let now = Instant::now();

        // 3.1 Lock-Free Producer Snapshot Load (Zero Mutex, Zero Allocation)
        let producer_snap = config.producer_snapshot.load();
        let forced_mode = producer_snap.forced_mode;

        match forced_mode {
            arcadematrix::core::types::ForcedEngineMode::Message => {
                if let Some(ref payload) = producer_snap.message_payload {
                    let is_new = match &last_message_payload {
                        Some(last) => !std::sync::Arc::ptr_eq(last, payload),
                        None => true,
                    };
                    if is_new {
                        last_message_payload = Some(std::sync::Arc::clone(payload));
                        let req_id = mqtt_req_gen.next_id();
                        let req = DisplayRequest::new(
                            DisplaySourceId::Mqtt,
                            req_id,
                            h3,
                            40,
                            RequestLifecycle::Transient,
                            true,
                            5000,
                        );
                        arbiter.submit_request(req);
                        message_sync.update(true, req_id, h3);
                    }
                }
            }
            arcadematrix::core::types::ForcedEngineMode::Marquee => {
                if !marquee_sync.active {
                    let req_id = marquee_req_gen.next_id();
                    let req = DisplayRequest::new(
                        DisplaySourceId::Marquee,
                        req_id,
                        h2,
                        30,
                        RequestLifecycle::Persistent,
                        true,
                        0,
                    );
                    arbiter.submit_request(req);
                    marquee_sync.update(true, req_id, h2);
                }
            }
            arcadematrix::core::types::ForcedEngineMode::None => {
                last_message_payload = None;
                if message_sync.active {
                    arbiter.cancel_request(DisplaySourceId::Mqtt, 0);
                    message_sync.update(false, 0, EngineHandle::NULL);
                }
                if marquee_sync.active {
                    arbiter.cancel_request(DisplaySourceId::Marquee, 0);
                    marquee_sync.update(false, 0, EngineHandle::NULL);
                }
            }
        }

        // 3.2 Rotation Producer Sync
        if rotation_sync.has_changed(true, i as u32, h1) {
            rotation_sync.update(true, i as u32, h1);
        }

        // 3.3 Arbiter Decision & O(1) Vector Handle Resolution
        let decision = arbiter.evaluate(now);
        let _resolved = engine_runtime.resolve_handle(decision.engine_handle);
        let _inst = engine_runtime.get_active_instance(decision.engine_handle);

        // 3.4 Pure FSM Transition, Update & Render (including MessageEngine update with lock-free snapshot)
        let _mode = runtime.transition_session(decision, &arbiter, &mut engine_runtime);
        runtime.update(&mut engine_runtime, &mut ctx);
        runtime.render(&mut engine_runtime, &mut ctx);
    }

    // 4. Disarm tracker and assert ZERO allocations occurred
    THREAD_TRACKING.with(|t| t.set(false));
    let total_allocations = THREAD_ALLOC_COUNT.with(|c| c.get());
    assert_eq!(
        total_allocations, 0,
        "Zero-allocation H-Core invariant violated: observed {} allocations during 10,000 cycles!",
        total_allocations
    );
}

#[test]
fn test_lock_free_producer_snapshot_boundary() {
    let config = arcadematrix::core::config::Config::new("config.json");

    // Initially None
    let snap0 = config.producer_snapshot.load();
    assert_eq!(
        snap0.forced_mode,
        arcadematrix::core::types::ForcedEngineMode::None
    );
    assert!(snap0.message_payload.is_none());

    // Publish Message mode from Control Plane
    let msg = arcadematrix::engines::message::MessagePayload::new(
        "Alert".to_string(),
        "#ff0000",
        1,
        "left",
        5,
    );
    config.set_message_payload(Some(msg));

    let snap1 = config.producer_snapshot.load();
    assert_eq!(
        snap1.forced_mode,
        arcadematrix::core::types::ForcedEngineMode::Message
    );
    assert_eq!(snap1.message_payload.as_ref().unwrap().text, "Alert");
    assert_eq!(snap1.generation, 1);

    // Switch to Marquee mode
    config.set_forced_engine_mode(arcadematrix::core::types::ForcedEngineMode::Marquee);
    let snap2 = config.producer_snapshot.load();
    assert_eq!(
        snap2.forced_mode,
        arcadematrix::core::types::ForcedEngineMode::Marquee
    );
    assert_eq!(snap2.generation, 2);

    // Clear forced engine
    config.clear_forced_engine();
    let snap3 = config.producer_snapshot.load();
    assert_eq!(
        snap3.forced_mode,
        arcadematrix::core::types::ForcedEngineMode::None
    );
    assert!(snap3.message_payload.is_none());
    assert_eq!(snap3.generation, 3);
}

// =========================================================================
// Group 2 — Intent Identity & SameIntent Exact Contract
// =========================================================================

#[test]
fn test_intent_identity_exact_matrix() {
    let mut arbiter = DisplayArbiter::new();
    let handle_a = EngineHandle::new(1, 1);
    let handle_b = EngineHandle::new(1, 2);

    let t0 = Instant::now();
    let req_baseline = DisplayRequest {
        source_id: DisplaySourceId::Mqtt,
        request_id: 100,
        engine_handle: handle_a,
        priority: 40,
        lifecycle: RequestLifecycle::Transient,
        preemptive: true,
        created_at: t0,
        duration_ms: 5000,
    };
    arbiter.submit_request(req_baseline);

    // Case A: Same Source + Same RequestID + Same Handle -> PRESERVES created_at
    let req_same = DisplayRequest {
        source_id: DisplaySourceId::Mqtt,
        request_id: 100,
        engine_handle: handle_a,
        priority: 40,
        lifecycle: RequestLifecycle::Transient,
        preemptive: true,
        created_at: Instant::now() + std::time::Duration::from_secs(60),
        duration_ms: 5000,
    };
    arbiter.submit_request(req_same);
    let dec_a = arbiter.evaluate(Instant::now());
    assert_eq!(dec_a.request_id, 100);
    assert_eq!(dec_a.engine_handle, handle_a);

    // Case B: Same Source + Same RequestID + DIFFERENT Handle -> NEW Intent (updated)
    let req_diff_handle = DisplayRequest {
        source_id: DisplaySourceId::Mqtt,
        request_id: 100,
        engine_handle: handle_b,
        priority: 40,
        lifecycle: RequestLifecycle::Transient,
        preemptive: true,
        created_at: Instant::now(),
        duration_ms: 5000,
    };
    arbiter.submit_request(req_diff_handle);
    let dec_b = arbiter.evaluate(Instant::now());
    assert_eq!(dec_b.engine_handle, handle_b);

    // Case C: Same Source + DIFFERENT RequestID + Same Handle -> NEW Intent
    let req_diff_id = DisplayRequest {
        source_id: DisplaySourceId::Mqtt,
        request_id: 101,
        engine_handle: handle_b,
        priority: 40,
        lifecycle: RequestLifecycle::Transient,
        preemptive: true,
        created_at: Instant::now(),
        duration_ms: 5000,
    };
    arbiter.submit_request(req_diff_id);
    let dec_c = arbiter.evaluate(Instant::now());
    assert_eq!(dec_c.request_id, 101);

    // Case D: DIFFERENT Source + Same RequestID + Same Handle -> NEW Intent
    let req_diff_source = DisplayRequest {
        source_id: DisplaySourceId::Marquee,
        request_id: 101,
        engine_handle: handle_b,
        priority: 30,
        lifecycle: RequestLifecycle::Persistent,
        preemptive: true,
        created_at: Instant::now(),
        duration_ms: 0,
    };
    arbiter.submit_request(req_diff_source);
    assert_eq!(arbiter.active_count(), 2);
}

// =========================================================================
// Group 3 — Arbiter Source Slots & Priority Replacement
// =========================================================================

#[test]
fn test_arbiter_source_slots_and_priority() {
    let mut arbiter = DisplayArbiter::new();

    // 1. Submit requests for all 4 available sources
    let sources = [
        (DisplaySourceId::Rotation, 10, EngineHandle::new(1, 1)),
        (DisplaySourceId::Gif, 20, EngineHandle::new(2, 1)),
        (DisplaySourceId::Marquee, 30, EngineHandle::new(3, 1)),
        (DisplaySourceId::Mqtt, 40, EngineHandle::new(4, 1)),
    ];

    for (source, prio, handle) in sources {
        let req = DisplayRequest::new(
            source,
            prio as u32,
            handle,
            prio,
            RequestLifecycle::Persistent,
            false,
            0,
        );
        arbiter.submit_request(req);
    }
    assert_eq!(arbiter.active_count(), 4);

    // Winner must be highest priority (Mqtt = 40)
    let dec = arbiter.evaluate(Instant::now());
    assert_eq!(dec.source_id, DisplaySourceId::Mqtt);
    assert_eq!(dec.priority, 40);

    // 2. Cancel highest priority Mqtt -> Next winner is Marquee = 30
    arbiter.cancel_request(DisplaySourceId::Mqtt, 0);
    assert_eq!(arbiter.active_count(), 3);
    let dec2 = arbiter.evaluate(Instant::now());
    assert_eq!(dec2.source_id, DisplaySourceId::Marquee);
    assert_eq!(dec2.priority, 30);
}

// =========================================================================
// Group 4 — Preemption Stack & Bounded Saturation
// =========================================================================

#[test]
fn test_preemption_stack_saturation_and_safety() {
    let mut runtime = DisplayRuntime::new();
    let arbiter = DisplayArbiter::new();
    let mut engine_runtime = EngineRuntime::new();

    for i in 1..=6 {
        engine_runtime.register_instance_handle(&format!("inst_{}", i), "clock");
    }

    // Fill stack to MAX_PREEMPTION_DEPTH (4)
    let d0 = DisplayDecision {
        source_id: DisplaySourceId::Rotation,
        engine_handle: EngineHandle::new(1, 1),
        request_id: 1,
        priority: 10,
        preemptive: false,
    };
    assert_eq!(
        runtime.transition_session(d0, &arbiter, &mut engine_runtime),
        TransitionMode::Replace
    );

    for depth in 1..=4 {
        let dec = DisplayDecision {
            source_id: DisplaySourceId::Mqtt,
            engine_handle: EngineHandle::new(1, (depth + 1) as u16),
            request_id: (depth + 1) as u32,
            priority: 10 + (depth as u8 * 10),
            preemptive: true,
        };
        assert_eq!(
            runtime.transition_session(dec, &arbiter, &mut engine_runtime),
            TransitionMode::Preempt
        );
        assert_eq!(runtime.preemption_depth(), depth);
    }

    // 5th preemption when depth == 4: strictly rejected, active session intact
    let d_overflow = DisplayDecision {
        source_id: DisplaySourceId::Mqtt,
        engine_handle: EngineHandle::new(1, 6),
        request_id: 999,
        priority: 90,
        preemptive: true,
    };
    assert_eq!(
        runtime.transition_session(d_overflow, &arbiter, &mut engine_runtime),
        TransitionMode::None
    );
    assert_eq!(runtime.preemption_depth(), 4);
    assert_eq!(
        runtime.active_session().engine_handle,
        EngineHandle::new(1, 5)
    );
}

// =========================================================================
// Group 5 — Registry Invalidation & Cross-Engine Safety
// =========================================================================

#[test]
fn test_registry_cross_engine_and_invalidation() {
    let mut engine_runtime = EngineRuntime::new();
    let h_clock = engine_runtime.register_instance_handle("main_clock", "clock");
    let h_weather = engine_runtime.register_instance_handle("main_weather", "weather");

    assert!(engine_runtime.resolve_handle(h_clock));
    assert!(engine_runtime.resolve_handle(h_weather));

    // Handle with wrong engine_id for instance_id must FAIL resolution
    let corrupted_handle = EngineHandle::new(99, h_clock.instance_id);
    assert!(!engine_runtime.resolve_handle(corrupted_handle));
    assert!(engine_runtime
        .get_active_instance(corrupted_handle)
        .is_none());

    // Invalidate handle
    engine_runtime.invalidate_handle(h_clock);
    assert!(!engine_runtime.resolve_handle(h_clock));
    assert!(engine_runtime.get_active_instance(h_clock).is_none());
}

// =========================================================================
// Group 6 — Submerged Orphan Cleanup on Resume
// =========================================================================

#[test]
fn test_submerged_orphan_cleanup_exact_resume() {
    let mut arbiter = DisplayArbiter::new();
    let mut runtime = DisplayRuntime::new();
    let mut engine_runtime = EngineRuntime::new();

    let h_rot = engine_runtime.register_instance_handle("rot", "clock");
    let h_mar = engine_runtime.register_instance_handle("mar", "marquee");
    let h_mqtt = engine_runtime.register_instance_handle("mqtt", "message");

    // Baseline: Rotation
    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Rotation,
        1,
        h_rot,
        10,
        RequestLifecycle::Persistent,
        false,
        10000,
    ));
    let d1 = arbiter.evaluate(Instant::now());
    runtime.transition_session(d1, &arbiter, &mut engine_runtime);

    // Preemption 1: Marquee
    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Marquee,
        2001,
        h_mar,
        30,
        RequestLifecycle::Persistent,
        true,
        0,
    ));
    let d2 = arbiter.evaluate(Instant::now());
    runtime.transition_session(d2, &arbiter, &mut engine_runtime);

    // Preemption 2: MQTT
    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Mqtt,
        3001,
        h_mqtt,
        40,
        RequestLifecycle::Transient,
        true,
        5000,
    ));
    let d3 = arbiter.evaluate(Instant::now());
    runtime.transition_session(d3, &arbiter, &mut engine_runtime);
    assert_eq!(runtime.preemption_depth(), 2);

    // Marquee cancelled while submerged
    arbiter.cancel_request(DisplaySourceId::Marquee, 2001);

    // MQTT ends
    arbiter.cancel_request(DisplaySourceId::Mqtt, 3001);
    let d4 = arbiter.evaluate(Instant::now());

    // Resume must pop & discard orphan Marquee and resume Rotation cleanly
    let mode = runtime.transition_session(d4, &arbiter, &mut engine_runtime);
    assert_eq!(mode, TransitionMode::Resume);
    assert_eq!(runtime.active_session().engine_handle, h_rot);
    assert_eq!(runtime.preemption_depth(), 0);
}

// =========================================================================
// Group 7 — Stack Exhaustion Semantics (TransitionMode::None)
// =========================================================================

#[test]
fn test_stack_exhaustion_returns_none() {
    let mut arbiter = DisplayArbiter::new();
    let mut runtime = DisplayRuntime::new();
    let mut engine_runtime = EngineRuntime::new();

    let h_rot = engine_runtime.register_instance_handle("rot", "clock");

    arbiter.submit_request(DisplayRequest::new(
        DisplaySourceId::Rotation,
        1,
        h_rot,
        10,
        RequestLifecycle::Persistent,
        false,
        10000,
    ));
    let d1 = arbiter.evaluate(Instant::now());
    runtime.transition_session(d1, &arbiter, &mut engine_runtime);

    // Cancel rotation -> no intents remaining in Arbiter
    arbiter.cancel_request(DisplaySourceId::Rotation, 1);
    let d2 = arbiter.evaluate(Instant::now());
    assert!(d2.is_none());

    // Transition when stack is empty and decision is None must produce TransitionMode::None
    let mode = runtime.transition_session(d2, &arbiter, &mut engine_runtime);
    assert_eq!(mode, TransitionMode::None);
    assert!(!runtime.is_active());
}

// =========================================================================
// Group 8 — OrientationManager Geometry & Versioning
// =========================================================================

#[test]
fn test_orientation_manager_projections() {
    let mut om = OrientationManager::new(64, 32, 0);
    assert_eq!(om.geometry().logical_width, 64);
    assert_eq!(om.geometry().logical_height, 32);
    let v0 = om.geometry().version;

    // 90° Tate
    assert!(om.set_rotation(1));
    assert_eq!(om.geometry().logical_width, 32);
    assert_eq!(om.geometry().logical_height, 64);
    assert_eq!(om.geometry().version, v0 + 1);

    // 180°
    assert!(om.set_rotation(2));
    assert_eq!(om.geometry().logical_width, 64);
    assert_eq!(om.geometry().logical_height, 32);

    // 270° Tate
    assert!(om.set_rotation(3));
    assert_eq!(om.geometry().logical_width, 32);
    assert_eq!(om.geometry().logical_height, 64);
}

// =========================================================================
// Group 9 — RequestIdGenerator Monotonicity & Wrapping
// =========================================================================

#[test]
fn test_request_id_generator_sequence_and_wrapping() {
    let mut gen = RequestIdGenerator::new(1001);
    assert_eq!(gen.next_id(), 1001);
    assert_eq!(gen.next_id(), 1002);
    assert_eq!(gen.next_id(), 1003);

    let mut gen_wrap = RequestIdGenerator::new(u32::MAX);
    assert_eq!(gen_wrap.next_id(), u32::MAX);
    // After wrapping, must not emit 0
    assert_eq!(gen_wrap.next_id(), 1);
}

// =========================================================================
// Group 10 — GNewsEngine Vertical Wrapping & Multi-Line Character Integrity
// =========================================================================

#[test]
fn test_gnews_wrap_text_no_missing_letters() {
    use arcadematrix::engines::dashboard::font::measure_text;
    use arcadematrix::engines::gnews::GNewsEngine;

    let text = "Guerre en Ukraine : Les dernières informations technologiques et gouvernementales internationales";
    let max_w = 60; // 64px display with 2px margins
    let lines = GNewsEngine::wrap_text_to_lines(text, max_w);

    // 1. Verify that NO line exceeds max_w
    for (i, line) in lines.iter().enumerate() {
        let w = measure_text(line);
        assert!(
            w <= max_w,
            "Line {} exceeds max_w: '{}' (w={}, max_w={})",
            i,
            line,
            w,
            max_w
        );
    }

    // 2. Verify that all non-space characters from original text are fully preserved (0 missing letters!)
    let original_chars: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    let wrapped_chars: Vec<char> = lines
        .join("")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    assert_eq!(
        original_chars, wrapped_chars,
        "Every single letter must be preserved without loss"
    );
}

#[test]
fn test_gnews_wrap_text_narrow_tate_display() {
    use arcadematrix::engines::dashboard::font::measure_text;
    use arcadematrix::engines::gnews::GNewsEngine;

    // Narrow 32px display (max_w = 28px)
    let text = "Apple annonce l'iPhone 17 Pro Max";
    let max_w = 28;
    let lines = GNewsEngine::wrap_text_to_lines(text, max_w);

    for (i, line) in lines.iter().enumerate() {
        let w = measure_text(line);
        assert!(
            w <= max_w,
            "Line {} exceeds max_w: '{}' (w={}, max_w={})",
            i,
            line,
            w,
            max_w
        );
    }

    let original_chars: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    let wrapped_chars: Vec<char> = lines
        .join("")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    assert_eq!(original_chars, wrapped_chars);
}

#[test]
fn test_gnews_distribute_text_to_rows_even_distribution_and_char_preservation() {
    use arcadematrix::engines::gnews::GNewsEngine;

    let text = "Guerre en Ukraine : Les dernieres informations technologiques et gouvernementales internationales";
    let rows = GNewsEngine::distribute_text_to_rows(text, 4);

    assert_eq!(
        rows.len(),
        4,
        "Expected 4 rows distributed, got {}",
        rows.len()
    );

    let original_chars: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    let row_chars: Vec<char> = rows
        .join("")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    assert_eq!(
        original_chars, row_chars,
        "All characters must be preserved without loss"
    );
}

#[test]
fn test_gnews_display_mode_schema_options() {
    use arcadematrix::core::registry::EngineRegistry;

    let gnews_desc = EngineRegistry::get_descriptor("gnews").expect("gnews descriptor");
    let dm_field = gnews_desc
        .schema
        .fields
        .iter()
        .find(|f| f.id == "display_mode")
        .expect("display_mode field");

    let opts = dm_field.options.as_ref().expect("options list");
    let opt_values: Vec<&str> = opts.iter().map(|o| o.value).collect();

    assert!(opt_values.contains(&"smooth_scroll"));
    assert!(opt_values.contains(&"serpentine"));
    assert!(opt_values.contains(&"vertical_crawl"));
    assert!(opt_values.contains(&"static_paged"));
}

#[test]
fn test_gif_candidate_roots_strict_orientation_separation() {
    use arcadematrix::engines::gif::GifEngine;

    // In vertical (Tate) mode: candidate roots MUST ONLY be vertical (gifs_tate, tate)
    // NEVER horizontal (gifs, yoko)
    let tate_roots = GifEngine::get_candidate_roots(true);
    for root in &tate_roots {
        let p_str = root.to_string_lossy().to_lowercase();
        assert!(
            p_str.ends_with("gifs_tate") || p_str.ends_with("tate"),
            "Tate root {:?} contains non-vertical folder!",
            root
        );
        assert!(
            !p_str.ends_with("/gifs") && !p_str.ends_with("/yoko"),
            "Tate root {:?} must not end with horizontal /gifs or /yoko!",
            root
        );
    }

    // In horizontal (Yoko) mode: candidate roots MUST ONLY be horizontal (gifs, yoko)
    // NEVER vertical (gifs_tate, tate)
    let yoko_roots = GifEngine::get_candidate_roots(false);
    for root in &yoko_roots {
        let p_str = root.to_string_lossy().to_lowercase();
        assert!(
            !p_str.contains("gifs_tate") && !p_str.contains("tate"),
            "Yoko root {:?} must not contain vertical gifs_tate or tate!",
            root
        );
    }
}

#[test]
fn test_gif_engine_is_tate_detection() {
    use arcadematrix::core::engine_contract::Engine;
    use arcadematrix::core::types::DisplayGeometry;
    use arcadematrix::engines::gif::GifEngine;

    let engine_vert = GifEngine::new(32, 64);
    assert!(engine_vert.is_tate(), "32x64 must be Tate");

    let engine_horiz = GifEngine::new(64, 32);
    assert!(!engine_horiz.is_tate(), "64x32 must be Yoko");

    let mut engine_square = GifEngine::new(64, 64);
    assert!(!engine_square.is_tate(), "64x64 unrotated must be Yoko");

    // Rotate 64x64 to 90 degrees
    let geom_rotated = DisplayGeometry::new(64, 64, 1, 1);
    engine_square.on_display_geometry_changed(&geom_rotated);
    assert!(engine_square.is_tate(), "64x64 rotated 90° must be Tate");
}

#[test]
fn test_gif_engine_plays_nothing_when_no_matching_gifs() {
    use arcadematrix::core::engine_contract::Engine;
    use arcadematrix::engines::gif::GifEngine;

    // Test with non-existent playlist folder: must return false, frames empty, and is_finished() = true
    let mut engine = GifEngine::new(32, 64);
    let success = engine.play_random_playlist_gif(&["nonexistent_folder_xyz".to_string()]);
    assert!(!success, "Must return false when no GIFs found");
    assert!(engine.is_empty(), "Frames must remain empty");
    assert!(
        engine.is_finished(),
        "Engine must report finished so rotation advances"
    );
}

#[test]
fn test_gnews_serpentine_track_pos_alternating_tiers() {
    use arcadematrix::engines::gnews::GNewsEngine;

    let x_min = 2;
    let x_max = 57;
    let body_y = 24;
    let line_spacing = 9;
    let leg_len = x_max - x_min; // 55
    let tier_len = leg_len + line_spacing; // 64

    // 1. Tier 0 (Even: Right -> Left)
    // Start of tier 0: dist = 0 -> (x_max, body_y)
    let (x0, y0) = GNewsEngine::serpentine_track_pos(0, x_min, x_max, body_y, line_spacing);
    assert_eq!(x0, x_max);
    assert_eq!(y0, body_y);

    // Mid of tier 0: dist = 20 -> x decreases
    let (x_mid0, y_mid0) =
        GNewsEngine::serpentine_track_pos(20, x_min, x_max, body_y, line_spacing);
    assert_eq!(x_mid0, x_max - 20);
    assert_eq!(y_mid0, body_y);

    // End of tier 0 leg: dist = 55 -> reaches x_min
    let (x_end0, y_end0) =
        GNewsEngine::serpentine_track_pos(leg_len, x_min, x_max, body_y, line_spacing);
    assert_eq!(x_end0, x_min);
    assert_eq!(y_end0, body_y);

    // 2. Turn 0 (Downwards at left edge)
    // Progress 4 px down the left turn
    let (x_turn0, y_turn0) =
        GNewsEngine::serpentine_track_pos(leg_len + 4, x_min, x_max, body_y, line_spacing);
    assert_eq!(x_turn0, x_min);
    assert_eq!(y_turn0, body_y + 4);

    // 3. Tier 1 (Odd: Left -> Right)
    // Start of tier 1: dist = tier_len -> (x_min, body_y + line_spacing)
    let (x1, y1) = GNewsEngine::serpentine_track_pos(tier_len, x_min, x_max, body_y, line_spacing);
    assert_eq!(x1, x_min);
    assert_eq!(y1, body_y + line_spacing);

    // Mid of tier 1: dist = tier_len + 25 -> x increases (Left -> Right)
    let (x_mid1, y_mid1) =
        GNewsEngine::serpentine_track_pos(tier_len + 25, x_min, x_max, body_y, line_spacing);
    assert_eq!(x_mid1, x_min + 25);
    assert_eq!(y_mid1, body_y + line_spacing);

    // End of tier 1 leg: dist = tier_len + leg_len -> reaches x_max
    let (x_end1, y_end1) =
        GNewsEngine::serpentine_track_pos(tier_len + leg_len, x_min, x_max, body_y, line_spacing);
    assert_eq!(x_end1, x_max);
    assert_eq!(y_end1, body_y + line_spacing);

    // 4. Turn 1 (Downwards at right edge)
    let (x_turn1, y_turn1) = GNewsEngine::serpentine_track_pos(
        tier_len + leg_len + 5,
        x_min,
        x_max,
        body_y,
        line_spacing,
    );
    assert_eq!(x_turn1, x_max);
    assert_eq!(y_turn1, body_y + line_spacing + 5);

    // 5. Tier 2 (Even: Right -> Left again)
    let (x2, y2) =
        GNewsEngine::serpentine_track_pos(2 * tier_len, x_min, x_max, body_y, line_spacing);
    assert_eq!(x2, x_max);
    assert_eq!(y2, body_y + 2 * line_spacing);
}

#[test]
fn test_gnews_serpentine_mental_track_transitions() {
    use arcadematrix::engines::gnews::GNewsEngine;

    let clip_min_x = 0;
    let clip_max_x = 64;
    let body_y = 24;
    let line_spacing = 9;
    let char_w = 6;
    let leg_len = clip_max_x - (clip_min_x - char_w); // 70

    // Tier 0: Row 0 (Right -> Left as u decreases)
    // At u = 0, character is fully off-screen to the left (x = -6)
    let (x0_end, y0_end) =
        GNewsEngine::serpentine_track_pos_mental(0, clip_min_x, clip_max_x, body_y, line_spacing);
    assert_eq!(x0_end, -6);
    assert_eq!(y0_end, body_y);

    // Initial visible offset for first character at S=0 (u = 8 -> x = 2)
    let (x0_init, y0_init) =
        GNewsEngine::serpentine_track_pos_mental(8, clip_min_x, clip_max_x, body_y, line_spacing);
    assert_eq!(x0_init, 2);
    assert_eq!(y0_init, body_y);

    // Transition between Row 1 right edge and Row 0 right edge (pure organic wrap-around)
    // Row 1 right edge: u = leg_len -> x = 64 (fully off-screen to the right)
    let (x1_right, y1_right) = GNewsEngine::serpentine_track_pos_mental(
        leg_len,
        clip_min_x,
        clip_max_x,
        body_y,
        line_spacing,
    );
    assert_eq!(x1_right, 64);
    assert_eq!(y1_right, body_y + line_spacing);

    // 1px progression (u = leg_len - 1) -> immediately entering Row 0 at right edge (x = 63)!
    let (x0_enter, y0_enter) = GNewsEngine::serpentine_track_pos_mental(
        leg_len - 1,
        clip_min_x,
        clip_max_x,
        body_y,
        line_spacing,
    );
    assert_eq!(x0_enter, 63);
    assert_eq!(y0_enter, body_y);

    // Transition between Row 2 left edge and Row 1 left edge (pure organic wrap-around)
    // Row 2 left edge: u = 2 * leg_len -> x = -6 (fully off-screen to the left)
    let (x2_left, y2_left) = GNewsEngine::serpentine_track_pos_mental(
        2 * leg_len,
        clip_min_x,
        clip_max_x,
        body_y,
        line_spacing,
    );
    assert_eq!(x2_left, -6);
    assert_eq!(y2_left, body_y + 2 * line_spacing);

    // 1px progression (u = 2 * leg_len - 1) -> immediately entering Row 1 at left edge (x = -5)!
    let (x1_enter, y1_enter) = GNewsEngine::serpentine_track_pos_mental(
        2 * leg_len - 1,
        clip_min_x,
        clip_max_x,
        body_y,
        line_spacing,
    );
    assert_eq!(x1_enter, -5);
    assert_eq!(y1_enter, body_y + line_spacing);
}

#[test]
fn test_gnews_serpentine_render_execution_no_panic() {
    use arcadematrix::core::matrix::MatrixBackend;
    use arcadematrix::engines::gnews::GNewsEngine;

    struct MockMatrix {
        width: u32,
        height: u32,
        pixels: Vec<(i32, i32, u8, u8, u8)>,
    }
    impl MockMatrix {
        fn new(w: u32, h: u32) -> Self {
            Self {
                width: w,
                height: h,
                pixels: Vec::new(),
            }
        }
    }
    impl MatrixBackend for MockMatrix {
        fn width(&self) -> u32 {
            self.width
        }
        fn height(&self) -> u32 {
            self.height
        }
        fn set_pixel(&mut self, x: i32, y: i32, r: u8, g: u8, b: u8) {
            self.pixels.push((x, y, r, g, b));
        }
        fn clear(&mut self) {
            self.pixels.clear();
        }
        fn update(&mut self) {}
        fn set_brightness(&mut self, _brightness: u8) {}
    }

    let title_chars: Vec<char> = "BREAKING NEWS: ArcadeMatrix Serpentine Flow Working!"
        .chars()
        .collect();
    let mut matrix = MockMatrix::new(64, 64);

    // Render at offset 0 (entry on line 1)
    GNewsEngine::render_serpentine_headline(&mut matrix, &title_chars, 0, 64, 24, 0, 64, 24, 64, 9);
    assert!(
        !matrix.pixels.is_empty(),
        "Pixels should be drawn on serpentine track"
    );

    // Render mid-stream (long snake spanning multiple lines)
    matrix.pixels.clear();
    GNewsEngine::render_serpentine_headline(
        &mut matrix,
        &title_chars,
        20,
        64,
        24,
        0,
        64,
        24,
        64,
        9,
    );
    assert!(
        !matrix.pixels.is_empty(),
        "Pixels should be drawn across S-curve tiers"
    );

    // Render on horizontal 256x64 display
    let mut matrix_wide = MockMatrix::new(256, 64);
    GNewsEngine::render_serpentine_headline(
        &mut matrix_wide,
        &title_chars,
        15,
        256,
        24,
        0,
        256,
        17,
        64,
        9,
    );
    assert!(
        !matrix_wide.pixels.is_empty(),
        "Widescreen 256x64 should render serpentine flow"
    );
}

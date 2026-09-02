use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Numerical compact handle uniquely identifying an engine instance on the hot path
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EngineHandle {
    pub engine_id: u16,   // Unique numeric identifier for the engine type
    pub instance_id: u16, // Unique numeric identifier for the configured instance
}

impl EngineHandle {
    pub const NULL: Self = Self {
        engine_id: 0,
        instance_id: 0,
    };

    #[inline]
    pub const fn new(engine_id: u16, instance_id: u16) -> Self {
        Self {
            engine_id,
            instance_id,
        }
    }

    #[inline]
    pub fn is_null(&self) -> bool {
        self.engine_id == 0 && self.instance_id == 0
    }
}

/// Strict priority scale for audio-excluded RPi architecture
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DisplaySourceId {
    Rotation = 10,
    Gif = 20,
    Marquee = 30,
    Mqtt = 40,
}

/// Request lifecycle duration contract
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestLifecycle {
    Transient,  // Automatically expires after duration_ms
    Persistent, // Stays active until explicit cancellation
}

/// Bounded intent stored in DisplayArbiter (Value type, zero-heap allocation)
#[derive(Debug, Clone, Copy)]
pub struct DisplayRequest {
    pub source_id: DisplaySourceId,
    pub request_id: u32,
    pub engine_handle: EngineHandle,
    pub priority: u8,
    pub lifecycle: RequestLifecycle,
    pub preemptive: bool,
    pub created_at: Instant,
    pub duration_ms: u32,
}

impl DisplayRequest {
    pub fn new(
        source_id: DisplaySourceId,
        request_id: u32,
        engine_handle: EngineHandle,
        priority: u8,
        lifecycle: RequestLifecycle,
        preemptive: bool,
        duration_ms: u32,
    ) -> Self {
        Self {
            source_id,
            request_id,
            engine_handle,
            priority,
            lifecycle,
            preemptive,
            created_at: Instant::now(),
            duration_ms,
        }
    }

    #[inline]
    pub fn is_expired(&self, now: Instant) -> bool {
        match self.lifecycle {
            RequestLifecycle::Persistent => false,
            RequestLifecycle::Transient => {
                if self.duration_ms == 0 {
                    false
                } else {
                    now.duration_since(self.created_at).as_millis() >= self.duration_ms as u128
                }
            }
        }
    }
}

/// Intent decision emitted by DisplayArbiter (Strictly no transition state inside)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayDecision {
    pub source_id: DisplaySourceId,
    pub engine_handle: EngineHandle,
    pub request_id: u32,
    pub priority: u8,
    pub preemptive: bool,
}

impl DisplayDecision {
    pub const NONE: Self = Self {
        source_id: DisplaySourceId::Rotation,
        engine_handle: EngineHandle::NULL,
        request_id: 0,
        priority: 0,
        preemptive: false,
    };

    #[inline]
    pub fn is_none(&self) -> bool {
        self.engine_handle.is_null() && self.priority == 0
    }

    #[inline]
    pub fn matches_intent(&self, session: &DisplaySession) -> bool {
        session.is_active
            && self.source_id == session.source_id
            && self.request_id == session.request_id
            && self.engine_handle == session.engine_handle
    }
}

/// Active display session owned exclusively by DisplayRuntime
#[derive(Debug, Clone, Copy)]
pub struct DisplaySession {
    pub session_id: u32,
    pub source_id: DisplaySourceId,
    pub engine_handle: EngineHandle,
    pub request_id: u32,
    pub priority: u8,
    pub started_at: Instant,
    pub is_active: bool,
}

impl DisplaySession {
    pub fn empty() -> Self {
        Self {
            session_id: 0,
            source_id: DisplaySourceId::Rotation,
            engine_handle: EngineHandle::NULL,
            request_id: 0,
            priority: 0,
            started_at: Instant::now(),
            is_active: false,
        }
    }
}

/// Compact stack entry for deterministic preemption tracking (POD representation)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreemptionEntry {
    pub session_id: u32,
    pub source_id: DisplaySourceId,
    pub engine_handle: EngineHandle,
    pub request_id: u32,
    pub priority: u8,
    pub started_at: Instant,
}

/// Internal session mutation transition modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionMode {
    None,
    Replace,
    Preempt,
    Resume,
}

/// Tri-state override for transverse overlays (Fighter)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FighterOverride {
    Unspecified,
    Disabled,
    Enabled,
}

impl Default for FighterOverride {
    fn default() -> Self {
        Self::Unspecified
    }
}

/// Centralized layout classification for geometric projections
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutClass {
    Landscape,
    Portrait,
    Square,
    Tall,
    Wide,
}

/// Full geometric display profile with version tracking for O(1) change detection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayGeometry {
    pub physical_width: u32,
    pub physical_height: u32,
    pub logical_width: u32,
    pub logical_height: u32,
    pub rotation: u8, // 0: 0°, 1: 90°, 2: 180°, 3: 270°
    pub layout_class: LayoutClass,
    pub version: u64,
}

impl DisplayGeometry {
    pub fn new(physical_width: u32, physical_height: u32, rotation: u8, version: u64) -> Self {
        let (logical_width, logical_height) = if rotation == 1 || rotation == 3 {
            (physical_height, physical_width)
        } else {
            (physical_width, physical_height)
        };

        let layout_class = Self::classify(logical_width, logical_height);

        Self {
            physical_width,
            physical_height,
            logical_width,
            logical_height,
            rotation,
            layout_class,
            version,
        }
    }

    #[inline]
    pub fn classify(width: u32, height: u32) -> LayoutClass {
        if width == height {
            LayoutClass::Square
        } else if height >= (width * 3) / 2 {
            LayoutClass::Tall
        } else if height > width {
            LayoutClass::Portrait
        } else if width >= height * 3 {
            LayoutClass::Wide
        } else {
            LayoutClass::Landscape
        }
    }
}

/// Edge-triggering state tracker for producers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProducerSyncState {
    pub active: bool,
    pub request_id: u32,
    pub handle: EngineHandle,
}

impl ProducerSyncState {
    pub const INIT: Self = Self {
        active: false,
        request_id: 0,
        handle: EngineHandle::NULL,
    };

    #[inline]
    pub fn has_changed(&self, active: bool, request_id: u32, handle: EngineHandle) -> bool {
        self.active != active || self.request_id != request_id || self.handle != handle
    }

    #[inline]
    pub fn update(&mut self, active: bool, request_id: u32, handle: EngineHandle) {
        self.active = active;
        self.request_id = request_id;
        self.handle = handle;
    }
}

/// Monotonic domain-owned request ID generator ensuring distinct event identity
#[derive(Debug, Clone)]
pub struct RequestIdGenerator {
    next_id: u32,
}

impl RequestIdGenerator {
    pub const fn new(initial_id: u32) -> Self {
        Self {
            next_id: initial_id,
        }
    }

    #[inline]
    pub fn next_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        if self.next_id == 0 {
            self.next_id = 1;
        }
        id
    }
}

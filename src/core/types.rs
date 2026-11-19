use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;

/// Mode for forced overriding of the display engine from the Control Plane
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ForcedEngineMode {
    None,
    Message,
    Marquee,
}

/// Cycle behavior once an in-game Showcase sequence finishes playing while the game is still running
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InGameCycleMode {
    Once,
    Loop,
    Alternating { marquee_interval_sec: u32 },
}

/// Preloaded in-game Showcase stream published from the FrontendSync worker to the render loop
#[derive(Clone)]
pub struct InGameShowcasePayload {
    pub session_token: u64,
    pub stream: Arc<crate::core::dmd_stream::PreloadedDmdStream>,
    pub cycle_mode: InGameCycleMode,
}

impl std::fmt::Debug for InGameShowcasePayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InGameShowcasePayload")
            .field("session_token", &self.session_token)
            .field("frame_count", &self.stream.frame_count)
            .field("cycle_mode", &self.cycle_mode)
            .finish()
    }
}

impl PartialEq for InGameShowcasePayload {
    fn eq(&self, other: &Self) -> bool {
        self.session_token == other.session_token && self.cycle_mode == other.cycle_mode
    }
}

impl Eq for InGameShowcasePayload {}

/// Copies a UTF-8 string into a fixed-size POD byte buffer, truncating safely on a char boundary.
#[inline]
pub fn copy_str_to_pod<const N: usize>(src: &str) -> [u8; N] {
    let mut dst = [0u8; N];
    if N == 0 {
        return dst;
    }
    let max_len = N - 1;
    let bytes = src.as_bytes();
    if bytes.len() <= max_len {
        dst[..bytes.len()].copy_from_slice(bytes);
    } else {
        let mut end = max_len;
        while end > 0 && !src.is_char_boundary(end) {
            end -= 1;
        }
        dst[..end].copy_from_slice(&bytes[..end]);
    }
    dst
}

/// Returns a zero-allocation `&str` slice from a null-padded UTF-8 POD byte buffer.
#[inline]
pub fn pod_bytes_to_str(buf: &[u8]) -> &str {
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    std::str::from_utf8(&buf[..len]).unwrap_or("")
}

/// Fixed-size POD entry for a single arcade high score row (zero H-Core allocation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncHiscoreEntryPod {
    pub rank: u8,
    pub player: [u8; 16],
    pub score: [u8; 16],
}

impl SyncHiscoreEntryPod {
    pub const EMPTY: Self = Self {
        rank: 0,
        player: [0u8; 16],
        score: [0u8; 16],
    };

    pub fn from_strs(rank: u8, player: &str, score: &str) -> Self {
        Self {
            rank,
            player: copy_str_to_pod(player.trim()),
            score: copy_str_to_pod(score.trim()),
        }
    }

    #[inline]
    pub fn player_str(&self) -> &str {
        pod_bytes_to_str(&self.player)
    }

    #[inline]
    pub fn score_str(&self) -> &str {
        pod_bytes_to_str(&self.score)
    }
}

impl Default for SyncHiscoreEntryPod {
    fn default() -> Self {
        Self::EMPTY
    }
}

/// Fixed-size POD summary for RetroAchievements progress on the active game (zero H-Core allocation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncRaSummaryPod {
    pub game_title: [u8; 48],
    pub unlocked: u16,
    pub total: u16,
    pub points_earned: u32,
    pub points_total: u32,
    pub hardcore: bool,
    pub recent_badge: [u8; 48],
    pub valid: bool,
}

impl SyncRaSummaryPod {
    pub const EMPTY: Self = Self {
        game_title: [0u8; 48],
        unlocked: 0,
        total: 0,
        points_earned: 0,
        points_total: 0,
        hardcore: false,
        recent_badge: [0u8; 48],
        valid: false,
    };

    #[inline]
    pub fn game_title_str(&self) -> &str {
        pod_bytes_to_str(&self.game_title)
    }

    #[inline]
    pub fn recent_badge_str(&self) -> &str {
        pod_bytes_to_str(&self.recent_badge)
    }
}

impl Default for SyncRaSummaryPod {
    fn default() -> Self {
        Self::EMPTY
    }
}

/// Pre-parsed fixed-size POD metadata for `HallOfFameCard` & `RetroAchievementsCard` during `playing`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncMetaPayload {
    pub session_token: u64,
    pub system: [u8; 24],
    pub game_title: [u8; 48],
    pub hiscore_count: u8,
    pub hiscores: [SyncHiscoreEntryPod; 5],
    pub ra: SyncRaSummaryPod,
    pub show_hiscores: bool,
    pub show_ra: bool,
    pub card_duration_sec: u32,
}

impl SyncMetaPayload {
    pub const EMPTY: Self = Self {
        session_token: 0,
        system: [0u8; 24],
        game_title: [0u8; 48],
        hiscore_count: 0,
        hiscores: [SyncHiscoreEntryPod::EMPTY; 5],
        ra: SyncRaSummaryPod::EMPTY,
        show_hiscores: true,
        show_ra: true,
        card_duration_sec: 8,
    };

    #[inline]
    pub fn system_str(&self) -> &str {
        pod_bytes_to_str(&self.system)
    }

    #[inline]
    pub fn game_title_str(&self) -> &str {
        pod_bytes_to_str(&self.game_title)
    }

    #[inline]
    pub fn has_active_hiscores(&self) -> bool {
        self.show_hiscores && self.hiscore_count > 0
    }

    #[inline]
    pub fn has_active_ra(&self) -> bool {
        self.show_ra && self.ra.valid && self.ra.total > 0
    }

    #[inline]
    pub fn has_any_card(&self) -> bool {
        self.has_active_hiscores() || self.has_active_ra()
    }
}

impl Default for SyncMetaPayload {
    fn default() -> Self {
        Self::EMPTY
    }
}

/// Fixed-size POD celebration payload for Priority 55 60 FPS celebration preemption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncCelebrationPayload {
    pub event_token: u64,
    /// `0` = New High Score Record, `1` = RetroAchievement Unlocked
    pub kind: u8,
    pub system: [u8; 24],
    pub title: [u8; 48],
    pub subtitle: [u8; 48],
    pub score_or_pts: [u8; 24],
    pub hardcore: bool,
    pub duration_ms: u32,
}

impl SyncCelebrationPayload {
    pub const EMPTY: Self = Self {
        event_token: 0,
        kind: 0,
        system: [0u8; 24],
        title: [0u8; 48],
        subtitle: [0u8; 48],
        score_or_pts: [0u8; 24],
        hardcore: false,
        duration_ms: 7500,
    };

    #[inline]
    pub fn system_str(&self) -> &str {
        pod_bytes_to_str(&self.system)
    }

    #[inline]
    pub fn title_str(&self) -> &str {
        pod_bytes_to_str(&self.title)
    }

    #[inline]
    pub fn subtitle_str(&self) -> &str {
        pod_bytes_to_str(&self.subtitle)
    }

    #[inline]
    pub fn score_or_pts_str(&self) -> &str {
        pod_bytes_to_str(&self.score_or_pts)
    }
}

impl Default for SyncCelebrationPayload {
    fn default() -> Self {
        Self::EMPTY
    }
}

/// Immutable lock-free snapshot published atomically by Control Plane to Realtime Plane
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProducerSnapshot {
    pub forced_mode: ForcedEngineMode,
    pub message_payload: Option<Arc<crate::engines::message::MessagePayload>>,
    pub ingame_showcase: Option<Arc<InGameShowcasePayload>>,
    pub sync_meta: Option<Arc<SyncMetaPayload>>,
    pub sync_celebration: Option<Arc<SyncCelebrationPayload>>,
    pub generation: u32,
}

impl Default for ProducerSnapshot {
    fn default() -> Self {
        Self {
            forced_mode: ForcedEngineMode::None,
            message_payload: None,
            ingame_showcase: None,
            sync_meta: None,
            sync_celebration: None,
            generation: 0,
        }
    }
}

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
    InGameShowcase = 45,
    SyncCelebration = 55,
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
    pub fn width(&self) -> u32 {
        self.logical_width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.logical_height
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

use crate::core::config::{ConfigSettings, RotationEntry};
use crate::core::registry::EngineRuntime;
use crate::core::types::{DisplayRequest, DisplaySourceId, EngineHandle, RequestLifecycle};
use std::time::Instant;

/// Decoupled RotationManager managing playlist pacing, slot index, and duration timers
pub struct RotationManager {
    current_index: usize,
    cycle_sequence: u32,
    slot_start_time: Instant,
}

impl Default for RotationManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RotationManager {
    pub fn new() -> Self {
        Self {
            current_index: 0,
            cycle_sequence: 1,
            slot_start_time: Instant::now(),
        }
    }

    #[inline]
    pub fn current_index(&self) -> usize {
        self.current_index
    }

    #[inline]
    pub fn slot_elapsed(&self) -> std::time::Duration {
        self.slot_start_time.elapsed()
    }

    pub fn reset(&mut self) {
        self.current_index = 0;
        self.cycle_sequence = self.cycle_sequence.wrapping_add(1);
        self.slot_start_time = Instant::now();
    }

    /// Advances to the next rotation slot
    pub fn advance(&mut self, rotation_list: &[RotationEntry]) {
        if rotation_list.is_empty() {
            return;
        }
        self.current_index = (self.current_index + 1) % rotation_list.len();
        self.cycle_sequence = self.cycle_sequence.wrapping_add(1);
        self.slot_start_time = Instant::now();
    }

    /// Evaluates if the current rotation slot has completed (either via time duration or is_finished)
    pub fn evaluate_advance(
        &mut self,
        rotation_list: &[RotationEntry],
        active_engine_finished: bool,
        active_engine_self_paced: bool,
    ) -> bool {
        if rotation_list.is_empty() {
            return false;
        }

        let entry = &rotation_list[self.current_index % rotation_list.len()];

        let should_advance = if active_engine_self_paced {
            active_engine_finished
        } else {
            let duration_secs = if entry.duration_sec > 0 {
                entry.duration_sec as u64
            } else {
                10 // Default fallback duration
            };
            self.slot_start_time.elapsed() >= std::time::Duration::from_secs(duration_secs)
                || active_engine_finished
        };

        if should_advance {
            self.advance(rotation_list);
            true
        } else {
            false
        }
    }

    /// Resolves the current rotation entry
    pub fn current_entry<'a>(
        &self,
        rotation_list: &'a [RotationEntry],
    ) -> Option<&'a RotationEntry> {
        if rotation_list.is_empty() {
            None
        } else {
            Some(&rotation_list[self.current_index % rotation_list.len()])
        }
    }

    /// Resolves the current EngineHandle from config instances
    pub fn current_handle(
        &self,
        rotation_list: &[RotationEntry],
        engine_runtime: &mut EngineRuntime,
        config: &ConfigSettings,
    ) -> Option<EngineHandle> {
        let entry = self.current_entry(rotation_list)?;
        // Find instance in config.instances
        let instance = config
            .instances
            .iter()
            .find(|inst| inst.instance_id == entry.instance_id)?;

        let handle =
            engine_runtime.register_instance_handle(&instance.instance_id, &instance.engine_id);
        Some(handle)
    }

    /// Builds a DisplayRequest for the currently active rotation slot
    pub fn build_rotation_request(
        &self,
        rotation_list: &[RotationEntry],
        engine_runtime: &mut EngineRuntime,
        config: &ConfigSettings,
    ) -> Option<DisplayRequest> {
        let entry = self.current_entry(rotation_list)?;
        let handle = self.current_handle(rotation_list, engine_runtime, config)?;

        Some(DisplayRequest::new(
            DisplaySourceId::Rotation,
            self.cycle_sequence,
            handle,
            DisplaySourceId::Rotation as u8, // Priority 10
            RequestLifecycle::Persistent,
            false, // Non-preemptive
            (entry.duration_sec * 1000) as u32,
        ))
    }
}

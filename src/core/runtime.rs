use crate::core::arbiter::DisplayArbiter;
use crate::core::engine_contract::EngineContext;
use crate::core::registry::EngineRuntime;
use crate::core::types::{
    DisplayDecision, DisplayGeometry, DisplaySession, DisplaySourceId, PreemptionEntry,
    TransitionMode,
};
use std::collections::HashMap;
use std::time::Instant;

pub const MAX_PREEMPTION_DEPTH: usize = 4;

/// Bounded fixed-depth preemption stack (Zero heap allocation)
#[derive(Debug, Clone, Copy)]
pub struct PreemptionStack {
    entries: [Option<PreemptionEntry>; MAX_PREEMPTION_DEPTH],
    depth: usize,
}

impl Default for PreemptionStack {
    fn default() -> Self {
        Self::new()
    }
}

impl PreemptionStack {
    pub const fn new() -> Self {
        Self {
            entries: [None; MAX_PREEMPTION_DEPTH],
            depth: 0,
        }
    }

    #[inline]
    pub fn depth(&self) -> usize {
        self.depth
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.depth >= MAX_PREEMPTION_DEPTH
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.depth == 0
    }

    pub fn push(&mut self, entry: PreemptionEntry) -> bool {
        if self.depth >= MAX_PREEMPTION_DEPTH {
            return false; // Deterministic rejection on saturation
        }
        self.entries[self.depth] = Some(entry);
        self.depth += 1;
        true
    }

    pub fn pop(&mut self) -> Option<PreemptionEntry> {
        if self.depth == 0 {
            return None;
        }
        self.depth -= 1;
        let entry = self.entries[self.depth].take();
        entry
    }

    pub fn peek(&self) -> Option<&PreemptionEntry> {
        if self.depth == 0 {
            None
        } else {
            self.entries[self.depth - 1].as_ref()
        }
    }

    pub fn clear(&mut self) {
        for slot in self.entries.iter_mut() {
            *slot = None;
        }
        self.depth = 0;
    }
}

/// Transactional 4-way DisplayRuntime FSM (REFRESH [Internal], PREEMPT, RESUME, REPLACE)
pub struct DisplayRuntime {
    active_session: DisplaySession,
    preemption_stack: PreemptionStack,
    next_session_id: u32,
    geometry_version: u64,
}

impl Default for DisplayRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl DisplayRuntime {
    pub fn new() -> Self {
        Self {
            active_session: DisplaySession::empty(),
            preemption_stack: PreemptionStack::new(),
            next_session_id: 1,
            geometry_version: 0,
        }
    }

    #[inline]
    pub fn active_session(&self) -> &DisplaySession {
        &self.active_session
    }

    #[inline]
    pub fn preemption_depth(&self) -> usize {
        self.preemption_stack.depth()
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        self.active_session.is_active
    }

    /// Evaluates and applies the session transition based on DisplayDecision and current intent state.
    pub fn transition_session(
        &mut self,
        decision: DisplayDecision,
        arbiter: &DisplayArbiter,
        engine_runtime: &mut EngineRuntime,
        context: &mut EngineContext,
        config_map: &HashMap<String, String>,
    ) -> TransitionMode {
        // 1. Decision is NONE (no active intent winning)
        if decision.is_none() {
            if self.active_session.is_active {
                // Active session ended -> Unwind preemption stack to find valid resumable session
                return self.unwind_stack_to_resume(arbiter, engine_runtime, context, config_map);
            }
            return TransitionMode::None;
        }

        // 2. Decision is active, evaluate transition against current session
        if self.active_session.is_active {
            // Case A: Exact match -> NOOP
            if decision.source_id == self.active_session.source_id
                && decision.engine_handle == self.active_session.engine_handle
                && decision.request_id == self.active_session.request_id
            {
                return TransitionMode::None;
            }

            // Case B: Same source + Same engine + New request_id -> REFRESH [Internal]
            if decision.source_id == self.active_session.source_id
                && decision.engine_handle == self.active_session.engine_handle
                && decision.request_id != self.active_session.request_id
            {
                self.active_session.request_id = decision.request_id;
                // In-place engine update without lifecycle disruption
                if let Some(engine) = engine_runtime.get_instance_by_handle(
                    decision.engine_handle,
                    context,
                    config_map,
                ) {
                    let cfg = crate::core::engine_contract::HashConfig { data: config_map };
                    engine.on_config_changed(&cfg);
                }
                return TransitionMode::None;
            }

            // Case C: Same source + Different engine handle -> REPLACE
            if decision.source_id == self.active_session.source_id
                && decision.engine_handle != self.active_session.engine_handle
            {
                // Transactional check
                if !engine_runtime.resolve_handle(decision.engine_handle) {
                    return TransitionMode::None; // Rejet transactionnel
                }

                self.deactivate_current(engine_runtime);
                self.activate_new(decision, engine_runtime, context, config_map);
                return TransitionMode::Replace;
            }

            // Case D: Higher priority decision and preemptive -> PREEMPT
            let active_priority = self.active_session_priority();
            if decision.priority > active_priority && decision.preemptive {
                // Transactional check: ensure incoming target engine can be resolved
                if !engine_runtime.resolve_handle(decision.engine_handle) {
                    return TransitionMode::None; // Rejet transactionnel: session courante intacte
                }

                // Check stack capacity (saturation rejection)
                if self.preemption_stack.is_full() {
                    return TransitionMode::None; // Depth == 4 -> rejection déterministe
                }

                // Pause active engine and push onto preemption stack
                if let Some(engine) =
                    engine_runtime.get_active_instance(self.active_session.engine_handle)
                {
                    engine.pause();
                }

                let preemption_entry = PreemptionEntry {
                    session_id: self.active_session.session_id,
                    source_id: self.active_session.source_id,
                    engine_handle: self.active_session.engine_handle,
                    request_id: self.active_session.request_id,
                    started_at: self.active_session.started_at,
                };
                self.preemption_stack.push(preemption_entry);

                // Activate new preemptive session
                self.activate_new(decision, engine_runtime, context, config_map);
                return TransitionMode::Preempt;
            }

            // Case E: Priority decreased (Active intent expired/cancelled, or lower intent winning)
            if decision.priority < active_priority {
                // If the active source is no longer present in Arbiter intents, resume parent
                if !arbiter.has_request(self.active_session.source_id) {
                    self.deactivate_current(engine_runtime);
                    return self.unwind_stack_or_activate(
                        decision,
                        arbiter,
                        engine_runtime,
                        context,
                        config_map,
                    );
                }
                // Otherwise if active source still has a valid intent, it stays dominant
                return TransitionMode::None;
            }

            // Case F: Same priority but different source -> REPLACE
            if decision.priority == active_priority
                && decision.source_id != self.active_session.source_id
            {
                if !engine_runtime.resolve_handle(decision.engine_handle) {
                    return TransitionMode::None; // Rejet transactionnel
                }
                self.deactivate_current(engine_runtime);
                self.activate_new(decision, engine_runtime, context, config_map);
                return TransitionMode::Replace;
            }
        } else {
            // 3. Initial activation when no session was active -> REPLACE
            if !engine_runtime.resolve_handle(decision.engine_handle) {
                return TransitionMode::None; // Rejet transactionnel
            }
            self.activate_new(decision, engine_runtime, context, config_map);
            return TransitionMode::Replace;
        }

        TransitionMode::None
    }

    /// Resilient stack unwinding helper to resume the next valid entry
    fn unwind_stack_to_resume(
        &mut self,
        arbiter: &DisplayArbiter,
        engine_runtime: &mut EngineRuntime,
        context: &mut EngineContext,
        config_map: &HashMap<String, String>,
    ) -> TransitionMode {
        self.deactivate_current(engine_runtime);

        while let Some(top) = self.preemption_stack.pop() {
            // Check if top entry's intent is still valid (Rotation is always persistent)
            let is_still_desired =
                top.source_id == DisplaySourceId::Rotation || arbiter.has_request(top.source_id);

            if is_still_desired && engine_runtime.resolve_handle(top.engine_handle) {
                if let Some(engine) =
                    engine_runtime.get_instance_by_handle(top.engine_handle, context, config_map)
                {
                    engine.resume();
                    self.active_session = DisplaySession {
                        session_id: top.session_id,
                        source_id: top.source_id,
                        engine_handle: top.engine_handle,
                        request_id: top.request_id,
                        started_at: top.started_at,
                        is_active: true,
                    };
                    return TransitionMode::Resume;
                }
            }
            // If top was expired/invalid, clean it up and continue unwinding
            if let Some(engine) = engine_runtime.get_active_instance(top.engine_handle) {
                engine.deactivate();
            }
        }

        self.active_session = DisplaySession::empty();
        TransitionMode::Resume
    }

    /// Resumes parent stack entry or activates lower winning intent
    fn unwind_stack_or_activate(
        &mut self,
        decision: DisplayDecision,
        arbiter: &DisplayArbiter,
        engine_runtime: &mut EngineRuntime,
        context: &mut EngineContext,
        config_map: &HashMap<String, String>,
    ) -> TransitionMode {
        // First try to resume from stack if top matches decision
        while let Some(top) = self.preemption_stack.pop() {
            if top.source_id == decision.source_id && top.engine_handle == decision.engine_handle {
                if let Some(engine) =
                    engine_runtime.get_instance_by_handle(top.engine_handle, context, config_map)
                {
                    engine.resume();
                    self.active_session = DisplaySession {
                        session_id: top.session_id,
                        source_id: top.source_id,
                        engine_handle: top.engine_handle,
                        request_id: decision.request_id,
                        started_at: top.started_at,
                        is_active: true,
                    };
                    return TransitionMode::Resume;
                }
            } else if top.source_id == DisplaySourceId::Rotation
                || arbiter.has_request(top.source_id)
            {
                // If top is another valid higher entry, resume it
                if let Some(engine) =
                    engine_runtime.get_instance_by_handle(top.engine_handle, context, config_map)
                {
                    engine.resume();
                    self.active_session = DisplaySession {
                        session_id: top.session_id,
                        source_id: top.source_id,
                        engine_handle: top.engine_handle,
                        request_id: top.request_id,
                        started_at: top.started_at,
                        is_active: true,
                    };
                    return TransitionMode::Resume;
                }
            }
        }

        // Otherwise activate the decision directly
        self.activate_new(decision, engine_runtime, context, config_map);
        TransitionMode::Resume
    }

    fn activate_new(
        &mut self,
        decision: DisplayDecision,
        engine_runtime: &mut EngineRuntime,
        context: &mut EngineContext,
        config_map: &HashMap<String, String>,
    ) {
        if let Some(engine) =
            engine_runtime.get_instance_by_handle(decision.engine_handle, context, config_map)
        {
            engine.activate();
        }

        let session_id = self.next_session_id;
        self.next_session_id += 1;

        self.active_session = DisplaySession {
            session_id,
            source_id: decision.source_id,
            engine_handle: decision.engine_handle,
            request_id: decision.request_id,
            started_at: Instant::now(),
            is_active: true,
        };
    }

    fn deactivate_current(&mut self, engine_runtime: &mut EngineRuntime) {
        if self.active_session.is_active {
            if let Some(engine) =
                engine_runtime.get_active_instance(self.active_session.engine_handle)
            {
                engine.deactivate();
            }
            self.active_session.is_active = false;
        }
    }

    #[inline]
    fn active_session_priority(&self) -> u8 {
        self.active_session.source_id as u8
    }

    /// Ticking update method called on the active engine every frame
    pub fn update(&mut self, engine_runtime: &mut EngineRuntime, context: &mut EngineContext) {
        if self.active_session.is_active {
            if let Some(engine) =
                engine_runtime.get_active_instance(self.active_session.engine_handle)
            {
                engine.update(context);
            }
        }
    }

    /// Renders the active engine into the base matrix framebuffer
    pub fn render(&mut self, engine_runtime: &mut EngineRuntime, context: &mut EngineContext) {
        if self.active_session.is_active {
            if let Some(engine) =
                engine_runtime.get_active_instance(self.active_session.engine_handle)
            {
                engine.render(context);
            }
        }
    }

    /// Synchronously notifies the active engine of geometric display changes (Tate mode, etc.)
    pub fn on_display_geometry_changed(
        &mut self,
        geometry: &DisplayGeometry,
        engine_runtime: &mut EngineRuntime,
    ) {
        if self.geometry_version != geometry.version {
            self.geometry_version = geometry.version;
            if self.active_session.is_active {
                if let Some(engine) =
                    engine_runtime.get_active_instance(self.active_session.engine_handle)
                {
                    engine.on_display_geometry_changed(geometry);
                }
            }
        }
    }
}

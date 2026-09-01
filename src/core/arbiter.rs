use crate::core::types::{DisplayDecision, DisplayRequest, DisplaySourceId};
use std::time::Instant;

pub const MAX_ARBITER_REQUESTS: usize = 8;

/// Stateless intent evaluator with bounded fixed storage (zero heap allocation)
pub struct DisplayArbiter {
    requests: [Option<DisplayRequest>; MAX_ARBITER_REQUESTS],
}

impl Default for DisplayArbiter {
    fn default() -> Self {
        Self::new()
    }
}

impl DisplayArbiter {
    pub const fn new() -> Self {
        Self {
            requests: [None; MAX_ARBITER_REQUESTS],
        }
    }

    /// Submits or updates an intent in the bounded storage.
    /// Invariant: If request_id matches an existing request for the same source,
    /// created_at is strictly preserved to ensure deterministic expiration.
    pub fn submit_request(&mut self, mut request: DisplayRequest) {
        // 1. Check if an intent already exists for this source
        for slot in self.requests.iter_mut() {
            if let Some(existing) = slot {
                if existing.source_id == request.source_id {
                    // Same source + same request_id -> PRESERVE created_at
                    if existing.request_id == request.request_id {
                        request.created_at = existing.created_at;
                    }
                    *existing = request;
                    return;
                }
            }
        }

        // 2. Otherwise, find the first available free slot
        for slot in self.requests.iter_mut() {
            if slot.is_none() {
                *slot = Some(request);
                return;
            }
        }

        // 3. Fallback if capacity is full: replace slot with lowest priority if incoming is higher
        let mut min_prio = request.priority;
        let mut min_idx = None;
        for (i, slot) in self.requests.iter().enumerate() {
            if let Some(existing) = slot {
                if existing.priority < min_prio {
                    min_prio = existing.priority;
                    min_idx = Some(i);
                }
            }
        }

        if let Some(idx) = min_idx {
            self.requests[idx] = Some(request);
        }
    }

    /// Cancels an intent from the bounded storage.
    /// If request_id is 0, cancels any request for this source_id.
    pub fn cancel_request(&mut self, source_id: DisplaySourceId, request_id: u32) {
        for slot in self.requests.iter_mut() {
            if let Some(existing) = slot {
                if existing.source_id == source_id
                    && (request_id == 0 || existing.request_id == request_id)
                {
                    *slot = None;
                }
            }
        }
    }

    /// Evaluates the current published intents and returns the winning intent decision.
    /// Purges expired transient requests in O(1) without heap allocation.
    pub fn evaluate(&mut self, now: Instant) -> DisplayDecision {
        let mut best_idx: Option<usize> = None;
        let mut best_prio: u8 = 0;
        let mut best_created = now;

        for (i, slot) in self.requests.iter_mut().enumerate() {
            if let Some(req) = slot {
                // Purge expired requests
                if req.is_expired(now) {
                    *slot = None;
                    continue;
                }

                // Evaluate highest priority (ties broken by earliest created_at)
                if req.priority > best_prio
                    || (req.priority == best_prio && req.created_at < best_created)
                {
                    best_prio = req.priority;
                    best_created = req.created_at;
                    best_idx = Some(i);
                }
            }
        }

        if let Some(idx) = best_idx {
            if let Some(winner) = &self.requests[idx] {
                return DisplayDecision {
                    source_id: winner.source_id,
                    engine_handle: winner.engine_handle,
                    request_id: winner.request_id,
                    priority: winner.priority,
                    preemptive: winner.preemptive,
                };
            }
        }

        DisplayDecision::NONE
    }

    /// Returns the number of currently active intents
    pub fn active_count(&self) -> usize {
        self.requests.iter().filter(|s| s.is_some()).count()
    }

    /// Checks if an intent with a specific source_id is currently present
    pub fn has_request(&self, source_id: DisplaySourceId) -> bool {
        self.requests.iter().any(|s| match s {
            Some(r) => r.source_id == source_id,
            None => false,
        })
    }
}

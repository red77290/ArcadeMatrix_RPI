use crate::core::engine_contract::EngineDescriptor;
use crate::core::types::EngineHandle;
use linkme::distributed_slice;

#[distributed_slice]
pub static ENGINES: [fn() -> EngineDescriptor];

pub struct EngineRegistry;

impl EngineRegistry {
    pub fn get_all_descriptors() -> Vec<EngineDescriptor> {
        let mut descriptors = Vec::new();
        for engine_fn in ENGINES {
            descriptors.push(engine_fn());
        }
        descriptors
    }

    pub fn get_descriptor(id: &str) -> Option<EngineDescriptor> {
        let canonical_id = match id {
            "sysinfo" | "sys_info" => "system_info",
            "gif" => "gifs",
            "cast" => "google_cast",
            other => other,
        };
        for engine_fn in ENGINES {
            let desc = engine_fn();
            if desc.metadata.id == canonical_id || desc.metadata.id == id {
                return Some(desc);
            }
        }
        None
    }

    pub fn engine_name_to_id(name: &str) -> u16 {
        match name {
            "clock" => 1,
            "crypto" => 2,
            "stock" => 3,
            "weather" => 4,
            "date" => 5,
            "gifs" | "gif" => 6,
            "marquee" => 7,
            "message" => 8,
            "system_info" | "sysinfo" | "sys_info" => 9,
            "frontend_sync" => 10,
            "spotify" => 11,
            "google_cast" | "cast" => 12,
            "dashboard" => 13,
            "fighter" => 14,
            "gnews" => 15,
            "mqttdata" => 16,
            "retro_games" => 17,
            "movie_showcase" => 18,
            "series_showcase" => 19,
            "anime_showcase" => 20,
            _ => 0,
        }
    }

    pub fn id_to_engine_name(id: u16) -> Option<&'static str> {
        match id {
            1 => Some("clock"),
            2 => Some("crypto"),
            3 => Some("stock"),
            4 => Some("weather"),
            5 => Some("date"),
            6 => Some("gifs"),
            7 => Some("marquee"),
            8 => Some("message"),
            9 => Some("system_info"),
            10 => Some("frontend_sync"),
            11 => Some("spotify"),
            12 => Some("google_cast"),
            13 => Some("dashboard"),
            14 => Some("fighter"),
            15 => Some("gnews"),
            16 => Some("mqttdata"),
            17 => Some("retro_games"),
            18 => Some("movie_showcase"),
            19 => Some("series_showcase"),
            20 => Some("anime_showcase"),
            _ => None,
        }
    }
}

use crate::core::engine_contract::{Engine, EngineContext, HashConfig};
use std::collections::HashMap;

/// Registered instance metadata and direct engine container stored in the permanent registry index
pub struct RegisteredInstance {
    pub handle: EngineHandle,
    pub instance_name: String,
    pub engine_name: String,
    pub engine: Option<Box<dyn Engine>>,
    pub ready: bool,
    pub needs_activate: bool,
    pub last_rotation_budget: Option<u32>,
    pub last_config: HashMap<String, String>,
    pub baseline_rss_bytes: usize,
    pub recycle_count: u32,
}

pub struct EngineRuntime {
    id_to_registered: Vec<Option<RegisteredInstance>>,
    instance_to_id: HashMap<String, u16>,
    next_instance_id: u16,
}

impl Default for EngineRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineRuntime {
    pub fn new() -> Self {
        Self {
            id_to_registered: vec![None], // index 0 is reserved for NULL
            instance_to_id: HashMap::new(),
            next_instance_id: 1,
        }
    }

    /// Registers or retrieves an interned EngineHandle for the given instance and engine IDs.
    /// This is a cold-path operation performed during initialization and config updates.
    pub fn register_instance_handle(
        &mut self,
        instance_id_str: &str,
        engine_id_str: &str,
    ) -> EngineHandle {
        let engine_id = EngineRegistry::engine_name_to_id(engine_id_str);
        let instance_id = if let Some(&id) = self.instance_to_id.get(instance_id_str) {
            id
        } else {
            let id = self.next_instance_id;
            self.next_instance_id += 1;
            self.instance_to_id.insert(instance_id_str.to_string(), id);
            id
        };

        let handle = EngineHandle::new(engine_id, instance_id);
        let idx = instance_id as usize;
        if idx >= self.id_to_registered.len() {
            self.id_to_registered.resize_with(idx + 1, || None);
        }

        if self.id_to_registered[idx].is_none() {
            let engine = if engine_id_str == "frontend_sync" {
                Some(
                    Box::new(crate::engines::frontend_sync::InGameShowcaseEngine::new())
                        as Box<dyn Engine>,
                )
            } else if let Some(desc) = EngineRegistry::get_descriptor(engine_id_str) {
                if desc.available {
                    Some((desc.factory)())
                } else {
                    None
                }
            } else {
                None
            };
            let ready = false;

            self.id_to_registered[idx] = Some(RegisteredInstance {
                handle,
                instance_name: instance_id_str.to_string(),
                engine_name: engine_id_str.to_string(),
                engine,
                ready,
                needs_activate: false,
                last_rotation_budget: None,
                last_config: HashMap::new(),
                baseline_rss_bytes: 0,
                recycle_count: 0,
            });
        }

        handle
    }

    /// Checks if a handle can be resolved strictly against the registry and instance table in O(1).
    /// Zero heap allocation (O(1)).
    #[inline]
    pub fn resolve_handle(&self, handle: EngineHandle) -> bool {
        if handle.is_null() {
            return false;
        }

        let idx = handle.instance_id as usize;
        if let Some(Some(entry)) = self.id_to_registered.get(idx) {
            if entry.handle.engine_id == handle.engine_id && entry.engine.is_some() {
                return true;
            }
        }
        false
    }

    /// Returns string identifiers (instance_id, engine_id) for an EngineHandle in O(1) without allocation.
    #[inline]
    pub fn handle_to_names(&self, handle: EngineHandle) -> Option<(&str, &str)> {
        let idx = handle.instance_id as usize;
        let entry = self.id_to_registered.get(idx)?.as_ref()?;
        if entry.handle.engine_id != handle.engine_id {
            return None;
        }
        Some((entry.instance_name.as_str(), entry.engine_name.as_str()))
    }

    /// Returns how many times an instance was auto-recycled by the runtime memory guard.
    #[inline]
    pub fn instance_recycle_count(&self, handle: EngineHandle) -> u32 {
        let idx = handle.instance_id as usize;
        self.id_to_registered
            .get(idx)
            .and_then(|s| s.as_ref())
            .filter(|e| e.handle.engine_id == handle.engine_id)
            .map(|e| e.recycle_count)
            .unwrap_or(0)
    }

    /// Retrieves an engine instance by its compact EngineHandle in O(1) without String allocations or HashMap lookups.
    #[inline]
    pub fn get_instance_by_handle(&mut self, handle: EngineHandle) -> Option<&mut Box<dyn Engine>> {
        self.get_active_instance(handle)
    }

    /// Retrieves an already initialized engine instance by handle in O(1) without String allocations or HashMap lookups.
    #[inline]
    pub fn get_active_instance(&mut self, handle: EngineHandle) -> Option<&mut Box<dyn Engine>> {
        if handle.is_null() {
            return None;
        }
        let idx = handle.instance_id as usize;
        let entry = self.id_to_registered.get_mut(idx)?.as_mut()?;
        if entry.handle.engine_id != handle.engine_id {
            return None;
        }
        entry.engine.as_mut()
    }

    /// Activates an instance and records its pre-activation RSS baseline.
    /// If the instance was previously recycled via `Drop` (`!entry.ready`), defers `activate()`
    /// until `ensure_ready_and_get_active()` runs `initialize()` on the current frame.
    pub fn activate_instance(&mut self, handle: EngineHandle) {
        if handle.is_null() {
            return;
        }
        let idx = handle.instance_id as usize;
        if let Some(Some(entry)) = self.id_to_registered.get_mut(idx) {
            if entry.handle.engine_id != handle.engine_id {
                return;
            }
            entry.baseline_rss_bytes = crate::core::memory::current_rss_bytes();
            if entry.ready {
                entry.needs_activate = false;
                if let Some(engine) = entry.engine.as_mut() {
                    engine.activate();
                }
            } else {
                entry.needs_activate = true;
            }
        }
    }

    /// Records and applies the rotation playback budget for an instance, preserving it across auto-recycles.
    pub fn set_instance_rotation_budget(&mut self, handle: EngineHandle, budget: u32) {
        if handle.is_null() {
            return;
        }
        let idx = handle.instance_id as usize;
        if let Some(Some(entry)) = self.id_to_registered.get_mut(idx) {
            if entry.handle.engine_id != handle.engine_id {
                return;
            }
            entry.last_rotation_budget = Some(budget);
            if let Some(engine) = entry.engine.as_mut() {
                engine.set_rotation_budget(budget);
            }
        }
    }

    /// Transparently re-initializes a recycled instance from `last_config` (and triggers pending
    /// `set_rotation_budget` / `activate`) before returning a mutable reference to the active engine.
    pub fn ensure_ready_and_get_active(
        &mut self,
        handle: EngineHandle,
        context: &mut EngineContext,
    ) -> Option<&mut Box<dyn Engine>> {
        if handle.is_null() {
            return None;
        }
        let idx = handle.instance_id as usize;
        let entry = self.id_to_registered.get_mut(idx)?.as_mut()?;
        if entry.handle.engine_id != handle.engine_id {
            return None;
        }
        if entry.engine.is_none() {
            if entry.engine_name == "frontend_sync" {
                entry.engine = Some(Box::new(
                    crate::engines::frontend_sync::InGameShowcaseEngine::new(),
                ));
            } else if let Some(desc) = EngineRegistry::get_descriptor(&entry.engine_name) {
                if desc.available {
                    entry.engine = Some((desc.factory)());
                }
            }
        }
        if let Some(engine) = entry.engine.as_mut() {
            if !entry.ready {
                let cfg = HashConfig {
                    data: &entry.last_config,
                };
                let _ = engine.initialize(context, &cfg);
                entry.ready = true;
                if let Some(budget) = entry.last_rotation_budget {
                    engine.set_rotation_budget(budget);
                }
                if entry.needs_activate {
                    entry.needs_activate = false;
                    engine.activate();
                }
            } else if entry.needs_activate {
                entry.needs_activate = false;
                engine.activate();
            }
        }
        entry.engine.as_mut()
    }

    /// Drops the current `Box<dyn Engine>` for `handle` (forcing Rust's compiler-generated `Drop`
    /// to release 100% of all fields owned by the engine struct), trims the process heap, and
    /// replaces the slot with a fresh `< 1 KB` uninitialized shell that will lazily re-initialize
    /// from `last_config` on its next active frame.
    pub fn recycle_instance(
        &mut self,
        handle: EngineHandle,
        reactivate_on_next_update: bool,
    ) -> bool {
        if handle.is_null() {
            return false;
        }
        let idx = handle.instance_id as usize;
        if let Some(Some(entry)) = self.id_to_registered.get_mut(idx) {
            if entry.handle.engine_id != handle.engine_id {
                return false;
            }
            if let Some(mut old_eng) = entry.engine.take() {
                old_eng.deactivate();
                drop(old_eng);
            }
            crate::core::memory::trim_heap();
            entry.engine = EngineRegistry::get_descriptor(&entry.engine_name)
                .filter(|d| d.available)
                .map(|d| (d.factory)());
            entry.ready = false;
            entry.needs_activate = reactivate_on_next_update;
            entry.recycle_count = entry.recycle_count.saturating_add(1);
            entry.baseline_rss_bytes = crate::core::memory::current_rss_bytes();
            return true;
        }
        false
    }

    /// Deactivates an outgoing engine instance, trims the process heap, and audits its post-deactivate
    /// RSS footprint. If the engine retained > 15 MB of net RSS growth (or if process RSS >= 200 MB),
    /// automatically drops and recycles the instance via `Drop`.
    pub fn deactivate_and_audit(&mut self, handle: EngineHandle) -> bool {
        self.deactivate_and_audit_with_rss(handle, None, None)
    }

    pub fn deactivate_and_audit_with_rss(
        &mut self,
        handle: EngineHandle,
        simulated_baseline_rss: Option<usize>,
        simulated_post_rss: Option<usize>,
    ) -> bool {
        if handle.is_null() {
            return false;
        }
        let idx = handle.instance_id as usize;
        let (baseline_rss, engine_name, instance_name) =
            if let Some(Some(entry)) = self.id_to_registered.get_mut(idx) {
                if entry.handle.engine_id != handle.engine_id {
                    return false;
                }
                if let Some(engine) = entry.engine.as_mut() {
                    engine.deactivate();
                }
                entry.needs_activate = false;
                (
                    simulated_baseline_rss.unwrap_or(entry.baseline_rss_bytes),
                    entry.engine_name.clone(),
                    entry.instance_name.clone(),
                )
            } else {
                return false;
            };

        crate::core::memory::trim_heap();
        let post_rss = simulated_post_rss.unwrap_or_else(crate::core::memory::current_rss_bytes);

        if crate::core::memory::should_recycle_after_deactivate(baseline_rss, post_rss) {
            tracing::warn!(
                "[MemoryGuard] Engine '{}' (instance '{}') retained excessive memory after deactivate (baseline={} MB, post={} MB); auto-recycling via Drop",
                engine_name,
                instance_name,
                baseline_rss / (1024 * 1024),
                post_rss / (1024 * 1024)
            );
            return self.recycle_instance(handle, false);
        }
        false
    }

    /// Periodic and turn-boundary RSS watchdog (modeled after ESP32 `MemoryManager`).
    /// - `Nominal` (< 200 MB): no-op.
    /// - `Moderate` (200..260 MB): notifies active engine via `on_memory_pressure(Moderate)` and
    ///   recycles all dormant (inactive) initialized instances.
    /// - `Critical` (>= 260 MB): recycles all dormant instances AND recycles the active instance
    ///   itself (`Drop` + lazy re-init on next frame) so even a single engine running overnight
    ///   with an internal leak can never exceed the 260 MB ceiling.
    pub fn enforce_memory_watchdog(
        &mut self,
        active_handle: EngineHandle,
        preserved_handles: &[EngineHandle],
        simulated_rss_bytes: Option<usize>,
    ) -> crate::core::memory::MemoryPressureLevel {
        let rss_bytes = simulated_rss_bytes.unwrap_or_else(crate::core::memory::current_rss_bytes);
        let level = crate::core::memory::evaluate_rss_pressure(rss_bytes);
        if level == crate::core::memory::MemoryPressureLevel::Nominal {
            return level;
        }

        // 1. Recycle all dormant initialized instances (excluding active_handle and preserved_handles)
        let dormant_handles: Vec<EngineHandle> = self
            .id_to_registered
            .iter()
            .filter_map(|slot| slot.as_ref())
            .filter(|entry| {
                entry.ready
                    && entry.handle != active_handle
                    && !preserved_handles.contains(&entry.handle)
            })
            .map(|entry| entry.handle)
            .collect();

        for h in dormant_handles {
            self.recycle_instance(h, false);
        }

        match level {
            crate::core::memory::MemoryPressureLevel::Nominal => {}
            crate::core::memory::MemoryPressureLevel::Moderate => {
                if let Some(eng) = self.get_active_instance(active_handle) {
                    eng.on_memory_pressure(level);
                }
                crate::core::memory::trim_heap();
            }
            crate::core::memory::MemoryPressureLevel::Critical => {
                if let Some((inst_name, eng_name)) = self.handle_to_names(active_handle) {
                    tracing::warn!(
                        "[MemoryGuard] Process RSS ({} MB) breached critical ceiling (260 MB) while running '{}' ('{}'); auto-recycling active engine via Drop",
                        rss_bytes / (1024 * 1024),
                        eng_name,
                        inst_name
                    );
                }
                self.recycle_instance(active_handle, true);
                crate::core::memory::trim_heap();
            }
        }
        level
    }

    /// Invalidate a handle (deactivates and removes it from the registry table).
    pub fn invalidate_handle(&mut self, handle: EngineHandle) {
        let idx = handle.instance_id as usize;
        if let Some(slot) = self.id_to_registered.get_mut(idx) {
            let matches = slot
                .as_ref()
                .is_some_and(|entry| entry.handle.engine_id == handle.engine_id);
            if matches {
                if let Some(mut entry) = slot.take() {
                    if let Some(mut eng) = entry.engine.take() {
                        eng.deactivate();
                    }
                }
                crate::core::memory::trim_heap();
            }
        }
    }

    /// Deactivates and drops any registered engine instance whose `instance_name` is not in
    /// `active_instance_ids` and whose handle is not in `preserved_handles`.
    /// Ensures both unchecked rotation instances and deleted instances (`DELETE /api/instances/{id}`)
    /// are cleanly deactivated and freed without instantiating unscheduled engines.
    pub fn prune_inactive_instances(
        &mut self,
        active_instance_ids: &std::collections::HashSet<&str>,
        preserved_handles: &[EngineHandle],
    ) -> usize {
        let mut pruned = 0usize;
        for slot in self.id_to_registered.iter_mut() {
            let should_prune = if let Some(entry) = slot.as_ref() {
                !preserved_handles.contains(&entry.handle)
                    && !active_instance_ids.contains(entry.instance_name.as_str())
            } else {
                false
            };
            if should_prune {
                if let Some(mut entry) = slot.take() {
                    if let Some(mut eng) = entry.engine.take() {
                        eng.deactivate();
                    }
                    pruned += 1;
                }
            }
        }
        if pruned > 0 {
            crate::core::memory::trim_heap();
        }
        pruned
    }

    /// Initializes and configures an engine instance in the Control Plane path.
    pub fn init_instance(
        &mut self,
        handle: EngineHandle,
        context: &mut EngineContext,
        config_map: &HashMap<String, String>,
    ) -> bool {
        let idx = handle.instance_id as usize;
        if let Some(Some(entry)) = self.id_to_registered.get_mut(idx) {
            if entry.handle.engine_id != handle.engine_id {
                return false;
            }
            if entry.engine.is_none() {
                if let Some(desc) = EngineRegistry::get_descriptor(&entry.engine_name) {
                    if desc.available {
                        entry.engine = Some((desc.factory)());
                    }
                }
            }
            if let Some(engine) = entry.engine.as_mut() {
                if !entry.ready {
                    let cfg = HashConfig { data: config_map };
                    let _ = engine.initialize(context, &cfg);
                    entry.ready = true;
                    entry.last_config = config_map.clone();
                } else if entry.last_config != *config_map {
                    let cfg = HashConfig { data: config_map };
                    engine.on_config_changed(&cfg);
                    entry.last_config = config_map.clone();
                }
                return true;
            }
        }
        false
    }

    /// Notifies an instance of updated configuration in the Control Plane path.
    pub fn apply_instance_config(
        &mut self,
        handle: EngineHandle,
        config_map: &HashMap<String, String>,
    ) {
        let idx = handle.instance_id as usize;
        if let Some(Some(entry)) = self.id_to_registered.get_mut(idx) {
            if entry.handle.engine_id == handle.engine_id {
                entry.last_config = config_map.clone();
                if entry.ready {
                    if let Some(engine) = entry.engine.as_mut() {
                        let cfg = HashConfig { data: config_map };
                        engine.on_config_changed(&cfg);
                    }
                }
            }
        }
    }

    /// Primary lazy factory for control-plane initialization and compatibility.
    pub fn get_instance(
        &mut self,
        instance_id: &str,
        engine_id: &str,
        context: &mut EngineContext,
        config_map: &HashMap<String, String>,
    ) -> Option<&mut Box<dyn Engine>> {
        let handle = self.register_instance_handle(instance_id, engine_id);
        self.init_instance(handle, context, config_map);
        self.get_instance_by_handle(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_prune_inactive_instances_reclaims_unscheduled_and_deleted_handles() {
        let mut runtime = EngineRuntime::new();
        let h_clock = runtime.register_instance_handle("clock_1", "clock");
        let h_movies = runtime.register_instance_handle("movie_showcase_1", "movie_showcase");
        let h_deleted = runtime.register_instance_handle("deleted_gif_1", "gifs");

        assert!(runtime.resolve_handle(h_clock));
        assert!(runtime.resolve_handle(h_movies));
        assert!(runtime.resolve_handle(h_deleted));

        let mut active: HashSet<&str> = HashSet::new();
        active.insert("clock_1");

        let pruned = runtime.prune_inactive_instances(&active, &[]);
        assert_eq!(pruned, 2);
        assert!(runtime.resolve_handle(h_clock));
        assert!(!runtime.resolve_handle(EngineHandle::new(999, h_clock.instance_id)));
        assert!(!runtime.resolve_handle(h_movies));
        assert!(!runtime.resolve_handle(h_deleted));
    }

    #[test]
    fn test_auto_recycle_on_excessive_post_deactivate_rss_and_watchdog() {
        let mut runtime = EngineRuntime::new();
        let h_clock = runtime.register_instance_handle("clock_1", "clock");
        let h_gifs = runtime.register_instance_handle("gifs_1", "gifs");

        let base_rss = 100 * 1024 * 1024;
        // 1. Clean engine (retains only +2 MB) -> NOT recycled
        let recycled_clean = runtime.deactivate_and_audit_with_rss(
            h_clock,
            Some(base_rss),
            Some(base_rss + 2 * 1024 * 1024),
        );
        assert!(!recycled_clean);
        assert_eq!(runtime.instance_recycle_count(h_clock), 0);

        // 2. Leaky engine (retains +25 MB after deactivate) -> automatically recycled via Drop!
        let recycled_leaky = runtime.deactivate_and_audit_with_rss(
            h_gifs,
            Some(base_rss),
            Some(base_rss + 25 * 1024 * 1024),
        );
        assert!(recycled_leaky);
        assert_eq!(runtime.instance_recycle_count(h_gifs), 1);
        // Handle remains resolvable for its next rotation turn
        assert!(runtime.resolve_handle(h_gifs));

        // 3. Critical watchdog ceiling (270 MB) while running h_clock alone -> auto-recycles active engine
        let level = runtime.enforce_memory_watchdog(h_clock, &[], Some(270 * 1024 * 1024));
        assert_eq!(level, crate::core::memory::MemoryPressureLevel::Critical);
        assert_eq!(runtime.instance_recycle_count(h_clock), 1);
        assert!(runtime.resolve_handle(h_clock));
    }
}

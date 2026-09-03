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
    pub last_config: HashMap<String, String>,
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
            let engine = if let Some(desc) = EngineRegistry::get_descriptor(engine_id_str) {
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
                last_config: HashMap::new(),
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

    /// Invalidate a handle (removes it from the registry table).
    pub fn invalidate_handle(&mut self, handle: EngineHandle) {
        let idx = handle.instance_id as usize;
        if let Some(slot) = self.id_to_registered.get_mut(idx) {
            if let Some(entry) = slot {
                if entry.handle.engine_id == handle.engine_id {
                    *slot = None;
                }
            }
        }
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
                if let Some(engine) = entry.engine.as_mut() {
                    let cfg = HashConfig { data: config_map };
                    engine.on_config_changed(&cfg);
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

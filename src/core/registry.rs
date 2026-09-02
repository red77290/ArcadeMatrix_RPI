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
            _ => None,
        }
    }
}

use crate::core::engine_contract::{Engine, EngineContext, HashConfig};
use std::collections::HashMap;

/// Registered instance metadata stored in the permanent registry index
#[derive(Debug, Clone)]
pub struct RegisteredInstance {
    pub handle: EngineHandle,
    pub instance_name: String,
    pub engine_name: String,
}

pub struct EngineRuntime {
    instances: HashMap<String, Box<dyn Engine>>,
    configs: HashMap<String, HashMap<String, String>>,
    instance_to_id: HashMap<String, u16>,
    id_to_registered: Vec<Option<RegisteredInstance>>,
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
            instances: HashMap::new(),
            configs: HashMap::new(),
            instance_to_id: HashMap::new(),
            id_to_registered: vec![None], // index 0 is reserved for NULL
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
        let registered = RegisteredInstance {
            handle,
            instance_name: instance_id_str.to_string(),
            engine_name: engine_id_str.to_string(),
        };

        let idx = instance_id as usize;
        if idx >= self.id_to_registered.len() {
            self.id_to_registered.resize(idx + 1, None);
        }
        self.id_to_registered[idx] = Some(registered);

        handle
    }

    /// Checks if a handle can be resolved strictly against the registry and instance table.
    /// Strict validation: engine_id matches AND instance_id is registered AND descriptor is available.
    /// Zero heap allocation (O(1)).
    #[inline]
    pub fn resolve_handle(&self, handle: EngineHandle) -> bool {
        if handle.is_null() {
            return false;
        }

        let idx = handle.instance_id as usize;
        if let Some(Some(entry)) = self.id_to_registered.get(idx) {
            if entry.handle.engine_id != handle.engine_id {
                return false;
            }
            if let Some(desc) = EngineRegistry::get_descriptor(&entry.engine_name) {
                return desc.available;
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

    /// Retrieves an engine instance by its compact EngineHandle without String allocations.
    pub fn get_instance_by_handle(
        &mut self,
        handle: EngineHandle,
        context: &mut EngineContext,
        config_map: &HashMap<String, String>,
    ) -> Option<&mut Box<dyn Engine>> {
        let idx = handle.instance_id as usize;
        let entry = self.id_to_registered.get(idx)?.as_ref()?;
        if entry.handle.engine_id != handle.engine_id {
            return None;
        }
        let instance_name = entry.instance_name.clone();
        let engine_name = entry.engine_name.clone();

        self.get_instance(&instance_name, &engine_name, context, config_map)
    }

    /// Retrieves an already initialized engine instance by handle without reconfiguring in O(1).
    #[inline]
    pub fn get_active_instance(&mut self, handle: EngineHandle) -> Option<&mut Box<dyn Engine>> {
        let idx = handle.instance_id as usize;
        let entry = self.id_to_registered.get(idx)?.as_ref()?;
        if entry.handle.engine_id != handle.engine_id {
            return None;
        }
        self.instances.get_mut(&entry.instance_name)
    }

    /// Primary lazy-once factory and live configuration accessor.
    pub fn get_instance(
        &mut self,
        instance_id: &str,
        engine_id: &str,
        context: &mut EngineContext,
        config_map: &HashMap<String, String>,
    ) -> Option<&mut Box<dyn Engine>> {
        let cfg = HashConfig { data: config_map };
        if !self.instances.contains_key(instance_id) {
            // First use: create + initialize exactly once, then cache.
            if let Some(desc) = EngineRegistry::get_descriptor(engine_id) {
                if !desc.available {
                    return None;
                }
                let mut engine = (desc.factory)();
                let _ = engine.initialize(context, &cfg);
                self.instances.insert(instance_id.to_string(), engine);
                self.configs
                    .insert(instance_id.to_string(), config_map.clone());
            } else {
                return None;
            }
        } else {
            // Already alive: if the config changed since last time, hot-reload it in place.
            let changed = self
                .configs
                .get(instance_id)
                .map(|prev| prev != config_map)
                .unwrap_or(true);
            if changed {
                if let Some(engine) = self.instances.get_mut(instance_id) {
                    engine.on_config_changed(&cfg);
                }
                self.configs
                    .insert(instance_id.to_string(), config_map.clone());
            }
        }
        self.instances.get_mut(instance_id)
    }
}

use serde::Serialize;
use std::collections::HashMap;

// =======================================================
// 1. Enums & Errors
// =======================================================

#[derive(Debug, Clone, Serialize)]
pub enum EngineError {
    InvalidConfig(String),
    MissingResource(String),
    InitializationFailed(String),
    RenderFailed(String),
    HardwareUnavailable(String),
    RuntimeError(String),
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum ValidationPolicy {
    Clamp,
    FallbackDefault,
    Reject,
    Accept,
}

#[derive(Debug, Clone, Serialize)]
pub enum ConfigType {
    Boolean,
    Integer,
    Float,
    String,
    Options,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigOption {
    pub label: &'static str,
    pub value: &'static str,
}

// =======================================================
// 2. Metadata & Capabilities
// =======================================================

#[derive(Debug, Clone, Serialize)]
pub struct EngineMetadata {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub version: &'static str,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Capabilities {
    pub supports_128x32: bool,
    pub supports_256x64: bool,
    pub realtime: bool,
    pub interruptible: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Requirements {
    pub needs_audio: bool,
    pub needs_network: bool,
    pub needs_sd: bool,
}

// =======================================================
// 3. Configuration Schema
// =======================================================

#[derive(Debug, Clone, Serialize)]
pub struct ConfigField {
    pub id: &'static str,
    pub field_type: ConfigType,
    pub label: &'static str,
    pub description: &'static str,
    pub default_value: &'static str,
    pub required: bool,
    pub min_val: Option<&'static str>,
    pub max_val: Option<&'static str>,
    pub step: Option<&'static str>,
    pub options: Option<Vec<ConfigOption>>,
    pub visible_when: Option<&'static str>,
    pub options_endpoint: Option<&'static str>,
    pub multiple: bool,
    pub validation_policy: ValidationPolicy,
}

impl Default for ConfigField {
    fn default() -> Self {
        Self {
            id: "",
            field_type: ConfigType::String,
            label: "",
            description: "",
            default_value: "",
            required: false,
            min_val: None,
            max_val: None,
            step: None,
            options: None,
            visible_when: None,
            options_endpoint: None,
            multiple: false,
            validation_policy: ValidationPolicy::Accept,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigSchema {
    pub fields: Vec<ConfigField>,
}

// =======================================================
// 4. Context & Instance Config
// =======================================================

pub trait EngineConfig {
    fn get_string(&self, key: &str, default_val: &str) -> String;
    fn get_int(&self, key: &str, default_val: i32) -> i32;
    fn get_bool(&self, key: &str, default_val: bool) -> bool;
}

pub struct HashConfig<'a> {
    pub data: &'a HashMap<String, String>,
}

impl<'a> EngineConfig for HashConfig<'a> {
    fn get_string(&self, key: &str, default_val: &str) -> String {
        self.data
            .get(key)
            .cloned()
            .unwrap_or_else(|| default_val.to_string())
    }
    fn get_int(&self, key: &str, default_val: i32) -> i32 {
        self.data
            .get(key)
            .and_then(|v| v.parse().ok())
            .unwrap_or(default_val)
    }
    fn get_bool(&self, key: &str, default_val: bool) -> bool {
        self.data
            .get(key)
            .map(|v| v == "true" || v == "1")
            .unwrap_or(default_val)
    }
}

use crate::core::matrix::MatrixBackend;

pub struct EngineContext<'a> {
    pub matrix: &'a mut dyn MatrixBackend,
    pub config: &'a crate::core::config::Config,
}

// =======================================================
// 5. Engine Trait
// =======================================================

pub trait Engine: Send + Sync {
    fn initialize(
        &mut self,
        context: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError>;
    fn activate(&mut self);
    fn update(&mut self, context: &mut EngineContext);
    fn render(&mut self, context: &mut EngineContext);
    fn deactivate(&mut self);

    // Dynamic Configuration
    fn on_config_changed(&mut self, _config: &dyn EngineConfig) {}

    // Intrinsic sequence completion signaling
    fn is_finished(&self) -> bool {
        false
    }

    /// Whether the engine currently needs a high frame rate (~25fps). Unlike the
    /// static `Capabilities::realtime` descriptor flag, this is evaluated every
    /// frame so an engine can switch cadence based on its live state (e.g. a
    /// clock rendering an animated theme). Defaults to `false`.
    fn is_realtime(&self) -> bool {
        false
    }

    /// For engines whose rotation advance is driven by an intrinsic count/loop
    /// rather than wall-clock time (e.g. the GIF engine playing N clips before
    /// moving on), the runtime passes the rotation entry's numeric value here as
    /// a playback "budget". Time-based engines ignore it. Defaults to no-op.
    fn set_rotation_budget(&mut self, _budget: u32) {}

    /// Whether this engine drives its own rotation advance through
    /// `is_finished()` (count/loop based) and must therefore NOT be
    /// force-advanced by the rotation duration timer. Defaults to `false`.
    fn self_paced(&self) -> bool {
        false
    }
}

// =======================================================
// 6. Registry & Factory
// =======================================================

pub type EngineFactory = fn() -> Box<dyn Engine>;

#[derive(Serialize)]
pub struct EngineDescriptor {
    pub metadata: EngineMetadata,
    pub capabilities: Capabilities,
    pub requirements: Requirements,
    pub schema: ConfigSchema,
    #[serde(skip)]
    pub factory: EngineFactory,
}

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

// Basic map implementation for tests
pub struct HashConfig {
    pub data: HashMap<String, String>,
}

impl EngineConfig for HashConfig {
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
    // In Sprint 2, this will contain references to Matrix, Logger, etc.
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

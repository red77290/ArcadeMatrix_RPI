🇬🇧 English | 🇫🇷 [Français](DEVELOPER_FR.md) | 🇪🇸 [Español](DEVELOPER_ES.md)

# Developer Guide (Raspberry Pi - Rust)

Welcome to the ArcadeMatrix development guide for Raspberry Pi. This document explains how to extend the architecture and create new Engines in Rust.

---

## 1. Understanding the Architecture: Engines, Registry, and Lifecycle

ArcadeMatrix no longer has a hardcoded list of its features in `app.rs`. The system relies on a **Registry** (using the `linkme` crate) that discovers engines at startup.

### 1.1 The Strict Lifecycle (Lazy-Once)

To prevent screen tearing and jitter caused by Rust's memory allocator (Heap), ArcadeMatrix enforces a strict lifecycle for each `Engine` trait implementation.

```text
initialize()
    │
    ├── heap allocations via 'String' or 'Vec'
    ├── loading assets (images, fonts)
    ├── caching setup
    └── heavy initialization
          ↓
activate()
    │
    └── temporary state preparation (resetting timers, etc.)
          ↓
update()
    │
    └── real-time logic (60 FPS) - **NO UNNECESSARY DYNAMIC ALLOCATIONS**
          ↓
render()
    │
    └── real-time rendering (60 FPS) - **NO UNNECESSARY DYNAMIC ALLOCATIONS**
          ↓
deactivate()
    │
    └── freeing external resources or stopping listeners
```

- **Golden rule:** Never instantiate new dynamic `String` or `Vec` inside `update()` or `render()`. Pre-allocate your buffers in `initialize()` and mutate them in place (e.g. `my_string.clear()` then `write!(&mut my_string, "...")`).
- **`on_config_changed()`:** Allows the engine to update its internal state when the user changes settings via the Web UI (asynchronous).
- **`is_finished()`:** Useful to signal the `EngineRuntime` that an engine has finished its task to force moving to the next engine without waiting for the timeout.

---

## 2. Tutorial: Creating a New Engine

To create a new engine, you must implement the `Engine` trait and provide an `EngineDescriptor` via the Registry.

### Step 1: Create the structure (`src/engines/my_engine.rs`)

```rust
use crate::core::engine_contract::{Engine, EngineConfig, EngineContext, EngineError};
use crate::core::matrix::MatrixBackend;

pub struct MyEngine {
    my_setting: String,
    counter: u32,
}

impl MyEngine {
    pub fn new() -> Self {
        Self {
            my_setting: String::new(),
            counter: 0,
        }
    }
}
```

### Step 2: Implement the Lifecycle

```rust
impl Engine for MyEngine {
    fn initialize(
        &mut self,
        _context: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        // Safe place for allocations
        self.my_setting = config.get_string("my_setting", "default");
        println!("MyEngine initialized!");
        Ok(())
    }

    fn activate(&mut self) {
        self.counter = 0; // Quick reset
    }

    fn update(&mut self, _context: &mut EngineContext) {
        // Fast business logic, NO allocations
        self.counter += 1;
    }

    fn render(&mut self, context: &mut EngineContext) {
        // Hardware rendering via context.matrix
        context.matrix.clear();
        // Caution: drawing text creates no allocation if using existing buffers
    }

    fn deactivate(&mut self) {}

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        self.my_setting = config.get_string("my_setting", "default");
    }

    fn is_finished(&self) -> bool {
        false
    }
}
```

### Step 3: Register the Engine at Startup

Add the descriptor at the bottom of your file to expose configuration fields to the Web API:

```rust
use crate::core::engine_contract::{
    Capabilities, ConfigField, ConfigSchema, ConfigType, EngineDescriptor, EngineFactory,
    EngineMetadata, Requirements,
};
use linkme::distributed_slice;

#[distributed_slice(crate::core::registry::ENGINES)]
fn register_MyEngine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "my_engine",
            name: "My Custom Engine",
            category: "misc",
            version: "1.0",
        },
        capabilities: Capabilities::default(),
        requirements: Requirements::default(),
        schema: ConfigSchema {
            fields: vec![ConfigField {
                id: "my_setting",
                field_type: ConfigType::String,
                label: "My Setting",
                description: "Enter a word to display",
                default_value: "default",
                options: None,
                min_val: None,
                max_val: None,
                required: false,
                step: None,
                visible_when: None,
                options_endpoint: None,
                multiple: false,
                validation_policy: crate::core::engine_contract::ValidationPolicy::Accept,
            }],
        },
        factory: || Box::new(MyEngine::new()),
    }
}
```

### Step 4: Add module reference

Open `src/engines/mod.rs` and add:
```rust
pub mod my_engine;
```

That's it! **No `app.rs` code needs to be modified**. The engine will be automatically listed in the Web API, and its `config.json` configuration will be managed in an isolated way.

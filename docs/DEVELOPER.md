🇬🇧 English | 🇫🇷 [Français](DEVELOPER_FR.md) | 🇪🇸 [Español](DEVELOPER_ES.md)

# Developer Guide (Raspberry Pi - Rust)

This is the **complete** guide to extending ArcadeMatrix on Raspberry Pi. It explains the `Engine` contract in full, the entire `ConfigField` schema (including **dynamic/custom option lists**, multiselect, conditional fields and self-healing policies), and walks through building a new engine end-to-end.

> For the *why* behind the design (Registry, Lazy-Once, Arbiter, threading, overlay), read [ARCHITECTURE.md](ARCHITECTURE.md). This guide is the *how-to*.

---

## Table of Contents

1. [Mental Model](#1-mental-model)
2. [The Engine Trait in Full](#2-the-engine-trait-in-full)
3. [The Lifecycle & Golden Rules](#3-the-lifecycle--golden-rules)
4. [Capabilities & Requirements](#4-capabilities--requirements)
5. [The ConfigSchema & ConfigField Reference](#5-the-configschema--configfield-reference)
6. [Custom / Dynamic Option Lists](#6-custom--dynamic-option-lists)
7. [Multiselect Fields](#7-multiselect-fields)
8. [Conditional Fields (`visible_when`)](#8-conditional-fields-visible_when)
9. [Self-Healing Validation Policies](#9-self-healing-validation-policies)
10. [Tutorial: Create a New Engine](#10-tutorial-create-a-new-engine)
11. [Tutorial: Add a Custom-List Endpoint](#11-tutorial-add-a-custom-list-endpoint)
12. [Reading Config in an Engine](#12-reading-config-in-an-engine)
13. [Rendering into the Matrix](#13-rendering-into-the-matrix)
14. [Testing & Local Run](#14-testing--local-run)
15. [Checklist](#15-checklist)

---

## 1. Mental Model

ArcadeMatrix has **no hardcoded list of features** in `app.rs`. Each engine is a self-registering plugin discovered at startup through a compile-time Registry (`linkme`).

```mermaid
flowchart LR
    DEV["You write src/engines/my_engine.rs"] --> REGT["#distributed_slice registration"]
    REGT --> REG["EngineRegistry (auto-discovery)"]
    REG --> API["GET /api/engines"]
    API --> UI["Dynamic Web UI (auto form)"]
    REG --> RT["EngineRuntime (Lazy-Once)"]
    RT --> SCREEN["LED matrix"]
```

Adding an engine touches **two files**: the engine itself and one `pub mod` line in `src/engines/mod.rs`. **`app.rs` is never edited.**

---

## 2. The Engine Trait in Full

Every engine implements `core::engine_contract::Engine`:

```rust
pub trait Engine: Send + Sync {
    // --- Mandatory lifecycle ---
    fn initialize(&mut self, ctx: &mut EngineContext, config: &dyn EngineConfig)
        -> Result<(), EngineError>;
    fn activate(&mut self);
    fn update(&mut self, ctx: &mut EngineContext);
    fn render(&mut self, ctx: &mut EngineContext);
    fn deactivate(&mut self);

    // --- Optional (have default impls) ---
    fn on_config_changed(&mut self, _config: &dyn EngineConfig) {}
    fn is_finished(&self) -> bool { false }
    fn is_realtime(&self) -> bool { false }
    fn set_rotation_budget(&mut self, _budget: u32) {}
    fn self_paced(&self) -> bool { false }
}
```

| Method | Default | When to override |
| :-- | :-- | :-- |
| `initialize` | — | Always. Allocate buffers, load assets, read config once. |
| `activate` | — | Always. Cheap reset of transient state. |
| `update` | — | Always. Business logic each frame. |
| `render` | — | Always. Draw into `ctx.matrix`. |
| `deactivate` | — | Always. Stop timers/listeners. |
| `on_config_changed` | no-op | If your engine has editable settings (almost always). Re-read them **in place**. |
| `is_finished` | `false` | If the engine has an intrinsic end (e.g. finished a token list) and should advance the rotation early. |
| `is_realtime` | `false` | If the engine animates only under some live state and needs ~25 FPS then. |
| `set_rotation_budget` | no-op | If rotation advance is count-based (e.g. play N GIFs). Receives the entry's numeric value. |
| `self_paced` | `false` | If the engine drives its own advance via `is_finished` and must NOT be force-advanced by the duration timer. |

---

## 3. The Lifecycle & Golden Rules

```mermaid
stateDiagram-v2
    [*] --> Initialized : factory() + initialize() (once)
    Initialized --> Active : activate()
    Active --> Active : update() + render() (hot loop)
    Active --> Active : on_config_changed() (live edit)
    Active --> Standby : deactivate()
    Standby --> Active : activate()
```

- **Golden rule #1 — allocate once.** Never create a fresh `String`/`Vec` in `update()`/`render()`. Pre-allocate in `initialize()` and mutate in place:
  ```rust
  self.buf.clear();
  write!(&mut self.buf, "{}:{}", h, m).ok();
  ```
- **Golden rule #2 — hot-reload in place.** In `on_config_changed()` re-read values into existing fields. The instance is **not** recreated (Lazy-Once), so keep your allocations.
- **Golden rule #3 — no blocking I/O in the hot loop.** Network/disk work belongs in a background thread; hand results to `update()` via a channel or shared state.

---

## 4. Capabilities & Requirements

Declared in the descriptor, these are static metadata the runtime and UI read.

```rust
Capabilities {
    supports_128x32: bool,  // panel geometry hints
    supports_256x64: bool,
    realtime: bool,         // true -> polled at ~25 FPS; false -> 1 Hz
    interruptible: bool,    // may be pre-empted by a higher-priority source
}

Requirements {
    needs_audio: bool,
    needs_network: bool,    // engine calls out to the internet
    needs_sd: bool,
}
```

- Set `realtime: true` **only** if you draw a new frame every tick (GIF, scrolling text, Spotify). Static content (clock/weather) must stay `false` to save CPU and Wi-Fi.
- For dynamic cadence (animate sometimes), keep `realtime: false` and override `is_realtime()` to return `true` while animating.

---

## 5. The ConfigSchema & ConfigField Reference

The schema is the **single source of truth** for the UI and the sanitizer. Every field:

```rust
pub struct ConfigField {
    pub id: &'static str,                 // config key (stored in config.json)
    pub field_type: ConfigType,           // Boolean | Integer | Float | String | Options
    pub label: &'static str,              // UI label
    pub description: &'static str,        // UI tooltip
    pub default_value: &'static str,      // injected when missing (self-healing)
    pub required: bool,
    pub min_val: Option<&'static str>,    // numeric bound (Integer/Float)
    pub max_val: Option<&'static str>,
    pub step: Option<&'static str>,       // UI stepper granularity
    pub options: Option<Vec<ConfigOption>>, // static choices for Options
    pub visible_when: Option<&'static str>, // conditional visibility
    pub options_endpoint: Option<&'static str>, // dynamic choices (custom list)
    pub multiple: bool,                   // multiselect (CSV storage)
    pub validation_policy: ValidationPolicy, // Clamp | FallbackDefault | Reject | Accept
}
```

`ConfigType` variants:

| Variant | Widget | Sanitizer behaviour |
| :-- | :-- | :-- |
| `Boolean` | Enabled/Disabled select | normalizes `true/1/yes/on` → `true`, else default |
| `Integer` | number input | parse + clamp/fallback to `min_val..max_val` |
| `Float` | number input | parse + clamp/fallback to `min_val..max_val` |
| `String` | text input | accepted as-is |
| `Options` | dropdown (or checkbox grid if `multiple`) | value must be in `options` (unless dynamic) |

> **Tip:** all values are stored as strings in `config.json`. Parse them with the `EngineConfig` helpers (`get_int`, `get_bool`, `get_string`).

---

## 6. Custom / Dynamic Option Lists

Sometimes the choices are **not known at compile time** — the installed fonts, the GIF folders on disk, the available themes. Instead of a static `options` list, point the field at an **options endpoint**. The frontend fetches it live and builds the widget.

```mermaid
sequenceDiagram
    participant UI as dynamic_engines.js
    participant API as api-server
    participant SRC as filesystem / theme table
    UI->>API: GET /api/engines
    API-->>UI: schema (field has options_endpoint)
    UI->>API: GET {options_endpoint}
    API->>SRC: enumerate resources
    SRC-->>API: entries
    API-->>UI: [{value,label}, ...]
    UI->>UI: render dropdown / checkbox grid
```

Built-in endpoints (all return `[{ "value": ..., "label": ... }]`):

| `options_endpoint` | Serves | Backed by |
| :-- | :-- | :-- |
| `/api/fonts` | font filenames | files in `fonts/` (`.ttf`, `.bdf`) |
| `/api/playlists` | GIF folder names | sub-dirs of `gifs/` |
| `/api/themes` | theme id/name | `core::theme::all_themes()` |

Real example — the **clock** engine's theme and font fields:

```rust
ConfigField {
    id: "theme",
    field_type: ConfigType::Options,
    label: "Theme",
    description: "Color theme",
    default_value: "matrix",
    options: None,                          // no static list
    options_endpoint: Some("/api/themes"),  // fetched live
    ..Default::default()
},
ConfigField {
    id: "font",
    field_type: ConfigType::Options,
    label: "Font",
    description: "Bitmap or TTF font",
    default_value: "PressStart2P.ttf",
    options_endpoint: Some("/api/fonts"),
    ..Default::default()
},
```

Because the list is fetched at render time, **dropping a new font into `fonts/` or a new folder into `gifs/` appears in the UI immediately** — no rebuild, no schema change.

---

## 7. Multiselect Fields

Set `multiple: true` on an `Options` field (static or dynamic) to let the user pick **several** values. The UI renders a checkbox grid; the selection is stored as a **comma-separated string** in the instance config.

Real example — the **GIF** engine's playlist selection:

```rust
ConfigField {
    id: "playlists",
    field_type: ConfigType::Options,
    label: "GIF Playlists",
    description: "Which GIF folders to play",
    default_value: "",
    options_endpoint: Some("/api/playlists"),
    multiple: true,                 // -> checkbox grid, CSV storage
    ..Default::default()
}
```

Stored as e.g. `"mario,zelda,sonic"`. In your engine, split it:

```rust
let selected: Vec<String> = config
    .get_string("playlists", "")
    .split(',')
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .map(String::from)
    .collect();
```

The sanitizer validates each token against the allowed set (for static options) and leaves dynamic-endpoint values untouched. This replaces the old "hardcode which GIFs to include/ignore" approach with an explicit, user-driven, declarative selection.

---

## 8. Conditional Fields (`visible_when`)

`visible_when` lets a field appear only when another field has a given state, so you can build dependent forms without engine-specific JavaScript. Set it to the id of the controlling field; the frontend shows the field conditionally.

```rust
ConfigField {
    id: "scroll_speed",
    field_type: ConfigType::Integer,
    label: "Scroll Speed",
    visible_when: Some("animated"), // only shown when the "animated" field is on
    ..Default::default()
}
```

---

## 9. Self-Healing Validation Policies

`validation_policy` decides what the `ConfigSanitizer` does with an out-of-range or unparseable value on boot / on save.

```mermaid
flowchart TD
    V["stored value"] --> P{valid?}
    P -->|"yes"| KEEP["keep"]
    P -->|"no (out of range)"| POL{validation_policy}
    POL -->|"Clamp"| C["clamp to min/max"]
    POL -->|"FallbackDefault"| F["reset to default_value"]
    POL -->|"Reject"| R["leave as-is (engine must cope)"]
    POL -->|"Accept"| A["leave as-is"]
    P -->|"no (unparseable number)"| PF{FallbackDefault?}
    PF -->|"yes"| F
    PF -->|"no"| A
```

| Policy | Out-of-range number | Unparseable number | Bad option value |
| :-- | :-- | :-- | :-- |
| `Clamp` | clamp to bound | left as-is | — |
| `FallbackDefault` | reset to default | reset to default | reset to default |
| `Reject` | left as-is | left as-is | — |
| `Accept` | left as-is | left as-is | — |

Missing keys are always **injected** with `default_value`; keys not in the schema are **pruned**. This is what makes OTA upgrades seamless (new fields appear, removed fields disappear).

---

## 10. Tutorial: Create a New Engine

### Step 1 — the struct (`src/engines/my_engine.rs`)

```rust
use crate::core::engine_contract::{Engine, EngineConfig, EngineContext, EngineError};

pub struct MyEngine {
    my_setting: String, // pre-allocated buffer
    counter: u32,
}

impl MyEngine {
    pub fn new() -> Self {
        Self { my_setting: String::new(), counter: 0 }
    }
}
```

### Step 2 — implement the lifecycle

```rust
impl Engine for MyEngine {
    fn initialize(&mut self, _ctx: &mut EngineContext, config: &dyn EngineConfig)
        -> Result<(), EngineError> {
        self.my_setting = config.get_string("my_setting", "default"); // alloc OK here
        Ok(())
    }

    fn activate(&mut self) { self.counter = 0; }

    fn update(&mut self, _ctx: &mut EngineContext) {
        self.counter += 1; // no allocation
    }

    fn render(&mut self, ctx: &mut EngineContext) {
        ctx.matrix.clear();
        // draw self.my_setting using existing buffers
    }

    fn deactivate(&mut self) {}

    fn on_config_changed(&mut self, config: &dyn EngineConfig) {
        self.my_setting = config.get_string("my_setting", "default"); // in place
    }
}
```

### Step 3 — register with a descriptor (self-discovery)

```rust
use crate::core::engine_contract::{
    Capabilities, ConfigField, ConfigSchema, ConfigType, EngineDescriptor,
    EngineMetadata, Requirements, ValidationPolicy,
};
use linkme::distributed_slice;

#[distributed_slice(crate::core::registry::ENGINES)]
fn register_my_engine() -> EngineDescriptor {
    EngineDescriptor {
        metadata: EngineMetadata {
            id: "my_engine",
            name: "My Custom Engine",
            category: "misc",
            version: "1.0",
        },
        capabilities: Capabilities::default(), // set realtime:true if you animate
        requirements: Requirements::default(),
        schema: ConfigSchema {
            fields: vec![ConfigField {
                id: "my_setting",
                field_type: ConfigType::String,
                label: "My Setting",
                description: "Text to display",
                default_value: "default",
                validation_policy: ValidationPolicy::Accept,
                ..Default::default() // use struct-update syntax for the rest
            }],
        },
        factory: || Box::new(MyEngine::new()),
    }
}
```

> Using `..Default::default()` keeps registrations short — you only spell out the fields that matter.

### Step 4 — expose the module (`src/engines/mod.rs`)

```rust
pub mod my_engine;
```

Done. The engine now appears in `GET /api/engines`, gets an auto-generated form in the Web UI, and its config is sanitized and hot-reloaded automatically. **No `app.rs` change.**

```mermaid
flowchart LR
    A["1. struct"] --> B["2. impl Engine"]
    B --> C["3. #distributed_slice descriptor"]
    C --> D["4. pub mod in engines/mod.rs"]
    D --> E["Auto: API + UI + sanitizer + rotation"]
```

---

## 11. Tutorial: Add a Custom-List Endpoint

If your field needs choices from a resource the user manages (files, playlists, presets), add an options endpoint and point a field at it.

### Step 1 — the handler (`src/api/server.rs`)

```rust
#[get("/api/presets")]
async fn get_presets(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if let Err(e) = check_auth(&req, &data.config) { return e; }
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir("presets") {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                out.push(json!({ "value": name, "label": name }));
            }
        }
    }
    HttpResponse::Ok().json(out)
}
```

Register it with the other services in the actix `App` builder.

### Step 2 — point a field at it

```rust
ConfigField {
    id: "preset",
    field_type: ConfigType::Options,
    label: "Preset",
    options_endpoint: Some("/api/presets"),
    // multiple: true, // uncomment for a checkbox grid
    ..Default::default()
}
```

The frontend needs **no** changes — `dynamic_engines.js` already fetches any `options_endpoint` and renders a dropdown (or checkbox grid when `multiple`).

---

## 12. Reading Config in an Engine

The engine receives a restricted `&dyn EngineConfig` proxy (never the whole `config.json`):

```rust
let interval = config.get_int("interval", 10);      // parsed i32
let enabled  = config.get_bool("enabled", true);    // true/1
let label    = config.get_string("label", "Hello"); // owned String
```

These map onto the instance's `HashMap<String,String>`. Keys correspond to your schema `id`s.

---

## 13. Rendering into the Matrix

`ctx.matrix` is a `&mut dyn MatrixBackend`. Typical pattern:

```rust
fn render(&mut self, ctx: &mut EngineContext) {
    ctx.matrix.clear();
    // draw pixels / text / bitmaps into ctx.matrix
    // do NOT call ctx.matrix.update() — the render loop flushes the frame
}
```

The **render loop** owns `update()` (the flush to the panel) and, after your `render()` returns, may run the additive **Fighter overlay** pass on top of your frame (see [ARCHITECTURE.md §11](ARCHITECTURE.md#11-the-fighter-overlay-compositor)).

---

## 14. Testing & Local Run

```bash
rtk cargo fmt
rtk cargo test          # unit + integration tests
rtk cargo build --release
```

- Unit-test pure logic (parsers, formatting) directly in the engine module (`#[cfg(test)]`).
- The mock matrix (`tests/test_matrix.rs`) lets you assert pixels without hardware.
- The registry test (`tests/test_registry.rs`) checks discovery, descriptors, and the runtime lifecycle — a good template for engine tests.

The pre-commit hook runs the release validator, doc/config-key validator, `cargo fmt --check`, and the full test suite.

---

## 15. Checklist

- [ ] Struct pre-allocates buffers; no allocation in `update`/`render`.
- [ ] `on_config_changed` re-reads every editable field **in place**.
- [ ] `Capabilities.realtime` reflects whether you animate every frame (or override `is_realtime`).
- [ ] Every schema field has a sensible `default_value` and `validation_policy`.
- [ ] Dynamic choices use `options_endpoint`; multi-value uses `multiple: true` (CSV).
- [ ] Registered via `#[distributed_slice]`; module added to `engines/mod.rs`.
- [ ] `app.rs` untouched.
- [ ] `cargo fmt`, `cargo test`, `cargo build --release` all pass.

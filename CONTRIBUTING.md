🇬🇧 English | 🇫🇷 [Français](CONTRIBUTING_FR.md) | 🇪🇸 [Español](CONTRIBUTING_ES.md)

# Contributing to ArcadeMatrix

Welcome to ArcadeMatrix! Whether you're fixing bugs, adding new animations, or bringing ArcadeMatrix to a new platform, we're glad you're here.

This document outlines the philosophy, architecture, and coding conventions of the project to ensure a robust, maintainable codebase as the project scales.

## Project Philosophy

The ArcadeMatrix project is divided into three distinct evolutionary phases:
1. **Prototype**: Getting the display to work.
2. **Product**: Adding features (GIFs, weather, clock, WebUI, MQTT...).
3. **Framework**: Stabilizing architecture, testing, and making it extensible.

We are currently in the **Framework** phase. Any new contribution must adhere to strict separation of concerns, avoiding "spaghetti code" and monolithic engines.

## Separation of Concerns: The Rendering Pipeline

We use a modern Rendering Pipeline to handle what is drawn to the matrix. If you want to add a new visual effect or a new way to display the time/date, please follow this flow:

`Data -> Engine -> Animation -> Renderer -> Matrix`

### Engines vs. Renderers

* **Engine (`engines/`)**: Responsible for data acquisition, state management, and business logic.
  * *Example*: `ClockEngine` knows *what time it is* and *when to rotate*. It does **not** know how to draw a shrinking white box.
  * *Example*: `WeatherEngine` knows how to fetch the API and parse the JSON.
* **Renderer (`engines/renderers/`)**: Responsible for drawing pixels to the frame. It has NO business logic. It takes raw data strings, fonts, and colors, and returns an image.
  * *Example*: `CyberpunkRenderer` knows how to draw falling green digital rain.
  * *Example*: `FlipRenderer` knows how to calculate bounding boxes and draw shrinking panels.

**Rule of Thumb:**
If you are adding a new visual theme that uses the *same data* (like a new clock face), create a **Renderer** (or a specialized Clock like PongClock which acts as a renderer).
If you are adding a completely new feature (like fetching stock prices or Spotify now playing), create an **Engine**.

## Code Conventions

* **Language**: The core repository uses Rust for the Raspberry Pi.
* **Typing**: Make full use of Rust's strong static typing and Traits to clarify Engine/Renderer contracts.
* **Testing**: All API routes and Core configuration logic must be covered by `cargo test`.
* **Hardware Independence**: Do not assume the matrix is exactly 64x32. Read the panel size from `MatrixConfig` (`matrix.width` / `matrix.height`) and declare the resolutions you support via the descriptor's `Capabilities` (`supports_128x32` / `supports_256x64`).

## The Engine Architecture (Registry / Descriptor / Factory)

The Core is **engine-agnostic**: it never names `Clock`, `Weather` or `Spotify`
directly. Each engine is a self-described plugin discovered at runtime:

```
Engine
 ├── Descriptor  (metadata + Capabilities + Requirements)
 ├── ConfigSchema (fields, types, defaults, min/max, options, options_endpoint)
 ├── Factory     (lazy construction, built once and cached)
 └── Lifecycle   (initialize → activate → update/render → deactivate)
```

- Engines are configured as **generic instances** (`instance_id` + `engine_id` +
  a `config` string map), not as hardcoded types.
- The Web UI is generated from `GET /api/engines`, so a new engine appears in
  the UI automatically once its `ConfigSchema` is declared — no frontend change.
- Config edits reach a running engine live through `on_config_changed()`; the
  config is self-healed (defaults injected, out-of-range values clamped or
  reset) before being persisted.

The full, authoritative walkthrough lives in
[`docs/DEVELOPER.md`](docs/DEVELOPER.md).

## Adding a New Engine

1. Create `src/engines/my_engine.rs` implementing the `Engine` contract
   (`initialize` / `activate` / `update` / `render` / `deactivate`, plus
   `is_finished` and `on_config_changed` as needed).
2. Provide an `EngineDescriptor`: metadata, `Capabilities` (set `realtime: true`
   only if it must update every frame), `Requirements`, and a `ConfigSchema`.
3. Register it in the registry via the `#[distributed_slice(ENGINES)]` factory
   entry so auto-discovery and the Web UI pick it up.
4. Add coverage in `tests/` (see `tests/test_registry.rs` and
   `tests/test_sanitizer.rs`).

## Adding a New Renderer

If you only need a new *look* for existing data (e.g. a new clock face), add a
**Renderer** instead of an Engine:
1. Create a new file in `src/engines/renderers/my_custom_renderer.rs`.
2. Implement the `Renderer` trait.
3. Register it in `src/engines/renderers/mod.rs`.

## Architecture Decision Records (ADR)

If you propose a major architectural change, please write an ADR in `docs/adr/`. Check existing ADRs to understand why certain design decisions (like avoiding multi-threading for rendering) were made.

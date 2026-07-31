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
* **Hardware Independence**: Do not assume the matrix is exactly 64x32. Always use `self.config.matrix_width` and `self.config.matrix_height`.

## Adding a New Renderer

*Note: The exact process is currently being updated for the Rust architecture.*
1. Create a new file in `src/engines/renderers/my_custom_renderer.rs`.
2. Implement the `Renderer` trait.
3. Register it in `src/engines/renderers/mod.rs`.

## Architecture Decision Records (ADR)

If you propose a major architectural change, please write an ADR in `docs/adr/`. Check existing ADRs to understand why certain design decisions (like avoiding multi-threading for rendering) were made.

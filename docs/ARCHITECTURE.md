🇬🇧 English | 🇫🇷 [Français](ARCHITECTURE_FR.md) | 🇪🇸 [Español](ARCHITECTURE_ES.md)

# Architecture Overview (Raspberry Pi - Rust)

This document provides a detailed overview of the ArcadeMatrix architecture on Raspberry Pi developed in **Rust**. It explains the deep design choices, memory strategy, rendering pipeline, and the "Lazy-Once" lifecycle of the engines.

---

## 1. Design Philosophy: Performance and "Jitter"

Unlike the ESP32, the Raspberry Pi has abundant RAM (512 MB to 8 GB). However, its operating system is not "Real-Time" (RTOS). The matrix driver (via DMA/GPIO) is extremely sensitive to micro-stutters ("jitter").

To maintain a stable 60 FPS refresh rate without screen tearing, **the hot loop (`update()` and `render()`) must not generate any unnecessary dynamic allocations**. Allocations cause heap cleaning or resizing work that can introduce unpredictable latency of a few milliseconds, which is enough to make the LED matrix flicker.

---

## 2. The "Lazy-Once" Lifecycle

To meet this constraint, the architecture relies on a very strict lifecycle model called **Lazy-Once**.

```mermaid
graph TD
                 Registry[Engine Registry]
                       │
                 Descriptor[EngineDescriptor]
                       │
                    Factory[Factory]
                       │
                 Instance[EngineInstance]
                       │
              ┌────────┴────────┐
              │                 │
        Context[EngineContext] Config[EngineConfig]
              │                 │
              └────────┬────────┘
                       │
                 Runtime[Engine Runtime]
                       │
          ┌────────────┼────────────┐
          │            │            │
       activate      update       render
          │            │            │
          └────────────┼────────────┘
                       │
                  deactivate
```

### Phase Explanation:

1. **`initialize()` (Allocation):**
   * **When?** Called *exactly once* in the entire life of the program, the very first time the engine needs to be displayed ("Lazy" instantiation).
   * **Why?** Avoids loading assets (images, fonts) into RAM for engines that the user has disabled in the configuration. This is where bitmaps are loaded and the playing field is prepared.
2. **`activate()` (Temporary Preparation):**
   * **When?** Called every time the engine becomes the "active" engine on screen.
   * **Why?** Allows resetting state (e.g., putting the Pong ball back in the center, or restarting a stopwatch) without having to reallocate memory.
3. **`update()` & `render()` (Hot Loop - 60 FPS):**
   * **Constraint:** **No unnecessary dynamic allocation.** The required memory (String, Vec) must have been reserved in `initialize` or reused (e.g., `String::clear()` then `write!()` instead of allocating new strings).
4. **`deactivate()` (Standby):**
   * Allows stopping heavy background tasks when the engine is no longer on screen.
5. **`is_finished()` (Conditional Jump):**
   * Allows the engine to signal to the rotation `Runtime` that it has finished its task (e.g., the Crypto Engine has finished displaying all its tokens).

---

## 3. Decoupling: Registry and Configuration

### Why doesn't the Core contain a list of concrete types?
In previous versions, `app.rs` manually included all clock files and created a huge `match` block with `Box::new(ClockEngine)`. This broke the open/closed principle (SOLID): adding an engine required modifying the core of the application.
Thanks to the **Registry** (based on the `#[distributed_slice]` macro), each engine registers itself autonomously during compilation. The application Core is completely unaware of the existence of concrete engines.

### Why does the Registry contain descriptors rather than instances?
Immediate instantiation of all engines at startup (`Box::new(...)`) would unnecessarily consume RAM and slow down boot time. Instead, the descriptor stores a **Factory** (a pointer function creating the instance on the fly) and the required metadata.

### Why separate `config.json` and `EngineConfig`?
The root file (`config.json`) describes the entire device (WiFi, Matrix, etc.). However, engines do not need — and should not have access to — WiFi or other engines' configuration. `EngineConfig` acts as a restricted view or proxy providing only the variables declared by the engine via its `ConfigSchema`.

### How does a config change reach a running engine?
Because instances are cached (Lazy-Once), a config edit must be actively pushed to the live engine rather than recreating it. The propagation chain is fully wired end to end:

```text
POST /api/instances        (api-server thread)
        │  validates engine_id, self-heals via ConfigSanitizer, saves config.json
        ▼
reset_rotation / reload_flag  (AtomicBool)
        │  read by the render thread on the next frame
        ▼
EngineRuntime.get_instance()  detects the config snapshot changed
        │
        ▼
engine.on_config_changed()   (same instance, no reallocation)
```

* **Instance edits** are applied **live** (`on_config_changed`) with no restart and no reallocation.
* **Hardware/network changes** (matrix, `disable_internal`, ...) set `reload_flag`, which the render loop honours by restarting the process cleanly so the driver re-initializes.
* The render cadence itself is chosen from the engine descriptor's `Capabilities.realtime` flag (≈25 FPS for animated engines, 1 Hz for static ones), never from a hardcoded engine name.

---

## 4. Runtime Isolation & Threading Model

ArcadeMatrix relies on a multi-threaded architecture to isolate hardware rendering from network operations:

1. **Dedicated Rendering Thread (`matrix-render`):**
   - Runs in a dedicated OS thread with an 8 MB stack.
   - Exclusive access to the LED matrix. If it were combined with the Web API, every HTTP request would cause a frame skip (tearing) on the matrix.

2. **Isolated Web API Thread (`api-server`):**
   - Runs on a single-threaded Tokio runtime (`Builder::new_current_thread()`).
   - Manages configuration via the web interface (port 80). Communicates with the rendering thread only via atomic primitives (`AtomicBool`) or short-lived asynchronous locks (`RwLock`).

3. **Background Services:**
   - **MQTT Listener / HTTP APIs:** Isolated to never block frame computation (`update()`).

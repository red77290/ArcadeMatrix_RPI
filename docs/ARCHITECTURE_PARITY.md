# Architecture Parity Matrix: ESP32 (V1.6-FinalLocked) ↔ Raspberry Pi

This document defines the formal architectural contract between **ArcadeMatrix (ESP32 C++)** and **ArcadeMatrix_RPi (Raspberry Pi Rust)**.

---

## 1. Architectural Parity Definition

```text
Architectural Parity (STRICT)
        │
        ├── same ownership boundaries
        ├── same data flow pipeline
        ├── same lifecycle & FSM
        ├── same arbitration & priority model
        ├── same intent identity contract
        ├── same geometry abstraction
        └── same hot-path zero-allocation invariants

Implementation Parity (PROHIBITED)
        │
        └── Native platform idioms (C++/FreeRTOS vs Rust/Linux threads) are preserved

Feature Parity (OUT OF SCOPE)
        │
        └── Hardware-specific peripherals (Audio, SHTC3, Gyro) remain excluded on RPi
```

---

## 2. Canonical Data Pipeline

```text
Producer
    │
    ▼
DisplayRequest (Copyable Value Type)
    │
    ▼
DisplayArbiter (Stateless, Bounded Array[8])
    │
    ▼
DisplayDecision (Stateless Winning Intent)
    │
    ▼
DisplayRuntime (FSM, Session Ownership, PreemptionStack<4>)
    │
    ▼
EngineRuntime (Handle Resolution, Lazy-Once Factory)
    │
    ▼
Engine Instance (update / render)
    │
    ▼
Base Framebuffer (MatrixBackend)
    │
    ▼
OverlayManager (Additive Compositing, e.g. Fighter)
    │
    ▼
Display Output (LED Matrix)
```

---

## 3. Detailed Component Parity Matrix

| Architectural Contract | ESP32 Reference (V1.6-FinalLocked) | Raspberry Pi Implementation | Parity Status |
| :--- | :--- | :--- | :--- |
| **Pipeline Direction** | `Producer → Request → Arbiter → Decision → Runtime → Engine → Framebuffer → Overlay` | `Producer → Request → Arbiter → Decision → Runtime → Engine → Framebuffer → Overlay` | ✅ **100% Identical** |
| **`EngineHandle`** | POD 4 bytes (`descriptorId: u16`, `instanceId: u16`) | Compact value type (`engine_id: u16`, `instance_id: u16`) | ✅ **100% Identical** |
| **`DisplayRequest`** | Value type (source, id, handle, priority, lifecycle, duration, created_at) | Value type (source_id, request_id, engine_handle, priority, lifecycle, duration_ms, created_at) | ✅ **100% Identical** |
| **`DisplayDecision`** | Winning intent without state or engine pointer | Winning intent without state or engine pointer | ✅ **100% Identical** |
| **DisplayArbiter** | Bounded array (8 slots), $O(\text{MAX})$, zero allocation, stateless | Bounded array (8 slots), $O(\text{MAX})$, zero allocation, stateless | ✅ **100% Identical** |
| **Intent Identity** | `source_id + request_id + engine_handle` | `source_id + request_id + engine_handle` | ✅ **100% Identical** |
| **Arbiter Saturation** | Evict lowest priority if incoming is strictly higher, else reject | Evict lowest priority if incoming is strictly higher, else reject | ✅ **100% Identical** |
| **DisplayRuntime** | Exclusive owner of active session, FSM, and lifecycle orchestration | Exclusive owner of active session, FSM, and lifecycle orchestration | ✅ **100% Identical** |
| **Transition FSM** | `NONE`, `REPLACE`, `PREEMPT`, `RESUME`, `REFRESH` (internal) | `NONE`, `REPLACE`, `PREEMPT`, `RESUME`, `REFRESH` (internal) | ✅ **100% Identical** |
| **`PreemptionStack`** | Bounded fixed capacity 4 (`PreemptionEntry` PODs) | Bounded fixed capacity 4 (`PreemptionEntry` PODs) | ✅ **100% Identical** |
| **Preemption Transactionality** | Target resolved & capacity checked BEFORE pause/push | Target resolved & capacity checked BEFORE pause/push | ✅ **100% Identical** |
| **Stack Saturation** | Rejection at `depth == 4` leaving active session intact | Rejection at `depth == 4` leaving active session intact | ✅ **100% Identical** |
| **Resume Validation** | Exact match on `source + request_id + handle` | Exact match on `source + request_id + handle` | ✅ **100% Identical** |
| **Orphan Cleanup** | Submerged cancelled/expired entries purged during unwinding | Submerged cancelled/expired entries purged during unwinding | ✅ **100% Identical** |
| **`RotationManager`** | Producer only, does NOT own active session or lifecycle | Producer only, does NOT own active session or lifecycle | ✅ **100% Identical** |
| **`OverlayManager`** | Additive compositor outside Arbiter (Fighter) | Additive compositor outside Arbiter (Fighter) | ✅ **100% Identical** |
| **`OrientationManager`** | Source abstraction (IMU) $\to$ `DisplayGeometry` | Source abstraction (Manual/API) $\to$ `DisplayGeometry` | ✅ **100% Identical** |
| **`DisplayGeometry`** | Versioned logical/physical dimensions & `LayoutClass` | Versioned logical/physical dimensions & `LayoutClass` | ✅ **100% Identical** |
| **Hot-Path Zero Allocation** | Decision/orchestration layer (H-Core) has zero heap allocation | Decision/orchestration layer (H-Core) has zero heap allocation | ✅ **100% Identical** |

---

## 4. Platform-Specific Implementations & Explicit Exclusions

### Platform Implementations (Permitted)
* **Concurrency**: ESP32 uses FreeRTOS tasks (Core 0 network/API, Core 1 render loop) with triple-buffer atomic CAS; RPi uses dedicated OS threads (`api-server`, `matrix-render`) with `RuntimeSnapshot` and atomic signals.
* **Backend**: ESP32 uses `ESP32-HUB75-MatrixPanel-I2S-DMA`; RPi uses `rpi-rgb-led-matrix` (hzeller).
* **RPi Features**: Spotify, Google Cast, Linux sysinfo/SoC temperature, dynamic font scaling.

### Hardware-Excluded Features (Out of Scope for RPi)
* ESP32 Audio Pipeline (`VisualizerEngine`, `MusicEngine`, `DecibelEngine`, `AudioHub`, `AudioSession`, `AirPlay`, `DLNA`, `Bluetooth Audio`)
* `SHTC3` Ambient Temperature (RPi reports SoC CPU/GPU temperature or weather temperature)
* `QMI8658` 6-axis IMU (RPi uses manual / API orientation setting producing the exact same `DisplayGeometry` contract).

# Architecture Parity: ESP32 Core 1 vs RPi H-Core

This document formalizes the normative architectural parity and real-time guarantees shared between **ArcadeMatrix (ESP32 / C++)** and **ArcadeMatrix_RPi (Raspberry Pi / Rust)**.

---

## 1. Executive Summary & Design Invariants

Both platforms adhere to the **V1.6-FinalLocked** execution model, enforcing strict separation between the **Control Plane (Core 0 / Async Tokio)** and the **Realtime Render Plane (Core 1 / H-Core Thread)**.

| Architectural Invariant | ESP32 Implementation (C++) | RPi Implementation (Rust) |
| :--- | :--- | :--- |
| **Hot-Path Memory Allocation** | **Strictly Zero-Allocation** (`malloc`, `new`, `std::vector`, `String` prohibited) | **Strictly Zero-Allocation** (`Vec::push`, `String`, `format!`, `serde_json`, `HashMap` lookups prohibited) |
| Architectural Invariant | ESP32 Implementation (C++) | RPi Implementation (Rust) |
| :--- | :--- | :--- |
| **Hot-Path Memory Allocation** | **Strictly Zero-Allocation** (`malloc`, `new`, `std::vector`, `String` prohibited) | **Strictly Zero-Allocation** (`Vec::push`, `String`, `format!`, `serde_json`, `HashMap` lookups prohibited) |
| **Hot-Path Lock Freedom** | **Zero Mutex** on Core 1 hot-path. SRSW Triple Buffer lock-free CAS. | **Zero Mutex** on H-Core render loop. Lock-free `ArcSwap<ProducerSnapshot>` atomic publication boundary. |
| **Intent Identity** | `source_id + request_id + engine_handle` | `source_id + request_id + engine_handle` (Uniform for all sources, zero exception) |
| **Arbiter Slot Model** | 8 private slots, $O(1)$ SPSC queue consumption | 8 bounded slots, $O(1)$ linear scan |
| **Preemption Stack** | Fixed `PreemptionStack<PreemptionEntry, 4>` | Bounded `[PreemptionEntry; 4]`, `depth: usize` |
| **Preemption Saturation** | Strict rejection at `depth == 4` (active session preserved) | Strict rejection at `depth == 4` (active session preserved) |
| **Handle Resolution** | $O(1)$ POD `EngineHandle` (uint16_t descriptorId, uint16_t instanceId) | $O(1)$ direct index into `Vec<Option<RegisteredInstance>>` |

---

## 2. Intent Identity & Lifecycle State Machine

A display request intent is uniquely identified by the 3-tuple:
$$\text{Intent} = (\text{source\_id}, \text{request\_id}, \text{engine\_handle})$$

### Truth Table & Transition Matrix

| Scenario | Arbiter Decision | Active Session State | Resulting `TransitionMode` | FSM Action |
| :--- | :--- | :--- | :--- | :--- |
| **Baseline Rotation** | `Rotation` (P10) | None | `TransitionMode::Replace` | `activate(rot)` |
| **Higher Priority Preemption** | `Mqtt` (P40, preemptive) | `Rotation` (P10) | `TransitionMode::Preempt` | `pause(rot) -> push(rot) -> activate(mqtt)` |
| **Stack Saturation** | `Mqtt` (P60, preemptive) | `depth == 4` | `TransitionMode::None` | Request rejected; active session continues |
| **Same Intent Refresh** | Same `(source, req_id, handle)` | Active | `TransitionMode::None` | Internal NOOP |
| **Internal Payload Update** | Same `source` & `handle`, new `req_id` | Active | `TransitionMode::None` | Internal update without pushing to preemption stack |
| **Preemption End (Valid Baseline)** | `Rotation` (P10) | `Mqtt` active, stack has `Rotation` | `TransitionMode::Resume` | `deactivate(mqtt) -> pop() -> resume(rot)` |
| **Submerged Orphan Expiry** | Intermediate session cancelled while submerged | Multi-level stack | `TransitionMode::Resume` | Pops expired intermediate sessions, resumes valid ancestor directly |
| **Stack Exhaustion** | None (`DisplayDecision::NONE`) | Empty stack | `TransitionMode::None` | `deactivate()` all, returns `None` |
| **Invalid Engine Handle** | Unknown `handle` | Any | `TransitionMode::None` | Transaction rejected; active session intact |

---

## 3. Realtime Boundary (Zero-Allocation & Zero-Mutex)

### 3.1 Control Plane (Async Tokio / Core 0)
- Responsible for HTTP REST API, WebSocket subscriptions, MQTT ingestion, and JSON configuration validation.
- Pre-parses incoming payloads into strongly-typed structures (e.g., `MessagePayload`).
- Publishes changes atomically to shared memory via `arc_swap::ArcSwap<ProducerSnapshot>` with zero lock acquisition on the realtime thread.

### 3.2 Realtime Render Plane (H-Core / Core 1)
- Consumes pre-parsed payloads and configuration snapshots via lock-free atomic pointer loads (`producer_snapshot.load()`).
- Zero mutex locks on the entire render loop hot-path.
- Executes `DisplayArbiter::evaluate()` in $O(1)$ time complexity without dynamic allocations.
- Resolves engine handles in $O(1)$ time via indexed vector offsets (`handle.instance_id as usize`).
- Dispatches lifecycle transitions (`activate`, `deactivate`, `pause`, `resume`) and invokes `update()` & `render()`.
- Verified by instrumented `GlobalAlloc` unit tests: **0 heap allocations over 10,000 steady-state iterations**.

---

## 4. Intentional Platform Differences

| Dimension | ESP32 (Embedded FreeRTOS / C++) | Raspberry Pi (Linux Embedded / Rust) | Rationale |
| :--- | :--- | :--- | :--- |
| **Matrix Hardware Driver** | ESP32 I2S parallel DMA / ESP32-S3 LCD peripheral | `rpi-rgb-led-matrix` C bindings / Mock backend | RPi utilizes direct GPIO memory mapping with hardware PWM timers |
| **Plugin Registration** | C++ Linker sets (`ENGINE_REGISTER`) | `linkme::distributed_slice` (`ENGINES`) | Platform idiomatic compile-time plugin registration |
| **Threading Model** | Dual-core FreeRTOS task pinning (`xTaskCreatePinnedToCore`) | OS threads: Tokio runtime + Realtime render thread (`std::thread::spawn`) | Standard POSIX concurrency model on Linux |
| **Configuration Triple Buffer** | Lock-free SRSW triple-buffer with atomics & double magic CRC | Pre-built immutable snapshots & `ArcSwap`/atomic flags | Leverages Rust memory safety while guaranteeing non-blocking reader execution |

---

## 5. Automated Verification & Compliance Test Suite

The normative compliance suite is codified in:
1. `tests/test_esp_alignment.rs`: Validates behavior parity with the ESP32 reference architecture.
2. `tests/test_v16_final_locked.rs`: Implements an instrumented `TrackingAllocator` providing mechanical proof of 0 heap allocations on the hot-path, comprehensive FSM matrix verification, bounded stack saturation, and registry safety.

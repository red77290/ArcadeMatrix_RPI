🇬🇧 [English](ARCHITECTURE_PARITY.md) | 🇫🇷 [Français](ARCHITECTURE_PARITY_FR.md) | 🇪🇸 Español

# Paridad Arquitectónica: ESP32 Core 1 vs RPi H-Core

Este documento formaliza la paridad arquitectónica normativa y las garantías en tiempo real compartidas entre **ArcadeMatrix (ESP32 / C++)** y **ArcadeMatrix_RPi (Raspberry Pi / Rust)**.

---

## 1. Resumen Ejecutivo e Invariantes de Diseño

Ambas plataformas se adhieren al modelo de ejecución **V1.6-FinalLocked**, imponiendo una separación estricta entre el **Plano de Control (Core 0 / Async Tokio)** y el **Plano de Renderizado en Tiempo Real (Core 1 / Hilo H-Core)**.

| Invariante Arquitectónico | Implementación ESP32 (C++) | Implementación RPi (Rust) |
| :--- | :--- | :--- |
| **Asignación de Memoria Hot-Path** | **Estrictamente Cero Asignaciones** (`malloc`, `new`, `std::vector`, `String` prohibidos) | **Estrictamente Cero Asignaciones** (`Vec::push`, `String`, `format!`, `serde_json`, `HashMap` prohibidos) |
| **Ausencia de Bloqueos Hot-Path** | **Cero Mutex** en el hot-path del Core 1. Triple Buffer SRSW lock-free CAS. | **Cero Mutex** en el bucle de renderizado H-Core. Frontera de publicación atómica `ArcSwap<ProducerSnapshot>`. |
| **Identidad de Intención** | `source_id + request_id + engine_handle` | `source_id + request_id + engine_handle` (Uniforme para todas las fuentes, cero excepciones) |
| **Modelo de Slots del Árbitro** | 8 slots privados, consumo de cola SPSC en $O(1)$ | 8 slots acotados, escaneo lineal en $O(1)$ |
| **Pila de Preempción** | `PreemptionStack<PreemptionEntry, 4>` fija | `[PreemptionEntry; 4]` acotado, `depth: usize` |
| **Saturación de Preempción** | Rechazo estricto en `depth == 4` (sesión activa preservada) | Rechazo estricto en `depth == 4` (sesión activa preservada) |
| **Resolución de Handle** | `EngineHandle` POD $O(1)$ (`uint16_t descriptorId`, `uint16_t instanceId`) | Indexación directa $O(1)$ en `Vec<Option<RegisteredInstance>>` |

---

## 2. Identidad de Intención y Máquina de Estados del Ciclo de Vida

Una intención de solicitud de visualización se identifica de forma única por la tupla de 3 elementos:
$$\text{Intent} = (\text{source\_id}, \text{request\_id}, \text{engine\_handle})$$

### Tabla de Verdad y Matriz de Transición

| Escenario | Decisión del Árbitro | Estado de la Sesión Activa | `TransitionMode` Resultante | Acción FSM |
| :--- | :--- | :--- | :--- | :--- |
| **Rotación Base** | `Rotation` (P10) | Ninguna | `TransitionMode::Replace` | `activate(rot)` |
| **Preempción Prioritaria** | `Mqtt` (P40, preemptivo) | `Rotation` (P10) | `TransitionMode::Preempt` | `pause(rot) -> push(rot) -> activate(mqtt)` |
| **Saturación de la Pila** | `Mqtt` (P60, preemptivo) | `depth == 4` | `TransitionMode::None` | Solicitud rechazada; la sesión activa continúa |
| **Refresco de la Misma Intención** | Mismo `(source, req_id, handle)` | Activa | `TransitionMode::None` | NOOP interno |
| **Actualización Interna de Payload** | Mismo `source` y `handle`, nuevo `req_id` | Activa | `TransitionMode::None` | Actualización interna sin apilar en la pila de preempción |
| **Fin de Preempción (Base Válida)** | `Rotation` (P10) | `Mqtt` activo, la pila tiene `Rotation` | `TransitionMode::Resume` | `deactivate(mqtt) -> pop() -> resume(rot)` |
| **Expiración de Huérfano Sumergido** | Sesión intermedia cancelada mientras estaba sumergida | Pila multinivel | `TransitionMode::Resume` | Desapila sesiones intermedias expiradas, reanuda directamente el ancestro válido |
| **Agotamiento de la Pila** | Ninguna (`DisplayDecision::NONE`) | Pila vacía | `TransitionMode::None` | `deactivate()` todo, devuelve `None` |
| **Handle de Motor Inválido** | `handle` desconocido | Cualquiera | `TransitionMode::None` | Transacción rechazada; sesión activa intacta |

---

## 3. Frontera de Tiempo Real (Cero Asignaciones y Cero Mutex)

### 3.1 Plano de Control (Async Tokio / Core 0)
- Responsable de la API REST HTTP, suscripciones MQTT y validación JSON de la configuración.
- Preanaliza las cargas útiles entrantes en estructuras fuertemente tipadas (p. ej., `MessagePayload`).
- Publica los cambios de forma atómica en memoria compartida mediante `arc_swap::ArcSwap<ProducerSnapshot>` sin adquirir bloqueos en el hilo de tiempo real.

### 3.2 Plano de Renderizado en Tiempo Real (H-Core / Core 1)
- Consume cargas útiles preanalizadas e instantáneas de configuración mediante lecturas atómicas de punteros (`producer_snapshot.load()`).
- Cero bloqueos mutex en todo el hot-path del bucle de renderizado.
- Ejecuta `DisplayArbiter::evaluate()` en complejidad temporal $O(1)$ sin asignaciones dinámicas.
- Resuelve los handles de motores en tiempo $O(1)$ mediante desplazamientos de vector indexados (`handle.instance_id as usize`).
- Despacha transiciones de ciclo de vida (`activate`, `deactivate`, `pause`, `resume`) e invoca `update()` y `render()`.
- Verificado por pruebas unitarias con `GlobalAlloc` instrumentado: **0 asignaciones en el heap durante 10 000 iteraciones en estado estacionario**.

---

## 4. Diferencias Intencionales de Plataforma

| Dimensión | ESP32 (FreeRTOS Embebido / C++) | Raspberry Pi (Linux Embebido / Rust) | Justificación |
| :--- | :--- | :--- | :--- |
| **Controlador de Hardware de Matriz** | DMA paralelo I2S ESP32 / Periférico LCD ESP32-S3 | Bindings C `rpi-rgb-led-matrix` / Backend Mock | La RPi utiliza mapeo de memoria GPIO directo con temporizadores PWM por hardware |
| **Registro de Plugins** | Linker sets C++ (`ENGINE_REGISTER`) | `linkme::distributed_slice` (`ENGINES`) | Registro idiomático de plugins en tiempo de compilación |
| **Modelo de Hilos** | Fijación de tareas FreeRTOS de doble núcleo (`xTaskCreatePinnedToCore`) | Hilos del SO: runtime Tokio + hilo de renderizado en tiempo real (`std::thread::spawn`) | Modelo de concurrencia POSIX estándar en Linux |
| **Triple Buffer de Configuración** | Triple-buffer SRSW lock-free con atómicos y doble magic CRC | Instantáneas inmutables preconstruidas y `ArcSwap` / flags atómicos | Aprovecha la seguridad de memoria de Rust garantizando una ejecución de lectura sin bloqueos |

---

## 5. Suite Automatizada de Verificación y Conformidad

La suite de conformidad normativa está codificada en:
1. `tests/test_esp_alignment.rs`: Valida la paridad de comportamiento con la arquitectura de referencia ESP32.
2. `tests/test_v16_final_locked.rs`: Implementa un `TrackingAllocator` instrumentado que proporciona prueba mecánica de 0 asignaciones en el heap en el hot-path, verificación exhaustiva de la matriz FSM, saturación de la pila acotada y seguridad del registro.


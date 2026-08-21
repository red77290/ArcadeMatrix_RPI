🇬🇧 [English](ARCHITECTURE.md) | 🇫🇷 [Français](ARCHITECTURE_FR.md) | 🇪🇸 Español

# Resumen de Arquitectura (Raspberry Pi - Rust)

Este documento proporciona una visión general detallada de la arquitectura de ArcadeMatrix en Raspberry Pi desarrollada en **Rust**. Explica las opciones de diseño profundo, la estrategia de memoria, la tubería de renderizado y el ciclo de vida "Lazy-Once" de los motores.

---

## 1. Filosofía de diseño: Rendimiento y "Jitter"

A diferencia del ESP32, la Raspberry Pi tiene abundante RAM (512 MB a 8 GB). Sin embargo, su sistema operativo no es "Tiempo Real" (RTOS). El controlador de la matriz (vía DMA/GPIO) es extremadamente sensible a los micro-cortes ("jitter").

Para mantener una frecuencia de actualización estable de 60 FPS sin desgarro de pantalla (tearing), **el bucle rápido (`update()` y `render()`) no debe generar ninguna asignación dinámica innecesaria**. Las asignaciones provocan tareas de limpieza o redimensionamiento del montón (heap) que pueden introducir una latencia impredecible de unos pocos milisegundos, lo suficiente para hacer parpadear la matriz LED.

---

## 2. El Ciclo de Vida "Lazy-Once"

Para cumplir con esta restricción, la arquitectura se basa en un modelo de ciclo de vida muy estricto llamado **Lazy-Once**.

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

### Explicación de las fases:

1. **`initialize()` (Asignación):**
   * **¿Cuándo?** Llamado *exactamente una vez* en toda la vida del programa, la primera vez que el motor debe mostrarse (instanciación perezosa "Lazy").
   * **¿Por qué?** Evita cargar activos (imágenes, fuentes) en RAM para los motores que el usuario ha desactivado en la configuración. Aquí es donde se cargan los mapas de bits y se prepara el terreno de juego.
2. **`activate()` (Preparación temporal):**
   * **¿Cuándo?** Llamado cada vez que el motor se convierte en el motor "activo" en la pantalla.
   * **¿Por qué?** Permite restablecer el estado (por ejemplo, poner la pelota de Pong en el centro, o reiniciar un cronómetro) sin tener que reasignar memoria.
3. **`update()` & `render()` (Hot Loop - 60 FPS):**
   * **Restricción:** **Ninguna asignación dinámica innecesaria.** La memoria requerida (String, Vec) debe haberse reservado en `initialize` o reutilizado (ej. `String::clear()` y luego `write!()` en lugar de asignar nuevas cadenas).
4. **`deactivate()` (En espera):**
   * Permite detener tareas pesadas en segundo plano cuando el motor ya no está en la pantalla.
5. **`is_finished()` (Salto condicional):**
   * Permite que el motor señale al `Runtime` de rotación que ha terminado su tarea (por ejemplo, el Motor Criptográfico ha terminado de mostrar todos sus tokens).

---

## 3. Desacoplamiento: Registro y Configuración

### ¿Por qué el Core no contiene una lista de tipos concretos?
En versiones anteriores, `app.rs` incluía manualmente todos los archivos de reloj y creaba un gran bloque `match` con `Box::new(ClockEngine)`. Esto rompía el principio abierto/cerrado (SOLID): agregar un motor requería modificar el núcleo de la aplicación.
Gracias al **Registro** (basado en la macro `#[distributed_slice]`), cada motor se registra de forma autónoma durante la compilación. El Núcleo de la aplicación ignora por completo la existencia de motores concretos.

### ¿Por qué el Registro contiene descriptores en lugar de instancias?
La instanciación inmediata de todos los motores en el arranque (`Box::new(...)`) consumiría innecesariamente RAM y ralentizaría el tiempo de arranque. En cambio, el descriptor almacena una **Fábrica** (Factory - una función puntero que crea la instancia sobre la marcha) y los metadatos requeridos.

### ¿Por qué separar `config.json` y `EngineConfig`?
El archivo raíz (`config.json`) describe todo el dispositivo (WiFi, Matriz, etc.). Sin embargo, los motores no necesitan — y no deben tener acceso a — la configuración de WiFi u otros motores. `EngineConfig` actúa como una vista restringida o proxy que proporciona solo las variables declaradas por el motor a través de su `ConfigSchema`.

### ¿Cómo llega un cambio de configuración a un motor en ejecución?
Como las instancias se guardan en caché (Lazy-Once), una edición de configuración debe enviarse activamente al motor en vivo en lugar de recrearlo. La cadena de propagación está completamente conectada de extremo a extremo:

```text
POST /api/instances        (hilo api-server)
        │  valida engine_id, se autorrepara vía ConfigSanitizer, guarda config.json
        ▼
reset_rotation / reload_flag  (AtomicBool)
        │  leído por el hilo de renderizado en el siguiente frame
        ▼
EngineRuntime.get_instance()  detecta que cambió el snapshot de configuración
        │
        ▼
engine.on_config_changed()   (misma instancia, sin reasignación)
```

* **Las ediciones de instancia** se aplican **en vivo** (`on_config_changed`) sin reinicio y sin reasignación.
* **Los cambios de hardware/red** (matrix, `disable_internal`, ...) establecen `reload_flag`, que el bucle de renderizado respeta reiniciando el proceso limpiamente para que el controlador se reinicialice.
* La cadencia de renderizado se elige a partir del flag `Capabilities.realtime` del descriptor del motor (≈25 FPS para motores animados, 1 Hz para estáticos), nunca desde un nombre de motor codificado.

---

## 4. Aislamiento del Runtime y Modelo de Hilos (Threading)

ArcadeMatrix se basa en una arquitectura de múltiples hilos para aislar el renderizado de hardware de las operaciones de red:

1. **Hilo de Renderizado Dedicado (`matrix-render`):**
   - Se ejecuta en un hilo de SO dedicado con una pila de 8 MB.
   - Acceso exclusivo a la matriz LED. Si se combinara con la API Web, cada petición HTTP causaría un salto de cuadro (tearing) en la matriz.

2. **Hilo API Web Aislado (`api-server`):**
   - Se ejecuta en un entorno de ejecución Tokio de un solo hilo (`Builder::new_current_thread()`).
   - Gestiona la configuración a través de la interfaz web (puerto 80). Se comunica con el hilo de renderizado solo a través de primitivas atómicas (`AtomicBool`) o bloqueos asincrónicos de corta duración (`RwLock`).

3. **Servicios en Segundo Plano:**
   - **Escucha MQTT / APIs HTTP:** Aislados para no bloquear nunca el cálculo de cuadros (`update()`).

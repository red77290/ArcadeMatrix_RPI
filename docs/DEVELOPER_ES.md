🇬🇧 [English](DEVELOPER.md) | 🇫🇷 [Français](DEVELOPER_FR.md) | 🇪🇸 Español

# Guía del Desarrollador (Raspberry Pi - Rust)

Bienvenido a la guía de desarrollo de ArcadeMatrix para Raspberry Pi. Este documento explica cómo extender la arquitectura y crear nuevos Motores en Rust.

---

## 1. Entendiendo la Arquitectura: Motores, Registro y Ciclo de Vida

ArcadeMatrix ya no tiene una lista codificada en `app.rs` de sus características. El sistema se basa en un **Registro** (usando el crate `linkme`) que descubre los motores en el arranque.

### 1.1 El Ciclo de Vida Estricto (Lazy-Once)

Para prevenir desgarros de pantalla y jitter causados por el asignador de memoria de Rust (Heap), ArcadeMatrix impone un estricto ciclo de vida para cada implementación del trait `Engine`.

```text
initialize()
    │
    ├── asignaciones en el montón (heap) vía 'String' o 'Vec'
    ├── carga de activos (imágenes, fuentes)
    ├── preparación de caché
    └── inicialización pesada
          ↓
activate()
    │
    └── preparación de estado temporal (reinicio de temporizadores, etc.)
          ↓
update()
    │
    └── lógica en tiempo real (60 FPS) - **SIN ASIGNACIONES DINÁMICAS INNECESARIAS**
          ↓
render()
    │
    └── renderizado en tiempo real (60 FPS) - **SIN ASIGNACIONES DINÁMICAS INNECESARIAS**
          ↓
deactivate()
    │
    └── liberación de recursos externos o detención de escuchas
```

- **Regla de oro:** Nunca instancie nuevos `String` o `Vec` dinámicos dentro de `update()` o `render()`. Preasigne sus búferes en `initialize()` y mutelos en el lugar (ej. `my_string.clear()` y luego `write!(&mut my_string, "...")`).
- **`on_config_changed()`:** Llamado **en vivo** por el `EngineRuntime` cada vez que cambia la configuración persistida de una instancia en caché (por ejemplo, cuando el usuario la edita en la interfaz Web). El motor **no** se recrea: conserva sus asignaciones y simplemente vuelve a leer los nuevos valores. Impleméntelo para aplicar ajustes sin reinicio.
- **`is_finished()`:** Útil para indicar al `EngineRuntime` que un motor ha terminado su tarea para forzar el paso al siguiente motor sin esperar el tiempo de espera.

### 1.2 Capabilities & Cadencia de Refresco

El runtime deriva su pausa por frame desde las `Capabilities` del descriptor del motor, **no** desde ningún nombre de motor codificado:

- `realtime: true` → el motor se consulta a ~25 FPS (40 ms) para una animación fluida (GIF, mensaje desplazable, Spotify).
- `realtime: false` (por defecto) → el motor se refresca una vez por segundo (1000 ms), ideal para contenido estático (reloj, fecha, clima) y mucho más ligero para CPU/Wi-Fi.

Establezca `realtime: true` en su descriptor solo si su motor anima cada frame.

### 1.3 Configuración Autorreparable

Cada valor que declare en el `ConfigSchema` es validado por el `ConfigSanitizer` al arrancar y en cada escritura. Para aprovecharlo, complete los metadatos de campo pertinentes:

- `field_type` (`Integer`, `Float`, `Boolean`, `Options`, `String`) selecciona la estrategia de validación.
- `min_val` / `max_val` delimitan los campos numéricos; `options` enumera los valores permitidos para `Options`.
- `validation_policy` (`Clamp`, `FallbackDefault`, `Reject`, `Accept`) decide qué ocurre con un valor fuera de rango.
- `default_value` se inyecta automáticamente cuando falta la clave (por ejemplo, un campo añadido por una OTA posterior). Las claves que ya no están presentes en el schema se eliminan.

---

## 2. Tutorial: Creación de un Nuevo Motor

Para crear un nuevo motor, debe implementar el trait `Engine` y proporcionar un `EngineDescriptor` a través del Registro.

### Paso 1: Crear la estructura (`src/engines/my_engine.rs`)

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

### Paso 2: Implementar el Ciclo de Vida

```rust
impl Engine for MyEngine {
    fn initialize(
        &mut self,
        _context: &mut EngineContext,
        config: &dyn EngineConfig,
    ) -> Result<(), EngineError> {
        // Lugar seguro para asignaciones
        self.my_setting = config.get_string("my_setting", "default");
        println!("MyEngine inicializado!");
        Ok(())
    }

    fn activate(&mut self) {
        self.counter = 0; // Reinicio rápido
    }

    fn update(&mut self, _context: &mut EngineContext) {
        // Lógica de negocio rápida, SIN asignaciones
        self.counter += 1;
    }

    fn render(&mut self, context: &mut EngineContext) {
        // Renderizado por hardware vía context.matrix
        context.matrix.clear();
        // Precaución: dibujar texto no crea asignación si se usan búferes existentes
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

### Paso 3: Registrar el Motor en el Arranque

Añada el descriptor en la parte inferior de su archivo para exponer los campos de configuración a la API Web:

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
        capabilities: Capabilities::default(), // usa `realtime: true` si animas cada frame
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
                // Controla el sanitizador autorreparable para campos numéricos/de opciones.
                validation_policy: crate::core::engine_contract::ValidationPolicy::Accept,
            }],
        },
        factory: || Box::new(MyEngine::new()),
    }
}
```

### Paso 4: Añadir referencia del módulo

Abra `src/engines/mod.rs` y añada:
```rust
pub mod my_engine;
```

¡Eso es todo! **No se necesita modificar ningún código en `app.rs`**. El motor se listará automáticamente en la API Web, y su configuración `config.json` se gestionará de forma aislada.

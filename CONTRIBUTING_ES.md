🇬🇧 [English](CONTRIBUTING.md) | 🇫🇷 [Français](CONTRIBUTING_FR.md) | 🇪🇸 Español

# Contribuir a ArcadeMatrix

¡Bienvenido a ArcadeMatrix! Tanto si corriges bugs, como si añades nuevas animaciones o llevas ArcadeMatrix a una nueva plataforma, nos alegra tenerte aquí.

Este documento describe la filosofía, la arquitectura y las convenciones de código del proyecto para garantizar una base de código robusta y mantenible a medida que el proyecto escala.

## Filosofía del proyecto

El proyecto ArcadeMatrix se divide en tres fases evolutivas distintas:
1. **Prototipo**: conseguir que la pantalla funcione.
2. **Producto**: añadir funciones (GIFs, weather, clock, WebUI, MQTT...).
3. **Framework**: estabilizar la arquitectura, las pruebas y hacerla extensible.

Actualmente estamos en la fase **Framework**. Cualquier nueva contribución debe respetar una separación estricta de responsabilidades, evitando el «spaghetti code» y los motores monolíticos.

## Separación de responsabilidades: el Rendering Pipeline

Utilizamos un Rendering Pipeline moderno para gestionar lo que se dibuja en la matriz. Si quieres añadir un nuevo efecto visual o una nueva forma de mostrar la hora/fecha, sigue este flujo:

`Data -> Engine -> Animation -> Renderer -> Matrix`

### Engines vs. Renderers

* **Engine (`engines/`)**: responsable de la adquisición de datos, la gestión del estado y la lógica de negocio.
  * *Ejemplo*: `ClockEngine` sabe *qué hora es* y *cuándo rotar*. **No** sabe cómo dibujar un recuadro blanco que se encoge.
  * *Ejemplo*: `WeatherEngine` sabe cómo consultar la API y parsear el JSON.
* **Renderer (`engines/renderers/`)**: responsable de dibujar píxeles en el frame. NO tiene lógica de negocio. Toma cadenas de datos en bruto, fuentes y colores, y devuelve una imagen.
  * *Ejemplo*: `CyberpunkRenderer` sabe dibujar una lluvia digital verde cayendo.
  * *Ejemplo*: `FlipRenderer` sabe calcular bounding boxes y dibujar paneles que se contraen.

**Regla general:**
Si estás añadiendo un nuevo tema visual que usa los *mismos datos* (como una nueva esfera de reloj), crea un **Renderer** (o un reloj especializado como `PongClock`, que actúa como renderer).
Si estás añadiendo una función completamente nueva (como recuperar precios bursátiles o Spotify now playing), crea un **Engine**.

## Convenciones de código

* **Lenguaje**: el repositorio principal usa Rust para Raspberry Pi.
* **Tipado**: aprovecha al máximo el tipado estático fuerte de Rust y sus Traits para aclarar los contratos Engine/Renderer.
* **Pruebas**: todas las rutas de la API y la lógica de configuración Core deben estar cubiertas por `cargo test`.
* **Independencia del hardware**: no supongas que la matriz es exactamente 64x32. Lee el tamaño del panel desde `MatrixConfig` (`matrix.width` / `matrix.height`) y declara las resoluciones que soportas mediante las `Capabilities` del descriptor (`supports_128x32` / `supports_256x64`).

## La arquitectura de motores (Registry / Descriptor / Factory)

El Core es **agnóstico respecto a los motores**: nunca nombra directamente a
`Clock`, `Weather` o `Spotify`. Cada motor es un plugin autodescrito que se
descubre en tiempo de ejecución:

```
Engine
 ├── Descriptor  (metadatos + Capabilities + Requirements)
 ├── ConfigSchema (campos, tipos, valores por defecto, min/max, opciones, options_endpoint)
 ├── Factory     (construcción perezosa, creado una sola vez y cacheado)
 └── Lifecycle   (initialize → activate → update/render → deactivate)
```

- Los motores se configuran como **instancias genéricas** (`instance_id` +
  `engine_id` + un `config` como mapa de cadenas), no como tipos hardcodeados.
- La interfaz web se genera desde `GET /api/engines`: un motor nuevo aparece
  automáticamente en la UI en cuanto se declara su `ConfigSchema`, sin tocar el
  frontend.
- Los cambios de configuración llegan a un motor en ejecución en vivo mediante
  `on_config_changed()`; la configuración se autorrepara (se inyectan valores
  por defecto, los valores fuera de rango se acotan o se reinician) antes de
  persistirse.

La guía completa y de referencia está en
[`docs/DEVELOPER_ES.md`](docs/DEVELOPER_ES.md).

## Añadir un nuevo motor

1. Crea `src/engines/my_engine.rs` implementando el contrato `Engine`
   (`initialize` / `activate` / `update` / `render` / `deactivate`, más
   `is_finished` y `on_config_changed` según haga falta).
2. Proporciona un `EngineDescriptor`: metadatos, `Capabilities` (pon
   `realtime: true` solo si debe actualizarse en cada frame), `Requirements` y
   un `ConfigSchema`.
3. Regístralo en el registro mediante la entrada factory
   `#[distributed_slice(ENGINES)]` para que el autodescubrimiento y la UI lo
   detecten.
4. Añade cobertura en `tests/` (ver `tests/test_registry.rs` y
   `tests/test_sanitizer.rs`).

## Añadir un nuevo Renderer

Si solo necesitas un nuevo *aspecto visual* para datos existentes (p. ej. una
nueva esfera de reloj), añade un **Renderer** en lugar de un motor:
1. Crea un archivo nuevo en `src/engines/renderers/my_custom_renderer.rs`.
2. Implementa el trait `Renderer`.
3. Regístralo en `src/engines/renderers/mod.rs`.

## Architecture Decision Records (ADR)

Si propones un cambio arquitectónico importante, redacta un ADR en `docs/adr/`. Revisa los ADR existentes para entender por qué se tomaron determinadas decisiones de arquitectura (como evitar el multi-threading en el renderizado).

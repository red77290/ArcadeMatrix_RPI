🇬🇧 [English](DEVELOPER.md) | 🇫🇷 [Français](DEVELOPER_FR.md) | 🇪🇸 Español

# Guía del desarrollador

Bienvenido a la guía de desarrollo de ArcadeMatrix. Este documento explica la arquitectura principal del proyecto y proporciona instrucciones paso a paso sobre cómo ampliarlo.

## Arquitectura: Renderers vs. Clocks

Desde la gran refactorización, ArcadeMatrix separa estrictamente **la estética visual (Renderers)** de **la lógica de comportamiento (Clocks)**. Entender esta diferencia es fundamental antes de empezar a programar.

### 1. Renderers (el «tema»)
Ubicados en `engines/renderers/`.
Un **Renderer** (p. ej. `CyberpunkRenderer`, `FlipRenderer`) es puramente estético. No le importa si está mostrando la hora, la fecha o el weather. Toma una cadena de texto, una fuente y la dibuja sobre un fondo estilizado o un efecto visual.
- **Responsabilidad:** fondos, colores, efectos de partículas, animaciones de transición.
- **Ventaja:** altamente reutilizable entre distintos Engines (`ClockEngine`, `DateEngine`, etc.).

### 2. Specialized Clocks (el «mini-juego»)
Ubicados en `engines/clocks/`.
Un **Specialized Clock** (p. ej. `PongClock`, `TetrisClock`, `PacManClock`) es un motor de lógica dinámica. Gestiona un estado interno (como una pelota rebotando o bloques cayendo) para construir visualmente la visualización de la hora.
- **Responsabilidad:** estado del juego, física, dibujo de sprites y generación visual de la hora en lugar de simplemente escribir una cadena.
- **Ventaja:** completamente autónomo y permite visualizaciones muy complejas, frame a frame.

## Ampliando la base de código Rust

*Nota: ArcadeMatrix se reescribió recientemente en Rust. Los tutoriales para desarrolladores sobre cómo agregar Renderers, Clocks y Engines se están actualizando para reflejar la nueva arquitectura Rust (`src/engines/`). Mientras tanto, puedes inspeccionar las implementaciones existentes en `src/engines/renderers` para ver cómo se implementa el trait `Renderer`.*

---

## Tutorial 1: crear un nuevo Renderer

Si quieres añadir un nuevo fondo genérico o efecto visual (como un tema «Synthwave») que pueda usarse tanto para Time como para Date:

1. **Crear el archivo:**
   Crea un archivo `engines/renderers/synthwave_renderer.rs`.

2. **Implementar el trait:**
   (Código omitido: consulta la implementación de `base_renderer.rs` para ver cómo implementar el trait `Renderer`).

3. **Registrar el Renderer:**
   Abre `engines/renderers/mod.rs` y añade tu nuevo Renderer al registro.

---

## Tutorial 2: crear un nuevo Specialized Clock

Si quieres crear un reloj complejo que juegue a un juego o construya la hora bloque por bloque:

1. **Crear el archivo:**
   Crea `engines/clocks/snake_clock.rs`.

2. **Implementar la lógica:**
   (Código omitido: consulta `clock_trait.rs` para ver los requisitos de la interfaz).

3. **Registrar el reloj:**
   Abre `engines/clock_engine.rs` e integra tu reloj en el motor de relojes.

---

## Tutorial 3: crear un nuevo elemento de screensaver (Engine)

Si quieres añadir un módulo completamente nuevo a la rotación idle:

1. **Crear el archivo del Engine:**
   Crea `engines/crypto.rs`.

2. **Registrar el Engine en la rotación:**
   Abre `src/core/rotation.rs` para incluir tu motor en el ciclo de ejecución.

3. **Actualizar UI y configuración:**
   - Actualiza `src/api/server.rs` para aceptar tu nuevo engine en los parámetros.
   - Actualiza `api/www/index.html` para que los usuarios puedan arrastrarlo y soltarlo en su rotación activa.

---

## Integración de API y Web UI

Cada vez que crees un tema nuevo o un reloj nuevo:
1. Actualiza `src/api/server.rs` si tu nueva función requiere nuevos ajustes.
2. Actualiza `api/www/index.html` para añadir tu nuevo Theme ID a los menús desplegables (`<select id="time_theme">`).

### ⚠️ El código fuente del frontend no está en este repositorio

`api/www/` solo contiene el dashboard **compilado/empaquetado** (`index.html`, `assets/index-*.js`,
`assets/index-*.css`: una build minificada de Vite, JS/HTML/CSS plano, **no** Vue.js a pesar de que
documentación antigua afirmaba lo contrario). Aquí no hay `package.json`, ni fuentes de componentes,
ni configuración de Vite versionada, por lo que el bundle **no puede reconstruirse ni modificarse
de forma significativa** solo desde este repositorio; únicamente puede editarse a mano sobre la salida
ya minificada, lo que no escala más allá de ajustes triviales (como las entradas del desplegable de
temas mencionadas arriba).

Si necesitas realizar cambios importantes en la UI, tienes dos opciones:
1. Localizar dónde vive el proyecto fuente original del frontend (si todavía existe) y volver a
   integrarlo en este repositorio, por ejemplo dentro de una nueva carpeta `frontend/`, con un paso
   de build que genere la salida en `api/www/`.
2. Reconstruir desde cero un pequeño proyecto frontend contra la API REST existente (consulta
   `src/api/server.rs` para la lista completa de rutas) si la fuente original realmente se ha perdido.

En cualquier caso, **no sigas distribuyendo silenciosamente solo un bundle compilado sin una fuente
de verdad documentada**: si encuentras/restauras la fuente, haz commit de ella y documenta aquí el
comando de build.

## Probando Tu Código

Imponemos un 100% de cobertura de pruebas en las rutas API. Para verificar tu código:
```bash
cargo test
```

## Flujo de Desarrollo Local Rápido (Cross-Compilation)

Para iterar rápidamente, no necesitas reconstruir todo el archivo `.img` de 14 GB ni compilar directamente en la Raspberry Pi lenta. ArcadeMatrix incluye scripts de compilación cruzada que funcionan en cualquier SO anfitrión (Windows, Linux, macOS) siempre que Docker esté instalado.

### 1. Construir el Binario
Este comando lanza un contenedor Docker ligero de Rust, instala la toolchain de compilación cruzada para ARM64 y compila tu código Rust nativamente en solo unos segundos. El binario resultante se guarda en `target/aarch64-unknown-linux-gnu/release/arcadematrix`.
```bash
bash scripts/build_local.sh
```

### 2. Desplegar en la Raspberry Pi
Este comando utiliza `scp` y `ssh` para subir el nuevo binario compilado directamente a tu Raspberry Pi activa y reinicia el servicio systemd.
```bash
bash scripts/deploy_to_pi.sh pi@<TU_DIRECCION_IP_PI>
```

## Pruebas Unitarias y TDD
El proyecto sigue los principios de TDD para la integración de API. Al agregar una nueva API, implemente la interfaz Provider correspondiente (`ICryptoProvider`, etc.) y escriba pruebas unitarias utilizando objetos Mock antes de conectarla. Las pruebas deben lograr la máxima cobertura en el análisis de JSON y la lógica de respaldo sin requerir hardware físico.

## Personalización del Usuario del SO (Custom User)
Si desea cambiar el usuario predeterminado (`pi`) y su contraseña predeterminada (`raspberry`) para la generación de imágenes o la implementación manual, puede editar el archivo `scripts/defaults.sh` antes de iniciar la compilación.

```bash
export AM_USER="tu_usuario"
export AM_PASS="tu_contraseña"
```
Durante la generación de la imagen con `scripts/build_image.sh`, estas variables se leerán automáticamente. El hash de la contraseña se calculará dinámicamente usando SHA-512 para su inyección en el archivo `.img` (`userconf.txt`).
Los scripts de implementación (`autoInstall.sh` y `deploy.sh`) también utilizarán estas variables para configurar correctamente los permisos (atravesar la carpeta principal para el demonio) e instalar los alias `.bash_aliases` en la ubicación correcta.

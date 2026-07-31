🇬🇧 [English](GETTING_STARTED.md) | 🇫🇷 [Français](GETTING_STARTED_FR.md) | 🇪🇸 Español

# Primeros pasos (app Raspberry Pi, configuración del workspace de desarrollo)

Esta guía es para desarrolladores que configuran un **entorno de desarrollo local** en su propia máquina
(Mac/Linux/Windows) para trabajar en la codebase de ArcadeMatrix_RPi, a diferencia de `QUICKSTART_ES.md`,
que está dirigido a usuarios finales que graban una imagen ya construida en una Raspberry Pi. Para arquitectura y
convenciones de contribución (Engines vs. Renderers), consulta `DEVELOPER_ES.md` y `../CONTRIBUTING_ES.md`.

## 1. Crear un entorno virtual

```bash
git clone <this-repo-url>
cd ArcadeMatrix_RPi
python3 -m venv .venv
source .venv/bin/activate        # Windows: .venv\Scripts\activate
```

## 2. Instalar dependencias

```bash
cargo build --release
pip install pytest                # not in Cargo.toml (runtime deps only) - needed for tests
```

**Aviso importante sobre hardware:** `Cargo.toml` **no incluye** `rgbmatrix`; ese es el binding de Rust de [hzeller's `rpi-rgb-led-matrix`](https://github.com/hzeller/rpi-rgb-led-matrix), una extensión C++ compilada que solo se compila/ejecuta en hardware Raspberry Pi real (habla directamente con los pines GPIO). Esto significa:
- `cargo run --release` **no puede ejecutarse de extremo a extremo en una máquina de desarrollo normal**: `src/core/matrix.rs` importa `rgbmatrix` de forma incondicional al cargar el módulo, así que fallará inmediatamente fuera de una Pi.
- Aun así, puedes desarrollar y probar todo lo que no necesite dibujar en un panel físico: la API Actix-web (`src/api/server.rs`), el parseo de configuración (`src/core/config.rs`), la lógica de rotación (`src/core/rotation.rs`) y gran parte de la lógica de negocio de los Engines; consulta la sección de pruebas más abajo, que ya mockea por completo la capa de matriz.
- Si quieres una vista previa visual en vivo en tu máquina de desarrollo sin una Pi, mira [`RGBMatrixEmulator`](https://github.com/ty-porter/RGBMatrixEmulator) (un paquete drop-in compatible con la API que renderiza en una ventana Pygame o en el navegador en lugar de GPIO real). **Actualmente no está cableado en este proyecto**: habría que cambiar condicionalmente `from rgbmatrix import ...` en `src/core/matrix.rs`, pero es un shim compatible bien conocido si quieres experimentar localmente.

## 3. Ejecutar la app de verdad

Para ejecutar la aplicación completa hace falta una Raspberry Pi con la matriz cableada según
`ARCHITECTURE_ES.md` / las instrucciones del fabricante del HAT, y `rgbmatrix` compilado/instalado
(`../install.sh` en la raíz del repo automatiza esto, incluida la configuración del servicio systemd). En la Pi:

```bash
sudo cargo run --release
```

(se requiere root/`sudo`; `rgbmatrix` necesita acceso directo a GPIO/DMA.)

Para una instalación totalmente turnkey, `../install.sh` configura una unidad systemd `arcadematrix.service` para que la
app arranque al encender y se reinicie tras un crash. Comprueba su estado/logs con:

```bash
sudo systemctl status arcadematrix.service
sudo journalctl -u arcadematrix.service -f      # live-tail systemd's own logs
```

## 4. Dónde están los logs

Independientemente de systemd, la app también escribe su propio archivo de logs rotativo junto a `src/main.rs`:

```bash
tail -f arcadematrix.log            # rotates at 5MB, keeps 3 backups (see src/main.rs)
```

También se escribe un `crash.log` independiente (sobrescribiéndolo) si el proceso muere por una excepción no capturada
(ver el handler `sys.excepthook` al principio de `src/main.rs`); revísalo primero después de cualquier crash.

## 5. Ejecutar la suite de tests

Esta es la forma principal de validar cambios sin necesitar hardware real: la suite existente ya mockea la matriz
(`MockMatrix`/`MockMatrixWrapper` de `tests/conftest.py`) para que se ejecute igual en tu máquina de desarrollo o en CI:

```bash
cargo test -v
```

Consulta la sección «Testing Your Code» de `DEVELOPER_ES.md` para las expectativas de cobertura del proyecto
(100 % en rutas API) y `../CONTRIBUTING_ES.md` para saber qué cuenta como Engine vs. Renderer al añadir nuevos tests.

## 6. Construir una imagen de release (opcional, para maintainers)

Si necesitas producir una imagen completa y grabable de Raspberry Pi OS (como la que se enlaza desde
`QUICKSTART_ES.md`), consulta `../scripts/build_image.sh` (macOS/Linux, requiere Docker): descarga
Raspberry Pi OS Lite, inyecta este repo, compila el Rust a bytecode para ocultar el código fuente y
crea la partición FAT32/exFAT `DATA` donde los usuarios finales dejan sus GIFs/fonts/sprites. Es un
proceso de 10-15 minutos y no es necesario para el desarrollo diario de funcionalidades; solo para generar
un nuevo artefacto de release.

## Solución de problemas

- **`ModuleNotFoundError: No module named 'rgbmatrix'`** al ejecutar `cargo run --release` fuera de una Pi:
  es lo esperado; consulta §2 arriba: usa `pytest` para desarrollo local en su lugar.
- **`ImportError` para `paho.mqtt`**: el soporte MQTT es opcional (flag `MQTT_AVAILABLE` en `src/main.rs`);
  instala `paho-mqtt` (ya está en `Cargo.toml`) si necesitas probar localmente la integración con Batocera/Recalbox.
- **Los tests fallan con errores de import de `rgbmatrix`**: asegúrate de probar a través de `src/api/server.rs`
  y de las fixtures proporcionadas (`tests/conftest.py`) en lugar de importar `core.matrix` directamente en
  un test nuevo; los tests existentes están estructurados específicamente para no tocar nunca ese módulo.

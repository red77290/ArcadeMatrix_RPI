🇬🇧 [English](GETTING_STARTED.md) | 🇫🇷 [Français](GETTING_STARTED_FR.md) | 🇪🇸 Español

# Primeros pasos (app Raspberry Pi en Rust, configuración del workspace de desarrollo)

Esta guía está pensada para desarrolladores que montan un **entorno de desarrollo local** en su equipo (Mac/Linux/Windows) para trabajar en la codebase nativa de ArcadeMatrix_RPi en **Rust**.

---

## 1. Requisitos del sistema

- **Rust Toolchain (1.75+)**: instalable con `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Cargo**: el gestor de paquetes y build de Rust (incluido con Rustup).

---

## 2. Compilar y ejecutar en local (Dev / Mock Matrix)

En cualquier Mac, Linux o Windows sin una Raspberry Pi física conectada:

```bash
git clone <this-repo-url>
cd ArcadeMatrix_RPi

# Comprobación rápida de compilación
cargo check

# Compilar y ejecutar en modo desarrollo con el Mock Canvas
cargo run
```

Por defecto en Mac/Windows, el proyecto usa `MockMatrix`, que simula la matriz LED en memoria mientras arranca el servidor web Actix en `http://127.0.0.1:8080`.

---

## 3. Ejecutar la suite de tests

La suite de tests de Rust valida la configuración, el registro y el ciclo de vida de los motores, el sanitizador de configuración autorreparable (`tests/test_sanitizer.rs`), los endpoints REST de Actix y la validación de binarios de actualización OTA (`POST /api/update`):

```bash
cargo test
```

Comprobación de formato y reglas de linter:

```bash
cargo fmt --check
cargo clippy -- -D warnings
```

---

## 4. Compilación cruzada y despliegue en Raspberry Pi

Para compilar el binario nativo para Raspberry Pi desde tu equipo Mac/Linux:

```bash
# Instalar cross
cargo install cross

# Compilación cruzada ARM 64-bit (Raspberry Pi 3, 4, Zero 2 W)
cross build --target aarch64-unknown-linux-gnu --release

# Compilación cruzada ARM 32-bit (Raspberry Pi 2, Zero)
cross build --target armv7-unknown-linux-gnueabihf --release
```

El binario resultante se encuentra en `target/aarch64-unknown-linux-gnu/release/arcadematrix`. Puedes copiarlo directamente a la Pi o actualizarlo sin interrupción desde la interfaz web (sección **Firmware Update (OTA)**).

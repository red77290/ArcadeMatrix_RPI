🇬🇧 [English](SCRIPTS.md) | 🇫🇷 [Français](SCRIPTS_FR.md) | 🇪🇸 Español

# Documentación de Scripts de ArcadeMatrix

Este documento explica el propósito, los requisitos previos y el uso de cada script en el directorio `scripts/`. Ayudan a **compilar**, **desplegar**, **crear imágenes SD**, **diagnosticar** y **validar** ArcadeMatrix_RPi en todas las plataformas.

## Requisitos Previos

### 1. Docker
Toda la compilación cruzada y la creación de imágenes se gestionan mediante Docker, por lo que nunca tendrá que instalar una cadena de herramientas ARM localmente.
- **macOS / Windows**: [Docker Desktop](https://www.docker.com/products/docker-desktop/) o Rancher Desktop.
- **Linux**: Docker Engine (`sudo apt install docker.io`).

### 2. Acceso SSH a la Raspberry Pi (solo despliegue)
- La Pi debe estar accesible en su red con SSH habilitado (`sudo raspi-config` > Interface Options > SSH).
- Anote su dirección IP.

### 3. sshpass (solo despliegue en Linux / macOS)
`deploy.sh` utiliza `sshpass` para automatizar la introducción de la contraseña.
- **macOS**: `brew install hudochenkov/sshpass/sshpass`
- **Linux**: `sudo apt install sshpass`
- **Windows**: no es necesario — `deploy.ps1` usa OpenSSH nativo y solicita la contraseña de forma interactiva (o usa autenticación por clave).

### 4. Valores predeterminados compartidos
`defaults.sh` centraliza las credenciales y rutas utilizadas por los scripts de compilación, despliegue e imagen (`AM_USER`, `AM_PASS`, ...). Es **importado** por los demás scripts: edítelo una sola vez.

---

## 🛠 Compilación

### `build.sh [aarch64|armv7|all]`
Compilador cruzado inteligente. Compila el binario de Rust para ARM de 64 bits (`aarch64`, Pi 3/4/5 y Zero 2 W), ARM de 32 bits (`armv7`, Pi 2/Zero) o ambos, dentro de Docker.
Valor por defecto: `all`.
**Uso**: `bash scripts/build.sh aarch64`

El binario resultante se guarda en `target/<triple>/release/arcadematrix`.

---

## 🚀 Despliegue

Los scripts de despliegue detectan automáticamente la arquitectura de la Pi, compilan mediante Docker, suben el binario y reinician el servicio `arcadematrix`.

### `deploy.sh` (macOS / Linux)
Opciones (todas opcionales; los valores predeterminados provienen de `defaults.sh`):
- `--ip <IP>` — IP de la Pi de destino
- `--user <USER>` — Usuario SSH (por defecto `pi`)
- `--pass <PASS>` — Contraseña SSH (usa `sshpass`)
- `--skip-build` — Reutiliza el binario ya compilado
- `--binary-path <PATH>` — Despliega un binario precompilado específico

**Uso**: `bash scripts/deploy.sh --ip 192.168.1.149 --user pi`

### `deploy.ps1` (Windows)
Equivalente en PowerShell utilizando OpenSSH nativo. Mismo comportamiento.
**Uso**: `./scripts/deploy.ps1 -Ip 192.168.1.149 -User pi -Pass raspberry [-SkipBuild] [-BinaryPath <path>]`

> 💡 Para un dispositivo en ejecución normalmente no necesita desplegar por script: utilice la **actualización de firmware OTA** en la interfaz Web (pestaña Sistema), que valida y reemplaza el binario por Wi-Fi. Consulte `docs/QUICKSTART_ES.md`.

---

## 💿 Creación de Imagen SD

### `build_image.sh`
Construye una imagen `.img` de Raspberry Pi OS lista para grabar con ArcadeMatrix preinstalado, mediante Docker. Produce una imagen de ~14 GB dimensionada para una tarjeta SD de 16 GB.
**Uso**: `bash scripts/build_image.sh`

Ayudantes internos (no se ejecutan directamente):
- **`docker_builder.sh`** — se ejecuta dentro del contenedor Docker para ensamblar la imagen.
- **`chroot_setup.sh`** — se ejecuta dentro del chroot de la imagen para instalar el SO, el servicio y las dependencias mediante `autoInstall.sh`.

---

## 🩺 Ejecución / Diagnóstico en la Pi

### `recovery.sh <PROJ_DIR>`
Se ejecuta en la Pi **antes** de que inicie el servicio principal. Busca en la partición boot un firmware de recuperación; si está presente, lo instala y hace una copia de respaldo para evitar bucles de arranque. Invocado automáticamente por el servicio.

### `wifi_diag.sh`
Registrador de diagnóstico de Wi-Fi / contención DMA en la Pi. Se ejecuta desacoplado (`setsid`), toma muestras periódicamente y escribe un archivo de registro en disco.
**Uso (en la Pi, como root)**: `sudo bash scripts/wifi_diag.sh`

### `run_ab_test.sh`
Benchmark A/B automatizado (carga del bus de memoria frente a hardware pulsing). Se ejecuta completamente desacoplado (~6 min), sobrevive a caídas de SSH y restaura la configuración al finalizar. Escribe archivos `diag_*.log` utilizados en `docs/benchmarks/`.

---

## ✅ Herramientas y Validación

Respaldan el hook `pre-commit` de git y también pueden ejecutarse manualmente.

### `install_hooks.py`
Instala los hooks de git de ArcadeMatrix en `.git/hooks` (`pre-commit` ejecuta los validadores + `cargo fmt --check` + pruebas).
**Uso**: `python3 scripts/install_hooks.py`

### `validate_docs.py`
Verifica la paridad documental y la validez de las claves de `config.json` en todo el conjunto de documentación EN/FR/ES.

### `validate_rpi_release.py`
Valida que los artefactos de lanzamiento y scripts de instalación (`Cargo.toml`, `autoInstall.sh`, `config.json`, `README*.md`) estén presentes y bien formados antes de publicar una versión.


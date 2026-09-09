🇬🇧 [English](README.md) | 🇫🇷 [Français](README_FR.md) | 🇪🇸 Español

# tools/

Utilidades del lado del PC/manuales para ArcadeMatrix_RPi. No forman parte de la aplicación
Flask/API - las ejecutas tú mismo, por separado, ya sea en tu propio ordenador o copiándolas a tu
Recalbox.

## `mugen_extractor/`

Convierte los archivos de personajes de MUGEN (`.sff`/`.air`) al formato binario de sprite `.fgt`
usado tanto por `engines/fighter.py` de este proyecto **como** por el `FighterEngine` (C++) del
proyecto hermano ESP32 - es la misma herramienta, mantenida idéntica en ambos repositorios ya que
no existe un mecanismo de paquetes compartido entre estas dos bases de código independientes. Ver
`mugen_extractor/README_ES.md` para el uso completo.

## `rpi_emulationstation_base_os_setup.sh` (y `recalbox_setup_mqtt.sh`)

Un script de shell independiente que copias en tu consola retro (Recalbox, Batocera o RetroPie) y ejecutas **directamente en el
dispositivo por SSH** (no desde tu PC) para instalar el daemon MQTT de "reproduciendo ahora":

1. `ssh <user>@<ip-consola>` (ej.: `root@<ip>` con contraseña `recalboxroot` para Recalbox, `root`/`linux` para Batocera, `pi`/`raspberry` para RetroPie).
2. Ejecuta el script:
   ```bash
   sh /tmp/rpi_emulationstation_base_os_setup.sh
   ```
3. Introduce la IP de tu ArcadeMatrix (ESP32 o Raspberry Pi) cuando se te solicite.
4. Selecciona tu sistema operativo objetivo:
   - `1: Detección automática`
   - `2: Recalbox`
   - `3: Batocera`
   - `4: RetroPie`
5. El script instala automáticamente el daemon, configura el inicio del sistema y sugiere reiniciar.

Lo que instala:
- Un pequeño daemon en Python (`arcadematrix_daemon.py`) que monitoriza el lanzamiento/detención de juegos y publica `{"status": "playing"|"browsing"|"stopped", "game": "<nombre base de la rom>", "system": "<SystemId>"}` vía MQTT en `system/playing/<os>` (`system/playing/recalbox`, `system/playing/batocera` o `system/playing/retropie`) mediante `mosquitto_pub`.
- Integración en el arranque para mantener el daemon activo tras cada reinicio.

**Nota para usuarios que también tienen un dispositivo ESP32 ArcadeMatrix**:
Este daemon y el formato de topic estándar (`system/playing/#`) son 100% idénticos y compatibles entre las versiones de ESP32 y Raspberry Pi. Una sola instalación en tu consola sirve para ambos dispositivos simultáneamente. También puedes ejecutar el instalador automatizado desde el PC (`tools/install.sh` / `tools/install.ps1`).

**Normalmente esto ocurre automáticamente mediante la función de instalación SSH de la interfaz
web** (`core/ssh_installer.py`, activada desde la página de Ajustes) - este script es el recurso
manual para quienes prefieren no dar sus credenciales SSH de Recalbox a la aplicación, o necesitan
inspeccionar/personalizar exactamente lo que se instala antes de ejecutarlo.

🇬🇧 [English](README.md) | 🇫🇷 [Français](README_FR.md) | 🇪🇸 Español

# tools/

Utilidades del lado del PC/manuales para ArcadeMatrix_RPi. Son herramientas complementarias independientes (no compiladas en el daemon de Rust) que ejecutas por separado en tu ordenador o en tu consola retro (Recalbox / Batocera / RetroPie).

## `mugen_extractor/`

Convierte los archivos de personajes de MUGEN (`.sff`/`.air`) al formato binario de sprite `.fgt`
usado tanto por `src/engines/fighter.rs` de este proyecto **como** por el `FighterEngine` (C++) del
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
- Un pequeño daemon en Python (`arcadematrix_daemon.py` o hook de eventos de EmulationStation) que monitoriza la selección, el lanzamiento y la salida de juegos y publica `{"status": "playing"|"browsing"|"stopped", "game": "<nombre base de la rom>", "system": "<SystemId>", "title": "<título visible>"}` vía MQTT en `system/playing/<os>` (`system/playing/recalbox`, `system/playing/batocera` o `system/playing/retropie`) mediante `mosquitto_pub`:
  - `"title"` es opcional y se incluye automáticamente cuando el título legible del juego en la consola (`Game=` en `/tmp/es_state.inf` en Recalbox o el argumento `$5` del hook de Batocera) difiere de `"game"`, permitiendo que `dmd_media_showcase` resuelva nombres cortos de ROMs mediante `fallback_query`.
  - Publicar `"browsing"` o `"stopped"` al salir de un juego restaura las marquesinas del menú en `0 ms` y hace que `FrontendSync` aborte de inmediato cualquier proceso de conversión de Showcase en curso en el servidor (`DELETE /api/v1/clients/{sync_id}/queue`, con `0` tráfico HTTP `DELETE` durante la navegación normal por los menús).
- Integración en el arranque para mantener el daemon activo tras cada reinicio.

**Nota para usuarios que también tienen un dispositivo ESP32 ArcadeMatrix**:
Este daemon y el formato de topic estándar (`system/playing/#`) son 100% idénticos y compatibles entre las versiones de ESP32 y Raspberry Pi. Una sola instalación en tu consola sirve para ambos dispositivos simultáneamente. También puedes ejecutar el instalador automatizado desde el PC (`tools/install.sh` / `tools/install.ps1`).

**Normalmente esto ocurre automáticamente mediante la función de instalación SSH de la interfaz
web** (`src/core/ssh_installer.rs`, activada desde la página de Ajustes) - este script es el recurso
manual para quienes prefieren no dar sus credenciales SSH de Recalbox a la aplicación, o necesitan
inspeccionar/personalizar exactamente lo que se instala antes de ejecutarlo.

🇬🇧 [English](QUICKSTART.md) | 🇫🇷 [Français](QUICKSTART_FR.md) | 🇪🇸 Español

# Guía de inicio rápido

Esta guía te ayudará a instalar y configurar ArcadeMatrix en tu Raspberry Pi.

## 1. Instalación (recomendada)

Proporcionamos una imagen precompilada lista para usar (`ArcadeMatrix_Release_aarch64.img.xz` para Pi 3/4/5/Zero 2 W o `armhf.img.xz` para Pi 1/2/Zero).

> **Tarjeta MicroSD requerida: `16 GB` reales como mínimo (`>= 15,1 GB`) — se recomiendan `32 GB`.** La imagen completa ocupa `15,03 GB` (`bootfs` + sistema Linux + partición `DATA` exFAT de ~`6 GB` para tus GIFs, sprites MUGEN y fuentes). Nunca uses una tarjeta de `8 GB` ni una de «16 GB» reducida (`< 15,03 GB`), o fallará la verificación y la partición `DATA` quedará truncada.

1. **Graba la imagen en tu tarjeta MicroSD:**
   - **Grabación directa sin descomprimir (recomendado):** usa **[Raspberry Pi Imager](https://www.raspberrypi.com/software/)** *(Sistema operativo → **Usar imagen personalizada**, selecciona directamente el archivo `.img.xz` y responde **«No»** cuando pregunte si deseas aplicar ajustes de personalización del SO)* o **[Balena Etcher](https://etcher.balena.io/)**.
   - **Si tu herramienta exige descomprimir a `.img` primero** *(ej. Rufus o Win32DiskImager — se siguen recomendando Pi Imager / Etcher porque Win32DiskImager no actualiza la tabla de particiones de Windows al terminar)*: extrae el `.img.xz` **únicamente** con **[7-Zip](https://www.7-zip.org/)** (Windows), **[Keka](https://www.keka.io/)** (macOS) o **`xz -d`** (Linux) disponiendo de al menos `16 GB` libres en disco *(evita el extractor nativo de Windows 11 y WinRAR, que pueden truncar archivos `.xz` mayores de 4 GB)*.
2. **Accede a la unidad USB `DATA`:** desconecta y vuelve a insertar físicamente la tarjeta SD en tu PC/Mac *(pulsa **Cancelar** si Windows sugiere formatear la partición Linux)*. Aparecerá una unidad USB llamada **DATA**.
   - 💡 *En Windows, si solo aparece `bootfs` (`DATA` no se muestra en el Explorador)*: haz **clic derecho en el botón Inicio → Administración de discos** (`diskmgmt.msc`), clic derecho sobre el bloque **`DATA (exFAT)`** → **Cambiar la letra y rutas de acceso de unidad... → Agregar... → Elige una letra (ej. `E:`) → Aceptar** *(usa un adaptador MicroSD por USB si el lector interno de un portátil muestra la opción en gris)*.
3. Abre el archivo `config.json` ubicado en esta unidad **DATA** para introducir tus credenciales de Wi-Fi (`"ssid"` y `"password"`) y el tamaño de tu matriz, y copia tus medios en `gifs/`, `fighters_32/`, `fighters_64/` o `fonts/`.
4. Inserta la tarjeta SD en la Raspberry Pi y enciéndela. ¡La dirección IP se mostrará en la matriz!

## 2. Configuración Web y Actualizaciones OTA

Una vez encendida la Pi, abre un navegador en tu teléfono o PC y ve a:
`http://<RASPBERRY_IP>:8080`

Aquí puedes configurar:
- Los colores, fuentes y temas del reloj y la fecha.
- Las funciones activadas en el bucle de rotación.
- Los ajustes de brillo y modo nocturno.
- 🔄 **Actualización del firmware (OTA)**: ve a la pestaña **System**, arrastra y suelta el binario compilado `arcadematrix_vX.Y.Z_aarch64` y pulsa **Upload & Update Firmware** para actualizar el sistema por Wi-Fi sin volver a grabar tu tarjeta SD.

## 3. Añadir contenido (GIFs, sprites, fuentes)

Para añadir tus propios medios, **simplemente conecta tu tarjeta SD a tu PC/Mac**.
La unidad **DATA** aparecerá como una memoria USB estándar (formato exFAT):

- **GIFs**: déjalos en la carpeta `gifs/`.
- **Sprites MUGEN**: usa nuestro extractor para generar archivos `.fgt` y colócalos en `fighters_32/` o `fighters_64/`.
- **Fonts**: deja fuentes `.ttf` o `.bdf` en la carpeta `fonts/`.

## 4. Conexión de hardware

Recomendamos usar un Adafruit RGB Matrix HAT o Bonnet conectado a una Raspberry Pi Zero 2 W o Pi 4. Asegúrate de que el conector HUB75 esté correctamente enchufado a tu panel LED.

## 5. Opcional: Conectar Recalbox / Batocera / RetroPie y Showcase en Juego

1. **Instalación en Consola en 1 Clic**: Abre **Sistema → MQTT & API → 🎮 FrontendSync**, introduce la IP y credenciales SSH de tu consola Recalbox, Batocera o RetroPie y pulsa **Install Sync Script** (la consola solo guarda la IP del DMD y descubre automáticamente el broker MQTT activo — nunca tendrás que reinstalar el script).
2. **Marquesinas Instantáneas**: Pon **Activar MQTT** en **Activado**. Al navegar por los menús de tu consola se mostrarán las marquesinas Pixelcade al instante (`0 ms`).
3. **Fichas y Vídeos de Gameplay en Juego: Modo Online, Offline o Híbrido** (con **[dmd_media_showcase](https://github.com/red77290/dmd_media_showcase)**):
   - 🌐 **Modo 100 % Online (Streaming directo sin copiar archivos)**: En **🎬 DMD Media Showcase**, activa el servidor Showcase (`🟢 Comunicación OK`), y en **🎮 FrontendSync**, activa **Usar Broker MQTT de Showcase si está activo** y **Showcase en Juego (Tarjeta + Vídeo Gameplay)** (`Alternar Marquesina y Showcase`, `Una vez` o `Bucle`).
   - 💾 **Modo 100 % Offline (Sin PC encendido ni Internet)**: En la pestaña **`7. Batch Scraper & Packs Offline`** de Showcase (desde un PC, NAS o móvil), importa tus `gamelist.xml` de Recalbox/Batocera y copia **`showcase/`** (o `showcase_tate/` en vertical) + **`pixelcade/`** en la unidad USB **DATA** (así como playlists de reloj `snes_platformer`... en `DATA:/gifs/`). Con **Showcase en Juego** activo en **🎮 FrontendSync**, tus `.gif` `RGB888` arrancan en **`0 ms`** incluso con Showcase Server apagado.
   - ⚡ **Modo Híbrido (Automático)**: Si `DATA:/showcase/` existe **y** Showcase Server está activo, la Raspberry Pi reproduce siempre primero el `.gif` local (`0` tráfico de red) y consulta al servidor Online solo para juegos nuevos que no estén en la tarjeta SD ([Tutorial Completo para Principiantes](https://github.com/red77290/dmd_media_showcase/blob/main/docs/TUTORIAL_ONLINE_OFFLINE_SCRAPING_ES.md)).

## 6. Anexo: Uso de las Herramientas para PC y Consolas (`ArcadeMatrix-tools.zip`)

Todas las herramientas opcionales para PC, Mac, Linux y consolas retro están disponibles en un paquete independiente — **sin necesidad de descargar ni clonar el código fuente**:

👉 **[⬇️ Descargar `ArcadeMatrix-tools.zip` (Última Release)](https://github.com/red77290/ArcadeMatrix_RPI/releases/latest/download/ArcadeMatrix-tools.zip)** *(también disponible en el [Portal Raspberry Pi](https://red77290.github.io/ArcadeMatrix_RPI/)).*

Descomprime `ArcadeMatrix-tools.zip` en tu ordenador para utilizar:
- **Sincronización de Consola y Daemon Universal de Récords (`ArcadeMatrix-tools/install.sh`, `ArcadeMatrix-tools/install.ps1`, `ArcadeMatrix-tools/rpi_emulationstation_base_os_setup.sh`)**: Alternativa manual al instalador en 1 clic de la Web UI para Recalbox, Batocera y RetroPie — muestra marquesinas en `0 ms` y celebra nuevos récords High-Score en ~3.100 ROMs MAME/FBNeo `.hi` + NVRAM Neo-Geo/CPS.
- **Extractor de Sprites MUGEN (`ArcadeMatrix-tools/mugen_extractor/`)**: Ejecuta `start_extractor.bat` (Windows) o `./start_extractor.sh` (macOS / Linux) para convertir personajes MUGEN (`chars/`) en sprites `.fgt` + `index.txt` listos para copiar en `DATA:/fighters_32/` o `DATA:/fighters_64/`.
- **Blueprints de Home Assistant (`ArcadeMatrix-tools/home_assistant/blueprints/`)**: Plantillas YAML listas para importar y enviar sensores, gráficas de 24h, tablas y clima al motor `mqttdata`.


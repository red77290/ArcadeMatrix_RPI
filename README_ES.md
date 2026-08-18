🇬🇧 [English](README.md) | 🇫🇷 [Français](README_FR.md) | 🇪🇸 Español

# ArcadeMatrix RPi 🍓👾

📺 **Demostración en Video / Presentación:** https://youtu.be/vAK880Io8yo?si=KSieTymlE7fLyZQs

La implementación original en **Rust** del proyecto **ArcadeMatrix**, diseñada específicamente para ejecutarse en una **Raspberry Pi** conectada a una matriz LED RGB (HUB75) a través del HAT de Adafruit o hardware Joy-IT.

Desarrollada en paralelo con la versión C++ para ESP32, esta implementación aprovecha la potencia de la Raspberry Pi para ofrecer un pipeline gráfico multihilo de alto rendimiento.

📚 **Documentación para desarrolladores:** [Primeros pasos (workspace dev)](docs/GETTING_STARTED_ES.md) · [Guía del desarrollador](docs/DEVELOPER_ES.md) · [Arquitectura](docs/ARCHITECTURE_ES.md) · [Guía rápida (usuarios finales)](docs/QUICKSTART_ES.md)

---

## 💾 Guía Rápida: Imagen precompilada (Recomendado)

Proporcionamos archivos `.img` precompilados y totalmente automatizados, publicados automáticamente en cada versión.

| Arquitectura | Recomendado para | Compatible con | Descargar |
|--------------|------------------|----------------|-----------|
| **64-bits (aarch64)** | Raspberry Pi 3, 4, 5, Zero 2 W | Pi 3, 4, 5, Zero 2 W | [⬇️ Descargar Imagen 64-bits](https://github.com/red77290/ArcadeMatrix_RPI/releases/latest/download/ArcadeMatrix_Release_aarch64.img.xz) |
| **32-bits (armhf)** | Raspberry Pi 1, 2, Zero (Original) | Todos los modelos Raspberry Pi | [⬇️ Descargar Imagen 32-bits](https://github.com/red77290/ArcadeMatrix_RPI/releases/latest/download/ArcadeMatrix_Release_armhf.img.xz) |

*(Ambos son archivos `.img.xz`: descomprímelos con 7-Zip/Keka/`xz -d` antes de grabarlos. Consulta la [lista completa de releases](https://github.com/red77290/ArcadeMatrix_RPI/releases) para versiones antiguas.)*

1. Graba el `.img` en tu tarjeta SD con **Raspberry Pi Imager**.
2. Cuando termine, vuelve a insertar la tarjeta SD en tu PC/Mac. ¡Verás aparecer una gran unidad USB **DATA** de 8 GB!
3. Abre el archivo `conf.ini` ubicado en esa unidad DATA para configurar el tamaño de tu matriz y tus credenciales de **Wi-Fi** (`SSID` y `PASS`).
4. Inserta la tarjeta SD en la Raspberry Pi y enciéndela.
5. La matriz se encenderá de inmediato y **mostrará la dirección IP** durante 5 segundos. ¡Usa esa IP para acceder a la interfaz Web!

---

## 🌟 Funciones (exclusivas de RPi vs ESP32)

* **Fuentes cargables dinámicamente (`.ttf`)**: ¡se acabaron los archivos de fuente hardcodeados! Deja cualquier fuente `.ttf` u `.otf` directamente en la carpeta `fonts/`, y la interfaz Web la listará automáticamente para usarla en el reloj o la fecha.
* **Tamaños y desplazamientos de reloj/fecha ilimitados**: ya no estás restringido a los tamaños 1, 2 o 3. Puedes establecer cualquier tamaño y colocar el texto libremente en paneles de matriz enormes (p. ej. 256x64).
* **Selección masiva de relojes**: disfruta de una variedad de relojes animados, incluidos los clásicos Arcade, Binary, Cyberpunk, Flip, Word, y los nuevos relojes **Pac-Man**, **Tetris**, **SlotMachine** y **Versus (Mugen)**.
* 📈 **Tickers de Criptomonedas y Bolsa en tiempo real**: cotizaciones en vivo y distintivos % 24h de CoinGecko, Binance y Yahoo Finance con caché configurable.
* **Lluvia digital Matrix real (Katakana)**: un efecto Matrix totalmente personalizado, ultra fluido y auténtico (`DotGothic16`) con Katakana de media anchura cayendo y texto en espacio negativo de «LED apagados» atravesando la lluvia.
* **Gradientes suaves personalizados**: además de los temas clásicos Publisher (Nintendo, Capcom, Sega...), ahora puedes elegir un tema **Custom Color / Gradient** y seleccionar dos colores para generar un gradiente dinámico.
* **Playlists de imágenes dinámicas (GIF/PNG/JPG)**: lee archivos `.gif` y `.png` reales de forma dinámica directamente desde el sistema de archivos, sin problemas de fragmentación en la tarjeta SD.
* **Potencia de Rust**: todo el motor, la API y el frontend se sirven con Rust (`image-rs` para dibujar, `Actix-web` para la API), lo que permite modificarlo mucho más rápido.

---

## 🚀 Requisitos de hardware y Compatibilidad

Gracias a la implementación nativa ultraligera en Rust (~5 MB binario, ~10 MB RAM, 0% CPU en reposo), **ArcadeMatrix ahora es totalmente compatible con hardware Raspberry Pi antiguo sin saltos de frames ni lag**:

1. **Raspberry Pi**: 
   - **Modelos Antiguos / Single-Core**: Pi 1 (B, B+, A+), Pi Zero, Pi Zero W *(¡Totalmente compatible sin lag gracias a Rust!)*
   - **Multi-Core**: Pi 2, Pi 3, Pi 4, Pi Zero 2 W *(Recomendado)*
   *(⚠️ **Advertencia para Pi 5**: la biblioteca hzeller rgb-led-matrix NO es compatible de forma nativa con Pi 5 a través de GPIO debido al nuevo chip RP1. ¡Debes usar una placa adaptadora activa para Pi 5! Se recomiendan encarecidamente Pi 4 o Zero 2W.)*
2. **Matriz LED RGB**: paneles HUB75 (p. ej. 64x64, 128x32, 256x64).
3. **Adafruit RGB Matrix HAT** (o Joy-IT, o cableado personalizado).
4. **Tarjeta MicroSD** (se recomiendan 16 GB o más para la imagen precompilada).

---

## 🛠️ Instalación Avanzada\n\n### Opción 2: instalación manual
Si prefieres instalarlo manualmente sobre una **Raspberry Pi OS Lite (64-bit)** recién instalada:
Una vez conectado a tu Raspberry Pi por SSH:

```bash
curl -sSL https://raw.githubusercontent.com/red77290/ArcadeMatrix_RPI/main/install.sh | bash
```
*(Si el repositorio es privado, primero tendrás que hacer `git clone` manualmente y luego ejecutar `./install.sh` dentro de la carpeta.)*

El script hará automáticamente lo siguiente:
1. Instalar Rust, Actix-web, image-rs y `build-essential`.
2. Descargar y compilar el driver `hzeller/rpi-rgb-led-matrix`.
3. Configurar `systemd` para iniciar ArcadeMatrix automáticamente al arrancar.

### Opción 3: Smart Deploy a través de ordenador
Si deseas compilar la aplicación en tu propio ordenador (mucho más rápido) y desplegarla automáticamente en la Raspberry Pi, puedes usar los scripts de despliegue inteligente basados en Docker.

**En macOS / Linux:**
```bash
bash scripts/deploy.sh --ip <PI_IP> --user <PI_USER> --pass <PI_PASS>
# Ejemplo: bash scripts/deploy.sh --ip 192.168.1.177 --user pi --pass raspberry
```

**En Windows (PowerShell):**
```powershell
.\scripts\deploy.ps1 -PI_IP "192.168.1.177" -PI_USER "pi" -PI_PASS "raspberry"
```
*El script detectará automáticamente la arquitectura de tu Pi, compilará el binario usando Docker, detendrá el servicio remoto, subirá el archivo y reiniciará la aplicación.*

---

## ⚠️ Advertencia de Hardware: Wi-Fi & Interferencias (Líneas VHS)

Las Raspberry Pi (especialmente Pi 3 y Zero W) comparten el reloj del bus Wi-Fi interno con el controlador de hardware **PWM/PCM** utilizado por la matriz LED.

Al controlar matrices grandes (como **256x64**), el controlador DMA debe empujar una cantidad masiva de datos a los pines GPIO. Si el pulso de hardware está habilitado (`disable_hardware_pulsing = false`), esto crea una intensa saturación del ancho de banda DMA que **asfixia al chip Wi-Fi interno (SDIO)**, causando pérdida severa de paquetes, lag y desconexiones.

Aunque un Wi-Fi inestable puede ser aceptable si solo usas la interfaz Web ocasionalmente para cambiar el reloj, esto rompe por completo las funciones que dependen de una conexión a internet o red local estable:
* **Recalbox/Batocera/Pixelcade MQTT Sync**: perderá mensajes o se desconectará.
* **Criptomonedas y Bolsa**: las llamadas a la API fallarán por timeout.
* **Clima (Weather)**: las llamadas a la API fallarán por timeout.

**Soluciones para usuarios de 256x64:**
1. **Usar un cable Ethernet (RJ45)** (Pi 3B/4): Evita el chip Wi-Fi SDIO por completo.
2. **Usar un adaptador Wi-Fi USB**: Los controladores USB usan un bus interno diferente y son inmunes a la saturación DMA del PWM.
3. **Establecer `disable_hardware_pulsing = true`**: Obliga a la matriz a usar renderizado por software (CPU bit-banging) en lugar de DMA por hardware. Tu Wi-Fi funcionará perfectamente, pero verás un ligero efecto de "líneas VHS" (parpadeo) en la matriz.

*(Nota: Las matrices **128x32** requieren 4 veces menos ancho de banda DMA, por lo que suelen funcionar perfectamente con el pulso de hardware habilitado y el Wi-Fi interno activo).*

*(Nota sobre la **Raspberry Pi 4**: La Pi 4 usa una arquitectura PCIe y un controlador DMA mucho más rápidos. Es muy probable que NO se vea afectada por este error de saturación Wi-Fi en 256x64. Sin embargo, esto aún no se ha probado oficialmente ya que no tenemos una Pi 4 a mano para confirmarlo).*

### Tabla de Compatibilidad Raspberry Pi & Resolución

| Resolución Matriz | Configuración de Red / Hardware Pulsing | Calidad de Imagen | Wi-Fi / APIs / MQTT |
|-------------|-----------------------------|----------------|-------------------|
| **128x32**  | Wi-Fi Interno + `disable_hardware_pulsing = false` | ✅ Perfecta | ✅ Estable |
| **256x64**  | **Cable Ethernet (RJ45)** + `disable_hardware_pulsing = false` | ✅ Perfecta | ✅ Estable |
| **256x64**  | **Adaptador Wi-Fi USB** + `disable_hardware_pulsing = false` | ✅ Perfecta | ✅ Estable |
| **256x64**  | Wi-Fi Interno + `disable_hardware_pulsing = true` | ⚠️ Ligero parpadeo | ✅ Estable |
| **256x64**  | Wi-Fi Interno + `disable_hardware_pulsing = false` | ✅ Perfecta | ❌ Roto / Timeouts |

---

## 🎨 Gestión de medios

La imagen precompilada incluye una **partición DATA dedicada de 8 GB** formateada como exFAT. ¡Eso significa que puedes conectar la tarjeta SD directamente a tu ordenador Windows o Mac para arrastrar y soltar tus archivos sin necesidad de SSH ni FTP!

### Sprites & GIFs
* **`/fighters_32/`** o **`/fighters_64/`**: coloca aquí tus sprites `.fgt` (consulta la sección de sprites MUGEN más abajo).
* **`/gifs/`**: deja tus bucles `.gif` dentro de carpetas aquí.
La interfaz Web analizará automáticamente esas carpetas y te permitirá marcar las que quieras reproducir.

### Fonts
* **`/fonts/`**: deja aquí tus archivos `.ttf`, `.otf` o `.bdf`. 
De forma predeterminada, el proyecto incluye `PressStart2P.ttf`, `VT323.ttf` y `DotGothic16.ttf`.

---

## 🕸️ Interfaz Web
Ve a `http://<YOUR_PI_IP>:8080/` para acceder al panel de control.

La interfaz es exactamente la misma que en la versión ESP32, y ofrece controles del Dashboard, selección de Playlist, configuración del reloj y ajustes MQTT, con controles adicionales para **Gradients** y **Unlimited Sizes**.

---

## 🕹️ Integración con Recalbox & Batocera (Pixelcade Marquees)

ArcadeMatrix es compatible con marquees dinámicos **estilo Pixelcade** cuando seleccionas o juegas un juego en tu Recalbox o Batocera.

Mientras navegas por tus listas de juegos, la Raspberry Pi descargará las marquees oficiales de Pixelcade desde GitHub **en segundo plano y en tiempo real**, las almacenará en caché en tu tarjeta SD y las mostrará en tu matriz LED. Si un juego no tiene imagen, mostrará un elegante texto animado de respaldo.

### Instalación automática (recomendada)
Ve a la pestaña **MQTT** en la interfaz Web de ArcadeMatrix, introduce la IP de tu Recalbox/Batocera junto con su contraseña root (por defecto `recalboxroot` o `linux`) y haz clic en **Install Sync Script**. Esto inyectará automáticamente el daemon a través de SSH.

### Instalación manual (Recalbox)
Si prefieres la instalación manual, o si falla la instalación por red:
1. Abre el archivo `tools/recalbox_setup_mqtt.sh` incluido en el proyecto.
2. Edita la línea `MQTT_BROKER="192.168.1.xxx"` y establece la IP de la Raspberry Pi que ejecuta la matriz LED.
3. Copia el archivo `tools/recalbox_setup_mqtt.sh` a tu Recalbox (por ejemplo, en `/recalbox/share/`).
4. Conéctate por SSH a tu Recalbox y ejecuta: `bash /recalbox/share/recalbox_setup_mqtt.sh`.

### ¿Cómo funciona la arquitectura del daemon?
A diferencia de los scripts nativos de Recalbox que se ejecutan (y congelan el sistema) con cada movimiento del joystick, ArcadeMatrix instala **un daemon Rust ultraligero en segundo plano**.
* **Cero lag:** consume un 0 % de CPU. EmulationStation no sufre stutter ni lag, incluso desplazándose a máxima velocidad.
* **Anti-spam (debounce):** si recorres rápidamente 50 juegos, el daemon no saturará la red. Solo envía el mensaje a la matriz si te detienes en un juego durante más de 150 milisegundos.
* **Thread safety:** del lado de la matriz LED, las descargas y el motor de dibujo están separados por threads con locks robustos, lo que evita cuelgues y corrupción en la caché de imágenes.

---

## 🔧 Configuración de la matriz
Si tienes una matriz más grande que 64x64 o 128x32, o si usas un HAT que no sea Adafruit, puede que necesites ajustar los argumentos de `hzeller` en `src/core/matrix.rs`. De forma predeterminada, está configurado como `--led-gpio-mapping=adafruit-hat` y `128x32`.

También puedes cambiar dinámicamente el brillo de la matriz desde los ajustes de la interfaz Web.
- Activa los modos Standby/Night.

---

## 📂 Gestión de medios (GIFs y sprites MUGEN)

### Añadir GIFs
Simplemente deja cualquier archivo `.gif` estándar en el directorio `gifs/`:
```text
ArcadeMatrix_RPi/
└── gifs/
    ├── mario_run.gif
    ├── sonic_wait.gif
    └── ...
```

### Añadir sprites MUGEN
Para lograr un rendimiento perfecto a 60fps y alineaciones exactas de «virtual ground» en enormes plantillas de personajes, el motor Fighter usa archivos `.fgt` preprocesados junto con un manifiesto `index.txt`.

**¡No puedes simplemente dejar imágenes sin procesar en las carpetas fighters!**
Debes usar la herramienta `mugen_extractor.py` incluida en la carpeta `tools/mugen_extractor/` para procesar tus personajes MUGEN. 

El extractor leerá archivos `.sff` y `.air` de MUGEN, calculará las bounding boxes perfectas para evitar el jitter de las animaciones y exportará archivos `.fgt` optimizados directamente a tus carpetas `fighters_32/` y `fighters_64/`.

Consulta `tools/mugen_extractor/README_ES.md` para ver las instrucciones completas sobre cómo añadir más personajes MUGEN.

---

## ⚙️ Configuración avanzada (conf.ini)

Si prefieres editar los ajustes manualmente en lugar de usar la interfaz Web, puedes editar directamente el archivo `conf.ini` ubicado en la partición **DATA** de tu tarjeta SD. 
Esto es especialmente útil para configurar el Wi-Fi antes del primer arranque.

### 🌐 [WIFI]
| Parameter | Default | Description |
|---|---|---|
| `SSID` | `YourNetworkName` | The name of your Wi-Fi network. |
| `PASS` | `YourNetworkPassword` | The password for your Wi-Fi network. |
| `CONFIGURED` | `false` | Set to `false` to force the Raspberry Pi to attempt a Wi-Fi connection on its next boot. Automatically sets back to `true` on success. |

### 🎛️ [MATRIX]
| Parameter | Default | Description |
|---|---|---|
| `ROWS` / `COLS` | `32` / `64` | The pixel dimensions of a single LED panel. |
| `HARDWARE_MAPPING` | `adafruit-hat` | Type of HAT/wiring used. (`adafruit-hat`, `adafruit-hat-pwm`, `regular-pi1`, `regular`). |
| `CHAIN` / `PARALLEL` | `1` / `1` | `CHAIN` for horizontal daisy-chaining. `PARALLEL` for vertical stacking on multiple HUB75 ports. |
| `SLOWDOWN` | `2` | Hardware slowdown (1 to 4). Increase if your Matrix has flickering or visual artifacts (especially Pi 3/4). |
| `disable_hardware_pulsing`| `false` | **CRÍTICO:** Cámbialo a `true` para evitar que el chip Wi-Fi interno falle (a costa de un ligero parpadeo). |
| `BRIGHTNESS` | `100` | Global matrix brightness (1 to 100). |
| `RGB_SEQUENCE` | `RGB` | Color order. Change to `RBG` or `BGR` if your colors look swapped. |

### ⏰ [TIME] & [DATE]
| Parameter | Default | Description |
|---|---|---|
| `FORMAT_24H` | `true` | `true` for 24-hour format, `false` for 12-hour AM/PM format. |
| `CLOCK_FONT` | `DotGothic16.ttf`| Name of the `.ttf` or `.bdf` file in the `/fonts/` folder to use for the clock. |
| `CLOCK_SIZE` | `16` | Font size (scaling factor) for the clock. |
| `THEME` | `0` | The numeric ID of the animated clock theme (e.g. 19 for Flip, 21 for True Matrix). |
| `CLOCK_COLOR_1` | `#ffffff` | Primary hex color. Used for gradients if theme is Custom (20). |
| `CLOCK_COLOR_2` | `#ffffff` | Secondary hex color. Used for gradients if theme is Custom (20). |

*(La sección `[DATE]` contiene parámetros idénticos para configurar la visualización de la fecha.)*

### 🔄 [IDLE]
| Parameter | Default | Description |
|---|---|---|
| `ROTATION` | `all` | Dictates rotation behavior (`clock`, `gifs`, `sprites`, or `all`). |
| `CLOCK_DURATION_SEC`| `10` | How long the clock/date stays on screen during the rotation loop. |
| `GIF_DURATION_SEC` | `10` | How long a single GIF stays on screen before advancing. |
| `SELECTED_GIFS` | *(empty)* | Comma-separated list of media to loop. Leave empty to play everything. |
| `SELECTED_SPRITES` | *(empty)* | Comma-separated list of sprites to loop. Leave empty to play everything. |

### 📈 [CRYPTO] & 📊 [STOCK]
| Parameter | Default | Description |
|---|---|---|
| `SYMBOLS` | `BTC,ETH,SOL,DOGE` / `AAPL,NVDA,TSLA,MSFT` | Comma-separated list of symbols to display. |
| `CACHE_TTL_MIN` | `1` | Refresh rate / Cache TTL in minutes (prevents API rate limiting). |

### 🌙 [STANDBY]
| Parameter | Default | Description |
|---|---|---|
| `NIGHT_MODE_ENABLED`| `false` | If `true`, the Matrix will automatically turn off and wake up. |
| `TURN_OFF_AT` | `23:00` | HH:MM formatted time for screen sleep. |
| `WAKE_UP_AT` | `07:00` | HH:MM formatted time for screen wake. |

### 🔒 [API]
| Parameter | Default | Description |
|---|---|---|
| `AUTH_ENABLED` | `false` | If `true`, requires the `X-API-Token` header to match `TOKEN` on the sensitive endpoints: `/api/wifi`, `/api/mqtt/install`, `/api/system/reboot`, `/api/system/shutdown`. Disabled by default so the bundled Web UI keeps working out of the box; enable it if the device is reachable beyond a trusted LAN. |
| `TOKEN` | *(auto-generated)* | A random token generated on first boot. Copy it here (or read it from `conf.ini` after first run) and send it as `X-API-Token` when calling protected endpoints. |

## 🙏 Agradecimientos

Un enorme agradecimiento a la comunidad de código abierto y a los creadores de las increíbles bibliotecas que impulsan este proyecto:
- **[rpi-rgb-led-matrix](https://github.com/hzeller/rpi-rgb-led-matrix)** por hzeller (y los bindings de Rust por AidanWallace)
- **[Actix-web](https://github.com/actix/actix-web)** por la rapidísima API web
- **[image-rs](https://github.com/image-rs/image)** por el procesamiento de imágenes
- **[rumqttc](https://github.com/bytebeamio/rumqtt)** por el soporte MQTT
- ¡Y a toda la comunidad de Rust por crear un ecosistema tan asombroso (Tokio, Serde, reqwest, tracing, etc.)!

¡Un agradecimiento especial al **RPiTeam** por el increíble pack de 600 GIFs!

## 📜 Licencia
Este proyecto está licenciado bajo la **[PolyForm Noncommercial License 1.0.0](LICENSE)**.

**En resumen:** eres libre de usar, modificar y compartir este proyecto para cualquier propósito no comercial (uso personal, proyectos hobbyistas, investigación, educación, organizaciones públicas/sin fines de lucro) - consulta el archivo [LICENSE](LICENSE) completo para los términos exactos. **Cualquier uso comercial (venta de unidades ensambladas, kits, o productos/servicios derivados) requiere una licencia separada - contacta a [Red1L](https://github.com/red77290) para discutir los términos comerciales.**

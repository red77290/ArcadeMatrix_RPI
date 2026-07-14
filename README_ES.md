# ArcadeMatrix RPi 🍓👾

Un port basado en Python del proyecto **ArcadeMatrix**, diseñado específicamente para ejecutarse en una **Raspberry Pi** conectada a una matriz LED RGB (HUB75) a través del HAT de Adafruit o hardware Joy-IT.

Este proyecto replica las increíbles características de la versión ESP32 eliminando por completo sus limitaciones de hardware.

---

## 🌟 Características (Exclusivas de RPi vs ESP32)

* **Fuentes cargables dinámicamente (`.ttf`)**: ¡Se acabaron los archivos de fuentes codificados en el sistema! Suelta cualquier fuente `.ttf` u `.otf` directamente en la carpeta `fonts/`, y la interfaz web la listará automáticamente para usarla en el Reloj o la Fecha.
* **Tamaños y desplazamientos de Reloj/Fecha ilimitados**: Ya no estás restringido al Tamaño 1, 2 o 3. Puedes establecer el tamaño a cualquier número y posicionar el texto libremente en paneles matriciales masivos (ej. 256x64).
* **Selección masiva de relojes**: ¡Disfruta de una variedad de relojes animados incluyendo los clásicos Arcade, Binario, Cyberpunk, Flip, Palabras, y los nuevos relojes **Pac-Man**, **Tetris**, **SlotMachine** (Tragamonedas) y **Versus (Mugen)**!
* **Verdadera lluvia digital Matrix (Katakana)**: Un efecto de lluvia digital Matrix completamente personalizado, fluido y genuino (`DotGothic16`) con Katakanas cayendo y texto en espacio negativo (LEDs apagados) que perfora la lluvia.
* **Degradados suaves personalizados**: Además de los temas clásicos de desarrolladores (Nintendo, Capcom, Sega...), ahora puedes elegir un tema de **Color / Degradado Personalizado** y elegir dos colores para generar un degradado dinámico.
* **Listas de reproducción dinámicas de imágenes (GIF/PNG/JPG)**: Lee archivos `.gif` y `.png` reales dinámicamente desde el sistema de archivos sin problemas de fragmentación de la tarjeta SD.
* **El poder de Python**: Todo el motor, la API y el frontend son servidos por Python (`Pillow` para dibujar, `Flask` para la API), permitiendo modificaciones mucho más rápidas.

---

## 🚀 Requisitos de Hardware

1. **Raspberry Pi**: Cualquier modelo hasta Pi 4 (Zero 2 W, Pi 3, Pi 4). 
   *(⚠️ **Advertencia Pi 5**: La librería hzeller rgb-led-matrix NO soporta la Pi 5 de forma nativa a través de los pines GPIO debido al nuevo chip RP1. ¡Debes usar una placa adaptadora activa para la Pi 5! Se recomiendan encarecidamente la Pi 4 o Zero 2W).*
2. **Matriz LED RGB**: Paneles HUB75 (ej. 64x64, 128x32, 256x64).
3. **Adafruit RGB Matrix HAT** (o Joy-IT, o cableado personalizado).
4. **Tarjeta MicroSD** (Se recomiendan 16 GB o más para la imagen precompilada).

---

## 💾 Instalación y Configuración

### Opción 1: Imagen Precompilada (Recomendada para usuarios)
Proporcionamos un archivo `.img` precompilado y totalmente automatizado (`ArcadeMatrix_Release.img`).
1. Flashea el archivo `.img` a tu tarjeta SD usando **Raspberry Pi Imager**.
2. Una vez flasheada, inserta la tarjeta SD en tu PC/Mac. ¡Verás aparecer una unidad USB de **8 GB llamada DATA**!
3. Abre el archivo `conf.ini` situado en esta unidad DATA para configurar el tamaño de tu Matriz y tus credenciales **Wi-Fi** (`SSID` y `PASS`).
4. Conecta la tarjeta SD a tu Raspberry Pi y enciéndela.
5. La Matriz se encenderá inmediatamente y **mostrará la dirección IP** durante 5 segundos. ¡Usa esta IP para acceder a la Interfaz Web!

### Opción 2: Instalación Manual
Si prefieres instalarlo manualmente en una instalación limpia de **Raspberry Pi OS Lite (64-bit)**:
Una vez conectado a tu Raspberry Pi mediante SSH:

```bash
curl -sSL https://raw.githubusercontent.com/red77290/ArcadeMatrix_RPI/main/install.sh | bash
```

El script automáticamente:
1. Instalará Python 3, Flask, Pillow y `build-essential`.
2. Descargará y compilará el controlador `hzeller/rpi-rgb-led-matrix`.
3. Desactivará el audio de la placa base para evitar parpadeos (flicker) en la matriz LED.
4. Configurará `systemd` para iniciar automáticamente ArcadeMatrix al arrancar.

---

## 🎨 Gestión de Medios

La imagen precompilada cuenta con una **partición DATA de 8 GB** formateada en exFAT. ¡Esto significa que puedes conectar tu tarjeta SD directamente a tu computadora con Windows o Mac para arrastrar y soltar tus archivos sin necesidad de SSH o FTP!

### Sprites & GIFs
* **`/fighters_32/`** o **`/fighters_64/`**: Coloca tus sprites `.fgt` aquí (Consulta la sección de Sprites MUGEN a continuación).
* **`/gifs/`**: Suelta tus bucles `.gif` en carpetas aquí dentro.
¡La interfaz web escaneará automáticamente estas carpetas y te permitirá marcar las que deseas reproducir!

### Fuentes (Fonts)
* **`/fonts/`**: Suelta tus archivos `.ttf`, `.otf` o `.bdf` aquí. 
Por defecto, el proyecto incluye `PressStart2P.ttf`, `VT323.ttf` y `DotGothic16.ttf`.

---

## 🕸️ Interfaz Web (Web UI)
Navega a `http://<IP_DE_TU_PI>:8080/` para acceder al panel de control.

La interfaz es exactamente la misma que la versión ESP32, ofreciendo controles del panel, selección de listas de reproducción, configuración del reloj y configuración MQTT, con controles añadidos para **Degradados** y **Tamaños Ilimitados**.

---

## 🔧 Configuración de la Matriz
Si tienes una matriz más grande que 64x64 o 128x32, o si estás utilizando un HAT no oficial de Adafruit, es posible que necesites ajustar los argumentos de `hzeller` en `core/matrix.py`. Por defecto, está establecido en `--led-gpio-mapping=adafruit-hat` y `128x32`.

También puedes cambiar el brillo de la matriz de forma dinámica a través de los Ajustes de la interfaz web.
- Habilita los modos de Espera/Noche.

---

## 📂 Gestión Avanzada de Medios (Sprites MUGEN)

### Añadir GIFs
Simplemente suelta cualquier archivo `.gif` estándar en la carpeta `gifs/`:
```text
ArcadeMatrix_RPi/
└── gifs/
    ├── mario_run.gif
    ├── sonic_wait.gif
    └── ...
```

### Añadir Sprites MUGEN
Para lograr un rendimiento perfecto a 60 fps y alineaciones exactas del "suelo virtual" a lo largo de listas masivas de personajes, el motor Fighter utiliza archivos preprocesados `.fgt` junto con un manifiesto `index.txt`.

**¡No puedes simplemente soltar imágenes PNG/GIF sin procesar en las carpetas de los luchadores!**
DEBES utilizar la herramienta `mugen_extractor.py` proporcionada en la carpeta `tools/mugen_extractor/` para procesar tus personajes MUGEN. 

El extractor leerá los archivos `.sff` y `.air` de MUGEN, calculará las cajas de colisión perfectas (bounding boxes) para evitar temblores de animación, y exportará archivos optimizados `.fgt` directamente en tus carpetas `fighters_32/` y `fighters_64/`.

¡Consulta `tools/mugen_extractor/README_ES.md` para ver las instrucciones completas sobre cómo añadir más personajes MUGEN!

---

## ⚙️ Configuración Avanzada (conf.ini)

Si prefieres editar la configuración manualmente en lugar de usar la interfaz web, puedes editar directamente el archivo `conf.ini` ubicado en la partición **DATA** de tu tarjeta SD.
Esto es especialmente útil para configurar el Wi-Fi antes del primer arranque.

### 🌐 [WIFI]
* `SSID`: El nombre de tu red Wi-Fi.
* `PASS`: La contraseña de tu red Wi-Fi.
* `CONFIGURED`: Establécelo en `false` para forzar a la Raspberry Pi a intentar conectarse en el próximo inicio. Una vez conectada, el sistema lo cambia automáticamente a `true`.

### 🎛️ [MATRIX]
* `ROWS` & `COLS`: Las dimensiones en píxeles de un solo panel LED (ej., `ROWS=32`, `COLS=64`).
* `HARDWARE_MAPPING`: El tipo de HAT o cableado utilizado. Usa `adafruit-hat` o `adafruit-hat-pwm` para los HAT de Adafruit. Usa `regular-pi1` o `regular` si cableas directamente a los pines GPIO.
* `CHAIN` & `PARALLEL`: Usa `CHAIN` para especificar cuántos paneles están en cadena horizontalmente. Usa `PARALLEL` si estás usando varios puertos HUB75 verticalmente.
* `SLOWDOWN`: Aumenta este valor (de 1 a 4) si tu matriz parpadea o muestra fallos visuales (especialmente en Raspberry Pi 3 y 4).

### ⏰ [TIME] & [DATE]
* `FORMAT_24H`: Ponlo en `true` para usar el formato de 24 horas, o `false` para formato de 12 horas AM/PM.
* `CLOCK_FONT`: El nombre del archivo `.ttf` o `.bdf` que debe estar dentro de `/fonts/`.
* `THEME`: El número de ID del reloj animado o del tema de fecha (tal como aparece en la Web UI).

### 🔄 [IDLE]
* `ROTATION`: Establece el comportamiento de rotación (`clock`, `gifs`, `sprites`, o `all`).
* `CLOCK_DURATION_SEC`: Cuánto tiempo permanece el reloj en pantalla.
* `SELECTED_GIFS` / `SELECTED_SPRITES`: Una lista separada por comas de los archivos que quieres en bucle. Déjalo vacío para reproducir todo.

### 🌙 [STANDBY]
* `NIGHT_MODE_ENABLED`: Si es `true`, la matriz se apagará y se despertará automáticamente en los horarios especificados.
* `TURN_OFF_AT` & `WAKE_UP_AT`: Horas en formato HH:MM para el apagado programado.

## 📜 Licencia
Este proyecto es de código abierto. ¡Disfruta de tu Reloj Arcade Retro Definitivo!

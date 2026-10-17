🇬🇧 [English](README.md) | 🇫🇷 [Français](README_FR.md) | 🇪🇸 Español

# ArcadeMatrix RPi 🍓👾

[![Comunidad Telegram](https://img.shields.io/badge/Telegram-Unirse%20a%20la%20Comunidad-2CA5E0?style=for-the-badge&logo=telegram&logoColor=white)](https://t.me/+DrSdAO8UMUViNmE0)
[![Ko-fi](https://img.shields.io/badge/Ko--fi-Apoyar%20el%20Proyecto-FF5E5B?style=for-the-badge&logo=ko-fi&logoColor=white)](https://ko-fi.com/red1l)

💬 **Comunidad Telegram:** [https://t.me/+DrSdAO8UMUViNmE0](https://t.me/+DrSdAO8UMUViNmE0)  
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
3. Abre el archivo `config.json` ubicado en esa unidad DATA para configurar el tamaño de tu matriz y tus credenciales de **Wi-Fi** (`"ssid"` y `"password"`).
4. Inserta la tarjeta SD en la Raspberry Pi y enciéndela.
5. La matriz se encenderá de inmediato y **mostrará la dirección IP** durante 5 segundos. ¡Usa esta IP para acceder a la Web UI!

---

## 🕹️ Motores Integrados y Simulaciones de Hardware

Cada motor en ArcadeMatrix RPi está diseñado en Rust multiproceso con **cero asignaciones dinámicas en el ciclo de renderizado**, garantizando una frecuencia de refresco de 60 FPS. A continuación se muestran simulaciones precisas de cada motor ejecutándose en pantallas físicas HUB75:

| Motor / ID | Simulación de Hardware | Descripción y Características Clave |
| :--- | :---: | :--- |
| **Desk Master Dashboard**<br>`dashboard` | <img src="docs/assets/engines/engine_dashboard.png" width="240" alt="Motor Dashboard"> | Reloj de escritorio horizontal completo con esfera analógica en pixel-art, segundero fluido, relojes mundiales, clima en vivo OpenWeatherMap y teletipo en directo de criptomonedas Binance y acciones Yahoo Finance. |
| **Reloj Retro Gaming**<br>`clock` | <img src="docs/assets/engines/engine_clock.png" width="240" alt="Reloj Retro"><br><br>[👉 **Ver galería de 10+ relojes retro ➔**](#-colección-signature-de-relojes-retro-temáticos) | Colección exclusiva de relojes animados de arcade y consolas retro (Metal Slug, Castlevania, Mario, Mega Man, Sonic, Pokédex, Pac-Man, Tetris, Reloj Mundial, Lluvia Matrix...) con sprites 100 % fieles al píxel original. |
| **Luchadores Arcade M.U.G.E.N**<br>`fighter` *(Overlay Transversal)* | <img src="docs/assets/engines/engine_fighter.png" width="240" alt="Motor Luchadores"> | Renderizador de superposición transversal (`system.idle_fighter_enabled`, no un ID de motor de rotación independiente) con auténticos sprites de juegos de lucha retro (Street Fighter, King of Fighters, etc.), barras de vida en tiempo real, escenarios personalizados y combates automáticos de CPU sobre relojes y fecha. |
| **Spotify Now Playing**<br>`spotify` | <img src="docs/assets/engines/engine_spotify.png" width="240" alt="Motor Spotify"> | Visualización en tiempo real de la pista en reproducción con carátula a todo color, desplazamiento artista/título, barras de ecualizador animadas y barra de progreso. |
| **Google Cast & Nest**<br>`google_cast` | <img src="docs/assets/engines/engine_googlecast.png" width="240" alt="Motor Google Cast"> | Descubrimiento automático mDNS de dispositivos Google Home / Nest Audio con carátulas de streaming, volumen y progreso de reproducción en vivo. |
| **Ticker y Gráfico Cripto**<br>`crypto` | <img src="docs/assets/engines/engine_crypto.png" width="240" alt="Motor Cripto"> | Precios en vivo de Binance / CoinGecko, indicador de variación en 24h y gráficos sparkline históricos en tiempo real con caché TTL inteligente. |
| **Ticker de Mercado de Valores**<br>`stock` | <img src="docs/assets/engines/engine_stock.png" width="240" alt="Motor Bolsa"> | Cotizaciones en tiempo real de Yahoo Finance, indicadores de 1D % y gráficos sparkline intradiarios para acciones y ETFs de NASDAQ/S&P. |
| **Pronóstico del Clima**<br>`weather` | <img src="docs/assets/engines/engine_weather.png" width="240" alt="Motor Clima"> | Condiciones exteriores en vivo, temperaturas máximas/mínimas, humedad, viento e iconos retro animados mediante OpenWeatherMap y Open-Meteo. |
| **HUD de Telemetría del Sistema**<br>`system_info` | <img src="docs/assets/engines/engine_sysinfo.png" width="240" alt="Motor SysInfo"> | Monitor de doble columna en tiempo real de uso de CPU (%), RAM (%), temperatura del hardware SoC Raspberry Pi (°C/°F) y tiempo de actividad con barras de colores. |
| **Ticker de Noticias en Vivo**<br>`gnews` | <img src="docs/assets/engines/engine_gnews.png" width="240" alt="Motor GNews"> | Titulares de última hora en tiempo real con etiquetas temáticas (`[TECH]`, `[MUNDO]`, `[ECON]`), baliza luminosa parpadeante de emisión en directo y teletipo fluido a 60 FPS. |
| **Home Assistant y MQTT**<br>`mqttdata` | <img src="docs/assets/engines/engine_mqttdata.png" width="240" alt="Motor MQTT Data"> | Paneles de Home Assistant, valores de sensores, gráficos históricos de 24h y pronósticos locales emitidos por MQTT con blueprints listos para usar. |
| **Reproductor de GIFs Animados**<br>`gifs` | <img src="docs/assets/engines/engine_gif.png" width="240" alt="Motor GIF"> | Reproducción fluida a 60 FPS de animaciones retro y listas de reproducción con aceleración de presentación de líneas de copia cero. |
| **Pancarta de Texto Desplazable**<br>`message` | <img src="docs/assets/engines/engine_message.png" width="240" alt="Motor Mensaje"> | Letreros de matriz de puntos personalizables con tipografía ámbar brillante, múltiples direcciones de desplazamiento y activadores de API REST. |
| **Marquesina de Recreativa Arcade**<br>`marquee` | <img src="docs/assets/engines/engine_marquee.png" width="240" alt="Motor Marquesina"> | Muestra marquesinas oficiales de juegos arcade iluminadas mediante integración Pixelcade para sistemas Batocera, Recalbox y RetroPie. |
| **Calendario y Fecha**<br>`date` | <img src="docs/assets/engines/engine_date.png" width="240" alt="Motor Fecha"> | Fecha en tamaño grande con diseño arcade, sombras proyectadas 3D estilo Capcom/Nintendo, soporte multilingüe (ES, EN, FR) y sincronización NTP. |

> [!NOTE]
> **Aviso sobre el renderizado de hardware:** Las capturas de pantalla, vistas previas de motores y posters de relojes presentados en esta documentación son simulaciones de software de alta fidelidad diseñadas para ilustrar el diseño, las animaciones y la telemetría. El aspecto visual real en un panel físico de matriz LED HUB75 puede variar según el paso de píxel (pitch), el filtro difusor acrílico, el brillo de los LEDs y la iluminación ambiental.

### 🌟 Puntos Clave de la Arquitectura (Exclusivas de RPi y Alineación ESP32)

* 🎯 **Motor de Arbitraje y Preferencia Canónico (`DisplayArbiter`)** : pipeline de decisión acotado en $O(1)$ sin asignaciones con arbitraje de prioridades (`Rotation` 10 $\to$ `GIF` 20 $\to$ `Marquee` 30 $\to$ `MQTT` 40), `PreemptionStack<4>`, identidad de intención estricta y limpieza resiliente de sesiones huérfanas.
* 🧭 **Orientación Dinámica y Modo Tate (`OrientationManager`)** : soporte multiángulo manual (0°, 90°, 180°, 270°) con clasificación geométrica automática (`LayoutClass`) y versionado reactivo (`DisplayGeometry`).
* 🔄 **Actualización de Firmware OTA (Over-The-Air)** : ¡Actualiza el binario nativo de Rust directamente desde la interfaz web sin tener que volver a flashear la imagen de tu tarjeta SD! *(Recuperación Failsafe: Si una actualización OTA rompe tu sistema, conecta tu tarjeta SD a un PC y suelta un firmware válido llamado `arcadematrix_recovery.bin` en la raíz de la partición `bootfs` O en la partición `DATA`. ¡Se instalará automáticamente en el próximo arranque!)*
* 🚀 **Rendimiento Nativo en Rust Multiproceso** : motor multiproceso de alto rendimiento basado en Actix-web y procesamiento gráfico en Rust puro con 0% de uso de CPU en reposo.
* ⚡ **Pipeline Canvas de Copia Cero DirtyBounds (`canvas_single`)** : lienzo único fuera de pantalla con presentación por tramos de filas sin copia y seguimiento de líneas modificadas `DirtyBounds`. ¡Elimina por completo las caídas e interferencias de Wi-Fi SDIO en chips Broadcom!
* 🗂️ **Biblioteca GIF de Red y Gestor de Archivos Web (`/api/gifs/*`)** : ¡gestión completa de playlists GIF directamente desde la Web UI! Explora carpetas de playlists, sube GIFs animados por Wi-Fi/Ethernet, crea/elimina carpetas, renombra y reindexa automáticamente en segundo plano. Soporte nativo de doble orientación (`?orientation=yoko|tate`) para visualizaciones horizontales (Yoko) y verticales (Tate) — realizado por [@TooncesToo](https://github.com/TooncesToo).
* 🔤 **Fuentes cargables dinámicamente (`.ttf`)** : deja cualquier fuente `.ttf` u `.otf` directamente en la carpeta `fonts/`, y la interfaz Web la listará automáticamente para usarla en el reloj o la fecha.
* 🌧️ **Lluvia digital Matrix real (Katakana)** : un efecto Matrix totalmente personalizado, ultra fluido y auténtico (`DotGothic16`) con Katakana de media anchura cayendo y texto en espacio negativo de «LED apagados» atravesando la lluvia.

---

## 🎨 Colección Signature de Relojes Retro Temáticos

ArcadeMatrix RPi incluye una colección exclusiva de relojes retro de recreativas y consolas, sincronizados con el hardware y renderizados con un 100 % de sprites originales fieles al píxel a 60 FPS constantes, con soporte multirresolución para modos Paisaje (64x64, 128x32, 256x64) y Vertical / Tate (32x64, 64x128, 64x256):

### 1. Metal Slug: Super Vehicle-001 (SNK Neo Geo) — Tema 41
*Pixel art auténtico de SNK Neo Geo con escenario del zoco del desierto árabe, animaciones de combate de Marco Rossi, tanque rebelde Di-Cokka, helicóptero de patrulla y tiroteos con ametralladora pesada.*
![Reloj Metal Slug](docs/assets/clocks/poster_metal_slug.png)

### 2. Castlevania (Konami NES) — Tema 31
*Campanario gótico con Simon Belmont subiendo la gran escalera de piedra hacia la cámara de Drácula, antorcha clásica sobre pedestal, murciélago vampiro batiendo las alas frente a la luna de sangre y dígitos marfil de alta legibilidad.*
![Reloj Castlevania](docs/assets/clocks/poster_castlevania.png)

### 3. Super Mario Bros (NES) — Tema 30
*Overworld del Reino Champiñón con bloques de ladrillo SMB1 100 % auténticos; Mario corre por la pantalla y salta bajo el bloque de dígitos provocando un rebote elástico y cambio de hora (con patada al caparazón verde de Koopa y moneda dorada en modo compacto 128x32, y escalada de 3 niveles en modo vertical).*
![Reloj Super Mario Bros](docs/assets/clocks/poster_super_mario.png)

### 4. Mega Man (Capcom NES) — Tema 35
*Fortaleza Castillo de Wily de Capcom con plataformas técnicas separadas, barra de energía, Metool durmiendo y Mega Man disparando su Buster a la cápsula de los minutos.*
![Reloj Mega Man](docs/assets/clocks/poster_megaman.png)

### 5. Sonic The Hedgehog (Sega Genesis) — Tema 39
*Plataformas ajedrezadas de Green Hill Zone, anillos dorados giratorios, resorte rojo, badnik Motobug y animaciones de Sonic quieto y en spin-dash saltando.*
![Reloj Sonic The Hedgehog](docs/assets/clocks/poster_sonic.png)

### 6. Pokémon Pokédex (Nintendo Game Boy) — Tema 32
*Auténtica interfaz Pokédex de doble pantalla con visor de inspección de criaturas, sprite animado de Pikachu, reloj digital con tipografía PKMN, barra de telemetría de segundos y sensor LED parpadeante.*
![Reloj Pokédex](docs/assets/clocks/poster_pokedex.png)

### 7. Pac-Man Arcade (Namco 1980) — Tema 26
*Laberinto arcade original de Namco con pasillos de neón azul, energizadores, puntos, Pac-Man animado y los 4 fantasmas persiguiéndolo (Blinky, Pinky, Inky, Clyde).*
![Reloj Pac-Man](docs/assets/clocks/poster_pacman.png)

### 8. Tetris Ruso (Alexey Pajitnov / Game Boy) — Tema 23
*Legendario puzle de bloques que caen con tetrominós 3D biselados dinámicos (I, J, L, O, S, T, Z) cayendo en cascada para ensamblar las horas y minutos en tiempo real.*
![Reloj Tetris](docs/assets/clocks/poster_tetris.png)

### 9. Reloj Mundial y Terminador Solar — Tema 33
*Mapa mundial continental de alta resolución con terminador solar día/noche dinámico que calcula la declinación solar en tiempo real, marcador de meridiano local y reloj dual UTC/local.*
![Reloj Mundial](docs/assets/clocks/poster_worldmap.png)

### 10. Lluvia Digital Matrix Real (Las Wachowski 1999) — Tema 21
*Icónica lluvia digital con cascadas de glifos verde fósforo, velocidades de caída aleatorias, cabezas guía blancas brillantes, estelas de persistencia y dígitos de reloj de neón brillante.*
![Reloj Matrix Rain](docs/assets/clocks/poster_matrix_rain.png)

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

## 🛠️ Instalación Avanzada

### Opción 2: instalación manual
Si prefieres instalarlo manualmente sobre una **Raspberry Pi OS Lite (64-bit)** recién instalada:
Una vez conectado a tu Raspberry Pi por SSH:

```bash
curl -sSL https://raw.githubusercontent.com/red77290/ArcadeMatrix_RPI/main/autoInstall.sh | bash
```
*(Si el repositorio es privado, primero tendrás que hacer `git clone` manualmente y luego ejecutar `./autoInstall.sh` dentro de la carpeta.)*

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
.\scripts\deploy.ps1 -Ip "192.168.1.177" -User "pi" -Pass "raspberry"
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

**Soluciones y Selección del Pipeline de Renderizado:**

ArcadeMatrix RPi incorpora ahora la abstracción canónica `IDrawingSurface` con un selector configurable de **Pipeline de Renderizado** en la interfaz Web (`#sys-tab-hardware`):

1. **`canvas_single` (Recomendado / Por defecto en modo `auto`)**: Lienzo único fuera de pantalla con presentación por tramos de filas sin copia y seguimiento de zonas modificadas `DirtyBounds`. En fotogramas estáticos de relojes y animaciones tranquilas, `present()` detecta de inmediato el estado limpio y nunca toca innecesariamente el bus de memoria Broadcom ni el motor DMA. ¡Esto **elimina por completo las caídas de Wi-Fi SDIO y la inanición del bus** en Raspberry Pi!
2. **`canvas_double`**: Doble lienzo fuera de pantalla con intercambio de buffers para máxima fluidez sin desgarros cuando la contención Wi-Fi no es un problema (p. ej. Ethernet / Wi-Fi USB).
3. **`direct_double`**: Doble buffer de hardware directo mediante el FFI C de `rpi-rgb-led-matrix`.
4. **`direct_single`**: Framebuffer único directo de hardware.
5. **Usar un cable Ethernet (RJ45)** (Pi 3B/4): Evita el chip Wi-Fi SDIO por completo.
6. **Usar un adaptador Wi-Fi USB**: Los controladores USB usan un bus interno diferente y son inmunes a la saturación DMA del PWM.
7. **Establecer `disable_hardware_pulsing = true`**: Obliga a la matriz a usar renderizado por software (CPU bit-banging) en lugar de DMA por hardware. Tu Wi-Fi funcionará perfectamente, pero verás un ligero efecto de "líneas VHS" (parpadeo) en la matriz.

*(Nota: Las matrices **128x32** requieren 4 veces menos ancho de banda DMA, por lo que suelen funcionar perfectamente con el pulso de hardware habilitado y el Wi-Fi interno activo).*

*(Nota sobre la **Raspberry Pi 4**: La Pi 4 usa una arquitectura PCIe y un controlador DMA mucho más rápidos. Es muy probable que NO se vea afectada por este error de saturación Wi-Fi en 256x64. Sin embargo, esto aún no se ha probado oficialmente ya que no tenemos una Pi 4 a mano para confirmarlo).*

### Tabla de Compatibilidad Raspberry Pi & Resolución

| Resolución Matriz | Pipeline de Renderizado | Configuración de Red / Hardware Pulsing | Calidad de Imagen | Wi-Fi / APIs / MQTT |
|-------------|-------------------------|-----------------------------|----------------|-------------------|
| **Cualquiera** | **`canvas_single`** (Por defecto) | Wi-Fi Interno + DMA Pulsing | ✅ Perfecta (0 parpadeo) | ✅ **100% Estable (Cero Caídas)** |
| **128x32**  | Cualquier pipeline      | Wi-Fi Interno + `disable_hardware_pulsing = false` | ✅ Perfecta | ✅ Estable |
| **256x64**  | `canvas_double` / `direct` | **Cable Ethernet (RJ45)** + DMA Pulsing | ✅ Perfecta | ✅ Estable |
| **256x64**  | `canvas_double` / `direct` | **Adaptador Wi-Fi USB** + DMA Pulsing | ✅ Perfecta | ✅ Estable |
| **256x64**  | `direct_single`         | Wi-Fi Interno + `disable_hardware_pulsing = true` | ⚠️ Ligero parpadeo | ✅ Estable |
| **256x64**  | `direct_double`         | Wi-Fi Interno + `disable_hardware_pulsing = false` | ✅ Perfecta | ❌ Roto / Timeouts |

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

ArcadeMatrix es compatible con marquees dinámicos **estilo Pixelcade** cuando seleccionas o juegas un juego en tu consola Recalbox, Batocera o RetroPie.

> [!IMPORTANT]
> **Requisito de versión de Batocera:** La sincronización dinámica de marquesinas durante la navegación por los juegos y sistemas requiere **Batocera v33 o superior** (Batocera introdujo los hooks de script de EmulationStation `game-selected` y `system-selected` en la versión 33). En Batocera v32 y versiones anteriores, solo se admiten los eventos de inicio y parada del juego (`game-start`, `game-end`). Recalbox es totalmente compatible en todas sus versiones a través de su daemon de monitorización de estado nativo.

Mientras navegas por tus listas de juegos, la Raspberry Pi descargará las marquees oficiales de Pixelcade desde GitHub **en segundo plano y en tiempo real**, las almacenará en caché en tu tarjeta SD y las mostrará en tu matriz LED. Si un juego no tiene imagen, mostrará un elegante texto animado de respaldo.

### Instalación automática (recomendada)
Ve a la pestaña **MQTT** en la interfaz Web de ArcadeMatrix, selecciona el SO de tu consola (`Detección auto`, `Recalbox`, `Batocera` o `RetroPie`), introduce la IP de la consola junto con sus credenciales y haz clic en **Install Sync Script**. Esto inyectará automáticamente el daemon a través de SSH.

### Instalación manual (Recalbox / Batocera / RetroPie)
Si prefieres la instalación manual, o si falla la instalación por red (consulta [tools/README_ES.md](tools/README_ES.md)):
1. Abre o copia `tools/rpi_emulationstation_base_os_setup.sh` (o `tools/recalbox_setup_mqtt.sh`).
2. Copia el archivo a tu consola (por ejemplo, mediante `scp`).
3. Conéctate por SSH a tu consola y ejecuta: `sh /tmp/rpi_emulationstation_base_os_setup.sh`.
4. Introduce la IP de tu ArcadeMatrix y selecciona tu sistema objetivo en el menú interactivo.

### ¿Cómo funciona la arquitectura del daemon?
A diferencia de los scripts nativos de Recalbox que se ejecutan (y congelan el sistema) con cada movimiento del joystick, ArcadeMatrix instala **un daemon Python/Shell ultraligero (`arcadematrix_daemon.py`) en segundo plano**.
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

## ⚙️ Configuración avanzada (config.json)

Si prefieres editar los ajustes manualmente en lugar de usar la interfaz Web, puedes editar directamente el archivo `config.json` ubicado en la partición **DATA** de tu tarjeta SD. Esto es especialmente útil para configurar el Wi-Fi antes del primer arranque.

> ℹ️ ArcadeMatrix usa un único **`config.json`** estructurado (el antiguo formato `conf.ini` se ha eliminado). El archivo es **autorreparable**: cualquier clave faltante se recrea con su valor por defecto en el arranque, así que una edición manual parcial siempre es segura.

```json
{
  "matrix": { "width": 64, "height": 32, "chain_length": 1, "mapping": "adafruit-hat",
              "rgb_sequence": "RGB", "slowdown": 2, "disable_hardware_pulsing": false },
  "wifi":   { "ssid": "YourNetwork", "password": "YourPassword", "configured": false,
              "disable_internal": false },
  "mqtt":   { "enabled": false, "broker": "192.168.1.50", "port": 1883 },
  "system": { "format_24h": true, "lang": "en", "night_mode_enabled": false,
              "turn_off_at": "23:00", "wake_up_at": "07:00", "night_brightness": 20 },
  "instances": [
    { "instance_id": "clock_main", "engine_id": "clock",
      "config": { "theme": "0", "font": "DotGothic16.ttf", "size": "16" } }
  ],
  "rotation": [ { "instance_id": "clock_main", "duration_sec": 30 } ],
  "api_auth_enabled": false,
  "api_token": ""
}
```

Puntos clave:
* **`matrix`** — controlador de hardware (tamaño del panel, `mapping`, `rgb_sequence`, `slowdown`, `disable_hardware_pulsing`, ...). Editarlo activa un reinicio automático.
* **`wifi`** — establece `configured: false` para forzar un intento de (re)conexión en el próximo arranque (se vuelve a poner automáticamente en `true` al tener éxito). Usa `disable_internal: true` con un adaptador USB externo.
* **`mqtt`** — sincronización de marquees Recalbox/Batocera (ver más abajo).
* **`system`** — zona horaria, idioma, formato 24h y programación Night/Standby (`night_brightness: 0` apaga completamente el panel).
* **`instances` / `rotation`** — cada motor es una *instancia* autocontenida; `rotation` decide el orden de visualización y el `duration_sec` de cada slot. Editar una instancia desde la interfaz Web se aplica **en vivo, sin reinicio**.
* **`api_auth_enabled` / `api_token`** — cuando está activado, los endpoints sensibles requieren la cabecera `X-API-Token` (ver la nota de API más abajo).

👉 **Referencia completa campo por campo:** [docs/CONFIGURATION_ES.md](docs/CONFIGURATION_ES.md).

### 🔒 Autenticación de la API
`api_auth_enabled` es `false` por defecto para que la interfaz Web incluida funcione inmediatamente. Establécelo en `true` (y copia el `api_token` generado automáticamente) para requerir la cabecera `X-API-Token` en endpoints sensibles como `/api/wifi`, `/api/mqtt/install`, `/api/system/reboot` y `/api/system/shutdown`. Actívalo siempre que el dispositivo sea accesible más allá de una LAN de confianza.

## 🙏 Agradecimientos

Un enorme agradecimiento a la comunidad de código abierto y a los creadores de las increíbles bibliotecas que impulsan este proyecto:
- **[rpi-rgb-led-matrix](https://github.com/hzeller/rpi-rgb-led-matrix)** por hzeller (y los bindings de Rust por AidanWallace)
- **[Actix-web](https://github.com/actix/actix-web)** por la rapidísima API web
- **[image-rs](https://github.com/image-rs/image)** por el procesamiento de imágenes
- **[rumqttc](https://github.com/bytebeamio/rumqtt)** por el soporte MQTT
- **[@TooncesToo](https://github.com/TooncesToo)** por desarrollar la API de biblioteca GIF de red, subida de archivos múltiples y gestor de archivos Web UI con soporte de doble orientación tanto en Raspberry Pi como en ESP32.
- ¡Y a toda la comunidad de Rust por crear un ecosistema tan asombroso (Tokio, Serde, reqwest, tracing, etc.)!

¡Un agradecimiento especial al **RPiTeam** por el increíble pack de 600 GIFs!

## ☕ Apoya el proyecto

[![Ko-fi](https://img.shields.io/badge/Ko--fi-Invítame%20a%20un%20café-FF5E5B?style=for-the-badge&logo=ko-fi&logoColor=white)](https://ko-fi.com/red1l)

Si te gusta este proyecto y deseas brindar tu apoyo, puedes [invitarme a un café en Ko-fi](https://ko-fi.com/red1l). Es totalmente voluntario, sin compromiso ni nada a cambio — ¡un gesto amable que motiva a seguir desarrollando y ayuda a probar en nuevo hardware!

## 📜 Licencia
Este proyecto está licenciado bajo la **[Licencia MIT](LICENSE)**.

**En resumen:** eres libre de usar, modificar, distribuir y sublicenciar este software para cualquier propósito, personal o comercial, siempre que se conserve el aviso de derechos de autor y el aviso de permiso originales. Consulta el archivo [LICENSE](LICENSE) completo para más detalles.

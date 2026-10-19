🇬🇧 [English](CONFIGURATION.md) | 🇫🇷 [Français](CONFIGURATION_FR.md) | 🇪🇸 Español

# Configuración Detallada (config.json) - Raspberry Pi

El sistema de configuración se basa exclusivamente en un único archivo `config.json` ubicado en la raíz del proyecto (o en la partición **DATA** de la imagen precompilada). Gestiona el controlador de hardware, la red, la integración MQTT, el comportamiento del sistema, la seguridad de la API y la lógica desacoplada de cada motor ("instancias").

> El formato heredado `conf.ini` se ha eliminado por completo. `config.json` es ahora la **única fuente de verdad**. En el arranque, el archivo se valida y se autorrepara (ver §8), así que un archivo parcial o editado a mano es seguro: las claves que falten se vuelven a crear con sus valores por defecto.

---

## 1. Estructura Global

```json
{
  "matrix": { ... },
  "wifi": { ... },
  "mqtt": { ... },
  "data_mqtt": { ... },
  "system": { ... },
  "instances": [ ... ],
  "rotation": [ ... ],
  "api_auth_enabled": false,
  "api_token": ""
}
```

---

## 2. El Bloque `"matrix"` (Controlador Hardware)

Este bloque configura los parámetros DMA para la biblioteca hzeller `rpi-rgb-led-matrix`. Cambiar cualquier valor de hardware activa un reinicio automático para que la nueva configuración del controlador tenga efecto.

| Clave | Tipo | Descripción |
| :--- | :--- | :--- |
| `width` | `int` | Ancho de un solo panel (ej. `64`). |
| `height` | `int` | Alto de un solo panel (ej. `32`). |
| `chain_length` | `int` | Número de paneles encadenados horizontalmente. |
| `parallel` | `int` | Número de cadenas de paneles en paralelo (`1`–`3`, por defecto `1`). |
| `power_limit_percent` | `int` | Límite global de potencia / brillo de la matriz en porcentaje (`1`–`100`, por defecto `40`). |
| `force_single_buffer` | `bool` | Fuerza el modo de buffer único cuando es `true` (por defecto `false`). |
| `color_depth` | `int` | Profundidad de color lógica en bits (`24` por defecto). |
| `mapping` | `String` | Cableado/mapeo GPIO (`regular`, `adafruit-hat`, `adafruit-hat-pwm`, ...). |
| `driver_chip` | `String` | Chip controlador (`SHIFTREG`, `FM6126A`). |
| `rgb_sequence` | `String` | Orden de colores (`RGB`, `RBG`, `BGR`, ...). Corrige aquí colores intercambiados. |
| `clk_phase` | `bool` | Inversión de fase de reloj (`false` por defecto). |
| `latch_blanking` | `int` | Ciclos de latch blanking (`0`–`8`, por defecto `1`). |
| `matrix_power` | `bool` | Estado de encendido por software de la matriz LED (`true` por defecto). |
| `slowdown` | `int` | Ralentización GPIO (`1`–`4`). Auméntala en Pi 3/4 si ves artefactos. |
| `pwm_bits` | `int` | Profundidad de color. Valor por defecto `11`; bájalo a `8` para ahorrar CPU. |
| `pwm_lsb_nanoseconds` | `int` | Ajuste del ancho de pulso LSB (avanzado, por defecto `130`). |
| `disable_hardware_pulsing` | `bool` | Ponlo en `true` para evitar que DMA asfixie el Wi-Fi interno (ligero parpadeo). |
| `limit_refresh_rate_hz` | `int` | Limita la frecuencia de refresco (`0` = sin límite). |
| `row_address_mode` | `int` | Tipo de direccionamiento de filas para paneles exóticos (`0` por defecto). |
| `multiplexing` | `int` | Tipo de multiplexado del panel (`0` por defecto). |
| `panel_type` | `String` | Cadena opcional de inicialización del panel (ej. `FM6126A`), normalmente vacía. |
| `rotation` | `int` | Orientación física de la pantalla en grados (`0`, `90`, `180`, `270`, por defecto `0`). |
| `transition_effect` | `String` | Efecto de transición visual aplicado al cambiar la orientación (`none`, `vortex`, `fade`, `slide`, ...). Por defecto `none`. |
| `transition_duration_ms` | `int` | Duración de la transición de orientación en milisegundos (`100`–`3000`, por defecto `500`). |
| `render_pipeline` | `String` | Pipeline de renderizado de hardware y estrategia de buffer: `auto` (selección automática por defecto), `canvas_single` (**Recomendado**, lienzo único fuera de pantalla con seguimiento de zonas modificadas 'dirty bounds' que protege el bus SDIO Wi-Fi de la saturación), `canvas_double` (doble buffer con lienzo), `direct_double` (doble buffer directo sin copia vía controlador C), `direct_single` (buffer directo simple). Por defecto `canvas_single`. |
| `slot_transition` | `String` | Efecto reproducido cuando la rotación pasa a la siguiente pantalla (`wipe`, `curtain`, `shutter`, `dissolve`, `checker`, `matrix`, `slide`, `zoom`, `glitch`, `random`, `none`). Por defecto `none`. |
| `slot_transition_duration_ms` | `int` | Duración de la transición entre pantallas, en milisegundos (`100`-`3000`, por defecto `500`). |

> El brillo diurno en vivo **no** se almacena en este bloque; se controla en tiempo de ejecución desde la interfaz Web (deslizador del Dashboard → `POST /api/system { "brightness_limit": 0-100 }`). El brillo nocturno vive en el bloque `system` (§4).

---

## 3. El Bloque `"wifi"`

| Clave | Tipo | Descripción |
| :--- | :--- | :--- |
| `ssid` | `String` | El nombre de su red Wi-Fi. |
| `password` | `String` | La clave WPA2. |
| `hostname` | `String` | Nombre de host del dispositivo anunciado en la red. |
| `configured` | `bool` | Ponlo en `false` para forzar un intento de (re)conexión en el próximo arranque. Se vuelve a poner en `true` automáticamente al tener éxito. |
| `disable_internal` | `bool` | Si usa un adaptador USB externo, deshabilita el Wi-Fi interno de la Pi (cambiar esto activa un reinicio). |

También puedes enviar credenciales en tiempo de ejecución con `POST /api/wifi { "ssid": "...", "password": "..." }`, lo que establece `configured=false` y reinicia el aprovisionamiento de red.

---

## 4. El Bloque `"system"` (Entorno y Espera)

| Clave | Tipo | Descripción |
| :--- | :--- | :--- |
| `timezone` | `String` | Cadena POSIX (ej. `CET-1CEST,M3.5.0,M10.5.0/3`). |
| `format_24h` | `bool` | Formato de hora. `true` = 23:00, `false` = 11:00 PM. |
| `lang` | `String` | Idioma del sistema (ej. `en`, `fr`, `es`). |
| `temp_unit` | `String` | Unidad global de temperatura (`"C"` para Celsius o `"F"` para Fahrenheit, por defecto `"C"`). |
| `night_mode_enabled` | `bool` | Activa el apagado automático / la reducción de brillo por la noche. |
| `turn_off_at` | `String` | Hora de inicio de espera (ej. `"23:00"`). |
| `wake_up_at` | `String` | Hora de despertar (ej. `"07:00"`). |
| `night_brightness` | `int` | Brillo de espera (`0` = matriz completamente apagada). |
| `day_brightness` | `int` | Brillo diurno en vivo (`0`–`100`). Se ajusta con el control del panel y se conserva tras un reinicio. |
| `idle_fighter_enabled` | `bool` | Interruptor principal de la superposición decorativa de Luchador sobre las pantallas de rotación inactivas (activación por pantalla mediante cada entrada de rotación). |
| `idle_fighter_interval` | `int` | Segundos entre dos animaciones de combate (mínimo `1`, por defecto `10`). |
| `idle_fighter_speed` | `int` | Porcentaje de velocidad de animación para el overlay de Luchador (`25`–`200`, por defecto `100`). |

---

## 5. El Bloque `"mqtt"` (Marquees Recalbox / Batocera / RetroPie)

| Clave | Tipo | Descripción |
| :--- | :--- | :--- |
| `enabled` | `bool` | Activa el listener MQTT para marquees estilo Pixelcade. |
| `broker` | `String` | IP/host del broker (normalmente la propia Pi). |
| `port` | `int` | Puerto del broker (por defecto `1883`). |
| `user` | `String` | Usuario del broker (opcional). |
| `pass` | `String` | Contraseña del broker (opcional). |
| `device_name` | `String` | Identificador publicado por este dispositivo. |
| `allow_overlay` | `bool` | Permite overlays decorativos (ej. Street Fighter) sobre las pantallas MQTT/marquee (por defecto `false`). |
| *(auto-suscripción)* | `system/playing/#` | Se suscribe automáticamente a todos los sistemas retro compatibles: `system/playing/recalbox`, `system/playing/batocera`, `system/playing/retropie`. |

El daemon de sincronización puede instalarse en la consola (Recalbox, Batocera, RetroPie) por SSH desde la interfaz Web (`POST /api/mqtt/install`) con selección o autodetección del sistema operativo, y sus logs pueden obtenerse con `POST /api/mqtt/logs`.

> [!NOTE]
> Para Batocera, se requiere la versión **v33 o superior** para el cambio dinámico de marquesinas durante la navegación (hooks `game-selected` y `system-selected`). Batocera v32 y versiones anteriores solo activan eventos de inicio/parada de juego. Recalbox es compatible en todas sus versiones.

---

## 5b. El Bloque `"data_mqtt"` (broker MQTT Data)

Credenciales del broker para el motor `mqttdata`. Esta conexión es distinta del bloque `"mqtt"` de arriba (marquesinas / control, que puede tomar la pantalla). Nada corre en segundo plano: el motor solo se conecta mientras su pantalla está activa y se desconecta al salir de la rotación.

| Clave | Tipo | Descripción |
| :--- | :--- | :--- |
| `broker` | `String` | Host o IP del broker. |
| `port` | `int` | Puerto del broker (por defecto `1883`). |
| `user` | `String` | Usuario (opcional). |
| `pass` | `String` | Contraseña (opcional). |

El id de cliente MQTT es `<hostname>-data`. Los cambios se aplican en la siguiente activación del motor (sin reinicio).

## 6. Seguridad de la API (`api_auth_enabled` / `api_token`)

Estas dos claves de nivel superior protegen los endpoints de escritura/administración.

| Clave | Tipo | Descripción |
| :--- | :--- | :--- |
| `api_auth_enabled` | `bool` | Si es `true`, los endpoints sensibles requieren que la cabecera `X-API-Token` coincida con `api_token`. |
| `api_token` | `String` | Token secreto (generado automáticamente en el primer arranque). La interfaz Web lo envía como `X-API-Token`. |

Está desactivado por defecto para que la interfaz Web incluida funcione inmediatamente. Actívalo si el dispositivo es accesible más allá de una LAN de confianza.

---

### Opciones de una instancia de reloj

Además de `theme`, `font`, `size` y los colores, una instancia de reloj admite:

| Clave | Tipo | Descripción |
| :--- | :--- | :--- |
| `clock_glow` | `enum` | Contorno alrededor de los dígitos: `0` ninguno, `1` neón (el color elegido a plena intensidad con un centro casi blanco, como lo dibuja la cara Matrix), `2` un color propio. Por defecto `0`. |
| `clock_glow_color` | `color` | Color del contorno cuando `clock_glow` es `2` (por defecto `#00FF41`). |

Los temas `30` a `34` y `37` son caras adaptadas de los clockfaces de Clockwise. Las marcadas
`(256x64)` están diseñadas para un panel ancho y muestran un aviso en los pequeños.

---

## 7. Motores: `"instances"` & `"rotation"`

La arquitectura desacoplada permite crear múltiples copias independientes y configuradas de forma distinta del mismo Motor.

### `"instances"`
Un array que contiene la configuración de cada bloque lógico.

```json
{
  "instance_id": "crypto_main",
  "engine_id": "crypto",
  "config": {
    "symbols": "BTC,ETH,SOL"
  }
}
```
* `instance_id`: Nombre único de este bloque.
* `engine_id`: El identificador interno del Motor Rust (debe ser un motor registrado — ver §9).
* `config`: Un mapa plano de valores `String` específicos del motor, validado contra su `ConfigSchema`.

Editar una instancia a través de la interfaz Web (`POST /api/instances`) se aplica **en vivo, sin reinicio**: el runtime llama al `on_config_changed()` del motor en el siguiente frame (hot-reload Lazy-Once). Añadir o eliminar una instancia reinicia limpiamente la rotación.

### `"rotation"`
Define el orden de visualización, la duración por slot y la activación de overlays transversales.

```json
{
  "instance_id": "crypto_main",
  "duration_sec": 30,
  "overlays": {
    "fighter": true
  }
}
```
* `instance_id`: Nombre de la instancia objetivo.
* `duration_sec`: Duración en pantalla en segundos (o cuota de reproducción para motores autónomos como GIF).
* `overlays.fighter`: (`bool`) Interruptor granular para el overlay decorativo de lucha M.U.G.E.N en esta pantalla específica.

Solo se inicializan las instancias enumeradas aquí, ahorrando memoria para funciones no utilizadas. La rotación se puede editar desde el panel **Rotation** de la interfaz Web (`GET`/`POST /api/rotation`).

> **Persistencia Legible**: Al guardarse en disco (`config.json`), el archivo siempre se escribe formateado con sangrías claras para permitir su inspección y modificación manual segura.

---

## 8. Validación Autorreparable

En cada arranque **y** en cada escritura vía `POST /api/instances`, el `ConfigSanitizer` reconcilia cada instancia con el `ConfigSchema` de su motor:

* **Clave faltante** → se inyecta el `default_value` del schema.
* **Integer / Float** → se parsea y, si está fuera de `min`/`max`, se limita o se restablece al valor por defecto (según el `validation_policy` del campo).
* **Boolean** → se normaliza (`true/1/yes/on` → `true`, `false/0/no/off` → `false`); un valor no parseable vuelve al valor por defecto.
* **Options** → el valor debe ser uno de los options declarados (lista separada por comas para selección múltiple); de lo contrario vuelve al valor por defecto.
* **Claves obsoletas** → las claves que ya no están presentes en el schema (por ejemplo después de una OTA que renombró un campo) se eliminan.

El resultado se guarda de forma atómica, por lo que una OTA que añade un nuevo campo lo autorrellena sin ninguna intervención del usuario.

---

## 9. Configuraciones de Motores

Cada motor anuncia sus propios campos mediante su `ConfigSchema` (descubrible en `GET /api/engines`, que es lo que alimenta la interfaz Web dinámica). Los motores más comunes:

### Motor: `clock`
| Campo | Tipo | Por defecto | Descripción |
| :--- | :--- | :--- | :--- |
| `theme` | `Options` | `0` | Índice del tema de reloj animado: `0` = Digital Estándar, `1` = Flip Clock, `2` = Cyberpunk, `3` = Word Clock, `4` = Binary Clock, `5` = Pac-Man, `6` = Tetris, `7` = Slot Machine, `8` = Versus (M.U.G.E.N), `9` = Pong, `10` = Matrix Rain (Katakana), `30` = Mario (patada al caparazón Koopa y escalada vertical Tate), `31` = Castlevania (Simon Belmont y latigazo), `32` = Pocket Index (Pokémon), `33` = World Clock, `34` = Ms. Pac-Man, `35` = Mega Man (disparo Buster cargado y Metool), `37` = Words Clock, `39` = Sonic (Green Hill y Badnik Motobug), `41` = Metal Slug (Marco Rossi, tanque Di-Cokka y granadas). |
| `format` | `String` | `%H:%M:%S` | Formato de hora strftime. |
| `font` | `String` | `PressStart2P.ttf` | Archivo de fuente de `/fonts/`. |
| `timezone` | `Options` | `Europe/Paris` | Zona horaria o región. |
| `size` | `int` | `2` | Factor de escalado de la fuente. |
| `clock_speed` | `int` | `100` | Velocidad de animación en porcentaje (caída de Tetris, barrido de Pac-Man, carrera de Mario); menor es más lento. |
| `clock_glow` | `Options` | `0` | Halo alrededor de los dígitos (`0` desactivado, `1` neón, `2` color de contorno propio). |
| `clock_glow_color` | `String` | `#00FF41` | Color del contorno cuando `clock_glow` es `2`. |
| `color_1` | `String` | `#FFFFFF` | Color hexadecimal primario (inicio de degradado en tema Custom). |
| `color_2` | `String` | `#FFFFFF` | Color hexadecimal secundario (fin de degradado en tema Custom). |
| `offset_x` | `int` | `0` | Desplazamiento horizontal en píxeles. |
| `offset_y` | `int` | `0` | Desplazamiento vertical en píxeles. |

### Motor: `date`
| Campo | Tipo | Por defecto | Descripción |
| :--- | :--- | :--- | :--- |
| `theme` | `Options` | `0` | Índice del tema de fecha. |
| `format` | `String` | `%d/%m` | Formato de fecha strftime. |
| `font` | `String` | `PressStart2P.ttf` | Archivo de fuente de `/fonts/`. |
| `timezone` | `Options` | `Europe/Paris` | Zona horaria o región. |
| `size` | `int` | `2` | Factor de escalado de la fuente. |
| `color_1` | `String` | `#FFFFFF` | Color hexadecimal primario. |
| `color_2` | `String` | `#FFFFFF` | Color hexadecimal secundario. |
| `offset_x` | `int` | `0` | Desplazamiento horizontal en píxeles. |
| `offset_y` | `int` | `0` | Desplazamiento vertical en píxeles. |

### Motor: `dashboard` (Smart Dashboard Hub)
| Campo | Tipo | Por defecto | Opciones | Descripción |
| :--- | :--- | :--- | :--- | :--- |
| `theme` | `Options` | `0` | `0` (Cyberpunk Neon), `1` (Amber HUD), `2` (Minimalist Luxury), `3` (Matrix Phosphor) | Paleta de colores para widgets y bordes del dashboard. |
| `clock_mode` | `Options` | `1` | `0` (Digital), `1` (Esfera), `2` (Mínimo) | Estilo de visualización del reloj. |
| `timezone` | `Options` | `system` | `system` o zona IANA | Zona horaria o ajuste del sistema del dispositivo. |
| `format_24h` | `Options` | `system` | `system`, `true`, `false` | Formato de hora de 24h o 12h. |
| `temp_unit` | `Options` | `system` | `system`, `C`, `F` | Celsius (°C) o Fahrenheit (°F). |
| `lang` | `Options` | `system` | `system`, `en`, `fr`, `es` | Idioma para textos y fechas del dashboard. |
| `show_clock` | `bool` | `true` | `true`, `false` | Muestra el reloj principal. |
| `show_world_clock` | `bool` | `true` | `true`, `false` | Muestra zonas horarias mundiales secundarias (`NYC`, `TYO`, `LON`...). |
| `world_clocks` | `String` | `NYC,TYO,LON` | Códigos de ciudad | Zonas horarias separadas por comas (ej. `NYC,TYO,LON,PAR`). |
| `show_weather` | `bool` | `true` | `true`, `false` | Muestra el pronóstico del clima y temperatura exterior. |
| `weather_city` | `String` | `Paris, FR` | Ciudad | Ciudad para el pronóstico meteorológico (hereda automáticamente `city` de la instancia `weather` si se deja por defecto). |
| `weather_api_key` | `String` | `""` | Clave API opcional | Clave API opcional de OpenWeatherMap (hereda `api_key` de la instancia `weather` si está vacía, y recurre automáticamente al servicio gratuito Open-Meteo sin clave). |
| `refresh_interval` | `Options` | `10` | `1`, `5`, `10`, `15`, `30`, `60` min | Frecuencia de actualización en segundo plano para el clima y mercados. |
| `show_markets` | `bool` | `true` | `true`, `false` | Muestra el teletipo bursátil y cripto en tiempo real. |
| `tracked_markets` | `String` | `BTC,ETH,SOL,NVDA` | Símbolos | Símbolos bursátiles y cripto separados por comas. |
| `show_sysinfo` | `bool` | `true` | `true`, `false` | Muestra métricas de CPU, RAM y Wi-Fi. |
| `show_date` | `bool` | `true` | `true`, `false` | Muestra la insignia de fecha. |
| `show_seconds` | `bool` | `true` | `true`, `false` | Muestra los segundos del reloj. |
| `smooth_seconds` | `bool` | `true` | `true`, `false` | Segundero de barrido continuo frente a salto por segundo. |
| `offset_x` | `int` | `0` | `-64` a `64` | Desplazamiento horizontal en píxeles. |
| `offset_y` | `int` | `0` | `-32` a `32` | Desplazamiento vertical en píxeles. |

### Motor: `crypto`
| Campo | Tipo | Por defecto | Opciones | Descripción |
| :--- | :--- | :--- | :--- | :--- |
| `symbols` | `String` | `BTC,ETH` | Separados por comas | Símbolos de criptomonedas a monitorear (CoinGecko / Binance). |
| `show_chart` | `bool` | `true` | `true`, `false` | Mostrar el gráfico sparkline histórico. |
| `chart_timeframe` | `Options` | `daily` | `hourly`, `daily`, `weekly`, `monthly` | Intervalo temporal para el historial de precios. |
| `page_seconds` | `int` | `5` | `3` a `30` | Segundos de permanencia en cada página antes de alternar. Como `crypto` es autónomo (`self_paced`), este tiempo por página controla la duración completa de la ranura de rotación para mostrar todas las criptomonedas configuradas antes de avanzar. |
| `currency` | `Options` | `USD` | `USD`, `EUR`, `GBP`, `JPY` | Moneda de comparación y símbolo de visualización (`$`, `€`, `£`, `¥`). |
| `cache_ttl_min` | `int` | `1` | `1` a `60` | Minutos de caché para la cotización. |

### Motor: `stock`
| Campo | Tipo | Por defecto | Opciones | Descripción |
| :--- | :--- | :--- | :--- | :--- |
| `symbols` | `String` | `AAPL,NVDA,TSLA` | Separados por comas | Tickers bursátiles a monitorear (Yahoo Finance). |
| `show_chart` | `bool` | `true` | `true`, `false` | Mostrar el gráfico sparkline histórico. |
| `chart_timeframe` | `Options` | `daily` | `hourly`, `daily`, `weekly`, `monthly` | Intervalo temporal para el historial de precios. |
| `page_seconds` | `int` | `5` | `3` a `30` | Segundos de permanencia en cada página antes de alternar. Como `stock` es autónomo (`self_paced`), este tiempo por página controla la duración completa de la ranura de rotación para mostrar todas las acciones configuradas antes de avanzar. |
| `currency` | `Options` | `USD` | `USD`, `EUR`, `GBP`, `JPY` | Moneda de comparación y símbolo de visualización (`$`, `€`, `£`, `¥`). |
| `cache_ttl_min` | `int` | `1` | `1` a `60` | Minutos de caché para la cotización. |

### Motor: `gnews` (Noticias en Vivo y Ticker GNews)
| Campo | Tipo | Predeterminado | Opciones | Descripción |
| :--- | :--- | :--- | :--- | :--- |
| `api_key` | `String` | `""` | Clave válida | Clave API de GNews.io (opcional; usa noticias de demostración si está vacía). |
| `category` | `Options` | `technology` | `general`, `world`, `nation`, `business`, `technology`, `entertainment`, `sports`, `science`, `health` | Categoría temática principal. |
| `keywords` | `String` | `""` | Texto / Consulta | Palabras clave de búsqueda o etiquetas personalizadas (ej. `ai OR arcade`). |
| `lang` | `Options` | `auto` | `auto`, `en`, `fr`, `es`, `de`, `it`, `pt`, `nl`, `ru`, `zh`, `ja` | Idioma de las noticias (`auto` sincroniza con el sistema). |
| `country` | `Options` | `auto` | `auto`, `us`, `fr`, `gb`, `es`, `de`, `ca`, `it`, `jp`, `au`, `br`, `in` | Edición regional por país. |
| `max_articles` | `int` | `5` | `3` a `15` | Cantidad máxima de artículos almacenados en caché y seleccionados en orden aleatorio sin repetición entre ranuras de rotación. |
| `articles_per_rotation` | `int` | `2` | `1` a `15` | Rotación con ritmo propio: cantidad de artículos aleatorios a desplazar por completo (`PauseStart` -> `Scrolling` -> `PauseEnd`) antes de avanzar la rotación. |
| `requests_per_day` | `int` | `10` | `1` a `100` | Total de solicitudes API por 24 horas (plan gratuito: máx. 100). |
| `force_refresh` | `bool` | `false` | `true`, `false` | Purga la caché de noticias y consulta inmediatamente la API. |
| `cache_ttl_min` | `int` | `30` | `5` a `120` | Intervalo de refresco de la caché en red (minutos). |
| `display_mode` | `Options` | `smooth_scroll` | `smooth_scroll`, `serpentine`, `vertical_crawl`, `static_paged` | Modo de animación (desplazamiento continuo derecha a izquierda, flujo serpentina arcade, desplazamiento vertical o paginación fija). |
| `scroll_speed` | `int` | `3` | `1` a `10` | Velocidad de desplazamiento del teletipo (1: Lento a 10: Rápido). |
| `pause_start_sec` / `scroll_pause_start_ms` | `int` | `2` / `1200` | `0` a `10` s (`0` a `4000` ms) | Tiempo de pausa inicial antes de comenzar el desplazamiento del artículo. |
| `pause_end_sec` / `scroll_pause_end_ms` | `int` | `2` / `1000` | `0` a `10` s (`0` a `4000` ms) | Tiempo de pausa final al terminar el artículo antes de cambiar. |
| `article_duration_sec` | `int` | `12` | `5` a `60` | Duración de visualización de respaldo por artículo en segundos. |
| `theme` | `Options` | `category_dynamic` | `category_dynamic`, `breaking_crimson`, `cyberpunk`, `monochrome_paper` | Esquema de colores visual. |
| `show_title` | `bool` | `false` | `true`, `false` | Incluye el titular del artículo en el texto desplazado. |
| `show_description` | `bool` | `false` | `true`, `false` | Incluye el resumen / entradilla del artículo en el texto desplazado. |
| `show_content` | `bool` | `true` | `true`, `false` | Incluye el cuerpo del artículo en el texto desplazado (con respaldo automático a descripción/título si está vacío). |
| `show_category_badge` | `bool` | `true` | `true`, `false` | Muestra la píldora de categoría en color (`[TECH]`, `[WORLD]`, etc.). |
| `show_source` | `bool` | `true` | `true`, `false` | Muestra el nombre de la fuente de noticias (`BBC News`, `Reuters`, etc.). |
| `show_time_ago` | `bool` | `true` | `true`, `false` | Muestra la antigüedad relativa (`5m ago`, `2h ago`). |
| `show_beacon` | `bool` | `true` | `true`, `false` | Muestra la baliza luminosa de directo parpadeante. |
| `show_progress_dots` | `bool` | `true` | `true`, `false` | Muestra los puntos de progreso (`● ○ ○ ○ ○`). |

### Motor: `weather`
| Campo | Tipo | Predeterminado | Opciones | Descripción |
| :--- | :--- | :--- | :--- | :--- |
| `api_key` | `String` | `""` | Clave API gratuita | Clave API de OpenWeatherMap (gratis en [openweathermap.org](https://home.openweathermap.org/users/sign_up)). |
| `city` | `String` | `""` | Texto | Ubicación de la ciudad para el pronóstico (ver guía abajo). |
| `units` | `Options` | `metric` | `metric`, `imperial` | Unidad de temperatura: `metric` para Celsius (°C) o `imperial` para Fahrenheit (°F). |
| `offset_x` | `int` | `0` | `-64` a `64` | Desplazamiento horizontal de píxeles. |
| `offset_y` | `int` | `0` | `-32` a `32` | Desplazamiento vertical de píxeles. |

En paneles de 128x32 y 256x64 las páginas del tiempo usan el mismo diseño que el firmware ESP32: icono, luego día y condición (formas largas si caben) y luego temperaturas alineadas a la derecha. En °F la máxima se muestra encima de la mínima (convención estadounidense). El idioma de los días sigue `system.lang` (`en`, `fr`, `es`).

#### Cómo formatear el campo `city` en OpenWeatherMap
OpenWeatherMap utiliza el código de país ISO 3166 (y el código de estado de 2 letras para EE. UU.):
* **Ubicaciones Internacionales:** Use `Ciudad,CodigoPais` (ej. `Paris,FR`, `London,GB`, `Tokyo,JP`, `Montreal,CA`).
* **Ubicaciones en Estados Unidos:** Use `Ciudad,CodigoEstado,CodigoPais` (ej. `Tucson,AZ,US`, `Miami,FL,US`, `Dallas,TX,US`).
* **Dónde buscar el nombre exacto:** Vaya a [openweathermap.org](https://openweathermap.org) y busque su ciudad.

### Motor: `system_info` (Monitor de Sistema)
| Campo | Tipo | Por defecto | Opciones | Descripción |
| :--- | :--- | :--- | :--- | :--- |
| `theme` | `Options` | `0` | `0` a `2` | Diseño visual: `0` = Barras y Medidores HUD, `1` = Cyberpunk Neon, `2` = Cuadrícula compacta (2x2). |
| `show_cpu` | `bool` | `true` | `true`, `false` | Muestra el porcentaje y medidor de carga de CPU (CPU %). |
| `show_ram` | `bool` | `true` | `true`, `false` | Muestra el porcentaje y medidor de memoria RAM (RAM %). |
| `show_temp` | `bool` | `true` | `true`, `false` | Muestra la temperatura de hardware del SoC (escala dinámica verde/ámbar/rojo). |
| `show_uptime` | `bool` | `true` | `true`, `false` | Muestra el contador de tiempo de actividad del sistema (Uptime). |
| `temp_unit` | `Options` | `C` | `C`, `F` | Unidad de temperatura: Celsius (`C`) o Fahrenheit (`F`). |
| `offset_x` | `int` | `0` | `-64` a `64` | Desplazamiento horizontal en píxeles. |
| `offset_y` | `int` | `0` | `-32` a `32` | Desplazamiento vertical en píxeles. |

> [!NOTE]
> **Overlay de Luchadores M.U.G.E.N (`fighter`):** A diferencia de los motores de rotación independientes, `fighter` es un renderizador de superposición transversal sobre pantallas inactivas compatibles (como `clock` y `date`). Se configura globalmente en el bloque `"system"` (`idle_fighter_enabled`, `idle_fighter_interval`, `idle_fighter_speed` — ver §4) y se activa por ranura de rotación mediante `rotation[].overlays.fighter` (ver §7).

### Motor: `google_cast` (Google Home / Nest Audio)
| Campo | Tipo | Por defecto | Descripción |
| :--- | :--- | :--- | :--- |
| `device_ip` | `String` | `""` | IP estática de su altavoz Google Home / Nest Audio. Deje en blanco para descubrimiento mDNS automático en la red local. |
| `device_name` | `String` | `""` | Filtro por nombre del dispositivo (ej. `Salón`) durante el escaneo automático en la LAN. |
| `show_album_art` | `bool` | `true` | Descarga y muestra la carátula del álbum en el lado izquierdo de la matriz. |
| `show_progress` | `bool` | `true` | Muestra la barra de progreso de reproducción en la parte inferior. |
| `show_visualizer` | `bool` | `true` | Muestra un ecualizador de frecuencias de audio animado durante la reproducción. |
| `show_volume` | `bool` | `true` | Muestra el porcentaje de volumen actual del altavoz Google Nest. |

### Motor: `spotify` (Reproductor Oficial Spotify)
| Campo | Tipo | Por defecto | Descripción |
| :--- | :--- | :--- | :--- |
| `client_id` | `String` | `""` | Su Client ID de Spotify Developer API. |
| `client_secret` | `String` | `""` | Su Client Secret de Spotify Developer API (opcional para PKCE). |
| `refresh_token` | `String` | `""` | Su Refresh Token OAuth2 de Spotify para sincronización continua de reproducción. |
| `show_album_art` | `bool` | `true` | Descarga y muestra la carátula de álbum a todo color de Spotify. |
| `show_progress` | `bool` | `true` | Muestra la barra de progreso temporal de la pista en la parte inferior. |
| `show_visualizer` | `bool` | `true` | Muestra un ecualizador de audio animado durante la reproducción. |
| `show_volume` | `bool` | `true` | Muestra el porcentaje de volumen de reproducción de Spotify activo. |

### Motor: `gifs`
| Campo | Tipo | Predeterminado | Opciones | Descripción |
| :--- | :--- | :--- | :--- | :--- |
| `playlists` | `String` (Multi) | `""` | Opciones desde `/api/playlists` | Carpetas / Listas de reproducción de GIFs activas para rotar (separadas por comas). |

### Motor: `message`
| Campo | Tipo | Predeterminado | Opciones | Descripción |
| :--- | :--- | :--- | :--- | :--- |
| `text` | `String` | `ArcadeMatrix` | Texto | Texto del mensaje o cartel a mostrar. |
| `color` | `String` | `#ffffff` | Color Hex | Color del texto en formato `#RRGGBB`. |
| `size` | `int` | `1` | `1` a `4` | Multiplicador de escala de la fuente. |
| `direction` | `Options` | `rtl` | `rtl`, `ltr`, `static` | Dirección de desplazamiento o visualización estática. |
| `speed` | `int` | `50` | `10` a `200` | Milisegundos por actualización de píxel (menor = más rápido; ignorado en estático). |

### Motor: `marquee`
| Campo | Tipo | Predeterminado | Opciones | Descripción |
| :--- | :--- | :--- | :--- | :--- |
| `file_path` | `FileAsset` | `data/marquees/marquee.gif` | `.gif,.png,.jpg,.jpeg` | Ruta al archivo GIF o imagen de marquesina (también recibe marquesinas Pixelcade / Recalbox / Batocera en directo mediante MQTT o HTTP). |
| `speed_multiplier` | `Float` | `1.0` | `0.25` a `3.0` | Factor de velocidad de reproducción de la animación. |
| `fit_mode` | `Options` | `fit` | `fit`, `center`, `stretch` | Modo de ajuste de escala en pantalla. |

### Motor: `mqttdata` (MQTT Data)
| Campo | Tipo | Por defecto | Descripción |
| :--- | :--- | :--- | :--- |
| `topics` | `String` | `""` | Hasta 6 topics MQTT completos, separados por comas. Cada topic es una página. |

Todo lo demás viene de cada mensaje retained: su `type` (`value`, `table`, `graph`, `weather`), su tiempo en pantalla (`seconds`) y su contenido. La pantalla se mantiene un ciclo completo de sus páginas. Formatos, límites y avisos: [MQTT_DATA_CONTRACT_ES.md](MQTT_DATA_CONTRACT_ES.md); blueprints de Home Assistant: [HOME_ASSISTANT_ES.md](HOME_ASSISTANT_ES.md).

---

*Nota: Todos los esquemas se pueden consultar en vivo en formato JSON mediante `GET /api/engines`.*

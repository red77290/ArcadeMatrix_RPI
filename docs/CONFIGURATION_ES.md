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
| `mapping` | `String` | Cableado/mapeo GPIO (`regular`, `adafruit-hat`, `adafruit-hat-pwm`, ...). |
| `driver_chip` | `String` | Chip controlador (`SHIFTREG`, `FM6126A`). |
| `rgb_sequence` | `String` | Orden de colores (`RGB`, `RBG`, `BGR`, ...). Corrige aquí colores intercambiados. |
| `slowdown` | `int` | Ralentización GPIO (`1`–`4`). Auméntala en Pi 3/4 si ves artefactos. |
| `pwm_bits` | `int` | Profundidad de color. Valor por defecto `11`; bájalo a `8` para ahorrar CPU. |
| `pwm_lsb_nanoseconds` | `int` | Ajuste del ancho de pulso LSB (avanzado). |
| `disable_hardware_pulsing` | `bool` | Ponlo en `true` para evitar que DMA asfixie el Wi-Fi interno (ligero parpadeo). |
| `limit_refresh_rate_hz` | `int` | Limita la frecuencia de refresco (`0` = sin límite). |
| `row_address_mode` | `int` | Tipo de direccionamiento de filas para paneles exóticos (`0` por defecto). |
| `multiplexing` | `int` | Tipo de multiplexado del panel (`0` por defecto). |
| `panel_type` | `String` | Cadena opcional de inicialización del panel (ej. `FM6126A`), normalmente vacía. |

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
| `night_mode_enabled` | `bool` | Activa el apagado automático / la reducción de brillo por la noche. |
| `turn_off_at` | `String` | Hora de inicio de espera (ej. `"23:00"`). |
| `wake_up_at` | `String` | Hora de despertar (ej. `"07:00"`). |
| `night_brightness` | `int` | Brillo de espera (`0` = matriz completamente apagada). |
| `day_brightness` | `int` | Brillo diurno en vivo (`0`–`100`). Se ajusta con el control del panel y se conserva tras un reinicio. |
| `idle_fighter_enabled` | `bool` | Interruptor principal de la superposición decorativa de Luchador sobre las pantallas de rotación inactivas (activación por pantalla mediante cada entrada de rotación). |
| `idle_fighter_interval` | `int` | Segundos entre dos animaciones de combate (mínimo `1`). |

---

## 5. El Bloque `"mqtt"` (Marquees Recalbox / Batocera)

| Clave | Tipo | Descripción |
| :--- | :--- | :--- |
| `enabled` | `bool` | Activa el listener MQTT para marquees estilo Pixelcade. |
| `broker` | `String` | IP/host del broker (normalmente la propia Pi). |
| `port` | `int` | Puerto del broker (por defecto `1883`). |
| `user` | `String` | Usuario del broker (opcional). |
| `pass` | `String` | Contraseña del broker (opcional). |
| `device_name` | `String` | Identificador publicado por este dispositivo. |
| `topic_batocera` | `String` | Tópico suscrito para eventos de juego de Batocera. |
| `topic_recalbox` | `String` | Tópico suscrito para eventos de juego de Recalbox. |

El daemon de sincronización puede instalarse en la consola por SSH desde la interfaz Web (`POST /api/mqtt/install`) y sus logs pueden obtenerse con `POST /api/mqtt/logs`.

---

## 6. Seguridad de la API (`api_auth_enabled` / `api_token`)

Estas dos claves de nivel superior protegen los endpoints de escritura/administración.

| Clave | Tipo | Descripción |
| :--- | :--- | :--- |
| `api_auth_enabled` | `bool` | Si es `true`, los endpoints sensibles requieren que la cabecera `X-API-Token` coincida con `api_token`. |
| `api_token` | `String` | Token secreto (generado automáticamente en el primer arranque). La interfaz Web lo envía como `X-API-Token`. |

Está desactivado por defecto para que la interfaz Web incluida funcione inmediatamente. Actívalo si el dispositivo es accesible más allá de una LAN de confianza.

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
| `theme` | `int` | `0` | Índice del tema de reloj animado: `0` = Digital Estándar, `1` = Flip Clock, `2` = Cyberpunk, `3` = Word Clock, `4` = Binary Clock, `5` = Pac-Man, `6` = Tetris, `7` = Slot Machine, `8` = Versus (M.U.G.E.N), `9` = Pong, `10` = Matrix Rain (Katakana). |
| `format` | `String` | `%H:%M:%S` | Formato de hora strftime. |
| `font` | `String` | `PressStart2P.ttf` | Archivo de fuente de `/fonts/`. |
| `size` | `int` | `2` | Factor de escalado de la fuente. |
| `color_1` | `String` | `#FFFFFF` | Color hexadecimal primario (inicio de degradado en tema Custom). |
| `color_2` | `String` | `#FFFFFF` | Color hexadecimal secundario (fin de degradado en tema Custom). |
| `offset_x` | `int` | `0` | Desplazamiento horizontal en píxeles. |
| `offset_y` | `int` | `0` | Desplazamiento vertical en píxeles. |

### Motor: `date`
| Campo | Tipo | Por defecto | Descripción |
| :--- | :--- | :--- | :--- |
| `theme` | `int` | `0` | Índice del tema de fecha. |
| `format` | `String` | `%d/%m` | Formato de fecha strftime. |
| `font` | `String` | `PressStart2P.ttf` | Archivo de fuente de `/fonts/`. |
| `size` | `int` | `2` | Factor de escalado de la fuente. |
| `color_1` | `String` | `#FFFFFF` | Color hexadecimal primario. |
| `color_2` | `String` | `#FFFFFF` | Color hexadecimal secundario. |
| `offset_x` | `int` | `0` | Desplazamiento horizontal en píxeles. |
| `offset_y` | `int` | `0` | Desplazamiento vertical en píxeles. |

### Motor: `crypto`
| Campo | Tipo | Por defecto | Opciones | Descripción |
| :--- | :--- | :--- | :--- | :--- |
| `symbols` | `String` | `BTC,ETH` | Separados por comas | Símbolos de criptomonedas a monitorear (CoinGecko / Binance). |
| `currency` | `Options` | `USD` | `USD`, `EUR`, `GBP`, `JPY` | Moneda de comparación y símbolo de visualización (`$`, `€`, `£`, `¥`). |
| `show_chart` | `bool` | `true` | `true`, `false` | Mostrar el gráfico sparkline histórico. |
| `chart_timeframe` | `Options` | `daily` | `hourly`, `daily`, `weekly`, `monthly` | Intervalo temporal para el historial de precios. |
| `page_seconds` | `int` | `5` | `3` a `30` | Segundos de permanencia en cada página antes de alternar. |
| `cache_ttl_min` | `int` | `1` | `1` a `60` | Minutos de caché para la cotización. |

### Motor: `stock`
| Campo | Tipo | Por defecto | Opciones | Descripción |
| :--- | :--- | :--- | :--- | :--- |
| `symbols` | `String` | `AAPL,NVDA,TSLA` | Separados por comas | Tickers bursátiles a monitorear (Yahoo Finance). |
| `currency` | `Options` | `USD` | `USD`, `EUR`, `GBP`, `JPY` | Moneda de comparación y símbolo de visualización (`$`, `€`, `£`, `¥`). |
| `show_chart` | `bool` | `true` | `true`, `false` | Mostrar el gráfico sparkline histórico. |
| `chart_timeframe` | `Options` | `daily` | `hourly`, `daily`, `weekly`, `monthly` | Intervalo temporal para el historial de precios. |
| `page_seconds` | `int` | `5` | `3` a `30` | Segundos de permanencia en cada página antes de alternar. |
| `cache_ttl_min` | `int` | `1` | `1` a `60` | Minutos de caché para la cotización. |

### Motor: `weather`
| Campo | Tipo | Predeterminado | Opciones | Descripción |
| :--- | :--- | :--- | :--- | :--- |
| `api_key` | `String` | `""` | Clave API gratuita | Clave API de OpenWeatherMap (gratis en [openweathermap.org](https://home.openweathermap.org/users/sign_up)). |
| `city` | `String` | `""` | Texto | Ubicación de la ciudad para el pronóstico (ver guía abajo). |
| `units` | `Options` | `metric` | `metric`, `imperial` | Unidad de temperatura: `metric` para Celsius (°C) o `imperial` para Fahrenheit (°F). |
| `lang` | `Options` | `en` | `en`, `fr`, `es` | Idioma de las etiquetas de días (TODAY / AUJ. / HOY). |
| `offset_x` | `int` | `0` | `-64` a `64` | Desplazamiento horizontal de píxeles. |
| `offset_y` | `int` | `0` | `-32` a `32` | Desplazamiento vertical de píxeles. |

#### Cómo formatear el campo `city` en OpenWeatherMap
OpenWeatherMap utiliza el código de país ISO 3166 (y el código de estado de 2 letras para EE. UU.):
* **Ubicaciones Internacionales:** Use `Ciudad,CodigoPais` (ej. `Paris,FR`, `London,GB`, `Tokyo,JP`, `Montreal,CA`).
* **Ubicaciones en Estados Unidos:** Use `Ciudad,CodigoEstado,CodigoPais` (ej. `Tucson,AZ,US`, `Miami,FL,US`, `Dallas,TX,US`).
* **Dónde buscar el nombre exacto:** Vaya a [openweathermap.org](https://openweathermap.org) y busque su ciudad.

### Motor: `sysinfo` (Monitor de Sistema)
| Campo | Tipo | Por defecto | Opciones | Descripción |
| :--- | :--- | :--- | :--- | :--- |
| `theme` | `int` | `0` | `0` a `2` | Tema visual: `0` = Barras de nivel de color, `1` = Cuadrícula compacta 4 bloques, `2` = Terminal Retro. |
| `show_cpu` | `bool` | `true` | `true`, `false` | Muestra el porcentaje de uso de CPU en tiempo real (CPU %). |
| `show_ram` | `bool` | `true` | `true`, `false` | Muestra el porcentaje de uso de memoria RAM (RAM %). |
| `show_temp` | `bool` | `true` | `true`, `false` | Muestra la temperatura de hardware del SoC (escala dinámica verde/ámbar/rojo). |
| `show_uptime` | `bool` | `true` | `true`, `false` | Muestra el tiempo de actividad del sistema (Uptime) en horas/días. |
| `temp_unit` | `Options` | `C` | `C`, `F` | Unidad de temperatura: Celsius (`C`) o Fahrenheit (`F`). |
| `offset_x` | `int` | `0` | `-64` a `64` | Desplazamiento horizontal en píxeles. |
| `offset_y` | `int` | `0` | `-32` a `32` | Desplazamiento vertical en píxeles. |

### Motor: `fighter` (Combate M.U.G.E.N)
| Campo | Tipo | Por defecto | Opciones | Descripción |
| :--- | :--- | :--- | :--- | :--- |
| `mode` | `Options` | `match` | `match`, `showcase` | Modo de combate: `match` (duelo con K.O. y victoria) o `showcase` (demostración continua). |
| `fighter_1` | `String` | `""` | Nombre de carpeta | Luchador P1 (dejar vacío para selección aleatoria). |
| `fighter_2` | `String` | `""` | Nombre de carpeta | Luchador P2 (dejar vacío para selección aleatoria). |
| `show_hud` | `bool` | `true` | `true`, `false` | Muestra barras de vida retro (HP), medidores de Super y nombres de luchadores. |
| `match_duration` | `int` | `30` | `10` a `120` | Duración máxima del asalto en segundos antes de agotar el tiempo. |

### Motor: `dashboard` (Smart Dashboard Hub)
| Campo | Tipo | Por defecto | Opciones | Descripción |
| :--- | :--- | :--- | :--- | :--- |
| `clock_mode` | `Options` | `0` | `0` (Digital), `1` (Esfera), `2` (Mínimo) | Estilo de visualización del reloj |
| `theme` | `Options` | `0` | `0` (Cyberpunk), `1` (Amber HUD), `2` (Minimalist), `3` (Matrix) | Paleta de colores para los widgets |
| `show_clock` | `bool` | `true` | `true`, `false` | Muestra el reloj principal |
| `show_world_clock` | `bool` | `true` | `true`, `false` | Muestra zonas horarias mundiales secundarias |
| `world_clocks` | `String` | `NYC,TYO,LON` | Códigos de aeropuerto | Zonas horarias separadas por comas (ej. `NYC,TYO,LON,PAR,SFO`) |
| `show_weather` | `bool` | `true` | `true`, `false` | Muestra el widget meteorológico en vivo |
| `weather_city` | `String` | `Paris,FR` | Ciudad | Consulta de ciudad para OpenWeatherMap |
| `show_markets` | `bool` | `true` | `true`, `false` | Muestra el teletipo bursátil y cripto |
| `tracked_markets` | `String` | `BTC,ETH,NVDA,AAPL` | Símbolos | Símbolos bursátiles separados por comas |
| `show_sysinfo` | `bool` | `true` | `true`, `false` | Muestra el uso de CPU % y RAM % |
| `show_date` | `bool` | `true` | `true`, `false` | Muestra la fecha actual |
| `show_seconds` | `bool` | `true` | `true`, `false` | Muestra el contador de segundos |

### Motor: `google_cast` (Google Home / Nest Audio)
| Campo | Tipo | Por defecto | Descripción |
| :--- | :--- | :--- | :--- |
| `device_ip` | `String` | `""` | IP estática de su altavoz Google Home / Nest Audio. Deje en blanco para descubrimiento mDNS automático en la red local. |
| `device_name` | `String` | `""` | Filtro por nombre del dispositivo (ej. `Salón`) durante el escaneo automático en la LAN. |
| `show_album_art` | `bool` | `true` | Descarga y muestra la carátula del álbum en el lado izquierdo de la matriz. |
| `show_progress` | `bool` | `true` | Muestra la barra de progreso de reproducción en la parte inferior. |
| `show_visualizer` | `bool` | `true` | Muestra un ecualizador de frecuencias de audio animado durante la reproducción. |
| `show_volume` | `bool` | `true` | Muestra el nivel de volumen actual del altavoz Google Nest. |

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
| `text` | `String` | `Hello` | Texto | Texto del mensaje o cartel a mostrar. |
| `color` | `String` | `#ffffff` | Color Hex | Color del texto en formato `#RRGGBB`. |
| `size` | `int` | `1` | `1` a `4` | Multiplicador de escala de la fuente. |
| `direction` | `Options` | `left` | `left`, `none` | Dirección de desplazamiento (`left` para desplazamiento hacia la izquierda, `none` para texto estático centrado). |
| `speed` | `int` | `50` | `10` a `200` | Milisegundos por píxel de desplazamiento (menor = más rápido; ignorado en estático). |
| `font` | `String` | `Default` | Dinámico | Archivo de fuente desde `/fonts/`. |

### Motor: `marquee`
| Campo | Tipo | Predeterminado | Descripción |
| :--- | :--- | :--- | :--- |
| *(auto)* | `None` | — | Motor interno de sincronización de marquesinas Pixelcade / Recalbox / Batocera recibidas mediante MQTT o Webhook. |

---

*Nota: Todos los esquemas se pueden consultar en vivo en formato JSON mediante `GET /api/engines`.*

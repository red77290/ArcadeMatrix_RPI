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
| `unit` | `String` | Unidad de medida para el clima (`metric` / `imperial`). |
| `temp_offset` | `float` | Offset de calibración aplicado a la temperatura reportada. |
| `night_mode_enabled` | `bool` | Activa el apagado automático / la reducción de brillo por la noche. |
| `turn_off_at` | `String` | Hora de inicio de espera (ej. `"23:00"`). |
| `wake_up_at` | `String` | Hora de despertar (ej. `"07:00"`). |
| `night_brightness` | `int` | Brillo de espera (`0` = matriz completamente apagada). |
| `day_brightness` | `int` | Brillo diurno en vivo (`0`–`100`). Se ajusta con el control del panel y se conserva tras un reinicio. |

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
Define el orden de visualización y la duración por slot.

```json
{
  "instance_id": "crypto_main",
  "duration_sec": 30
}
```
Solo se inicializan las instancias enumeradas aquí, ahorrando memoria para funciones no utilizadas. La rotación se puede editar desde el panel **Rotation** de la interfaz Web (`GET`/`POST /api/rotation`).

> Nota: `duration_sec` pertenece a la entrada de **rotation**, no al `config` de la instancia.

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
| `theme` | `int` | `0` | Índice del tema de reloj animado. |
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

Otros motores registrados (`weather`, `crypto`, `stock`, `gif`, `message`, `marquee`, `spotify`) exponen sus propios campos de la misma forma — consulta `GET /api/engines` para ver el schema autoritativo y siempre actualizado.

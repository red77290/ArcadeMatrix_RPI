🇬🇧 [English](CONFIGURATION.md) | 🇫🇷 [Français](CONFIGURATION_FR.md) | 🇪🇸 Español

# Configuración Detallada (config.json) - Raspberry Pi

El sistema de configuración se basa exclusivamente en un archivo `config.json` ubicado en la raíz del proyecto. Gestiona la configuración del hardware, Wi-Fi y la lógica de los bloques lógicos independientes ("instancias").

---

## 1. Estructura Global

```json
{
  "matrix": { ... },
  "wifi": { ... },
  "system": { ... },
  "instances": [ ... ],
  "rotation": [ ... ]
}
```

---

## 2. El Bloque `"matrix"` (Controlador Hardware)

Este bloque configura los parámetros DMA para la librería hzeller.

| Clave | Tipo | Descripción |
| :--- | :--- | :--- |
| `width` | `int` | Ancho de un solo panel (ej. `64`). |
| `height` | `int` | Alto de un solo panel (ej. `32`). |
| `chain_length` | `int` | Número de paneles encadenados horizontalmente. |
| `parallel` | `int` | Número de cadenas paralelas (Específico de Raspberry Pi). |
| `pwm_bits` | `int` | Profundidad de color. Valor por defecto `11`. Puede bajarse a `8` para ahorrar CPU. |
| `driver_chip` | `String` | Chip controlador (`SHIFTREG`, `FM6126A`). |
| `brightness` | `int` | Limitador de brillo máximo por software (`0` a `100`). |

---

## 3. El Bloque `"system"` (Entorno y Espera)

| Clave | Tipo | Descripción |
| :--- | :--- | :--- |
| `timezone` | `String` | Cadena POSIX (ej. `CET-1CEST,M3.5.0,M10.5.0/3`). |
| `format_24h` | `bool` | Formato de hora. `true` = 23:00, `false` = 11:00 PM. |
| `lang` | `String` | Idioma del sistema (ej. `en`, `es`). |
| `night_mode_enabled` | `bool` | Activa el apagado automático o la reducción de brillo por la noche. |
| `turn_off_at` | `String` | Hora de inicio de espera (ej. `"23:00"`). |
| `wake_up_at` | `String` | Hora de despertar (ej. `"07:00"`). |
| `night_brightness` | `int` | Brillo de espera (`0` = matriz completamente apagada). |
| `fighter_enabled` | `bool` | Activa la superposición de sprites de combate MUGEN (`.fgt`) sobre otros motores. |

---

## 4. El Bloque `"wifi"`

| Clave | Tipo | Descripción |
| :--- | :--- | :--- |
| `ssid` | `String` | El nombre de su red Wi-Fi. |
| `password` | `String` | La clave WPA2. |
| `disable_internal_wifi` | `bool` | Si usa un adaptador externo, deshabilite el Wi-Fi interno de la Pi. |

---

## 5. Motores: `"instances"` & `"rotation"`

La arquitectura desacoplada permite crear múltiples copias configuradas de forma independiente del mismo Motor.

### `"instances"`
Esta es una matriz que contiene la configuración de cada bloque lógico.

```json
{
  "instance_id": "crypto_main",
  "engine_id": "crypto",
  "config": {
    "symbols": "BTC,ETH,SOL",
    "duration_sec": 10
  }
}
```
* `instance_id`: Nombre único de este bloque.
* `engine_id`: El identificador interno del Motor Rust.
* `config`: Un objeto JSON dinámico específico para el motor (sus `Capabilities`).

### `"rotation"`
Define el orden de visualización en la pantalla.

```json
{
  "instance_id": "crypto_main",
  "duration_sec": 30
}
```
La aplicación solo inicializará los motores enumerados aquí, ahorrando memoria para las funciones no enumeradas.

🇬🇧 English | 🇫🇷 [Français](CONFIGURATION_FR.md) | 🇪🇸 [Español](CONFIGURATION_ES.md)

# Detailed Configuration (config.json) - Raspberry Pi

The configuration system relies exclusively on a `config.json` file located at the root of the project. It handles hardware configuration, Wi-Fi, and the logic of the independent logical blocks ("instances").

---

## 1. Global Structure

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

## 2. The `"matrix"` Block (Hardware Driver)

This block configures the DMA parameters for the hzeller library.

| Key | Type | Description |
| :--- | :--- | :--- |
| `width` | `int` | Width of a single panel (e.g., `64`). |
| `height` | `int` | Height of a single panel (e.g., `32`). |
| `chain_length` | `int` | Number of panels chained horizontally. |
| `parallel` | `int` | Number of parallel chains (Raspberry Pi specific). |
| `pwm_bits` | `int` | Color depth. Default value `11`. Can be lowered to `8` to save CPU. |
| `driver_chip` | `String` | Controller chip (`SHIFTREG`, `FM6126A`). |
| `brightness` | `int` | Maximum software brightness limiter (`0` to `100`). |

---

## 3. The `"system"` Block (Environment and Standby)

| Key | Type | Description |
| :--- | :--- | :--- |
| `timezone` | `String` | POSIX string (e.g., `CET-1CEST,M3.5.0,M10.5.0/3`). |
| `format_24h` | `bool` | Time format. `true` = 23:00, `false` = 11:00 PM. |
| `lang` | `String` | System language (e.g., `en`, `fr`). |
| `night_mode_enabled` | `bool` | Enables automatic turn-off or brightness reduction at night. |
| `turn_off_at` | `String` | Standby start time (e.g., `"23:00"`). |
| `wake_up_at` | `String` | Wake-up time (e.g., `"07:00"`). |
| `night_brightness` | `int` | Standby brightness (`0` = matrix completely off). |
| `fighter_enabled` | `bool` | Enables MUGEN combat sprites overlay (`.fgt`) on top of other engines. |

---

## 4. The `"wifi"` Block

| Key | Type | Description |
| :--- | :--- | :--- |
| `ssid` | `String` | The name of your Wi-Fi network. |
| `password` | `String` | The WPA2 key. |
| `disable_internal_wifi` | `bool` | If using an external dongle, disable the Pi's internal Wi-Fi. |

---

## 5. Engines: `"instances"` & `"rotation"`

The decoupled architecture allows you to create multiple independent configured copies of the same Engine.

### `"instances"`
This is an array containing the configuration of each logical block.

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
* `instance_id`: Unique name of this block.
* `engine_id`: The internal identifier of the Rust Engine.
* `config`: A dynamic JSON object specific to the engine (its `Capabilities`).

### `"rotation"`
Defines the display order on the screen.

```json
{
  "instance_id": "crypto_main",
  "duration_sec": 30
}
```
The application will only initialize engines listed here, saving memory for unlisted features.

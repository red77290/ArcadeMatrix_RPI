🇬🇧 English | 🇫🇷 [Français](CONFIGURATION_FR.md) | 🇪🇸 [Español](CONFIGURATION_ES.md)

# Detailed Configuration (config.json) - Raspberry Pi

The configuration system relies exclusively on a single `config.json` file located at the root of the project (or on the **DATA** partition of the pre-built image). It handles the hardware driver, network, MQTT integration, system behaviour, API security, and the decoupled logic of each engine ("instances").

> The legacy `conf.ini` format has been fully removed. `config.json` is now the **single source of truth**. On boot the file is validated and self-healed (see §8), so a partial or hand-edited file is safe: missing keys are re-created with their defaults.

---

## 1. Global Structure

```json
{
  "matrix": { ... },
  "wifi": { ... },
  "mqtt": { ... },
  "data_mqtt": { ... },
  "showcase": { ... },
  "system": { ... },
  "instances": [ ... ],
  "rotation": [ ... ],
  "api_auth_enabled": false,
  "api_token": ""
}
```

---

## 2. The `"matrix"` Block (Hardware Driver)

This block configures the DMA parameters for the hzeller `rpi-rgb-led-matrix` library. Changing any hardware value triggers an automatic restart so the new driver settings take effect.

| Key | Type | Description |
| :--- | :--- | :--- |
| `width` | `int` | Width of a single panel (e.g., `64`). |
| `height` | `int` | Height of a single panel (e.g., `32`). |
| `chain_length` | `int` | Number of panels chained horizontally. |
| `parallel` | `int` | Number of parallel panel chains (`1`–`3`, default `1`). |
| `power_limit_percent` | `int` | Global matrix brightness/power limit percentage (`1`–`100`, default `40`). |
| `force_single_buffer` | `bool` | Force single-buffer mode when true (default `false`). |
| `color_depth` | `int` | Logical color depth in bits (`24` default). |
| `mapping` | `String` | GPIO wiring/mapping (`regular`, `adafruit-hat`, `adafruit-hat-pwm`, ...). |
| `driver_chip` | `String` | Controller chip (`SHIFTREG`, `FM6126A`). |
| `rgb_sequence` | `String` | Color order (`RGB`, `RBG`, `BGR`, ...). Fix swapped colors here. |
| `clk_phase` | `bool` | Clock phase inversion (`false` default). |
| `latch_blanking` | `int` | Latch blanking cycles (`0`–`8`, default `1`). |
| `matrix_power` | `bool` | Master software power state of the LED matrix (`true` default). |
| `slowdown` | `int` | GPIO slowdown (`1`–`4`). Increase on Pi 3/4 if you see artifacts. |
| `pwm_bits` | `int` | Color depth. Default `11`; lower to `8` to save CPU. |
| `pwm_lsb_nanoseconds` | `int` | LSB pulse width tuning (advanced, default `130`). |
| `disable_hardware_pulsing` | `bool` | Set `true` to stop DMA starving the internal Wi-Fi (slight flicker). |
| `limit_refresh_rate_hz` | `int` | Cap the refresh rate (`0` = uncapped). |
| `row_address_mode` | `int` | Row addressing type for exotic panels (`0` default). |
| `multiplexing` | `int` | Panel multiplexing type (`0` default). |
| `panel_type` | `String` | Optional panel init string (e.g. `FM6126A`), usually empty. |
| `rotation` | `int` | Physical screen orientation in degrees (`0`, `90`, `180`, `270`, default `0`). |
| `transition_effect` | `String` | Visual transition effect applied on orientation changes (`none`, `vortex`, `fade`, `slide`, ...). Default `none`. |
| `transition_duration_ms` | `int` | Orientation transition duration in milliseconds (`100`–`3000`, default `500`). |
| `render_pipeline` | `String` | Hardware rendering pipeline & buffer strategy: `auto` (default auto-selection), `canvas_single` (**Recommended**, single off-screen canvas with dirty-bounds tracking protecting Wi-Fi SDIO bus from saturation), `canvas_double` (double-buffered canvas), `direct_double` (zero-copy double buffering via C driver), `direct_single` (direct single frame buffer). Default `canvas_single`. |
| `slot_transition` | `String` | Effect played when the rotation moves to the next slot (`wipe`, `curtain`, `shutter`, `dissolve`, `checker`, `matrix`, `slide`, `zoom`, `glitch`, `random`, `none`). Default `none`. |
| `slot_transition_duration_ms` | `int` | Slot transition duration in milliseconds (`100`-`3000`, default `500`). |

> Live daytime brightness is **not** stored in this block; it is controlled at runtime from the Web UI (Dashboard slider → `POST /api/system { "brightness_limit": 0-100 }`). Night brightness lives in the `system` block (§4).

---

## 3. The `"wifi"` Block

| Key | Type | Description |
| :--- | :--- | :--- |
| `ssid` | `String` | The name of your Wi-Fi network. |
| `password` | `String` | The WPA2 key. |
| `hostname` | `String` | Device hostname advertised on the network. |
| `configured` | `bool` | Set to `false` to force a (re)connection attempt on next boot. Set back to `true` automatically on success. |
| `disable_internal` | `bool` | If using an external USB dongle, disable the Pi's internal Wi-Fi (changing this triggers a restart). |

You can also push credentials at runtime with `POST /api/wifi { "ssid": "...", "password": "..." }`, which sets `configured=false` and restarts the network provisioning.

---

## 4. The `"system"` Block (Environment & Standby)

| Key | Type | Description |
| :--- | :--- | :--- |
| `timezone` | `String` | POSIX string (e.g., `CET-1CEST,M3.5.0,M10.5.0/3`). |
| `format_24h` | `bool` | Time format. `true` = 23:00, `false` = 11:00 PM. |
| `lang` | `String` | System language (e.g., `en`, `fr`, `es`). |
| `temp_unit` | `String` | Global temperature unit (`"C"` for Celsius or `"F"` for Fahrenheit, default `"C"`). |
| `night_mode_enabled` | `bool` | Enables automatic turn-off / brightness reduction at night. |
| `turn_off_at` | `String` | Standby start time (e.g., `"23:00"`). |
| `wake_up_at` | `String` | Wake-up time (e.g., `"07:00"`). |
| `night_brightness` | `int` | Standby brightness (`0` = matrix completely off). |
| `day_brightness` | `int` | Live daytime brightness (`0`–`100`). Set from the dashboard slider and persisted across restarts. |
| `idle_fighter_enabled` | `bool` | Master switch for the decorative Fighter overlay composited on top of idle rotation screens (per-screen opt-in via each rotation entry). |
| `idle_fighter_interval` | `int` | Seconds between two fight animations (minimum `1`, default `10`). |
| `idle_fighter_speed` | `int` | Animation speed percentage for the Fighter overlay (`25`–`200`, default `100`). |

---

## 5. The `"mqtt"` Block (Recalbox / Batocera Marquees)

| Key | Type | Description |
| :--- | :--- | :--- |
| `enabled` | `bool` | Enable the MQTT listener for Pixelcade-style marquees. |
| `broker` | `String` | Broker IP/host (usually the Pi itself). |
| `port` | `int` | Broker port (default `1883`). |
| `user` | `String` | Broker username (optional). |
| `pass` | `String` | Broker password (optional). |
| `device_name` | `String` | Identifier published by this device. |
| `allow_overlay` | `bool` | Allow decorative overlays (e.g. Street Fighter) on top of MQTT/marquee screens (default `false`). |
| *(auto-subscription)* | `system/playing/#` | Subscribes automatically to all supported retro gaming systems: `system/playing/recalbox`, `system/playing/batocera`, `system/playing/retropie`. |

The sync daemon can be installed on the console (Recalbox, Batocera, RetroPie) over SSH from the Web UI (`POST /api/mqtt/install`) with target OS selection or auto-detection, and its logs fetched with `POST /api/mqtt/logs`.

> [!NOTE]
> For Batocera, version **v33 or newer** is required for dynamic marquee browsing (`game-selected` and `system-selected` hooks). Batocera v32 and earlier only trigger game launch/stop events. Recalbox is supported on all versions.

---

## 5b. The `"data_mqtt"` Block (MQTT Data broker)

Broker credentials for the `mqttdata` engine. This connection is separate from the `"mqtt"` block above (marquees / control, which can take over the display). Nothing runs in the background: the engine connects only while its screen is active and disconnects when it leaves the rotation.

| Key | Type | Description |
| :--- | :--- | :--- |
| `broker` | `String` | Broker host or IP. |
| `port` | `int` | Broker port (default `1883`). |
| `user` | `String` | Username (optional). |
| `pass` | `String` | Password (optional). |

The MQTT client id is `<hostname>-data`. Changes apply on the engine's next activation (no restart).

## 6. API Security (`api_auth_enabled` / `api_token`)

These two top-level keys secure the write/administrative endpoints.

| Key | Type | Description |
| :--- | :--- | :--- |
| `api_auth_enabled` | `bool` | If `true`, sensitive endpoints require the `X-API-Token` header to match `api_token`. |
| `api_token` | `String` | Secret token (auto-generated on first boot). Sent by the Web UI as `X-API-Token`. |

Disabled by default so the bundled Web UI works out of the box. Enable it if the device is reachable beyond a trusted LAN.

---

### Clock instance options

Beyond `theme`, `font`, `size` and the colours, a clock instance takes:

| Key | Type | Description |
| :--- | :--- | :--- |
| `clock_glow` | `enum` | Outline around the digits: `0` off, `1` neon (the chosen colour at full strength with a near-white centre, as the Matrix face draws it), `2` a colour of its own. Default `0`. |
| `clock_glow_color` | `color` | Outline colour when `clock_glow` is `2` (default `#00FF41`). |

Themes `30`-`34` and `37` are faces adapted from the Clockwise clockfaces. Those named `(256x64)` are
laid out for a wide panel and show a notice on smaller ones.

---

## 7. Engines: `"instances"` & `"rotation"`

The decoupled architecture lets you create multiple independent, differently-configured copies of the same Engine.

### `"instances"`
An array holding the configuration of each logical block.

```json
{
  "instance_id": "crypto_main",
  "engine_id": "crypto",
  "config": {
    "symbols": "BTC,ETH,SOL"
  }
}
```
* `instance_id`: Unique name of this block.
* `engine_id`: The internal identifier of the Rust Engine (must be a registered engine — see §9).
* `config`: A flat map of `String` values specific to the engine, validated against its `ConfigSchema`.

Editing an instance through the Web UI (`POST /api/instances`) is applied **live, without a restart**: the runtime calls the engine's `on_config_changed()` on the next frame (Lazy-Once hot-reload). Adding or removing an instance resets the rotation cleanly.

### `"rotation"`
Defines the display order, per-slot duration, and transverse overlay toggles.

```json
{
  "instance_id": "crypto_main",
  "duration_sec": 30,
  "overlays": {
    "fighter": true
  }
}
```
* `instance_id`: Target engine instance.
* `duration_sec`: Dwell duration in seconds (or playback budget for self-paced engines like GIF).
* `overlays.fighter`: (`bool`) Granular toggle for the M.U.G.E.N Fighter decorative overlay on this specific screen.

Only instances listed here are ever initialized, saving memory for unused features. The rotation is editable from the Web UI **Rotation** panel (`GET`/`POST /api/rotation`).

> **Human-Readable Persistence**: When saved to disk (`config.json`), the file is always written formatted and indented (`to_string_pretty`) so it can be cleanly inspected and edited by hand without breaking.

---

## 8. Self-Healing Validation

On every boot **and** on every write via `POST /api/instances`, the `ConfigSanitizer` reconciles each instance against its engine `ConfigSchema`:

* **Missing key** → the schema `default_value` is injected.
* **Integer / Float** → parsed and, if out of `min`/`max`, clamped or reset to default (per the field's `validation_policy`).
* **Boolean** → normalized (`true/1/yes/on` → `true`, `false/0/no/off` → `false`); an unparseable value falls back to the default.
* **Options** → the value must be one of the declared options (comma-separated list for multi-select); otherwise it falls back to the default.
* **Obsolete keys** → keys no longer present in the schema (e.g. after an OTA that renamed a field) are pruned.

The result is saved atomically, so an OTA that adds a new field self-populates it without any user intervention.

---

## 9. Engine Configurations

Each engine advertises its own fields through its `ConfigSchema` (discoverable at `GET /api/engines`, which is what powers the dynamic Web UI). The most common engines:

### Engine: `clock`
| Field | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `theme` | `Options` | `0` | Animated clock theme index: `0` = Digital Standard, `1` = Flip Clock, `2` = Cyberpunk, `3` = Word Clock, `4` = Binary Clock, `5` = Pac-Man, `6` = Tetris, `7` = Slot Machine, `8` = Versus (M.U.G.E.N), `9` = Pong, `10` = Matrix Rain (Katakana), `30` = Mario (Koopa kick & Tate vertical climb), `31` = Castlevania (Simon Belmont & whip attack), `32` = Pocket Index (Pokémon), `33` = World Clock, `34` = Ms. Pac-Man, `35` = Mega Man (Buster charge & Metool), `37` = Words Clock, `39` = Sonic (Green Hill & Badnik Motobug), `41` = Metal Slug (Marco Rossi, Di-Cokka & grenade arcs). |
| `format` | `String` | `%H:%M:%S` | strftime time format. |
| `font` | `String` | `PressStart2P.ttf` | Font file from `/fonts/`. |
| `timezone` | `Options` | `Europe/Paris` | Timezone or region override. |
| `size` | `int` | `2` | Font scaling factor. |
| `clock_speed` | `int` | `100` | Animation speed in percent (Tetris block fall, Pac-Man sweep, Mario run); lower is slower. |
| `clock_glow` | `Options` | `0` | Halo around the digits (`0` off, `1` neon, `2` custom outline color). |
| `clock_glow_color` | `String` | `#00FF41` | Outline color when `clock_glow` is `2`. |
| `color_1` | `String` | `#FFFFFF` | Primary hex color (gradient start on Custom theme). |
| `color_2` | `String` | `#FFFFFF` | Secondary hex color (gradient end on Custom theme). |
| `offset_x` | `int` | `0` | Horizontal pixel offset. |
| `offset_y` | `int` | `0` | Vertical pixel offset. |

### Engine: `date`
| Field | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `theme` | `Options` | `0` | Date theme index. |
| `format` | `String` | `%d/%m` | strftime date format. |
| `font` | `String` | `PressStart2P.ttf` | Font file from `/fonts/`. |
| `timezone` | `Options` | `Europe/Paris` | Timezone or region override. |
| `size` | `int` | `2` | Font scaling factor. |
| `color_1` | `String` | `#FFFFFF` | Primary hex color. |
| `color_2` | `String` | `#FFFFFF` | Secondary hex color. |
| `offset_x` | `int` | `0` | Horizontal pixel offset. |
| `offset_y` | `int` | `0` | Vertical pixel offset. |

### Engine: `dashboard` (Smart Dashboard Hub)
| Field | Type | Default | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `theme` | `Options` | `0` | `0` (Cyberpunk Neon), `1` (Amber HUD), `2` (Minimalist Luxury), `3` (Matrix Phosphor) | Color palette for dashboard widgets and borders. |
| `clock_mode` | `Options` | `1` | `0` (Digital), `1` (Watch Dial), `2` (Minimal) | Clock rendering face and dial style. |
| `timezone` | `Options` | `system` | `system` or IANA timezone | Timezone override or device system setting. |
| `format_24h` | `Options` | `system` | `system`, `true`, `false` | 24-hour or 12-hour time format. |
| `temp_unit` | `Options` | `system` | `system`, `C`, `F` | Celsius (°C) or Fahrenheit (°F). |
| `lang` | `Options` | `system` | `system`, `en`, `fr`, `es` | Language for dashboard strings and dates. |
| `show_clock` | `bool` | `true` | `true`, `false` | Display primary time widget. |
| `show_world_clock` | `bool` | `true` | `true`, `false` | Display secondary world timezones (`NYC`, `TYO`, `LON`...). |
| `world_clocks` | `String` | `NYC,TYO,LON` | Airport/City codes | Comma-separated list of secondary timezone codes. |
| `show_weather` | `bool` | `true` | `true`, `false` | Display outdoor temperature and weather forecast. |
| `weather_city` | `String` | `Paris, FR` | Text | City location for weather forecast (inherits `city` from the `weather` instance if left at default). |
| `weather_api_key` | `String` | `""` | Optional API key | Optional OpenWeatherMap API key (inherits `api_key` from the `weather` instance if empty, and falls back automatically to free Open-Meteo without an API key). |
| `refresh_interval` | `Options` | `10` | `1`, `5`, `10`, `15`, `30`, `60` min | Background refresh frequency for weather and market quotes. |
| `show_markets` | `bool` | `true` | `true`, `false` | Display real-time crypto and stock ticker badges. |
| `tracked_markets` | `String` | `BTC,ETH,SOL,NVDA` | Comma-separated | Crypto and stock ticker symbols to cycle through. |
| `show_sysinfo` | `bool` | `true` | `true`, `false` | Display CPU, RAM & Wi-Fi metrics. |
| `show_date` | `bool` | `true` | `true`, `false` | Display calendar date badge. |
| `show_seconds` | `bool` | `true` | `true`, `false` | Display seconds in clock. |
| `smooth_seconds` | `bool` | `true` | `true`, `false` | Smooth sweeping seconds vs crisp ticks. |
| `offset_x` | `int` | `0` | `-64` to `64` | Horizontal pixel offset. |
| `offset_y` | `int` | `0` | `-32` to `32` | Vertical pixel offset. |

### Engine: `crypto`
| Field | Type | Default | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `symbols` | `String` | `BTC,ETH` | Comma-separated | Crypto symbols to monitor (CoinGecko / Binance). |
| `show_chart` | `bool` | `true` | `true`, `false` | Display historical price sparkline chart. |
| `chart_timeframe` | `Options` | `daily` | `hourly`, `daily`, `weekly`, `monthly` | Timeframe for historical price series. |
| `page_seconds` | `int` | `5` | `3` to `30` | Seconds to dwell on each page. Because `crypto` is self-paced in rotation, it overrides the slot timer and displays all configured symbols for one complete cycle before rotation advances. |
| `currency` | `Options` | `USD` | `USD`, `EUR`, `GBP`, `JPY` | Currency comparison and display symbol (`$`, `€`, `£`, `¥`). |
| `cache_ttl_min` | `int` | `1` | `1` to `60` | Minutes to cache quote price. |

### Engine: `stock`
| Field | Type | Default | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `symbols` | `String` | `AAPL,NVDA,TSLA` | Comma-separated | Stock ticker symbols to monitor (Yahoo Finance). |
| `show_chart` | `bool` | `true` | `true`, `false` | Display historical price sparkline chart. |
| `chart_timeframe` | `Options` | `daily` | `hourly`, `daily`, `weekly`, `monthly` | Timeframe for historical price series. |
| `page_seconds` | `int` | `5` | `3` to `30` | Seconds to dwell on each page. Because `stock` is self-paced in rotation, it overrides the slot timer and displays all configured symbols for one complete cycle before rotation advances. |
| `currency` | `Options` | `USD` | `USD`, `EUR`, `GBP`, `JPY` | Currency comparison and display symbol (`$`, `€`, `£`, `¥`). |
| `cache_ttl_min` | `int` | `1` | `1` to `60` | Minutes to cache quote price. |

### Engine: `gnews` (GNews Live Feed & Breaking News Ticker)
| Field | Type | Default | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `api_key` | `String` | `""` | Any valid key | GNews.io API key (optional; uses demo articles if empty). |
| `category` | `Options` | `technology` | `general`, `world`, `nation`, `business`, `technology`, `entertainment`, `sports`, `science`, `health` | Primary news category. |
| `keywords` | `String` | `""` | Text / Query | Custom search query or filter tags (e.g. `ai OR arcade`). |
| `lang` | `Options` | `auto` | `auto`, `en`, `fr`, `es`, `de`, `it`, `pt`, `nl`, `ru`, `zh`, `ja` | Article language (`auto` syncs with system language). |
| `country` | `Options` | `auto` | `auto`, `us`, `fr`, `gb`, `es`, `de`, `ca`, `it`, `jp`, `au`, `br`, `in` | Country edition (`auto` uses local region). |
| `max_articles` | `int` | `5` | `3` to `15` | Maximum number of articles cached and rotated in randomized non-repeating order across rotation slots. |
| `articles_per_rotation` | `int` | `2` | `1` to `15` | Self-paced rotation count: number of random articles to scroll completely (`PauseStart` -> `Scrolling` -> `PauseEnd`) before rotation advances to the next screen. |
| `requests_per_day` | `int` | `10` | `1` to `100` | Total API requests per 24 hours (Free tier: max 100). |
| `force_refresh` | `bool` | `false` | `true`, `false` | Purge cached news of obsolete language and immediately query API. |
| `cache_ttl_min` | `int` | `30` | `5` to `120` | Cache refresh interval in minutes between background API queries. |
| `display_mode` | `Options` | `smooth_scroll` | `smooth_scroll`, `serpentine`, `vertical_crawl`, `static_paged` | Animation mode (smooth right-to-left ticker, arcade serpentine snake flow, vertical crawl, or static paged word wrapping). |
| `scroll_speed` | `int` | `3` | `1` to `10` | Ticker scrolling speed (1: Slow to 10: Fast). |
| `pause_start_sec` / `scroll_pause_start_ms` | `int` | `2` / `1200` | `0` to `10` s (`0` to `4000` ms) | Initial pause dwell time at article start before scrolling. |
| `pause_end_sec` / `scroll_pause_end_ms` | `int` | `2` / `1000` | `0` to `10` s (`0` to `4000` ms) | Pause dwell time at end of article before transitioning. |
| `article_duration_sec` | `int` | `12` | `5` to `60` | Fallback display duration per article in seconds. |
| `theme` | `Options` | `category_dynamic` | `category_dynamic`, `breaking_crimson`, `cyberpunk`, `monochrome_paper` | Color palette scheme. |
| `show_title` | `bool` | `false` | `true`, `false` | Include the article headline title in the scrolled text. |
| `show_description` | `bool` | `false` | `true`, `false` | Include the article summary/lead in the scrolled text. |
| `show_content` | `bool` | `true` | `true`, `false` | Include the article body content in the scrolled text (with automatic fallback to description/title if empty). |
| `show_category_badge` | `bool` | `true` | `true`, `false` | Display color-coded category pill (`[TECH]`, `[WORLD]`, etc.). |
| `show_source` | `bool` | `true` | `true`, `false` | Display news source name badge (`BBC News`, `Reuters`, etc.). |
| `show_time_ago` | `bool` | `true` | `true`, `false` | Display relative time badge (`5m ago`, `2h ago`). |
| `show_beacon` | `bool` | `true` | `true`, `false` | Display pulsing live broadcast beacon dot. |
| `show_progress_dots` | `bool` | `true` | `true`, `false` | Display article index dots (`● ○ ○ ○ ○`). |

### Engine: `weather`
| Field | Type | Default | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `api_key` | `String` | `""` | Free API key | Your OpenWeatherMap API Key (free tier at [openweathermap.org](https://home.openweathermap.org/users/sign_up)). |
| `city` | `String` | `""` | Text | City location (see formatting guide below). |
| `units` | `Options` | `metric` | `metric`, `imperial` | Temperature unit: `metric` for Celsius (°C) or `imperial` for Fahrenheit (°F). |
| `offset_x` | `int` | `0` | `-64` to `64` | Horizontal pixel shift. |
| `offset_y` | `int` | `0` | `-32` to `32` | Vertical pixel shift. |

On 128x32 and 256x64 panels the weather pages use the same layout as the ESP32 firmware: icon, then the day and condition (the long forms when they fit), then the temperatures right-aligned. In °F the high is shown above the low (US convention). Day labels follow `system.lang` (`en`, `fr`, `es`).

#### How to Format the `city` Field on OpenWeatherMap
OpenWeatherMap uses the ISO 3166 country code (and 2-letter state code for the US) to disambiguate locations:
* **International Locations:** Use `City,CountryCode` (e.g. `Paris,FR`, `London,GB`, `Tokyo,JP`, `Montreal,CA`).
* **United States Locations:** Use `City,StateCode,CountryCode` (e.g. `Tucson,AZ,US`, `Miami,FL,US`, `Dallas,TX,US`). Specifying only the city or omitting the country may return an incorrect city with the same name.
* **Where to Look:** Go to [openweathermap.org](https://openweathermap.org), search for your city. The top search result header and URL show the exact `City,State,Country` string recognized by the API.

### Engine: `system_info` (System Monitor)
| Field | Type | Default | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `theme` | `Options` | `0` | `0` to `2` | Visual layout style: `0` = HUD Bars & Gauges, `1` = Cyberpunk Neon, `2` = Compact Grid (2x2). |
| `show_cpu` | `bool` | `true` | `true`, `false` | Display processor load percentage & gauge. |
| `show_ram` | `bool` | `true` | `true`, `false` | Display memory usage percentage & gauge. |
| `show_temp` | `bool` | `true` | `true`, `false` | Display hardware SoC temperature (dynamic green/amber/red color scale). |
| `show_uptime` | `bool` | `true` | `true`, `false` | Display system Uptime counter. |
| `temp_unit` | `Options` | `C` | `C`, `F` | Temperature unit: Celsius (`C`) or Fahrenheit (`F`). |
| `offset_x` | `int` | `0` | `-64` to `64` | Horizontal pixel shift. |
| `offset_y` | `int` | `0` | `-32` to `32` | Vertical pixel shift. |

> [!NOTE]
> **M.U.G.E.N Fighter Overlay (`fighter`):** Unlike standalone rotation engines, `fighter` is a transverse overlay renderer composited on top of compatible idle screens (such as `clock` and `date`). It is configured globally in the `"system"` block (`idle_fighter_enabled`, `idle_fighter_interval`, `idle_fighter_speed` — see §4) and toggled per rotation slot via `rotation[].overlays.fighter` (see §7).

### Engine: `google_cast` (Google Home / Nest Audio)
| Field | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `device_ip` | `String` | `""` | Static IP of your Google Home / Nest Audio speaker. Leave blank for automatic mDNS local discovery. |
| `device_name` | `String` | `""` | Device name filter (e.g. `Living Room Speaker`) when automatically scanning your LAN. |
| `show_album_art` | `bool` | `true` | Downloads and displays album artwork on the left of the LED matrix. |
| `show_progress` | `bool` | `true` | Displays real-time playback progress bar at the bottom. |
| `show_visualizer` | `bool` | `true` | Displays animated audio frequency equalizer during playback. |
| `show_volume` | `bool` | `true` | Displays current Google Nest speaker volume percentage. |

### Engine: `spotify` (Spotify Official Player)
| Field | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `client_id` | `String` | `""` | Your Spotify Developer API Client ID. |
| `client_secret` | `String` | `""` | Your Spotify Developer API Client Secret (optional for PKCE). |
| `refresh_token` | `String` | `""` | Your Spotify OAuth2 Refresh Token for seamless ongoing playback sync. |
| `show_album_art` | `bool` | `true` | Downloads and displays full-color Spotify album artwork. |
| `show_progress` | `bool` | `true` | Displays track playback time progress bar at the bottom. |
| `show_visualizer` | `bool` | `true` | Displays animated audio equalizer during playback. |
| `show_volume` | `bool` | `true` | Displays active Spotify volume percentage. |

### Engine: `gifs`
| Field | Type | Default | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `playlists` | `String` (Multi) | `""` | Options from `/api/playlists` | Active GIF playlists or folders to cycle through (comma-separated). |

### Engine: `message`
| Field | Type | Default | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `text` | `String` | `ArcadeMatrix` | Text | The text banner or message to display. |
| `color` | `String` | `#ffffff` | Hex Color | Text color in `#RRGGBB` format. |
| `size` | `int` | `1` | `1` to `4` | Text scale multiplier. |
| `direction` | `Options` | `rtl` | `rtl`, `ltr`, `static` | Scroll direction or static display. |
| `speed` | `int` | `50` | `10` to `200` | Milliseconds per pixel update (lower is faster; ignored when static). |

### Engine: `marquee`
| Field | Type | Default | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `file_path` | `FileAsset` | `data/marquees/marquee.gif` | `.gif,.png,.jpg,.jpeg` | Path to marquee GIF or image file (also receives live Pixelcade/Recalbox/Batocera marquees over MQTT / HTTP). |
| `speed_multiplier` | `Float` | `1.0` | `0.25` to `3.0` | Animation playback speed factor. |
| `fit_mode` | `Options` | `fit` | `fit`, `center`, `stretch` | Display scaling mode. |

### Engine: `mqttdata` (MQTT Data)
| Field | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `topics` | `String` | `""` | Up to 6 full MQTT topics, comma separated. Each topic is one page. |

Everything else comes from each retained payload: its `type` (`value`, `table`, `graph`, `weather`), its time on screen (`seconds`) and its content. The screen stays up for one full cycle of its pages. The payload formats, limits and notices are in [MQTT_DATA_CONTRACT.md](MQTT_DATA_CONTRACT.md); Home Assistant blueprints are described in [HOME_ASSISTANT.md](HOME_ASSISTANT.md).

### Engines: `retro_games`, `movie_showcase`, `series_showcase`, `anime_showcase` (DMD Media Showcase)
> [!IMPORTANT]
> These 4 Showcase engines **only work with the companion [dmd_media_showcase](https://github.com/red77290/dmd_media_showcase) application installed on another machine on the same local network** (Raspberry Pi, NAS, Docker host, or PC). The server connection is configured once globally in the `showcase` root block of `config.json` (or via the unified **Showcase Server** card in the Web UI Settings tab).

| Root `showcase` Field | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `showcase.enabled` | `bool` | `false` | Master activation toggle (disabled by default so zero network requests are made if `dmd_media_showcase` is not installed). |
| `showcase.server_url` | `String` | `"http://dmdmediashowcase.local:8890"` | Pre-filled mDNS auto-discovery URL of your **[dmd_media_showcase](https://github.com/red77290/dmd_media_showcase)** server on the local network (verified by the live **`🟢 Communication OK`** indicator in the Web UI). |
| `showcase.transport_mode` | `Options` | `mqtt_udp` | Transport protocol (`mqtt_udp` for MQTT push + UDP streaming, or `http_poll` for direct HTTP polling). |
| `showcase.use_showcase_broker` | `bool` | `true` | Automatically connect to the MQTT broker running on the Showcase server host (`<server_ip>:1883`). |
| `showcase.mqtt_broker_source` | `Options` | `global` | Fallback MQTT broker source when `use_showcase_broker` is `false` (`global` uses `mqtt.broker`, `custom` uses `mqtt_custom_url`). |
| `showcase.mqtt_custom_url` | `String` | `""` | Custom MQTT broker URL (e.g. `mqtt://192.168.1.200:1883`) when `mqtt_broker_source` is `custom`. |

| Instance Field (`retro_games`, `movie_showcase`, `series_showcase`, `anime_showcase`) | Type | Default | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `text_scroll_speed` | `Options` | `4` | `1` to `10` (`slow`, `normal`, `fast`) | Reading speed for the organic full-synopsis scroll on the Info Card (`1` = Slow .. `10` = Turbo). |
| `jacket_panorama` | `bool` | `true` | `true`, `false` | Scroll the full-width cover/poster from top to bottom right after the info card. |
| `marquee_panorama` | `bool` | `false` | `true`, `false` | Scroll the arcade marquee or backdrop banner from top to bottom after the poster. |
| `snapshot_panorama_count` | `int` | `0` | `0` to `5` | Additional gameplay snapshots or stills to scroll from top to bottom (`0`..`5`). |
| `video_max_duration_s` | `int` | `8` | `2` to `120` | Maximum seconds for the streamed RGB565 video clip. |
| `video_fps` | `int` | `15` | `8` to `60` | Target frame rate for streamed RGB565 video clips. |
| `video_mode` | `Options` | `auto_action` | `auto_action`, `panorama`, `center_crop` | Video framing strategy powered by `dmd_gif_converter` (`auto_action` Let Me Handle It AI, `panorama` top-to-bottom scan, `center_crop`). |
| `show_poster` | `bool` | `true` | `true`, `false` | Display the fitted mini-jacket artwork on the card. |
| `show_synopsis` | `bool` | `true` | `true`, `false` | Scroll the complete localized plot summary on the card. |
| `show_video` | `bool` | `true` | `true`, `false` | Stream the RGB565 video clip if available (skipped if unavailable). |
| `color_boost` | `Options` | `auto` | `auto`, `pixel_art`, `anime`, `cinema` | HUB75 LED colorimetry profile (`auto` Let Me Handle It AI Smart Color Boost, or fixed domain preset `pixel_art`, `anime`, `cinema`). |
| `lang` | `Options` | `system` | `system,en,fr,es,de,it` | Metadata language for titles and synopses (`system` syncs with the DMD's configured UI language). |
| `systems` | `List` | `""` | Dynamic (`/api/v1/games/filters`) | (`retro_games` only) Filter by arcade/console systems (leave empty for all). |
| `genres` | `List` | `""` | Dynamic (`/api/v1/{domain}/filters`) | Filter by genres dynamically retrieved from the Showcase server referential. |
| `decades` | `List` | `""` | Dynamic (`/api/v1/{domain}/filters`) | Filter by release decades dynamically retrieved from the Showcase server referential. |
| `video_type` | `Options` | `any` / `opening` | `any`, `trailer`, `scene`, `opening`, `ending` | (`movie_showcase`, `series_showcase`, `anime_showcase`) Preferred video clip type. |

---

*Note: All schemas can also be queried dynamically in JSON format from the running system at `GET /api/engines`.*

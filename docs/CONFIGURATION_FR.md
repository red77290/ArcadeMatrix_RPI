🇬🇧 [English](CONFIGURATION.md) | 🇫🇷 Français | 🇪🇸 [Español](CONFIGURATION_ES.md)

# Configuration Détaillée (config.json)

Le système de configuration de la version Raspberry Pi a migré du vieux format plat `conf.ini` vers un format JSON structuré `config.json`. Ce fichier centralise le pilotage matériel, les réglages système, ainsi que la configuration fine et découplée de chaque moteur (`Engine`).

Ce fichier se trouve à la racine du projet (ou dans `/home/pi/ArcadeMatrix_RPi/config.json` sur le système déployé). 

---

## 1. Structure Globale

```json
{
  "matrix": { ... },
  "wifi": { ... },
  "mqtt": { ... },
  "system": { ... },
  "api_auth_enabled": false,
  "api_token": "9101d2ff5928c93107e537aa3c07a282",
  "instances": [ ... ],
  "rotation": [ ... ]
}
```

---

## 2. Le Bloc `"matrix"` (Matériel HUB75)

Ce bloc est passé directement à la librairie sous-jacente `rpi-rgb-led-matrix`. Ce sont les fondations matérielles.

| Clé | Type | Description |
| :--- | :--- | :--- |
| `width` | `int` | Largeur d'un seul panneau (ex: `64`). |
| `height` | `int` | Hauteur d'un seul panneau (ex: `32` ou `64`). |
| `chain_length` | `int` | Nombre de panneaux chaînés horizontalement (ex: `2` pour un 128x32 composé de deux dalles de 64x32). |
| `panel_type` | `String` | Type de panneau si besoin d'un multiplexage spécifique (vide par défaut). |
| `multiplexing` | `int` | Mappage de multiplexage (0 = régulier). Certaines matrices "outdoor" nécessitent des valeurs comme `1` ou `2`. |
| `mapping` | `String` | Chaînage logiciel (ex: `regular`, `U-mapper`). Utile si vos dalles sont chaînées verticalement. |
| `pwm_bits` | `int` | Profondeur des couleurs. Valeur par défaut `11`. Réduire cette valeur augmente le taux de rafraîchissement au détriment de la précision colorimétrique. |
| `pwm_lsb_nanoseconds` | `int` | Base de temps PWM (défaut `130`). Réduisez-le si la matrice scintille (flickering). |
| `power_limit_percent` | `int` | Limiteur logiciel de consommation électrique (`1` à `100`). Parfait pour soulager des alimentations un peu justes. |
| `force_single_buffer` | `bool` | Si `true`, le rendu est moins fluide (tearing possible) mais divise par 2 l'utilisation mémoire. |
| `rgb_sequence` | `String` | Séquence des couleurs. Laissez `RGB` ou mettez `RBG` / `BGR` si vos couleurs sont inversées. |
| `limit_refresh_rate_hz` | `int` | Cap de rafraichissement (0 = illimité). |
| `driver_chip` | `String` | Puce contrôleur. Laissez `SHIFTREG` sauf matrice spéciale (ex: `FM6126A`). |
| `clk_phase` | `bool` | Inverse le front d'horloge. Utile si vos pixels sont décalés (glitching). |
| `latch_blanking` | `int` | Temps de repos entre deux envois de ligne. |
| `row_address_mode` | `int` | `0` = régulier, `1` = AB, `2` = direct. |
| `disable_hardware_pulsing`| `bool`| Désactive l'impulsion GPIO matérielle. |
| `matrix_power` | `bool` | Interrupteur logiciel d'alimentation. |

---

## 3. Le Bloc `"system"` (Environnement et Veille)

| Clé | Type | Description |
| :--- | :--- | :--- |
| `timezone` | `String` | Chaîne POSIX (ex: `CET-1CEST,M3.5.0,M10.5.0/3`). Utilisée par le système pour régler le temps local. |
| `format_24h` | `bool` | Format de l'heure. `true` = 23:00, `false` = 11:00 PM. |
| `lang` | `String` | Langue du système (ex: `en`, `fr`). |
| `unit` | `String` | Unité de température (`c` pour Celsius, `f` pour Fahrenheit). |
| `temp_offset` | `float` | Étalonnage logiciel si vous utilisez un capteur de température embarqué. |
| `night_mode_enabled` | `bool` | Active l'extinction ou la réduction de luminosité automatique la nuit. |
| `turn_off_at` | `String` | Heure de début de la veille (ex: `"23:00"`). |
| `wake_up_at` | `String` | Heure de réveil (ex: `"07:00"`). |
| `night_brightness` | `int` | Luminosité de veille (`0` = matrice complètement éteinte). |

---

## 4. Le Bloc `"mqtt"` (Communication Frontend : Batocera/Recalbox)

C'est ici qu'on configure l'intégration avec votre borne d'arcade.

| Clé | Type | Description |
| :--- | :--- | :--- |
| `enabled` | `bool` | Active le client MQTT. |
| `broker` | `String` | L'IP de votre système Batocera/Recalbox, ou `127.0.0.1`. |
| `port` | `int` | Port MQTT (défaut `1883`). |
| `user` & `pass` | `String` | Identifiants si votre broker est sécurisé. |
| `device_name` | `String` | Le nom d'ArcadeMatrix sur le réseau MQTT. |
| `topic_batocera` | `String` | Le topic écouté pour scraper le jeu vidéo en cours sur Batocera. |
| `topic_recalbox` | `String` | Le topic écouté pour scraper Recalbox. |

---

## 5. Le Bloc `"wifi"`

| Clé | Type | Description |
| :--- | :--- | :--- |
| `ssid` | `String` | Le nom de votre réseau Wi-Fi. |
| `password` | `String` | La clé Wi-Fi. |
| `hostname` | `String` | Le nom mDNS pour accéder à l'interface via `http://hostname.local`. |
| `configured` | `bool` | Vaut `true` une fois que l'utilisateur a paramétré son Wi-Fi. |
| `disable_internal` | `bool` | Désactiver la gestion interne Wi-Fi (si gérée via raspi-config). |

---

## 6. Moteurs : `"instances"` & `"rotation"`

Depuis le Sprint 13, les modules sont entièrement découplés.
L'utilisateur crée des `"instances"`, et référence ces instances dans la `"rotation"`.

### `"instances"`
C'est un tableau contenant la configuration de chaque bloc. Un utilisateur peut avoir 3 horloges différentes si désiré.

```json
{
  "instance_id": "mon_horloge_cyberpunk",
  "engine_id": "clock",
  "config": {
    "theme": "18",
    "format": "%H:%M"
  }
}
```
* `instance_id` : Nom unique défini par l'UI.
* `engine_id` : L'identifiant du moteur (déclaré via `EngineMetadata::id` en Rust).
* `config` : Un dictionnaire purement dynamique clé/valeur, respectant le `ConfigSchema` du moteur.

### `"rotation"`
Définit la Playlist. L'ordre du tableau est l'ordre d'affichage.

```json
{
  "instance_id": "mon_horloge_cyberpunk",
  "duration_sec": 30
}
```
Ici, ArcadeMatrix affichera l'instance `mon_horloge_cyberpunk` pendant 30 secondes avant de passer au module suivant.

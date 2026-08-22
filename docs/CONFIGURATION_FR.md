🇬🇧 [English](CONFIGURATION.md) | 🇫🇷 Français | 🇪🇸 [Español](CONFIGURATION_ES.md)

# Configuration détaillée (config.json) - Raspberry Pi

Le système de configuration repose exclusivement sur un unique fichier `config.json` situé à la racine du projet (ou sur la partition **DATA** de l'image précompilée). Il gère le pilote matériel, le réseau, l'intégration MQTT, le comportement système, la sécurité de l'API et la logique découplée de chaque moteur (« instances »).

> L'ancien format `conf.ini` a été entièrement supprimé. `config.json` est désormais la **source unique de vérité**. Au démarrage, le fichier est validé et autoréparé (voir §8), ce qui rend sûr un fichier partiel ou modifié à la main : les clés manquantes sont recréées avec leurs valeurs par défaut.

---

## 1. Structure globale

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

## 2. Le bloc `"matrix"` (pilote matériel)

Ce bloc configure les paramètres DMA de la bibliothèque hzeller `rpi-rgb-led-matrix`. Modifier une valeur matérielle déclenche un redémarrage automatique afin que les nouveaux réglages du pilote soient appliqués.

| Clé | Type | Description |
| :--- | :--- | :--- |
| `width` | `int` | Largeur d'un seul panneau (ex. `64`). |
| `height` | `int` | Hauteur d'un seul panneau (ex. `32`). |
| `chain_length` | `int` | Nombre de panneaux chaînés horizontalement. |
| `mapping` | `String` | Câblage/mapping GPIO (`regular`, `adafruit-hat`, `adafruit-hat-pwm`, ...). |
| `driver_chip` | `String` | Puce contrôleur (`SHIFTREG`, `FM6126A`). |
| `rgb_sequence` | `String` | Ordre des couleurs (`RGB`, `RBG`, `BGR`, ...). Corrigez ici les couleurs inversées. |
| `slowdown` | `int` | Ralentissement GPIO (`1`–`4`). Augmentez-le sur Pi 3/4 si vous voyez des artefacts. |
| `pwm_bits` | `int` | Profondeur des couleurs. Défaut `11` ; baissez à `8` pour économiser du CPU. |
| `pwm_lsb_nanoseconds` | `int` | Réglage de la largeur d'impulsion LSB (avancé). |
| `disable_hardware_pulsing` | `bool` | Mettre à `true` pour éviter que le DMA n'affame le Wi-Fi interne (léger scintillement). |
| `limit_refresh_rate_hz` | `int` | Limite le taux de rafraîchissement (`0` = illimité). |
| `row_address_mode` | `int` | Type d'adressage des lignes pour panneaux exotiques (`0` par défaut). |
| `multiplexing` | `int` | Type de multiplexage du panneau (`0` par défaut). |
| `panel_type` | `String` | Chaîne d'initialisation optionnelle du panneau (ex. `FM6126A`), généralement vide. |

> La luminosité de jour en direct **n'est pas** stockée dans ce bloc ; elle est contrôlée à l'exécution depuis la Web UI (curseur du Dashboard → `POST /api/system { "brightness_limit": 0-100 }`). La luminosité de nuit se trouve dans le bloc `system` (§4).

---

## 3. Le bloc `"wifi"`

| Clé | Type | Description |
| :--- | :--- | :--- |
| `ssid` | `String` | Le nom de votre réseau Wi-Fi. |
| `password` | `String` | La clé WPA2. |
| `hostname` | `String` | Nom d'hôte de l'appareil annoncé sur le réseau. |
| `configured` | `bool` | Mettre à `false` pour forcer une tentative de (re)connexion au prochain démarrage. Repasse automatiquement à `true` en cas de succès. |
| `disable_internal` | `bool` | Si vous utilisez un dongle USB externe, désactive le Wi-Fi interne du Pi (modifier ceci déclenche un redémarrage). |

Vous pouvez aussi pousser des identifiants à l'exécution avec `POST /api/wifi { "ssid": "...", "password": "..." }`, ce qui définit `configured=false` et relance le provisionnement réseau.

---

## 4. Le bloc `"system"` (environnement et veille)

| Clé | Type | Description |
| :--- | :--- | :--- |
| `timezone` | `String` | Chaîne POSIX (ex. `CET-1CEST,M3.5.0,M10.5.0/3`). |
| `format_24h` | `bool` | Format de l'heure. `true` = 23:00, `false` = 11:00 PM. |
| `lang` | `String` | Langue du système (ex. `en`, `fr`, `es`). |
| `unit` | `String` | Unité de mesure pour la météo (`metric` / `imperial`). |
| `temp_offset` | `float` | Décalage d'étalonnage appliqué à la température rapportée. |
| `night_mode_enabled` | `bool` | Active l'extinction automatique / la réduction de luminosité la nuit. |
| `turn_off_at` | `String` | Heure de début de la veille (ex. `"23:00"`). |
| `wake_up_at` | `String` | Heure de réveil (ex. `"07:00"`). |
| `night_brightness` | `int` | Luminosité de veille (`0` = matrice complètement éteinte). |
| `day_brightness` | `int` | Luminosité de jour en direct (`0`–`100`). Réglée via le curseur du tableau de bord et conservée après un redémarrage. |
| `idle_fighter_enabled` | `bool` | Interrupteur principal de l'overlay Combattant décoratif superposé aux écrans de rotation en veille (activation par écran via chaque entrée de rotation). |
| `idle_fighter_interval` | `int` | Secondes entre deux animations de combat (minimum `1`). |

---

## 5. Le bloc `"mqtt"` (marquees Recalbox / Batocera)

| Clé | Type | Description |
| :--- | :--- | :--- |
| `enabled` | `bool` | Active l'écouteur MQTT pour les marquees de style Pixelcade. |
| `broker` | `String` | IP/hôte du broker (généralement le Pi lui-même). |
| `port` | `int` | Port du broker (défaut `1883`). |
| `user` | `String` | Nom d'utilisateur du broker (optionnel). |
| `pass` | `String` | Mot de passe du broker (optionnel). |
| `device_name` | `String` | Identifiant publié par cet appareil. |
| `topic_batocera` | `String` | Topic écouté pour les événements de jeux Batocera. |
| `topic_recalbox` | `String` | Topic écouté pour les événements de jeux Recalbox. |

Le démon de synchronisation peut être installé sur la console via SSH depuis la Web UI (`POST /api/mqtt/install`) et ses journaux récupérés avec `POST /api/mqtt/logs`.

---

## 6. Sécurité de l'API (`api_auth_enabled` / `api_token`)

Ces deux clés de premier niveau sécurisent les endpoints d'écriture et d'administration.

| Clé | Type | Description |
| :--- | :--- | :--- |
| `api_auth_enabled` | `bool` | Si `true`, les endpoints sensibles exigent que l'en-tête `X-API-Token` corresponde à `api_token`. |
| `api_token` | `String` | Jeton secret (généré automatiquement au premier démarrage). Envoyé par la Web UI comme `X-API-Token`. |

Désactivé par défaut pour que la Web UI intégrée fonctionne immédiatement. Activez-le si l'appareil est accessible au-delà d'un LAN de confiance.

---

## 7. Moteurs : `"instances"` & `"rotation"`

L'architecture découplée permet de créer plusieurs copies indépendantes, configurées différemment, du même Engine.

### `"instances"`
Un tableau contenant la configuration de chaque bloc logique.

```json
{
  "instance_id": "crypto_main",
  "engine_id": "crypto",
  "config": {
    "symbols": "BTC,ETH,SOL"
  }
}
```
* `instance_id` : Nom unique de ce bloc.
* `engine_id` : Identifiant interne de l'Engine Rust (doit être un moteur enregistré — voir §9).
* `config` : Une map plate de valeurs `String` propres au moteur, validée par rapport à son `ConfigSchema`.

Modifier une instance via la Web UI (`POST /api/instances`) est appliqué **à chaud, sans redémarrage** : le runtime appelle `on_config_changed()` du moteur à la frame suivante (hot-reload Lazy-Once). Ajouter ou supprimer une instance réinitialise proprement la rotation.

### `"rotation"`
Définit l'ordre d'affichage et la durée de chaque créneau.

```json
{
  "instance_id": "crypto_main",
  "duration_sec": 30
}
```
Seules les instances listées ici sont initialisées, ce qui économise de la mémoire pour les fonctionnalités inutilisées. La rotation est modifiable depuis le panneau **Rotation** de la Web UI (`GET`/`POST /api/rotation`).

> Note : `duration_sec` appartient à l'entrée de **rotation**, pas au `config` de l'instance.

---

## 8. Validation autoréparatrice

À chaque démarrage **et** à chaque écriture via `POST /api/instances`, le `ConfigSanitizer` réconcilie chaque instance avec le `ConfigSchema` de son moteur :

* **Clé manquante** → le `default_value` du schéma est injecté.
* **Integer / Float** → analysé puis, si hors `min`/`max`, borné ou réinitialisé à la valeur par défaut (selon le `validation_policy` du champ).
* **Boolean** → normalisé (`true/1/yes/on` → `true`, `false/0/no/off` → `false`) ; une valeur impossible à analyser revient à la valeur par défaut.
* **Options** → la valeur doit faire partie des options déclarées (liste séparée par des virgules pour la sélection multiple) ; sinon elle revient à la valeur par défaut.
* **Clés obsolètes** → les clés qui ne sont plus présentes dans le schéma (ex. après une OTA qui a renommé un champ) sont supprimées.

Le résultat est enregistré atomiquement, donc une OTA qui ajoute un nouveau champ le renseigne automatiquement sans intervention utilisateur.

---

## 9. Configurations des moteurs

Chaque moteur expose ses propres champs via son `ConfigSchema` (consultable à `GET /api/engines`, qui alimente la Web UI dynamique). Les moteurs les plus courants :

### Moteur : `clock`
| Champ | Type | Défaut | Description |
| :--- | :--- | :--- | :--- |
| `theme` | `int` | `0` | Index du thème d'horloge animé. |
| `format` | `String` | `%H:%M:%S` | Format d'heure strftime. |
| `font` | `String` | `PressStart2P.ttf` | Fichier de police depuis `/fonts/`. |
| `size` | `int` | `2` | Facteur de mise à l'échelle de la police. |
| `color_1` | `String` | `#FFFFFF` | Couleur hex principale (début du dégradé sur le thème Custom). |
| `color_2` | `String` | `#FFFFFF` | Couleur hex secondaire (fin du dégradé sur le thème Custom). |
| `offset_x` | `int` | `0` | Décalage horizontal en pixels. |
| `offset_y` | `int` | `0` | Décalage vertical en pixels. |

### Moteur : `date`
| Champ | Type | Défaut | Description |
| :--- | :--- | :--- | :--- |
| `theme` | `int` | `0` | Index du thème de date. |
| `format` | `String` | `%d/%m` | Format de date strftime. |
| `font` | `String` | `PressStart2P.ttf` | Fichier de police depuis `/fonts/`. |
| `size` | `int` | `2` | Facteur de mise à l'échelle de la police. |
| `color_1` | `String` | `#FFFFFF` | Couleur hex principale. |
| `color_2` | `String` | `#FFFFFF` | Couleur hex secondaire. |
| `offset_x` | `int` | `0` | Décalage horizontal en pixels. |
| `offset_y` | `int` | `0` | Décalage vertical en pixels. |

Les autres moteurs enregistrés (`weather`, `crypto`, `stock`, `gif`, `message`, `marquee`, `spotify`) exposent leurs propres champs de la même manière — consultez `GET /api/engines` pour le schéma de référence, toujours à jour.

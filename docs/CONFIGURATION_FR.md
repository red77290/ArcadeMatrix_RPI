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
Définit l'ordre d'affichage, la durée de chaque créneau et l'activation des overlays transversaux.

```json
{
  "instance_id": "crypto_main",
  "duration_sec": 30,
  "overlays": {
    "fighter": true
  }
}
```
* `instance_id` : Nom de l'instance ciblée.
* `duration_sec` : Durée d'affichage en secondes (ou quota de lecture pour les moteurs autonomes comme GIF).
* `overlays.fighter` : (`bool`) Interrupteur granulaire pour l'overlay décoratif de combat M.U.G.E.N sur cet écran précis.

Seules les instances listées ici sont initialisées, ce qui économise de la mémoire pour les fonctionnalités inutilisées. La rotation est modifiable depuis le panneau **Rotation** de la Web UI (`GET`/`POST /api/rotation`).

> **Persistance Lisible** : Lors de son enregistrement sur disque (`config.json`), le fichier est systématiquement écrit formaté avec des indentations claires pour permettre une relecture et modification manuelle sans risque.

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
| `theme` | `int` | `0` | Index du thème d'horloge : `0` = Digital Standard, `1` = Flip Clock, `2` = Cyberpunk, `3` = Word Clock, `4` = Binary Clock, `5` = Pac-Man, `6` = Tetris, `7` = Slot Machine, `8` = Versus (M.U.G.E.N), `9` = Pong, `10` = Matrix Rain (Katakana). |
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

### Moteur : `crypto`
| Champ | Type | Défaut | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `symbols` | `String` | `BTC,ETH` | Séparés par virgule | Symboles cryptos à surveiller (CoinGecko / Binance). |
| `currency` | `Options` | `USD` | `USD`, `EUR`, `GBP`, `JPY` | Devise de cotation et symbole monétaire (`$`, `€`, `£`, `¥`). |
| `show_chart` | `bool` | `true` | `true`, `false` | Afficher la courbe sparkline historique. |
| `chart_timeframe` | `Options` | `daily` | `hourly`, `daily`, `weekly`, `monthly` | Échelle de temps pour l'historique des cours. |
| `page_seconds` | `int` | `5` | `3` à `30` | Secondes d'affichage par page avant alternance. |
| `cache_ttl_min` | `int` | `1` | `1` à `60` | Minutes de rétention du cache de cotation. |

### Moteur : `stock`
| Champ | Type | Défaut | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `symbols` | `String` | `AAPL,NVDA,TSLA` | Séparés par virgule | Tickers boursiers à surveiller (Yahoo Finance). |
| `currency` | `Options` | `USD` | `USD`, `EUR`, `GBP`, `JPY` | Devise de cotation et symbole monétaire (`$`, `€`, `£`, `¥`). |
| `show_chart` | `bool` | `true` | `true`, `false` | Afficher la courbe sparkline historique. |
| `chart_timeframe` | `Options` | `daily` | `hourly`, `daily`, `weekly`, `monthly` | Échelle de temps pour l'historique des cours. |
| `page_seconds` | `int` | `5` | `3` à `30` | Secondes d'affichage par page avant alternance. |
### Moteur : `gnews` (Actualités en Direct & Ticker GNews)
| Champ | Type | Défaut | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `api_key` | `String` | `""` | Clé valide | Clé API GNews.io (optionnelle ; utilise des articles de démo si vide). |
| `category` | `Options` | `technology` | `general`, `world`, `nation`, `business`, `technology`, `entertainment`, `sports`, `science`, `health` | Catégorie thématique principale. |
| `keywords` | `String` | `""` | Texte / Requête | Mots-clés de recherche ou tags personnalisés (ex. `ai OR arcade`). |
| `lang` | `Options` | `auto` | `auto`, `en`, `fr`, `es`, `de`, `it`, `pt`, `nl`, `ru`, `zh`, `ja` | Langue des articles (`auto` synchronise avec la langue système). |
| `country` | `Options` | `auto` | `auto`, `us`, `fr`, `gb`, `es`, `de`, `ca`, `it`, `jp`, `au`, `br`, `in` | Édition régionale du pays. |
| `max_articles` | `int` | `5` | `3` à `15` | Nombre maximal d'articles mis en cache et alternés par cycle. |
| `cache_ttl_min` | `int` | `30` | `5` à `120` | Intervalle de rafraîchissement réseau du cache en minutes. |
| `display_mode` | `Options` | `smooth_scroll` | `smooth_scroll`, `serpentine`, `vertical_crawl`, `static_paged` | Mode d'animation (défilement fluide droite-gauche, flux serpentin arcade, défilement vertical ou pagination statique). |
| `scroll_speed` | `int` | `3` | `1` à `5` | Multiplicateur de vitesse de défilement (1 : Lent ~18 px/s à 5 : Turbo ~60 px/s). |
| `scroll_pause_start_ms` | `int` | `1200` | `0` à `4000` | Temps de pause fixe (ms) au début du titre avant le défilement. |
| `scroll_pause_end_ms` | `int` | `1000` | `0` à `4000` | Temps de pause fixe (ms) à la fin du titre avant la transition. |
| `article_duration_sec` | `int` | `12` | `5` à `60` | Durée d'affichage par article en secondes. |
| `theme` | `Options` | `category_dynamic` | `category_dynamic`, `breaking_crimson`, `cyberpunk`, `monochrome_paper` | Schéma de couleurs visuel. |
| `show_category_badge` | `bool` | `true` | `true`, `false` | Affiche le badge thématique coloré (`[TECH]`, `[WORLD]`, etc.). |
| `show_source` | `bool` | `true` | `true`, `false` | Affiche le nom de la source d'actualités (`BBC News`, `Reuters`, etc.). |
| `show_time_ago` | `bool` | `true` | `true`, `false` | Affiche l'ancienneté relative (`5m ago`, `2h ago`). |
| `show_beacon` | `bool` | `true` | `true`, `false` | Affiche le témoin lumineux de direct clignotant. |
| `show_progress_dots` | `bool` | `true` | `true`, `false` | Affiche les points indicateurs de progression (`● ○ ○ ○ ○`). |

### Moteur : `weather`
| Champ | Type | Défaut | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `api_key` | `String` | `""` | Clé API gratuite | Clé API OpenWeatherMap (création gratuite sur [openweathermap.org](https://home.openweathermap.org/users/sign_up)). |
| `city` | `String` | `""` | Texte | Ville pour les prévisions météo (voir guide ci-dessous). |
| `units` | `Options` | `metric` | `metric`, `imperial` | Unité de température : `metric` pour Celsius (°C) ou `imperial` pour Fahrenheit (°F). |
| `lang` | `Options` | `en` | `en`, `fr`, `es` | Langue d'affichage des jours (TODAY / AUJ. / HOY). |
| `offset_x` | `int` | `0` | `-64` à `64` | Décalage horizontal en pixels. |
| `offset_y` | `int` | `0` | `-32` à `32` | Décalage vertical en pixels. |

#### Formatage du champ `city` sur OpenWeatherMap
OpenWeatherMap utilise le code pays ISO 3166 (et le code d'état à 2 lettres pour les États-Unis) :
* **Emplacements Internationaux :** Utilisez `Ville,CodePays` (ex. `Paris,FR`, `London,GB`, `Tokyo,JP`, `Montreal,CA`).
* **Emplacements aux États-Unis :** Utilisez `Ville,CodeEtat,CodePays` (ex. `Tucson,AZ,US`, `Miami,FL,US`, `Dallas,TX,US`).
* **Où trouver le nom exact :** Rendez-vous sur [openweathermap.org](https://openweathermap.org) et recherchez votre ville.

### Moteur : `sysinfo` (Moniteur Système)
| Champ | Type | Défaut | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `theme` | `int` | `0` | `0` à `2` | Thème visuel : `0` = Jauges horizontales colorées, `1` = Grille compacte 4 blocs, `2` = Terminal Retro. |
| `show_cpu` | `bool` | `true` | `true`, `false` | Affiche l'utilisation instantanée du processeur (CPU %). |
| `show_ram` | `bool` | `true` | `true`, `false` | Affiche l'utilisation de la mémoire vive (RAM %). |
| `show_temp` | `bool` | `true` | `true`, `false` | Affiche la température matérielle du SoC (vert/orange/rouge dynamique). |
| `show_uptime` | `bool` | `true` | `true`, `false` | Affiche le temps d'activité (Uptime) en heures/jours. |
| `temp_unit` | `Options` | `C` | `C`, `F` | Unité de température : Celsius (`C`) ou Fahrenheit (`F`). |
| `offset_x` | `int` | `0` | `-64` à `64` | Décalage horizontal en pixels. |
| `offset_y` | `int` | `0` | `-32` à `32` | Décalage vertical en pixels. |

### Moteur : `fighter` (Combat M.U.G.E.N)
| Champ | Type | Défaut | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `mode` | `Options` | `match` | `match`, `showcase` | Mode de combat : `match` (duel complet avec K.O. et victoire) ou `showcase` (démonstration continue de coups). |
| `fighter_1` | `String` | `""` | Nom du dossier | Combattant P1 (laisser vide pour sélection aléatoire dans la liste). |
| `fighter_2` | `String` | `""` | Nom du dossier | Combattant P2 (laisser vide pour sélection aléatoire dans la liste). |
| `show_hud` | `bool` | `true` | `true`, `false` | Affiche les barres de vie rétro (HP), les jauges de Super et les noms des combattants. |
| `match_duration` | `int` | `30` | `10` à `120` | Durée maximale d'un round en secondes avant time-out. |

### Moteur : `dashboard` (Smart Dashboard Hub)
| Champ | Type | Défaut | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `clock_mode` | `Options` | `0` | `0` (Digital), `1` (Cadran), `2` (Minimal) | Style de rendu de l'horloge |
| `theme` | `Options` | `0` | `0` (Cyberpunk), `1` (Amber HUD), `2` (Minimalist), `3` (Matrix) | Palette de couleurs des widgets |
| `show_clock` | `bool` | `true` | `true`, `false` | Affiche l'horloge principale |
| `show_world_clock` | `bool` | `true` | `true`, `false` | Affiche les fuseaux horaires mondiaux secondaires |
| `world_clocks` | `String` | `NYC,TYO,LON` | Codes aéroports | Fuseaux horaires séparés par virgules (ex. `NYC,TYO,LON,PAR,SFO`) |
| `show_weather` | `bool` | `true` | `true`, `false` | Affiche le widget météo en direct |
| `weather_city` | `String` | `Paris,FR` | Ville | Requête de ville pour OpenWeatherMap |
| `show_markets` | `bool` | `true` | `true`, `false` | Affiche le ticker défilant crypto & bourse |
| `tracked_markets` | `String` | `BTC,ETH,NVDA,AAPL` | Symboles | Symboles de marché séparés par virgules |
| `show_sysinfo` | `bool` | `true` | `true`, `false` | Affiche les compteurs CPU % et RAM % |
| `show_date` | `bool` | `true` | `true`, `false` | Affiche la date courante |
| `show_seconds` | `bool` | `true` | `true`, `false` | Affiche le compteur de secondes |

### Moteur : `google_cast` (Google Home / Nest Audio)
| Champ | Type | Défaut | Description |
| :--- | :--- | :--- | :--- |
| `device_ip` | `String` | `""` | IP statique de votre enceinte Google Home / Nest Audio. Laissez vide pour la découverte mDNS automatique sur le réseau local. |
| `device_name` | `String` | `""` | Filtre sur le nom de l'appareil (ex. `Salon`) lors de la détection automatique sur le LAN. |
| `show_album_art` | `bool` | `true` | Télécharge et affiche la pochette de l'album à gauche de la matrice. |
| `show_progress` | `bool` | `true` | Affiche la barre de progression temporelle de la lecture en bas de l'écran. |
| `show_visualizer` | `bool` | `true` | Affiche l'égaliseur de fréquences animé quand la musique est en lecture. |
| `show_volume` | `bool` | `true` | Affiche le niveau de volume actuel de l'enceinte Google Nest. |

### Moteur : `spotify` (Lecteur Officiel Spotify)
| Champ | Type | Défaut | Description |
| :--- | :--- | :--- | :--- |
| `client_id` | `String` | `""` | Votre Client ID Spotify Developer API. |
| `client_secret` | `String` | `""` | Votre Client Secret Spotify Developer API (optionnel pour PKCE). |
| `refresh_token` | `String` | `""` | Votre Refresh Token OAuth2 Spotify pour la synchronisation continue de la lecture. |
| `show_album_art` | `bool` | `true` | Télécharge et affiche la pochette d'album Spotify sur la matrice. |
| `show_progress` | `bool` | `true` | Affiche la barre de progression temporelle du morceau en bas de l'écran. |
| `show_visualizer` | `bool` | `true` | Affiche l'égaliseur audio animé quand la musique est en lecture. |
| `show_volume` | `bool` | `true` | Affiche le pourcentage de volume de lecture Spotify actif. |

### Moteur : `gifs`
| Champ | Type | Défaut | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `playlists` | `String` (Multi) | `""` | Options depuis `/api/playlists` | Dossiers / Playlists de GIFs actives à faire défiler (séparées par virgule). |

### Moteur : `message`
| Champ | Type | Défaut | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `text` | `String` | `Hello` | Texte | Texte du message ou de la bannière à afficher. |
| `color` | `String` | `#ffffff` | Couleur Hex | Couleur du texte au format `#RRGGBB`. |
| `size` | `int` | `1` | `1` à `4` | Multiplicateur d'échelle de la police. |
| `direction` | `Options` | `left` | `left`, `none` | Sens de défilement (`left` pour défilement vers la gauche, `none` pour texte statique centré). |
| `speed` | `int` | `50` | `10` à `200` | Millisecondes par pixel de défilement (plus bas = plus rapide ; ignoré en statique). |
| `font` | `String` | `Default` | Dynamique | Police de caractères depuis `/fonts/`. |

### Moteur : `marquee`
| Champ | Type | Défaut | Description |
| :--- | :--- | :--- | :--- |
| *(auto)* | `None` | — | Moteur interne de synchronisation des marquees Pixelcade / Recalbox / Batocera reçus via MQTT ou Webhook. |

---

*Note : L'ensemble des schémas de configuration peut également être interrogé en direct au format JSON via `GET /api/engines`.*

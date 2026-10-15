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
  "data_mqtt": { ... },
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
| `parallel` | `int` | Nombre de chaînes parallèles (`1`–`3`, défaut `1`). |
| `power_limit_percent` | `int` | Limite globale de puissance / luminosité de la matrice en pourcentage (`1`–`100`, défaut `40`). |
| `force_single_buffer` | `bool` | Force le mode simple buffer lorsque défini à `true` (défaut `false`). |
| `color_depth` | `int` | Profondeur de couleur logique en bits (`24` par défaut). |
| `mapping` | `String` | Câblage/mapping GPIO (`regular`, `adafruit-hat`, `adafruit-hat-pwm`, ...). |
| `driver_chip` | `String` | Puce contrôleur (`SHIFTREG`, `FM6126A`). |
| `rgb_sequence` | `String` | Ordre des couleurs (`RGB`, `RBG`, `BGR`, ...). Corrigez ici les couleurs inversées. |
| `clk_phase` | `bool` | Inversion de phase d'horloge (`false` par défaut). |
| `latch_blanking` | `int` | Cycles de latch blanking (`0`–`8`, défaut `1`). |
| `matrix_power` | `bool` | État d'alimentation logicielle de la matrice LED (`true` par défaut). |
| `slowdown` | `int` | Ralentissement GPIO (`1`–`4`). Augmentez-le sur Pi 3/4 si vous voyez des artefacts. |
| `pwm_bits` | `int` | Profondeur des couleurs. Défaut `11` ; baissez à `8` pour économiser du CPU. |
| `pwm_lsb_nanoseconds` | `int` | Réglage de la largeur d'impulsion LSB (avancé, défaut `130`). |
| `disable_hardware_pulsing` | `bool` | Mettre à `true` pour éviter que le DMA n'affame le Wi-Fi interne (léger scintillement). |
| `limit_refresh_rate_hz` | `int` | Limite le taux de rafraîchissement (`0` = illimité). |
| `row_address_mode` | `int` | Type d'adressage des lignes pour panneaux exotiques (`0` par défaut). |
| `multiplexing` | `int` | Type de multiplexage du panneau (`0` par défaut). |
| `panel_type` | `String` | Chaîne d'initialisation optionnelle du panneau (ex. `FM6126A`), généralement vide. |
| `rotation` | `int` | Orientation physique de l'écran en degrés (`0`, `90`, `180`, `270`, défaut `0`). |
| `transition_effect` | `String` | Effet de transition visuelle lors des changements d'orientation (`none`, `vortex`, `fade`, `slide`, ...). Défaut `none`. |
| `transition_duration_ms` | `int` | Durée de la transition d'orientation en millisecondes (`100`–`3000`, défaut `500`). |
| `render_pipeline` | `String` | Pipeline de rendu matériel & stratégie de bufferisation : `auto` (sélection automatique par défaut), `canvas_single` (**Recommandé**, canvas unique hors-écran avec suivi des zones modifiées 'dirty bounds' protégeant le bus SDIO Wi-Fi contre la saturation), `canvas_double` (double buffer avec canvas), `direct_double` (double buffer direct zéro-copie via pilote C), `direct_single` (buffer direct simple). Par défaut `canvas_single`. |
| `slot_transition` | `String` | Effet joué lorsque la rotation passe à l'écran suivant (`wipe`, `curtain`, `shutter`, `dissolve`, `checker`, `matrix`, `slide`, `zoom`, `glitch`, `random`, `none`). Par défaut `none`. |
| `slot_transition_duration_ms` | `int` | Durée de la transition entre écrans, en millisecondes (`100`-`3000`, par défaut `500`). |

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
| `temp_unit` | `String` | Unité de température globale (`"C"` pour Celsius ou `"F"` pour Fahrenheit, défaut `"C"`). |
| `night_mode_enabled` | `bool` | Active l'extinction automatique / la réduction de luminosité la nuit. |
| `turn_off_at` | `String` | Heure de début de la veille (ex. `"23:00"`). |
| `wake_up_at` | `String` | Heure de réveil (ex. `"07:00"`). |
| `night_brightness` | `int` | Luminosité de veille (`0` = matrice complètement éteinte). |
| `day_brightness` | `int` | Luminosité de jour en direct (`0`–`100`). Réglée via le curseur du tableau de bord et conservée après un redémarrage. |
| `idle_fighter_enabled` | `bool` | Interrupteur principal de l'overlay Combattant décoratif superposé aux écrans de rotation en veille (activation par écran via chaque entrée de rotation). |
| `idle_fighter_interval` | `int` | Secondes entre deux animations de combat (minimum `1`, défaut `10`). |
| `idle_fighter_speed` | `int` | Vitesse d'animation en pourcentage pour l'overlay Combattant (`25`–`200`, défaut `100`). |

---

## 5. Le bloc `"mqtt"` (marquees Recalbox / Batocera / RetroPie)

| Clé | Type | Description |
| :--- | :--- | :--- |
| `enabled` | `bool` | Active l'écouteur MQTT pour les marquees de style Pixelcade. |
| `broker` | `String` | IP/hôte du broker (généralement le Pi lui-même). |
| `port` | `int` | Port du broker (défaut `1883`). |
| `user` | `String` | Nom d'utilisateur du broker (optionnel). |
| `pass` | `String` | Mot de passe du broker (optionnel). |
| `device_name` | `String` | Identifiant publié par cet appareil. |
| `allow_overlay` | `bool` | Autorise les overlays décoratifs (ex. Street Fighter) sur les écrans MQTT/marquee (défaut `false`). |
| *(auto-abonnement)* | `system/playing/#` | S'abonne automatiquement à tous les systèmes rétro pris en charge : `system/playing/recalbox`, `system/playing/batocera`, `system/playing/retropie`. |

Le démon de synchronisation peut être installé sur la console (Recalbox, Batocera, RetroPie) via SSH depuis la Web UI (`POST /api/mqtt/install`) avec sélection ou détection automatique de l'OS cible, et ses journaux récupérés avec `POST /api/mqtt/logs`.

> [!NOTE]
> Pour Batocera, la version **v33 ou supérieure** est requise pour le changement dynamique de marquee pendant la navigation (hooks `game-selected` et `system-selected`). Batocera v32 et versions antérieures ne déclenchent que les événements de lancement/arrêt de jeu. Recalbox est supporté sur toutes ses versions.

---

## 5b. Le bloc `"data_mqtt"` (broker MQTT Data)

Identifiants du broker pour le moteur `mqttdata`. Cette connexion est distincte du bloc `"mqtt"` ci-dessus (marquees / contrôle, qui peut prendre la main sur l'affichage). Rien ne tourne en arrière-plan : le moteur ne se connecte que pendant que son écran est affiché et se déconnecte quand il quitte la rotation.

| Clé | Type | Description |
| :--- | :--- | :--- |
| `broker` | `String` | Hôte ou IP du broker. |
| `port` | `int` | Port du broker (défaut `1883`). |
| `user` | `String` | Utilisateur (optionnel). |
| `pass` | `String` | Mot de passe (optionnel). |

L'identifiant client MQTT est `<hostname>-data`. Les changements s'appliquent à la prochaine activation du moteur (sans redémarrage).

## 6. Sécurité de l'API (`api_auth_enabled` / `api_token`)

Ces deux clés de premier niveau sécurisent les endpoints d'écriture et d'administration.

| Clé | Type | Description |
| :--- | :--- | :--- |
| `api_auth_enabled` | `bool` | Si `true`, les endpoints sensibles exigent que l'en-tête `X-API-Token` corresponde à `api_token`. |
| `api_token` | `String` | Jeton secret (généré automatiquement au premier démarrage). Envoyé par la Web UI comme `X-API-Token`. |

Désactivé par défaut pour que la Web UI intégrée fonctionne immédiatement. Activez-le si l'appareil est accessible au-delà d'un LAN de confiance.

---

### Options d'une instance d'horloge

Outre `theme`, `font`, `size` et les couleurs, une instance d'horloge accepte :

| Clé | Type | Description |
| :--- | :--- | :--- |
| `clock_glow` | `enum` | Contour autour des chiffres : `0` aucun, `1` néon (la couleur choisie à pleine intensité avec un centre presque blanc, comme le fait la face Matrix), `2` une couleur dédiée. Par défaut `0`. |
| `clock_glow_color` | `color` | Couleur du contour lorsque `clock_glow` vaut `2` (par défaut `#00FF41`). |

Les thèmes `30` à `34` et `37` sont des faces adaptées des clockfaces Clockwise. Celles marquées
`(256x64)` sont conçues pour un panneau large et affichent un avertissement sur les petits panneaux.

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
| `theme` | `Options` | `0` | Index du thème d'horloge : `0` = Digital Standard, `1` = Flip Clock, `2` = Cyberpunk, `3` = Word Clock, `4` = Binary Clock, `5` = Pac-Man, `6` = Tetris, `7` = Slot Machine, `8` = Versus (M.U.G.E.N), `9` = Pong, `10` = Matrix Rain (Katakana), `30` = Mario (coup de carapace Koopa & ascension verticale Tate), `31` = Castlevania (Simon Belmont & coup de fouet), `32` = Pocket Index (Pokémon), `33` = World Clock, `34` = Ms. Pac-Man, `35` = Mega Man (tir plasma chargé & Metool), `37` = Words Clock, `39` = Sonic (Green Hill & Badnik Motobug), `41` = Metal Slug (Marco Rossi, char Di-Cokka & tirs d'obus). |
| `format` | `String` | `%H:%M:%S` | Format d'heure strftime. |
| `font` | `String` | `PressStart2P.ttf` | Fichier de police depuis `/fonts/`. |
| `timezone` | `Options` | `Europe/Paris` | Fuseau horaire ou région. |
| `size` | `int` | `2` | Facteur de mise à l'échelle de la police. |
| `clock_speed` | `int` | `100` | Vitesse d'animation en pourcentage (chute Tetris, balayage Pac-Man, course Mario) ; plus bas = plus lent. |
| `clock_glow` | `Options` | `0` | Halo autour des chiffres (`0` désactivé, `1` néon, `2` couleur de contour dédiée). |
| `clock_glow_color` | `String` | `#00FF41` | Couleur du contour lorsque `clock_glow` vaut `2`. |
| `color_1` | `String` | `#FFFFFF` | Couleur hex principale (début du dégradé sur le thème Custom). |
| `color_2` | `String` | `#FFFFFF` | Couleur hex secondaire (fin du dégradé sur le thème Custom). |
| `offset_x` | `int` | `0` | Décalage horizontal en pixels. |
| `offset_y` | `int` | `0` | Décalage vertical en pixels. |

### Moteur : `date`
| Champ | Type | Défaut | Description |
| :--- | :--- | :--- | :--- |
| `theme` | `Options` | `0` | Index du thème de date. |
| `format` | `String` | `%d/%m` | Format de date strftime. |
| `font` | `String` | `PressStart2P.ttf` | Fichier de police depuis `/fonts/`. |
| `timezone` | `Options` | `Europe/Paris` | Fuseau horaire ou région. |
| `size` | `int` | `2` | Facteur de mise à l'échelle de la police. |
| `color_1` | `String` | `#FFFFFF` | Couleur hex principale. |
| `color_2` | `String` | `#FFFFFF` | Couleur hex secondaire. |
| `offset_x` | `int` | `0` | Décalage horizontal en pixels. |
| `offset_y` | `int` | `0` | Décalage vertical en pixels. |

### Moteur : `dashboard` (Smart Dashboard Hub)
| Champ | Type | Défaut | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `theme` | `Options` | `0` | `0` (Cyberpunk Neon), `1` (Amber HUD), `2` (Minimalist Luxury), `3` (Matrix Phosphor) | Palette de couleurs des widgets et bordures. |
| `clock_mode` | `Options` | `1` | `0` (Digital), `1` (Cadran), `2` (Minimal) | Style de rendu de l'horloge. |
| `timezone` | `Options` | `system` | `system` ou fuseau IANA | Fuseau horaire ou réglage système de l'appareil. |
| `format_24h` | `Options` | `system` | `system`, `true`, `false` | Format horaire 24h ou 12h. |
| `temp_unit` | `Options` | `system` | `system`, `C`, `F` | Celsius (°C) ou Fahrenheit (°F). |
| `lang` | `Options` | `system` | `system`, `en`, `fr`, `es` | Langue des libellés et dates du dashboard. |
| `show_clock` | `bool` | `true` | `true`, `false` | Affiche l'horloge principale. |
| `show_world_clock` | `bool` | `true` | `true`, `false` | Affiche les fuseaux horaires mondiaux secondaires (`NYC`, `TYO`, `LON`...). |
| `world_clocks` | `String` | `NYC,TYO,LON` | Codes villes | Fuseaux horaires séparés par virgules (ex. `NYC,TYO,LON,PAR`). |
| `show_weather` | `bool` | `true` | `true`, `false` | Affiche le widget météo et la température extérieure. |
| `weather_city` | `String` | `Paris, FR` | Ville | Ville pour les prévisions météo. |
| `show_markets` | `bool` | `true` | `true`, `false` | Affiche le ticker défilant crypto & bourse. |
| `tracked_markets` | `String` | `BTC,ETH,SOL,NVDA` | Symboles | Symboles de marché séparés par virgules. |
| `show_sysinfo` | `bool` | `true` | `true`, `false` | Affiche les métriques CPU, RAM & Wi-Fi. |
| `show_date` | `bool` | `true` | `true`, `false` | Affiche le badge de date. |
| `show_seconds` | `bool` | `true` | `true`, `false` | Affiche les secondes de l'horloge. |
| `smooth_seconds` | `bool` | `true` | `true`, `false` | Balayage fluide des secondes ou saut par seconde. |
| `offset_x` | `int` | `0` | `-64` à `64` | Décalage horizontal en pixels. |
| `offset_y` | `int` | `0` | `-32` à `32` | Décalage vertical en pixels. |

### Moteur : `crypto`
| Champ | Type | Défaut | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `symbols` | `String` | `BTC,ETH` | Séparés par virgule | Symboles cryptos à surveiller (CoinGecko / Binance). |
| `show_chart` | `bool` | `true` | `true`, `false` | Afficher la courbe sparkline historique. |
| `chart_timeframe` | `Options` | `daily` | `hourly`, `daily`, `weekly`, `monthly` | Échelle de temps pour l'historique des cours. |
| `page_seconds` | `int` | `5` | `3` à `30` | Secondes d'affichage par page avant alternance. |
| `currency` | `Options` | `USD` | `USD`, `EUR`, `GBP`, `JPY` | Devise de cotation et symbole monétaire (`$`, `€`, `£`, `¥`). |
| `cache_ttl_min` | `int` | `1` | `1` à `60` | Minutes de rétention du cache de cotation. |

### Moteur : `stock`
| Champ | Type | Défaut | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `symbols` | `String` | `AAPL,NVDA,TSLA` | Séparés par virgule | Tickers boursiers à surveiller (Yahoo Finance). |
| `show_chart` | `bool` | `true` | `true`, `false` | Afficher la courbe sparkline historique. |
| `chart_timeframe` | `Options` | `daily` | `hourly`, `daily`, `weekly`, `monthly` | Échelle de temps pour l'historique des cours. |
| `page_seconds` | `int` | `5` | `3` à `30` | Secondes d'affichage par page avant alternance. |
| `currency` | `Options` | `USD` | `USD`, `EUR`, `GBP`, `JPY` | Devise de cotation et symbole monétaire (`$`, `€`, `£`, `¥`). |
| `cache_ttl_min` | `int` | `1` | `1` à `60` | Minutes de rétention du cache de cotation. |

### Moteur : `gnews` (Actualités en Direct & Ticker GNews)
| Champ | Type | Défaut | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `api_key` | `String` | `""` | Clé valide | Clé API GNews.io (optionnelle ; utilise des articles de démo si vide). |
| `category` | `Options` | `technology` | `general`, `world`, `nation`, `business`, `technology`, `entertainment`, `sports`, `science`, `health` | Catégorie thématique principale. |
| `keywords` | `String` | `""` | Texte / Requête | Mots-clés de recherche ou tags personnalisés (ex. `ai OR arcade`). |
| `lang` | `Options` | `auto` | `auto`, `en`, `fr`, `es`, `de`, `it`, `pt`, `nl`, `ru`, `zh`, `ja` | Langue des articles (`auto` synchronise avec la langue système). |
| `country` | `Options` | `auto` | `auto`, `us`, `fr`, `gb`, `es`, `de`, `ca`, `it`, `jp`, `au`, `br`, `in` | Édition régionale du pays. |
| `max_articles` | `int` | `5` | `3` à `15` | Nombre maximal d'articles mis en cache et alternés par cycle. |
| `requests_per_day` | `int` | `10` | `1` à `100` | Nombre total de requêtes API par 24 heures (offre gratuite : max 100). |
| `force_refresh` | `bool` | `false` | `true`, `false` | Purge le cache d'articles et interroge immédiatement l'API. |
| `cache_ttl_min` | `int` | `30` | `5` à `120` | Intervalle de rafraîchissement réseau du cache en minutes. |
| `display_mode` | `Options` | `smooth_scroll` | `smooth_scroll`, `serpentine`, `vertical_crawl`, `static_paged` | Mode d'animation (défilement fluide droite-gauche, flux serpentin arcade, défilement vertical ou pagination statique). |
| `scroll_speed` | `int` | `3` | `1` à `10` | Vitesse de défilement du bandeau (1 : Lent à 10 : Rapide). |
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
| `offset_x` | `int` | `0` | `-64` à `64` | Décalage horizontal en pixels. |
| `offset_y` | `int` | `0` | `-32` à `32` | Décalage vertical en pixels. |

Sur les panneaux 128x32 et 256x64, les pages météo utilisent la même mise en page que le firmware ESP32 : icône, puis jour et condition (formes longues si elles tiennent), puis températures alignées à droite. En °F, la maximale est affichée au-dessus de la minimale (convention américaine). La langue des jours suit `system.lang` (`en`, `fr`, `es`).

#### Formatage du champ `city` sur OpenWeatherMap
OpenWeatherMap utilise le code pays ISO 3166 (et le code d'état à 2 lettres pour les États-Unis) :
* **Emplacements Internationaux :** Utilisez `Ville,CodePays` (ex. `Paris,FR`, `London,GB`, `Tokyo,JP`, `Montreal,CA`).
* **Emplacements aux États-Unis :** Utilisez `Ville,CodeEtat,CodePays` (ex. `Tucson,AZ,US`, `Miami,FL,US`, `Dallas,TX,US`).
* **Où trouver le nom exact :** Rendez-vous sur [openweathermap.org](https://openweathermap.org) et recherchez votre ville.

### Moteur : `system_info` (Moniteur Système)
| Champ | Type | Défaut | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `theme` | `Options` | `0` | `0` à `2` | Disposition visuelle : `0` = Barres & Jauges HUD, `1` = Cyberpunk Neon, `2` = Grille compacte (2x2). |
| `show_cpu` | `bool` | `true` | `true`, `false` | Affiche le pourcentage et la jauge de charge processeur (CPU %). |
| `show_ram` | `bool` | `true` | `true`, `false` | Affiche le pourcentage et la jauge de mémoire vive (RAM %). |
| `show_temp` | `bool` | `true` | `true`, `false` | Affiche la température matérielle du SoC (vert/orange/rouge dynamique). |
| `show_uptime` | `bool` | `true` | `true`, `false` | Affiche le compteur de temps d'activité (Uptime). |
| `temp_unit` | `Options` | `C` | `C`, `F` | Unité de température : Celsius (`C`) ou Fahrenheit (`F`). |
| `offset_x` | `int` | `0` | `-64` à `64` | Décalage horizontal en pixels. |
| `offset_y` | `int` | `0` | `-32` à `32` | Décalage vertical en pixels. |

> [!NOTE]
> **Overlay Combattants M.U.G.E.N (`fighter`) :** Contrairement aux moteurs de rotation autonomes, `fighter` est un moteur d'overlay transverse superposé aux écrans compatibles (comme `clock` et `date`). Il se configure globalement dans le bloc `"system"` (`idle_fighter_enabled`, `idle_fighter_interval`, `idle_fighter_speed` — voir §4) et s'active par créneau de rotation via `rotation[].overlays.fighter` (voir §7).

### Moteur : `google_cast` (Google Home / Nest Audio)
| Champ | Type | Défaut | Description |
| :--- | :--- | :--- | :--- |
| `device_ip` | `String` | `""` | IP statique de votre enceinte Google Home / Nest Audio. Laissez vide pour la découverte mDNS automatique sur le réseau local. |
| `device_name` | `String` | `""` | Filtre sur le nom de l'appareil (ex. `Salon`) lors de la détection automatique sur le LAN. |
| `show_album_art` | `bool` | `true` | Télécharge et affiche la pochette de l'album à gauche de la matrice. |
| `show_progress` | `bool` | `true` | Affiche la barre de progression temporelle de la lecture en bas de l'écran. |
| `show_visualizer` | `bool` | `true` | Affiche l'égaliseur de fréquences animé quand la musique est en lecture. |
| `show_volume` | `bool` | `true` | Affiche le pourcentage de volume actuel de l'enceinte Google Nest. |

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
| `text` | `String` | `ArcadeMatrix` | Texte | Texte du message ou de la bannière à afficher. |
| `color` | `String` | `#ffffff` | Couleur Hex | Couleur du texte au format `#RRGGBB`. |
| `size` | `int` | `1` | `1` à `4` | Multiplicateur d'échelle de la police. |
| `direction` | `Options` | `rtl` | `rtl`, `ltr`, `static` | Sens de défilement ou affichage statique. |
| `speed` | `int` | `50` | `10` à `200` | Millisecondes par mise à jour d'un pixel (plus bas = plus rapide ; ignoré en statique). |

### Moteur : `marquee`
| Champ | Type | Défaut | Options | Description |
| :--- | :--- | :--- | :--- | :--- |
| `file_path` | `FileAsset` | `data/marquees/marquee.gif` | `.gif,.png,.jpg,.jpeg` | Chemin vers le fichier GIF ou image de marquee (reçoit également les marquees Pixelcade / Recalbox / Batocera en direct via MQTT ou HTTP). |
| `speed_multiplier` | `Float` | `1.0` | `0.25` à `3.0` | Facteur de vitesse de lecture de l'animation. |
| `fit_mode` | `Options` | `fit` | `fit`, `center`, `stretch` | Mode de mise à l'échelle de l'image. |

### Moteur : `mqttdata` (MQTT Data)
| Champ | Type | Défaut | Description |
| :--- | :--- | :--- | :--- |
| `topics` | `String` | `""` | Jusqu'à 6 topics MQTT complets, séparés par des virgules. Chaque topic est une page. |

Tout le reste vient de chaque message retained : son `type` (`value`, `table`, `graph`, `weather`), sa durée d'affichage (`seconds`) et son contenu. L'écran reste affiché le temps d'un cycle complet de ses pages. Formats, limites et messages : [MQTT_DATA_CONTRACT_FR.md](MQTT_DATA_CONTRACT_FR.md) ; blueprints Home Assistant : [HOME_ASSISTANT_FR.md](HOME_ASSISTANT_FR.md).

---

*Note : L'ensemble des schémas de configuration peut également être interrogé en direct au format JSON via `GET /api/engines`.*

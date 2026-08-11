🇬🇧 [English](README.md) | 🇫🇷 Français | 🇪🇸 [Español](README_ES.md)

# ArcadeMatrix RPi 🍓👾

L'implémentation **Rust** originale du projet **ArcadeMatrix**, spécialement conçue pour fonctionner sur un **Raspberry Pi** connecté à une matrice LED RGB (HUB75) via le HAT Adafruit ou le matériel Joy-IT.

Développée en parallèle de la version ESP32 C++, cette version tire parti de la puissance du Raspberry Pi pour offrir un pipeline graphique multi-threadé haute performance.

📚 **Documentation développeur :** [Premiers pas (workspace dev)](docs/GETTING_STARTED_FR.md) · [Guide développeur](docs/DEVELOPER_FR.md) · [Architecture](docs/ARCHITECTURE_FR.md) · [Guide rapide (utilisateurs finaux)](docs/QUICKSTART_FR.md)

---

## 💾 Guide Rapide : Image précompilée (Recommandé)

Nous fournissons des fichiers `.img` précompilés et entièrement automatisés, publiés automatiquement à chaque nouvelle version.

| Architecture | Recommandé pour | Compatible avec | Télécharger |
|--------------|-----------------|-----------------|-------------|
| **64-bits (aarch64)** | Raspberry Pi 3, 4, 5, Zero 2 W | Pi 3, 4, 5, Zero 2 W | [⬇️ Télécharger Image 64-bits](https://github.com/red77290/ArcadeMatrix_RPI/releases/latest/download/ArcadeMatrix_Release_aarch64.img.xz) |
| **32-bits (armhf)** | Raspberry Pi 1, 2, Zero (Original) | Tous les modèles Raspberry Pi | [⬇️ Télécharger Image 32-bits](https://github.com/red77290/ArcadeMatrix_RPI/releases/latest/download/ArcadeMatrix_Release_armhf.img.xz) |

*(Ce sont des fichiers `.img.xz` — décompressez avec 7-Zip/Keka/`xz -d` avant le flash. Voir la [liste complète des releases](https://github.com/red77290/ArcadeMatrix_RPI/releases) pour les anciennes versions.)*

1. Flashez le `.img` sur votre carte SD avec **Raspberry Pi Imager**.
2. Une fois le flash terminé, réinsérez la carte SD dans votre PC/Mac. Vous verrez apparaître un lecteur USB **DATA** de !
3. Ouvrez le fichier `conf.ini` situé sur ce lecteur DATA pour configurer la taille de votre matrice et vos identifiants **Wi-Fi** (`SSID` et `PASS`).
4. Insérez la carte SD dans votre Raspberry Pi et allumez-le.
5. La matrice s'allumera immédiatement et **affichera l'adresse IP** pendant 5 secondes. Utilisez cette IP pour accéder à l'interface Web !

---

## 🌟 Fonctionnalités (exclusivités RPi vs ESP32)

* 🔄 **Mises à jour Over-The-Air (OTA)** : mettez à jour le binaire binaire Rust directement depuis l'interface Web sans re-flasher votre image de carte SD !
* 🚀 **Performances Rust natives** : moteur multi-threadé hautes performances utilisant Actix-web et le traitement d'images en Rust compilé pur avec 0% d'utilisation CPU au repos.
* **Polices chargeables dynamiquement (`.ttf`)** : fini les fichiers de police codés en dur ! Déposez n'importe quelle police `.ttf` ou `.otf` directement dans le dossier `fonts/`, et l'interface Web la listera automatiquement pour l'utiliser sur l'horloge ou la date.
* **Tailles et décalages d'horloge/date illimités** : vous n'êtes plus limité aux tailles 1, 2 ou 3. Vous pouvez définir n'importe quelle taille et positionner librement le texte sur d'immenses panneaux matriciels (ex. 256x64).
* **Sélection massive d'horloges** : profitez d'une variété d'horloges animées comprenant les classiques Arcade, Binary, Cyberpunk, Flip, Word, ainsi que les toutes nouvelles horloges **Pac-Man**, **Tetris**, **SlotMachine** et **Versus (Mugen)** !
* 📈 **Tickers Crypto & Bourse en temps réel** : cotations en direct et badges % sur 24h depuis CoinGecko, Binance et Yahoo Finance avec cache configurable.
* **Véritable pluie numérique Matrix (Katakana)** : un effet Matrix entièrement personnalisé, ultra fluide et authentique (`DotGothic16`) avec des Katakana demi-largeur qui tombent et un texte en espace négatif d'« LED éteintes » qui perce la pluie.
* **Dégradés fluides personnalisés** : en plus des thèmes classiques Publisher (Nintendo, Capcom, Sega...), vous pouvez désormais choisir un thème **Custom Color / Gradient** et sélectionner deux couleurs pour générer un dégradé dynamique.
* **Playlists d'images dynamiques (GIF/PNG/JPG)** : lisez de vrais fichiers `.gif` et `.png` dynamiquement directement depuis le système de fichiers, sans problèmes de fragmentation de carte SD.

---

## 🚀 Prérequis matériels & Compatibilité

Grâce à l'implémentation Rust natif ultra-léger (~5 Mo de binaire, ~10 Mo de RAM, 0% d'utilisation CPU au repos), **ArcadeMatrix prend désormais pleinement en charge les anciennes générations de Raspberry Pi sans aucun ralentissement ni baisse de framerate** :

1. **Raspberry Pi** : 
   - **Anciens modèles / Mono-cœur** : Pi 1 (B, B+, A+), Pi Zero, Pi Zero W *(Pleinement pris en charge avec 0 lag grâce à Rust !)*
   - **Multi-cœurs** : Pi 2, Pi 3, Pi 4, Pi Zero 2 W *(Recommandé)*
   - *(⚠️ **Avertissement Pi 5** : La bibliothèque hzeller rgb-led-matrix ne prend PAS en charge nativement le Pi 5 via GPIO à cause de la nouvelle puce RP1. Vous devez utiliser une carte adaptatrice active pour Pi 5 !).*
2. **Matrice LED RGB** : panneaux HUB75 (ex. 64x64, 128x32, 256x64).
3. **Adafruit RGB Matrix HAT** (ou Joy-IT, ou câblage personnalisé).
4. **Carte MicroSD** (16 Go ou plus recommandés pour l'image précompilée).

---

## 🛠️ Installation Avancée\n\n### Option 2 : installation manuelle
Si vous préférez l'installer manuellement sur un **Raspberry Pi OS Lite (64-bit)** fraîchement installé :
Une fois connecté à votre Raspberry Pi en SSH :

```bash
curl -sSL https://raw.githubusercontent.com/red77290/ArcadeMatrix_RPI/main/install.sh | bash
```
*(Si le dépôt est privé, vous devrez d'abord faire le `git clone` manuellement puis exécuter `./install.sh` depuis le dossier.)*

Le script va automatiquement :
1. Installer Rust, Actix-web, image-rs et `build-essential`.
2. Télécharger et compiler le pilote `hzeller/rpi-rgb-led-matrix`.
3. Configurer `systemd` pour démarrer automatiquement ArcadeMatrix au boot.

### Option 3 : Installation depuis un ordinateur via script (Smart Deploy)
Si vous souhaitez compiler l'application sur votre propre ordinateur (beaucoup plus rapide) et la déployer automatiquement sur le Raspberry Pi, vous pouvez utiliser les scripts de déploiement intelligents basés sur Docker.

**Sur macOS / Linux :**
```bash
bash scripts/deploy.sh --ip <PI_IP> --user <PI_USER> --pass <PI_PASS>
# Exemple : bash scripts/deploy.sh --ip 192.168.1.177 --user pi --pass raspberry
```

**Sur Windows (PowerShell) :**
```powershell
.\scripts\deploy.ps1 -PI_IP "192.168.1.177" -PI_USER "pi" -PI_PASS "raspberry"
```
*Le script détectera automatiquement l'architecture de votre Pi, compilera le binaire via Docker, arrêtera le service distant, enverra le fichier et relancera l'application.*

---

## ⚠️ Avertissement Matériel : Wi-Fi & Interférences (Lignes VHS)

Les Raspberry Pi (particulièrement les Pi 3 et Zero W) partagent l'horloge de leur bus Wi-Fi interne avec le contrôleur **PWM/PCM** utilisé par la matrice LED.

Lorsque l'on pilote de très grandes matrices (comme du **256x64**), le contrôleur DMA doit pousser une quantité massive de données vers les broches GPIO. Si le pulsing matériel est activé (`disable_hardware_pulsing = false`), cela crée une intense saturation de la bande passante DMA qui **asphyxie la puce Wi-Fi interne (SDIO)**, provoquant de sévères pertes de paquets, du lag, et des déconnexions.

Bien qu'un Wi-Fi instable puisse être tolérable si vous utilisez l'interface Web uniquement pour changer l'horloge de temps en temps, cela casse complètement les fonctionnalités qui dépendent d'une connexion internet ou locale stable :
* **Recalbox/Batocera/Pixelcade (MQTT)** : Perte des messages ou déconnexion du broker.
* **Crypto & Bourse (Stocks)** : Les appels API vont subir des timeouts et échoueront.
* **Météo (Weather)** : Les appels API échoueront.

**Solutions pour les utilisateurs de matrice 256x64 :**
1. **Utiliser un câble Ethernet (RJ45)** (Pi 3B/4) : Contourne complètement la puce Wi-Fi SDIO.
2. **Utiliser un Dongle Wi-Fi USB** : Les contrôleurs USB utilisent un bus interne différent et sont immunisés contre la saturation DMA du PWM.
3. **Mettre `disable_hardware_pulsing = true`** : Force la matrice à utiliser un rendu logiciel (CPU bit-banging) au lieu du DMA matériel. Votre Wi-Fi fonctionnera à la perfection, mais vous verrez un léger effet de "lignes VHS" (scintillement) sur la matrice.

*(Note : Les matrices **128x32** nécessitent 4 fois moins de bande passante DMA, elles fonctionnent donc généralement très bien avec le pulsing matériel activé et le Wi-Fi interne).*

*(Note concernant le **Raspberry Pi 4** : Le Pi 4 utilise une architecture PCIe et un contrôleur DMA beaucoup plus rapides. Il est très probable qu'il ne soit PAS impacté par ce bug de saturation Wi-Fi en 256x64. Cependant, cela n'a pas encore été testé officiellement car nous n'avons pas de Pi 4 sous la main pour le confirmer.)*

### Tableau de Compatibilité Raspberry Pi & Résolution

| Résolution de la matrice | Configuration Réseau / Hardware Pulsing | Qualité d'Image | Wi-Fi / APIs / MQTT |
|-------------|-----------------------------|----------------|-------------------|
| **128x32**  | Wi-Fi Interne + `disable_hardware_pulsing = false` | ✅ Parfaite | ✅ Stable |
| **256x64**  | **Câble Ethernet (RJ45)** + `disable_hardware_pulsing = false` | ✅ Parfaite | ✅ Stable |
| **256x64**  | **Dongle Wi-Fi USB** + `disable_hardware_pulsing = false` | ✅ Parfaite | ✅ Stable |
| **256x64**  | Wi-Fi Interne + `disable_hardware_pulsing = true` | ⚠️ Léger scintillement | ✅ Stable |
| **256x64**  | Wi-Fi Interne + `disable_hardware_pulsing = false` | ✅ Parfaite | ❌ Crash Wi-Fi / Timeouts |

---

## 🎨 Gestion des médias

L'image précompilée inclut une **partition DATA dédiée de 8 Go** formatée en exFAT. Cela signifie que vous pouvez brancher directement votre carte SD sur votre ordinateur Windows ou Mac pour glisser-déposer vos fichiers sans avoir besoin de SSH ni de FTP !

### Sprites & GIFs
* **`/fighters_32/`** ou **`/fighters_64/`** : placez ici vos sprites `.fgt` (voir la section Sprites MUGEN ci-dessous).
* **`/gifs/`** : déposez vos boucles `.gif` dans des dossiers à l'intérieur.
L'interface Web analysera automatiquement ces dossiers et vous permettra de cocher ceux que vous voulez lire !

### Fonts
* **`/fonts/`** : déposez ici vos fichiers `.ttf`, `.otf` ou `.bdf`. 
Par défaut, le projet est livré avec `PressStart2P.ttf`, `VT323.ttf` et `DotGothic16.ttf`.

---

## 🕸️ Interface Web
Naviguez vers `http://<YOUR_PI_IP>:8080/` pour accéder au panneau de contrôle.

L'interface est exactement la même que sur la version ESP32, avec les contrôles du Dashboard, la sélection de Playlist, la configuration de l'horloge et les paramètres MQTT, ainsi que des contrôles supplémentaires pour les **Gradients** et les **Unlimited Sizes**.

---

## 🕹️ Intégration Recalbox & Batocera (Pixelcade Marquees)

ArcadeMatrix prend en charge les marquees dynamiques **style Pixelcade** lorsque vous sélectionnez ou lancez un jeu sur votre Recalbox ou Batocera !

Quand vous parcourez vos listes de jeux, le Raspberry Pi télécharge les marquees officielles Pixelcade depuis GitHub **en arrière-plan et en temps réel**, les met en cache sur votre carte SD et les affiche sur votre matrice LED. Si un jeu n'a pas d'image, il affichera un élégant texte animé de repli.

### Installation automatique (recommandée)
Allez dans l'onglet **MQTT** de l'interface Web ArcadeMatrix, saisissez l'IP de votre Recalbox/Batocera ainsi que son mot de passe root (par défaut `recalboxroot` ou `linux`) et cliquez sur **Install Sync Script**. Cela injectera automatiquement le daemon via SSH.

### Installation manuelle (Recalbox)
Si vous préférez l'installation manuelle, ou si l'installation réseau échoue :
1. Ouvrez le fichier `tools/recalbox_setup_mqtt.sh` inclus dans le projet.
2. Modifiez la ligne `MQTT_BROKER="192.168.1.xxx"` et définissez l'IP du Raspberry Pi qui fait tourner la matrice LED.
3. Copiez le fichier `tools/recalbox_setup_mqtt.sh` vers votre Recalbox (par ex. dans `/recalbox/share/`).
4. Connectez-vous en SSH à votre Recalbox et exécutez : `bash /recalbox/share/recalbox_setup_mqtt.sh`.

### Comment fonctionne l'architecture du daemon ?
Contrairement aux scripts natifs Recalbox qui s'exécutent (et figent le système) à chaque mouvement de joystick, ArcadeMatrix installe **un daemon Rust ultra-léger en arrière-plan**.
* **Zéro lag :** consomme 0 % de CPU. EmulationStation ne subit aucun stutter ni lag, même en défilant à vitesse maximale.
* **Anti-spam (debounce) :** si vous parcourez rapidement 50 jeux, le daemon n'inondera pas le réseau. Il n'envoie le message à la matrice que si vous restez sur un jeu plus de 150 millisecondes.
* **Thread safety :** côté matrice LED, les téléchargements et le moteur de dessin sont séparés par des threads avec des verrous robustes, empêchant les gels et la corruption du cache d'images.

---

## 🔧 Configuration de la matrice
Si vous avez une matrice plus grande que 64x64 ou 128x32, ou si vous utilisez un HAT non Adafruit, vous devrez peut-être ajuster les arguments `hzeller` dans `src/core/matrix.rs`. Par défaut, c'est réglé sur `--led-gpio-mapping=adafruit-hat` et `128x32`.

Vous pouvez aussi modifier la luminosité de la matrice dynamiquement via les paramètres de l'interface Web.
- Activez les modes Standby/Night.

---

## 📂 Gestion des médias (GIFs et sprites MUGEN)

### Ajouter des GIFs
Déposez simplement n'importe quels fichiers `.gif` standards dans le répertoire `gifs/` :
```text
ArcadeMatrix_RPi/
└── gifs/
    ├── mario_run.gif
    ├── sonic_wait.gif
    └── ...
```

### Ajouter des sprites MUGEN
Pour obtenir des performances parfaites à 60fps et des alignements exacts de « virtual ground » sur d'immenses rosters de personnages, le moteur Fighter utilise des fichiers `.fgt` prétraités accompagnés d'un manifeste `index.txt`.

**Vous ne pouvez pas simplement déposer des images brutes dans les dossiers fighters !**
Vous DEVEZ utiliser l'outil `mugen_extractor.py` fourni dans le dossier `tools/mugen_extractor/` pour traiter vos personnages MUGEN. 

L'extracteur lira les fichiers `.sff` et `.air` de MUGEN, calculera les bounding boxes parfaites pour éviter le jitter des animations, et exportera des fichiers `.fgt` optimisés directement dans vos dossiers `fighters_32/` et `fighters_64/`.

Veuillez consulter `tools/mugen_extractor/README_FR.md` pour les instructions complètes sur l'ajout de nouveaux personnages MUGEN !

---

## ⚙️ Configuration avancée (conf.ini)

Si vous préférez éditer les réglages manuellement au lieu d'utiliser l'interface Web, vous pouvez modifier directement le fichier `conf.ini` situé sur la partition **DATA** de votre carte SD. 
C'est particulièrement utile pour configurer le Wi-Fi avant le premier démarrage.

### 🌐 [WIFI]
| Parameter | Default | Description |
|---|---|---|
| `SSID` | `YourNetworkName` | The name of your Wi-Fi network. |
| `PASS` | `YourNetworkPassword` | The password for your Wi-Fi network. |
| `CONFIGURED` | `false` | Set to `false` to force the Raspberry Pi to attempt a Wi-Fi connection on its next boot. Automatically sets back to `true` on success. |

### 🎛️ [MATRIX]
| Parameter | Default | Description |
|---|---|---|
| `ROWS` / `COLS` | `32` / `64` | The pixel dimensions of a single LED panel. |
| `HARDWARE_MAPPING` | `adafruit-hat` | Type of HAT/wiring used. (`adafruit-hat`, `adafruit-hat-pwm`, `regular-pi1`, `regular`). |
| `CHAIN` / `PARALLEL` | `1` / `1` | `CHAIN` for horizontal daisy-chaining. `PARALLEL` for vertical stacking on multiple HUB75 ports. |
| `SLOWDOWN` | `2` | Hardware slowdown (1 to 4). Increase if your Matrix has flickering or visual artifacts (especially Pi 3/4). |
| `disable_hardware_pulsing`| `false` | **CRITIQUE :** Mettre sur `true` pour empêcher le crash du Wi-Fi interne (au prix d'un léger scintillement). |
| `BRIGHTNESS` | `100` | Global matrix brightness (1 to 100). |
| `RGB_SEQUENCE` | `RGB` | Color order. Change to `RBG` or `BGR` if your colors look swapped. |

### ⏰ [TIME] & [DATE]
| Parameter | Default | Description |
|---|---|---|
| `FORMAT_24H` | `true` | `true` for 24-hour format, `false` for 12-hour AM/PM format. |
| `CLOCK_FONT` | `DotGothic16.ttf`| Name of the `.ttf` or `.bdf` file in the `/fonts/` folder to use for the clock. |
| `CLOCK_SIZE` | `16` | Font size (scaling factor) for the clock. |
| `THEME` | `0` | The numeric ID of the animated clock theme (e.g. 19 for Flip, 21 for True Matrix). |
| `CLOCK_COLOR_1` | `#ffffff` | Primary hex color. Used for gradients if theme is Custom (20). |
| `CLOCK_COLOR_2` | `#ffffff` | Secondary hex color. Used for gradients if theme is Custom (20). |

*(La section `[DATE]` contient des paramètres identiques pour configurer l'affichage de la date.)*

### 🔄 [IDLE]
| Parameter | Default | Description |
|---|---|---|
| `ROTATION` | `all` | Dictates rotation behavior (`clock`, `gifs`, `sprites`, or `all`). |
| `CLOCK_DURATION_SEC`| `10` | How long the clock/date stays on screen during the rotation loop. |
| `GIF_DURATION_SEC` | `10` | How long a single GIF stays on screen before advancing. |
| `SELECTED_GIFS` | *(empty)* | Comma-separated list of media to loop. Leave empty to play everything. |
| `SELECTED_SPRITES` | *(empty)* | Comma-separated list of sprites to loop. Leave empty to play everything. |

### 📈 [CRYPTO] & 📊 [STOCK]
| Parameter | Default | Description |
|---|---|---|
| `SYMBOLS` | `BTC,ETH,SOL,DOGE` / `AAPL,NVDA,TSLA,MSFT` | Comma-separated list of symbols to display. |
| `CACHE_TTL_MIN` | `1` | Refresh rate / Cache TTL in minutes (prevents API rate limiting). |

### 🌙 [STANDBY]
| Parameter | Default | Description |
|---|---|---|
| `NIGHT_MODE_ENABLED`| `false` | If `true`, the Matrix will automatically turn off and wake up. |
| `TURN_OFF_AT` | `23:00` | HH:MM formatted time for screen sleep. |
| `WAKE_UP_AT` | `07:00` | HH:MM formatted time for screen wake. |

### 🔒 [API]
| Parameter | Default | Description |
|---|---|---|
| `AUTH_ENABLED` | `false` | If `true`, requires the `X-API-Token` header to match `TOKEN` on the sensitive endpoints: `/api/wifi`, `/api/mqtt/install`, `/api/system/reboot`, `/api/system/shutdown`. Disabled by default so the bundled Web UI keeps working out of the box; enable it if the device is reachable beyond a trusted LAN. |
| `TOKEN` | *(auto-generated)* | A random token generated on first boot. Copy it here (or read it from `conf.ini` after first run) and send it as `X-API-Token` when calling protected endpoints. |

## 📜 Licence
Ce projet est publié sous la **[PolyForm Noncommercial License 1.0.0](LICENSE)**.

**En résumé :** vous êtes libre d'utiliser, modifier et partager ce projet pour tout usage non-commercial (usage personnel, projet hobbyiste, recherche, éducation, organismes publics/à but non lucratif) - voir le fichier [LICENSE](LICENSE) complet pour les termes exacts. **Tout usage commercial (vente d'unités assemblées, de kits, ou de produits/services dérivés) nécessite une licence séparée - contactez [Red1L](https://github.com/red77290) pour discuter des conditions commerciales.**

## Personnalisation Utilisateur (Custom User)
Si vous souhaitez changer l`utilisateur par défaut (`pi`) et son mot de passe par défaut (`raspberry`) pour la génération des images, vous pouvez modifier le fichier `scripts/defaults.sh` avant de lancer la compilation.

```bash
export AM_USER="votre_utilisateur"
export AM_PASS="votre_mot_de_passe"
```
Lors de la génération avec `scripts/build_image.sh`, ces variables seront automatiquement lues. Le hachage du mot de passe sera dynamiquement calculé avec SHA-512 pour l`injection dans l`image `.img` (`userconf.txt`).
Les scripts de déploiement (`autoInstall.sh` et `deploy.sh`) utiliseront également ces variables pour configurer correctement les permissions (traversée du dossier home pour le daemon) et installer les alias `.bash_aliases` au bon endroit.

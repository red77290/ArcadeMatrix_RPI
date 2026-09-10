🇬🇧 [English](README.md) | 🇫🇷 Français | 🇪🇸 [Español](README_ES.md)

# tools/

Utilitaires côté PC/manuels pour ArcadeMatrix_RPi. Ils ne font pas partie de l'application
Flask/API - vous les exécutez vous-même, séparément, soit sur votre propre ordinateur, soit en les
copiant sur votre Recalbox.

## `mugen_extractor/`

Convertit les fichiers de personnages MUGEN (`.sff`/`.air`) au format binaire de sprite `.fgt`
utilisé à la fois par `engines/fighter.py` de ce projet **et** par le `FighterEngine` (C++) du
projet frère ESP32 - c'est le même outil, conservé identique dans les deux dépôts puisqu'il n'y a
pas de mécanisme de package partagé entre ces deux bases de code indépendantes. Voir
`mugen_extractor/README_FR.md` pour l'utilisation complète.

## `rpi_emulationstation_base_os_setup.sh` (et `recalbox_setup_mqtt.sh`)

Un script shell autonome que vous copiez sur votre console de rétro-gaming (Recalbox, Batocera ou RetroPie) et exécutez **directement sur
l'appareil via SSH** (pas depuis votre PC) pour installer le daemon MQTT "en cours de lecture" :

1. `ssh <user>@<ip-console>` (ex : `root@<ip>` mot de passe `recalboxroot` pour Recalbox, `root`/`linux` pour Batocera, `pi`/`raspberry` pour RetroPie).
2. Lancez le script :
   ```bash
   sh /tmp/rpi_emulationstation_base_os_setup.sh
   ```
3. Saisissez l'adresse IP de votre ArcadeMatrix (ESP32 ou Raspberry Pi) lorsque demandée.
4. Sélectionnez votre système cible :
   - `1: Détection automatique`
   - `2: Recalbox`
   - `3: Batocera`
   - `4: RetroPie`
5. Le script installe automatiquement le daemon, configure le démarrage du système et propose de redémarrer.

Ce qu'il installe :
- Un petit daemon Python (`arcadematrix_daemon.py`) qui surveille les lancements/arrêts de jeux et publie `{"status": "playing"|"browsing"|"stopped", "game": "<nom de base de la rom>", "system": "<SystemId>"}` via MQTT sur `system/playing/<os>` (`system/playing/recalbox`, `system/playing/batocera` ou `system/playing/retropie`) via `mosquitto_pub`.
- L'intégration au démarrage assurant la persistance après chaque redémarrage.

**Note pour les utilisateurs qui ont aussi un appareil ESP32 ArcadeMatrix** :
Ce daemon et le format de topic standard (`system/playing/#`) sont 100% identiques et compatibles entre les versions ESP32 et Raspberry Pi. Une seule installation sur votre console sert les deux appareils simultanément. Vous pouvez également exécuter l'installeur automatisé côté PC (`tools/install.sh` / `tools/install.ps1`).

**Normalement cela se fait automatiquement via la fonctionnalité d'installation SSH de l'interface
web** (`core/ssh_installer.py`, déclenchée depuis la page Réglages) - ce script est le recours
manuel pour ceux qui préfèrent ne pas donner leurs identifiants SSH Recalbox à l'application, ou qui
ont besoin d'inspecter/personnaliser exactement ce qui est installé avant de l'exécuter.

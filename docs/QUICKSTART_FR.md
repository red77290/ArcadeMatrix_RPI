🇬🇧 [English](QUICKSTART.md) | 🇫🇷 Français | 🇪🇸 [Español](QUICKSTART_ES.md)

# Guide de démarrage rapide

Ce guide vous aidera à installer et configurer ArcadeMatrix sur votre Raspberry Pi.

## 1. Installation (recommandée)

Nous fournissons une image précompilée prête à l'emploi (`ArcadeMatrix_Release_aarch64.img.xz` pour Pi 3/4/5/Zero 2 W ou `armhf.img.xz` pour Pi 1/2/Zero).

> **Carte MicroSD requise : `16 Go` minimum réel (`>= 15,1 Go`) — `32 Go` fortement recommandé.** L'image complète fait `15,03 Go` (`bootfs` + système Linux + partition `DATA` exFAT d'environ `6 Go` pour vos GIFs, sprites MUGEN et polices). N'utilisez pas une carte `8 Go` ni une carte « 16 Go » réduite (`< 15,03 Go`), sinon la vérification échouera et la partition `DATA` sera tronquée.

1. **Flashez l'image sur votre carte MicroSD :**
   - **Sans décompression préalable (recommandé) :** utilisez **[Raspberry Pi Imager](https://www.raspberrypi.com/software/)** *(Système d'exploitation → **Utiliser une image personnalisée**, sélectionnez directement le fichier `.img.xz`, et répondez impérativement **« Non »** s'il propose d'appliquer des réglages personnalisés d'OS)* ou **[Balena Etcher](https://etcher.balena.io/)**.
   - **Si votre outil exige de décompresser en `.img` d'abord** *(ex. Rufus ou Win32DiskImager — Pi Imager / Etcher restent recommandés car Win32DiskImager ne rafraîchit pas la table des partitions sous Windows)* : extrayez le `.img.xz` **uniquement** avec **[7-Zip](https://www.7-zip.org/)** (Windows), **[Keka](https://www.keka.io/)** (macOS) ou **`xz -d`** (Linux) en prévoyant au moins `16 Go` d'espace libre *(évitez l'extracteur intégré de Windows 11 et WinRAR qui peuvent tronquer les archives `.xz` > 4 Go)*.
2. **Ouvrez le lecteur USB `DATA` :** débranchez puis réinsérez physiquement la carte SD dans votre PC/Mac *(cliquez sur **Annuler** si Windows propose de formater la partition Linux)*. Un lecteur USB nommé **DATA** apparaît.
   - 💡 *Sous Windows, si seul `bootfs` apparaît (`DATA` absent de l'Explorateur)* : faites **Clic droit sur le menu Démarrer → Gestion des disques** (`diskmgmt.msc`), clic droit sur le bloc **`DATA (exFAT)`** → **Modifier la lettre de lecteur et les chemins d'accès... → Ajouter... → Choisissez une lettre (ex. `E:`) → OK** *(utilisez un adaptateur MicroSD en USB si un vieux lecteur SD interne de PC portable grise l'option)*.
3. Ouvrez le fichier `config.json` situé sur ce lecteur **DATA** pour y renseigner vos identifiants Wi-Fi (`"ssid"` et `"password"`) ainsi que la taille de votre matrice, et déposez vos médias dans `gifs/`, `fighters_32/`, `fighters_64/` ou `fonts/`.
4. Insérez la carte SD dans le Raspberry Pi et allumez-le. L'adresse IP s'affichera sur la matrice !

## 2. Configuration Web & Mises à jour OTA

Une fois le Pi allumé, ouvrez un navigateur sur votre téléphone ou PC et allez sur :
`http://<RASPBERRY_IP>:8080`

Ici vous pourrez configurer :
- Les couleurs, polices et thèmes de l'horloge et de la date.
- Les fonctionnalités activées dans la boucle de rotation.
- Les réglages de luminosité et de mode nuit.
- 🔄 **Mise à jour du firmware (OTA)** : allez dans l'onglet **System**, glissez-déposez le binaire compilé `arcadematrix_vX.Y.Z_aarch64` et cliquez sur **Upload & Update Firmware** pour mettre à jour le système sans jamais re-flasher votre carte SD !

## 3. Ajout de contenu (GIFs, sprites, polices)

Pour ajouter vos propres médias, **branchez simplement votre carte SD sur votre PC/Mac**.
Le lecteur **DATA** apparaîtra comme une clé USB standard (format exFAT) :

- **GIFs** : déposez-les dans le dossier `gifs/`.
- **Sprites MUGEN** : utilisez notre extracteur pour générer des fichiers `.fgt` et placez-les dans `fighters_32/` ou `fighters_64/`.
- **Fonts** : déposez des polices `.ttf` ou `.bdf` dans le dossier `fonts/`.

## 4. Connexion matérielle

Nous recommandons d'utiliser un Adafruit RGB Matrix HAT ou Bonnet connecté à un Raspberry Pi Zero 2 W ou Pi 4. Assurez-vous que le connecteur HUB75 est correctement branché à votre panneau LED.

## 5. Optionnel : Relier Recalbox / Batocera / RetroPie & Showcase en Jeu

1. **Installation Console en 1 Clic** : Ouvrez **Système → MQTT & API → 🎮 FrontendSync**, renseignez l'IP et les identifiants SSH de votre console Recalbox, Batocera ou RetroPie, puis cliquez sur **Install Sync Script** (la console ne mémorise que l'IP du DMD et découvre automatiquement le broker MQTT actif — aucune réinstallation ultérieure requise).
2. **Marquees Instantanés** : Passez **Activer MQTT** sur **Activé**. Parcourir vos menus affiche immédiatement les marquees Pixelcade (`0 ms`).
3. **Fiches & Vidéos de Gameplay en Jeu : Mode Online, Mode Offline ou Hybride** (avec **[dmd_media_showcase](https://github.com/red77290/dmd_media_showcase)**) :
   - 🌐 **Mode 100 % Online (Streaming Direct sans copier de fichiers)** : Dans **🎬 DMD Media Showcase**, activez le serveur Showcase (`🟢 Communication OK`), puis dans **🎮 FrontendSync**, activez **Utiliser le Broker MQTT Showcase si actif** et **Showcase en Jeu (Carte + Vidéo Gameplay)** (`Alterner Marquee & Showcase`, `Une fois` ou `Boucle`).
   - 💾 **Mode 100 % Offline (Sans PC allumé ni Internet)** : Dans l'onglet **`7. Batch Scraper & Packs Offline`** de Showcase (depuis un PC, NAS ou smartphone), importez vos `gamelist.xml` Recalbox/Batocera et copiez les dossiers **`showcase/`** (ou `showcase_tate/` en vertical) + **`pixelcade/`** sur votre lecteur USB **DATA** (ainsi que vos playlists horloge `snes_platformer`... dans `DATA:/gifs/`). Activez simplement **Showcase en Jeu** dans **🎮 FrontendSync** : vos `.gif` `RGB888` démarrent en **`0 ms`** même avec Showcase Server désactivé !
   - ⚡ **Mode Hybride (Automatique)** : Si `DATA:/showcase/` est présent **et** que Showcase Server est activé, le Raspberry Pi joue toujours le `.gif` local en priorité instantanée (`0` trafic réseau) et n'interroge le serveur Online que pour les nouveaux jeux absents de la carte SD ([Tutoriel complet Débutant](https://github.com/red77290/dmd_media_showcase/blob/main/docs/TUTORIAL_ONLINE_OFFLINE_SCRAPING_FR.md)).

## 6. Annexe : Utiliser les Outils Compagnons PC & Consoles (`ArcadeMatrix-tools.zip`)

Tous les outils optionnels pour PC, Mac, Linux et consoles rétro sont regroupés dans une archive autonome — **sans avoir à télécharger le code source complet** :

👉 **[⬇️ Télécharger `ArcadeMatrix-tools.zip` (Dernière Release)](https://github.com/red77290/ArcadeMatrix_RPI/releases/latest/download/ArcadeMatrix-tools.zip)** *(également disponible sur le [Portail Raspberry Pi](https://red77290.github.io/ArcadeMatrix_RPI/)).*

Décompressez `ArcadeMatrix-tools.zip` sur votre ordinateur pour accéder à :
- **Sync Console & Daemon Universel High-Scores (`ArcadeMatrix-tools/install.sh`, `ArcadeMatrix-tools/install.ps1`, `ArcadeMatrix-tools/rpi_emulationstation_base_os_setup.sh`)** : Alternative manuelle à l'installation en 1 clic de la Web UI pour Recalbox, Batocera et RetroPie — affiche les marquees en `0 ms` et célèbre vos nouveaux records High-Scores sur ~3 100 ROMs MAME/FBNeo `.hi` + NVRAM Neo-Geo/CPS.
- **Extracteur de Sprites MUGEN (`ArcadeMatrix-tools/mugen_extractor/`)** : Lancez `start_extractor.bat` (Windows) ou `./start_extractor.sh` (macOS / Linux) pour convertir vos personnages MUGEN (`chars/`) en fichiers `.fgt` + `index.txt` à copier dans `DATA:/fighters_32/` ou `DATA:/fighters_64/`.
- **Blueprints Home Assistant (`ArcadeMatrix-tools/home_assistant/blueprints/`)** : Automatisations YAML prêtes à importer pour envoyer capteurs, courbes 24h, tableaux et météo vers le moteur `mqttdata`.


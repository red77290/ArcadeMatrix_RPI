🇬🇧 [English](QUICKSTART.md) | 🇫🇷 Français | 🇪🇸 [Español](QUICKSTART_ES.md)

# Guide de démarrage rapide

Ce guide vous aidera à installer et configurer ArcadeMatrix sur votre Raspberry Pi.

## 1. Installation (recommandée)

Nous fournissons une image précompilée prête à l'emploi.

1. Flashez le fichier `ArcadeMatrix_Release.img` sur votre carte SD avec **Raspberry Pi Imager**.
2. Une fois terminé, réinsérez la carte SD dans votre PC/Mac. Un lecteur USB de 8 Go nommé **DATA** apparaîtra.
3. Ouvrez le fichier `config.json` situé sur ce lecteur **DATA** pour y renseigner vos identifiants Wi-Fi (`SSID` et `PASS`) ainsi que la taille de votre matrice.
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

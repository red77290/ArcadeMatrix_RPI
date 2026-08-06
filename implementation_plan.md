# Fix Image Build, Missing Media, Flickering, and Colors

Ce plan résout les 5 problèmes rencontrés sur l'image générée (v2.0.0+) :

## 1. Clignotement de l'écran (Flickering) & Audio
La désactivation de l'audio et l'isolation CPU ne s'appliquaient pas correctement sur l'image Docker générée par la CI, car le script `autoInstall.sh` tourne dans un chroot où `/boot/firmware` n'est pas le vrai noyau.
**Solution** : `docker_builder.sh` va directement modifier `/mnt/rootfs/boot/firmware/config.txt` et `cmdline.txt` avant de fermer l'image, garantissant `dtparam=audio=off` et `isolcpus=3`.

## 2. Polices et GIFs manquants
La partition DATA n'était peut-être pas montée automatiquement au démarrage car le script `chroot_setup.sh` utilisait le `UUID` généré par `blkid` qui peut échouer dans Docker.
**Solution** : Utiliser `LABEL=DATA` dans le `/etc/fstab` de l'image (puisque nous avons formaté avec `mkfs.exfat -L "DATA"`). De plus, s'assurer que les dossiers vides (comme `gifs`) soient bien créés.

## 3. Alias CLI Manquants
L'alias pour redémarrer le service manquait dans la version Rust.
**Solution** : Ajouter un fichier `/home/pi/.bash_aliases` contenant :
- `alias am="sudo systemctl restart arcadematrix"`
- `alias am-log="sudo journalctl -u arcadematrix -f"`

## 4. Inversion des couleurs (rgb_sequence ignoré)
Dans la librairie C++ via le wrapper Rust, la chaîne de caractères `rgb_sequence` (ex: "BGR") est libérée (garbage collected) juste après l'initialisation de la matrice, avant certaines opérations de dessin. Cela cause la lecture d'une mémoire corrompue et le retour à "RGB" par défaut, ignorant les réglages de l'UI.
**Solution** : Modifier `src/core/matrix.rs` pour forcer la conservation en mémoire (memory leak volontaire de 4 octets) du paramètre `rgb_sequence` afin que le pointeur reste valide pendant toute la durée de vie de l'application.

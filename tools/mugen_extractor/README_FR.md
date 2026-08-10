# ArcadeMatrix MUGEN Sprite Extractor

🇬🇧 [English](README.md) | 🇫🇷 Français | 🇪🇸 [Español](README_ES.md)

Ce script Python (`mugen_extractor.py`) a été conçu sur mesure pour extraire, optimiser et convertir des personnages issus du moteur de jeu de combat **MUGEN**, afin de les rendre compatibles avec les moteurs `FighterEngine` d'ArcadeMatrix (versions ESP32 en C++ et Raspberry Pi en Python).

## À quoi ça sert ?

Les jeux de combat (MUGEN en particulier) gèrent les sprites avec des palettes de couleurs compliquées (`.act`, `.sff`) et des scripts d'animation (`.air`) qui incluent des retards variables entre chaque frame, ainsi que des boîtes de collision.

De plus, la taille de la matrice LED est très limitée (ex: 64x32). Les sprites MUGEN originaux sont souvent trop grands et n'ont pas toujours le même alignement d'une animation à l'autre (par exemple, un personnage qui saute aura une image plus grande vers le haut).

Le but de cet outil est de :
1. **Lire les formats MUGEN natifs** (`.sff` v1 et `.air`).
2. **Décoder la palette maître** (pour que les couleurs soient correctes).
3. **Sélectionner uniquement les animations nécessaires** pour ArcadeMatrix (`walk`, `attack`, `hit`, `win`, `special`, `super`, `fall`).
4. **Calculer une échelle uniforme** (Scale) basée sur la hauteur standard du personnage (en position `stand` ou `walk`) pour qu'il tienne dans la hauteur de votre matrice LED (ex: 32 pixels).
5. **Générer un alignement parfait (Virtual Ground)** : L'outil calcule une boîte globale englobante pour s'assurer que la ligne de sol (`ground_y`) et le centre du personnage (`origin_x`) restent parfaitement fixes d'une animation à l'autre. Ainsi, le personnage ne "tremble" pas ou ne change pas de taille lorsqu'il donne un coup !
6. **Convertir en `.fgt` (Fighter Format)** : Le format `.fgt` est un format binaire optimisé créé spécifiquement pour ArcadeMatrix, stockant les pixels en RGB565 avec un code couleur de transparence, prêt à être lu ultra-rapidement par l'ESP32 et le Raspberry Pi.

## Prérequis

Assurez-vous d'avoir Python 3 installé avec la librairie d'images PIL (Pillow) :

```bash
pip install Pillow
```

## Structure du répertoire MUGEN

Le script s'attend à ce que vous fournissiez un dossier source contenant plusieurs sous-dossiers, un par personnage. Chaque personnage doit contenir au minimum ses fichiers `.sff` et `.air`.

Exemple :
```text
/chemin/vers/mugen_chars/
    ├── Ryu/
    │   ├── ryu.sff
    │   ├── ryu.air
    │   └── ryu.def
    ├── Ken/
    │   ├── ken.sff
    │   └── ken.air
    └── ChunLi/
```

## Comment l'utiliser

Exécutez le script avec des arguments en ligne de commande - inutile de modifier le moindre code :

```bash
python mugen_extractor.py --src /Chemin/Vers/Vos/Personnages/Mugen/chars --dest ./fighters_32
```

Options :
| Option | Alias court | Défaut | Description |
|---|---|---|---|
| `--src` | `-i` | *(obligatoire)* | Dossier contenant vos sous-dossiers de personnages MUGEN. |
| `--dest` | `-o` | `./fighters_32` | Dossier de sortie pour les fichiers `.fgt` générés + `index.json`/`index.txt`. |
| `--mode` | | `FULLSIZE` | `SCALED` redimensionne les personnages pour occuper exactement la hauteur du panneau (ESP32 standard, sans PSRAM) ; `FULLSIZE` conserve l'échelle 1:1 (RPi ou ESP32-S3 avec PSRAM - voir `docs/HARDWARE_FR.md`). |
| `--scale` | `--scaling` | `None` | Facteur d'échelle manuel (ex: `0.5` pour diviser les sprites par 2 et économiser 75%% de RAM, `0.8`, `2.0`). Surpasse le calcul automatique. |
| `--compress` | | désactivé | Compresse les fichiers `.fgt` en gzip (`.fgt.gz`) - utile sur RPi pour économiser de l'espace disque. |

Pour cibler à la fois une matrice 32px et 64px, exécutez-le simplement deux fois avec des dossiers `--dest` différents :

```bash
python mugen_extractor.py --src /Chemin/Vers/Vos/Personnages/Mugen/chars --dest ./fighters_32
python mugen_extractor.py --src /Chemin/Vers/Vos/Personnages/Mugen/chars --dest ./fighters_64
```

### Alternative : assistant interactif (aucune option en ligne de commande requise)

Si vous préférez ne pas saisir d'options vous-même, `start_extractor.sh` (macOS/Linux) /
`start_extractor.bat` (Windows) créent un environnement virtuel Python local, installent
automatiquement `Pillow`, et vous demandent les dossiers d'entrée/sortie de manière interactive
(ils appellent `mugen_extractor.py -i <entrée> -o <sortie>` pour vous) :

```bash
./start_extractor.sh     # macOS/Linux
start_extractor.bat      # Windows
```

### Processus d'extraction

Le script crée (ou vide) le dossier de sortie unique donné par `--dest`/`-o` (par défaut
`./fighters_32`) - relancez-le deux fois avec des `--dest` différents si vous avez besoin d'un
export 32px ET 64px (voir l'exemple « cibler les deux » ci-dessus). Pour chaque personnage, il
crée un sous-dossier (ex: `fighters_32/Ryu/`) contenant :
- `walk.fgt`
- `attack.fgt`
- `hit.fgt`
- `win.fgt`
- *(et optionnellement `special1.fgt`/`special2.fgt`/`special3.fgt`, `super1.fgt`/`super2.fgt`/`super3.fgt`, et `fall.fgt` - jusqu'à 3 coups spéciaux et 3 super/ultra sont auto-détectés par personnage à partir de leurs IDs d'animation `.air` MUGEN ; ceux non trouvés sont simplement ignorés)*

Il génère également deux fichiers d'index à la racine du dossier d'export, lus par des moteurs différents :
- `index.json` - métadonnées complètes incluant `has_special`/`has_super`/`special_count`/`super_count`. Lu par le moteur **Raspberry Pi** (`engines/fighter.py`), qui utilise ces indicateurs pour choisir parmi toutes les variantes spéciales/super chargées pendant le combat.
- `index.txt` - un CSV plat plus simple (`name,height,ground_y,origin_x,width,head_y`) sans métadonnées spéciales/super. Lu par le moteur **ESP32** (`FighterEngine.cpp`), qui n'a pas besoin de ces indicateurs : il tente simplement de charger un fichier aléatoire `special1`-`special3`/`super1`-`super3` par combat et l'ignore proprement si ce fichier précis n'existe pas pour un personnage donné (économie mémoire - une seule variante spéciale/super reste chargée à la fois sur ESP32, contre les trois sur RPi).

Les deux fichiers d'index contiennent toujours les métadonnées de positionnement partagées (`height`, `ground_y`, `origin_x`, `width`, `head_y`) nécessaires aux deux moteurs pour aligner correctement les combattants sur la matrice.

## Pourquoi les personnages ignoraient la ligne de sol auparavant ?

Auparavant, chaque animation (`walk`, `attack`) était redimensionnée de manière isolée en recadrant les pixels transparents. Résultat : une attaque haute rendait l'image de l'attaque plus grande que celle de la marche, ce qui modifiait l'échelle et décalait le personnage vers le bas.

Avec cette version **v4**, le script effectue deux passages :
1. Il mesure les proportions maximales globales du personnage sur toutes ses animations confondues.
2. Il applique un ratio d'échelle strict basé uniquement sur son animation de marche/repos.
3. Il dessine toutes les frames sur un "Canvas" global de taille fixe (ex: 48x48), afin que l'axe des pieds du personnage tombe toujours sur le pixel exact `ground_y`. Les moteurs lisent cette valeur `ground_y` pour les aligner ensemble !

---
*Ce script est open source et conçu pour l'écosystème ArcadeMatrix.*

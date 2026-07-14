# ArcadeMatrix MUGEN Sprite Extractor

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

Dans le script `mugen_extractor.py`, descendez tout en bas dans la section `if __name__ == "__main__":` et modifiez les chemins d'accès selon votre configuration :

```python
if __name__ == "__main__":
    # 1. Dossier contenant les personnages MUGEN
    src_dir = "/Chemin/Vers/Vos/Personnages/Mugen/chars"
    
    # 2. Dossiers de destination et hauteurs cibles (TARGET_HEIGHT)
    out_dirs = [
        ("./fighters_32", 32), # Pour matrice P64x32
        ("./fighters_64", 64)  # Pour matrice P128x64 ou P64x64
    ]
```

Puis exécutez le script :

```bash
python mugen_extractor.py
```

### Processus d'extraction

Le script va créer (ou vider) les dossiers `fighters_32` et `fighters_64`. Pour chaque personnage, il va créer un sous-dossier (ex: `fighters_32/Ryu/`) contenant :
- `walk.fgt`
- `attack.fgt`
- `hit.fgt`
- `win.fgt`
- *(et optionnellement `special1.fgt`, `super1.fgt`, `fall.fgt` s'ils sont trouvés)*

Il génère également deux fichiers d'index à la racine du dossier d'export :
- `index.json`
- `index.txt`

Ces fichiers d'index contiennent les métadonnées (Hauteur, `ground_y`, `origin_x`, etc.) nécessaires aux moteurs de rendu de l'ArcadeMatrix pour positionner correctement les combattants sur la matrice.

## Pourquoi les personnages ignoraient la ligne de sol auparavant ?

Auparavant, chaque animation (`walk`, `attack`) était redimensionnée de manière isolée en recadrant les pixels transparents. Résultat : une attaque haute rendait l'image de l'attaque plus grande que celle de la marche, ce qui modifiait l'échelle et décalait le personnage vers le bas.

Avec cette version **v4**, le script effectue deux passages :
1. Il mesure les proportions maximales globales du personnage sur toutes ses animations confondues.
2. Il applique un ratio d'échelle strict basé uniquement sur son animation de marche/repos.
3. Il dessine toutes les frames sur un "Canvas" global de taille fixe (ex: 48x48), afin que l'axe des pieds du personnage tombe toujours sur le pixel exact `ground_y`. Les moteurs lisent cette valeur `ground_y` pour les aligner ensemble !

---
*Ce script est open source et conçu pour l'écosystème ArcadeMatrix.*

# Architecture & Guide Développeur — MUGEN Sprite Extractor

Ce document détaille l'architecture logicielle, le fonctionnement interne des parsers, les spécifications des formats de fichiers et les pistes d'amélioration pour le script `tools/mugen_extractor/mugen_extractor.py`.

---

## 1. Objectif & Vue d'ensemble

Le script **`mugen_extractor.py`** a pour mission de convertir des personnages conçus pour le moteur de jeu de combat **Elecbyte M.U.G.E.N** en animations binaires optimisées (`.fgt` / `.fgt.gz`) pour les moteurs de combat d'**ArcadeMatrix** (ESP32 C++ et Raspberry Pi Rust/Python).

### Défis techniques résolus :
1. **Hétérogénéité des formats MUGEN :** Formats propriétaires binaires (`.sff` v1), scripts d'animation (`.air`), scripts d'états (`.cns` / `.st`) et palettes indexées (`.act`).
2. **Harmonisation des palettes :** Les auteurs MUGEN ont souvent exporté des sprites avec des palettes internes corrompues ou "dummy", s'appuyant sur le remapping dynamique du moteur MUGEN via les fichiers `.act`.
3. **Alignement spatial inter-animations (Virtual Ground) :** Empêcher le personnage de "sautiller" ou de changer de centre de gravité entre une posture d'attente, une marche ou un coup de pied sauté.
4. **Contraintes mémoires embarquées :** Rendre les sprites légers et pré-calculés en format RGB565 natif avec canal de transparence direct.

---

## 2. Pipeline de Traitement Global

```
                     +----------------------------------+
                     | Dossier du Personnage MUGEN      |
                     +----------------------------------+
                                       |
                   +-------------------+-------------------+
                   |                   |                   |
                   v                   v                   v
            [ DefParser ]       [ CnsParser ]       [ SFFv1Parser ]
                   |                   |                   |
     - Trouve sprite/anim/cns   - [Size] (head, scale) - Décode subheaders
     - Résout pal.defaults      - [Statedef] anims     - Cache PCX data
                   |                   |                   |
                   +-------------------+-------------------+
                                       |
                                       v
                                [ AirParser ]
                   - Actions & Frames ([Begin Action])
                   - Offsets relatifs (ox, oy)
                   - Flips graphiques (H, V, HV)
                                       |
                                       v
                         [ resolve_master_palette() ]
                   - Scoring heuristique (score_palette)
                   - Pénalité noir > 15% / néon > 15%
                   - Priorité pal.defaults
                                       |
                   +-------------------+-------------------+
                   |                                       |
                   v                                       v
            [ Passe 1 : Géométrie ]                [ Passe 2 : Rendu ]
     - Bounding Box globale (orig_w, orig_h) - Application palette maître
     - Calcul ground_y, origin_x, head_y     - Redimensionnement Nearest
     - Calcul de l'échelle (scale)           - Encodage RGB565 binaire (.fgt)
```

---

## 3. Spécifications des Formats MUGEN Décodés

### 3.1. Fichier de Définition (`.def`) — `DefParser`
Le fichier `.def` est le point d'entrée du personnage. Il déclare les associations de fichiers :

* **`[Info]` :**
  * `pal.defaults = 1, 2, ...` : Ordre officiel de préférence des palettes choisi par l'auteur.
* **`[Files]` :**
  * `sprite = <nom>.sff` : Fichier de sprites officiel.
  * `anim = <nom>.air` : Fichier d'animations officiel.
  * `cns = <nom>.cns`, `st = <nom>.cns`, `st1..st10 = ...` : Scripts de constantes et d'états.
  * `pal1` à `pal12 = <nom>.act` : Mapping des 12 palettes de couleurs.

### 3.2. Fichier d'États et Constantes (`.cns` / `.st`) — `CnsParser`
Le parser analyse deux éléments clés :
1. **`[Size]` :**
   * `head.pos = X, Y` : Coordonnée Y de la tête relative au sol (valeur négative, ex: `-90`).
   * `xscale`, `yscale` : Facteurs d'échelle officiels (ex: `0.5` pour les sprites Hi-Res, `2.0` pour les sprites rétro).
2. **`[Statedef <ID>]` :**
   * MUGEN standardise les identifiants d'états :
     * `0` : Stand
     * `20`, `21` : Walk Forward / Walk Back
     * `200..999` : Attaques normales
     * `5000..5020` : Réaction aux coups (Hit)
     * `5030..5150` : Chutes / K.O. (Fall)
     * `180..199` : Victoire (Win) / Provocation (Taunt)
     * `1000..2999` : Attaques spéciales
     * `3000..4999` : Attaques supers
   * `CnsParser` extrait la ligne `anim = <ID>` ou `[State ..., ...] type = ChangeAnim` $\rightarrow$ `value = <ID>`.

### 3.3. Fichier de Sprites SFFv1 (`.sff`) — `SFFv1Parser`
* **Header global (512 octets) :**
  * `signature` : `ElecbyteSpr\0` (12 octets)
  * `num_images` (uint32 à l'offset 20)
  * `first_offset` (uint32 à l'offset 24)
* **Subheader par image (32 octets) :**
  * `next_offset` (uint32, 4o)
  * `data_length` (uint32, 4o)
  * `x`, `y` (int16, 4o) : Axe d'alignement du sprite par rapport au point d'origine
  * `group`, `image` (uint16, 4o) : Clé d'identification du sprite `(grp, img)`
  * `prev_copy` (uint16, 2o)
  * `same_pal` (uint8, 1o)
* **Données PCX :**
  * Image encodée en 8-bit RLE PCX.
  * Les 768 derniers octets contiennent la palette locale VGA 256 couleurs (si `data_length > 768`).

### 3.4. Fichier d'Animations AIR (`.air`) — `AirParser`
Chaque bloc commence par `[Begin Action <ID>]`. Chaque ligne de frame suit le format standard Elecbyte :
```text
grp, img, ox, oy, delay, [flip], [blend]
```
* `ox`, `oy` : Décalages relatifs en pixels ajoutés à l'axe du sprite (`total_ox = sff_x - air_ox`).
* `delay` : Durée d'affichage en ticks (1 tick = 1/60e de seconde, `-1` = boucle infinie).
* `flip` : Flags de retournement (`H` pour horizontal, `V` pour vertical, `HV` pour les deux).
* `blend` : Mode de transparence (`A` = Additif, `S` = Soustractif).

---

## 4. Spécification du Format Binaire `.fgt` (ArcadeMatrix Fighter Format)

Le format `.fgt` est un format d'animation compact et streamable conçu pour minimiser le coût CPU et mémoire sur microcontrôleur :

### Structure du fichier binaire :

| Offset | Taille | Type | Description |
|---|---|---|---|
| `0x00` | 3 octets | ASCII | Magic Bytes : `FGT` |
| `0x03` | 1 octet | uint8 | Version du format (`1`) |
| `0x04` | 2 octets | uint16 LE | Largeur du Canvas (`canvas_w`) |
| `0x06` | 2 octets | uint16 LE | Hauteur du Canvas (`canvas_h`) |
| `0x08` | 2 octets | uint16 LE | Nombre de frames (`num_frames`) |
| `0x0A` | 2 octets | uint16 LE | Couleur de transparence RGB565 (`0x0000`) |
| `0x0C` | `2 * num_frames` | uint16 LE[] | Tableau des délais de chaque frame (en ticks) |
| `0x0C + (2*N)` | `N * W * H * 2` | uint16 LE[] | Flux continu des pixels RGB565 pour chaque frame |

> **Note sur la compression :** L'option `--compress` génère des fichiers `.fgt.gz` via gzip standard, particulièrement adaptés au stockage sur Raspberry Pi ou cartes SD.

---

## 5. Algorithme de Résolution des Palettes (`resolve_master_palette`)

Pour éviter les sprites noirs ou néon générés par les palettes "dummy" des créateurs MUGEN (ou à l'inverse les fichiers `.act` déclarés corrompus/vides) :

1. **Sélection du Sprite de Référence Corps :**
   * Parcourt les frames du corps (groupes clés `0`, `1`, `5`, `10`, `20`, `21`, `40`, `100`, `200`, `5000`).
   * Sélectionne la frame possédant le plus grand nombre d'indices de pixels distincts (pour une évaluation optimale).
   * Exclut systématiquement le groupe `9000` (portraits / icônes de sélection) afin d'éviter toute contamination.

2. **Collecte Multi-Candidats :**
   * **Candidats `.def` :** Palettes déclarées dans `[Files]` (`pal1..pal12`), avec priorité absolue aux slots déclarés dans `pal.defaults` de l'auteur.
   * **Candidats `SFFv1` :** Palette intégrée au sprite de référence, palette de garde `(0,0)`, première palette du SFF et sous-palettes locales du SFF.
   * **Candidats `.act` :** Fichiers `.act` additionnels présents dans le dossier du personnage.

3. **Fonction d'Évaluation & Filtrage (`score_palette`) :**
   * **Rejet des palettes monochromes :** Si la palette ne génère qu'une seule nuance de couleur (`u_colors <= 1`) alors que le sprite a plusieurs indices, la palette est rejetée (`score = -999.0`).
   * **Rejet du masquage néon :** Si plus de 25% des pixels visibles sont des couleurs pures saturées de masquage (cyan/magenta/vert vif), la palette est rejetée.
   * **Calcul du Score :**
     $$\text{Base Score} = \text{Couleurs Uniques} \times 10$$
     $$\text{Bonus Luminance Naturelle} = +100 \quad \text{si } 25 \le \text{Luminance Moyenne} \le 200$$
     $$\text{Pénalité Sous/Sur-exposition} = -30 \text{ (si } L < 15 \text{)}, \quad -80 \text{ (si } L > 225 \text{)}$$
   * **Bonus Auteur / Source :**
     * `DEF(pal.defaults)` : **+50 pts**
     * `SFF(sprite_corps)` : **+40 pts**
     * `SFF(stand)` : **+35 pts**
     * `DEF(pal1..12)` : **+30 pts**
     * `SFF(first)` : **+30 pts**
     * `SFF(local)` : **+20 pts**
     * `ACT(dossier)` : **+10 pts**

---

## 6. Guide pour les Développeurs : Comment Contribuer

### 6.1. Ajouter le support du format SFFv2 (MUGEN 1.0 / 1.1)
Le format SFFv2 utilise une structure différente basée sur des blocs LZO, RLE8 ou PNG compressés :
* Implémenter une classe `SFFv2Parser`.
* Détecter la signature dans le header : `ElecbyteSpr\x00` avec version `0x02, 0x00, 0x00, 0x02`.
* Décompresser les sous-blocs LZO / Zlib vers le même dictionnaire d'images en mémoire `self.images[(grp, img)] = {'x': x, 'y': y, 'data': raw_rgba_or_indexed}`.

### 6.2. Supporter l'Alpha Blending (`A`, `S`, `ASxxxDxxx`)
Actuellement, les pixels avec alpha < 128 sont écrits comme transparents (`0x0000`).
* Dans la passe de rendu (`Pass 2`), lire le flag `fr.get('blend')`.
* Pour un mode additif (`A`), convertir les pixels semi-transparents avec un masque spécifique ou pré-mélanger avec un fond sombre.

### 6.3. Ajouter un Mode Palette Hybride (FX / Projectiles séparés)
Si un projectile ou une flamme utilise une palette distincte du corps du combattant :
* Calculer le `score_palette` de la palette locale du PCX pour cette frame spécifique.
* Si le score local est élevé (> 30.0) et correspond à un groupe FX (1000+), utiliser la palette locale au lieu de la `master_palette`.

---

## 7. Commandes de Validation & Tests

Pour tester vos modifications sur un ensemble de personnages de référence :

```bash
# Test interactif guidé
./start_extractor.sh

# Test direct en ligne de commande
python3 mugen_extractor.py --src "/chemin/vers/chars" --dest "./test_out" --mode SCALED --workers 4
```

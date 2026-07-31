🇬🇧 [English](DEVELOPER.md) | 🇫🇷 Français | 🇪🇸 [Español](DEVELOPER_ES.md)

# Guide développeur

Bienvenue dans le guide de développement d'ArcadeMatrix. Ce document explique l'architecture principale du projet et fournit des instructions pas à pas sur la manière de l'étendre.

## Architecture : Renderers vs. Clocks

Depuis le refactor majeur, ArcadeMatrix sépare strictement **l'esthétique visuelle (Renderers)** de la **logique comportementale (Clocks)**. Comprendre cette différence est essentiel avant de commencer à coder.

### 1. Renderers (le « thème »)
Situés dans `engines/renderers/`.
Un **Renderer** (p. ex. `CyberpunkRenderer`, `FlipRenderer`) est purement esthétique. Il ne se soucie pas de savoir s'il affiche l'heure, la date ou la météo. Il prend une chaîne de texte, une police, puis la dessine sur un fond stylisé ou un effet visuel.
- **Responsabilité :** arrière-plans, couleurs, effets de particules, animations de transition.
- **Avantage :** fortement réutilisable entre différents Engines (`ClockEngine`, `DateEngine`, etc.).

### 2. Specialized Clocks (le « mini-jeu »)
Situés dans `engines/clocks/`.
Un **Specialized Clock** (p. ex. `PongClock`, `TetrisClock`, `PacManClock`) est un moteur de logique dynamique. Il gère un état interne (comme une balle qui rebondit ou des blocs qui tombent) pour construire visuellement l'affichage de l'heure.
- **Responsabilité :** état du jeu, physique, dessin des sprites et génération visuelle de l'heure au lieu d'écrire simplement une chaîne.
- **Avantage :** totalement autonome et permet des v## Étendre la base de code Rust

*Note : ArcadeMatrix a été récemment réécrit en Rust. Les tutoriels pour développeurs concernant l'ajout de Renderers, Clocks et Engines sont en cours de mise à jour pour refléter la nouvelle architecture Rust (`src/engines/`). En attendant, vous pouvez inspecter les implémentations existantes dans `src/engines/renderers` pour voir comment le trait `Renderer` est implémenté.*

---

## Tutoriel 1 : créer un nouveau Renderer

Si vous voulez ajouter un nouvel arrière-plan générique ou un nouvel effet visuel (comme un thème « Synthwave ») pouvant être utilisé à la fois pour l'heure et la date :

1. **Créer le fichier :**
   [Code placeholder]

2. **Sous-classer BaseRenderer :**
   [Code placeholder]

3. **Enregistrer le Renderer :**
   Ouvrez `engines/renderers/__init__.py` et associez un nouveau `theme_id` à `SynthwaveRenderer` dans la fonction `get_renderer`.

---

## Tutoriel 2 : créer une nouvelle Specialized Clock

Si vous voulez créer une horloge complexe qui joue à un jeu ou construit l'heure bloc par bloc (comme une horloge « Snake ») :

1. **Créer le fichier :**
   [Code placeholder]

2. **Implémenter la logique :**
   Une Specialized Clock n'hérite pas d'une classe de base, mais elle DOIT exposer une méthode `tick()`.
   [Code placeholder]

3. **Enregistrer l'horloge :**
   Ouvrez `engines/clock.py`. Instanciez votre horloge dans `__init__()`, puis ajoutez une condition `elif` dans la boucle de la méthode `run()` pour rediriger un `theme_id` spécifique vers votre méthode `snake_clock.tick(...)`.

---

## Tutoriel 3 : créer un nouvel élément de screensaver (Engine)

Si vous voulez ajouter un module totalement nouveau à la rotation idle (p. ex. un suivi du prix des cryptos), vous devez créer un **Engine** complet.

1. **Créer le fichier Engine :**
   [Code placeholder]

2. **Enregistrer l'Engine dans la rotation :**
   Ouvrez `src/core/rotation.rs`.
   - Importez votre engine en haut : `from engines.crypto import CryptoEngine`
   - Ajoutez-le au dictionnaire `self.engines` dans `__init__` ainsi que dans le bloc de recréation `reload_flag`.
   - Associez sa durée dans le bloc d'exécution `run()` :
     ```python
     elif engine_name == 'crypto':
         engine.run(86400 if is_single else 10) # 10 seconds default
     ```

3. **Mettre à jour l'UI & la configuration :**
   - Mettez à jour `src/api/server.rs` pour accepter `'crypto'` dans le tableau de rotation.
   - Mettez à jour `api/www/index.html` pour ajouter un `<div class="feature-item" data-id="crypto">Crypto Tracker</div>` afin que les utilisateurs puissent le glisser-déposer dans leur rotation active.

---

## Intégration API & Web UI

Chaque fois que vous créez un nouveau thème ou une nouvelle horloge :
1. Mettez à jour `src/api/server.rs` si votre nouvelle fonctionnalité nécessite de nouveaux réglages.
2. Mettez à jour `api/www/index.html` pour ajouter votre nouvel identifiant de thème aux menus déroulants (`<select id="time_theme">`).

### ⚠️ Le code source du frontend n'est pas dans ce dépôt

`api/www/` ne contient que le dashboard **compilé/bundlé** (`index.html`, `assets/index-*.js`,
`assets/index-*.css` — un build Vite minifié, JS/HTML/CSS pur, **pas** Vue.js malgré l'ancienne
documentation qui l'affirmait). Aucun `package.json`, aucun code source de composants et aucune
config Vite ne sont versionnés ici ; le bundle **ne peut donc pas être reconstruit ni modifié de
manière pertinente** à partir de ce dépôt seul — uniquement édité à la main dans la sortie déjà
minifiée, ce qui ne passe pas à l'échelle au-delà de petits ajustements (comme les entrées du menu
déroulant de thèmes mentionnées plus haut).

Si vous devez apporter des modifications substantielles à l'UI, vous avez deux options :
1. Retrouver l'emplacement du projet source frontend d'origine (s'il existe encore) et le
   réintégrer dans ce dépôt, par exemple dans un nouveau dossier `frontend/`, avec une étape de
   build qui sort dans `api/www/`.
2. Reconstruire un petit projet frontend depuis zéro contre l'API REST existante (voir
   `src/api/server.rs` pour la liste complète des routes) si la source originale est réellement perdue.

Dans tous les cas, **ne continuez pas discrètement à ne livrer qu'un bundle compilé sans source de
vérité documentée** — si vous retrouvez/restaurez la source, committez-la et documentez ici la
commande de build.

## Tester Votre Code

Nous appliquons une couverture de test à 100% sur les routes de l'API. Pour vérifier votre code :
```bash
cargo test
```

## Flux de Développement Local Rapide (Cross-Compilation)

Pour itérer rapidement, vous n'avez pas besoin de reconstruire l'intégralité du fichier `.img` de 14 Go ni de compiler directement sur le Raspberry Pi. ArcadeMatrix inclut des scripts de compilation croisée qui fonctionnent sur n'importe quel système d'exploitation hôte (Windows, Linux, macOS) tant que Docker est installé.

### 1. Construire le Binaire
Cette commande lance un conteneur Docker Rust léger, installe la toolchain de cross-compilation ARM64, et compile nativement votre code Rust en quelques secondes. Le binaire résultant est enregistré dans `target/aarch64-unknown-linux-gnu/release/arcadematrix`.
```bash
bash scripts/build_local.sh
```

### 2. Déployer sur le Raspberry Pi
Cette commande utilise `scp` et `ssh` pour pousser directement le nouveau binaire compilé sur votre Raspberry Pi et redémarre le service systemd.
```bash
bash scripts/deploy_to_pi.sh pi@<VOTRE_ADRESSE_IP_PI>
```

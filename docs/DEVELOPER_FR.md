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
- **Avantage :** totalement autonome et permet des visualisations très complexes, frame par frame.

---

## Tutoriel 1 : créer un nouveau Renderer

Si vous voulez ajouter un nouvel arrière-plan générique ou un nouvel effet visuel (comme un thème « Synthwave ») pouvant être utilisé à la fois pour l'heure et la date :

1. **Créer le fichier :**
   Créez un fichier `engines/renderers/synthwave_renderer.py`.

2. **Sous-classer BaseRenderer :**
   ```python
   from .base_renderer import BaseRenderer
   from core.theme import draw_styled_text
   from PIL import ImageDraw

   class SynthwaveRenderer(BaseRenderer):
       def __init__(self, config):
           super().__init__(config)
           # Initialize your persistent state here (e.g., grid positions)

       def render(self, img, text, font, theme_id, color1, color2, offset_x, offset_y, scale_factor=1.0):
           draw = ImageDraw.Draw(img)
           # 1. Draw your Synthwave sun and grid background
           draw.rectangle([0, 0, img.width, img.height], fill=(20, 0, 40))

           # 2. Draw the text on top using the base renderer logic
           return super().render(img, text, font, theme_id, color1, color2, offset_x, offset_y, scale_factor)
   ```

3. **Enregistrer le Renderer :**
   Ouvrez `engines/renderers/__init__.py` et associez un nouveau `theme_id` à `SynthwaveRenderer` dans la fonction `get_renderer`.

---

## Tutoriel 2 : créer une nouvelle Specialized Clock

Si vous voulez créer une horloge complexe qui joue à un jeu ou construit l'heure bloc par bloc (comme une horloge « Snake ») :

1. **Créer le fichier :**
   Créez `engines/clocks/snake_clock.py`.

2. **Implémenter la logique :**
   Une Specialized Clock n'hérite pas d'une classe de base, mais elle DOIT exposer une méthode `tick()`.
   ```python
   from PIL import Image, ImageDraw

   class SnakeClock:
       def __init__(self, width, height):
           self.w = width
           self.h = height
           self.snake_pos = []
           self.target_time = ""

       def tick(self, img, time_str, font, color1, color2, scale_factor=1.0):
           draw = ImageDraw.Draw(img)

           # Update snake logic based on time_str
           if time_str != self.target_time:
               self.target_time = time_str
               # spawn new food, etc.

           # Draw snake
           # ...

           # Draw the time string
           # ...

           return img
   ```

3. **Enregistrer l'horloge :**
   Ouvrez `engines/clock.py`. Instanciez votre horloge dans `__init__()`, puis ajoutez une condition `elif` dans la boucle de la méthode `run()` pour rediriger un `theme_id` spécifique vers votre méthode `snake_clock.tick(...)`.

---

## Tutoriel 3 : créer un nouvel élément de screensaver (Engine)

Si vous voulez ajouter un module totalement nouveau à la rotation idle (p. ex. un suivi du prix des cryptos), vous devez créer un **Engine** complet.

1. **Créer le fichier Engine :**
   Créez `engines/crypto.py`.

   ```python
   import time
   from PIL import Image, ImageDraw
   from core.theme import load_font

   class CryptoEngine:
       def __init__(self, matrix_wrapper, config, fighter_engine=None):
           self.mw = matrix_wrapper
           self.config = config
           self.fighter_engine = fighter_engine

       def run(self, duration_sec):
           start_time = time.time()
           canvas = self.mw.get_canvas()
           font = load_font("04B_03.ttf", 16)

           while time.time() - start_time < duration_sec:
               if getattr(self.config, 'reload_flag', False):
                   break

               # 1. Fetch your data
               price = "$65,000"

               # 2. Draw your canvas
               img = Image.new('RGB', (self.config.matrix_width, self.config.matrix_height), (0, 0, 0))
               draw = ImageDraw.Draw(img)
               draw.text((0, 0), f"BTC:\n{price}", font=font, fill=(255, 200, 0))

               # 3. Add Fighters overlay if enabled
               if self.fighter_engine:
                   img = self.fighter_engine.tick(img)

               # 4. Push to hardware
               canvas.SetImage(img)
               canvas = self.mw.swap_canvas(canvas)

               time.sleep(1) # Loop speed
   ```

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

## Tester votre code

Nous imposons une couverture de test de 100 % sur les routes API. Pour vérifier votre code :
```bash
cargo test
```

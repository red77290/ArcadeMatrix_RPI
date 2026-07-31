🇬🇧 English | 🇫🇷 [Français](DEVELOPER_FR.md) | 🇪🇸 [Español](DEVELOPER_ES.md)

# Developer Guide

Welcome to the ArcadeMatrix development guide. This document explains the core architecture of the project and provides step-by-step instructions on how to extend it.

## Architecture: Renderers vs. Clocks

Since the major refactoring, ArcadeMatrix strictly separates the **visual aesthetics (Renderers)** from the **behavioral logic (Clocks)**. Understanding this difference is crucial before you start coding.

### 1. Renderers (The "Theme")
Located in `engines/renderers/`.
A **Renderer** (e.g., `CyberpunkRenderer`, `FlipRenderer`) is purely aesthetic. It doesn't care if it's displaying the time, the date, or the weather. It takes a text string, a font, and draws it on top of a styled background or visual effect.
- **Responsibility:** Backgrounds, colors, particle effects, transition animations.
- **Advantage:** Highly reusable across different Engines (`ClockEngine`, `DateEngine`, etc.).

### 2. Specialized Clocks (The "Mini-Game")
Located in `engines/clocks/`.
A **Specialized Clock** (e.g., `PongClock`, `TetrisClock`, `PacManClock`) is a dynamic logic engine. It manages an internal state (like a ball bouncing or blocks falling) to construct the time display visually.
- **Responsibility:** Game state, physics, sprite drawing, and generating the time visually rather than just writing a string.
- **Advantage:** Completely autonomous and allows for highly complex, frame-by-frame visualizations.

---

## Tutorial 1: Creating a New Renderer

If you want to add a new generic background or visual effect (like a "Synthwave" theme) that can be used for both Time and Date:

1. **Create the File**:
   Create a file `engines/renderers/synthwave_renderer.py`.

2. **Subclass BaseRenderer**:
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

3. **Register the Renderer**:
   Open `engines/renderers/__init__.py` and map a new `theme_id` to `SynthwaveRenderer` inside the `get_renderer` function.

---

## Tutorial 2: Creating a New Specialized Clock

If you want to create a complex clock that plays a game or builds the time block by block (like a "Snake" clock):

1. **Create the File**:
   Create `engines/clocks/snake_clock.py`.

2. **Implement the Logic**:
   A Specialized Clock doesn't inherit from a base class, but it MUST expose a `tick()` method.
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

3. **Register the Clock**:
   Open `engines/clock.py`. Instantiate your clock in `__init__()`, and add an `elif` condition in the `run()` method loop to route a specific `theme_id` to your `snake_clock.tick(...)` method.

---

## Tutorial 3: Creating a New Screensaver Element (Engine)

If you want to add a completely new module to the idle rotation (e.g., a Crypto price tracker), you need to create a full **Engine**.

1. **Create the Engine File**:
   Create `engines/crypto.py`.

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

2. **Register the Engine in Rotation**:
   Open `src/core/rotation.rs`.
   - Import your engine at the top: `from engines.crypto import CryptoEngine`
   - Add it to the `self.engines` dictionary inside `__init__` and inside the `reload_flag` recreation block.
   - Map its duration in the `run()` execution block:
     ```python
     elif engine_name == 'crypto':
         engine.run(86400 if is_single else 10) # 10 seconds default
     ```

3. **Update UI & Configuration**:
   - Update `src/api/server.rs` to accept `'crypto'` in the rotation array.
   - Update `api/www/index.html` to add a `<div class="feature-item" data-id="crypto">Crypto Tracker</div>` so users can drag-and-drop it into their active rotation.

---

## API & Web UI Integration

Whenever you create a new theme or clock:
1. Update `src/api/server.rs` if your new feature requires new settings.
2. Update `api/www/index.html` to add your new Theme ID to the dropdown menus (`<select id="time_theme">`).

### ⚠️ Frontend source is not in this repository

`api/www/` only contains the **built/bundled** dashboard (`index.html`, `assets/index-*.js`,
`assets/index-*.css` - a minified Vite build, plain JS/HTML/CSS, **not** Vue.js despite older
documentation claiming otherwise). There is no `package.json`, no component sources, and no Vite
config committed here, so the bundle **cannot be rebuilt or meaningfully modified** from this repo
alone - only hand-edited in the already-minified output, which doesn't scale for anything beyond
trivial tweaks (like the theme dropdown entries mentioned above).

If you need to make substantial UI changes, you have two options:
1. Track down wherever the original frontend source project lives (if it still exists) and add it
   back into this repo, e.g. under a new `frontend/` folder, with a build step that outputs into
   `api/www/`.
2. Rebuild a small frontend project from scratch against the existing REST API (see `src/api/server.rs`
   for the full route list) if the original source is truly lost.

Either way, **do not silently keep shipping only a compiled bundle with no documented source of
truth** - if you find/restore the source, commit it and document the build command here.

## Testing Your Code

We enforce a 100% test coverage on API routes. To verify your code:
```bash
cargo test
```

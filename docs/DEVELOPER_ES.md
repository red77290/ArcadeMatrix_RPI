🇬🇧 [English](DEVELOPER.md) | 🇫🇷 [Français](DEVELOPER_FR.md) | 🇪🇸 Español

# Guía del desarrollador

Bienvenido a la guía de desarrollo de ArcadeMatrix. Este documento explica la arquitectura principal del proyecto y proporciona instrucciones paso a paso sobre cómo ampliarlo.

## Arquitectura: Renderers vs. Clocks

Desde la gran refactorización, ArcadeMatrix separa estrictamente **la estética visual (Renderers)** de **la lógica de comportamiento (Clocks)**. Entender esta diferencia es fundamental antes de empezar a programar.

### 1. Renderers (el «tema»)
Ubicados en `engines/renderers/`.
Un **Renderer** (p. ej. `CyberpunkRenderer`, `FlipRenderer`) es puramente estético. No le importa si está mostrando la hora, la fecha o el weather. Toma una cadena de texto, una fuente y la dibuja sobre un fondo estilizado o un efecto visual.
- **Responsabilidad:** fondos, colores, efectos de partículas, animaciones de transición.
- **Ventaja:** altamente reutilizable entre distintos Engines (`ClockEngine`, `DateEngine`, etc.).

### 2. Specialized Clocks (el «mini-juego»)
Ubicados en `engines/clocks/`.
Un **Specialized Clock** (p. ej. `PongClock`, `TetrisClock`, `PacManClock`) es un motor de lógica dinámica. Gestiona un estado interno (como una pelota rebotando o bloques cayendo) para construir visualmente la visualización de la hora.
- **Responsabilidad:** estado del juego, física, dibujo de sprites y generación visual de la hora en lugar de simplemente escribir una cadena.
- **Ventaja:** completamente autónomo y permite visualizaciones muy complejas, frame a frame.

---

## Tutorial 1: crear un nuevo Renderer

Si quieres añadir un nuevo fondo genérico o efecto visual (como un tema «Synthwave») que pueda usarse tanto para Time como para Date:

1. **Crear el archivo:**
   Crea un archivo `engines/renderers/synthwave_renderer.py`.

2. **Heredar de BaseRenderer:**
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

3. **Registrar el Renderer:**
   Abre `engines/renderers/__init__.py` y asigna un nuevo `theme_id` a `SynthwaveRenderer` dentro de la función `get_renderer`.

---

## Tutorial 2: crear un nuevo Specialized Clock

Si quieres crear un reloj complejo que juegue a un juego o construya la hora bloque por bloque (como un reloj «Snake»):

1. **Crear el archivo:**
   Crea `engines/clocks/snake_clock.py`.

2. **Implementar la lógica:**
   Un Specialized Clock no hereda de una clase base, pero DEBE exponer un método `tick()`.
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

3. **Registrar el reloj:**
   Abre `engines/clock.py`. Instancia tu reloj en `__init__()`, y añade una condición `elif` en el bucle del método `run()` para enrutar un `theme_id` específico a tu método `snake_clock.tick(...)`.

---

## Tutorial 3: crear un nuevo elemento de screensaver (Engine)

Si quieres añadir un módulo completamente nuevo a la rotación idle (p. ej. un rastreador del precio de crypto), necesitas crear un **Engine** completo.

1. **Crear el archivo del Engine:**
   Crea `engines/crypto.py`.

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

2. **Registrar el Engine en la rotación:**
   Abre `src/core/rotation.rs`.
   - Importa tu engine al principio: `from engines.crypto import CryptoEngine`
   - Añádelo al diccionario `self.engines` dentro de `__init__` y dentro del bloque de recreación `reload_flag`.
   - Asigna su duración en el bloque de ejecución `run()`:
     ```python
     elif engine_name == 'crypto':
         engine.run(86400 if is_single else 10) # 10 seconds default
     ```

3. **Actualizar UI y configuración:**
   - Actualiza `src/api/server.rs` para aceptar `'crypto'` en el array de rotación.
   - Actualiza `api/www/index.html` para añadir un `<div class="feature-item" data-id="crypto">Crypto Tracker</div>` para que los usuarios puedan arrastrarlo y soltarlo en su rotación activa.

---

## Integración de API y Web UI

Cada vez que crees un tema nuevo o un reloj nuevo:
1. Actualiza `src/api/server.rs` si tu nueva función requiere nuevos ajustes.
2. Actualiza `api/www/index.html` para añadir tu nuevo Theme ID a los menús desplegables (`<select id="time_theme">`).

### ⚠️ El código fuente del frontend no está en este repositorio

`api/www/` solo contiene el dashboard **compilado/empaquetado** (`index.html`, `assets/index-*.js`,
`assets/index-*.css`: una build minificada de Vite, JS/HTML/CSS plano, **no** Vue.js a pesar de que
documentación antigua afirmaba lo contrario). Aquí no hay `package.json`, ni fuentes de componentes,
ni configuración de Vite versionada, por lo que el bundle **no puede reconstruirse ni modificarse
de forma significativa** solo desde este repositorio; únicamente puede editarse a mano sobre la salida
ya minificada, lo que no escala más allá de ajustes triviales (como las entradas del desplegable de
temas mencionadas arriba).

Si necesitas realizar cambios importantes en la UI, tienes dos opciones:
1. Localizar dónde vive el proyecto fuente original del frontend (si todavía existe) y volver a
   integrarlo en este repositorio, por ejemplo dentro de una nueva carpeta `frontend/`, con un paso
   de build que genere la salida en `api/www/`.
2. Reconstruir desde cero un pequeño proyecto frontend contra la API REST existente (consulta
   `src/api/server.rs` para la lista completa de rutas) si la fuente original realmente se ha perdido.

En cualquier caso, **no sigas distribuyendo silenciosamente solo un bundle compilado sin una fuente
de verdad documentada**: si encuentras/restauras la fuente, haz commit de ella y documenta aquí el
comando de build.

## Probar tu código

Exigimos una cobertura de tests del 100 % en las rutas API. Para verificar tu código:
```bash
cargo test
```

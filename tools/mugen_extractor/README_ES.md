# ArcadeMatrix MUGEN Sprite Extractor

Este script de Python (`mugen_extractor.py`) está diseñado a medida para extraer, optimizar y convertir personajes de juegos de lucha del motor **MUGEN** para hacerlos compatibles con el `FighterEngine` de ArcadeMatrix (tanto en la versión C++ de ESP32 como en la de Python de Raspberry Pi).

## ¿Para qué sirve?

Los juegos de lucha (MUGEN en particular) manejan los sprites con paletas de colores complejas (`.act`, `.sff`) y scripts de animación (`.air`) que incluyen retrasos variables entre cada frame (cuadro), así como cajas de colisión.

Además, el tamaño de una matriz LED es muy limitado (ej., 64x32). Los sprites originales de MUGEN suelen ser demasiado grandes y no siempre tienen la misma alineación de una animación a otra (por ejemplo, un personaje saltando tendrá una imagen más grande que se expande hacia arriba).

El objetivo de esta herramienta es:
1. **Leer los formatos nativos de MUGEN** (`.sff` v1 y `.air`).
2. **Decodificar la paleta maestra** (para que los colores sean correctos).
3. **Seleccionar solo las animaciones necesarias** para ArcadeMatrix (`walk`, `attack`, `hit`, `win`, `special`, `super`, `fall`).
4. **Calcular una escala uniforme** basada en la altura estándar del personaje (en posición `stand` o `walk`) para que encajen dentro de la altura de tu matriz LED (ej., 32 píxeles).
5. **Generar una alineación perfecta (Virtual Ground o Suelo Virtual)**: La herramienta calcula una caja de colisión (bounding box) global para asegurar que la línea del suelo (`ground_y`) y el centro del personaje (`origin_x`) permanezcan perfectamente fijos de una animación a otra. ¡Esto evita que el personaje "tiemble" o cambie de tamaño al atacar!
6. **Convertir a `.fgt` (Formato Fighter)**: El formato `.fgt` es un formato binario optimizado creado específicamente para ArcadeMatrix, que almacena píxeles en RGB565 con un código de color transparente, listo para ser leído de forma ultrarrápida por el ESP32 y la Raspberry Pi.

## Requisitos previos

Asegúrate de tener Python 3 instalado junto con la biblioteca de imágenes PIL (Pillow):

```bash
pip install Pillow
```

## Estructura del Directorio MUGEN

El script espera que proporciones una carpeta de origen que contenga varias subcarpetas, una por personaje. Cada personaje debe contener al menos sus archivos `.sff` y `.air`.

Ejemplo:
```text
/ruta/a/mugen_chars/
    ├── Ryu/
    │   ├── ryu.sff
    │   ├── ryu.air
    │   └── ryu.def
    ├── Ken/
    │   ├── ken.sff
    │   └── ken.air
    └── ChunLi/
```

## Cómo usarlo

En el script `mugen_extractor.py`, desplázate hasta el final de la sección `if __name__ == "__main__":` y modifica las rutas según tu configuración:

```python
if __name__ == "__main__":
    # 1. Carpeta que contiene los personajes de MUGEN
    src_dir = "/Ruta/A/Tus/Personajes/Mugen/chars"
    
    # 2. Carpetas de destino y alturas objetivo (TARGET_HEIGHT)
    out_dirs = [
        ("./fighters_32", 32), # Para matriz P64x32
        ("./fighters_64", 64)  # Para matriz P128x64 o P64x64
    ]
```

Luego ejecuta el script:

```bash
python mugen_extractor.py
```

### Proceso de Extracción

El script creará (o vaciará) las carpetas `fighters_32` y `fighters_64`. Por cada personaje, creará una subcarpeta (ej., `fighters_32/Ryu/`) que contendrá:
- `walk.fgt`
- `attack.fgt`
- `hit.fgt`
- `win.fgt`
- *(y opcionalmente `special1.fgt`, `super1.fgt`, `fall.fgt` si se encuentran)*

También genera dos archivos de índice en la raíz de la carpeta de exportación:
- `index.json`
- `index.txt`

Estos archivos de índice contienen los metadatos (Altura, `ground_y`, `origin_x`, etc.) que necesitan los motores de renderizado de ArcadeMatrix para posicionar correctamente a los luchadores en la matriz.

## ¿Por qué los personajes ignoraban la línea del suelo antes?

Anteriormente, cada animación (`walk`, `attack`) se escalaba de forma aislada recortando los píxeles transparentes. Como resultado, un ataque alto hacía que la imagen del ataque fuera más grande que la imagen de caminar, cambiando la escala y desplazando al personaje hacia abajo.

Con esta versión **v4**, el script realiza dos pasadas:
1. Mide las proporciones máximas globales del personaje sumando todas sus animaciones combinadas.
2. Aplica un ratio de escala estricto basado únicamente en su animación de caminar/esperar (walk/idle).
3. Dibuja todos los frames (cuadros) en un "Canvas" global de tamaño fijo (ej., 48x48), para que el eje de los pies del personaje caiga siempre sobre el píxel exacto `ground_y`. ¡Los motores leen este valor `ground_y` para alinearlos a la perfección!

---
*Este script es de código abierto y está diseñado para el ecosistema ArcadeMatrix.*

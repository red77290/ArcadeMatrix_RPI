# ArcadeMatrix MUGEN Sprite Extractor

🇬🇧 [English](README.md) | 🇫🇷 [Français](README_FR.md) | 🇪🇸 Español

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

Ejecuta el script con argumentos de línea de comandos - no hace falta editar ningún código:

```bash
python mugen_extractor.py --src /Ruta/A/Tus/Personajes/Mugen/chars --dest ./fighters_32
```

Opciones:
| Opción | Alias corto | Por defecto | Descripción |
|---|---|---|---|
| `--src` | `-i` | *(obligatorio)* | Carpeta que contiene tus subcarpetas de personajes MUGEN. |
| `--dest` | `-o` | `./fighters_32` | Carpeta de salida para los archivos `.fgt` generados + `index.json`/`index.txt`. |
| `--mode` | | `FULLSIZE` | `SCALED` redimensiona los personajes para ajustarse exactamente a la altura del panel (ESP32 estándar, sin PSRAM); `FULLSIZE` mantiene la escala 1:1 (RPi o ESP32-S3 con PSRAM - ver `docs/HARDWARE_ES.md`). |
| `--scale` | `--scaling` | `None` | Factor de escala personalizado (ej: `0.5` para reducir al 50% ahorrando 75% de RAM, `0.8`, `2.0`). Anula el cálculo automático. |
| `--compress` | | desactivado | Comprime los archivos `.fgt` de salida en gzip (`.fgt.gz`) - útil en RPi para ahorrar espacio en disco. |

Para generar tanto una matriz de 32px como de 64px, simplemente ejecútalo dos veces con carpetas `--dest` diferentes:

```bash
python mugen_extractor.py --src /Ruta/A/Tus/Personajes/Mugen/chars --dest ./fighters_32
python mugen_extractor.py --src /Ruta/A/Tus/Personajes/Mugen/chars --dest ./fighters_64
```

### Alternativa: asistente interactivo (sin necesidad de opciones de línea de comandos)

Si prefieres no escribir las opciones tú mismo, `start_extractor.sh` (macOS/Linux) /
`start_extractor.bat` (Windows) crean un entorno virtual de Python local, instalan `Pillow`
automáticamente, y te piden las carpetas de entrada/salida de forma interactiva (ellos llaman a
`mugen_extractor.py -i <entrada> -o <salida>` por ti):

```bash
./start_extractor.sh     # macOS/Linux
start_extractor.bat      # Windows
```

### Proceso de Extracción

El script crea (o vacía) la carpeta de salida única indicada por `--dest`/`-o` (por defecto
`./fighters_32`) - ejecútalo dos veces con `--dest` diferentes si necesitas una exportación de
32px Y 64px (ver el ejemplo "apuntar a ambos" más arriba). Por cada personaje, crea una subcarpeta
(ej., `fighters_32/Ryu/`) que contiene:
- `walk.fgt`
- `attack.fgt`
- `hit.fgt`
- `win.fgt`
- *(y opcionalmente `special1.fgt`/`special2.fgt`/`special3.fgt`, `super1.fgt`/`super2.fgt`/`super3.fgt`, y `fall.fgt` - hasta 3 movimientos especiales y 3 super/ultra se detectan automáticamente por personaje a partir de sus IDs de animación `.air` de MUGEN; los que no se encuentran simplemente se omiten)*

También genera dos archivos de índice en la raíz de la carpeta de exportación, leídos por motores distintos:
- `index.json` - metadatos completos incluyendo `has_special`/`has_super`/`special_count`/`super_count`. Lo lee el motor de **Raspberry Pi** (`engines/fighter.py`), que usa estos indicadores para elegir entre todas las variantes especiales/super cargadas durante el combate.
- `index.txt` - un CSV plano más simple (`name,height,ground_y,origin_x,width,head_y`) sin metadatos de especiales/super. Lo lee el motor **ESP32** (`FighterEngine.cpp`), que no necesita esos indicadores: simplemente intenta cargar un archivo aleatorio `special1`-`special3`/`super1`-`super3` por combate y lo omite correctamente si ese archivo concreto no existe para un personaje dado (ahorro de memoria - solo se mantiene cargada una variante especial/super a la vez en ESP32, frente a las tres en RPi).

Ambos archivos de índice siempre contienen los metadatos de posicionamiento compartidos (`height`, `ground_y`, `origin_x`, `width`, `head_y`) que necesitan ambos motores para alinear correctamente a los luchadores en la matriz.

## ¿Por qué los personajes ignoraban la línea del suelo antes?

Anteriormente, cada animación (`walk`, `attack`) se escalaba de forma aislada recortando los píxeles transparentes. Como resultado, un ataque alto hacía que la imagen del ataque fuera más grande que la imagen de caminar, cambiando la escala y desplazando al personaje hacia abajo.

Con esta versión **v4**, el script realiza dos pasadas:
1. Mide las proporciones máximas globales del personaje sumando todas sus animaciones combinadas.
2. Aplica un ratio de escala estricto basado únicamente en su animación de caminar/esperar (walk/idle).
3. Dibuja todos los frames (cuadros) en un "Canvas" global de tamaño fijo (ej., 48x48), para que el eje de los pies del personaje caiga siempre sobre el píxel exacto `ground_y`. ¡Los motores leen este valor `ground_y` para alinearlos a la perfección!

---
*Este script es de código abierto y está diseñado para el ecosistema ArcadeMatrix.*

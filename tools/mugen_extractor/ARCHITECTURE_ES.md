# Arquitectura y Guía para Desarrolladores — MUGEN Sprite Extractor

🇬🇧 [English](ARCHITECTURE.md) | 🇫🇷 [Français](ARCHITECTURE_FR.md) | 🇪🇸 Español | 📖 [Guía de Usuario](README_ES.md)

Este documento detalla la arquitectura de software, el funcionamiento interno de los analizadores (parsers), las especificaciones de formatos de archivo y las directrices para desarrolladores del script `tools/mugen_extractor/mugen_extractor.py`.

---

## 1. Objetivo y Visión General

El script **`mugen_extractor.py`** tiene como misión convertir personajes diseñados para el motor de juegos de lucha **Elecbyte M.U.G.E.N** en animaciones binarias optimizadas (`.fgt` / `.fgt.gz`) para los motores de combate de **ArcadeMatrix** (ESP32 en C++ y Raspberry Pi en Rust/Python).

### Desafíos técnicos resueltos:
1. **Heterogeneidad de formatos MUGEN:** Formatos binarios propietarios (`.sff` v1), scripts de animación (`.air`), scripts de estados (`.cns` / `.st`) y paletas indexadas (`.act`).
2. **Armonización de paletas:** Los autores de MUGEN a menudo exportaron sprites con paletas internas corruptas o "dummy", dependiendo del remapeo dinámico del motor a través de archivos `.act`.
3. **Alineación espacial entre animaciones (Virtual Ground):** Evitar que el personaje "salte" o cambie su centro de gravedad entre la postura de espera (stand), la caminata o una patada aérea.
4. **Restricciones de memoria embebida:** Generar sprites ligeros y precalculados en formato nativo RGB565 con canal de transparencia directo.

---

## 2. Pipeline de Procesamiento Global

```
                     +----------------------------------+
                     | Carpeta del Personaje MUGEN      |
                     +----------------------------------+
                                       |
                   +-------------------+-------------------+
                   |                   |                   |
                   v                   v                   v
            [ DefParser ]       [ CnsParser ]       [ SFFv1Parser ]
                   |                   |                   |
     - Busca sprite/anim/cns    - [Size] (head, scale) - Decodifica subheaders
     - Resuelve pal.defaults    - [Statedef] anims     - Cachea datos PCX
                   |                   |                   |
                   +-------------------+-------------------+
                                       |
                                       v
                                [ AirParser ]
                   - Acciones y frames ([Begin Action])
                   - Desplazamientos relativos (ox, oy)
                   - Flips gráficos (H, V, HV)
                                       |
                                       v
                         [ resolve_master_palette() ]
                   - Puntuación heurística (score_palette)
                   - Expansión Modulo Bank (16/32/64c)
                   - Desplazamiento dinámico (Offset Shift)
                   - Prioridad pal.defaults
                                       |
                   +-------------------+-------------------+
                   |                                       |
                   v                                       v
            [ Paso 1: Geometría ]                  [ Paso 2: Renderizado ]
     - Bounding Box global (orig_w, orig_h)  - Aplica paleta maestra
     - Calcula ground_y, origin_x, head_y    - Redimensionamiento Nearest
     - Calcula factor de escala (scale)      - Codificación RGB565 binaria (.fgt)
```

---

## 3. Especificaciones de Formatos MUGEN Decodificados

### 3.1. Archivo de Definición (`.def`) — `DefParser`
El archivo `.def` es el punto de entrada del personaje. Declara las asociaciones de archivos:

* **`[Info]`:**
  * `pal.defaults = 1, 2, ...`: Orden oficial de preferencia de paletas elegido por el autor.
* **`[Files]`:**
  * `sprite = <nombre>.sff`: Archivo oficial de sprites.
  * `anim = <nombre>.air`: Archivo oficial de animaciones.
  * `cns = <nombre>.cns`, `st = <nombre>.cns`, `st1..st10 = ...`: Scripts de constantes y estados.
  * `pal1` a `pal12 = <nombre>.act`: Mapeo de las 12 paletas de colores.

### 3.2. Archivo de Estados y Constantes (`.cns` / `.st`) — `CnsParser`
El analizador procesa dos secciones clave:
1. **`[Size]`:**
   * `head.pos = X, Y`: Coordenada Y de la cabeza relativa al suelo (valor negativo, ej: `-90`).
   * `xscale`, `yscale`: Factores de escala oficiales (ej: `0.5` para sprites Hi-Res, `2.0` para sprites retro).
2. **`[Statedef <ID>]`:**
   * MUGEN estandariza los identificadores de estado:
     * `0`: Stand (Guardia)
     * `20`, `21`: Walk Forward / Walk Back
     * `200..999`: Ataques normales
     * `5000..5020`: Reacción a golpes (Hit)
     * `5030..5150`: Caídas / K.O. (Fall)
     * `180..199`: Victoria (Win) / Provocación (Taunt)
     * `1000..2999`: Ataques especiales
     * `3000..4999`: Ataques supers
   * `CnsParser` extrae la línea `anim = <ID>` o `[State ..., ...] type = ChangeAnim` $\rightarrow$ `value = <ID>`.

### 3.3. Archivo de Sprites SFFv1 (`.sff`) — `SFFv1Parser`
* **Header global (512 bytes):**
  * `signature`: `ElecbyteSpr\0` (12 bytes)
  * `num_images` (uint32 en offset 20)
  * `first_offset` (uint32 en offset 24)
* **Subheader por imagen (32 bytes):**
  * `next_offset` (uint32, 4b)
  * `data_length` (uint32, 4b)
  * `x`, `y` (int16, 4b): Eje de alineación del sprite respecto al punto de origen
  * `group`, `image` (uint16, 4b): Clave identificadora del sprite `(grp, img)`
  * `prev_copy` (uint16, 2b)
  * `same_pal` (uint8, 1b)
* **Datos PCX:**
  * Imagen codificada en 8-bit RLE PCX.
  * Los últimos 768 bytes contienen la paleta local VGA de 256 colores (si `data_length > 768`).

### 3.4. Archivo de Animaciones AIR (`.air`) — `AirParser`
Cada bloque comienza con `[Begin Action <ID>]`. Cada línea de frame sigue el formato estándar Elecbyte:
```text
grp, img, ox, oy, delay, [flip], [blend]
```
* `ox`, `oy`: Desplazamientos relativos en píxeles añadidos al eje del sprite (`total_ox = sff_x - air_ox`).
* `delay`: Duración de visualización en ticks (1 tick = 1/60 seg, `-1` = bucle infinito).
* `flip`: Indicadores de inversión (`H` para horizontal, `V` para vertical, `HV` para ambos).
* `blend`: Modo de transparencia (`A` = Aditivo, `S` = Sustractivo).

---

## 4. Especificación del Formato Binario `.fgt` (ArcadeMatrix Fighter Format)

El formato `.fgt` es un formato de animación compacto y secuencial diseñado para minimizar el consumo de CPU y memoria en microcontroladores:

### Estructura del archivo binario:

| Offset | Tamaño | Tipo | Descripción |
|---|---|---|---|
| `0x00` | 3 bytes | ASCII | Magic Bytes: `FGT` |
| `0x03` | 1 byte | uint8 | Versión del formato (`1`) |
| `0x04` | 2 bytes | uint16 LE | Ancho del Canvas (`canvas_w`) |
| `0x06` | 2 bytes | uint16 LE | Alto del Canvas (`canvas_h`) |
| `0x08` | 2 bytes | uint16 LE | Número de cuadros (`num_frames`) |
| `0x0A` | 2 bytes | uint16 LE | Color de transparencia RGB565 (`0x0000`) |
| `0x0C` | `2 * num_frames` | uint16 LE[] | Tabla de retrasos de cada cuadro (en ticks) |
| `0x0C + (2*N)` | `N * W * H * 2` | uint16 LE[] | Flujo continuo de píxeles RGB565 para cada cuadro |

> **Nota sobre compresión:** La opción `--compress` genera archivos `.fgt.gz` mediante gzip estándar, óptimos para almacenamiento en Raspberry Pi o tarjetas SD.

---

## 5. Algoritmo de Resolución de Paletas (`resolve_master_palette`)

Para garantizar una representación 100% fiel en rips arcade (Capcom, NeoGeo, Simpsons), digitalizaciones (Mortal Kombat, Midway) y creaciones originales de MUGEN:

1. **Selección del Sprite de Referencia del Cuerpo:**
   * Recorre los cuadros corporales clave (grupos `0`, `1`, `5`, `10`, `20`, `21`, `40`, `100`, `200`, `5000`).
   * Selecciona el cuadro con el mayor número de índices de píxeles distintos (para una evaluación óptima).
   * Excluye sistemáticamente el grupo `9000` (retratos / iconos de selección) para evitar contaminación.

2. **Recolección y Expansión Multi-Candidatos:**
   * **Candidatos `.def`:** Paletas declaradas en `[Files]` (`pal1..pal12`), con máxima prioridad a los slots de `pal.defaults`.
   * **Expansión Modulo Bank (16, 32, 64):** Para sprites indexados en bancos parciales (ej: Krusty the Clown, Capcom CPS2), generación de variantes `bank16`, `bank32`, `bank64`.
   * **Offset Shifting:** Para personajes digitalizados (Mortal Kombat) donde la paleta inicia en un slot alto (ej: 176), desplazamiento al slot 0 vía `shift_min`.
   * **Candidatos `SFFv1`:** Paleta integrada en el sprite de referencia, paleta de guardia `(0,0)`, primera paleta del SFF y subpaletas locales.
   * **Candidatos `.act`:** Archivos `.act` adicionales presentes en la carpeta del personaje.

3. **Función de Evaluación y Filtrado (`score_palette`):**
   * **Rechazo de paletas monocromáticas:** Si la paleta genera un solo tono (`u_colors <= 1`) cuando el sprite tiene múltiples índices, se rechaza (`score = -999.0`).
   * **Rechazo de máscaras binarias de debug:** Si $\le 3$ colores con al menos 2 esquinas binarias saturadas puras `(0/255, 0/255, 0/255)`, se rechaza.
   * **Cálculo de Puntuación:**
     $$\text{Puntuación Base} = \text{Colores Únicos} \times 10$$
     $$\text{Bono Luminancia Natural} = +100 \quad \text{si } 20 \le \text{Luminancia Media} \le 210$$
     $$\text{Penalización Sub/Sobre-exposición} = -30 \text{ (si } L < 15 \text{)}, \quad -80 \text{ (si } L > 225 \text{)}$$
   * **Bonos de Autor / Origen:**
     * `DEF(pal.defaults)`: **+150 pts** (variante bank/shift: **+140 pts**)
     * `DEF(pal1..12)`: **+100 pts** (variante bank/shift: **+90 pts**)
     * `SFF(sprite_cuerpo)`: **+40 pts**
     * `SFF(stand)`: **+35 pts**
     * `SFF(first)`: **+30 pts**
     * `SFF(local)`: **+20 pts**
     * `ACT(carpeta)`: **+10 pts**

---

## 6. Guía para Desarrolladores: Cómo Contribuir

### 6.1. Agregar soporte para formato SFFv2 (MUGEN 1.0 / 1.1)
El formato SFFv2 utiliza una estructura basada en bloques comprimidos LZO, RLE8 o PNG:
* Implementar la clase `SFFv2Parser`.
* Detectar la firma en el header: `ElecbyteSpr\x00` con versión `0x02, 0x00, 0x00, 0x02`.
* Descomprimir los sub-bloques hacia el diccionario en memoria `self.images[(grp, img)] = {'x': x, 'y': y, 'data': raw_rgba_or_indexed}`.

### 6.2. Soportar Alpha Blending (`A`, `S`, `ASxxxDxxx`)
Actualmente los píxeles con alpha < 128 se escriben como transparentes (`0x0000`).
* En el paso de renderizado (`Paso 2`), leer la propiedad `fr.get('blend')`.
* Para modo aditivo (`A`), convertir píxeles semitransparentes con máscara específica o mezclar con fondo oscuro.

### 6.3. Agregar Modo de Paleta Híbrida (FX / Proyectiles separados)
Si un proyectil o llama utiliza una paleta distinta a la del cuerpo:
* Calcular el `score_palette` de la paleta local del PCX para ese frame específico.
* Si el score local es alto (> 30.0) y corresponde a un grupo FX (1000+), utilizar la paleta local en lugar de la `master_palette`.

---

## 7. Comandos de Validación y Pruebas

Para probar modificaciones en personajes de referencia:

```bash
# Prueba interactiva guiada
./start_extractor.sh

# Prueba directa por línea de comandos
python3 mugen_extractor.py --src "/ruta/a/chars" --dest "./test_out" --mode SCALED --workers 4
```

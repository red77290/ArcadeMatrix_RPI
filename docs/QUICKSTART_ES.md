🇬🇧 [English](QUICKSTART.md) | 🇫🇷 [Français](QUICKSTART_FR.md) | 🇪🇸 Español

# Guía de inicio rápido

Esta guía te ayudará a instalar y configurar ArcadeMatrix en tu Raspberry Pi.

## 1. Instalación (recomendada)

Proporcionamos una imagen precompilada lista para usar.

1. Graba el archivo `ArcadeMatrix_Release.img` en tu tarjeta SD con **Raspberry Pi Imager**.
2. Cuando termine, vuelve a insertar la tarjeta SD en tu PC/Mac. Aparecerá una unidad USB de 8 GB llamada **DATA**.
3. Abre el archivo `config.json` ubicado en esta unidad **DATA** para introducir tus credenciales de Wi-Fi (`SSID` y `PASS`) y el tamaño de tu matriz.
4. Inserta la tarjeta SD en la Raspberry Pi y enciéndela. ¡La dirección IP se mostrará en la matriz!

## 2. Configuración Web

Una vez encendida la Pi, abre un navegador en tu teléfono o PC y ve a:
`http://<RASPBERRY_IP>:8080`

Aquí puedes configurar:
- Los colores, fuentes y temas del reloj y la fecha.
- Las funciones activadas en el bucle de rotación.
- Los ajustes de brillo y modo nocturno.

## 3. Añadir contenido (GIFs, sprites, fuentes)

Para añadir tus propios medios, **simplemente conecta tu tarjeta SD a tu PC/Mac**.
La unidad **DATA** aparecerá como una memoria USB estándar (formato exFAT):

- **GIFs**: déjalos en la carpeta `gifs/`.
- **Sprites MUGEN**: usa nuestro extractor para generar archivos `.fgt` y colócalos en `fighters_32/` o `fighters_64/`.
- **Fonts**: deja fuentes `.ttf` o `.bdf` en la carpeta `fonts/`.

*(La adición de medios se hace exclusivamente conectando la tarjeta SD a un ordenador o por SSH/SFTP. Si usas SSH, asegúrate primero de que la partición DATA esté correctamente montada. No existe ninguna función de carga vía Web.)*

## 4. Conexión de hardware

Recomendamos usar un Adafruit RGB Matrix HAT o Bonnet conectado a una Raspberry Pi Zero 2 W o Pi 4. Asegúrate de que el conector HUB75 esté correctamente enchufado a tu panel LED.

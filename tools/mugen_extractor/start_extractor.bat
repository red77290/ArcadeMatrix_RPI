@echo off
setlocal enabledelayedexpansion
echo =====================================================
echo    ArcadeMatrix RPi MUGEN / Sprite Character Extractor
echo =====================================================

cd /d "%~dp0"

IF NOT EXIST "venv" (
    echo [INFO] Creating Python Virtual Environment...
    python -m venv venv
    if errorlevel 1 (
        echo [ERROR] Python is not installed or not in PATH!
        pause
        exit /b
    )
)

echo [INFO] Activating virtual environment...
call venv\Scripts\activate.bat

echo [INFO] Installing requirements...
pip install -r requirements.txt -q

echo.
set "default_input=.\chars"
set /p input_folder="Dossier Source MUGEN [default: %default_input%]: "
if "%input_folder%"=="" set "input_folder=%default_input%"

echo.
set "default_output=.\fighters_64"
set /p output_folder="Dossier Destination [default: %default_output%]: "
if "%output_folder%"=="" set "output_folder=%default_output%"

echo.
echo -----------------------------------------------------
echo Choix du Facteur d'Echelle (Scaling Factor) :
echo    * 1.0   : Recommande Raspberry Pi / Matrice 64px+
echo    * 0.5   : Echelle 50%% pour matrice 32px
echo    * auto  : Ajustement automatique proportionnel
echo    * Ou entrez une valeur personnalisee (ex: 0.4, 0.75, 1.25, 2.0)
echo -----------------------------------------------------
set "scale_input="
set /p scale_input="Echelle souhaitee [default: 1.0]: "
if "%scale_input%"=="" set "scale_input=1.0"

echo.
echo -----------------------------------------------------
echo Compression des fichiers (.fgt vs .fgt.gz) :
echo    * y (Oui) : Recommande pour Raspberry Pi (-80%% espace)
echo    * n (Non) : Fichiers bruts non compresses
echo -----------------------------------------------------
set "compress_input="
set /p compress_input="Compresser en .fgt.gz ? (y/n) [default: y]: "
if "%compress_input%"=="" set "compress_input=y"

set "EXTRA_ARGS="
if "%scale_input%"=="auto" (
    set "EXTRA_ARGS=--mode SCALED"
) else (
    set "EXTRA_ARGS=--scale %scale_input%"
)

if /i "%compress_input%"=="y" (
    set "EXTRA_ARGS=%EXTRA_ARGS% --compress"
)

echo.
echo Lancement de l'extraction...
python mugen_extractor.py -i "%input_folder%" -o "%output_folder%" %EXTRA_ARGS%

echo.
echo [DONE]
pause

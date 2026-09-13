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
set /p input_folder="Source MUGEN Folder [default: %default_input%]: "
if "%input_folder%"=="" set "input_folder=%default_input%"

echo.
set "default_output=.\fighters_32"
set /p output_folder="Output Folder [default: %default_output%]: "
if "%output_folder%"=="" set "output_folder=%default_output%"

echo.
echo -----------------------------------------------------
echo Scaling Factor Selection:
echo    * 0.5   : Recommended for ESP32 128x32 / 64x32 Matrix
echo    * 1.0   : Original 1:1 scale (128x64, 256x64 Matrix, RPi)
echo    * auto  : Automatic proportional scaling to matrix height
echo    * Or enter custom multiplier (e.g. 0.4, 0.75, 1.5)
echo -----------------------------------------------------
set "scale_input="
set /p scale_input="Desired scale [default: 0.5]: "
if "%scale_input%"=="" set "scale_input=0.5"

echo.
echo -----------------------------------------------------
echo File Compression (.fgt vs .fgt.gz):
echo    * n (No)  : Recommended for ESP32 / SD Card / LittleFS
echo    * y (Yes) : Recommended for Raspberry Pi (-80%% space)
echo -----------------------------------------------------
set "compress_input="
set /p compress_input="Compress to .fgt.gz? (y/n) [default: n]: "
if "%compress_input%"=="" set "compress_input=n"

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
echo Starting extraction...
python mugen_extractor.py -i "%input_folder%" -o "%output_folder%" %EXTRA_ARGS%

echo.
echo [DONE]
pause

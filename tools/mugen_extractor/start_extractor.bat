@echo off
echo ===================================
echo ArcadeMatrix MUGEN / Sprite Extractor
echo ===================================

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
echo Please enter the path to the folder containing your MUGEN chars:
set /p input_folder="Input Folder: "

echo.
echo Please enter the path where you want the .fgt files saved:
set /p output_folder="Output Folder (e.g. ./fighters_32): "

echo.
echo Select your target platform:
echo 1) ESP32 - 128x32 Matrix (Scale 0.5, Uncompressed)
echo 2) ESP32 - 256x64 Matrix (No Scale, Uncompressed)
echo 3) Raspberry Pi (No Scale, Compressed)
set /p platform_choice="Choice (1/2/3): "

echo.
if "%platform_choice%"=="1" (
    echo [INFO] Running for ESP32 128x32...
    python mugen_extractor.py -i "%input_folder%" -o "%output_folder%" --scale 0.5
) else if "%platform_choice%"=="2" (
    echo [INFO] Running for ESP32 256x64...
    python mugen_extractor.py -i "%input_folder%" -o "%output_folder%"
) else if "%platform_choice%"=="3" (
    echo [INFO] Running for Raspberry Pi...
    python mugen_extractor.py -i "%input_folder%" -o "%output_folder%" --compress
) else (
    echo [ERROR] Invalid choice. Using default settings...
    python mugen_extractor.py -i "%input_folder%" -o "%output_folder%"
)

echo.
echo [DONE]
pause

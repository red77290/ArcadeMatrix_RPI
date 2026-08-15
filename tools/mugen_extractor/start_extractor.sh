#!/bin/bash
echo "==================================="
echo "ArcadeMatrix MUGEN / Sprite Extractor"
echo "==================================="

# Go to script directory
cd "$(dirname "$0")" || exit

if [ ! -d "venv" ]; then
    echo "[INFO] Creating Python Virtual Environment..."
    python3 -m venv venv
    if [ $? -ne 0 ]; then
        echo "[ERROR] python3 is not installed or not in PATH!"
        exit 1
    fi
fi

echo "[INFO] Activating virtual environment..."
source venv/bin/activate

echo "[INFO] Installing requirements..."
pip install -r requirements.txt -q

echo ""
read -p "Input Folder (containing MUGEN chars): " input_folder
read -p "Output Folder (e.g. ./fighters_32): " output_folder

echo ""
echo "Select your target platform:"
echo "1) ESP32 - 128x32 Matrix (Scale 0.5, Uncompressed)"
echo "2) ESP32 - 256x64 Matrix (No Scale, Uncompressed)"
echo "3) Raspberry Pi (No Scale, Compressed)"
read -p "Choice (1/2/3): " platform_choice

echo ""
if [ "$platform_choice" = "1" ]; then
    echo "[INFO] Running for ESP32 128x32..."
    python mugen_extractor.py -i "$input_folder" -o "$output_folder" --scale 0.5
elif [ "$platform_choice" = "2" ]; then
    echo "[INFO] Running for ESP32 256x64..."
    python mugen_extractor.py -i "$input_folder" -o "$output_folder"
elif [ "$platform_choice" = "3" ]; then
    echo "[INFO] Running for Raspberry Pi..."
    python mugen_extractor.py -i "$input_folder" -o "$output_folder" --compress
else
    echo "[ERROR] Invalid choice. Using default settings..."
    python mugen_extractor.py -i "$input_folder" -o "$output_folder"
fi

echo ""
echo "[DONE]"

#!/bin/bash
echo "====================================================="
echo "   ArcadeMatrix RPi MUGEN / Sprite Character Extractor"
echo "====================================================="

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
# 1. Input Folder
default_input="./chars"
read -e -p "📁 Source MUGEN Folder (containing character subfolders) [$default_input]: " input_folder
input_folder="${input_folder:-$default_input}"

# 2. Output Folder
default_output="./fighters_32"
read -e -p "📁 Output Folder (e.g. ./fighters_32) [$default_output]: " output_folder
output_folder="${output_folder:-$default_output}"

# 3. Scaling
echo ""
echo "-----------------------------------------------------"
echo "📏 Scaling Factor Selection:"
echo "   • 0.5   : Recommended for ESP32 128x32 / 64x32 Matrix (~32px height)"
echo "   • 1.0   : Original 1:1 scale (128x64, 256x64 Matrix, RPi, ESP32-S3 PSRAM)"
echo "   • auto  : Automatic proportional scaling to matrix height"
echo "   • Or enter a custom multiplier (e.g. 0.4, 0.6, 0.75, 1.25)"
echo "-----------------------------------------------------"
read -p "Desired scale [default: 0.5]: " scale_input
scale_input="${scale_input:-0.5}"

# 4. Compression
echo ""
echo "-----------------------------------------------------"
echo "🗜️  File Compression (.fgt vs .fgt.gz):"
echo "   • n (No)  : Recommended for ESP32 / SD Card / LittleFS (fast decoding)"
echo "   • y (Yes) : Recommended for Raspberry Pi / limited storage (-80% space)"
echo "-----------------------------------------------------"
read -p "Compress to .fgt.gz? (y/n) [default: n]: " compress_input
compress_input="${compress_input:-n}"

# Build arguments
EXTRA_ARGS=""
if [ "$scale_input" = "auto" ] || [ "$scale_input" = "scaled" ] || [ "$scale_input" = "SCALED" ]; then
    EXTRA_ARGS="--mode SCALED"
elif [ "$scale_input" = "1.0" ] || [ "$scale_input" = "full" ] || [ "$scale_input" = "FULLSIZE" ]; then
    EXTRA_ARGS="--scale 1.0"
else
    EXTRA_ARGS="--scale $scale_input"
fi

if [ "$compress_input" = "y" ] || [ "$compress_input" = "Y" ] || [ "$compress_input" = "yes" ]; then
    EXTRA_ARGS="$EXTRA_ARGS --compress"
fi

echo ""
echo "🚀 Starting extraction:"
echo "   • Source      : $input_folder"
echo "   • Destination : $output_folder"
echo "   • Parameters  : $EXTRA_ARGS"
echo ""

python mugen_extractor.py -i "$input_folder" -o "$output_folder" $EXTRA_ARGS

echo ""
echo "✅ [DONE] Extraction complete."

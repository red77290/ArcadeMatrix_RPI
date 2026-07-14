#!/bin/bash
# ArcadeMatrix Image Builder for macOS
# This script uses Docker to build a precompiled Raspberry Pi OS image

set -e

if ! command -v docker &> /dev/null; then
    echo "❌ Error: Docker is not installed or not running. Please launch Rancher Desktop or Docker Desktop."
    exit 1
fi

# Target total image size. Default to 14G to safely fit on a 16GB SD card.
IMAGE_SIZE=${1:-14G}

echo "=========================================================="
echo "      ArcadeMatrix RPi Image Builder (macOS -> Docker)"
echo "=========================================================="
echo "This will download Raspberry Pi OS, inject your code, compile it"
echo "to hide the Python source, and create a DATA partition."
echo ""
echo "🎯 Target Image Size: $IMAGE_SIZE"
echo ""
echo "⏳ This process will take 10 to 15 minutes. Please wait..."
echo "=========================================================="

# Run the Ubuntu container in privileged mode to allow loop devices and mounting
docker run --rm --privileged \
    -e IMAGE_SIZE="$IMAGE_SIZE" \
    -v "$(pwd)":/workspace \
    -w /workspace \
    ubuntu:22.04 \
    /bin/bash /workspace/scripts/docker_builder.sh

echo "=========================================================="
echo "✅ Build Complete!"
echo "You can now flash 'ArcadeMatrix_Release.img' using Raspberry Pi Imager."
echo "=========================================================="

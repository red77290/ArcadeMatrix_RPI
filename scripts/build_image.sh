#!/bin/bash
# ArcadeMatrix Image Builder for macOS / Linux / Windows
# This script uses Docker to build a precompiled Raspberry Pi OS image

set -e

if ! command -v docker &> /dev/null; then
    echo "❌ Error: Docker is not installed or not running. Please launch Rancher Desktop or Docker Desktop."
    exit 1
fi

# Target total image size. Default to 14G to safely fit on a 16GB SD card.
IMAGE_SIZE=${1:-14G}

echo "=========================================================="
echo "      ArcadeMatrix RPi Image Builder (Multi-Platform -> Docker)"
echo "=========================================================="
echo "This will download Raspberry Pi OS, cross-compile the Rust"
echo "binary, and inject it into a minimal DATA partition."
echo ""
echo "🎯 Target Image Size: $IMAGE_SIZE"
echo ""
echo "⏳ This process will take about 5 minutes. Please wait..."
echo "=========================================================="

echo "🦀 Step 1: Cross-compiling Rust binary for ARM64..."
bash scripts/build_local.sh

echo "📦 Step 2: Building Raspberry Pi OS Image..."

# Run the Ubuntu container in privileged mode to allow loop devices and mounting
docker run --rm --privileged \
    -e IMAGE_SIZE="$IMAGE_SIZE" \
    -v "$(pwd)":/workspace \
    -w /workspace \
    debian:bookworm \
    /bin/bash /workspace/scripts/docker_builder.sh

echo "=========================================================="
echo "✅ Build Complete!"
echo "You can now flash 'ArcadeMatrix_Release.img' using Raspberry Pi Imager."
echo "=========================================================="

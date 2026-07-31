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
echo "This will download Raspberry Pi OS, cross-compile the Rust"
echo "binary, and inject it into a minimal DATA partition."
echo ""
echo "🎯 Target Image Size: $IMAGE_SIZE"
echo ""
echo "⏳ This process will take about 5 minutes. Please wait..."
echo "=========================================================="

echo "🦀 Step 1: Cross-compiling Rust binary for ARM64..."
docker run --rm \
    -v "$(pwd)":/workspace \
    -w /workspace \
    rust:1.80-bookworm \
    bash -c "dpkg --add-architecture arm64 && apt-get update && apt-get install -y gcc-aarch64-linux-gnu g++-aarch64-linux-gnu libc6-dev-arm64-cross libssl-dev:arm64 && rustup target add aarch64-unknown-linux-gnu && export PKG_CONFIG_ALLOW_CROSS=1 && export PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig && CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc cargo build --release --target aarch64-unknown-linux-gnu"

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

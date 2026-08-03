#!/bin/bash
# ArcadeMatrix Smart Cross-Compiler
# Cross-compiles the Rust project for ARM64 (aarch64) or ARM32 (armv7) using Docker

set -e

TARGET_ARCH=${1:-"all"}

echo "=========================================================="
echo "      ArcadeMatrix RPi Smart Builder"
echo "=========================================================="

# Build docker command
DOCKER_CMD="
echo '=> Installing cross-compilation toolchains...'
dpkg --add-architecture arm64
dpkg --add-architecture armhf
apt-get update -qq
apt-get install -y -qq gcc-aarch64-linux-gnu g++-aarch64-linux-gnu libc6-dev-arm64-cross libssl-dev:arm64 gcc-arm-linux-gnueabihf g++-arm-linux-gnueabihf libc6-dev-armhf-cross libssl-dev:armhf
export PKG_CONFIG_ALLOW_CROSS=1
"

if [ "$TARGET_ARCH" = "aarch64" ] || [ "$TARGET_ARCH" = "all" ]; then
    DOCKER_CMD+="
echo '=> Adding Rust target for 64-bit (aarch64)...'
rustup target add aarch64-unknown-linux-gnu
echo '=> Compiling for 64-bit (aarch64)...'
export PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig
export CC=aarch64-linux-gnu-gcc
export CXX=aarch64-linux-gnu-g++
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc cargo build --release --target aarch64-unknown-linux-gnu
"
fi

if [[ "$TARGET_ARCH" == armv* ]] || [ "$TARGET_ARCH" = "all" ]; then
    DOCKER_CMD+="
echo '=> Adding Rust target for 32-bit (armv7)...'
rustup target add armv7-unknown-linux-gnueabihf
echo '=> Compiling for 32-bit (armv7)...'
export PKG_CONFIG_PATH=/usr/lib/arm-linux-gnueabihf/pkgconfig
export CC=arm-linux-gnueabihf-gcc
export CXX=arm-linux-gnueabihf-g++
CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc cargo build --release --target armv7-unknown-linux-gnueabihf
"
fi

docker run --rm \
    -v "$(pwd)":/workspace \
    -w /workspace \
    rust:bookworm \
    bash -c "$DOCKER_CMD"

echo ""
echo "✅ Compilation Complete!"
echo "=========================================================="

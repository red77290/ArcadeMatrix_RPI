#!/bin/bash
# ArcadeMatrix Fast Local Builder
# Cross-compiles the Rust project for both ARM64 (aarch64) and ARM32 (armv7) using Docker

set -e

echo "=========================================================="
echo "      ArcadeMatrix RPi Fast Local Builder (Multi-Arch)"
echo "=========================================================="
echo "This will compile the Rust binary for both 64-bit and 32-bit OS."
echo "Please wait, this uses Docker and may take a few minutes..."
echo ""

docker run --rm \
    -v "$(pwd)":/workspace \
    -w /workspace \
    rust:bookworm \
    bash -c "
    echo '=> Installing cross-compilation toolchains...'
    dpkg --add-architecture arm64
    dpkg --add-architecture armhf
    apt-get update -qq
    apt-get install -y -qq gcc-aarch64-linux-gnu g++-aarch64-linux-gnu libc6-dev-arm64-cross libssl-dev:arm64 gcc-arm-linux-gnueabihf g++-arm-linux-gnueabihf libc6-dev-armhf-cross libssl-dev:armhf
    
    echo '=> Adding Rust targets...'
    rustup target add aarch64-unknown-linux-gnu armv7-unknown-linux-gnueabihf
    
    export PKG_CONFIG_ALLOW_CROSS=1
    
    echo '=> Compiling for 64-bit (aarch64)...'
    export PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig
    export CC=aarch64-linux-gnu-gcc
    export CXX=aarch64-linux-gnu-g++
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc cargo build --release --target aarch64-unknown-linux-gnu
    
    echo '=> Compiling for 32-bit (armv7)...'
    export PKG_CONFIG_PATH=/usr/lib/arm-linux-gnueabihf/pkgconfig
    export CC=arm-linux-gnueabihf-gcc
    export CXX=arm-linux-gnueabihf-g++
    CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc cargo build --release --target armv7-unknown-linux-gnueabihf
    "

echo ""
echo "✅ Multi-Arch Compilation Complete!"
echo "- 64-bit binary: target/aarch64-unknown-linux-gnu/release/arcadematrix"
echo "- 32-bit binary: target/armv7-unknown-linux-gnueabihf/release/arcadematrix"
echo "=========================================================="

#!/bin/bash
# ArcadeMatrix Fast Local Builder
# Cross-compiles the Rust project for ARM64 (aarch64) only using Docker

set -e

echo "=========================================================="
echo "      ArcadeMatrix RPi Fast Local Builder (64-bit)"
echo "=========================================================="
echo "This will compile the Rust binary for 64-bit OS only."
echo "Please wait, this uses Docker and may take a minute..."
echo ""

docker run --rm \
    -v "$(pwd)":/workspace \
    -w /workspace \
    rust:bookworm \
    bash -c "
    echo '=> Installing cross-compilation toolchains...'
    dpkg --add-architecture arm64
    apt-get update -qq
    apt-get install -y -qq gcc-aarch64-linux-gnu g++-aarch64-linux-gnu libc6-dev-arm64-cross libssl-dev:arm64
    
    echo '=> Adding Rust targets...'
    rustup target add aarch64-unknown-linux-gnu
    
    export PKG_CONFIG_ALLOW_CROSS=1
    
    echo '=> Compiling for 64-bit (aarch64)...'
    export PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig
    export CC=aarch64-linux-gnu-gcc
    export CXX=aarch64-linux-gnu-g++
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc cargo build --release --target aarch64-unknown-linux-gnu
    "

echo ""
echo "✅ 64-bit Compilation Complete!"
echo "- Binary: target/aarch64-unknown-linux-gnu/release/arcadematrix"
echo "=========================================================="

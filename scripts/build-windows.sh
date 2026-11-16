#!/usr/bin/env bash
# 在 macOS 上交叉编译 Windows 版（x86_64-pc-windows-gnu）。
# 前置：brew install mingw-w64；rustup target add x86_64-pc-windows-gnu
set -euo pipefail
cd "$(dirname "$0")/.."

export PATH="/opt/homebrew/bin:$PATH"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc

cargo build --profile windows --target x86_64-pc-windows-gnu -p gitsc-app

mkdir -p dist
cp target/x86_64-pc-windows-gnu/windows/gitsc.exe dist/gitsc-windows-x86_64.exe
ls -lh dist/gitsc-windows-x86_64.exe
